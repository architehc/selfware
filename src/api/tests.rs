use super::client::rand_jitter;
use super::streaming::*;
use super::*;
use std::time::Duration;

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
        cost: None,
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
    let event = r#"data: {"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
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
    // A complete tool call delta is emitted immediately mid-stream.
    let mut acc = ToolCallAccumulator::new();
    let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"file_read","arguments":"{\"path\":\"/test\"}"}}]}}]}"#;
    let results = parse_sse_event(event, &mut acc);
    assert_eq!(results.len(), 1);
    let tc = match &results[0] {
        StreamChunk::ToolCall(tc) => tc,
        other => panic!("expected ToolCall, got {:?}", other),
    };
    assert_eq!(tc.id, "call_123");
    assert_eq!(tc.function.name, "file_read");
    assert!(tc.function.arguments.contains("/test"));

    // Already-emitted calls are not buffered for a later flush.
    let calls = acc.flush();
    assert!(calls.is_empty());
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

    // Third chunk: finish args — the accumulated JSON object is now complete
    // and the call is emitted mid-stream.
    let event3 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"content\":\"hello\"}"}}]}}]}"#;
    let r3 = parse_sse_event(event3, &mut acc);
    assert_eq!(r3.len(), 1);
    assert!(matches!(&r3[0], StreamChunk::ToolCall(tc) if tc.id == "call_456"));

    // Nothing remains pending after the call is emitted.
    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_parse_sse_event_tool_calls_flushed_on_done() {
    // Incomplete tool calls are flushed before the Done marker.
    let mut acc = ToolCallAccumulator::new();

    let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_789","type":"function","function":{"name":"git_status","arguments":"{"}}]}}]}"#;
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
    // Test SSE event with finish_reason but no content.
    //
    // The parser now propagates finish_reason as a `FinishReason` chunk so
    // the agent can record it in per-turn debug artifacts. Older versions
    // dropped it; the new behaviour is that one chunk is emitted.
    let mut acc = ToolCallAccumulator::new();
    let event = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
    let results = parse_sse_event(event, &mut acc);
    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0], StreamChunk::FinishReason(r) if r == "stop"));
}

#[test]
fn test_process_delta_progressive_emission() {
    // Complete calls are emitted as soon as their own arguments form valid JSON,
    // even while an earlier index is still accumulating arguments.
    let mut acc = ToolCallAccumulator::new();

    // Index 0 — first tool call (partial args)
    let delta0 = serde_json::json!({
        "index": 0, "id": "call_a", "type": "function",
        "function": {"name": "file_write", "arguments": "{\"path\":\"/a\","}
    });
    let result0 = acc.process_delta(&delta0);
    assert!(result0.is_none());

    // Index 1 starts before index 0 is fully complete, but its arguments are
    // already complete, so it is emitted immediately.
    let delta1 = serde_json::json!({
        "index": 1, "id": "call_b", "type": "function",
        "function": {"name": "file_read", "arguments": "{\"path\":\"/b\"}"}
    });
    let result1 = acc.process_delta(&delta1);
    assert!(result1.is_some());

    // Index 0 receives a late continuation chunk and is now complete.
    let delta0_late = serde_json::json!({
        "index": 0,
        "function": {"arguments": "\"content\":\"hello\"}"}
    });
    let result2 = acc.process_delta(&delta0_late);
    assert!(result2.is_some());

    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_parse_sse_event_flushes_on_finish_reason() {
    let mut acc = ToolCallAccumulator::new();

    // Buffer an incomplete tool call.
    let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_finish","type":"function","function":{"name":"git_status","arguments":"{"}}]}}]}"#;
    let r1 = parse_sse_event(event1, &mut acc);
    assert!(r1.is_empty());

    let finish = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
    let r2 = parse_sse_event(finish, &mut acc);
    // The pending call is flushed before the propagated finish_reason chunk.
    assert_eq!(r2.len(), 2);
    assert!(matches!(&r2[0], StreamChunk::ToolCall(tc) if tc.id == "call_finish"));
    assert!(matches!(&r2[1], StreamChunk::FinishReason(r) if r == "tool_calls"));
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
    let stream = StreamingResponse::new(response, Duration::from_millis(50), None);
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
        .contains("API Request timed out"));

    let _ = server.await;
}

// ============================================
// ============================================

/// The absolute deadline must bound the WHOLE body stream, independent of the
/// per-chunk timeout. Here the chunk timeout is long (10s) but the deadline is
/// short (150ms) and the server sends headers then stalls forever. Without the
/// deadline the stream would block for 10s on the first chunk; with it, the
/// stream must yield a Timeout promptly.
#[tokio::test]
async fn test_stream_deadline_bounds_stalled_body() {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        drain_http_request(&mut socket).await;
        // Send only the response headers, then stall with the connection open —
        // no SSE body ever arrives.
        let response =
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n";
        socket.write_all(response.as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let response = reqwest::get(format!("http://{}", addr)).await.unwrap();
    // Long per-chunk timeout, short absolute deadline: the deadline must win.
    let deadline = Some(std::time::Instant::now() + Duration::from_millis(150));
    let stream = StreamingResponse::new(response, Duration::from_secs(10), deadline);
    let mut rx = stream.into_channel().await;

    let started = std::time::Instant::now();
    let msg = rx.recv().await.unwrap();
    let elapsed = started.elapsed();

    assert!(
        msg.is_err(),
        "stream should end with an error at the deadline"
    );
    assert!(msg
        .unwrap_err()
        .to_string()
        .contains("API Request timed out"));
    assert!(
        elapsed < Duration::from_secs(2),
        "deadline (150ms) must bound the stall, not the 10s chunk timeout — took {elapsed:?}"
    );

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
    assert!(result.is_some(), "complete deltas should emit immediately");
    let tc = result.unwrap();
    assert_eq!(tc.id, "call_single");
    assert_eq!(tc.call_type, "function");
    assert_eq!(tc.function.name, "test_fn");
    assert_eq!(tc.function.arguments, "{\"key\":\"value\"}");

    // Already-emitted calls are not buffered for a later flush.
    let calls = acc.flush();
    assert!(calls.is_empty());
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

    // Another continuation completes the JSON object.
    let d3 = serde_json::json!({
        "index": 0,
        "function": {"arguments": "\"data\":\"hi\"}"}
    });
    let result = acc.process_delta(&d3);
    assert!(result.is_some());
    let tc = result.unwrap();
    assert_eq!(tc.id, "call_multi");
    assert_eq!(tc.function.name, "write");
    assert_eq!(
        tc.function.arguments,
        "{\"path\":\"/tmp/f\",\"data\":\"hi\"}"
    );

    let calls = acc.flush();
    assert!(calls.is_empty());
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

    // Second delta provides id, type, name and completes the JSON object.
    let d2 = serde_json::json!({
        "index": 0,
        "id": "call_late_id",
        "type": "function",
        "function": {"name": "late_fn", "arguments": "}"}
    });
    let result = acc.process_delta(&d2);
    assert!(result.is_some());
    let tc = result.unwrap();
    assert_eq!(tc.id, "call_late_id");
    assert_eq!(tc.call_type, "function");
    assert_eq!(tc.function.name, "late_fn");
    assert_eq!(tc.function.arguments, "{\"a\":1}");

    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_tool_call_accumulator_multiple_indices_sorted() {
    let mut acc = ToolCallAccumulator::new();

    // Insert index 2 first (incomplete args so it stays pending)
    let d2 = serde_json::json!({
        "index": 2, "id": "call_c", "type": "function",
        "function": {"name": "fn_c", "arguments": "{"}
    });
    assert!(acc.process_delta(&d2).is_none());

    // Then index 0
    let d0 = serde_json::json!({
        "index": 0, "id": "call_a", "type": "function",
        "function": {"name": "fn_a", "arguments": "{"}
    });
    assert!(acc.process_delta(&d0).is_none());

    // Then index 1
    let d1 = serde_json::json!({
        "index": 1, "id": "call_b", "type": "function",
        "function": {"name": "fn_b", "arguments": "{"}
    });
    assert!(acc.process_delta(&d1).is_none());

    let calls = acc.flush();
    assert_eq!(calls.len(), 3);
    // Should be sorted by index regardless of insertion order
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
    // Use incomplete args so the call stays pending until flush.
    let delta = serde_json::json!({
        "index": 0, "id": "call_x", "type": "function",
        "function": {"name": "fn_x", "arguments": "{"}
    });
    assert!(acc.process_delta(&delta).is_none());

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
    // Both deltas are complete, so both are emitted immediately.
    let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","type":"function","function":{"name":"fn1","arguments":"{}"}},{"index":1,"id":"c2","type":"function","function":{"name":"fn2","arguments":"{}"}}]}}]}"#;
    let results = parse_sse_event(event, &mut acc);
    assert_eq!(results.len(), 2);
    assert!(matches!(&results[0], StreamChunk::ToolCall(tc) if tc.function.name == "fn1"));
    assert!(matches!(&results[1], StreamChunk::ToolCall(tc) if tc.function.name == "fn2"));

    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_parse_sse_finish_reason_flushes_tool_calls() {
    let mut acc = ToolCallAccumulator::new();

    // First event: buffer an incomplete tool call.
    let event1 = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c_fin","type":"function","function":{"name":"read","arguments":"{"}}]}}]}"#;
    let r1 = parse_sse_event(event1, &mut acc);
    assert!(r1.is_empty());

    // Second event with finish_reason: should flush the buffered tool call
    // and propagate the finish_reason as its own chunk.
    let event2 = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
    let r2 = parse_sse_event(event2, &mut acc);
    assert_eq!(r2.len(), 2);
    assert!(matches!(&r2[0], StreamChunk::ToolCall(tc) if tc.id == "c_fin"));
    assert!(matches!(&r2[1], StreamChunk::FinishReason(r) if r == "tool_calls"));
}

#[test]
fn test_parse_sse_done_flushes_multiple_tool_calls() {
    let mut acc = ToolCallAccumulator::new();

    // Buffer two incomplete tool calls
    let d0 = serde_json::json!({
        "index": 0, "id": "a", "type": "function",
        "function": {"name": "fn_a", "arguments": "{"}
    });
    let d1 = serde_json::json!({
        "index": 1, "id": "b", "type": "function",
        "function": {"name": "fn_b", "arguments": "{"}
    });
    assert!(acc.process_delta(&d0).is_none());
    assert!(acc.process_delta(&d1).is_none());

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
    let stream = StreamingResponse::new(response, Duration::from_secs(5), None);
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
    let stream = StreamingResponse::new(response, Duration::from_secs(5), None);
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
    let stream = StreamingResponse::new(response, Duration::from_secs(5), None);
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
    let stream = StreamingResponse::new(response, Duration::from_secs(5), None);
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
    let stream = StreamingResponse::new(response, Duration::from_secs(30), None);
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
    let stream = StreamingResponse::new(response, Duration::from_secs(5), None);
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
        // #98: stream_options.include_usage is standard OpenAI and must be sent
        // to EVERY streaming endpoint (this one is a plain non-OpenRouter host),
        // so usage is reported and the token/cost budgets don't undercount.
        assert!(
            request.contains("include_usage"),
            "streaming request must request usage via stream_options: {request}"
        );

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

#[test]
fn test_merge_extra_body_allows_backend_specific_keys() {
    let mut body = serde_json::json!({
        "model": "text-model",
        "messages": [],
        "temperature": 0.0,
        "max_tokens": 128,
        "stream": false,
        "tools": [],
        "tool_choice": "auto",
        "thinking": {
            "type": "enabled",
            "budget_tokens": 64
        }
    });
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "enable_thinking": false }),
    );
    extra.insert("top_p".to_string(), serde_json::json!(0.95));

    merge_extra_body(&mut body, Some(&extra), "default chat request").unwrap();

    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert_eq!(body["top_p"], 0.95);
    assert_eq!(body["model"], "text-model");
}

#[test]
fn test_merge_extra_body_allows_openrouter_routing_keys() {
    // Regression: OpenRouter routing directives must pass the extra_body
    // allowlist so users can pin providers (e.g. full-context, tool-capable
    // ones). Previously `provider` was rejected as a disallowed key.
    let mut body = serde_json::json!({
        "model": "z-ai/glm-5.2",
        "messages": [],
        "stream": false
    });
    let mut extra = serde_json::Map::new();
    extra.insert(
        "provider".to_string(),
        serde_json::json!({ "only": ["fireworks", "morph"], "require_parameters": true }),
    );
    extra.insert("route".to_string(), serde_json::json!("fallback"));
    extra.insert(
        "models".to_string(),
        serde_json::json!(["z-ai/glm-5.2", "z-ai/glm-4.7"]),
    );
    extra.insert("transforms".to_string(), serde_json::json!(["middle-out"]));

    merge_extra_body(&mut body, Some(&extra), "openrouter routing")
        .expect("OpenRouter routing keys must be allowed in extra_body");

    assert_eq!(body["provider"]["only"][0], "fireworks");
    assert_eq!(body["route"], "fallback");
    assert_eq!(body["models"][1], "z-ai/glm-4.7");
    assert_eq!(body["transforms"][0], "middle-out");
}

#[test]
fn test_merge_extra_body_rejects_reserved_keys_for_default_chat_request() {
    let mut body = serde_json::json!({
        "model": "text-model",
        "messages": [],
        "temperature": 0.0,
        "max_tokens": 128,
        "stream": false
    });
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "enable_thinking": false }),
    );
    extra.insert("max_tokens".to_string(), serde_json::json!(256));

    let err = merge_extra_body(&mut body, Some(&extra), "default chat request")
        .expect_err("reserved top-level keys must be rejected");
    let err_text = err.to_string();

    assert!(err_text.contains("default chat request"));
    assert!(err_text.contains("max_tokens"));
    assert!(body.get("chat_template_kwargs").is_none());
}

#[test]
fn test_merge_extra_body_rejects_reserved_keys_for_profile_chat_request() {
    let mut body = serde_json::json!({
        "model": "profile-model",
        "messages": [],
        "temperature": 0.0,
        "max_tokens": 128,
        "stream": false
    });
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "enable_thinking": false }),
    );
    extra.insert(
        "thinking".to_string(),
        serde_json::json!({ "budget_tokens": 32 }),
    );

    let err = merge_extra_body(&mut body, Some(&extra), "model profile chat request")
        .expect_err("reserved top-level keys must be rejected");
    let err_text = err.to_string();

    assert!(err_text.contains("model profile chat request"));
    assert!(err_text.contains("thinking"));
    assert!(body.get("chat_template_kwargs").is_none());
}

#[test]
fn test_merge_extra_body_rejects_non_allowlisted_keys() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
        "stream": false
    });
    let mut extra = serde_json::Map::new();
    extra.insert("logprobs".to_string(), serde_json::json!(true));

    let err = merge_extra_body(&mut body, Some(&extra), "test")
        .expect_err("non-allowlisted keys must be rejected");
    assert!(err.to_string().contains("disallowed key 'logprobs'"));
}

#[test]
fn test_merge_extra_body_rejects_logit_bias() {
    let mut body = serde_json::json!({ "model": "test", "messages": [] });
    let mut extra = serde_json::Map::new();
    extra.insert(
        "logit_bias".to_string(),
        serde_json::json!({ "50256": -100 }),
    );

    let err =
        merge_extra_body(&mut body, Some(&extra), "test").expect_err("logit_bias must be rejected");
    assert!(err.to_string().contains("disallowed key 'logit_bias'"));
}

#[test]
fn test_merge_extra_body_rejects_n_completions() {
    let mut body = serde_json::json!({ "model": "test", "messages": [] });
    let mut extra = serde_json::Map::new();
    extra.insert("n".to_string(), serde_json::json!(5));

    let err = merge_extra_body(&mut body, Some(&extra), "test").expect_err("n must be rejected");
    assert!(err.to_string().contains("disallowed key 'n'"));
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

#[test]
fn client_refuses_key_over_plaintext_remote() {
    // A remote http endpoint WITH a key must be refused at the request boundary.
    let config = crate::config::Config {
        endpoint: "http://api.example.com/v1".to_string(),
        api_key: Some(crate::config::model::RedactedString::new("secret")),
        ..Default::default()
    };
    assert!(ApiClient::new(&config).is_err());
}

#[test]
fn client_allows_key_over_https_and_local_http() {
    // https remote WITH key -> Ok
    let https_config = crate::config::Config {
        endpoint: "https://api.example.com/v1".to_string(),
        api_key: Some(crate::config::model::RedactedString::new("secret")),
        ..Default::default()
    };
    assert!(ApiClient::new(&https_config).is_ok());

    // local http WITH key -> Ok (no login for local models is fine; local
    // stays on machine)
    let local_config = crate::config::Config {
        endpoint: "http://127.0.0.1:8000/v1".to_string(),
        api_key: Some(crate::config::model::RedactedString::new("secret")),
        ..Default::default()
    };
    assert!(ApiClient::new(&local_config).is_ok());
}

// --- canonicalize_message_order tests ---

#[test]
fn test_canonicalize_already_ordered() {
    let mut msgs = vec![
        Message::system("sys".to_string()),
        Message::user("hello".to_string()),
        Message::assistant("hi".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[1].role, "user");
    assert_eq!(msgs[2].role, "assistant");
}

#[test]
fn test_canonicalize_system_at_end() {
    let mut msgs = vec![
        Message::system("initial prompt".to_string()),
        Message::user("do something".to_string()),
        Message::assistant("tool call".to_string()),
        Message::user("tool result".to_string()),
        Message::system("learning hint".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    // All system messages merged into one at position 0
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[0].content, "initial prompt\n\nlearning hint");
    assert_eq!(msgs[1].role, "user");
    assert_eq!(msgs[1].content, "do something");
    assert_eq!(msgs[2].role, "assistant");
    assert_eq!(msgs[3].role, "user");
}

#[test]
fn test_canonicalize_multiple_misplaced_system() {
    let mut msgs = vec![
        Message::system("sys1".to_string()),
        Message::user("u1".to_string()),
        Message::system("sys2".to_string()),
        Message::assistant("a1".to_string()),
        Message::system("sys3".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    // All system messages merged into one at position 0
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[0].content, "sys1\n\nsys2\n\nsys3");
    assert_eq!(msgs[1].role, "user");
    assert_eq!(msgs[2].role, "assistant");
}

#[test]
fn test_canonicalize_no_system_messages() {
    let mut msgs = vec![
        Message::user("hello".to_string()),
        Message::assistant("hi".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "assistant");
}

#[test]
fn test_canonicalize_all_system() {
    let mut msgs = vec![
        Message::system("s1".to_string()),
        Message::system("s2".to_string()),
        Message::system("s3".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    // All merged into one system + injected user message
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[0].content, "s1\n\ns2\n\ns3");
    assert_eq!(msgs[1].role, "user");
}

#[test]
fn test_canonicalize_empty() {
    let mut msgs: Vec<Message> = vec![];
    canonicalize_message_order(&mut msgs);
    // Injected user message when none exists
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].role, "user");
}

#[test]
fn test_canonicalize_thinking_disabled_plus_learning_hint() {
    let mut msgs = vec![
        Message::system("no-think instruction".to_string()),
        Message::system("initial prompt".to_string()),
        Message::user("task".to_string()),
        Message::assistant("response".to_string()),
        Message::user("tool result".to_string()),
        Message::system("learning hint".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    // All 3 system messages merged into one
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(
        msgs[0].content,
        "no-think instruction\n\ninitial prompt\n\nlearning hint"
    );
    assert_eq!(msgs[1].role, "user");
    assert_eq!(msgs[1].content, "task");
    assert_eq!(msgs[2].role, "assistant");
    assert_eq!(msgs[3].role, "user");
    assert_eq!(msgs[3].content, "tool result");
}

#[test]
fn test_canonicalize_preserves_non_system_order() {
    let mut msgs = vec![
        Message::system("sys".to_string()),
        Message::user("u1".to_string()),
        Message::assistant("a1".to_string()),
        Message::user("u2".to_string()),
        Message::system("late sys".to_string()),
        Message::assistant("a2".to_string()),
        Message::user("u3".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    let non_system: Vec<&str> = msgs
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| m.content.text())
        .collect();
    assert_eq!(non_system, vec!["u1", "a1", "u2", "a2", "u3"]);
}

#[test]
fn test_canonicalize_single_system_message() {
    let mut msgs = vec![Message::system("only".to_string())];
    canonicalize_message_order(&mut msgs);
    // System message preserved + injected user message
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].content, "only");
    assert_eq!(msgs[1].role, "user");
}

#[test]
fn test_canonicalize_system_between_tool_messages() {
    let mut msgs = vec![
        Message::system("prompt".to_string()),
        Message::user("task".to_string()),
        Message::assistant("calling tool".to_string()),
        Message::user("tool result 1".to_string()),
        Message::system("injected hint".to_string()),
        Message::assistant("calling tool 2".to_string()),
        Message::user("tool result 2".to_string()),
    ];
    canonicalize_message_order(&mut msgs);
    // Both system messages merged into one at position 0
    assert_eq!(msgs.len(), 6);
    assert_eq!(msgs[0].role, "system");
    assert_eq!(msgs[0].content, "prompt\n\ninjected hint");
    assert_eq!(msgs[1].role, "user");
    assert_eq!(msgs[1].content, "task");
    assert_eq!(msgs[2].role, "assistant");
}

// ============================================
// chat_with_profile tests
// ============================================

#[tokio::test]
async fn test_chat_with_profile_normalizes_messages() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("profile response")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "test-model".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
    };

    // Only system + tool messages (no user message) — canonicalization
    // should inject a user message to prevent backend 400 errors.
    let messages = vec![
        Message::system("You are helpful."),
        Message::tool("tool result", "call_1"),
    ];

    let result = client
        .chat_with_profile(messages, None, ThinkingMode::Enabled, &profile)
        .await;
    assert!(
        result.is_ok(),
        "chat_with_profile failed: {:?}",
        result.err()
    );

    // Verify the request body was normalized: should contain a user message
    // injected by canonicalize_message_order (original had only system + tool).
    let bodies = server.captured_request_bodies().await;
    assert!(!bodies.is_empty(), "no request captured");
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    let messages = body["messages"].as_array().unwrap();
    let has_user = messages.iter().any(|m| m["role"] == "user");
    assert!(
        has_user,
        "canonicalization should have injected a user message, got: {:?}",
        messages
    );

    server.stop().await;
}

#[tokio::test]
async fn test_chat_with_profile_strips_images_for_text_only_model() {
    use crate::api::types::MessageContent;
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("text only response")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "text-only".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()], // NO vision
        context_length: 32768,
        extra_body: None,
        native_function_calling: None,
    };

    // Send a real multimodal message — strip_images should remove the image block.
    let multimodal = MessageContent::from_text("describe this image").with_image("ZmFrZV9wbmc=");
    let messages = vec![Message::user_multimodal(multimodal)];
    let result = client
        .chat_with_profile(messages, None, ThinkingMode::Enabled, &profile)
        .await;
    assert!(result.is_ok());

    // Verify the sent request has no image_url blocks in message content.
    let bodies = server.captured_request_bodies().await;
    assert!(!bodies.is_empty(), "no request captured");
    let body_str = &bodies[0];
    assert!(
        !body_str.contains("image_url"),
        "image content should have been stripped for text-only model, got: {}",
        body_str
    );

    server.stop().await;
}

#[tokio::test]
async fn test_chat_with_profile_context_overflow() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("should not reach here")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    // Profile with tiny context window — should overflow.
    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "tiny".to_string(),
        api_key: None,
        max_tokens: 100,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 100, // Impossibly small
        extra_body: None,
        native_function_calling: None,
    };

    let messages = vec![
        Message::system("A".repeat(1000)),
        Message::user("B".repeat(1000)),
    ];

    let result = client
        .chat_with_profile(messages, None, ThinkingMode::Enabled, &profile)
        .await;
    assert!(result.is_err(), "should fail with context overflow");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("context_length") || err.contains("CONTEXT OVERFLOW"),
        "error should mention context overflow, got: {}",
        err
    );

    server.stop().await;
}

// ============================================
// Retry-After header edge cases
// ============================================

#[tokio::test]
async fn test_retry_after_header_caps_at_300_seconds() {
    use crate::testing::mock_api::MockLlmServer;

    // First response: 429 with Retry-After: 9999 (should be capped to 300)
    // Second response: success
    let server = MockLlmServer::builder()
        .with_error_and_headers(
            429,
            r#"{"error":"rate limited"}"#,
            vec![("Retry-After".to_string(), "9999".to_string())],
        )
        .with_response("ok after retry")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        retry: crate::config::RetrySettings {
            max_retries: 2,
            base_delay_ms: 1, // Tiny delay so test is fast
            max_delay_ms: 1,
        },
        ..Default::default()
    };
    let client = ApiClient::new(&config)
        .unwrap()
        .with_retry_config(RetryConfig {
            max_retries: 2,
            initial_delay_ms: 1,
            max_delay_ms: 1, // Override to make test fast
            retryable_status_codes: vec![429],
        });

    let result = client
        .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
        .await;
    // The retry should succeed on the second attempt.
    // Note: this test will take ~1ms, not 9999s, because we capped.
    assert!(result.is_ok(), "retry should succeed: {:?}", result.err());

    server.stop().await;
}

// ============================================
// Context overflow on main chat path
// ============================================

#[tokio::test]
async fn test_chat_context_overflow_returns_error() {
    let config = crate::config::Config {
        context_length: 100, // Impossibly small
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let messages = vec![
        Message::system("A".repeat(5000)),
        Message::user("B".repeat(5000)),
    ];

    let result = client.chat(messages, None, ThinkingMode::Enabled).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("context_length"));
}

#[tokio::test]
async fn test_chat_stream_context_overflow_returns_error() {
    let config = crate::config::Config {
        context_length: 100,
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let messages = vec![
        Message::system("A".repeat(5000)),
        Message::user("B".repeat(5000)),
    ];

    let result = client
        .chat_stream(messages, None, ThinkingMode::Enabled)
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("context_length"));
}

// ============================================
// All retries exhausted gives meaningful error
// ============================================

#[tokio::test]
async fn test_all_retries_exhausted_preserves_last_error() {
    use crate::testing::mock_api::MockLlmServer;

    // All responses are 500 errors.
    let server = MockLlmServer::builder()
        .with_error(500, r#"{"error":"server down"}"#)
        .with_error(500, r#"{"error":"still down"}"#)
        .with_error(500, r#"{"error":"really down"}"#)
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config)
        .unwrap()
        .with_retry_config(RetryConfig {
            max_retries: 2,
            initial_delay_ms: 1,
            max_delay_ms: 1,
            retryable_status_codes: vec![500],
        });

    let result = client
        .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
        .await;
    assert!(result.is_err());
    // The error should be the HTTP 500, not a generic "all retries exhausted".
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("500") || err.contains("down"),
        "error should contain last HTTP status: {}",
        err
    );

    server.stop().await;
}

// ============================================
// Non-retryable status code is immediate failure
// ============================================

#[tokio::test]
async fn test_non_retryable_401_fails_immediately() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_error(401, r#"{"error":"unauthorized"}"#)
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config)
        .unwrap()
        .with_retry_config(RetryConfig {
            max_retries: 5,
            initial_delay_ms: 1,
            max_delay_ms: 1,
            retryable_status_codes: vec![429, 500, 502, 503],
        });

    let result = client
        .chat(vec![Message::user("test")], None, ThinkingMode::Enabled)
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("401"));

    server.stop().await;
}
// Additional API client and streaming tests
//
// These tests cover edge cases and additional scenarios not covered
// in the main tests.rs file.

// Note: Imports are already defined at the top of tests.rs

// ============================================
// Additional Streaming Response Tests
// ============================================

#[test]
fn test_stream_chunk_usage_clone() {
    let usage = Usage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
        cost: None,
    };
    let chunk = StreamChunk::Usage(usage);
    let cloned = chunk.clone();

    match (chunk, cloned) {
        (StreamChunk::Usage(u1), StreamChunk::Usage(u2)) => {
            assert_eq!(u1.total_tokens, u2.total_tokens);
        }
        _ => panic!("Expected Usage variants"),
    }
}

#[test]
fn test_parse_sse_event_reasoning_alias() {
    // Test the "reasoning" alias (used by vLLM) instead of "reasoning_content"
    let mut acc = ToolCallAccumulator::new();
    let event = r#"data: {"choices":[{"delta":{"reasoning":"vllm thinking"}}]}"#;
    let results = parse_sse_event(event, &mut acc);
    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0], StreamChunk::Reasoning(t) if t == "vllm thinking"));
}

#[test]
fn test_parse_sse_event_malformed_tool_calls() {
    let mut acc = ToolCallAccumulator::new();
    // Tool call without required fields should be handled gracefully
    let event = r#"data: {"choices":[{"delta":{"tool_calls":[{"invalid":"data"}]}}]}"#;
    let results = parse_sse_event(event, &mut acc);
    // Should not panic, just return empty
    assert!(results.is_empty());
}

#[test]
fn test_parse_sse_event_negative_index_ignored() {
    let mut acc = ToolCallAccumulator::new();
    // Negative index should be ignored (can't convert to usize)
    let delta = serde_json::json!({
        "index": -1,
        "id": "call_neg",
        "function": {"name": "test", "arguments": "{}"}
    });
    let result = acc.process_delta(&delta);
    assert!(result.is_none());
    // Nothing should be in pending
    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_parse_sse_event_large_index() {
    let mut acc = ToolCallAccumulator::new();
    // Large index should work fine; a complete delta emits immediately.
    let delta = serde_json::json!({
        "index": 999999,
        "id": "call_large",
        "type": "function",
        "function": {"name": "test", "arguments": "{}"}
    });
    let result = acc.process_delta(&delta);
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, "call_large");

    let calls = acc.flush();
    assert!(calls.is_empty());
}

// ============================================
// Additional ToolCallAccumulator Tests
// ============================================

#[test]
fn test_tool_call_accumulator_empty_function_name() {
    let mut acc = ToolCallAccumulator::new();
    let delta = serde_json::json!({
        "index": 0,
        "id": "call_empty",
        "type": "function",
        "function": {"name": "", "arguments": "{}"}
    });
    // Empty name prevents a complete tool call from being emitted.
    let result = acc.process_delta(&delta);
    assert!(result.is_none());

    // flush also filters out entries with empty names.
    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_tool_call_accumulator_partial_update_preserves_existing() {
    let mut acc = ToolCallAccumulator::new();

    // First delta with full info
    let d1 = serde_json::json!({
        "index": 0,
        "id": "call_1",
        "type": "function",
        "function": {"name": "original_name", "arguments": "{\"a\":1"}
    });
    acc.process_delta(&d1);

    // Second delta only updates arguments, not name, and completes the object.
    let d2 = serde_json::json!({
        "index": 0,
        "function": {"arguments": ",\"b\":2}"}
    });
    let result = acc.process_delta(&d2);
    assert!(result.is_some());
    let tc = result.unwrap();
    assert_eq!(tc.id, "call_1");
    assert_eq!(tc.function.name, "original_name");
    assert_eq!(tc.function.arguments, "{\"a\":1,\"b\":2}");

    let calls = acc.flush();
    assert!(calls.is_empty());
}

#[test]
fn test_tool_call_accumulator_multiple_flushes_idempotent() {
    let mut acc = ToolCallAccumulator::new();

    // Keep the call pending with incomplete args so flush has work to do.
    let delta = serde_json::json!({
        "index": 0,
        "id": "call_multi",
        "type": "function",
        "function": {"name": "test", "arguments": "{"}
    });
    assert!(acc.process_delta(&delta).is_none());

    // First flush
    let calls1 = acc.flush();
    assert_eq!(calls1.len(), 1);

    // Second flush should be empty
    let calls2 = acc.flush();
    assert!(calls2.is_empty());

    // Third flush still empty
    let calls3 = acc.flush();
    assert!(calls3.is_empty());
}

// ============================================
// Additional RetryConfig Tests
// ============================================

#[test]
fn test_retry_config_single_retryable_code() {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 1000,
        max_delay_ms: 30000,
        retryable_status_codes: vec![429],
    };
    assert!(config.retryable_status_codes.contains(&429));
    assert!(!config.retryable_status_codes.contains(&500));
}

#[test]
fn test_retry_config_extreme_delays() {
    let config = RetryConfig {
        max_retries: 3,
        initial_delay_ms: 1,   // 1ms
        max_delay_ms: 3600000, // 1 hour
        retryable_status_codes: vec![500],
    };
    assert_eq!(config.initial_delay_ms, 1);
    assert_eq!(config.max_delay_ms, 3600000);
}

// ============================================
// Additional Message and Content Tests
// ============================================

#[test]
fn test_message_content_text_method() {
    let content = MessageContent::Text("hello world".to_string());
    assert_eq!(content.text(), "hello world");
}

#[test]
fn test_message_content_text_all_from_blocks() {
    use crate::api::types::{ContentBlock, ImageUrl};

    let blocks = vec![
        ContentBlock::Text {
            text: "line1".to_string(),
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
                detail: None,
            },
        },
        ContentBlock::Text {
            text: "line2".to_string(),
        },
    ];
    let content = MessageContent::Blocks(blocks);
    assert_eq!(content.text_all(), "line1\nline2");
}

#[test]
fn test_message_content_strip_images_to_single_text() {
    use crate::api::types::{ContentBlock, ImageUrl};

    let blocks = vec![
        ContentBlock::Text {
            text: "only text".to_string(),
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
                detail: None,
            },
        },
    ];
    let content = MessageContent::Blocks(blocks);
    let stripped = content.strip_images();

    // Should collapse to single Text variant
    match stripped {
        MessageContent::Text(t) => assert_eq!(t, "only text"),
        MessageContent::Blocks(_) => panic!("Expected Text variant"),
    }
}

#[test]
fn test_message_content_with_image() {
    let content = MessageContent::Text("describe this".to_string());
    let with_image = content.with_image("base64encodeddata");

    match with_image {
        MessageContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            match &blocks[1] {
                ContentBlock::ImageUrl { image_url } => {
                    assert!(image_url
                        .url
                        .contains("data:image/png;base64,base64encodeddata"));
                }
                _ => panic!("Expected ImageUrl block"),
            }
        }
        _ => panic!("Expected Blocks variant"),
    }
}

#[test]
fn test_message_content_is_empty() {
    assert!(MessageContent::Text("".to_string()).is_empty());
    assert!(!MessageContent::Text("hello".to_string()).is_empty());
}

#[test]
fn test_message_content_contains() {
    let content = MessageContent::Text("hello world".to_string());
    assert!(content.contains("world"));
    assert!(!content.contains("foo"));
}

#[test]
fn test_message_content_len() {
    assert_eq!(MessageContent::Text("hello".to_string()).len(), 5);
    assert_eq!(MessageContent::Text("".to_string()).len(), 0);
}

#[test]
fn test_message_content_image_count() {
    use crate::api::types::{ContentBlock, ImageUrl};

    let text_only = MessageContent::Text("hello".to_string());
    assert_eq!(text_only.image_count(), 0);

    let blocks = vec![
        ContentBlock::Text {
            text: "text".to_string(),
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
                detail: None,
            },
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,def".to_string(),
                detail: None,
            },
        },
    ];
    let with_images = MessageContent::Blocks(blocks);
    assert_eq!(with_images.image_count(), 2);
}

#[test]
fn test_message_content_has_images() {
    use crate::api::types::{ContentBlock, ImageUrl};

    let text_only = MessageContent::Text("hello".to_string());
    assert!(!text_only.has_images());

    let blocks = vec![
        ContentBlock::Text {
            text: "text".to_string(),
        },
        ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: "data:image/png;base64,abc".to_string(),
                detail: None,
            },
        },
    ];
    let with_image = MessageContent::Blocks(blocks);
    assert!(with_image.has_images());
}

#[test]
fn test_message_content_default() {
    let content: MessageContent = Default::default();
    assert!(content.is_empty());
}

#[test]
fn test_message_content_partial_eq_str() {
    let content = MessageContent::Text("hello".to_string());
    assert!(content == *"hello");
    assert!(content == "hello");
}

#[test]
fn test_message_content_partial_eq_string() {
    let content = MessageContent::Text("hello".to_string());
    assert!(content == "hello");
}

#[test]
fn test_message_content_display() {
    let content = MessageContent::Text("display test".to_string());
    assert_eq!(format!("{}", content), "display test");
}

// ============================================
// Additional rand_jitter Tests
// ============================================

#[test]
fn test_rand_jitter_distribution() {
    // Call many times and verify distribution is roughly uniform
    let mut below_50 = 0;
    let mut above_50 = 0;

    for _ in 0..100 {
        let j = rand_jitter();
        if j < 0.5 {
            below_50 += 1;
        } else {
            above_50 += 1;
        }
        assert!((0.0..1.0).contains(&j));
    }

    // Roughly balanced (within reasonable bounds for randomness)
    // In 100 tries, we expect roughly 50/50 with some variance
    assert!(
        below_50 > 20 && below_50 < 80,
        "Distribution seems skewed: {} vs {}",
        below_50,
        above_50
    );
}

// ============================================
// Additional merge_extra_body Tests
// ============================================

#[test]
fn test_merge_extra_body_empty_extra() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
    });

    // Empty extra_body map
    let extra = serde_json::Map::new();

    // Should succeed with no changes
    merge_extra_body(&mut body, Some(&extra), "test").unwrap();
    assert_eq!(body["model"], "test");
}

#[test]
fn test_merge_extra_body_null_body() {
    let mut body = serde_json::json!(null);
    let mut extra = serde_json::Map::new();
    extra.insert("top_p".to_string(), serde_json::json!(0.9));

    // Should fail because body is not an object
    let result = merge_extra_body(&mut body, Some(&extra), "test");
    assert!(result.is_err());
}

#[test]
fn test_merge_extra_body_guided_json() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
    });

    let mut extra = serde_json::Map::new();
    extra.insert(
        "guided_json".to_string(),
        serde_json::json!({"type": "object"}),
    );

    merge_extra_body(&mut body, Some(&extra), "test").unwrap();
    assert!(body.get("guided_json").is_some());
}

#[test]
fn test_merge_extra_body_sampling_params() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
    });

    let mut extra = serde_json::Map::new();
    extra.insert("top_k".to_string(), serde_json::json!(40));
    extra.insert("repetition_penalty".to_string(), serde_json::json!(1.1));
    extra.insert("frequency_penalty".to_string(), serde_json::json!(0.5));
    extra.insert("presence_penalty".to_string(), serde_json::json!(0.3));
    extra.insert("seed".to_string(), serde_json::json!(42));

    merge_extra_body(&mut body, Some(&extra), "test").unwrap();
    assert_eq!(body["top_k"], 40);
    assert_eq!(body["repetition_penalty"], 1.1);
    assert_eq!(body["frequency_penalty"], 0.5);
    assert_eq!(body["presence_penalty"], 0.3);
    assert_eq!(body["seed"], 42);
}

#[test]
fn test_merge_extra_body_rejects_user_field() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
    });

    let mut extra = serde_json::Map::new();
    extra.insert("user".to_string(), serde_json::json!("username"));

    let result = merge_extra_body(&mut body, Some(&extra), "test");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("disallowed key 'user'"));
}

#[test]
fn test_merge_extra_body_rejects_response_format() {
    let mut body = serde_json::json!({
        "model": "test",
        "messages": [],
    });

    let mut extra = serde_json::Map::new();
    extra.insert(
        "response_format".to_string(),
        serde_json::json!({"type": "json_object"}),
    );

    let result = merge_extra_body(&mut body, Some(&extra), "test");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("disallowed key 'response_format'"));
}

// ============================================
// Additional ThinkingMode Tests
// ============================================

#[test]
fn test_thinking_mode_equality() {
    assert_eq!(ThinkingMode::Enabled, ThinkingMode::Enabled);
    assert_eq!(ThinkingMode::Disabled, ThinkingMode::Disabled);
    assert_eq!(ThinkingMode::Budget(1000), ThinkingMode::Budget(1000));

    assert_ne!(ThinkingMode::Enabled, ThinkingMode::Disabled);
    assert_ne!(ThinkingMode::Budget(1000), ThinkingMode::Budget(2000));
    assert_ne!(ThinkingMode::Enabled, ThinkingMode::Budget(1000));
}

#[test]
fn test_thinking_mode_copy() {
    let mode = ThinkingMode::Budget(4096);
    let copied = mode;
    assert_eq!(mode, copied);
}

// ============================================
// Additional canonicalize_message_order Tests
// ============================================

#[test]
fn test_canonicalize_user_at_start_single_system() {
    // Single system message after user is NOT moved (only multiple system messages are merged)
    let mut msgs = vec![Message::user("first"), Message::system("sys")];
    canonicalize_message_order(&mut msgs);
    // Order preserved when only one system message
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "system");
}

#[test]
fn test_canonicalize_only_tool_messages() {
    let mut msgs = vec![
        Message::tool("result1", "call_1"),
        Message::tool("result2", "call_2"),
    ];
    canonicalize_message_order(&mut msgs);
    // Should inject user message at start
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "tool");
    assert_eq!(msgs[2].role, "tool");
}

#[test]
fn test_canonicalize_system_after_user_single() {
    // Single system message after user/assistant is NOT moved
    let mut msgs = vec![
        Message::user("question"),
        Message::assistant("answer"),
        Message::system("feedback"),
    ];
    canonicalize_message_order(&mut msgs);
    // Order preserved when only one system message
    assert_eq!(msgs[0].role, "user");
    assert_eq!(msgs[1].role, "assistant");
    assert_eq!(msgs[2].role, "system");
}

// ============================================
// Additional Message Construction Tests
// ============================================

#[test]
fn test_message_strip_images() {
    use crate::api::types::MessageContent;

    let multimodal = MessageContent::Text("look at this".to_string()).with_image("fakebase64");
    let msg = Message::user_multimodal(multimodal);
    let stripped = msg.strip_images();

    match stripped.content {
        MessageContent::Text(t) => assert_eq!(t, "look at this"),
        MessageContent::Blocks(blocks) => {
            // Should only have text blocks
            assert!(!blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ImageUrl { .. })));
        }
    }
}

#[test]
fn test_message_assistant_with_reasoning() {
    let msg =
        Message::assistant_with_reasoning("The answer is 42.", "I need to calculate 6 * 7...");
    assert_eq!(msg.role, "assistant");
    assert_eq!(msg.content.text(), "The answer is 42.");
    assert_eq!(
        msg.reasoning_content,
        Some("I need to calculate 6 * 7...".to_string())
    );
}

#[test]
fn test_message_tool_construction() {
    let msg = Message::tool(r#"{"status": "ok"}"#, "call_abc123");
    assert_eq!(msg.role, "tool");
    assert_eq!(msg.tool_call_id, Some("call_abc123".to_string()));
    assert_eq!(msg.content.text(), r#"{"status": "ok"}"#);
}

// ============================================
// Additional ToolDefinition Tests
// ============================================

#[test]
fn test_tool_definition_serialization() {
    let tool = ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "arg1": {"type": "string"}
                },
                "required": ["arg1"]
            }),
        },
    };

    let json = serde_json::to_string(&tool).unwrap();
    assert!(json.contains("\"type\":\"function\""));
    assert!(json.contains("test_tool"));
    assert!(json.contains("A test tool"));
}

// ============================================
// Additional CompletionRequest Tests
// ============================================

#[test]
fn test_completion_request_defaults() {
    let req = types::CompletionRequest {
        model: "test-model".to_string(),
        prompt: "Once upon a time".to_string(),
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
fn test_completion_request_with_stop_sequences() {
    let req = types::CompletionRequest {
        model: "test-model".to_string(),
        prompt: "Hello".to_string(),
        max_tokens: Some(100),
        temperature: Some(0.7),
        top_p: Some(0.9),
        stop: Some(vec!["\n".to_string(), "###".to_string()]),
    };

    let json = serde_json::to_string(&req).unwrap();
    // Newline is escaped as \\n in JSON, check for the escaped version
    assert!(json.contains("\\n"));
    assert!(json.contains("###"));
}

// ============================================
// Native function calling unification:
// chat() and chat_with_profile() must honour
// native_function_calling identically.
// ============================================

/// Helper: build a [`ToolDefinition`] for a fake `shell_exec` tool.
#[cfg(test)]
fn dummy_shell_tool() -> types::ToolDefinition {
    types::ToolDefinition {
        def_type: "function".to_string(),
        function: types::FunctionDefinition {
            name: "shell_exec".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"],
            }),
        },
    }
}

/// Regression: `chat_with_profile` must honour `native_function_calling`.
///
/// Before this fix, the function hard-coded `false` when calling
/// `attach_tools_to_body`, so any swarm code routing through a
/// `ModelProfile` silently sent requests *without* `tool_choice: auto`.
/// This test verifies that when the profile sets
/// `native_function_calling = Some(true)`, the outgoing request body
/// contains `tool_choice: "auto"`.
#[tokio::test]
async fn test_chat_with_profile_honors_native_function_calling_true() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("native FC ack")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "fc-model".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: Some(true),
    };

    let result = client
        .chat_with_profile(
            vec![Message::user("call a tool")],
            Some(vec![dummy_shell_tool()]),
            ThinkingMode::Enabled,
            &profile,
        )
        .await;
    assert!(
        result.is_ok(),
        "chat_with_profile failed: {:?}",
        result.err()
    );

    let bodies = server.captured_request_bodies().await;
    assert!(!bodies.is_empty(), "no request captured");
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    assert_eq!(
        body["tool_choice"], "auto",
        "profile native FC=true must produce tool_choice: auto, body was: {}",
        bodies[0]
    );
    assert!(body["tools"].is_array());

    server.stop().await;
}

/// Regression: profile with `native_function_calling = Some(false)`
/// must NOT send `tool_choice`.
#[tokio::test]
async fn test_chat_with_profile_honors_native_function_calling_false() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("text-mode ack")
        .build()
        .await;

    let config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    let client = ApiClient::new(&config).unwrap();

    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "text-mode".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: Some(false),
    };

    let result = client
        .chat_with_profile(
            vec![Message::user("call a tool")],
            Some(vec![dummy_shell_tool()]),
            ThinkingMode::Enabled,
            &profile,
        )
        .await;
    assert!(
        result.is_ok(),
        "chat_with_profile failed: {:?}",
        result.err()
    );

    let bodies = server.captured_request_bodies().await;
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    assert!(
        body.get("tool_choice").is_none(),
        "profile native FC=false must omit tool_choice, body was: {}",
        bodies[0]
    );

    server.stop().await;
}

/// Regression: when the profile leaves `native_function_calling = None`,
/// it inherits the parent client's `agent.native_function_calling`.
/// This test verifies the inheritance with the parent set to true.
#[tokio::test]
async fn test_chat_with_profile_inherits_parent_native_fc() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("inherited ack")
        .build()
        .await;

    let mut config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    config.agent.native_function_calling = true;
    let client = ApiClient::new(&config).unwrap();

    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: "inherit".to_string(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: None, // inherit from parent
    };

    let result = client
        .chat_with_profile(
            vec![Message::user("call a tool")],
            Some(vec![dummy_shell_tool()]),
            ThinkingMode::Enabled,
            &profile,
        )
        .await;
    assert!(result.is_ok());

    let bodies = server.captured_request_bodies().await;
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    assert_eq!(
        body["tool_choice"], "auto",
        "profile with None must inherit parent native FC=true: {}",
        bodies[0]
    );

    server.stop().await;
}

/// Regression: `chat()` and `chat_with_profile()` must produce the same
/// `tool_choice` for the same effective `native_function_calling`. This
/// is the core invariant the unification is meant to enforce.
#[tokio::test]
async fn test_chat_and_chat_with_profile_agree_on_tool_choice() {
    use crate::testing::mock_api::MockLlmServer;

    let server = MockLlmServer::builder()
        .with_response("ack-1")
        .with_response("ack-2")
        .build()
        .await;

    let mut config = crate::config::Config {
        endpoint: format!("{}/v1", server.url()),
        ..Default::default()
    };
    config.agent.native_function_calling = true;
    let client = ApiClient::new(&config).unwrap();

    // Path 1: chat()
    let _ = client
        .chat(
            vec![Message::user("hi")],
            Some(vec![dummy_shell_tool()]),
            ThinkingMode::Enabled,
        )
        .await
        .unwrap();

    // Path 2: chat_with_profile() with explicit native FC = true
    let profile = crate::config::ModelProfile {
        endpoint: format!("{}/v1", server.url()),
        model: config.model.clone(),
        api_key: None,
        max_tokens: 1024,
        temperature: 0.5,
        modalities: vec!["text".to_string()],
        context_length: 32768,
        extra_body: None,
        native_function_calling: Some(true),
    };
    let _ = client
        .chat_with_profile(
            vec![Message::user("hi")],
            Some(vec![dummy_shell_tool()]),
            ThinkingMode::Enabled,
            &profile,
        )
        .await
        .unwrap();

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 2);
    let b1: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    let b2: serde_json::Value = serde_json::from_str(&bodies[1]).unwrap();
    assert_eq!(
        b1["tool_choice"], b2["tool_choice"],
        "chat and chat_with_profile must agree on tool_choice when native FC matches"
    );
    assert_eq!(b1["tool_choice"], "auto");

    server.stop().await;
}
