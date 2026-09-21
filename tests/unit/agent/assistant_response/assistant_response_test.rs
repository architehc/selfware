use super::{
    build_assistant_history_message, context_boundary_insert_pos, history_ends_on_open_pair,
    insertion_splits_tool_pair, resolve_step_token_counts,
};
use crate::config::{Config, ExecutionMode};
use crate::testing::mock_api::MockLlmServer;

#[test]
fn uses_reported_values_when_present() {
    let (input, output) = resolve_step_token_counts(Some(1200), Some(300), 999, 999);
    assert_eq!(input, 1200);
    assert_eq!(output, 300);
}

#[test]
fn falls_back_to_estimates_when_usage_absent() {
    // Backend omitted usage entirely (common on local vLLM/SGLang).
    let (input, output) = resolve_step_token_counts(None, None, 1500, 220);
    assert_eq!(input, 1500);
    assert_eq!(output, 220);
}

#[test]
fn mixes_reported_and_estimated_per_field() {
    // Provider gave prompt tokens but not completion tokens.
    let (input, output) = resolve_step_token_counts(Some(1200), None, 999, 220);
    assert_eq!(input, 1200);
    assert_eq!(output, 220);
}

#[test]
fn test_assistant_response_reasoning_history_omitted_by_default() {
    let cfg = crate::config::Config::default();
    assert!(!cfg.preserve_thinking());

    let msg = build_assistant_history_message(
        "<think>internal thought</think>final answer",
        Some("internal thought".to_string()),
        None,
        cfg.preserve_thinking(),
    );
    assert_eq!(msg.content.text(), "final answer");
    assert!(
        msg.reasoning_content.is_none(),
        "history reasoning must be None when preserve_thinking=false"
    );
}

#[test]
fn test_assistant_response_reasoning_history_preserved_when_configured() {
    let mut cfg = crate::config::Config::default();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "preserve_thinking": true }),
    );
    cfg.extra_body = Some(extra);
    assert!(cfg.preserve_thinking());

    let msg = build_assistant_history_message(
        "<think>internal thought</think>final answer",
        Some("internal thought".to_string()),
        None,
        cfg.preserve_thinking(),
    );
    assert_eq!(msg.content.text(), "final answer");
    assert_eq!(
        msg.reasoning_content.as_deref(),
        Some("internal thought"),
        "history reasoning must be preserved when preserve_thinking=true"
    );
}

#[test]
fn test_multi_turn_context_growth_compact_with_preserve_thinking_false() {
    let mut messages = Vec::new();
    let cfg = crate::config::Config::default();

    // Turn 1
    messages.push(crate::api::types::Message::user("What is 2+2?"));
    let reasoning_1 = "The user is asking for 2+2. Let's compute it step by step...".repeat(50);
    messages.push(build_assistant_history_message(
        "4",
        Some(reasoning_1),
        None,
        cfg.preserve_thinking(),
    ));

    // Turn 2
    messages.push(crate::api::types::Message::user("What is 3+3?"));
    let reasoning_2 = "The user asks for 3+3...".repeat(50);
    messages.push(build_assistant_history_message(
        "6",
        Some(reasoning_2),
        None,
        cfg.preserve_thinking(),
    ));

    // Turn 3
    messages.push(crate::api::types::Message::user("What is 4+4?"));
    let reasoning_3 = "The user asks for 4+4...".repeat(50);
    messages.push(build_assistant_history_message(
        "8",
        Some(reasoning_3),
        None,
        cfg.preserve_thinking(),
    ));

    let total_chars: usize = messages
        .iter()
        .map(|m| m.content.len() + m.reasoning_content.as_ref().map_or(0, |r| r.len()))
        .sum();
    assert!(
        total_chars < 200,
        "history characters ({}) must stay small when preserve_thinking=false",
        total_chars
    );
    let estimated_tokens: usize = messages
        .iter()
        .map(|m| crate::token_count::estimate_content_tokens(m.content.text()))
        .sum();
    assert!(
        estimated_tokens < 100,
        "estimated history tokens ({}) should stay neat (<100)",
        estimated_tokens
    );
}

fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
    Config {
        endpoint,
        model: "mock-model".to_string(),
        context_length: 500_000,
        max_tokens: 8192,
        agent: crate::config::AgentConfig {
            max_iterations: 8,
            step_timeout_secs: 30,
            stream_stall_timeout_secs: None,
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

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_get_assistant_step_response_call_site_omits_reasoning_when_preserve_thinking_false() {
    let server = MockLlmServer::builder()
        .with_reasoning_response("Final step answer", "Internal step thinking")
        .build()
        .await;

    let config = mock_agent_config(format!("{}/v1", server.url()), false);
    assert!(!config.preserve_thinking());

    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    agent
        .messages
        .push(crate::api::types::Message::user("Hello"));

    let response = agent.get_assistant_step_response(false).await.unwrap();
    assert_eq!(response.content.trim(), "Final step answer");

    let last_msg = agent
        .messages
        .last()
        .expect("must have pushed message to history");
    assert_eq!(last_msg.content.text(), "Final step answer");
    assert!(
        last_msg.reasoning_content.is_none(),
        "history message reasoning_content must be None when preserve_thinking=false"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_get_assistant_step_response_call_site_preserves_reasoning_when_preserve_thinking_true(
) {
    let server = MockLlmServer::builder()
        .with_reasoning_response("Final step answer", "Internal step thinking")
        .build()
        .await;

    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "preserve_thinking": true }),
    );
    config.extra_body = Some(extra);
    assert!(config.preserve_thinking());

    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    agent
        .messages
        .push(crate::api::types::Message::user("Hello"));

    let response = agent.get_assistant_step_response(false).await.unwrap();
    assert_eq!(response.content.trim(), "Final step answer");

    let last_msg = agent
        .messages
        .last()
        .expect("must have pushed message to history");
    assert_eq!(last_msg.content.text(), "Final step answer");
    assert_eq!(
        last_msg.reasoning_content.as_deref(),
        Some("Internal step thinking"),
        "history message reasoning_content must be preserved when preserve_thinking=true"
    );
    server.stop().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn test_get_assistant_step_response_streaming_preserves_thinking_when_configured() {
    let server = MockLlmServer::builder()
        .with_reasoning_response("Final step answer", "Internal step thinking")
        .build()
        .await;

    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.streaming = true;
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({ "preserve_thinking": true }),
    );
    config.extra_body = Some(extra);
    assert!(config.preserve_thinking());

    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    agent
        .messages
        .push(crate::api::types::Message::user("Hello"));

    let response = agent.get_assistant_step_response(false).await.unwrap();
    assert_eq!(response.content.trim(), "Final step answer");

    let last_msg = agent
        .messages
        .last()
        .expect("must have pushed message to history");
    assert_eq!(last_msg.content.text(), "Final step answer");
    assert_eq!(
        last_msg.reasoning_content.as_deref(),
        Some("Internal step thinking"),
        "history message reasoning_content must be preserved in streaming mode when preserve_thinking=true"
    );
    server.stop().await;
}

// ============================================================================
// Context-boundary insertion must never split a tool-call/tool-result pair
// ============================================================================

use crate::api::types::{Message, MessageContent, ToolCall, ToolFunction};

fn assistant_with_native_calls(ids: &[&str]) -> Message {
    let calls = ids
        .iter()
        .map(|id| ToolCall {
            id: id.to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "test_tool".to_string(),
                arguments: "{}".to_string(),
            },
        })
        .collect();
    Message {
        role: "assistant".to_string(),
        content: MessageContent::Text(String::new()),
        reasoning_content: None,
        tool_calls: Some(calls),
        tool_call_id: None,
        name: None,
    }
}

/// The OpenAI wire invariant: every assistant that carries `tool_calls` must
/// be immediately followed by a contiguous run of its role=tool results, and
/// no tool message may live outside such a run. SGLang/vLLM/OpenAI reject any
/// payload that violates this with HTTP 400.
fn assert_tool_pair_invariants(messages: &[Message]) {
    let mut i = 0;
    while i < messages.len() {
        let has_calls = messages[i]
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty());
        if has_calls {
            let mut j = i + 1;
            while j < messages.len() && messages[j].role == "tool" {
                j += 1;
            }
            assert!(
                j > i + 1,
                "assistant tool_calls at index {} must be immediately followed by tool results",
                i
            );
            i = j;
        } else {
            assert_ne!(
                messages[i].role, "tool",
                "orphan tool message at index {} (no immediately preceding assistant tool_calls)",
                i
            );
            i += 1;
        }
    }
}

/// Regression: the boundary slot was computed as `len - 6` and inserted
/// unconditionally, which frequently landed INSIDE an
/// assistant(tool_calls)/tool-result pair. The assembled list must keep every
/// assistant(tool_calls) immediately followed by its tool results.
#[test]
fn context_boundary_slot_never_splits_tool_call_pairs() {
    // 10 messages: the naive `len - 6 = 4` slot lands between the two tool
    // results of the pair at index 2 — the exact split that 400s on strict
    // OpenAI-compatible endpoints.
    let messages = vec![
        Message::system("You are a coding agent."),
        Message::user("Investigate the widget."),
        assistant_with_native_calls(&["call_1", "call_1b"]),
        Message::tool("result 1", "call_1"),
        Message::tool("result 1b", "call_1b"),
        Message::user("The results are applied."),
        Message::assistant("Intermediate summary."),
        Message::user("Continue."),
        Message::assistant("More reasoning."),
        Message::user("Final question."),
    ];
    // Precondition: the previously-used slot would have split the pair.
    assert!(insertion_splits_tool_pair(&messages, 4));

    let pos = context_boundary_insert_pos(&messages).expect("a safe slot must exist");
    assert!(
        !insertion_splits_tool_pair(&messages, pos),
        "chosen slot {pos} must not split a pair"
    );
    // The slot is pushed back to just before the pair's opening assistant.
    assert_eq!(pos, 2);

    // Assemble exactly like the request path does, then verify the invariant.
    let mut assembled = messages.clone();
    let boundary = "<context_boundary>\nreference above, task below\n</context_boundary>";
    assembled.insert(pos, Message::user(boundary));
    assert_tool_pair_invariants(&assembled);
    assert_eq!(assembled[pos].content.text(), boundary);
}

/// The history ending on an OPEN pair (assistant tool_calls whose results
/// have not been appended yet) must skip the boundary insertion entirely —
/// the results land right behind the call on the next dispatch step.
#[test]
fn context_boundary_skipped_when_tail_is_open_pair() {
    let messages = vec![
        Message::system("You are a coding agent."),
        Message::user("Investigate the widget."),
        assistant_with_native_calls(&["call_1"]),
        Message::tool("result 1", "call_1"),
        Message::user("Applied."),
        Message::assistant("Partial."),
        Message::user("Continue."),
        Message::assistant("Working."),
        Message::user("Next."),
        assistant_with_native_calls(&["call_9"]), // open pair at the tail
    ];
    assert!(history_ends_on_open_pair(&messages));
    assert_eq!(
        context_boundary_insert_pos(&messages),
        None,
        "no boundary may be inserted when the tail is an open pair"
    );

    // Control: the same history closed by its results may insert.
    let mut closed = messages;
    closed.pop();
    closed.push(Message::tool("result 9", "call_9"));
    closed.push(Message::user("Done."));
    assert!(!history_ends_on_open_pair(&closed));
    assert!(context_boundary_insert_pos(&closed).is_some());
}

/// End-to-end: `get_assistant_step_response` injects the boundary into the
/// outbound request WITHOUT splitting the tool-call pair, so the wire body
/// still satisfies the pairing invariant (native FC keeps the actual
/// role=tool / tool_calls shape on the wire).
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn context_boundary_keeps_tool_pairs_adjacent_on_the_wire() {
    let server = MockLlmServer::builder()
        .with_response("Final step answer")
        .build()
        .await;

    let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
    config.agent.native_function_calling = true;
    let mut agent = crate::agent::Agent::new(config).await.unwrap();

    // A context map with at least one tracked file activates the boundary
    // branch.
    let mut map = crate::agent::context_map::ContextMap::new(2_000, 0.75, 0.20, 0.05);
    map.register_tree_entry("src/wire.rs".into(), 100);
    agent.context_map = map;

    // 10 messages; the naive 6-from-the-end slot would split the pair at
    // index 2 (the boundary must instead land before it).
    for m in [
        Message::system("You are a coding agent."),
        Message::user("Investigate the widget."),
        assistant_with_native_calls(&["call_1", "call_1b"]),
        Message::tool("result 1", "call_1"),
        Message::tool("result 1b", "call_1b"),
        Message::user("The results are applied."),
        Message::assistant("Intermediate summary."),
        Message::user("Continue."),
        Message::assistant("More reasoning."),
        Message::user("Final question."),
    ] {
        agent.messages.push(m);
    }

    let response = agent.get_assistant_step_response(false).await.unwrap();
    assert!(!response.content.trim().is_empty());

    let bodies = server.captured_request_bodies().await;
    assert!(!bodies.is_empty(), "no request captured");
    let body: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    let wire_messages: Vec<Message> =
        serde_json::from_value(body["messages"].clone()).expect("wire messages must parse");

    // The boundary WAS injected…
    assert!(
        wire_messages
            .iter()
            .any(|m| m.role == "user" && m.content.text().contains("<context_boundary>")),
        "context boundary marker must be present on the wire"
    );
    // …and no assistant(tool_calls)/tool-result pair was split.
    assert_tool_pair_invariants(&wire_messages);

    server.stop().await;
}
