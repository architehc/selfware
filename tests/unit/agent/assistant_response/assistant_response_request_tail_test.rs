//! End-to-end request assembly: what actually goes over the wire.
//!
//! Per-turn dynamic content (failure hint, context-map tree with live token
//! counts, learning hints, RAG) and the work ledger travel at the END of the
//! request; the system message stays byte-identical between turns.

use super::*;
use crate::api::types::Message;
use crate::config::{Config, ExecutionMode};
use crate::testing::mock_api::MockLlmServer;

fn config(endpoint: String) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 24_000,
        max_tokens: 2048,
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: true,
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

fn read_call(id: &str, path: &str) -> Message {
    let mut message = Message::assistant("");
    message.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: id.to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_read".to_string(),
            arguments: serde_json::json!({ "path": path }).to_string(),
        },
    }]);
    message
}

fn request_messages(body: &str) -> Vec<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(body).expect("request body is JSON");
    v["messages"].as_array().cloned().unwrap_or_default()
}

fn content_of(message: &serde_json::Value) -> String {
    match &message["content"] {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn wire_requests_keep_a_stable_system_message_and_end_with_the_ledger() {
    let server = MockLlmServer::builder()
        .with_response("ok")
        .with_response("ok")
        .build()
        .await;
    let mut agent = Agent::new(config(format!("{}/v1", server.url())))
        .await
        .unwrap();
    agent.messages = vec![
        Message::system("You are selfware. Stable system prompt."),
        Message::user("Fix the lexer bug"),
    ];
    // A per-turn hint that previously was merged into the system message.
    agent.pending_failure_hint = Some("PREVIOUS STEP FAILED: cargo_test".to_string());

    agent.get_assistant_step_response(false).await.unwrap();

    // The conversation grows by one successful read.
    agent.messages.push(read_call("r1", "src/lexer.rs"));
    agent.messages.push(Message::tool(
        serde_json::json!({"content": "fn lex() {}", "total_lines": 1}).to_string(),
        "r1",
    ));
    // The run loop pushes per-step banners as role=system mid-conversation;
    // the send path used to hoist them into the system prompt.
    agent
        .messages
        .push(Message::system("PROGRESS BANNER: step 2 of 8"));
    agent.get_assistant_step_response(false).await.unwrap();

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 2);
    let first = request_messages(&bodies[0]);
    let second = request_messages(&bodies[1]);

    assert_eq!(first[0]["role"], "system");
    assert_eq!(second[0]["role"], "system");
    let sys1 = content_of(&first[0]);
    let sys2 = content_of(&second[0]);
    assert_eq!(
        sys1, sys2,
        "system message must be byte-identical between turns"
    );
    assert!(!sys1.contains("PREVIOUS STEP FAILED"));
    assert!(!sys2.contains("Work ledger"));
    assert!(!sys2.contains("PROGRESS BANNER"));
    assert_eq!(
        second.iter().filter(|m| m["role"] == "system").count(),
        1,
        "only the leading system prompt is sent as role=system"
    );
    assert!(
        second
            .iter()
            .any(|m| m["role"] == "user" && content_of(m).contains("PROGRESS BANNER: step 2 of 8")),
        "the banner still reaches the model, as a user-role context note"
    );

    // The failure hint reached the model — at the end of the first request.
    let first_last = content_of(first.last().unwrap());
    assert!(first_last.contains("PREVIOUS STEP FAILED: cargo_test"));

    // The second request ends with the ledger listing the read.
    let second_last = second.last().unwrap();
    assert_eq!(second_last["role"], "user");
    let tail = content_of(second_last);
    assert!(tail.contains("Work ledger"), "tail: {tail}");
    assert!(
        tail.contains("src/lexer.rs — whole file (1 lines)"),
        "tail: {tail}"
    );

    server.stop().await;
}
