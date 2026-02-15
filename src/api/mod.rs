use anyhow::{Context, Result};
use futures::StreamExt;
use reqwest::{Client, StatusCode};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub mod types;

use types::*;

// ─── Circuit Breaker ─────────────────────────────────────────────────

/// Circuit breaker states
const CB_CLOSED: u8 = 0; // Normal operation — requests flow through
const CB_OPEN: u8 = 1; // Tripped — requests are rejected immediately
const CB_HALF_OPEN: u8 = 2; // Probing — one request allowed to test recovery

/// Circuit breaker for LLM API calls.
///
/// Prevents cascading failures when the upstream LLM endpoint is down or
/// overloaded.  After `failure_threshold` consecutive failures the breaker
/// *opens* and rejects calls instantly for `open_timeout`.  After the
/// timeout it moves to *half-open* and allows a single probe request.
pub struct CircuitBreaker {
    state: AtomicU8,
    consecutive_failures: AtomicU32,
    failure_threshold: u32,
    open_timeout: Duration,
    last_failure_time: Mutex<Option<Instant>>,
}

impl CircuitBreaker {
    /// Create a new circuit breaker.
    ///
    /// * `failure_threshold` — consecutive failures before opening (default 5).
    /// * `open_timeout` — how long to stay open before probing (default 30 s).
    pub fn new(failure_threshold: u32, open_timeout: Duration) -> Self {
        Self {
            state: AtomicU8::new(CB_CLOSED),
            consecutive_failures: AtomicU32::new(0),
            failure_threshold,
            open_timeout,
            last_failure_time: Mutex::new(None),
        }
    }

    /// Check whether a request is allowed.
    ///
    /// Returns `true` if the request should proceed (closed or half-open probe),
    /// `false` if the breaker is open.
    pub fn allow_request(&self) -> bool {
        let state = self.state.load(Ordering::Acquire);
        match state {
            CB_CLOSED => true,
            CB_OPEN => {
                // Check if the open timeout has elapsed
                let should_probe = {
                    let guard = self.last_failure_time.lock().unwrap();
                    guard
                        .map(|t| t.elapsed() >= self.open_timeout)
                        .unwrap_or(true)
                };
                if should_probe {
                    // Transition to half-open (only one probe)
                    self.state
                        .compare_exchange(CB_OPEN, CB_HALF_OPEN, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                } else {
                    false
                }
            }
            CB_HALF_OPEN => false, // Only one probe at a time
            _ => true,
        }
    }

    /// Record a successful response, resetting the breaker to closed.
    pub fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Release);
        let prev = self.state.swap(CB_CLOSED, Ordering::AcqRel);
        if prev != CB_CLOSED {
            info!("Circuit breaker closed — LLM endpoint recovered");
        }
    }

    /// Record a failed response.  If failures exceed the threshold the
    /// breaker opens.
    pub fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::AcqRel) + 1;
        *self.last_failure_time.lock().unwrap() = Some(Instant::now());

        if failures >= self.failure_threshold {
            let prev = self.state.swap(CB_OPEN, Ordering::AcqRel);
            if prev != CB_OPEN {
                warn!(
                    "Circuit breaker OPEN after {} consecutive failures — \
                     rejecting requests for {:?}",
                    failures, self.open_timeout
                );
            }
        }
    }

    /// Current state name (for diagnostics / logging).
    pub fn state_name(&self) -> &'static str {
        match self.state.load(Ordering::Acquire) {
            CB_CLOSED => "closed",
            CB_OPEN => "open",
            CB_HALF_OPEN => "half-open",
            _ => "unknown",
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(5, Duration::from_secs(30))
    }
}

/// A streaming response that yields chunks as they arrive
#[allow(dead_code)]
pub struct StreamingResponse {
    response: reqwest::Response,
}

impl StreamingResponse {
    fn new(response: reqwest::Response) -> Self {
        Self { response }
    }

    /// Process the stream and send chunks through a channel
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

pub struct ApiClient {
    client: Client,
    config: crate::config::Config,
    base_url: String,
    retry_config: RetryConfig,
    circuit_breaker: CircuitBreaker,
}

impl ApiClient {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        // Use step_timeout from config, with 4 hour default for slow local models
        let request_timeout = config.agent.step_timeout_secs.max(14400);
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout)) // From config, min 4 hours
            .connect_timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url: config.endpoint.clone(),
            config: config.clone(),
            retry_config: RetryConfig::default(),
            circuit_breaker: CircuitBreaker::default(),
        })
    }

    /// Create client with custom retry configuration
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

    /// Send request with exponential backoff retry logic and circuit breaker protection.
    async fn send_with_retry(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        // Circuit breaker check — fail fast when the endpoint is known to be down.
        if !self.circuit_breaker.allow_request() {
            anyhow::bail!(
                "Circuit breaker is {} — LLM endpoint temporarily unavailable. \
                 Will probe again in {:?}.",
                self.circuit_breaker.state_name(),
                Duration::from_secs(30)
            );
        }

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
                        self.circuit_breaker.record_success();
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
                    self.circuit_breaker.record_failure();
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
    #[allow(dead_code)]
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

    // ─── Circuit Breaker Tests ───────────────────────────────────────

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.state_name(), "closed");
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));

        // Three consecutive failures should trip the breaker
        cb.record_failure();
        assert!(cb.allow_request()); // still closed
        cb.record_failure();
        assert!(cb.allow_request()); // still closed
        cb.record_failure(); // threshold reached
        assert_eq!(cb.state_name(), "open");
        assert!(!cb.allow_request()); // now open — blocked
    }

    #[test]
    fn test_circuit_breaker_success_resets_failures() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));

        cb.record_failure();
        cb.record_failure();
        // Two failures, one more would trip it
        cb.record_success(); // reset counter
        cb.record_failure();
        cb.record_failure();
        // Still only 2 failures since the reset
        assert_eq!(cb.state_name(), "closed");
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        // Use a 0ms timeout so it expires immediately
        let cb = CircuitBreaker::new(1, Duration::from_millis(0));

        cb.record_failure(); // opens the breaker
        assert_eq!(cb.state_name(), "open");

        // Timeout is 0ms, so next allow_request should transition to half-open
        std::thread::sleep(Duration::from_millis(1));
        assert!(cb.allow_request()); // transitions to half-open, allows probe
        assert_eq!(cb.state_name(), "half-open");
        assert!(!cb.allow_request()); // second request blocked in half-open
    }

    #[test]
    fn test_circuit_breaker_closes_on_probe_success() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(0));

        cb.record_failure();
        std::thread::sleep(Duration::from_millis(1));
        assert!(cb.allow_request()); // half-open probe

        cb.record_success(); // probe succeeded — close the breaker
        assert_eq!(cb.state_name(), "closed");
        assert!(cb.allow_request());
    }

    #[test]
    fn test_circuit_breaker_reopens_on_probe_failure() {
        let cb = CircuitBreaker::new(1, Duration::from_millis(0));

        cb.record_failure(); // open
        std::thread::sleep(Duration::from_millis(1));
        assert!(cb.allow_request()); // half-open probe

        cb.record_failure(); // probe failed — back to open
        assert_eq!(cb.state_name(), "open");
    }

    #[test]
    fn test_circuit_breaker_default_values() {
        let cb = CircuitBreaker::default();
        assert_eq!(cb.failure_threshold, 5);
        assert_eq!(cb.open_timeout, Duration::from_secs(30));
    }
}
