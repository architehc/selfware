use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::{Client, StatusCode};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub mod types;

use types::*;

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

/// A streaming response that yields chunks as they arrive
// Streaming infrastructure (used by chat_streaming)
pub struct StreamingResponse {
    response: reqwest::Response,
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.response.status())
            .finish()
    }
}

impl StreamingResponse {
    fn new(response: reqwest::Response) -> Self {
        Self { response }
    }

    /// Process the stream and send chunks through a channel
    #[allow(dead_code)] // Streaming API - used by chat_streaming
    pub async fn into_channel(self) -> mpsc::Receiver<Result<StreamChunk>> {
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut stream = self.response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            if let Some(chunk) = parse_sse_event(&event) {
                                if tx.send(Ok(chunk)).await.is_err() {
                                    return; // Receiver dropped
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(anyhow::anyhow!("Stream error: {}", e))).await;
                        return;
                    }
                }
            }
        });

        rx
    }

    /// Collect all chunks into a complete response
    #[allow(dead_code)] // Streaming API - collects stream into response
    pub async fn collect(self) -> Result<ChatResponse> {
        let mut rx = self.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        };

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            match chunk {
                StreamChunk::Content(text) => content.push_str(&text),
                StreamChunk::Reasoning(text) => reasoning.push_str(&text),
                StreamChunk::ToolCall(call) => tool_calls.push(call),
                StreamChunk::Usage(u) => usage = u,
                StreamChunk::Done => break,
            }
        }

        Ok(ChatResponse {
            id: "streamed".to_string(),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model: "unknown".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content,
                    reasoning_content: if reasoning.is_empty() {
                        None
                    } else {
                        Some(reasoning)
                    },
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                    name: None,
                },
                reasoning_content: None,
                finish_reason: Some("stop".to_string()),
            }],
            usage,
        })
    }
}

/// A chunk from a streaming response
#[derive(Debug, Clone)]
// Streaming infrastructure (used by chat_streaming)
pub enum StreamChunk {
    /// Text content
    Content(String),
    /// Reasoning/thinking content
    Reasoning(String),
    /// A tool call
    ToolCall(ToolCall),
    /// Token usage information
    Usage(Usage),
    /// Stream is complete
    Done,
}

/// Parse a Server-Sent Events (SSE) event
// Streaming infrastructure (used by chat_streaming)
fn parse_sse_event(event: &str) -> Option<StreamChunk> {
    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                return Some(StreamChunk::Done);
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                // Extract content from choices[0].delta.content
                if let Some(content) = json
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    if !content.is_empty() {
                        return Some(StreamChunk::Content(content.to_string()));
                    }
                }

                // Extract reasoning content
                if let Some(reasoning) = json
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"))
                    .and_then(|d| d.get("reasoning_content"))
                    .and_then(|c| c.as_str())
                {
                    if !reasoning.is_empty() {
                        return Some(StreamChunk::Reasoning(reasoning.to_string()));
                    }
                }

                // Extract usage if present
                if let Some(usage) = json.get("usage") {
                    if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                        return Some(StreamChunk::Usage(u));
                    }
                }
            }
        }
    }
    None
}

/// Retry configuration for API calls
#[derive(Clone, Debug)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial delay between retries (doubles each attempt)
    pub initial_delay_ms: u64,
    /// Maximum delay between retries
    pub max_delay_ms: u64,
    /// HTTP status codes that should trigger a retry
    pub retryable_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            retryable_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

impl RetryConfig {
    pub fn from_settings(settings: &crate::config::RetrySettings) -> Self {
        Self {
            max_retries: settings.max_retries,
            initial_delay_ms: settings.base_delay_ms,
            max_delay_ms: settings.max_delay_ms,
            retryable_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

pub struct ApiClient {
    client: Client,
    config: crate::config::Config,
    base_url: String,
    retry_config: RetryConfig,
}

impl ApiClient {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        // Use step_timeout from config with reasonable 60s minimum
        // Users can configure longer timeouts for slow models
        let request_timeout = config.agent.step_timeout_secs.max(60);
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: config.endpoint.clone(),
            config: config.clone(),
            retry_config: RetryConfig::from_settings(&config.retry),
        })
    }

    /// Create client with custom retry configuration
    #[allow(dead_code)] // Builder method for API configuration
    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": false,
        });

        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

        match thinking {
            ThinkingMode::Enabled => {
                // Default behavior, no special config needed
            }
            ThinkingMode::Disabled => {
                body["thinking"] = serde_json::json!({"type": "disabled"});
            }
            ThinkingMode::Budget(tokens) => {
                body["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": tokens
                });
            }
        }

        self.send_with_retry(&body).await
    }

    /// Stream a chat completion response
    /// Returns a receiver that yields chunks as they arrive
    #[allow(dead_code)] // Streaming API endpoint
    pub async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse> {
        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": self.config.max_tokens,
            "stream": true,
        });

        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

        match thinking {
            ThinkingMode::Enabled => {}
            ThinkingMode::Disabled => {
                body["thinking"] = serde_json::json!({"type": "disabled"});
            }
            ThinkingMode::Budget(tokens) => {
                body["thinking"] = serde_json::json!({
                    "type": "enabled",
                    "budget_tokens": tokens
                });
            }
        }

        let url = format!("{}/chat/completions", self.base_url);
        debug!("Starting streaming request to {}", url);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .context("Failed to send streaming request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        Ok(StreamingResponse::new(response))
    }

    /// Send request with exponential backoff retry logic
    async fn send_with_retry(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        let mut last_error: Option<anyhow::Error> = None;
        let mut delay_ms = self.retry_config.initial_delay_ms;

        for attempt in 0..=self.retry_config.max_retries {
            if attempt > 0 {
                warn!(
                    "Retry attempt {}/{} after {}ms delay",
                    attempt, self.retry_config.max_retries, delay_ms
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                // Exponential backoff with jitter
                delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                // Add jitter (±10%)
                let jitter = (delay_ms as f64 * 0.1 * (rand_jitter() - 0.5)) as u64;
                delay_ms = delay_ms.saturating_add_signed(jitter as i64);
            }

            debug!("Sending request to {} (attempt {})", url, attempt + 1);

            let result = self
                .client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(body)
                .send()
                .await;

            match result {
                Ok(response) => {
                    let status = response.status();

                    if status.is_success() {
                        // Debug: log raw response body if SELFWARE_DEBUG is set
                        let body_text = response
                            .text()
                            .await
                            .context("Failed to read response body")?;

                        debug!("API response body ({} chars)", body_text.len());
                        if std::env::var("SELFWARE_DEBUG").is_ok()
                            && std::env::var("SELFWARE_DEBUG_RAW").is_ok()
                        {
                            eprintln!("=== RAW API RESPONSE ===\n{}\n=== END RAW ===", body_text);
                        }

                        let chat_response: ChatResponse = serde_json::from_str(&body_text)
                            .context("Failed to parse response JSON")?;
                        return Ok(chat_response);
                    }

                    // Check if this status code is retryable
                    if self
                        .retry_config
                        .retryable_status_codes
                        .contains(&status.as_u16())
                    {
                        let error_text = response.text().await.unwrap_or_default();
                        warn!("Retryable error ({}): {}", status, error_text);
                        last_error = Some(anyhow::anyhow!("API error {}: {}", status, error_text));

                        // Check for Retry-After header
                        if status == StatusCode::TOO_MANY_REQUESTS {
                            // Could parse Retry-After header here if needed
                        }
                        continue;
                    }

                    // Non-retryable error
                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("API error {}: {}", status, error_text);
                }
                Err(e) => {
                    // Network errors are generally retryable
                    if e.is_timeout() || e.is_connect() {
                        warn!("Network error (retrying): {}", e);
                        last_error = Some(e.into());
                        continue;
                    }
                    // Other errors (e.g., invalid URL) are not retryable
                    return Err(e).context("Failed to send request");
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Request failed after retries")))
    }
}

#[async_trait]
impl LlmClient for ApiClient {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse> {
        self.chat(messages, tools, thinking).await
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse> {
        self.chat_stream(messages, tools, thinking).await
    }
}

/// Generate a random jitter value between 0 and 1
fn rand_jitter() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1000) as f64 / 1000.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThinkingMode {
    /// Full thinking enabled (default)
    Enabled,
    /// Thinking disabled for faster responses
    Disabled,
    /// Thinking with a specific token budget
    #[allow(dead_code)] // For models supporting thinking budget
    Budget(usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thinking_mode_enabled() {
        let mode = ThinkingMode::Enabled;
        assert_eq!(mode, ThinkingMode::Enabled);
    }

    #[test]
    fn test_thinking_mode_disabled() {
        let mode = ThinkingMode::Disabled;
        assert_eq!(mode, ThinkingMode::Disabled);
    }

    #[test]
    fn test_thinking_mode_budget() {
        let mode = ThinkingMode::Budget(1024);
        assert_eq!(mode, ThinkingMode::Budget(1024));
        if let ThinkingMode::Budget(tokens) = mode {
            assert_eq!(tokens, 1024);
        }
    }

    #[test]
    fn test_thinking_mode_debug() {
        let mode = ThinkingMode::Budget(500);
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("Budget"));
        assert!(debug_str.contains("500"));
    }

    #[test]
    fn test_thinking_mode_clone() {
        let mode = ThinkingMode::Budget(2048);
        let cloned = mode;
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_stream_chunk_content() {
        let chunk = StreamChunk::Content("Hello".to_string());
        if let StreamChunk::Content(text) = chunk {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected Content variant");
        }
    }

    #[test]
    fn test_stream_chunk_reasoning() {
        let chunk = StreamChunk::Reasoning("Thinking...".to_string());
        if let StreamChunk::Reasoning(text) = chunk {
            assert_eq!(text, "Thinking...");
        } else {
            panic!("Expected Reasoning variant");
        }
    }

    #[test]
    fn test_stream_chunk_done() {
        let chunk = StreamChunk::Done;
        assert!(matches!(chunk, StreamChunk::Done));
    }

    #[test]
    fn test_stream_chunk_usage() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        let chunk = StreamChunk::Usage(usage.clone());
        if let StreamChunk::Usage(u) = chunk {
            assert_eq!(u.total_tokens, 150);
        }
    }

    #[test]
    fn test_stream_chunk_debug() {
        let chunk = StreamChunk::Content("test".to_string());
        let debug = format!("{:?}", chunk);
        assert!(debug.contains("Content"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_stream_chunk_clone() {
        let chunk = StreamChunk::Content("original".to_string());
        let cloned = chunk.clone();
        if let StreamChunk::Content(text) = cloned {
            assert_eq!(text, "original");
        }
    }

    #[test]
    fn test_parse_sse_event_done() {
        let event = "data: [DONE]";
        let result = parse_sse_event(event);
        assert!(matches!(result, Some(StreamChunk::Done)));
    }

    #[test]
    fn test_parse_sse_event_content() {
        let event = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        let result = parse_sse_event(event);
        assert!(matches!(result, Some(StreamChunk::Content(_))));
        if let Some(StreamChunk::Content(text)) = result {
            assert_eq!(text, "Hello");
        }
    }

    #[test]
    fn test_parse_sse_event_reasoning() {
        let event = r#"data: {"choices":[{"delta":{"reasoning_content":"Thinking about it"}}]}"#;
        let result = parse_sse_event(event);
        assert!(matches!(result, Some(StreamChunk::Reasoning(_))));
    }

    #[test]
    fn test_parse_sse_event_usage() {
        let event =
            r#"data: {"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let result = parse_sse_event(event);
        assert!(matches!(result, Some(StreamChunk::Usage(_))));
    }

    #[test]
    fn test_parse_sse_event_empty_content() {
        let event = r#"data: {"choices":[{"delta":{"content":""}}]}"#;
        let result = parse_sse_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_event_no_data_prefix() {
        let event = "not a data line";
        let result = parse_sse_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_event_invalid_json() {
        let event = "data: {invalid json}";
        let result = parse_sse_event(event);
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_event_multiline() {
        let event = "event: message\ndata: [DONE]";
        let result = parse_sse_event(event);
        assert!(matches!(result, Some(StreamChunk::Done)));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 30000);
        assert!(config.retryable_status_codes.contains(&429));
        assert!(config.retryable_status_codes.contains(&500));
        assert!(config.retryable_status_codes.contains(&503));
    }

    #[test]
    fn test_retry_config_from_settings() {
        let settings = crate::config::RetrySettings {
            max_retries: 9,
            base_delay_ms: 250,
            max_delay_ms: 12000,
        };
        let config = RetryConfig::from_settings(&settings);

        assert_eq!(config.max_retries, 9);
        assert_eq!(config.initial_delay_ms, 250);
        assert_eq!(config.max_delay_ms, 12000);
        assert!(config.retryable_status_codes.contains(&429));
        assert!(config.retryable_status_codes.contains(&500));
    }

    #[test]
    fn test_retry_config_clone() {
        let config = RetryConfig::default();
        let cloned = config.clone();
        assert_eq!(cloned.max_retries, config.max_retries);
    }

    #[test]
    fn test_retry_config_debug() {
        let config = RetryConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("RetryConfig"));
        assert!(debug.contains("max_retries"));
    }

    #[test]
    fn test_rand_jitter_range() {
        // Call multiple times and verify it returns values in [0, 1)
        for _ in 0..10 {
            let jitter = rand_jitter();
            assert!(jitter >= 0.0);
            assert!(jitter < 1.0);
        }
    }

    // ============================================
    // Retry Logic Tests
    // ============================================

    #[test]
    fn test_retry_config_custom_values() {
        let config = RetryConfig {
            max_retries: 5,
            initial_delay_ms: 500,
            max_delay_ms: 60000,
            retryable_status_codes: vec![429, 503],
        };
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.initial_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 60000);
        assert_eq!(config.retryable_status_codes.len(), 2);
        assert!(config.retryable_status_codes.contains(&429));
        assert!(config.retryable_status_codes.contains(&503));
    }

    #[test]
    fn test_retry_config_status_code_check() {
        let config = RetryConfig::default();
        // Check that all expected retryable codes are present
        assert!(config.retryable_status_codes.contains(&429)); // Too Many Requests
        assert!(config.retryable_status_codes.contains(&500)); // Internal Server Error
        assert!(config.retryable_status_codes.contains(&502)); // Bad Gateway
        assert!(config.retryable_status_codes.contains(&503)); // Service Unavailable
        assert!(config.retryable_status_codes.contains(&504)); // Gateway Timeout

        // Verify non-retryable codes are not present
        assert!(!config.retryable_status_codes.contains(&400)); // Bad Request
        assert!(!config.retryable_status_codes.contains(&401)); // Unauthorized
        assert!(!config.retryable_status_codes.contains(&403)); // Forbidden
        assert!(!config.retryable_status_codes.contains(&404)); // Not Found
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let config = RetryConfig::default();
        let mut delay_ms = config.initial_delay_ms;

        // Simulate exponential backoff without jitter
        let expected_delays = [1000, 2000, 4000, 8000, 16000, 30000]; // capped at max_delay_ms

        for (i, expected) in expected_delays.iter().enumerate() {
            if i > 0 {
                delay_ms = (delay_ms * 2).min(config.max_delay_ms);
            }
            assert_eq!(delay_ms, *expected, "Mismatch at iteration {}", i);
        }
    }

    #[test]
    fn test_backoff_respects_max_delay() {
        let config = RetryConfig {
            max_retries: 10,
            initial_delay_ms: 10000,
            max_delay_ms: 15000,
            retryable_status_codes: vec![500],
        };

        let mut delay_ms = config.initial_delay_ms;

        // After first backoff: 10000 * 2 = 20000, but capped at 15000
        delay_ms = (delay_ms * 2).min(config.max_delay_ms);
        assert_eq!(delay_ms, 15000);

        // Subsequent backoffs should stay at max
        delay_ms = (delay_ms * 2).min(config.max_delay_ms);
        assert_eq!(delay_ms, 15000);
    }

    // ============================================
    // Request Construction Tests
    // ============================================

    #[test]
    fn test_chat_request_body_construction_basic() {
        // Test that basic chat request body is constructed correctly
        let messages = vec![Message::system("You are helpful"), Message::user("Hello")];

        let body = serde_json::json!({
            "model": "test-model",
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 4096,
            "stream": false,
        });

        // Verify the structure
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["stream"], false);
        assert!(body["messages"].is_array());
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_chat_request_body_with_tools() {
        let messages = vec![Message::user("Read a file")];

        let tools = vec![ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: "file_read".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                }),
            },
        }];

        let mut body = serde_json::json!({
            "model": "test-model",
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 4096,
            "stream": false,
        });

        body["tools"] = serde_json::json!(tools);

        // Verify tools are included
        assert!(body.get("tools").is_some());
        let tools_array = body["tools"].as_array().unwrap();
        assert_eq!(tools_array.len(), 1);
        assert_eq!(tools_array[0]["function"]["name"], "file_read");
    }

    #[test]
    fn test_chat_request_body_with_thinking_disabled() {
        let body = serde_json::json!({
            "model": "test-model",
            "messages": [],
            "thinking": {"type": "disabled"}
        });

        assert_eq!(body["thinking"]["type"], "disabled");
    }

    #[test]
    fn test_chat_request_body_with_thinking_budget() {
        let budget_tokens = 2048;
        let body = serde_json::json!({
            "model": "test-model",
            "messages": [],
            "thinking": {
                "type": "enabled",
                "budget_tokens": budget_tokens
            }
        });

        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], budget_tokens);
    }

    // ============================================
    // Response Parsing Tests
    // ============================================

    #[test]
    fn test_parse_chat_response_basic() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you today?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.object, "chat.completion");
        assert_eq!(response.model, "test-model");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            "Hello! How can I help you today?"
        );
        assert_eq!(response.usage.prompt_tokens, 9);
        assert_eq!(response.usage.completion_tokens, 12);
        assert_eq!(response.usage.total_tokens, 21);
    }

    #[test]
    fn test_parse_chat_response_with_tool_calls() {
        let json = r#"{
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_abc123",
                        "type": "function",
                        "function": {
                            "name": "file_read",
                            "arguments": "{\"path\": \"/tmp/test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 20,
                "total_tokens": 35
            }
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();

        assert_eq!(
            response.choices[0].finish_reason,
            Some("tool_calls".to_string())
        );
        let tool_calls = response.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_abc123");
        assert_eq!(tool_calls[0].function.name, "file_read");
    }

    #[test]
    fn test_parse_chat_response_with_reasoning() {
        let json = r#"{
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "The answer is 42.",
                    "reasoning_content": "Let me think about this step by step..."
                },
                "reasoning_content": "Let me think about this step by step...",
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 50,
                "total_tokens": 60
            }
        }"#;

        let response: ChatResponse = serde_json::from_str(json).unwrap();

        assert_eq!(response.choices[0].message.content, "The answer is 42.");
        assert_eq!(
            response.choices[0].message.reasoning_content,
            Some("Let me think about this step by step...".to_string())
        );
    }

    #[test]
    fn test_parse_chat_response_invalid_json() {
        let json = r#"{ invalid json }"#;
        let result: Result<ChatResponse, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_chat_response_missing_required_fields() {
        // Missing "choices" field
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "test-model",
            "usage": {
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        }"#;

        let result: Result<ChatResponse, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // ============================================
    // Error Handling Tests
    // ============================================

    #[test]
    fn test_http_status_code_classification() {
        // Test that we can correctly identify retryable vs non-retryable status codes
        let config = RetryConfig::default();

        // Retryable status codes
        let retryable = [429, 500, 502, 503, 504];
        for code in retryable {
            assert!(
                config.retryable_status_codes.contains(&code),
                "Status {} should be retryable",
                code
            );
        }

        // Non-retryable status codes (client errors)
        let non_retryable = [400, 401, 403, 404, 405, 422];
        for code in non_retryable {
            assert!(
                !config.retryable_status_codes.contains(&code),
                "Status {} should NOT be retryable",
                code
            );
        }
    }

    #[test]
    fn test_error_message_format() {
        // Test that error messages are properly formatted
        let status = 429u16;
        let error_text = "Rate limit exceeded. Please retry after 60 seconds.";
        let error = format!("API error {}: {}", status, error_text);

        assert!(error.contains("429"));
        assert!(error.contains("Rate limit"));
    }

    #[test]
    fn test_parse_sse_event_with_tool_call() {
        // Test parsing SSE event with tool call delta (streaming scenario)
        let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"id":"call_123","type":"function","function":{"name":"file_read","arguments":"{\"path\":\"/test\"}"}}]}}]}"#;

        // The current implementation doesn't parse tool_calls in SSE events
        // This test documents the current behavior
        let result = parse_sse_event(event);
        // Tool calls are not extracted in the current SSE parser
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_sse_event_finish_reason() {
        // Test SSE event with finish_reason but no content
        let event = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let result = parse_sse_event(event);
        // Should return None since there's no content or reasoning
        assert!(result.is_none());
    }

    // ============================================
    // API URL Construction Tests
    // ============================================

    #[test]
    fn test_api_url_construction() {
        let base_url = "http://localhost:8000/v1";
        let url = format!("{}/chat/completions", base_url);
        assert_eq!(url, "http://localhost:8000/v1/chat/completions");
    }

    #[test]
    fn test_api_url_construction_with_trailing_slash() {
        let base_url = "http://localhost:8000/v1/";
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        assert_eq!(url, "http://localhost:8000/v1/chat/completions");
    }

    #[test]
    fn test_api_url_construction_https() {
        let base_url = "https://api.example.com/v1";
        let url = format!("{}/chat/completions", base_url);
        assert_eq!(url, "https://api.example.com/v1/chat/completions");
    }

    // ============================================
    // Stream Chunk Processing Tests
    // ============================================

    #[test]
    fn test_stream_chunk_tool_call() {
        let tool_call = ToolCall {
            id: "call_test".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "test_function".to_string(),
                arguments: r#"{"arg": "value"}"#.to_string(),
            },
        };

        let chunk = StreamChunk::ToolCall(tool_call.clone());
        if let StreamChunk::ToolCall(tc) = chunk {
            assert_eq!(tc.id, "call_test");
            assert_eq!(tc.function.name, "test_function");
        } else {
            panic!("Expected ToolCall variant");
        }
    }

    #[test]
    fn test_multiple_sse_events_in_buffer() {
        // Simulate multiple SSE events in a buffer (as would happen during streaming)
        let buffer = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n";

        let events: Vec<&str> = buffer.split("\n\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(events.len(), 2);

        // First event
        let result1 = parse_sse_event(events[0]);
        assert!(matches!(result1, Some(StreamChunk::Content(_))));
        if let Some(StreamChunk::Content(text)) = result1 {
            assert_eq!(text, "Hello");
        }

        // Second event
        let result2 = parse_sse_event(events[1]);
        assert!(matches!(result2, Some(StreamChunk::Content(_))));
        if let Some(StreamChunk::Content(text)) = result2 {
            assert_eq!(text, " world");
        }
    }

    #[test]
    fn test_parse_sse_event_with_whitespace() {
        // Test SSE event with extra whitespace
        let event = "  data: [DONE]  ";
        // The parser strips "data: " prefix, should handle this
        let result = parse_sse_event(event.trim());
        assert!(matches!(result, Some(StreamChunk::Done)));
    }

    #[test]
    fn test_retry_config_empty_retryable_codes() {
        // Edge case: no retryable status codes
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            retryable_status_codes: vec![],
        };

        assert!(!config.retryable_status_codes.contains(&500));
        assert!(!config.retryable_status_codes.contains(&429));
    }

    #[test]
    fn test_retry_config_with_zero_retries() {
        // Edge case: no retries allowed
        let config = RetryConfig {
            max_retries: 0,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            retryable_status_codes: vec![500],
        };

        assert_eq!(config.max_retries, 0);
        // With max_retries = 0, only 1 attempt (the initial one) should be made
    }

    #[test]
    fn test_retry_config_with_zero_delays() {
        // Edge case: instant retries (no delay)
        let config = RetryConfig {
            max_retries: 3,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            retryable_status_codes: vec![500],
        };

        assert_eq!(config.initial_delay_ms, 0);
        assert_eq!(config.max_delay_ms, 0);
        // Even with doubling, 0 * 2 = 0
    }
}

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

        /// Create a mock client pre-loaded with a sequence of responses.
        ///
        /// Each call to `chat()` pops the next response from the front of the
        /// queue. If the queue is exhausted, `chat()` returns an error.
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
        ) -> Result<ChatResponse> {
            let mut queue = self
                .responses
                .lock()
                .map_err(|e| anyhow::anyhow!("MockLlmClient lock poisoned: {}", e))?;
            queue
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("MockLlmClient: no more responses in queue"))
        }

        async fn chat_stream(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<ToolDefinition>>,
            _thinking: ThinkingMode,
        ) -> Result<StreamingResponse> {
            anyhow::bail!("Streaming not supported in MockLlmClient")
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn sample_response(content: &str) -> ChatResponse {
            ChatResponse {
                id: "mock-id".to_string(),
                object: "chat.completion".to_string(),
                created: 0,
                model: "mock-model".to_string(),
                choices: vec![Choice {
                    index: 0,
                    message: Message::assistant(content),
                    reasoning_content: None,
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            }
        }

        #[tokio::test]
        async fn test_mock_returns_queued_responses() {
            let mock = MockLlmClient::with_responses(vec![
                sample_response("first"),
                sample_response("second"),
            ]);

            let r1 = mock
                .chat(vec![], None, ThinkingMode::Disabled)
                .await
                .unwrap();
            assert_eq!(r1.choices[0].message.content, "first");

            let r2 = mock
                .chat(vec![], None, ThinkingMode::Disabled)
                .await
                .unwrap();
            assert_eq!(r2.choices[0].message.content, "second");
        }

        #[tokio::test]
        async fn test_mock_errors_when_queue_exhausted() {
            let mock = MockLlmClient::new();
            let result = mock.chat(vec![], None, ThinkingMode::Disabled).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("no more responses"));
        }

        #[tokio::test]
        async fn test_mock_stream_returns_error() {
            let mock = MockLlmClient::new();
            let result = mock.chat_stream(vec![], None, ThinkingMode::Disabled).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Streaming not supported"));
        }
    }
}
