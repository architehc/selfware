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
