//! Mock LLM API Server for CI Testing
//!
//! Provides a [`MockLlmServer`] that emulates an OpenAI-compatible
//! `/v1/chat/completions` endpoint. Designed for deterministic unit and
//! integration tests that must not depend on a live model endpoint.
//!
//! # Features
//! - Canned text responses
//! - Tool-call responses (function calling)
//! - Configurable error responses (status code + body)
//! - Latency simulation
//! - Builder pattern for ergonomic test setup
//!
//! # Example
//! ```ignore
//! let server = MockLlmServer::builder()
//!     .with_response("Hello from mock!")
//!     .with_latency_ms(50)
//!     .build()
//!     .await;
//! let url = server.url(); // e.g. "http://127.0.0.1:12345"
//! // ... point your client at `url` ...
//! server.stop().await;
//! ```

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single tool call that the mock server can include in its response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockToolCall {
    /// Unique call id, e.g. `"call_abc123"`.
    pub id: String,
    /// Tool function name, e.g. `"file_read"`.
    pub name: String,
    /// JSON-encoded arguments string, e.g. `"{\"path\":\"foo.rs\"}"`.
    pub arguments: String,
}

/// Describes how the mock server should respond to the next request.
#[derive(Debug, Clone)]
pub enum MockResponse {
    /// Return a plain assistant text message.
    Text(String),
    /// Return a response containing tool calls.
    ToolCalls(Vec<MockToolCall>),
    /// Return an HTTP error with the given status code and body.
    Error { status: u16, body: String },
}

/// A lightweight mock HTTP server that speaks just enough of the
/// OpenAI chat-completions protocol to satisfy selfware's API client.
pub struct MockLlmServer {
    /// Local URL the server listens on, e.g. `"http://127.0.0.1:PORT"`.
    url: String,
    /// Sender half of a shutdown signal.
    shutdown_tx: watch::Sender<bool>,
    /// Join handle for the background accept loop.
    handle: tokio::task::JoinHandle<()>,
}

impl MockLlmServer {
    /// Create a new [`MockLlmServerBuilder`] for ergonomic configuration.
    pub fn builder() -> MockLlmServerBuilder {
        MockLlmServerBuilder::default()
    }

    /// Start the mock server with the given configuration.
    ///
    /// Binds to `127.0.0.1:0` (OS-assigned port), spawns a background
    /// tokio task, and returns a handle that can be used to query the URL
    /// or stop the server.
    pub async fn start(config: MockServerConfig) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind mock server");
        let addr = listener.local_addr().expect("failed to get local addr");
        let url = format!("http://{}", addr);

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let config = Arc::new(config);

        let handle = tokio::spawn(accept_loop(listener, config, shutdown_rx));

        Self {
            url,
            shutdown_tx,
            handle,
        }
    }

    /// The base URL of this mock server (e.g. `"http://127.0.0.1:54321"`).
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Signal the server to stop accepting new connections and wait for the
    /// background task to finish.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.handle.await;
    }
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Configuration produced by the builder that drives mock server behaviour.
#[derive(Debug, Clone)]
pub struct MockServerConfig {
    /// Queue of responses. Each request pops the next response; when the
    /// queue is exhausted the server falls back to `default_response`.
    pub responses: Vec<MockResponse>,
    /// Response used after the queue is empty.
    pub default_response: MockResponse,
    /// Artificial latency added before every response (milliseconds).
    pub latency_ms: u64,
    /// Model name included in the JSON response body.
    pub model: String,
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            responses: Vec::new(),
            default_response: MockResponse::Text("Hello from MockLlmServer".to_string()),
            latency_ms: 0,
            model: "mock-model".to_string(),
        }
    }
}

/// Builder for [`MockLlmServer`] providing a fluent configuration API.
#[derive(Default)]
pub struct MockLlmServerBuilder {
    config: MockServerConfig,
}

impl MockLlmServerBuilder {
    /// Queue a plain text response. Responses are served in FIFO order.
    pub fn with_response(mut self, text: impl Into<String>) -> Self {
        self.config.responses.push(MockResponse::Text(text.into()));
        self
    }

    /// Queue a tool-call response.
    pub fn with_tool_calls(mut self, calls: Vec<MockToolCall>) -> Self {
        self.config.responses.push(MockResponse::ToolCalls(calls));
        self
    }

    /// Queue an error response with the specified HTTP status code and body.
    pub fn with_error(mut self, status: u16, body: impl Into<String>) -> Self {
        self.config.responses.push(MockResponse::Error {
            status,
            body: body.into(),
        });
        self
    }

    /// Set the artificial latency (in milliseconds) applied before every
    /// response is sent.
    pub fn with_latency(mut self, ms: u64) -> Self {
        self.config.latency_ms = ms;
        self
    }

    /// Override the model name returned in response bodies.
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set the fallback response used once the queued responses are exhausted.
    pub fn with_default_response(mut self, resp: MockResponse) -> Self {
        self.config.default_response = resp;
        self
    }

    /// Build and start the mock server, returning a ready-to-use handle.
    pub async fn build(self) -> MockLlmServer {
        MockLlmServer::start(self.config).await
    }
}

// ---------------------------------------------------------------------------
// Internal: accept loop & request handling
// ---------------------------------------------------------------------------

/// Background accept loop. Runs until `shutdown_rx` signals true.
async fn accept_loop(
    listener: TcpListener,
    config: Arc<MockServerConfig>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    // Wrap the response index in a mutex so we can pop from the queue
    let response_idx = Arc::new(Mutex::new(0usize));

    loop {
        tokio::select! {
            // Check shutdown signal
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            // Accept new connections
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let cfg = Arc::clone(&config);
                        let idx = Arc::clone(&response_idx);
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, cfg, idx).await {
                                tracing::debug!("mock server connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::debug!("mock server accept error: {}", e);
                    }
                }
            }
        }
    }
}

/// Handle a single HTTP connection. Reads the request, determines the
/// response from the config, and writes raw HTTP back.
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    config: Arc<MockServerConfig>,
    response_idx: Arc<Mutex<usize>>,
) -> std::io::Result<()> {
    // Read request into a buffer (we don't need to parse it fully)
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);

    // Only handle POST /v1/chat/completions
    let is_chat = request.starts_with("POST") && request.contains("/v1/chat/completions");

    if !is_chat {
        // Return 404 for anything else
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    // Apply configured latency
    if config.latency_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(config.latency_ms)).await;
    }

    // Pick the next response
    let mock_response = {
        let mut idx = response_idx.lock().await;
        if *idx < config.responses.len() {
            let resp = config.responses[*idx].clone();
            *idx += 1;
            resp
        } else {
            config.default_response.clone()
        }
    };

    match mock_response {
        MockResponse::Text(text) => {
            let body = format_chat_response(&config.model, &text, None);
            write_http_response(&mut stream, 200, &body).await?;
        }
        MockResponse::ToolCalls(calls) => {
            let tool_calls_json = format_tool_calls(&calls);
            let body = format_chat_response(&config.model, "", Some(&tool_calls_json));
            write_http_response(&mut stream, 200, &body).await?;
        }
        MockResponse::Error { status, body } => {
            write_http_response(&mut stream, status, &body).await?;
        }
    }

    Ok(())
}

/// Format a JSON body conforming to the OpenAI chat completions response
/// schema.
fn format_chat_response(model: &str, content: &str, tool_calls: Option<&str>) -> String {
    let tool_calls_field = match tool_calls {
        Some(tc) => format!(r#","tool_calls":{}"#, tc),
        None => String::new(),
    };

    let finish_reason = if tool_calls.is_some() {
        "tool_calls"
    } else {
        "stop"
    };

    // Escape content for JSON embedding
    let escaped_content = serde_json::to_string(content).unwrap_or_else(|_| "\"\"".to_string());
    // Remove surrounding quotes since we embed it in the template
    let escaped_content = &escaped_content[1..escaped_content.len() - 1];

    format!(
        r#"{{"id":"mock-resp-1","object":"chat.completion","created":1700000000,"model":"{}","choices":[{{"index":0,"message":{{"role":"assistant","content":"{}"{}}},"finish_reason":"{}"}}],"usage":{{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}}}"#,
        model, escaped_content, tool_calls_field, finish_reason,
    )
}

/// Serialize a list of [`MockToolCall`]s into the JSON array format expected
/// by the OpenAI API.
fn format_tool_calls(calls: &[MockToolCall]) -> String {
    let items: Vec<String> = calls
        .iter()
        .map(|c| {
            // Escape arguments string for safe JSON embedding
            let escaped_args =
                serde_json::to_string(&c.arguments).unwrap_or_else(|_| "\"{}\"".to_string());
            format!(
                r#"{{"id":"{}","type":"function","function":{{"name":"{}","arguments":{}}}}}"#,
                c.id, c.name, escaped_args,
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// Write a full HTTP/1.1 response to the stream.
async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    body: &str,
) -> std::io::Result<()> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };

    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        status_text,
        body.len(),
        body,
    );

    stream.write_all(response.as_bytes()).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the builder creates a server that binds to a local port
    /// and returns a valid URL.
    #[tokio::test]
    async fn test_mock_server_starts_and_returns_url() {
        let server = MockLlmServer::builder()
            .with_response("hello")
            .build()
            .await;

        let url = server.url();
        assert!(url.starts_with("http://127.0.0.1:"));

        server.stop().await;
    }

    /// Verify that a plain text response is returned as a well-formed
    /// OpenAI chat completion JSON body.
    #[tokio::test]
    async fn test_mock_server_text_response() {
        let server = MockLlmServer::builder()
            .with_response("Mock answer")
            .build()
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/chat/completions", server.url()))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = resp.json().await.expect("json parse failed");
        assert_eq!(
            body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or(""),
            "Mock answer"
        );
        assert_eq!(body["choices"][0]["finish_reason"], "stop");

        server.stop().await;
    }

    /// Verify that tool call responses include the expected structure.
    #[tokio::test]
    async fn test_mock_server_tool_call_response() {
        let server = MockLlmServer::builder()
            .with_tool_calls(vec![MockToolCall {
                id: "call_1".to_string(),
                name: "file_read".to_string(),
                arguments: r#"{"path":"test.rs"}"#.to_string(),
            }])
            .build()
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/chat/completions", server.url()))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "read file"}]
            }))
            .send()
            .await
            .expect("request failed");

        let body: serde_json::Value = resp.json().await.expect("json parse failed");
        let tool_calls = &body["choices"][0]["message"]["tool_calls"];
        assert!(tool_calls.is_array());
        assert_eq!(tool_calls[0]["function"]["name"], "file_read");
        assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");

        server.stop().await;
    }

    /// Verify that error responses return the configured HTTP status code.
    #[tokio::test]
    async fn test_mock_server_error_response() {
        let server = MockLlmServer::builder()
            .with_error(429, r#"{"error":"rate limit exceeded"}"#)
            .build()
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("{}/v1/chat/completions", server.url()))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status().as_u16(), 429);

        server.stop().await;
    }

    /// Verify that latency simulation adds a measurable delay.
    #[tokio::test]
    async fn test_mock_server_latency_simulation() {
        let server = MockLlmServer::builder()
            .with_response("delayed")
            .with_latency(100)
            .build()
            .await;

        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let _resp = client
            .post(format!("{}/v1/chat/completions", server.url()))
            .json(&serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .expect("request failed");

        let elapsed = start.elapsed();
        assert!(
            elapsed.as_millis() >= 80,
            "expected at least 80ms delay, got {}ms",
            elapsed.as_millis()
        );

        server.stop().await;
    }

    /// Verify that queued responses are served in FIFO order and that
    /// the default response is used after the queue is exhausted.
    #[tokio::test]
    async fn test_mock_server_response_queue() {
        let server = MockLlmServer::builder()
            .with_response("first")
            .with_response("second")
            .with_default_response(MockResponse::Text("fallback".to_string()))
            .build()
            .await;

        let client = reqwest::Client::new();
        let make_request = |c: &reqwest::Client, url: String| {
            c.post(format!("{}/v1/chat/completions", url))
                .json(&serde_json::json!({
                    "model": "test",
                    "messages": [{"role": "user", "content": "hi"}]
                }))
                .send()
        };

        // First response
        let r1: serde_json::Value = make_request(&client, server.url().to_string())
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(r1["choices"][0]["message"]["content"], "first");

        // Second response
        let r2: serde_json::Value = make_request(&client, server.url().to_string())
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(r2["choices"][0]["message"]["content"], "second");

        // Third request -> fallback
        let r3: serde_json::Value = make_request(&client, server.url().to_string())
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(r3["choices"][0]["message"]["content"], "fallback");

        server.stop().await;
    }

    /// Verify that non-chat endpoints return 404.
    #[tokio::test]
    async fn test_mock_server_returns_404_for_unknown_paths() {
        let server = MockLlmServer::builder()
            .with_response("hello")
            .build()
            .await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("{}/v1/models", server.url()))
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status().as_u16(), 404);

        server.stop().await;
    }

    /// Verify the builder can set a custom model name that appears in
    /// the response body.
    #[tokio::test]
    async fn test_mock_server_custom_model_name() {
        let server = MockLlmServer::builder()
            .with_response("hi")
            .with_model("gpt-4-test")
            .build()
            .await;

        let client = reqwest::Client::new();
        let body: serde_json::Value = client
            .post(format!("{}/v1/chat/completions", server.url()))
            .json(&serde_json::json!({
                "model": "gpt-4-test",
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(body["model"], "gpt-4-test");

        server.stop().await;
    }

    /// Verify that format_chat_response produces valid JSON.
    #[test]
    fn test_format_chat_response_valid_json() {
        let body = format_chat_response("test-model", "Hello world", None);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&body);
        assert!(parsed.is_ok(), "response body is not valid JSON: {}", body);
    }

    /// Verify that format_tool_calls produces valid JSON.
    #[test]
    fn test_format_tool_calls_valid_json() {
        let calls = vec![
            MockToolCall {
                id: "c1".to_string(),
                name: "file_read".to_string(),
                arguments: r#"{"path":"a.rs"}"#.to_string(),
            },
            MockToolCall {
                id: "c2".to_string(),
                name: "shell_exec".to_string(),
                arguments: r#"{"command":"ls"}"#.to_string(),
            },
        ];
        let json_str = format_tool_calls(&calls);
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        assert!(parsed.is_ok(), "tool calls JSON is invalid: {}", json_str);
        let arr = parsed.unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 2);
    }

    /// Verify the default MockServerConfig is sensible.
    #[test]
    fn test_mock_server_config_defaults() {
        let config = MockServerConfig::default();
        assert!(config.responses.is_empty());
        assert_eq!(config.latency_ms, 0);
        assert_eq!(config.model, "mock-model");
    }
}
