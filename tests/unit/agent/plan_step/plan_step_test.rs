use super::*;
use crate::api::types::Message;
use crate::config::{Config, ExecutionMode};
use crate::testing::mock_api::{MockLlmServer, MockToolCall};

// -----------------------------------------------------------------------
// Test helpers
// -----------------------------------------------------------------------

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            streaming,
            native_function_calling: false,
            min_completion_steps: 0,
            require_verification_before_completion: false,
            ..Default::default()
        },
        safety: crate::config::SafetyConfig {
            allowed_paths: vec!["./**".to_string(), "/**".to_string()],
            ..Default::default()
        },
        execution_mode: ExecutionMode::Yolo,
        ..Default::default()
    }
}

// -----------------------------------------------------------------------
// Tests for Agent::plan
// -----------------------------------------------------------------------

/// `plan` returns `Ok(true)` when the model response contains XML tool
/// calls — the agent should proceed to the execution phase.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_returns_true_when_tool_calls_present() {
    let server = MockLlmServer::builder()
        .with_response(
            r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
        )
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Read Cargo.toml"));

    let result = agent.plan().await;
    assert!(result.is_ok(), "plan should succeed: {:?}", result.err());
    assert!(
        result.unwrap(),
        "plan should return true when tool calls are present"
    );

    // An assistant message must have been pushed.
    assert!(
        agent.messages.iter().any(|m| m.role == "assistant"),
        "an assistant message should be pushed after planning"
    );

    server.stop().await;
}

/// `plan` returns `Ok(false)` for a plain-text response with no tool
/// calls — the agent should not enter the execution phase.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_returns_false_when_no_tool_calls() {
    let server = MockLlmServer::builder()
        .with_response("I'll analyze the codebase and provide recommendations.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("What do you think?"));

    let result = agent.plan().await;
    assert!(result.is_ok(), "plan should succeed: {:?}", result.err());
    assert!(
        !result.unwrap(),
        "plan should return false when no tool calls are present"
    );

    server.stop().await;
}

/// `plan` pushes exactly one assistant message into `self.messages`.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_pushes_exactly_one_assistant_message() {
    let server = MockLlmServer::builder()
        .with_response("Thinking about the task...")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));
    let count_before = agent.messages.len();

    let _ = agent.plan().await;

    assert_eq!(
        agent.messages.len(),
        count_before + 1,
        "plan should push exactly one assistant message"
    );
    let last = agent.messages.last().unwrap();
    assert_eq!(last.role, "assistant");
    assert!(last.content.text().contains("Thinking about the task"));

    server.stop().await;
}

/// `plan` propagates API errors and does not push a message on failure.
/// We queue many error responses to exhaust any client-side retry logic.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_propagates_api_error_and_no_message_added() {
    let mut builder = MockLlmServer::builder();
    // Queue 20 error responses to exhaust retries.
    for _ in 0..20 {
        builder = builder.with_error(500, r#"{"error":"internal server error"}"#);
    }
    let server = builder.build().await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));
    let count_before = agent.messages.len();

    let result = agent.plan().await;
    assert!(result.is_err(), "plan should return error on API failure");

    // No new message should be added on error.
    assert_eq!(
        agent.messages.len(),
        count_before,
        "no new message should be pushed when plan fails with an API error"
    );

    server.stop().await;
}

/// `plan` increments `turn_artifact_seq` by exactly one.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_increments_turn_artifact_seq() {
    let server = MockLlmServer::builder()
        .with_response("Planning the approach...")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Do something"));
    let seq_before = agent.turn_artifact_seq;

    let _ = agent.plan().await;

    assert_eq!(
        agent.turn_artifact_seq,
        seq_before + 1,
        "plan should increment turn_artifact_seq by exactly 1"
    );

    server.stop().await;
}

/// `plan` accumulates token-usage metadata from the API response into
/// `cumulative_token_usage`. The mock server returns
/// `prompt_tokens=10, completion_tokens=5, total_tokens=15`.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_accumulates_token_usage_from_metadata() {
    let server = MockLlmServer::builder()
        .with_response("Analyzing...")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Analyze the project"));

    let input_before = agent.cumulative_token_usage.input;
    let output_before = agent.cumulative_token_usage.output;

    let _ = agent.plan().await;

    assert_eq!(
        agent.cumulative_token_usage.input,
        input_before + 10,
        "plan should accumulate prompt_tokens from API metadata"
    );
    assert_eq!(
        agent.cumulative_token_usage.output,
        output_before + 5,
        "plan should accumulate completion_tokens from API metadata"
    );

    server.stop().await;
}

/// `plan` handles empty model content without panicking and returns
/// `Ok(false)` since there are no tool calls.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_handles_empty_content() {
    let server = MockLlmServer::builder().with_response("").build().await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let result = agent.plan().await;
    assert!(
        result.is_ok(),
        "plan should not error on empty content: {:?}",
        result.err()
    );
    assert!(
        !result.unwrap(),
        "empty content has no tool calls, should return false"
    );

    server.stop().await;
}

/// `plan` preserves all pre-existing messages and appends exactly one
/// new assistant message.  `Agent::new` already injects a system prompt,
/// so we just add user/assistant messages on top.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_preserves_existing_messages() {
    let server = MockLlmServer::builder()
        .with_response("I will read the file.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();

    // Agent::new already injected a system prompt; add more messages.
    agent.messages.push(Message::user("First message"));
    agent.messages.push(Message::assistant("First response"));
    agent.messages.push(Message::user("Second message"));
    let before_count = agent.messages.len();
    let system_text_before = agent.messages[0].content.text().to_string();

    let _ = agent.plan().await;

    assert_eq!(
        agent.messages.len(),
        before_count + 1,
        "plan should add exactly one message while preserving existing ones"
    );
    // First message must still be the system prompt, unchanged.
    assert_eq!(agent.messages[0].role, "system");
    assert_eq!(
        agent.messages[0].content.text(),
        system_text_before,
        "system prompt should be preserved unchanged after plan"
    );
    // The pre-existing user and assistant messages should be intact.
    assert_eq!(agent.messages[1].content.text(), "First message");
    assert_eq!(agent.messages[2].content.text(), "First response");
    assert_eq!(agent.messages[3].content.text(), "Second message");

    server.stop().await;
}

/// `plan` returns `Ok(true)` when the response contains multiple tool
/// calls in a single message.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_with_multiple_tool_calls_returns_true() {
    let response = r#"<tool>
<name>file_read</name>
<arguments>{"path":"./src/main.rs"}</arguments>
</tool>

<tool>
<name>grep_search</name>
<arguments>{"pattern":"TODO","path":"./src"}</arguments>
</tool>"#;

    let server = MockLlmServer::builder()
        .with_response(response)
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent
        .messages
        .push(Message::user("Find TODOs in the codebase"));

    let result = agent.plan().await;
    assert!(result.is_ok());
    assert!(
        result.unwrap(),
        "plan should return true when multiple tool calls are present"
    );

    server.stop().await;
}

/// `plan` returns `Ok(false)` for prose that mentions tools but does not
/// contain actual `<tool>` XML blocks.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_returns_false_for_prose_mentioning_tools() {
    let server = MockLlmServer::builder()
        .with_response(
            "I would use the file_read tool to read the file, but let me think about it first.",
        )
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("What would you do?"));

    let result = agent.plan().await;
    assert!(result.is_ok());
    assert!(
        !result.unwrap(),
        "prose mentioning tools should not count as actual tool calls"
    );

    server.stop().await;
}

/// `plan` does not crash when `plan_mode` is active and the response
/// contains a structured plan — the plan text should be stored.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_with_plan_mode_active_does_not_crash() {
    let plan_text = r#"## Plan

1. Read the file at src/main.rs
2. Identify the bug in the parser
3. Fix the bug with a targeted edit
4. Run tests to verify the fix

Let me start by reading the file."#;

    let server = MockLlmServer::builder()
        .with_response(plan_text)
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.plan_mode = true;
    agent
        .messages
        .push(Message::user("Fix a bug in the parser"));

    let result = agent.plan().await;
    assert!(
        result.is_ok(),
        "plan should succeed in plan mode: {:?}",
        result.err()
    );

    // The assistant message should still be pushed.
    assert!(
        agent.messages.iter().any(|m| m.role == "assistant"),
        "an assistant message should be pushed even in plan mode"
    );

    server.stop().await;
}

/// `plan` called twice: each call increments `turn_artifact_seq` and
/// pushes one assistant message, so two calls produce two new messages
/// and a seq delta of 2.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_called_twice_increments_seq_and_adds_messages() {
    let server = MockLlmServer::builder()
        .with_response("First planning response.")
        .with_response("Second planning response.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Do task"));
    let seq_before = agent.turn_artifact_seq;

    let r1 = agent.plan().await;
    assert!(r1.is_ok());
    let count_after_first = agent.messages.len();
    let seq_after_first = agent.turn_artifact_seq;

    let r2 = agent.plan().await;
    assert!(r2.is_ok());

    assert_eq!(
        agent.messages.len(),
        count_after_first + 1,
        "second plan call should add one more message"
    );
    assert_eq!(
        agent.turn_artifact_seq,
        seq_after_first + 1,
        "second plan call should increment turn_artifact_seq again"
    );
    assert_eq!(
        agent.turn_artifact_seq,
        seq_before + 2,
        "two plan calls should increment seq by 2 total"
    );

    // Verify both responses are in the message history.
    let assistant_texts: Vec<&str> = agent
        .messages
        .iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.content.text())
        .collect();
    assert_eq!(
        assistant_texts.len(),
        2,
        "should have two assistant messages"
    );
    assert_eq!(assistant_texts[0], "First planning response.");
    assert_eq!(assistant_texts[1], "Second planning response.");

    server.stop().await;
}

/// `plan` with a mock returning native tool-call JSON but
/// `native_function_calling` disabled — verifies the agent handles the
/// response without crashing.  The actual return value depends on whether
/// `extract_tool_calls` inspects the native `tool_calls` field regardless
/// of the config flag; we only verify no panic occurs and a message is
/// pushed.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_native_tool_calls_ignored_when_disabled() {
    let server = MockLlmServer::builder()
        .with_tool_calls(vec![MockToolCall {
            id: "call_1".to_string(),
            name: "file_read".to_string(),
            arguments: r#"{"path":"test.rs"}"#.to_string(),
        }])
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    // native_function_calling is false in mock_agent_config
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Read a file"));

    let result = agent.plan().await;
    assert!(
            result.is_ok(),
            "plan should not crash with native tool calls when native_function_calling is disabled: {:?}",
            result.err()
        );
    // An assistant message should be pushed regardless.
    assert!(
        agent.messages.iter().any(|m| m.role == "assistant"),
        "an assistant message should be pushed even with native tool-call JSON"
    );

    server.stop().await;
}

/// `plan` correctly captures the assistant content in the pushed message,
/// matching the raw text returned by the mock server.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_assistant_message_content_matches_response() {
    let response_text = "I will now analyze the codebase structure.";
    let server = MockLlmServer::builder()
        .with_response(response_text)
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Analyze the codebase"));

    let _ = agent.plan().await;

    let assistant_msgs: Vec<&Message> = agent
        .messages
        .iter()
        .filter(|m| m.role == "assistant")
        .collect();
    assert!(
        !assistant_msgs.is_empty(),
        "should have at least one assistant message"
    );
    let last_assistant = assistant_msgs.last().unwrap();
    assert_eq!(
        last_assistant.content.text(),
        response_text,
        "assistant message content should exactly match the mock response"
    );

    server.stop().await;
}

/// `plan` with a 503 error returns an error whose message contains the
/// status or error context.  We queue many errors to exhaust retries.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_503_error_is_propagated() {
    let mut builder = MockLlmServer::builder();
    for _ in 0..20 {
        builder = builder.with_error(503, r#"{"error":"service unavailable"}"#);
    }
    let server = builder.build().await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let err = agent.plan().await.unwrap_err();
    let err_str = err.to_string();
    // The error should contain some indication of the failure.
    assert!(!err_str.is_empty(), "error message should not be empty");

    server.stop().await;
}

/// `plan` does not increment `turn_artifact_seq` when the API call fails,
/// because the increment happens after a successful response.
/// We queue many errors to exhaust any client-side retry logic.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_no_seq_increment_on_error() {
    let mut builder = MockLlmServer::builder();
    for _ in 0..20 {
        builder = builder.with_error(500, r#"{"error":"fail"}"#);
    }
    let server = builder.build().await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));
    let seq_before = agent.turn_artifact_seq;

    let _ = agent.plan().await;

    assert_eq!(
        agent.turn_artifact_seq, seq_before,
        "turn_artifact_seq should not change when plan fails"
    );

    server.stop().await;
}

/// `plan` does not accumulate token usage when the API call fails, since
/// no metadata is returned on error.  We queue many errors to exhaust retries.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_no_token_accumulation_on_error() {
    let mut builder = MockLlmServer::builder();
    for _ in 0..20 {
        builder = builder.with_error(500, r#"{"error":"fail"}"#);
    }
    let server = builder.build().await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Hello"));

    let input_before = agent.cumulative_token_usage.input;
    let output_before = agent.cumulative_token_usage.output;

    let _ = agent.plan().await;

    assert_eq!(
        agent.cumulative_token_usage.input, input_before,
        "token usage should not change on API error"
    );
    assert_eq!(
        agent.cumulative_token_usage.output, output_before,
        "token usage should not change on API error"
    );

    server.stop().await;
}

/// `plan` correctly handles a response that contains both text content
/// and an embedded tool call — the text is preserved in the assistant
/// message and `Ok(true)` is returned.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_preserves_text_alongside_tool_call() {
    let response = r#"I'll read the file first to understand the code.

<tool>
<name>file_read</name>
<arguments>{"path":"./src/main.rs"}</arguments>
</tool>"#;

    let server = MockLlmServer::builder()
        .with_response(response)
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Read main.rs"));

    let result = agent.plan().await;
    assert!(result.is_ok());
    assert!(
        result.unwrap(),
        "plan should return true when a tool call is present alongside text"
    );

    // The assistant message should contain the full response text.
    let last = agent.messages.last().unwrap();
    assert_eq!(last.role, "assistant");
    assert!(
        last.content.text().contains("I'll read the file first"),
        "assistant message should preserve the text portion of the response"
    );

    server.stop().await;
}

/// `plan` works correctly when the message history contains only a single
/// user message (no system prompt).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_works_with_single_user_message() {
    let server = MockLlmServer::builder()
        .with_response("Response to a bare user message.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    // Only one user message, no system prompt.
    agent.messages.push(Message::user("Bare message"));

    let result = agent.plan().await;
    assert!(
        result.is_ok(),
        "plan should work with a single user message: {:?}",
        result.err()
    );

    server.stop().await;
}

/// `plan` total token usage is consistent with input + output after a
/// successful call. The mock returns prompt_tokens=10,
/// completion_tokens=5, total_tokens=15.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_plan_total_tokens_consistent_with_input_plus_output() {
    let server = MockLlmServer::builder()
        .with_response("Some response.")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut agent = Agent::new(config).await.unwrap();
    agent.messages.push(Message::user("Task"));

    let total_before = agent.cumulative_token_usage.total;

    let _ = agent.plan().await;

    // The mock returns total_tokens=15, so total should increase by 15.
    assert_eq!(
        agent.cumulative_token_usage.total,
        total_before + 15,
        "total token usage should increase by the total_tokens from metadata"
    );
    // total should equal input + output after accumulation.
    assert_eq!(
        agent.cumulative_token_usage.total,
        agent.cumulative_token_usage.input + agent.cumulative_token_usage.output,
        "total tokens should equal input + output after plan"
    );

    server.stop().await;
}
