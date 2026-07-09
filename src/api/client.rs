//! HTTP client for OpenAI-compatible chat completion APIs.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::errors::ApiError;
use crate::supervision::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError,
};
use crate::tokens::{estimate_messages_tokens, estimate_tool_definitions_tokens};

/// Helper: token estimate for an optional tool list, returning 0 when `None`.
fn estimate_tool_definitions_tokens_opt(tools: Option<&Vec<ToolDefinition>>) -> usize {
    tools
        .map(|t| estimate_tool_definitions_tokens(t))
        .unwrap_or(0)
}

use super::streaming::StreamingResponse;
use super::types::*;
use super::{
    attach_tools_to_body, canonicalize_message_order, maybe_prepend_disabled_thinking_instruction,
    merge_extra_body, LlmClient, ThinkingMode,
};

/// Print a request body to stderr when the configured debug `requests`
/// channel is active (CLI `--debug=requests`, `[debug] log_requests = true`,
/// or the legacy `SELFWARE_DEBUG_REQUEST` env var).
///
/// The body is sanitized of obvious credential locations before printing —
/// defence in depth, since the JSON request body should never carry an API
/// key in normal operation (creds live on headers).
fn maybe_log_request_body(
    debug: &crate::config::DebugConfig,
    body: &serde_json::Value,
    label: &str,
) {
    if !debug.should_log_requests() {
        return;
    }
    let mut sanitized = body.clone();
    crate::agent::turn_artifacts::sanitize_request_body(&mut sanitized);
    match serde_json::to_string_pretty(&sanitized) {
        Ok(s) => eprintln!("=== SELFWARE_DEBUG_REQUEST ({label}) ===\n{s}\n=== END REQUEST ==="),
        Err(e) => eprintln!("SELFWARE_DEBUG_REQUEST: failed to format body: {e}"),
    }
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
    pub(crate) base_url: String,
    pub(crate) retry_config: RetryConfig,
    circuit_breaker: Arc<CircuitBreaker>,
    /// Progress emitter for `LlmRequestSent` / `LlmResponseReceived` events.
    /// Defaults to a no-op; the agent installs a real emitter via
    /// [`Self::with_progress_emitter`] so the same observability pipe used by
    /// step / tool events also covers the LLM round trip.
    progress_emitter: Arc<dyn crate::agent::progress::ProgressEmitter>,
}

impl ApiClient {
    pub fn new(config: &crate::config::Config) -> Result<Self> {
        let request_timeout = config.agent.step_timeout_secs.max(60);
        let client = Client::builder()
            .timeout(Duration::from_secs(request_timeout))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

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
            progress_emitter: Arc::new(crate::agent::progress::NoopProgressEmitter),
        })
    }

    /// Return a reference to the underlying [`Config`](crate::config::Config).
    pub fn config(&self) -> &crate::config::Config {
        &self.config
    }

    pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }

    /// Install a [`ProgressEmitter`](crate::agent::progress::ProgressEmitter)
    /// for `LlmRequestSent` / `LlmResponseReceived` events.  The agent calls
    /// this so HTTP round-trips show up in the same structured stream as
    /// step / tool / guard events.
    pub fn with_progress_emitter(
        &mut self,
        emitter: Arc<dyn crate::agent::progress::ProgressEmitter>,
    ) {
        self.progress_emitter = emitter;
    }

    pub async fn completion(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
        stop: Option<Vec<String>>,
    ) -> Result<CompletionResponse> {
        self.circuit_breaker
            .call(|| self.completion_inner(prompt, max_tokens, stop.clone()))
            .await
            .map_err(|e| match e {
                CircuitBreakerError::CircuitOpen => {
                    ApiError::Network("Circuit breaker is open - API is unavailable".to_string())
                        .into()
                }
                CircuitBreakerError::OperationFailed(err) => err,
            })
    }

    async fn completion_inner(
        &self,
        prompt: &str,
        max_tokens: Option<usize>,
        stop: Option<Vec<String>>,
    ) -> Result<CompletionResponse> {
        let url = format!("{}/completions", self.base_url);

        let req = CompletionRequest {
            model: self.config.model.clone(),
            prompt: prompt.to_string(),
            max_tokens,
            temperature: Some(0.1),
            top_p: Some(0.9),
            stop,
        };

        // Retry transient failures (429/5xx/network) with exponential backoff,
        // matching the chat/stream paths. The FIM completion path previously gave
        // up on the first transient error.
        let max_attempts = self.retry_config.max_retries + 1;
        let mut delay_ms = self.retry_config.initial_delay_ms;
        for attempt in 1..=max_attempts {
            let mut request = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");
            if let Some(ref key) = self.config.api_key {
                request = request.header("Authorization", format!("Bearer {}", key.expose()));
            }

            match request.json(&req).send().await {
                Ok(response) if response.status().is_success() => {
                    let resp: CompletionResponse = response.json().await?;
                    return Ok(resp);
                }
                Ok(response) => {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    if Self::is_retryable_status(status) && attempt < max_attempts {
                        warn!(
                            "completion retryable error {} (attempt {}/{}); retrying after {}ms",
                            status, attempt, max_attempts, delay_ms
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                        continue;
                    }
                    return Err(ApiError::HttpStatus {
                        status: status.as_u16(),
                        message: text,
                    }
                    .into());
                }
                Err(e) => {
                    if attempt < max_attempts {
                        warn!(
                            "completion network error {} (attempt {}/{}); retrying after {}ms",
                            e, attempt, max_attempts, delay_ms
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                        continue;
                    }
                    return Err(e.into());
                }
            }
        }
        Err(ApiError::Network("completion: retries exhausted".to_string()).into())
    }

    pub async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<ChatResponse> {
        let (resp, _meta) = self.chat_with_meta(messages, tools, thinking).await?;
        Ok(resp)
    }

    /// Like [`Self::chat`], but also returns the exact request body that was sent
    /// and HTTP-layer timing.  Used by the per-turn debug capture.
    pub async fn chat_with_meta(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<(ChatResponse, ChatMetadata)> {
        // Compute the prompt-token estimate before `messages` is moved into
        // `build_chat_body` so the progress event reports the actual outgoing
        // size (messages + tool definitions).
        let estimated_tokens = estimate_messages_tokens(&messages)
            + estimate_tool_definitions_tokens_opt(tools.as_ref());
        let body = self.build_chat_body(messages, tools, thinking, false)?;
        maybe_log_request_body(&self.config.debug, &body, "chat");

        self.progress_emitter
            .emit(crate::agent::progress::ProgressEvent::LlmRequestSent {
                tokens: estimated_tokens,
            });

        let started = std::time::Instant::now();
        let resp = self.send_with_retry(&body).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let finish_reason = resp.choices.first().and_then(|c| c.finish_reason.clone());

        self.progress_emitter
            .emit(crate::agent::progress::ProgressEvent::LlmResponseReceived {
                finish_reason: finish_reason.clone().unwrap_or_else(|| "unknown".into()),
                completion_tokens: resp.usage.completion_tokens as u32,
            });

        let meta = ChatMetadata {
            request_body: body,
            elapsed_ms,
            finish_reason,
            prompt_tokens: Some(resp.usage.prompt_tokens as u32),
            completion_tokens: Some(resp.usage.completion_tokens as u32),
            total_tokens: Some(resp.usage.total_tokens as u32),
        };
        Ok((resp, meta))
    }

    /// Build a non-streaming or streaming chat-completion request body.
    ///
    /// Encapsulates message normalization, context-budget enforcement, tool
    /// attachment, thinking-mode injection and `extra_body` merging.  The
    /// returned `serde_json::Value` is exactly what would be POSTed to the
    /// backend (sans HTTP headers — credentials live there, not in the body).
    fn build_chat_body(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
        stream: bool,
    ) -> Result<serde_json::Value> {
        let mut messages = messages;
        maybe_prepend_disabled_thinking_instruction(&mut messages, &thinking);
        canonicalize_message_order(&mut messages);

        let message_tokens = estimate_messages_tokens(&messages);
        let tool_tokens = tools
            .as_ref()
            .map(|t| estimate_tool_definitions_tokens(t))
            .unwrap_or(0);
        let input_tokens = message_tokens + tool_tokens;

        let hard_limit = self.config.context_length;
        let min_output = 512_usize;
        if input_tokens + min_output > hard_limit {
            let msg = format!(
                "input_tokens ({}) + min_output ({}) > context_length ({}). \
                 Messages: {} tokens, Tools: {} tokens. Context trimming failed to stay within limits.",
                input_tokens, min_output, hard_limit, message_tokens, tool_tokens
            );
            tracing::error!("CONTEXT OVERFLOW: {}", msg);
            return Err(ApiError::ContextOverflow(msg).into());
        }

        let available_for_output = hard_limit.saturating_sub(input_tokens);
        let max_tokens = self
            .config
            .max_tokens
            .min(available_for_output.max(min_output));

        let mut body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": self.config.temperature,
            "max_tokens": max_tokens,
            "stream": stream,
        });

        attach_tools_to_body(&mut body, &tools, self.config.agent.native_function_calling);

        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }

        merge_extra_body(
            &mut body,
            self.config.extra_body.as_ref(),
            if stream {
                "streaming chat request"
            } else {
                "default chat request"
            },
        )?;

        Ok(body)
    }

    pub async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<StreamingResponse> {
        let (stream, _meta) = self
            .chat_stream_with_meta(messages, tools, thinking)
            .await?;
        Ok(stream)
    }

    /// Like [`Self::chat_stream`], but also returns the exact request body that was
    /// sent.  Used by the per-turn debug capture; the caller is responsible
    /// for filling in `finish_reason` / token usage from the SSE stream.
    pub async fn chat_stream_with_meta(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<(StreamingResponse, ChatMetadata)> {
        let estimated_tokens = estimate_messages_tokens(&messages)
            + estimate_tool_definitions_tokens_opt(tools.as_ref());
        let body = self.build_chat_body(messages, tools, thinking, true)?;
        maybe_log_request_body(&self.config.debug, &body, "chat_stream");

        self.progress_emitter
            .emit(crate::agent::progress::ProgressEvent::LlmRequestSent {
                tokens: estimated_tokens,
            });

        let started = std::time::Instant::now();
        let body_for_meta = body.clone();
        let stream = self
            .circuit_breaker
            .call(|| self.chat_stream_send(body.clone()))
            .await
            .map_err(|e| -> anyhow::Error {
                match e {
                    CircuitBreakerError::CircuitOpen => ApiError::Network(
                        "Circuit breaker is open - API is unavailable".to_string(),
                    )
                    .into(),
                    CircuitBreakerError::OperationFailed(err) => err,
                }
            })?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        let meta = ChatMetadata {
            request_body: body_for_meta,
            elapsed_ms,
            finish_reason: None,
            prompt_tokens: None,
            completion_tokens: None,
            total_tokens: None,
        };
        Ok((stream, meta))
    }

    async fn chat_stream_send(&self, body: serde_json::Value) -> Result<StreamingResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        debug!("Starting streaming request to {}", url);

        let mut delay_ms = self.retry_config.initial_delay_ms;
        let max_attempts = self.retry_config.max_retries + 1;

        for attempt in 1..=max_attempts {
            let mut request = self
                .client
                .post(&url)
                .header("Content-Type", "application/json");

            if let Some(ref key) = self.config.api_key {
                request = request.header("Authorization", format!("Bearer {}", key.expose()));
            }

            match request.json(&body).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let stream_chunk_timeout_secs = self.config.agent.step_timeout_secs.max(30);
                        return Ok(StreamingResponse::new(
                            response,
                            Duration::from_secs(stream_chunk_timeout_secs),
                        ));
                    }

                    let retry_after = Self::parse_retry_after(response.headers());
                    let text = response.text().await.unwrap_or_default();
                    if Self::is_retryable_status(status) && attempt < max_attempts {
                        let sleep_ms = self.retry_sleep_ms(delay_ms, retry_after);
                        warn!(
                            "Streaming request retryable error {} (attempt {}/{}); retrying after {}ms{}",
                            status, attempt, max_attempts, sleep_ms,
                            if retry_after.is_some() { " (server Retry-After)" } else { " (jittered)" }
                        );
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                        continue;
                    }
                    return Err(ApiError::HttpStatus {
                        status: status.as_u16(),
                        message: text,
                    }
                    .into());
                }
                Err(e) => {
                    if attempt < max_attempts {
                        let sleep_ms = self.retry_sleep_ms(delay_ms, None);
                        warn!(
                            "Streaming request network error: {} (attempt {}/{}); retrying after {}ms (jittered)",
                            e, attempt, max_attempts, sleep_ms
                        );
                        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                        delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                        continue;
                    }
                    return Err(ApiError::Network(format!(
                        "Failed to send streaming request after {} attempts: {}",
                        attempt, e
                    ))
                    .into());
                }
            }
        }

        // Unreachable because the loop always returns, but keeps the compiler happy.
        Err(ApiError::Network("Streaming request exhausted retries".to_string()).into())
    }

    fn is_retryable_status(status: reqwest::StatusCode) -> bool {
        status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    }

    /// Parse a `Retry-After` response header (delta-seconds form) into seconds.
    fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
        headers
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    /// Backoff sleep for a retry: honor the server's `Retry-After` (capped at
    /// max_delay) when present, otherwise the current exponential `delay_ms` with
    /// ±25% jitter so many clients don't retry in lockstep against a rate-limited
    /// gateway (thundering herd).
    fn retry_sleep_ms(&self, delay_ms: u64, retry_after_secs: Option<u64>) -> u64 {
        if let Some(secs) = retry_after_secs {
            return secs.saturating_mul(1000).min(self.retry_config.max_delay_ms);
        }
        let jitter = rand::random::<f64>() * 0.5 - 0.25; // [-0.25, +0.25]
        (((delay_ms as f64) * (1.0 + jitter)).max(0.0) as u64)
            .min(self.retry_config.max_delay_ms)
    }

    async fn send_with_retry(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        self.circuit_breaker
            .call(|| self.send_with_retry_inner(body))
            .await
            .map_err(|e| match e {
                CircuitBreakerError::CircuitOpen => {
                    ApiError::Network("Circuit breaker is open - API is unavailable".to_string())
                        .into()
                }
                CircuitBreakerError::OperationFailed(err) => err,
            })
    }

    async fn send_with_retry_inner(&self, body: &serde_json::Value) -> Result<ChatResponse> {
        self.send_request_with_retry(body, &self.base_url, self.config.api_key.as_ref())
            .await
    }

    async fn send_request_with_retry(
        &self,
        body: &serde_json::Value,
        endpoint: &str,
        api_key: Option<&crate::config::RedactedString>,
    ) -> Result<ChatResponse> {
        let url = format!("{}/chat/completions", endpoint);
        let mut last_error: Option<anyhow::Error> = None;
        let mut delay_ms = self.retry_config.initial_delay_ms;
        let mut honored_retry_after = false;

        for attempt in 0..=self.retry_config.max_retries {
            if attempt > 0 {
                warn!(
                    "Retry attempt {}/{} after {}ms delay",
                    attempt, self.retry_config.max_retries, delay_ms
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                // If the server explicitly told us how long to wait, do not
                // also apply exponential backoff doubling for this iteration.
                if honored_retry_after {
                    honored_retry_after = false;
                } else {
                    delay_ms = (delay_ms * 2).min(self.retry_config.max_delay_ms);
                    let jitter = (delay_ms as f64 * 0.1 * (rand_jitter() - 0.5)) as i64;
                    delay_ms = (delay_ms as i64).saturating_add(jitter).max(1) as u64;
                    delay_ms = delay_ms.min(self.retry_config.max_delay_ms);
                }
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
                        let body_text = response
                            .text()
                            .await
                            .context("Failed to read response body")?;

                        debug!("API response body ({} chars)", body_text.len());
                        if self.config.debug.should_log_responses() {
                            // Some OpenAI-compatible gateways (OpenRouter
                            // included) echo the offending API key back in
                            // error/response bodies -- redact before
                            // printing, same as maybe_log_request_body does
                            // for the request side.
                            let redacted = crate::observability::telemetry::redact_secrets(&body_text);
                            eprintln!("=== RAW API RESPONSE ===\n{}\n=== END RAW ===", redacted);
                        }

                        let chat_response: ChatResponse = serde_json::from_str(&body_text)
                            .context("Failed to parse response JSON")?;
                        if let Err(e) = chat_response.usage.validate() {
                            warn!(
                                "API returned inconsistent token usage: {}. Using response anyway.",
                                e
                            );
                        }
                        return Ok(chat_response);
                    }

                    if self
                        .retry_config
                        .retryable_status_codes
                        .contains(&status.as_u16())
                    {
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

                        // Honour Retry-After but never exceed the configured max_delay_ms.
                        if let Some(retry_secs) = retry_after_secs {
                            let retry_ms = retry_secs * 1000;
                            delay_ms = retry_ms.min(self.retry_config.max_delay_ms);
                            honored_retry_after = true;
                        }
                        continue;
                    }

                    let status_code = status.as_u16();
                    let error_text = response.text().await.unwrap_or_default();
                    return Err(ApiError::HttpStatus {
                        status: status_code,
                        message: error_text,
                    }
                    .into());
                }
                Err(e) => {
                    if e.is_timeout() || e.is_connect() {
                        warn!("Network error (retrying): {}", e);
                        last_error = Some(ApiError::Network(e.to_string()).into());
                        continue;
                    }
                    return Err(ApiError::Network(e.to_string()).into());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ApiError::Network("Request failed after all retries".to_string()).into()
        }))
    }

    /// Send a chat completion to an alternate model described by a `ModelProfile`.
    ///
    /// Applies the same message normalization and context budgeting as the main
    /// `chat()` path so profile-based calls cannot silently diverge.
    pub async fn chat_with_profile(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        thinking: ThinkingMode,
        profile: &crate::config::ModelProfile,
    ) -> Result<ChatResponse> {
        let mut messages: Vec<Message> = if !profile.supports_vision() {
            messages.iter().map(|m| m.strip_images()).collect()
        } else {
            messages
        };

        // Apply the same message normalization as the main chat path.
        maybe_prepend_disabled_thinking_instruction(&mut messages, &thinking);
        canonicalize_message_order(&mut messages);

        // Context budgeting: cap output tokens to what the profile can produce.
        let message_tokens = estimate_messages_tokens(&messages);
        let tool_tokens = tools
            .as_ref()
            .map(|t| estimate_tool_definitions_tokens(t))
            .unwrap_or(0);
        let input_tokens = message_tokens + tool_tokens;
        let hard_limit = profile.context_length;
        let min_output = 512_usize;

        if input_tokens + min_output > hard_limit {
            let msg = format!(
                "input_tokens ({}) + min_output ({}) > context_length ({}) for profile '{}'. \
                 Messages: {} tokens, Tools: {} tokens.",
                input_tokens, min_output, hard_limit, profile.model, message_tokens, tool_tokens
            );
            tracing::error!("CONTEXT OVERFLOW (profile): {}", msg);
            return Err(ApiError::ContextOverflow(msg).into());
        }

        let available_for_output = hard_limit.saturating_sub(input_tokens);
        let max_tokens = profile.max_tokens.min(available_for_output.max(min_output));

        let mut body = serde_json::json!({
            "model": profile.model,
            "messages": messages,
            "temperature": profile.temperature,
            "max_tokens": max_tokens,
            "stream": false,
        });

        // Resolve native FC for this profile: an explicit profile setting
        // wins, otherwise inherit the parent client's
        // `agent.native_function_calling`. Previously this path was
        // hard-coded to `false`, which silently disabled native FC for
        // any swarm code that routed through a `ModelProfile` even when
        // the parent agent had it enabled. See `src/api/tool_calling.rs`.
        let native_fc =
            profile.effective_native_function_calling(self.config.agent.native_function_calling);
        attach_tools_to_body(&mut body, &tools, native_fc);

        if let ThinkingMode::Budget(tokens) = thinking {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": tokens
            });
        }

        merge_extra_body(
            &mut body,
            profile.extra_body.as_ref(),
            "model profile chat request",
        )?;

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

/// Blocking probe of `/v1/models` to detect which backend engine is running.
///
/// Inspects response headers (`Server`) and body content for tell-tale
/// strings from llama.cpp, SGLang, or vLLM.  Returns a lower-case label
/// like `"llama.cpp"`, `"sglang"`, `"vllm"`, or `"unknown"`.
///
/// This is a synchronous helper so it can be called from the bench-harness
/// runner (which runs inside `spawn_blocking`).
pub fn detect_backend(endpoint: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build blocking HTTP client")?;

    let url = format!("{}/models", endpoint.trim_end_matches('/'));

    let response = client
        .get(&url)
        .send()
        .context("Failed to query /v1/models")?;

    // Header hints
    if let Some(server) = response.headers().get("server") {
        if let Ok(s) = server.to_str() {
            let lower = s.to_lowercase();
            if lower.contains("llama") {
                return Ok("llama.cpp".into());
            }
            if lower.contains("sglang") {
                return Ok("sglang".into());
            }
            if lower.contains("vllm") {
                return Ok("vllm".into());
            }
        }
    }

    // Body hints
    let body = response.text().unwrap_or_default();
    let lower = body.to_lowercase();
    if lower.contains("llama.cpp") || lower.contains("llamacpp") {
        return Ok("llama.cpp".into());
    }
    if lower.contains("sglang") {
        return Ok("sglang".into());
    }
    if lower.contains("vllm") {
        return Ok("vllm".into());
    }

    Ok("unknown".into())
}

/// Generate a uniform random jitter value in `[0.0, 1.0)`.
///
/// The previous implementation derived the value from
/// `SystemTime::now().subsec_nanos() % 1000` and was not actually random
/// when called repeatedly in a tight loop — sequential calls landed in
/// the same nanosecond bucket, producing skewed distributions. This now
/// uses thread_rng so retry jitter is genuinely uniform.
pub(crate) fn rand_jitter() -> f64 {
    use rand::distr::{Distribution, StandardUniform};
    StandardUniform.sample(&mut rand::rng())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_backend_returns_string_for_unresponsive() {
        // A port that is almost certainly closed.
        let result = detect_backend("http://127.0.0.1:59999/v1");
        // Should either fail (network error) or return "unknown".
        if let Ok(label) = result {
            assert!(
                ["llama.cpp", "sglang", "vllm", "unknown"].contains(&label.as_str()),
                "unexpected backend label: {}",
                label
            );
        }
        // network error is acceptable
    }
}
