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
pub mod types;

pub use client::{ApiClient, RetryConfig};
pub use streaming::{StreamChunk, StreamingResponse};
pub use types::*;

const DISABLED_THINKING_SYSTEM_MESSAGE: &str =
    "CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.";

/// Enforce OpenAI-style message ordering: all system messages must precede
/// non-system messages. SGLang and other strict backends reject requests
/// where system messages appear after user/assistant/tool messages.
fn canonicalize_message_order(messages: &mut Vec<Message>) {
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
fn attach_tools_to_body(
    body: &mut serde_json::Value,
    tools: &Option<Vec<ToolDefinition>>,
    native_tool_choice: bool,
) {
    if let Some(tools) = tools {
        body["tools"] = serde_json::json!(tools);
        if native_tool_choice {
            body["tool_choice"] = serde_json::json!("auto");
        }
    }
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
];

fn merge_extra_body(
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

/// Mock LLM client for unit testing.
///
/// Provides a queue-based mock that returns pre-configured `ChatResponse`
/// values from `chat()` calls. Streaming is not supported and will return
/// an error.
#[cfg(test)]
pub mod mock {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    pub struct MockLlmClient {
        responses: Mutex<VecDeque<ChatResponse>>,
    }

    impl MockLlmClient {
        /// Create a new mock client with an empty response queue.
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(VecDeque::new()),
            }
        }

        /// Create a mock client pre-loaded with the given responses.
        ///
        /// Responses are returned in FIFO order by `chat()`.
        pub fn with_responses(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: Mutex::new(VecDeque::from(responses)),
            }
        }
    }

    impl Default for MockLlmClient {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        async fn chat(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolDefinition>>,
            _thinking: ThinkingMode,
        ) -> anyhow::Result<ChatResponse> {
            let mut queue = self
                .responses
                .lock()
                .map_err(|_| anyhow::anyhow!("Mock responses mutex poisoned"))?;
            queue
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("No more mock responses"))
        }

        async fn chat_stream(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolDefinition>>,
            _thinking: ThinkingMode,
        ) -> anyhow::Result<StreamingResponse> {
            Err(anyhow::anyhow!("Streaming not supported in mock client"))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn test_mock_returns_responses_in_order() {
            let r1 = ChatResponse {
                id: "1".into(),
                object: "chat.completion".into(),
                created: 0,
                model: "mock".into(),
                choices: vec![],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                },
            };
            let r2 = ChatResponse {
                id: "2".into(),
                ..r1.clone()
            };
            let client = MockLlmClient::with_responses(vec![r1, r2]);
            let first = client
                .chat(vec![], None, ThinkingMode::Enabled)
                .await
                .unwrap();
            assert_eq!(first.id, "1");
            let second = client
                .chat(vec![], None, ThinkingMode::Enabled)
                .await
                .unwrap();
            assert_eq!(second.id, "2");
        }

        #[tokio::test]
        async fn test_mock_errors_when_empty() {
            let client = MockLlmClient::new();
            let result = client.chat(vec![], None, ThinkingMode::Enabled).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_mock_stream_not_supported() {
            let client = MockLlmClient::new();
            let result = client
                .chat_stream(vec![], None, ThinkingMode::Enabled)
                .await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Streaming not supported"));
        }
    }
}
