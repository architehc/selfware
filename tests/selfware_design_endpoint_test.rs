//! Integration tests for the `llm.selfware.design` SGLang endpoint.
//!
//! Exercises `https://llm.selfware.design/v1` serving `qwen38-flash-next` across:
//! 1. Endpoint Discovery & SGLang Runtime Metadata
//! 2. Basic Chat Completion & Token Usage
//! 3. Streaming SSE Contract & Chunk Accumulation
//! 4. Reasoning Channel Separation (Thinking Mode)
//! 5. Fast Path with Thinking Disabled
//! 6. Qwen XML Tool-Calling Extraction via Unified Extractor
//! 7. Multi-Turn Tool Execution Round-Trip
//! 8. Agent File Tool Calling Contract
//! 9. Concurrent Stream Throughput & Stability
//!
//! Run with:
//!   cargo test --test selfware_design_endpoint_test -- --nocapture

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client as HttpClient;
use serde_json::{json, Value};

use selfware::api::client::RetryConfig;
use selfware::api::extract_tool_calls;
use selfware::api::types::{FunctionDefinition, Message, ToolDefinition};
use selfware::api::{ApiClient, StreamChunk, ThinkingMode};
use selfware::config::{AgentConfig, Config, RedactedString};

const DEFAULT_ENDPOINT: &str = "https://llm.selfware.design/v1";
const DEFAULT_MODEL: &str = "qwen38-flash-next";
const PER_TEST_TIMEOUT: Duration = Duration::from_secs(45);

fn endpoint() -> String {
    std::env::var("SELFWARE_ENDPOINT").unwrap_or_else(|_| DEFAULT_ENDPOINT.to_string())
}

fn model() -> String {
    std::env::var("SELFWARE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string())
}

fn api_key() -> Option<String> {
    std::env::var("SELFWARE_API_KEY").ok()
}

async fn is_endpoint_reachable() -> bool {
    let client = match HttpClient::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    let url = format!("{}/models", endpoint().trim_end_matches('/'));
    let mut req = client.get(&url);
    if let Some(key) = api_key() {
        req = req.bearer_auth(key);
    }
    match req.send().await {
        Ok(r) => r.status().is_success(),
        Err(_) => false,
    }
}

macro_rules! skip_if_unreachable {
    () => {
        if !is_endpoint_reachable().await {
            if std::env::var("REQUIRE_ENDPOINT").is_ok() {
                panic!(
                    "Endpoint {} is required by REQUIRE_ENDPOINT but was unreachable",
                    endpoint()
                );
            }
            let test_name = module_path!();
            eprintln!(
                "SKIPPED: {} - endpoint {} is unreachable in current environment",
                test_name,
                endpoint()
            );
            return;
        }
    };
}

fn build_test_config(
    native_function_calling: bool,
    extra_body: Option<serde_json::Map<String, Value>>,
) -> Config {
    let mut cfg = Config {
        endpoint: endpoint(),
        model: model(),
        api_key: api_key().map(RedactedString::new),
        max_tokens: 2048,
        context_length: 32_768,
        temperature: 0.0,
        agent: AgentConfig {
            native_function_calling,
            step_timeout_secs: 45,
            ..AgentConfig::default()
        },
        extra_body,
        ..Config::default()
    };
    cfg.retry.max_retries = 1;
    cfg
}

fn build_test_client(cfg: &Config) -> ApiClient {
    ApiClient::new(cfg)
        .expect("ApiClient::new must succeed")
        .with_retry_config(RetryConfig {
            max_retries: 1,
            initial_delay_ms: 200,
            max_delay_ms: 1_000,
            retryable_status_codes: vec![429, 502, 503],
        })
}

fn calculator_tool() -> ToolDefinition {
    ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "calculator".to_string(),
            description: "Multiply two integers and return the product.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": {
                        "type": "integer",
                        "description": "First integer"
                    },
                    "b": {
                        "type": "integer",
                        "description": "Second integer"
                    }
                },
                "required": ["a", "b"]
            }),
        },
    }
}

fn file_read_tool() -> ToolDefinition {
    ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "file_read".to_string(),
            description: "Read the contents of a file at the given path.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative or absolute file path"
                    }
                },
                "required": ["path"]
            }),
        },
    }
}

// ── 1. Endpoint Discovery & SGLang Runtime Metadata ─────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_models_discovery() {
    skip_if_unreachable!();

    let client = HttpClient::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let url = format!("{}/models", endpoint().trim_end_matches('/'));
    let mut req = client.get(&url);
    if let Some(key) = api_key() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.expect("GET /models must succeed");
    assert!(resp.status().is_success());

    let body: Value = resp.json().await.expect("valid JSON expected");
    let data = body
        .get("data")
        .and_then(|v| v.as_array())
        .expect("data array expected");
    assert!(!data.is_empty(), "expected at least one model in catalog");

    let expected_model = model();
    let found = data.iter().find(|m| {
        m.get("id")
            .and_then(|id| id.as_str())
            .map(|id| id.contains(&expected_model))
            .unwrap_or(false)
    });
    assert!(
        found.is_some(),
        "model '{}' not listed in endpoint /models: {:?}",
        expected_model,
        data
    );

    let model_entry = found.unwrap();
    let owned_by = model_entry
        .get("owned_by")
        .and_then(|v| v.as_str())
        .expect("model catalog entry must contain owned_by");
    assert_eq!(owned_by, "sglang", "expected SGLang backend");

    let max_len = model_entry
        .get("max_model_len")
        .and_then(|v| v.as_u64())
        .expect("model catalog entry must contain max_model_len");
    assert!(
        max_len >= 1_000_000,
        "advertised max_model_len should be >= 1,000,000 (1M context window), got: {}",
        max_len
    );
}

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_sglang_server_info() {
    skip_if_unreachable!();

    let base = endpoint();
    let host_root = base
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(&base);
    let url = format!("{}/get_server_info", host_root);

    let client = HttpClient::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => {
            if std::env::var("REQUIRE_ENDPOINT").is_ok() {
                panic!("/get_server_info endpoint must be reachable when REQUIRE_ENDPOINT is set");
            }
            eprintln!("SKIPPED: /get_server_info endpoint not reachable or disabled on proxy");
            return;
        }
    };

    let body: Value = resp.json().await.expect("valid JSON from /get_server_info");
    assert!(body.get("version").is_some(), "expected SGLang version");
    let tool_parser = body
        .get("tool_call_parser")
        .and_then(|v| v.as_str())
        .expect("expected tool_call_parser in /get_server_info");
    assert_eq!(tool_parser, "qwen");
    let reasoning_parser = body
        .get("reasoning_parser")
        .and_then(|v| v.as_str())
        .expect("expected reasoning_parser in /get_server_info");
    assert_eq!(reasoning_parser, "qwen3");
}

// ── 2. Plain Chat Completion & Token Usage ───────────────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_plain_chat_completion() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(false, Some(extra));
    let client = build_test_client(&cfg);

    let messages = vec![
        Message::system("You are a concise assistant. Reply with only the requested number."),
        Message::user("What is 17 + 23? Answer with only the number."),
    ];

    let start = Instant::now();
    let resp = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(messages, None, ThinkingMode::Disabled),
    )
    .await
    .expect("test timed out")
    .expect("plain chat request failed");
    let elapsed = start.elapsed();

    let choice = resp.choices.first().expect("at least one choice");
    let text = choice.message.content.text_all();
    assert!(
        text.contains("40"),
        "expected '40' in response, got: '{}'",
        text
    );
    assert!(resp.usage.prompt_tokens > 0, "expected prompt tokens > 0");
    assert!(
        resp.usage.completion_tokens > 0,
        "expected completion tokens > 0"
    );
    println!("Plain chat completed in {:.2?}: '{}'", elapsed, text.trim());
}

// ── 3. Streaming SSE Contract & Chunk Accumulation ───────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_streaming_sse_contract() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(false, Some(extra));
    let client = build_test_client(&cfg);

    let messages = vec![Message::user(
        "Reply with the word 'PONG' and nothing else.",
    )];

    let start = Instant::now();
    let stream = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat_stream(messages, None, ThinkingMode::Disabled),
    )
    .await
    .expect("stream connection timed out")
    .expect("chat_stream request failed");

    let mut rx = stream.into_channel().await;
    let mut chunks_received = 0usize;
    let mut accumulated = String::new();
    let mut got_done = false;

    while let Some(item) = rx.recv().await {
        let chunk = item.expect("valid stream chunk");
        chunks_received += 1;
        match chunk {
            StreamChunk::Content(text) => accumulated.push_str(&text),
            StreamChunk::Done => {
                got_done = true;
                break;
            }
            _ => {}
        }
    }

    assert!(got_done, "expected StreamChunk::Done terminating stream");
    assert!(
        chunks_received > 0,
        "empty stream error: no chunks received"
    );
    assert!(
        accumulated.to_lowercase().contains("pong"),
        "streamed text missing 'pong': '{}'",
        accumulated
    );
    println!(
        "Streamed in {:.2?} (chunks={}, done={}): '{}'",
        start.elapsed(),
        chunks_received,
        got_done,
        accumulated.trim()
    );
}

// ── 4. Reasoning Channel Separation (Thinking Mode) ──────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_thinking_separation() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": true }),
    );
    let cfg = build_test_config(false, Some(extra));
    let client = build_test_client(&cfg);

    let messages = vec![
        Message::system("Solve the problem. Your final answer must be a single number only."),
        Message::user(
            "Alice has 12 apples. She gives half to Bob, then eats two of \
             the remainder. How many apples does Alice have left? Answer with \
             a number only.",
        ),
    ];

    let start = Instant::now();
    let resp = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(messages, None, ThinkingMode::Enabled),
    )
    .await
    .expect("test timed out")
    .expect("thinking request failed");

    let choice = resp.choices.first().expect("at least one choice");
    let content = choice.message.content.text_all();
    let reasoning = choice
        .reasoning_content
        .as_deref()
        .or(choice.message.reasoning_content.as_deref())
        .unwrap_or_default();

    // Critical assertion 1: NO <think> tags leak into content
    assert!(
        !content.contains("<think>") && !content.contains("</think>"),
        "reasoning leaked into content channel: '{}'",
        content
    );

    // Critical assertion 2: reasoning was captured
    assert!(
        !reasoning.is_empty(),
        "expected reasoning_content to be populated"
    );

    // Critical assertion 3: answer correctness
    assert!(
        content.contains('4'),
        "expected answer '4' in content, got: '{}'",
        content
    );

    // Critical assertion 4: verify reasoning tokens reported in endpoint usage
    let http = HttpClient::builder()
        .timeout(PER_TEST_TIMEOUT)
        .build()
        .unwrap();
    let mut raw_req = http
        .post(format!("{}/chat/completions", endpoint().trim_end_matches('/')))
        .json(&json!({
            "model": model(),
            "messages": [
                {"role": "system", "content": "Solve the problem. Your final answer must be a single number only."},
                {"role": "user", "content": "Alice has 12 apples. She gives half to Bob, then eats two of the remainder. How many apples does Alice have left? Answer with a number only."}
            ],
            "temperature": 0.0,
            "max_tokens": 1024,
            "chat_template_kwargs": { "enable_thinking": true }
        }));
    if let Some(key) = api_key() {
        raw_req = raw_req.bearer_auth(key);
    }
    if let Ok(raw_res) = raw_req.send().await {
        if raw_res.status().is_success() {
            if let Ok(raw_json) = raw_res.json::<Value>().await {
                let reasoning_tokens = raw_json
                    .pointer("/usage/completion_tokens_details/reasoning_tokens")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        raw_json
                            .pointer("/usage/reasoning_tokens")
                            .and_then(|v| v.as_u64())
                    });
                if let Some(tokens) = reasoning_tokens {
                    assert!(
                        tokens > 0,
                        "expected >0 reasoning tokens when enable_thinking=true, got: {}",
                        tokens
                    );
                } else if std::env::var("REQUIRE_ENDPOINT").is_ok() {
                    panic!(
                        "endpoint usage did not report reasoning_tokens: {:?}",
                        raw_json.get("usage")
                    );
                }
            }
        }
    }

    println!(
        "Thinking test in {:.2?}: reasoning={} chars, content='{}'",
        start.elapsed(),
        reasoning.len(),
        content.trim()
    );
}

// ── 5. Fast Path with Thinking Disabled ──────────────────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_thinking_disabled_budget() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(false, Some(extra));
    let client = build_test_client(&cfg);

    let messages = vec![Message::user("What is 1 + 1? Answer with only the number.")];

    let start = Instant::now();
    let resp = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(messages, None, ThinkingMode::Disabled),
    )
    .await
    .expect("test timed out")
    .expect("fast path request failed");

    let choice = resp.choices.first().expect("at least one choice");
    let reasoning = choice
        .reasoning_content
        .as_deref()
        .or(choice.message.reasoning_content.as_deref());

    // When enable_thinking=false, reasoning should be None or 0 chars
    assert!(
        reasoning.is_none() || reasoning.unwrap().is_empty(),
        "reasoning should be absent with enable_thinking=false"
    );
    assert!(
        choice.message.content.text_all().contains('2'),
        "expected '2' in content"
    );

    // Verify 0 reasoning tokens reported in endpoint usage
    let http = HttpClient::builder()
        .timeout(PER_TEST_TIMEOUT)
        .build()
        .unwrap();
    let mut raw_req = http
        .post(format!("{}/chat/completions", endpoint().trim_end_matches('/')))
        .json(&json!({
            "model": model(),
            "messages": [{"role": "user", "content": "What is 1 + 1? Answer with only the number."}],
            "temperature": 0.0,
            "max_tokens": 64,
            "chat_template_kwargs": { "enable_thinking": false }
        }));
    if let Some(key) = api_key() {
        raw_req = raw_req.bearer_auth(key);
    }
    if let Ok(raw_res) = raw_req.send().await {
        if raw_res.status().is_success() {
            if let Ok(raw_json) = raw_res.json::<Value>().await {
                let reasoning_tokens = raw_json
                    .pointer("/usage/completion_tokens_details/reasoning_tokens")
                    .and_then(|v| v.as_u64())
                    .or_else(|| {
                        raw_json
                            .pointer("/usage/reasoning_tokens")
                            .and_then(|v| v.as_u64())
                    });
                if let Some(tokens) = reasoning_tokens {
                    assert_eq!(
                        tokens, 0,
                        "expected 0 reasoning tokens when enable_thinking=false, got: {}",
                        tokens
                    );
                } else if std::env::var("REQUIRE_ENDPOINT").is_ok() {
                    panic!(
                        "endpoint usage did not report reasoning_tokens: {:?}",
                        raw_json.get("usage")
                    );
                }
            }
        }
    }

    println!(
        "Fast path completed in {:.2?}, tokens={}",
        start.elapsed(),
        resp.usage.completion_tokens
    );
}

// ── 6. Qwen XML Tool-Calling Extraction via Unified Extractor ────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_tool_call_xml_extraction() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(true, Some(extra));
    let client = build_test_client(&cfg);
    let tools = vec![calculator_tool()];

    let messages = vec![
        Message::system(
            "You have access to a `calculator` tool that multiplies two integers. \
             When the user asks for arithmetic, you MUST call the tool.",
        ),
        Message::user("What is 17 * 23? Use the calculator tool."),
    ];

    let start = Instant::now();
    let resp = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(messages, Some(tools), ThinkingMode::Disabled),
    )
    .await
    .expect("test timed out")
    .expect("tool-calling request failed");

    let choice = resp.choices.first().expect("at least one choice");
    let tool_calls = extract_tool_calls(&choice.message, true);

    assert!(
        !tool_calls.is_empty(),
        "expected extract_tool_calls to extract at least one call from: {:?}",
        choice.message
    );

    let call = &tool_calls[0];
    assert_eq!(call.function.name, "calculator");

    let args: Value =
        serde_json::from_str(&call.function.arguments).expect("arguments must be valid JSON");
    let a = args.get("a").and_then(|v| v.as_i64()).expect("param a");
    let b = args.get("b").and_then(|v| v.as_i64()).expect("param b");

    assert!(
        (a == 17 && b == 23) || (a == 23 && b == 17),
        "expected arguments 17 and 23, got: a={}, b={}",
        a,
        b
    );

    println!(
        "Tool call extracted in {:.2?}: {}(a={}, b={}) [id={}]",
        start.elapsed(),
        call.function.name,
        a,
        b,
        call.id
    );
}

// ── 7. Multi-Turn Tool Execution Round-Trip ──────────────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_multiturn_tool_execution_roundtrip() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(true, Some(extra));
    let client = build_test_client(&cfg);
    let tools = vec![calculator_tool()];

    // Turn 1: request tool call
    let turn1_messages = vec![
        Message::system(
            "You have access to a `calculator` tool that multiplies two integers. \
             When the user asks for arithmetic, you MUST call the tool.",
        ),
        Message::user("What is 17 * 23? Use the calculator tool."),
    ];

    let resp1 = client
        .chat(
            turn1_messages.clone(),
            Some(tools.clone()),
            ThinkingMode::Disabled,
        )
        .await
        .expect("turn 1 chat request failed");

    let choice1 = resp1.choices.first().expect("choice in turn 1");
    let extracted = extract_tool_calls(&choice1.message, true);
    assert!(!extracted.is_empty(), "turn 1 must emit a tool call");
    let call = &extracted[0];

    // Turn 2: feed tool execution result back
    let mut turn2_messages = turn1_messages;
    let mut assistant_msg = choice1.message.clone();
    if assistant_msg
        .tool_calls
        .as_ref()
        .is_none_or(|c| c.is_empty())
    {
        assistant_msg.tool_calls = Some(extracted.clone());
    }
    turn2_messages.push(assistant_msg);
    turn2_messages.push(Message::tool("391", call.id.clone()));

    let start = Instant::now();
    let resp2 = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(turn2_messages, Some(tools), ThinkingMode::Disabled),
    )
    .await
    .expect("turn 2 timed out")
    .expect("turn 2 chat request failed");

    let reply = resp2
        .choices
        .first()
        .map(|c| c.message.content.text_all())
        .unwrap_or_default();

    assert!(
        reply.contains("391"),
        "expected tool result '391' in final answer: '{}'",
        reply
    );
    println!(
        "Multi-turn tool roundtrip in {:.2?}: '{}'",
        start.elapsed(),
        reply.trim()
    );
}

// ── 8. Agent File Tool Calling Contract ──────────────────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_file_read_tool_contract() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(true, Some(extra));
    let client = build_test_client(&cfg);
    let tools = vec![file_read_tool()];

    let messages = vec![
        Message::system(
            "You are a coding agent with access to `file_read`. When asked to inspect a file, call `file_read`.",
        ),
        Message::user("Please read the contents of './Cargo.toml' using the file_read tool."),
    ];

    let resp = tokio::time::timeout(
        PER_TEST_TIMEOUT,
        client.chat(messages, Some(tools), ThinkingMode::Disabled),
    )
    .await
    .expect("test timed out")
    .expect("file_read tool request failed");

    let choice = resp.choices.first().expect("choice present");
    let calls = extract_tool_calls(&choice.message, true);
    assert!(!calls.is_empty(), "file_read tool call expected");

    let call = &calls[0];
    assert_eq!(call.function.name, "file_read");

    let args: Value = serde_json::from_str(&call.function.arguments)
        .expect("file_read arguments must be valid JSON");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        path.contains("Cargo.toml"),
        "expected path to mention Cargo.toml, got: '{}'",
        path
    );
    println!("File read tool call verified: path='{}'", path);
}

// ── 9. Concurrent Stream Throughput & Stability ──────────────────────────────

#[tokio::test]
#[ignore = "requires live endpoint"]
async fn test_selfware_design_concurrent_streams() {
    skip_if_unreachable!();

    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        json!({ "enable_thinking": false }),
    );
    let cfg = build_test_config(false, Some(extra));
    let client = Arc::new(build_test_client(&cfg));

    let prompts = vec![
        ("req_1", "Reply with just: ONE"),
        ("req_2", "Reply with just: TWO"),
        ("req_3", "Reply with just: THREE"),
    ];

    let start = Instant::now();
    let mut handles = Vec::new();

    for (id, prompt) in prompts {
        let client = Arc::clone(&client);
        let handle = tokio::spawn(async move {
            let messages = vec![Message::user(prompt)];
            let stream = client
                .chat_stream(messages, None, ThinkingMode::Disabled)
                .await?;
            let mut rx = stream.into_channel().await;
            let mut text = String::new();
            while let Some(item) = rx.recv().await {
                if let Ok(StreamChunk::Content(chunk)) = item {
                    text.push_str(&chunk);
                }
            }
            Ok::<(String, String), anyhow::Error>((id.to_string(), text))
        });
        handles.push(handle);
    }

    let mut completed = 0;
    for handle in handles {
        let res = handle.await.expect("task join failed");
        let (id, text) = res.expect("request failed");
        assert!(!text.trim().is_empty(), "{}: response text was empty", id);
        completed += 1;
        println!("Concurrent stream {} returned: '{}'", id, text.trim());
    }

    assert_eq!(completed, 3, "all 3 concurrent streams should succeed");
    println!(
        "3 concurrent streams completed successfully in {:.2?}",
        start.elapsed()
    );
}
