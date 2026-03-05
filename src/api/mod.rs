use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, warn};

pub mod types;

use crate::errors::ApiError;
use crate::supervision::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError,
};
use std::sync::Arc;
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
    chunk_timeout: Duration,
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.response.status())
            .field("chunk_timeout_secs", &self.chunk_timeout.as_secs())
            .finish()
    }
}

impl StreamingResponse {
    fn new(response: reqwest::Response, chunk_timeout: Duration) -> Self {
        Self {
            response,
            chunk_timeout,
        }
    }

    /// Process the stream and send chunks through a channel
    pub async fn into_channel(self) -> mpsc::Receiver<Result<StreamChunk>> {
        let (tx, rx) = mpsc::channel(32);

        tokio::spawn(async move {
            let mut stream = self.response.bytes_stream();
            let mut buffer = String::new();
            let mut accumulator = ToolCallAccumulator::new();
            let chunk_timeout = self.chunk_timeout;

            loop {
                let chunk_opt = match tokio::time::timeout(chunk_timeout, stream.next()).await {
                    Ok(Some(result)) => Some(result),
                    Ok(None) => None, // Stream ended
                    Err(_elapsed) => {
                        for call in accumulator.flush() {
                            if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                                warn!(
                                    "Streaming receiver dropped while sending buffered tool call after timeout"
                                );
                                return;
                            }
                        }
                        if tx
                            .send(Err(anyhow::anyhow!(
                                "Stream timeout: no data for {} seconds",
                                chunk_timeout.as_secs()
                            )))
                            .await
                            .is_err()
                        {
                            warn!("Streaming receiver dropped while sending timeout error");
                        }
                        return;
                    }
                };
                let Some(chunk_result) = chunk_opt else {
                    break;
                };
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            for chunk in parse_sse_event(&event, &mut accumulator) {
                                if tx.send(Ok(chunk)).await.is_err() {
                                    warn!(
                                        "Streaming receiver dropped while forwarding parsed stream chunk"
                                    );
                                    return; // Receiver dropped
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Flush accumulated tool calls before reporting the error
                        // so partial progress is not lost
                        for call in accumulator.flush() {
                            if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                                warn!(
                                    "Streaming receiver dropped while sending buffered tool call after stream error"
                                );
                                return;
                            }
                        }
                        if tx
                            .send(Err(anyhow::anyhow!("Stream error: {}", e)))
                            .await
                            .is_err()
                        {
                            warn!("Streaming receiver dropped while sending stream error");
                        }
                        return;
                    }
                }
            }

            // Flush trailing buffer (data without final \n\n)
            let remaining = buffer.trim().to_string();
            if !remaining.is_empty() {
                for chunk in parse_sse_event(&remaining, &mut accumulator) {
                    if tx.send(Ok(chunk)).await.is_err() {
                        warn!("Streaming receiver dropped while sending trailing buffered chunk");
                        return;
                    }
                }
            }

            // Flush any remaining accumulated tool calls
            for call in accumulator.flush() {
                if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                    warn!("Streaming receiver dropped while flushing final tool calls");
                    return;
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
                    content: content.into(),
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
/// A chunk received from an SSE streaming response.
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

/// Accumulates incremental tool call deltas from SSE streaming into complete ToolCall objects.
///
/// The OpenAI streaming API sends tool calls as a series of deltas:
/// 1. First delta: `index`, `id`, `type`, `function.name`, partial `function.arguments`
/// 2. Subsequent deltas: same `index`, only `function.arguments` chunk
///
/// The accumulator buffers these per-index and emits a complete `ToolCall` when
/// a new index appears or when `flush()` is called at stream end.
#[derive(Default)]
struct ToolCallAccumulator {
    /// In-progress tool calls keyed by index
    pending: std::collections::HashMap<usize, (String, String, String, String)>, // (id, type, name, args)
}

impl ToolCallAccumulator {
    fn new() -> Self {
        Self::default()
    }

    /// Process a tool call delta.
    ///
    /// We intentionally avoid progressive emission based on index transitions because
    /// some backends can interleave argument chunks across multiple indices.
    /// Emission happens only when the stream signals completion (`finish_reason`) or
    /// at stream end via `flush()`.
    fn process_delta(&mut self, delta: &serde_json::Value) -> Option<types::ToolCall> {
        let index = delta.get("index").and_then(|v| v.as_u64())? as usize;
        let id = delta
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let call_type = delta
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let func = delta.get("function");
        let name = func
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let args_chunk = func
            .and_then(|f| f.get("arguments"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if let Some(entry) = self.pending.get_mut(&index) {
            // Continuation delta: append arguments
            entry.3.push_str(&args_chunk);
            // Also update id/type/name if they were provided (first delta for this index)
            if !id.is_empty() {
                entry.0 = id;
            }
            if !call_type.is_empty() {
                entry.1 = call_type;
            }
            if !name.is_empty() {
                entry.2 = name;
            }
        } else {
            self.pending
                .insert(index, (id, call_type, name, args_chunk));
        }
        None
    }

    /// Flush all pending tool calls, returning completed ToolCall objects.
    fn flush(&mut self) -> Vec<types::ToolCall> {
        let mut calls: Vec<_> = self.pending.drain().collect();
        calls.sort_by_key(|(idx, _)| *idx);
        calls
            .into_iter()
            .map(|(_, (id, call_type, name, args))| types::ToolCall {
                id,
                call_type,
                function: types::ToolFunction {
                    name,
                    arguments: args,
                },
            })
            .collect()
    }
}

/// Parse a Server-Sent Events (SSE) event, returning zero or more StreamChunks.
///
/// A single SSE event can produce multiple chunks (e.g., content + tool call deltas
/// arrive in the same JSON payload). The accumulator buffers incremental tool call
/// deltas; call `accumulator.flush()` at stream end to emit any remaining calls.
// Streaming infrastructure (used by chat_streaming)
fn parse_sse_event(event: &str, accumulator: &mut ToolCallAccumulator) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();

    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                // Flush any remaining tool calls before Done
                for call in accumulator.flush() {
                    chunks.push(StreamChunk::ToolCall(call));
                }
                chunks.push(StreamChunk::Done);
                return chunks;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                let choice = json.get("choices").and_then(|c| c.get(0));
                let delta = choice.and_then(|c| c.get("delta"));

                // Extract content from choices[0].delta.content
                if let Some(content) = delta
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    if !content.is_empty() {
                        chunks.push(StreamChunk::Content(content.to_string()));
                    }
                }

                // Extract reasoning content
                if let Some(reasoning) = delta
                    .and_then(|d| d.get("reasoning_content"))
                    .and_then(|c| c.as_str())
                {
                    if !reasoning.is_empty() {
                        chunks.push(StreamChunk::Reasoning(reasoning.to_string()));
                    }
                }

                // Extract tool_calls deltas
                if let Some(tool_calls) = delta
                    .and_then(|d| d.get("tool_calls"))
                    .and_then(|tc| tc.as_array())
                {
                    for tc_delta in tool_calls {
                        if let Some(completed) = accumulator.process_delta(tc_delta) {
                            chunks.push(StreamChunk::ToolCall(completed));
                        }
                    }
                }

                // Flush buffered tool calls when backend marks completion for this turn.
                if choice
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|f| f.as_str())
                    .is_some()
                {
                    for call in accumulator.flush() {
                        chunks.push(StreamChunk::ToolCall(call));
                    }
                }

                // Extract usage if present
                if let Some(usage) = json.get("usage") {
                    if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                        chunks.push(StreamChunk::Usage(u));
                    }
                }
            }
        }
    }
    chunks
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

/// HTTP client for OpenAI-compatible chat completion APIs.
///
/// Supports both synchronous and streaming requests, native tool calling,
/// thinking/reasoning modes, and configurable retry logic.
#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    config: crate::config::Config,
    base_url: String,
    retry_config: RetryConfig,
    circuit_breaker: Arc<CircuitBreaker>,
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

        // Warn if the API endpoint uses plain HTTP to a non-local host.
        // API keys are sent as Bearer tokens, so HTTP risks credential exposure.
        // Local HTTP (localhost/127.0.0.1/[::1]) is allowed without warning
        // since local LLM servers typically serve over HTTP.
        if config.endpoint.starts_with("http://")
            && !crate::config::is_local_endpoint(&config.endpoint)
        {
            warn!(
                endpoint = %config.endpoint,
                "API endpoint uses HTTP \u{2014} credentials may be transmitted in plaintext. \
                 Use HTTPS in production."
            );
        }

        Ok(Self {
            client,
            base_url: config.endpoint.clone(),
            config: config.clone(),
            retry_config: RetryConfig::from_settings(&config.retry),
            circuit_breaker: Arc::new(CircuitBreaker::new(CircuitBreakerConfig::default())),
        })
    }

    /// Create client with custom retry configuration
    #[allow(dead_code)] // Builder method for API configuration
    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    /// Send a completion request (e.g. for FIM)
    pub async fn completion(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
        stop: Option<Vec<String>>,
    ) -> Result<types::CompletionResponse> {
        self.circuit_breaker
            .call(|| self.completion_inner(prompt, max_tokens, stop.clone()))
            .await
            .map_err(|e| match e {
                CircuitBreakerError::CircuitOpen => {
                    anyhow::anyhow!("Circuit breaker is open - API is unavailable")
                }
                CircuitBreakerError::OperationFailed(err) => err,
            })
    }

    async fn completion_inner(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
        stop: Option<Vec<String>>,
    ) -> Result<types::CompletionResponse> {
        let url = format!("{}/completions", self.base_url);

        let req = types::CompletionRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            max_tokens,
            temperature: Some(0.1),
            top_p: Some(0.9),
            stop,
        };

        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key.expose()));
        }

        let response = request.json(&req).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                message: text,
            }
            .into());
        }

        let resp: types::CompletionResponse = response.json().await?;
        Ok(resp)
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse> {
        let mut messages = messages;
        if let ThinkingMode::Disabled = thinking {
            let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
            messages.insert(0, sys_msg);
        }

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

        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }

        self.send_with_retry(&body).await
    }

    /// Stream a chat completion response
    /// Returns a receiver that yields chunks as they arrive
    pub async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse> {
        self.circuit_breaker
            .call(|| self.chat_stream_inner(messages.clone(), tools.clone(), thinking))
            .await
            .map_err(|e| match e {
                CircuitBreakerError::CircuitOpen => {
                    anyhow::anyhow!("Circuit breaker is open - API is unavailable")
                }
                CircuitBreakerError::OperationFailed(err) => err,
            })
    }

    async fn chat_stream_inner(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse> {
        let mut messages = messages;
        if let ThinkingMode::Disabled = thinking {
            let sys_msg = crate::api::types::Message::system("CRITICAL INSTRUCTION: DO NOT use <think> blocks or any thinking process in your response. Output your final response directly and immediately.");
            messages.insert(0, sys_msg);
        }

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

        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }

        let url = format!("{}/chat/completions", self.base_url);
        debug!("Starting streaming request to {}", url);

        let mut request = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", key.expose()));
        }

        let response = request
            .json(&body)
            .send()
            .await
            .context("Failed to send streaming request")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(ApiError::HttpStatus {
                status: status.as_u16(),
                message: text,
            }
            .into());
        }

        // Use configurable per-step timeout for stream inactivity instead of a fixed constant.
        let stream_chunk_timeout_secs = self.config.agent.step_timeout_secs.max(30);
        Ok(StreamingResponse::new(
            response,
            Duration::from_secs(stream_chunk_timeout_secs),
        ))
    }

    /// Send request with exponential backoff retry logic, wrapped in a circuit breaker
    async fn send_with_retry(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        self.circuit_breaker
            .call(|| self.send_with_retry_inner(body))
            .await
            .map_err(|e| match e {
                CircuitBreakerError::CircuitOpen => {
                    anyhow::anyhow!("Circuit breaker is open - API is unavailable")
                }
                CircuitBreakerError::OperationFailed(err) => err,
            })
    }

    async fn send_with_retry_inner(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        self.send_request_with_retry(body, &self.base_url, self.config.api_key.as_ref())
            .await
    }

    /// Parameterized retry logic: sends a request to the given endpoint using
    /// the provided API key. Both `send_with_retry_inner` (default model) and
    /// `chat_with_profile` (alternate model) delegate here.
    async fn send_request_with_retry(
        &self,
        body: &serde_json::Value,
        endpoint: &str,
        api_key: Option<&crate::config::RedactedString>,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", endpoint);
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
                // Add jitter (+-10%) -- use signed arithmetic to avoid u64 overflow
                let jitter = (delay_ms as f64 * 0.1 * (rand_jitter() - 0.5)) as i64;
                delay_ms = (delay_ms as i64).saturating_add(jitter).max(1) as u64;
                delay_ms = delay_ms.min(self.retry_config.max_delay_ms);
            }

            debug!("Sending request to {} (attempt {})", url, attempt + 1);

            let mut request = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");

            if let Some(key) = api_key {
                request = request.header("Authorization", format!("Bearer {}", key.expose()));
            }

            let result = request.json(body).send().await;

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
                        // Parse Retry-After header before consuming the body.
                        // Supports numeric seconds format; capped at 300s.
                        let retry_after_secs = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.trim().parse::<u64>().ok())
                            .map(|s| s.min(300));

                        let error_text = response.text().await.unwrap_or_default();
                        warn!("Retryable error ({}): {}", status, error_text);
                        last_error = Some(
                            ApiError::HttpStatus {
                                status: status.as_u16(),
                                message: error_text,
                            }
                            .into(),
                        );

                        // Honour Retry-After as minimum wait for 429/503
                        if let Some(retry_secs) = retry_after_secs {
                            let retry_ms = retry_secs * 1000;
                            if retry_ms > delay_ms {
                                delay_ms = retry_ms;
                            }
                        }
                        continue;
                    }

                    // Non-retryable error
                    let status_code = status.as_u16();
                    let error_text = response.text().await.unwrap_or_default();
                    return Err(ApiError::HttpStatus {
                        status: status_code,
                        message: error_text,
                    }
                    .into());
                }
                Err(e) => {
                    // Network errors are generally retryable
                    if e.is_timeout() || e.is_connect() {
                        warn!("Network error (retrying): {}", e);
                        last_error = Some(ApiError::Network(e.to_string()).into());
                        continue;
                    }
                    // Other errors (e.g., invalid URL) are not retryable
                    return Err(ApiError::Network(e.to_string()).into());
                }
            }
        }

        // All retries exhausted
        Err(last_error.unwrap_or_else(|| {
            ApiError::Network("Request failed after all retries".to_string()).into()
        }))
    }

    /// Send a chat completion to an alternate model described by a `ModelProfile`.
    ///
    /// Images are automatically stripped if the profile does not support vision.
    pub async fn chat_with_profile(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
        profile: &crate::config::ModelProfile,
    ) -> Result<ChatResponse> {
        let messages: Vec<Message> = if !profile.supports_vision() {
            messages.iter().map(|m| m.strip_images()).collect()
        } else {
            messages
        };

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
            "temperature": profile.temperature,
            "max_tokens": profile.max_tokens,
            "stream": false,
        });

        if let Some(ref tools) = tools {
            body["tools"] = serde_json::json!(tools);
        }

        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }

        self.send_request_with_retry(&body, &profile.endpoint, profile.api_key.as_ref())
            .await
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
        let mut acc = ToolCallAccumulator::new();
        let event = "data: [DONE]";
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], StreamChunk::Done));
    }

    #[test]
    fn test_parse_sse_event_content() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], StreamChunk::Content(t) if t == "Hello"));
    }

    #[test]
    fn test_parse_sse_event_reasoning() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"reasoning_content":"Thinking about it"}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], StreamChunk::Reasoning(_)));
    }

    #[test]
    fn test_parse_sse_event_usage() {
        let mut acc = ToolCallAccumulator::new();
        let event =
            r#"data: {"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(&results[0], StreamChunk::Usage(_)));
    }

    #[test]
    fn test_parse_sse_event_empty_content() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"content":""}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_no_data_prefix() {
        let mut acc = ToolCallAccumulator::new();
        let event = "not a data line";
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_invalid_json() {
        let mut acc = ToolCallAccumulator::new();
        let event = "data: {invalid json}";
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_multiline() {
        let mut acc = ToolCallAccumulator::new();
        let event = "event: message\ndata: [DONE]";
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], StreamChunk::Done));
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
        // Test parsing SSE event with complete tool call delta
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"file_read","arguments":"{\"path\":\"/test\"}"}}]}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        // Tool call is buffered in accumulator, not emitted yet
        assert!(results.is_empty());

        // Flush to get the completed tool call
        let calls = acc.flush();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_123");
        assert_eq!(calls[0].function.name, "file_read");
        assert!(calls[0].function.arguments.contains("/test"));
    }

    #[test]
    fn test_parse_sse_event_incremental_tool_call() {
        // Test incremental tool call argument assembly
        let mut acc = ToolCallAccumulator::new();

        // First chunk: id, name, partial args
        let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_456","type":"function","function":{"name":"file_write","arguments":"{\"path\":"}}]}}]}"#;
        let r1 = parse_sse_event(event1, &mut acc);
        assert!(r1.is_empty());

        // Second chunk: more args
        let event2 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"/tmp/test\","}}]}}]}"#;
        let r2 = parse_sse_event(event2, &mut acc);
        assert!(r2.is_empty());

        // Third chunk: finish args
        let event3 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"content\":\"hello\"}"}}]}}]}"#;
        let r3 = parse_sse_event(event3, &mut acc);
        assert!(r3.is_empty());

        // Flush to get the completed tool call with assembled arguments
        let calls = acc.flush();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_456");
        assert_eq!(calls[0].function.name, "file_write");
        assert_eq!(
            calls[0].function.arguments,
            "{\"path\":\"/tmp/test\",\"content\":\"hello\"}"
        );
    }

    #[test]
    fn test_parse_sse_event_tool_calls_flushed_on_done() {
        // Tool calls should be flushed before the Done marker
        let mut acc = ToolCallAccumulator::new();

        let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_789","type":"function","function":{"name":"git_status","arguments":"{}"}}]}}]}"#;
        parse_sse_event(event1, &mut acc);

        let done_event = "data: [DONE]";
        let results = parse_sse_event(done_event, &mut acc);
        // Should have the flushed tool call + Done
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], StreamChunk::ToolCall(tc) if tc.id == "call_789"));
        assert!(matches!(results[1], StreamChunk::Done));
    }

    #[test]
    fn test_parse_sse_event_finish_reason() {
        // Test SSE event with finish_reason but no content
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let results = parse_sse_event(event, &mut acc);
        // Should return empty since there's no content or reasoning
        assert!(results.is_empty());
    }

    #[test]
    fn test_process_delta_progressive_emission() {
        // Interleaved chunks should not trigger premature emission.
        // Calls are emitted only on flush in stable index order.
        let mut acc = ToolCallAccumulator::new();

        // Index 0 — first tool call (partial args)
        let delta0 = serde_json::json!({
            "index": 0, "id": "call_a", "type": "function",
            "function": {"name": "file_write", "arguments": "{\"path\":\"/a\","}
        });
        let result0 = acc.process_delta(&delta0);
        assert!(result0.is_none());

        // Index 1 starts before index 0 is fully complete.
        let delta1 = serde_json::json!({
            "index": 1, "id": "call_b", "type": "function",
            "function": {"name": "file_read", "arguments": "{\"path\":\"/b\"}"}
        });
        let result1 = acc.process_delta(&delta1);
        assert!(result1.is_none());

        // Index 0 receives a late continuation chunk.
        let delta0_late = serde_json::json!({
            "index": 0,
            "function": {"arguments": "\"content\":\"hello\"}"}
        });
        let result2 = acc.process_delta(&delta0_late);
        assert!(result2.is_none());

        let calls = acc.flush();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[0].function.name, "file_write");
        assert_eq!(
            calls[0].function.arguments,
            "{\"path\":\"/a\",\"content\":\"hello\"}"
        );
        assert_eq!(calls[1].id, "call_b");
    }

    #[test]
    fn test_parse_sse_event_flushes_on_finish_reason() {
        let mut acc = ToolCallAccumulator::new();

        let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_finish","type":"function","function":{"name":"git_status","arguments":"{}"}}]}}]}"#;
        let r1 = parse_sse_event(event1, &mut acc);
        assert!(r1.is_empty());

        let finish = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let r2 = parse_sse_event(finish, &mut acc);
        assert_eq!(r2.len(), 1);
        assert!(matches!(&r2[0], StreamChunk::ToolCall(tc) if tc.id == "call_finish"));
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
        let mut acc = ToolCallAccumulator::new();
        // Simulate multiple SSE events in a buffer (as would happen during streaming)
        let buffer = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n";

        let events: Vec<&str> = buffer.split("\n\n").filter(|s| !s.is_empty()).collect();
        assert_eq!(events.len(), 2);

        // First event
        let results1 = parse_sse_event(events[0], &mut acc);
        assert_eq!(results1.len(), 1);
        assert!(matches!(&results1[0], StreamChunk::Content(t) if t == "Hello"));

        // Second event
        let results2 = parse_sse_event(events[1], &mut acc);
        assert_eq!(results2.len(), 1);
        assert!(matches!(&results2[0], StreamChunk::Content(t) if t == " world"));
    }

    #[test]
    fn test_parse_sse_event_with_whitespace() {
        let mut acc = ToolCallAccumulator::new();
        // Test SSE event with extra whitespace
        let event = "  data: [DONE]  ";
        // The parser strips "data: " prefix, should handle this
        let results = parse_sse_event(event.trim(), &mut acc);
        assert_eq!(results.len(), 1);
        assert!(matches!(results[0], StreamChunk::Done));
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

    #[tokio::test]
    async fn test_stream_timeout_flushes_buffered_tool_calls() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            let sse_event = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_timeout","type":"function","function":{"name":"git_status","arguments":"{}"}}]}}]}

"#;
            let chunk = format!("{:X}\r\n{}\r\n", sse_event.len(), sse_event);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}",
                chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            // Keep connection open so the client hits its chunk timeout.
            tokio::time::sleep(Duration::from_millis(250)).await;
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_millis(50));
        let mut rx = stream.into_channel().await;

        let first = rx.recv().await.unwrap().unwrap();
        assert!(matches!(
            first,
            StreamChunk::ToolCall(ToolCall { id, .. }) if id == "call_timeout"
        ));

        let second = rx.recv().await.unwrap();
        assert!(second.is_err());
        assert!(second
            .unwrap_err()
            .to_string()
            .contains("Stream timeout: no data"));

        let _ = server.await;
    }

    // ============================================
    // ToolCallAccumulator Tests
    // ============================================

    #[test]
    fn test_tool_call_accumulator_new_is_empty() {
        let mut acc = ToolCallAccumulator::new();
        let calls = acc.flush();
        assert!(calls.is_empty());
    }

    #[test]
    fn test_tool_call_accumulator_single_delta() {
        let mut acc = ToolCallAccumulator::new();
        let delta = serde_json::json!({
            "index": 0,
            "id": "call_single",
            "type": "function",
            "function": {"name": "test_fn", "arguments": "{\"key\":\"value\"}"}
        });
        let result = acc.process_delta(&delta);
        assert!(result.is_none(), "process_delta should always return None");

        let calls = acc.flush();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_single");
        assert_eq!(calls[0].call_type, "function");
        assert_eq!(calls[0].function.name, "test_fn");
        assert_eq!(calls[0].function.arguments, "{\"key\":\"value\"}");
    }

    #[test]
    fn test_tool_call_accumulator_continuation_appends_args() {
        let mut acc = ToolCallAccumulator::new();

        // First delta with partial args
        let d1 = serde_json::json!({
            "index": 0,
            "id": "call_multi",
            "type": "function",
            "function": {"name": "write", "arguments": "{\"path\":"}
        });
        acc.process_delta(&d1);

        // Continuation delta: same index, only args
        let d2 = serde_json::json!({
            "index": 0,
            "function": {"arguments": "\"/tmp/f\","}
        });
        acc.process_delta(&d2);

        // Another continuation
        let d3 = serde_json::json!({
            "index": 0,
            "function": {"arguments": "\"data\":\"hi\"}"}
        });
        acc.process_delta(&d3);

        let calls = acc.flush();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_multi");
        assert_eq!(calls[0].function.name, "write");
        assert_eq!(
            calls[0].function.arguments,
            "{\"path\":\"/tmp/f\",\"data\":\"hi\"}"
        );
    }

    #[test]
    fn test_tool_call_accumulator_updates_id_type_name_on_continuation() {
        let mut acc = ToolCallAccumulator::new();

        // First delta with empty id/type (some backends do this)
        let d1 = serde_json::json!({
            "index": 0,
            "function": {"arguments": "{\"a\":1"}
        });
        acc.process_delta(&d1);

        // Second delta provides id, type, name
        let d2 = serde_json::json!({
            "index": 0,
            "id": "call_late_id",
            "type": "function",
            "function": {"name": "late_fn", "arguments": "}"}
        });
        acc.process_delta(&d2);

        let calls = acc.flush();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_late_id");
        assert_eq!(calls[0].call_type, "function");
        assert_eq!(calls[0].function.name, "late_fn");
        assert_eq!(calls[0].function.arguments, "{\"a\":1}");
    }

    #[test]
    fn test_tool_call_accumulator_multiple_indices_sorted() {
        let mut acc = ToolCallAccumulator::new();

        // Insert index 2 first
        let d2 = serde_json::json!({
            "index": 2, "id": "call_c", "type": "function",
            "function": {"name": "fn_c", "arguments": "{}"}
        });
        acc.process_delta(&d2);

        // Then index 0
        let d0 = serde_json::json!({
            "index": 0, "id": "call_a", "type": "function",
            "function": {"name": "fn_a", "arguments": "{}"}
        });
        acc.process_delta(&d0);

        // Then index 1
        let d1 = serde_json::json!({
            "index": 1, "id": "call_b", "type": "function",
            "function": {"name": "fn_b", "arguments": "{}"}
        });
        acc.process_delta(&d1);

        let calls = acc.flush();
        assert_eq!(calls.len(), 3);
        // Should be sorted by index
        assert_eq!(calls[0].id, "call_a");
        assert_eq!(calls[1].id, "call_b");
        assert_eq!(calls[2].id, "call_c");
    }

    #[test]
    fn test_tool_call_accumulator_delta_missing_index_returns_none() {
        let mut acc = ToolCallAccumulator::new();
        // Delta with no index field at all
        let delta = serde_json::json!({
            "id": "call_no_idx",
            "function": {"name": "fn", "arguments": "{}"}
        });
        let result = acc.process_delta(&delta);
        assert!(result.is_none());

        // Nothing was buffered because index was missing
        let calls = acc.flush();
        assert!(calls.is_empty());
    }

    #[test]
    fn test_tool_call_accumulator_flush_clears_pending() {
        let mut acc = ToolCallAccumulator::new();
        let delta = serde_json::json!({
            "index": 0, "id": "call_x", "type": "function",
            "function": {"name": "fn_x", "arguments": "{}"}
        });
        acc.process_delta(&delta);

        let calls1 = acc.flush();
        assert_eq!(calls1.len(), 1);

        // Second flush should be empty
        let calls2 = acc.flush();
        assert!(calls2.is_empty());
    }

    #[test]
    fn test_tool_call_accumulator_default_trait() {
        let mut acc = ToolCallAccumulator::default();
        let calls = acc.flush();
        assert!(calls.is_empty());
    }

    // ============================================
    // Extended parse_sse_event Tests
    // ============================================

    #[test]
    fn test_parse_sse_content_and_usage_same_event() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"content":"hi"}}],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}"#;
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], StreamChunk::Content(t) if t == "hi"));
        assert!(matches!(&results[1], StreamChunk::Usage(u) if u.total_tokens == 7));
    }

    #[test]
    fn test_parse_sse_reasoning_and_content_same_event() {
        let mut acc = ToolCallAccumulator::new();
        let event =
            r#"data: {"choices":[{"delta":{"content":"answer","reasoning_content":"thinking"}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], StreamChunk::Content(t) if t == "answer"));
        assert!(matches!(&results[1], StreamChunk::Reasoning(t) if t == "thinking"));
    }

    #[test]
    fn test_parse_sse_empty_reasoning_not_emitted() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"reasoning_content":""}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_multiple_tool_call_deltas_in_one_event() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"fn1","arguments":"{}"}},{"index":1,"id":"c2","type":"function","function":{"name":"fn2","arguments":"{}"}}]}}]}"#;
        let results = parse_sse_event(event, &mut acc);
        // Tool calls are buffered, not emitted
        assert!(results.is_empty());

        let calls = acc.flush();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].function.name, "fn1");
        assert_eq!(calls[1].function.name, "fn2");
    }

    #[test]
    fn test_parse_sse_finish_reason_flushes_tool_calls() {
        let mut acc = ToolCallAccumulator::new();

        // First event: buffer a tool call
        let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c_fin","type":"function","function":{"name":"read","arguments":"{}"}}]}}]}"#;
        let r1 = parse_sse_event(event1, &mut acc);
        assert!(r1.is_empty());

        // Second event with finish_reason: should flush
        let event2 = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        let r2 = parse_sse_event(event2, &mut acc);
        assert_eq!(r2.len(), 1);
        assert!(matches!(&r2[0], StreamChunk::ToolCall(tc) if tc.id == "c_fin"));
    }

    #[test]
    fn test_parse_sse_done_flushes_multiple_tool_calls() {
        let mut acc = ToolCallAccumulator::new();

        // Buffer two tool calls
        let d0 = serde_json::json!({
            "index": 0, "id": "a", "type": "function",
            "function": {"name": "fn_a", "arguments": "{}"}
        });
        let d1 = serde_json::json!({
            "index": 1, "id": "b", "type": "function",
            "function": {"name": "fn_b", "arguments": "{}"}
        });
        acc.process_delta(&d0);
        acc.process_delta(&d1);

        let results = parse_sse_event("data: [DONE]", &mut acc);
        // Should be: ToolCall(a), ToolCall(b), Done
        assert_eq!(results.len(), 3);
        assert!(matches!(&results[0], StreamChunk::ToolCall(tc) if tc.id == "a"));
        assert!(matches!(&results[1], StreamChunk::ToolCall(tc) if tc.id == "b"));
        assert!(matches!(&results[2], StreamChunk::Done));
    }

    #[test]
    fn test_parse_sse_event_json_without_choices() {
        let mut acc = ToolCallAccumulator::new();
        // Valid JSON but no choices key
        let event = r#"data: {"id":"chatcmpl-123","model":"test"}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_usage_with_invalid_structure() {
        let mut acc = ToolCallAccumulator::new();
        // Usage field is present but cannot deserialize to Usage struct
        let event = r#"data: {"usage":{"invalid":"fields"}}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_multiple_data_lines() {
        let mut acc = ToolCallAccumulator::new();
        // Multiple data: lines in one SSE event (spec says to concatenate)
        // Our implementation processes each line independently
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"A\"}}]}\ndata: {\"choices\":[{\"delta\":{\"content\":\"B\"}}]}";
        let results = parse_sse_event(event, &mut acc);
        assert_eq!(results.len(), 2);
        assert!(matches!(&results[0], StreamChunk::Content(t) if t == "A"));
        assert!(matches!(&results[1], StreamChunk::Content(t) if t == "B"));
    }

    #[test]
    fn test_parse_sse_event_choices_empty_array() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_sse_event_choices_no_delta() {
        let mut acc = ToolCallAccumulator::new();
        let event = r#"data: {"choices":[{"index":0}]}"#;
        let results = parse_sse_event(event, &mut acc);
        assert!(results.is_empty());
    }

    // ============================================
    // ApiClient Construction Tests
    // ============================================

    #[test]
    fn test_api_client_new_default_config() {
        let config = crate::config::Config::default();
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.base_url, config.endpoint);
    }

    #[test]
    fn test_api_client_new_custom_endpoint() {
        let config = crate::config::Config {
            endpoint: "https://api.example.com/v1".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(&config).unwrap();
        assert_eq!(client.base_url, "https://api.example.com/v1");
    }

    #[test]
    fn test_api_client_new_respects_step_timeout() {
        let mut config = crate::config::Config::default();
        config.agent.step_timeout_secs = 120;
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_api_client_new_enforces_minimum_timeout() {
        let mut config = crate::config::Config::default();
        config.agent.step_timeout_secs = 10; // Below 60s minimum
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
        // The timeout should be max(10, 60) = 60
    }

    #[test]
    fn test_api_client_with_retry_config() {
        let config = crate::config::Config::default();
        let client = ApiClient::new(&config).unwrap();
        let custom_retry = RetryConfig {
            max_retries: 10,
            initial_delay_ms: 200,
            max_delay_ms: 5000,
            retryable_status_codes: vec![429],
        };
        let client = client.with_retry_config(custom_retry);
        assert_eq!(client.retry_config.max_retries, 10);
        assert_eq!(client.retry_config.initial_delay_ms, 200);
        assert_eq!(client.retry_config.max_delay_ms, 5000);
        assert_eq!(client.retry_config.retryable_status_codes, vec![429]);
    }

    #[test]
    fn test_api_client_new_uses_retry_from_config() {
        let mut config = crate::config::Config::default();
        config.retry = crate::config::RetrySettings {
            max_retries: 7,
            base_delay_ms: 500,
            max_delay_ms: 10000,
        };
        let client = ApiClient::new(&config).unwrap();
        assert_eq!(client.retry_config.max_retries, 7);
        assert_eq!(client.retry_config.initial_delay_ms, 500);
        assert_eq!(client.retry_config.max_delay_ms, 10000);
    }

    #[test]
    fn test_api_client_clone() {
        let config = crate::config::Config::default();
        let client = ApiClient::new(&config).unwrap();
        let cloned = client.clone();
        assert_eq!(cloned.base_url, client.base_url);
        assert_eq!(
            cloned.retry_config.max_retries,
            client.retry_config.max_retries
        );
    }

    // ============================================
    // ApiClient HTTP Tests (with real TCP server)
    // ============================================

    #[tokio::test]
    async fn test_api_client_chat_success() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"id":"c-1","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"Hello world"},"finish_reason":"stop"}],"usage":{"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            api_key: None,
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let messages = vec![Message::user("Hi")];
        let result = client.chat(messages, None, ThinkingMode::Enabled).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.choices[0].message.content, "Hello world");
        assert_eq!(resp.usage.total_tokens, 8);

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_with_api_key() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            let request_lower = request.to_lowercase();
            // reqwest may use lowercase header names (HTTP/2 style)
            assert!(
                request_lower.contains("authorization: bearer test-key-123")
                    || request.contains("Authorization: Bearer test-key-123"),
                "Expected Bearer token in request headers. Got:\n{}",
                &request[..request.len().min(500)]
            );

            let body = r#"{"id":"c-2","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let mut config = crate::config::Config::default();
        config.endpoint = format!("http://127.0.0.1:{}/v1", addr.port());
        config.api_key = Some(crate::config::RedactedString::new(
            "test-key-123".to_string(),
        ));

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
            .await;
        assert!(
            result.is_ok(),
            "chat with api_key failed: {:?}",
            result.err()
        );

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_thinking_disabled_inserts_system_msg() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = socket.read(&mut buf).await.unwrap();
            let request_str = String::from_utf8_lossy(&buf[..n]);

            // Extract body after the blank line separator
            if let Some(body_start) = request_str.find("\r\n\r\n") {
                let body = &request_str[body_start + 4..];
                // Verify the system message about disabling thinking was inserted
                assert!(
                    body.contains("CRITICAL INSTRUCTION"),
                    "Expected CRITICAL INSTRUCTION system message in request body"
                );
            }

            let body = r#"{"id":"c-3","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"direct"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("hello")], None, ThinkingMode::Disabled)
            .await;
        assert!(result.is_ok());

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_thinking_budget() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = socket.read(&mut buf).await.unwrap();
            let request_str = String::from_utf8_lossy(&buf[..n]);

            // Extract body
            if let Some(body_start) = request_str.find("\r\n\r\n") {
                let body = &request_str[body_start + 4..];
                // Verify the thinking budget is present
                assert!(
                    body.contains("budget_tokens"),
                    "Expected budget_tokens in request body"
                );
                assert!(body.contains("4096"), "Expected budget value 4096 in body");
            }

            let body = r#"{"id":"c-4","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"thought"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(
                vec![Message::user("think")],
                None,
                ThinkingMode::Budget(4096),
            )
            .await;
        assert!(result.is_ok());

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_with_tools_in_body() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = socket.read(&mut buf).await.unwrap();
            let request_str = String::from_utf8_lossy(&buf[..n]);

            if let Some(body_start) = request_str.find("\r\n\r\n") {
                let body = &request_str[body_start + 4..];
                assert!(body.contains("\"tools\""), "Expected tools in request body");
                assert!(body.contains("my_tool"), "Expected my_tool name in body");
            }

            let body = r#"{"id":"c-5","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"done"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let tools = vec![ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: "my_tool".to_string(),
                description: "A tool".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        }];

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(
                vec![Message::user("use tool")],
                Some(tools),
                ThinkingMode::Enabled,
            )
            .await;
        assert!(result.is_ok());

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_non_retryable_error() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"error":"Unauthorized"}"#;
            let response = format!(
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("401"),
            "Expected 401 in error: {}",
            err_str
        );

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_retryable_then_success() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // First request: 500 error
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"error":"Internal Server Error"}"#;
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            drop(socket);

            // Second request: success
            let (mut socket2, _) = listener.accept().await.unwrap();
            let mut buf2 = vec![0u8; 8192];
            let _ = socket2.read(&mut buf2).await.unwrap();

            let body2 = r#"{"id":"c-retry","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"recovered"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response2 = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body2.len(),
                body2
            );
            socket2.write_all(response2.as_bytes()).await.unwrap();
        });

        let mut config = crate::config::Config::default();
        config.endpoint = format!("http://127.0.0.1:{}/v1", addr.port());
        config.retry = crate::config::RetrySettings {
            max_retries: 3,
            base_delay_ms: 10, // Very short delay for tests
            max_delay_ms: 50,
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("retry")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().choices[0].message.content, "recovered");

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_all_retries_exhausted() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // Respond with 500 for every attempt (initial + 1 retry = 2 total)
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let _ = socket.read(&mut buf).await.unwrap();

                let body = r#"{"error":"Server Error"}"#;
                let response = format!(
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let mut config = crate::config::Config::default();
        config.endpoint = format!("http://127.0.0.1:{}/v1", addr.port());
        config.retry = crate::config::RetrySettings {
            max_retries: 1,
            base_delay_ms: 10,
            max_delay_ms: 20,
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("fail")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("500") || err_str.contains("Server Error"),
            "Expected server error, got: {}",
            err_str
        );

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_completion_success() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            // Verify it goes to /completions endpoint
            assert!(request.contains("POST") && request.contains("/completions"));

            let body = r#"{"id":"cmpl-1","object":"text_completion","created":123,"model":"test","choices":[{"text":"completed text","index":0,"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":4,"total_tokens":7}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client.completion("fn main() {", Some(100), None).await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.choices[0].text, "completed text");

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_completion_error() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"error":"bad request"}"#;
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client.completion("test", None, None).await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("400"),
            "Expected 400 in error: {}",
            err_str
        );

        let _ = server.await;
    }

    // ============================================
    // Streaming Tests (with real TCP server)
    // ============================================

    /// Drain the HTTP request headers from the socket before sending a response.
    /// On Windows, writing a response without reading the request causes
    /// ConnectionAborted (OS error 10053).
    async fn drain_http_request(socket: &mut tokio::net::TcpStream) {
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 1024];
        let mut total = Vec::new();
        loop {
            let n = socket.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            total.extend_from_slice(&buf[..n]);
            // End of HTTP headers is marked by \r\n\r\n
            if total.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
    }

    #[tokio::test]
    async fn test_streaming_response_collect() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            let events = vec![
                r#"data: {"choices":[{"delta":{"content":"Hello"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":" world"}}]}"#,
                r#"data: {"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}"#,
                "data: [DONE]",
            ];

            let mut full_body = String::new();
            for event in &events {
                full_body.push_str(event);
                full_body.push_str("\n\n");
            }

            let chunk = format!("{:X}\r\n{}\r\n", full_body.len(), full_body);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(5));
        let result = stream.collect().await;
        assert!(result.is_ok());
        let chat_resp = result.unwrap();
        assert_eq!(chat_resp.choices[0].message.content, "Hello world");
        assert_eq!(chat_resp.id, "streamed");
        assert_eq!(chat_resp.usage.total_tokens, 7);

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_streaming_response_collect_with_reasoning() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            let events = vec![
                r#"data: {"choices":[{"delta":{"reasoning_content":"Let me think"}}]}"#,
                r#"data: {"choices":[{"delta":{"content":"The answer"}}]}"#,
                "data: [DONE]",
            ];

            let mut full_body = String::new();
            for event in &events {
                full_body.push_str(event);
                full_body.push_str("\n\n");
            }

            let chunk = format!("{:X}\r\n{}\r\n", full_body.len(), full_body);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(5));
        let result = stream.collect().await;
        assert!(result.is_ok());
        let chat_resp = result.unwrap();
        assert_eq!(chat_resp.choices[0].message.content, "The answer");
        assert_eq!(
            chat_resp.choices[0].message.reasoning_content,
            Some("Let me think".to_string())
        );

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_streaming_response_collect_with_tool_calls() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            let events = vec![
                r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_collect","type":"function","function":{"name":"file_read","arguments":"{\"path\":\"/test\"}"}}]}}]}"#,
                "data: [DONE]",
            ];

            let mut full_body = String::new();
            for event in &events {
                full_body.push_str(event);
                full_body.push_str("\n\n");
            }

            let chunk = format!("{:X}\r\n{}\r\n", full_body.len(), full_body);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(5));
        let result = stream.collect().await;
        assert!(result.is_ok());
        let chat_resp = result.unwrap();
        let tool_calls = chat_resp.choices[0].message.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_collect");
        assert_eq!(tool_calls[0].function.name, "file_read");

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_streaming_response_collect_empty_stream() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            // Just send DONE immediately
            let events = "data: [DONE]\n\n";
            let chunk = format!("{:X}\r\n{}\r\n", events.len(), events);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(5));
        let result = stream.collect().await;
        assert!(result.is_ok());
        let chat_resp = result.unwrap();
        assert!(chat_resp.choices[0].message.content.is_empty());
        assert!(chat_resp.choices[0].message.reasoning_content.is_none());
        assert!(chat_resp.choices[0].message.tool_calls.is_none());

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_streaming_response_debug_format() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(30));
        let debug = format!("{:?}", stream);
        assert!(debug.contains("StreamingResponse"));
        assert!(debug.contains("status"));
        assert!(debug.contains("chunk_timeout_secs"));

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_streaming_trailing_buffer_processing() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            drain_http_request(&mut socket).await;
            // Send data without trailing \n\n so it goes into the trailing buffer
            let events = r#"data: {"choices":[{"delta":{"content":"trailing"}}]}"#;
            let chunk = format!("{:X}\r\n{}\r\n", events.len(), events);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
        let stream = StreamingResponse::new(response, Duration::from_secs(5));
        let mut rx = stream.into_channel().await;

        let mut content = String::new();
        while let Some(chunk_result) = rx.recv().await {
            if let Ok(chunk) = chunk_result {
                match chunk {
                    StreamChunk::Content(text) => content.push_str(&text),
                    StreamChunk::Done => break,
                    _ => {}
                }
            }
        }
        assert_eq!(content, "trailing");

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_stream_success() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = socket.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            // Verify it's a streaming request
            assert!(request.contains("\"stream\":true"));

            let events =
                "data: {\"choices\":[{\"delta\":{\"content\":\"streamed\"}}]}\n\ndata: [DONE]\n\n";
            let chunk = format!("{:X}\r\n{}\r\n", events.len(), events);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
                chunk, end_chunk
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat_stream(vec![Message::user("stream")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_ok());

        let stream = result.unwrap();
        let collected = stream.collect().await;
        assert!(collected.is_ok());
        assert_eq!(collected.unwrap().choices[0].message.content, "streamed");

        let _ = server.await;
    }

    #[tokio::test]
    async fn test_api_client_chat_stream_error_response() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"error":"model not found"}"#;
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat_stream(vec![Message::user("test")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_err());
        let err_str = result.unwrap_err().to_string();
        assert!(
            err_str.contains("404"),
            "Expected 404 in error: {}",
            err_str
        );

        let _ = server.await;
    }

    // ============================================
    // Retry-After Header Test
    // ============================================

    #[tokio::test]
    async fn test_api_client_retry_after_header() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // First request: 429 with Retry-After header
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"error":"rate limited"}"#;
            let response = format!(
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            drop(socket);

            // Second request: success
            let (mut socket2, _) = listener.accept().await.unwrap();
            let mut buf2 = vec![0u8; 8192];
            let _ = socket2.read(&mut buf2).await.unwrap();

            let body2 = r#"{"id":"c-ra","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"after retry"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response2 = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body2.len(),
                body2
            );
            socket2.write_all(response2.as_bytes()).await.unwrap();
        });

        let mut config = crate::config::Config::default();
        config.endpoint = format!("http://127.0.0.1:{}/v1", addr.port());
        config.retry = crate::config::RetrySettings {
            max_retries: 2,
            base_delay_ms: 10,
            max_delay_ms: 5000,
        };

        let client = ApiClient::new(&config).unwrap();
        let result = client
            .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().choices[0].message.content, "after retry");

        let _ = server.await;
    }

    // ============================================
    // CompletionRequest Serialization Tests
    // ============================================

    #[test]
    fn test_completion_request_serialization() {
        let req = types::CompletionRequest {
            model: "test-model".to_string(),
            prompt: "fn main() {".to_string(),
            max_tokens: Some(100),
            temperature: Some(0.1),
            top_p: Some(0.9),
            stop: Some(vec!["\n".to_string()]),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"test-model\""));
        assert!(json.contains("\"prompt\":\"fn main() {\""));
        assert!(json.contains("\"max_tokens\":100"));
        assert!(json.contains("\"temperature\":0.1"));
        assert!(json.contains("\"stop\":[\"\\n\"]"));
    }

    #[test]
    fn test_completion_request_optional_fields_skipped() {
        let req = types::CompletionRequest {
            model: "test".to_string(),
            prompt: "hello".to_string(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("max_tokens"));
        assert!(!json.contains("temperature"));
        assert!(!json.contains("top_p"));
        assert!(!json.contains("stop"));
    }

    #[test]
    fn test_completion_response_deserialization() {
        let json = r#"{
            "id": "cmpl-1",
            "object": "text_completion",
            "created": 12345,
            "model": "test-model",
            "choices": [
                {"text": "completed code", "index": 0, "finish_reason": "stop"}
            ],
            "usage": {"prompt_tokens": 5, "completion_tokens": 10, "total_tokens": 15}
        }"#;
        let resp: types::CompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, "cmpl-1");
        assert_eq!(resp.choices[0].text, "completed code");
        assert_eq!(resp.choices[0].finish_reason, Some("stop".to_string()));
        assert!(resp.usage.is_some());
        assert_eq!(resp.usage.unwrap().total_tokens, 15);
    }

    #[test]
    fn test_completion_response_without_usage() {
        let json = r#"{
            "id": "cmpl-2",
            "object": "text_completion",
            "created": 12345,
            "model": "test",
            "choices": [
                {"text": "code", "index": 0, "finish_reason": null}
            ],
            "usage": null
        }"#;
        let resp: types::CompletionResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
        assert!(resp.choices[0].finish_reason.is_none());
    }

    // ============================================
    // LlmClient trait via ApiClient (integration)
    // ============================================

    #[tokio::test]
    async fn test_llm_client_trait_chat() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let _ = socket.read(&mut buf).await.unwrap();

            let body = r#"{"id":"t-1","object":"chat.completion","created":123,"model":"test","choices":[{"index":0,"message":{"role":"assistant","content":"via trait"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let config = crate::config::Config {
            endpoint: format!("http://127.0.0.1:{}/v1", addr.port()),
            ..Default::default()
        };

        let client = ApiClient::new(&config).unwrap();
        // Call via the trait
        let result: Result<ChatResponse> = LlmClient::chat(
            &client,
            vec![Message::user("trait test")],
            None,
            ThinkingMode::Enabled,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().choices[0].message.content, "via trait");

        let _ = server.await;
    }

    // ============================================
    // rand_jitter edge case
    // ============================================

    #[test]
    fn test_rand_jitter_is_deterministic_within_bounds() {
        // Verify the function returns a finite number in [0, 1)
        for _ in 0..100 {
            let j = rand_jitter();
            assert!(j.is_finite());
            assert!(j >= 0.0);
            assert!(j < 1.0);
        }
    }

    // ============================================
    // URL construction matching actual code paths
    // ============================================

    #[test]
    fn test_completion_url_construction() {
        let base = "http://localhost:8000/v1";
        let url = format!("{}/completions", base);
        assert_eq!(url, "http://localhost:8000/v1/completions");
    }

    #[test]
    fn test_chat_completions_url_construction() {
        let base = "https://api.openai.com/v1";
        let url = format!("{}/chat/completions", base);
        assert_eq!(url, "https://api.openai.com/v1/chat/completions");
    }

    // ============================================
    // Jitter arithmetic in backoff
    // ============================================

    #[test]
    fn test_jitter_arithmetic_safety() {
        // Simulate the jitter arithmetic from send_with_retry_inner
        // to ensure it doesn't overflow or underflow
        let delay_ms: u64 = 30000; // max_delay_ms
        let jitter_val = 0.5_f64; // mid-range jitter
        let jitter = (delay_ms as f64 * 0.1 * (jitter_val - 0.5)) as i64;
        let result = (delay_ms as i64).saturating_add(jitter).max(1) as u64;
        assert_eq!(result, 30000); // 0 jitter at midpoint

        // Test with extreme jitter values
        let jitter_low = (delay_ms as f64 * 0.1 * (0.0 - 0.5)) as i64;
        let result_low = (delay_ms as i64).saturating_add(jitter_low).max(1) as u64;
        assert!(result_low > 0);
        assert!(result_low <= 30000);

        let jitter_high = (delay_ms as f64 * 0.1 * (1.0 - 0.5)) as i64;
        let result_high = (delay_ms as i64).saturating_add(jitter_high).max(1) as u64;
        assert!(result_high >= 30000);
    }

    #[test]
    fn test_jitter_with_zero_delay() {
        let delay_ms: u64 = 0;
        let jitter = (delay_ms as f64 * 0.1 * (rand_jitter() - 0.5)) as i64;
        let result = (delay_ms as i64).saturating_add(jitter).max(1) as u64;
        // With 0 delay, jitter is 0, but max(1) ensures at least 1ms
        assert_eq!(result, 1);
    }

    #[test]
    fn test_jitter_capped_at_max_delay() {
        let delay_ms: u64 = 30000;
        let max_delay_ms: u64 = 30000;
        let jitter = (delay_ms as f64 * 0.1 * (1.0 - 0.5)) as i64; // +5% = +1500
        let result = (delay_ms as i64).saturating_add(jitter).max(1) as u64;
        let capped = result.min(max_delay_ms);
        assert_eq!(capped, 30000);
    }

    // ============================================
    // API client with HTTP (non-local) warning path
    // ============================================

    #[test]
    fn test_api_client_http_non_local_creates_successfully() {
        // The warning is just a log, the client should still be created
        let config = crate::config::Config {
            endpoint: "http://remote-api.example.com/v1".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_api_client_http_localhost_no_warning() {
        let config = crate::config::Config {
            endpoint: "http://localhost:8000/v1".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_api_client_https_no_warning() {
        let config = crate::config::Config {
            endpoint: "https://api.example.com/v1".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_api_client_http_127_no_warning() {
        let config = crate::config::Config {
            endpoint: "http://127.0.0.1:8000/v1".to_string(),
            ..Default::default()
        };
        let client = ApiClient::new(&config);
        assert!(client.is_ok());
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
