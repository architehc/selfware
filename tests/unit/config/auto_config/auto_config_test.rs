use super::*;

#[test]
fn test_detect_backend() {
    assert_eq!(detect_backend("vllm"), BackendType::Vllm);
    assert_eq!(detect_backend("sglang"), BackendType::Sglang);
    assert_eq!(detect_backend("ollama"), BackendType::Ollama);
    assert_eq!(detect_backend("openai"), BackendType::OpenAI);
    assert_eq!(detect_backend("random"), BackendType::Unknown);
}

#[test]
fn test_backend_name() {
    assert_eq!(BackendType::Vllm.name(), "vLLM");
    assert_eq!(BackendType::Sglang.name(), "SGLang");
}

#[test]
fn test_configurator_new() {
    let c = AutoConfigurator::new("http://localhost:8000/v1", Some("test-key"));
    assert_eq!(c.endpoint, "http://localhost:8000/v1");
    assert_eq!(c.api_key.as_deref(), Some("test-key"));
}

#[test]
fn test_configurator_strips_trailing_slash() {
    let c = AutoConfigurator::new("http://localhost:8000/v1/", None);
    assert_eq!(c.endpoint, "http://localhost:8000/v1");
}

// ── generate_config: never emit a config the loader rejects ─────────────

/// Minimal OpenAI-ish HTTP server: serves `/models` reporting the given
/// `max_model_len` and a canned non-streaming chat completion. Raw TCP in
/// a background thread — no async runtime needed on the serving side.
fn spawn_mock_server(max_model_len: u64) -> String {
    use std::io::{BufRead, BufReader, Write};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let mut reader = BufReader::new(stream);
            // Read the request line + headers (body not needed).
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() {
                continue;
            }
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if line == "\r\n" {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let body = if request_line.contains("/models") {
                format!(
                    r#"{{"data":[{{"id":"m0","root":"m0","owned_by":"vllm","max_model_len":{}}}]}}"#,
                    max_model_len
                )
            } else {
                // chat/completions (streaming probe only checks the status)
                r#"{"choices":[{"message":{"content":"HELLO_SELFWARE"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_string()
            };
            let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
            let mut stream = reader.into_inner();
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    format!("http://{}/v1", addr)
}

#[tokio::test]
async fn test_generate_config_zero_max_model_len_uses_conservative_context() {
    // A /models listing WITHOUT a usable max_model_len (0) used to produce
    // context_length=0 / token_budget=0 — a config the loader rejects.
    let url = spawn_mock_server(0);
    let configurator = AutoConfigurator::new(&url, None);
    let config = configurator.generate_config("m0").await.unwrap();
    assert_eq!(
        config.context_length,
        crate::config::UNKNOWN_MODEL_CONTEXT_LENGTH,
        "unknown context must fall back to the conservative default, not 0"
    );
    assert!(
        config.agent.token_budget > 0,
        "token_budget must never be 0 in a generated config"
    );
    // And the whole thing passes the same validation a disk read applies.
    config.validate().unwrap();
}

#[tokio::test]
async fn test_generate_config_uses_reported_context_length() {
    let url = spawn_mock_server(131072);
    let configurator = AutoConfigurator::new(&url, None);
    let config = configurator.generate_config("m0").await.unwrap();
    assert_eq!(config.context_length, 131072);
    // context minus output+overhead headroom, unchanged from before.
    assert_eq!(config.agent.token_budget, 131072 - (16384 + 50_000));
    config.validate().unwrap();
}
