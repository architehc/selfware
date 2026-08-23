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

pub use client::{ApiClient, RetryConfig};
pub use streaming::{StreamChunk, StreamingResponse};
pub use tool_calling::{
    attach_tools, extract_tool_calls, extract_tool_calls_from_text, parsed_to_tool_call,
};
pub use types::*;

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

    let has_user = messages.iter().any(|m| m.role == "user");
    if !has_user {
        let insert_pos = if messages.first().map(|m| m.role.as_str()) == Some("system") {
            1
        } else {
            0
        };
        messages.insert(insert_pos, Message::user("Continue with the task."));
    }
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
/// injection of fields like `logprobs`, `logit_bias`, `n`, `user`, or
/// `response_format` that could leak data or alter behavior unexpectedly.
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
) -> Result<()> {
    let Some(extra_body) = extra_body else {
        return Ok(());
    };

    let body_obj = body
        .as_object_mut()
        .context("request body must be a JSON object")?;

    for key in extra_body.keys() {
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
