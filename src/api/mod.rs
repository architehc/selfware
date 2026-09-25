//! LLM API client layer.
//!
//! Submodules:
//! - [`types`]: Request/response types (Message, ChatResponse, ToolCall, etc.)
//! - [`streaming`]: SSE streaming infrastructure
//! - [`client`]: HTTP client with retry logic and circuit breaker

use anyhow::{bail, Context, Result};
use async_trait::async_trait;

pub mod client;
pub mod streaming;
pub mod tool_calling;
pub mod types;
pub mod usage;

pub use client::{ApiClient, RetryConfig};
pub use streaming::{StreamChunk, StreamingResponse};
pub use tool_calling::{
    attach_tools, extract_tool_calls, extract_tool_calls_from_text, parsed_to_tool_call,
};
pub use types::*;
pub use usage::UsageCoverage;

const DISABLED_THINKING_SYSTEM_MESSAGE: &str =
    "CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.";

/// Enforce OpenAI-style message ordering: all system messages must precede
/// non-system messages. SGLang and other strict backends reject requests
/// where system messages appear after user/assistant/tool messages.
/// Some providers (Moonshot/Kimi, others) reject an `assistant` message whose
/// content is empty. A pure "thinking" turn — or a step that produced only
/// reasoning and no tool call — leaves exactly that, so every subsequent
/// request 400s and the agent loops on the error. Recognize the state and adapt:
/// give such a message a minimal non-empty body (its own reasoning if present,
/// otherwise a placeholder). Messages that carry tool calls are left alone —
/// their empty content is valid.
fn sanitize_assistant_content(messages: &mut [Message]) {
    for message in messages.iter_mut() {
        if message.role != "assistant" {
            continue;
        }
        let has_tool_calls = message
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty());
        if has_tool_calls || !message.content.text().trim().is_empty() {
            continue;
        }
        let filler = message
            .reasoning_content
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("(no textual output)")
            .to_string();
        message.content = MessageContent::Text(filler);
    }
}

fn canonicalize_message_order(messages: &mut Vec<Message>) {
    // Every send path canonicalizes order here, so also guarantee no empty
    // assistant message reaches providers that reject them (Moonshot/Kimi).
    sanitize_assistant_content(messages);

    let sys_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "system")
        .map(|(i, _)| i)
        .collect();

    if sys_indices.len() > 1 {
        let merged_content: String = sys_indices
            .iter()
            .map(|&i| messages[i].content.to_string())
            .collect::<Vec<_>>()
            .join("\n\n");

        for &i in sys_indices.iter().rev() {
            messages.remove(i);
        }

        messages.insert(0, Message::system(merged_content));
    }

    // Mid-conversation system pushes (hoisted above) and interrupted turns
    // leave same-role neighbours behind, e.g. user(tool result) + user(hint)
    // or user(interrupted prompt) + user(new prompt). Strict local chat
    // templates (Mistral, Gemma, Llama-2: "Conversation roles must
    // alternate") reject that, so fold plain-text same-role neighbours.
    coalesce_adjacent_plain_turns(messages);

    let has_user = messages.iter().any(|m| m.role == "user");
    if !has_user {
        let insert_pos = if messages.first().map(|m| m.role.as_str()) == Some("system") {
            1
        } else {
            0
        };
        messages.insert(insert_pos, Message::user("Continue with the task."));
    }

    // Anthropic rejects assistant prefill: the conversation must not END with
    // an assistant message (measured live 2026-09-01: claude-fable-5/opus-5 via
    // OpenRouter 400'd on every request once a recovery path left the history
    // trailing on assistant). Ending on a user message is accepted by every
    // provider, so close the turn with a minimal continuation.
    //
    // Exception: an assistant that still carries `tool_calls` is an OPEN pair —
    // its role=tool results arrive on the next dispatch step. Wedging a user
    // message between the call and its future results would produce the exact
    // "messages with role 'tool' must immediately follow an assistant message
    // with 'tool_calls'" rejection OpenAI-compatible endpoints raise (HTTP
    // 400); skip the continuation so the results can land right behind the
    // call. (A dangling call is dropped upstream by the caller's pair
    // invariants before this point; the provider still rejects a genuinely
    // orphaned tail pair either way.)
    let tail_is_open_pair = messages
        .last()
        .map(|m| m.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()))
        .unwrap_or(false);
    if !tail_is_open_pair && messages.last().map(|m| m.role.as_str()) == Some("assistant") {
        messages.push(Message::user("Continue with the task."));
    }
}

/// Whether a message is a plain text turn that may be folded into an adjacent
/// message of the same role at send time.
///
/// Only `user` and `assistant` messages with `MessageContent::Text`, no
/// `tool_calls`, no `tool_call_id` and no `name` qualify. XML tool-result user
/// messages (`<tool_result>`) are excluded: they are protocol payloads the
/// agent pairs with prior calls, and consecutive tool-result/user messages are
/// legitimate in OpenAI-compatible APIs. Multimodal (`Blocks`) content is
/// never merged so images keep their own message. Mirrors the conservative
/// predicate of `Agent::is_mergeable_user_turn` in
/// `agent/context_management.rs`.
fn is_plain_mergeable_turn(message: &Message) -> bool {
    if message.role != "user" && message.role != "assistant" {
        return false;
    }
    if !matches!(message.content, MessageContent::Text(_)) {
        return false;
    }
    if message.tool_calls.as_ref().is_some_and(|c| !c.is_empty())
        || message.tool_call_id.is_some()
        || message.name.is_some()
    {
        return false;
    }
    !(message.role == "user" && message.content.text().contains("<tool_result>"))
}

/// Merge adjacent plain-text user/user and assistant/assistant messages
/// (joined with a blank line, order preserved). Anything that fails
/// [`is_plain_mergeable_turn`] is left in place untouched; this pass never
/// reorders messages.
fn coalesce_adjacent_plain_turns(messages: &mut Vec<Message>) {
    if messages.len() < 2 {
        return;
    }
    let mut out: Vec<Message> = Vec::with_capacity(messages.len());
    for message in messages.drain(..) {
        if let Some(prev) = out.last_mut() {
            if prev.role == message.role
                && is_plain_mergeable_turn(prev)
                && is_plain_mergeable_turn(&message)
            {
                let merged = format!("{}\n\n{}", prev.content.text(), message.content.text());
                prev.content = MessageContent::Text(merged);
                prev.reasoning_content =
                    match (prev.reasoning_content.take(), message.reasoning_content) {
                        (Some(a), Some(b)) => Some(format!("{a}\n\n{b}")),
                        (a, b) => a.or(b),
                    };
                continue;
            }
        }
        out.push(message);
    }
    *messages = out;
}

fn maybe_prepend_disabled_thinking_instruction(
    messages: &mut Vec<Message>,
    thinking: &ThinkingMode,
) {
    if matches!(thinking, ThinkingMode::Disabled) {
        messages.insert(0, Message::system(DISABLED_THINKING_SYSTEM_MESSAGE));
    }
}

/// Attach tool definitions to a chat-completion request body.
///
/// Thin wrapper over [`tool_calling::attach_tools`] — kept for backward
/// compatibility with internal callers in this module. New code should
/// prefer the public re-export at the crate root.
fn attach_tools_to_body(
    body: &mut serde_json::Value,
    tools: &Option<Vec<ToolDefinition>>,
    native_tool_choice: bool,
) {
    tool_calling::attach_tools(body, tools, native_tool_choice);
}

/// Keys that the request builder owns — extra_body must not override these.
const RESERVED_EXTRA_BODY_KEYS: &[&str] = &[
    "model",
    "messages",
    "tools",
    "tool_choice",
    "stream",
    "max_tokens",
    "temperature",
    "thinking",
];

/// Allowlisted extra_body keys — safe sampling/backend parameters.
///
/// Any key not in this list AND not reserved is rejected. This prevents
/// injection of fields like `logit_bias`, `n`, or `user` that could alter
/// behavior unexpectedly.
const ALLOWED_EXTRA_BODY_KEYS: &[&str] = &[
    // Sampling parameters
    "top_p",
    "top_k",
    "min_p",
    "repetition_penalty",
    "frequency_penalty",
    "presence_penalty",
    "seed",
    "stop",
    // Backend-specific extensions (vLLM, SGLang)
    "chat_template_kwargs",
    "guided_json",
    "guided_regex",
    "guided_choice",
    "skip_special_tokens",
    "spaces_between_special_tokens",
    "add_generation_prompt",
    // Structured output and introspective capabilities
    "response_format",
    "logprobs",
    "top_logprobs",
    // Best-of / beam search (resource control, not data leakage)
    "best_of",
    "use_beam_search",
    "length_penalty",
    "early_stopping",
    // OpenRouter routing directives (control which upstream provider serves the
    // request; e.g. pin providers that offer the full context window and honor
    // tool calls). These steer routing, not output content, so they are safe.
    "provider",
    "models",
    "route",
    "transforms",
    // Reasoning-effort control (OpenRouter `reasoning_effort` / `reasoning`):
    // steers how much the model thinks, not output content — same class as
    // best_of/provider above. Needed so hosted reasoning models (GLM 5.3)
    // don't burn the whole completion budget on hidden reasoning before the
    // answer (measured 2026-08-23: unbounded reasoning exhausted 16k tokens
    // with finish_reason=length and zero answer content).
    "reasoning_effort",
    "reasoning",
];

pub(crate) fn merge_extra_body(
    body: &mut serde_json::Value,
    extra_body: Option<&serde_json::Map<String, serde_json::Value>>,
    context: &str,
    endpoint: Option<&str>,
) -> Result<()> {
    let Some(extra_body) = extra_body else {
        return Ok(());
    };

    let body_obj = body
        .as_object_mut()
        .context("request body must be a JSON object")?;

    for (key, value) in extra_body {
        let k = key.as_str();
        if RESERVED_EXTRA_BODY_KEYS.contains(&k) {
            bail!("{} extra_body cannot override reserved key: {}", context, k);
        }
        if !ALLOWED_EXTRA_BODY_KEYS.contains(&k) {
            bail!(
                "{} extra_body contains disallowed key '{}'. \
                 Only sampling and backend-specific parameters are permitted. \
                 Allowed keys: {}",
                context,
                k,
                ALLOWED_EXTRA_BODY_KEYS.join(", ")
            );
        }
        if k == "reasoning_effort" {
            let Some(val) = value.as_str() else {
                bail!(
                    "{} extra_body.reasoning_effort must be a string, got: {}",
                    context,
                    value
                );
            };
            let model_name = body_obj.get("model").and_then(|v| v.as_str()).unwrap_or("");
            let is_qwen = model_name.to_ascii_lowercase().contains("qwen");

            // 1. High rejected for Qwen everywhere: Qwen chat template refuses "high" on all serving stacks
            if is_qwen && val.eq_ignore_ascii_case("high") {
                bail!(
                    "{} extra_body cannot set reasoning_effort to 'high' for Qwen models. \
                     The Qwen chat template refuses 'high' on all serving stacks (accepted values: 'low', 'medium', or default/xhigh in chat_template_kwargs).",
                    context
                );
            }

            // 2. xhigh rejected on SGLang or unverified endpoints
            if val.eq_ignore_ascii_case("xhigh") {
                match endpoint.and_then(crate::config::check_sglang_backend) {
                    Some(true) => {
                        bail!(
                            "{} extra_body cannot set reasoning_effort to 'xhigh' at top-level on SGLang. \
                             Top-level reasoning_effort only accepts 'low' or 'medium' ('xhigh' is rejected by SGLang schema). \
                             For xhigh reasoning, place it in chat_template_kwargs.reasoning_effort \
                             or omit the field (default is xhigh).",
                            context
                        );
                    }
                    None => {
                        bail!(
                            "{} extra_body cannot set reasoning_effort to 'xhigh' at top-level on unverified endpoint. \
                             Top-level reasoning_effort is rejected by SGLang schema. \
                             For xhigh reasoning, place it in chat_template_kwargs.reasoning_effort \
                             or omit the field (default is xhigh).",
                            context
                        );
                    }
                    Some(false) => {}
                }
            }
        }
    }

    for (key, value) in extra_body {
        body_obj.insert(key.clone(), value.clone());
    }

    Ok(())
}

/// Trait abstraction over the LLM API client, enabling test mocking.
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// Send a chat completion request (non-streaming).
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse>;

    /// Send a streaming chat completion request.
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThinkingMode {
    /// Full thinking enabled (default)
    Enabled,
    /// Thinking disabled for faster responses
    Disabled,
    /// Thinking with a specific token budget
    Budget(usize),
    /// Recovery retry of a main turn whose whole completion budget went to
    /// hidden reasoning: the request is built as for `Enabled`, then every
    /// reasoning-effort pin is stepped down one level, or thinking is
    /// switched off where no effort can be lowered (see
    /// [`client::apply_reasoning_step_down`]). `max_tokens` is untouched.
    StepDown,
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/api/message_compat_test.rs"]
mod message_compat_test;

/// Mock LLM client for unit testing.
///
/// Provides a queue-based mock that returns pre-configured `ChatResponse`
/// values from `chat()` calls. Streaming is not supported and will return
/// an error.
#[cfg(test)]
#[path = "mock.rs"]
pub mod mock;
