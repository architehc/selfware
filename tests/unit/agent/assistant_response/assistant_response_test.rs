use super::{
    build_assistant_history_message, context_boundary_insert_pos, history_ends_on_open_pair,
    insertion_splits_tool_pair, reasoning_carries_tool_markup, resolve_step_token_counts,
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

// ── finish_reason=length with reasoning-only output (2026-09-21 review, P2) ──
//
// A streamed response cut off by the completion budget whose ONLY output is
// a reasoning trace (empty answer text) is TRUNCATED, not a deliverable.
// Previously the truncated reasoning was promoted to content (the tag-free
// model fallback) and stored as the final answer, bypassing execution's
// length rejection. It must now fail typed — `ReasoningBudgetExhausted`,
// the same semantic the non-streaming client applies — so the turn is never
// accepted as completed work.

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "raw TCP SSE server unreliable under heavy parallelism on Windows CI"
)]
async fn streamed_reasoning_only_length_never_becomes_the_final_answer() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // A raw SSE server that answers any request with one reasoning delta and
    // a final `finish_reason: "length"` — no answer text, no [DONE]. It
    // serves TWO requests: the original and the main turn's single
    // reasoning step-down retry, which exhausts the same way.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Drain the request head (any method/path).
            let mut buf = [0u8; 4096];
            let mut head = Vec::new();
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                head.extend_from_slice(&buf[..n]);
                if head.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let sse = "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"Let me think about this carefully before the answer gets cut off...\"},\"finish_reason\":null}]}\n\n\
                   data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":320,\"total_tokens\":332}}\n\n";
            // Chunked framing exactly like the passing mock streams: the API
            // client's body decoder requires it.
            let chunk = format!("{:X}\r\n{}\r\n", sse.len(), sse);
            let end_chunk = "0\r\n\r\n";
            let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{}{}",
            chunk, end_chunk
        );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });

    let config = mock_agent_config(format!("http://{addr}/v1"), true);
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    agent
        .messages
        .push(crate::api::types::Message::user("Write the refactor plan."));

    let result = agent.get_assistant_step_response(false).await;
    let err = match result {
        Ok(resp) => panic!(
            "reasoning-only + finish_reason=length must fail typed, not become a final \
             answer; got Ok with {} content chars",
            resp.content.chars().count()
        ),
        Err(e) => e,
    };
    assert!(
        matches!(
            err.downcast_ref::<crate::errors::ApiError>(),
            Some(crate::errors::ApiError::ReasoningBudgetExhausted { .. })
        ),
        "expected ApiError::ReasoningBudgetExhausted, got: {err:?}"
    );
    assert!(
        err.to_string().contains("retried once with"),
        "the failure must name the step-down retry: {err}"
    );
    server.await.unwrap();
}

// ── Reasoning-budget recovery: one bounded step-down retry per main turn ──

/// Config with a pinned reasoning effort (`chat_template_kwargs`, the shape of
/// the tracked llm.selfware.design config) so the non-streaming client's own
/// unpinned recovery stays out of the way and the agent's retry is exercised.
fn pinned_xhigh_config(endpoint: String, streaming: bool) -> Config {
    let mut config = mock_agent_config(endpoint, streaming);
    let mut kwargs = serde_json::Map::new();
    kwargs.insert("reasoning_effort".into(), serde_json::json!("xhigh"));
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".into(),
        serde_json::Value::Object(kwargs),
    );
    config.extra_body = Some(extra);
    config
}

fn reasoning_retry_events(
    recorder: &crate::agent::progress::RecordingProgressEmitter,
) -> Vec<(String, String)> {
    recorder
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            crate::agent::progress::ProgressEvent::TurnDecision { decision, detail }
                if decision.starts_with("reasoning_budget") =>
            {
                Some((decision, detail))
            }
            _ => None,
        })
        .collect()
}

fn body_reasoning_effort(raw: &str) -> Option<String> {
    let json_start = raw.find('{')?;
    let body: serde_json::Value = serde_json::from_str(&raw[json_start..]).ok()?;
    body["chat_template_kwargs"]["reasoning_effort"]
        .as_str()
        .map(str::to_string)
}

async fn run_reasoning_recovery_case(
    streaming: bool,
    server: &MockLlmServer,
) -> (
    anyhow::Result<super::AssistantStepResponse>,
    crate::agent::progress::RecordingProgressEmitter,
) {
    let recorder = crate::agent::progress::RecordingProgressEmitter::new();
    let config = pinned_xhigh_config(format!("{}/v1", server.url()), streaming);
    let mut agent = crate::agent::Agent::new(config)
        .await
        .unwrap()
        .with_progress_emitter(std::sync::Arc::new(recorder.clone()));
    agent
        .messages
        .push(crate::api::types::Message::user("Write the refactor plan."));
    let result = agent.get_assistant_step_response(false).await;
    (result, recorder)
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn reasoning_only_length_then_success_retries_once_with_lower_effort() {
    // Both call shapes must reach the recovery: streamed and non-streaming.
    for streaming in [true, false] {
        let server = MockLlmServer::builder()
            .with_finished_response("", Some("thinking until the budget runs out"), "length")
            .with_response("The refactor plan: extract the parser.")
            .build()
            .await;
        let (result, recorder) = run_reasoning_recovery_case(streaming, &server).await;
        let resp =
            result.unwrap_or_else(|e| panic!("streaming={streaming}: retry must recover: {e:?}"));
        assert_eq!(
            resp.content, "The refactor plan: extract the parser.",
            "streaming={streaming}"
        );

        let bodies = server.captured_request_bodies().await;
        assert_eq!(bodies.len(), 2, "streaming={streaming}: exactly one retry");
        assert_eq!(body_reasoning_effort(&bodies[0]).as_deref(), Some("xhigh"));
        assert_eq!(
            body_reasoning_effort(&bodies[1]).as_deref(),
            Some("high"),
            "streaming={streaming}: retry steps effort down one level"
        );
        // max_tokens is never raised by the retry.
        for raw in &bodies {
            assert!(
                raw.contains("\"max_tokens\":8192"),
                "streaming={streaming}: {raw}"
            );
        }

        let events = reasoning_retry_events(&recorder);
        assert_eq!(events.len(), 1, "streaming={streaming}: {events:?}");
        assert_eq!(events[0].0, "reasoning_budget_retry");
        assert!(
            events[0].1.contains("reasoning_effort xhigh -> high"),
            "{events:?}"
        );
        server.stop().await;
    }
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn reasoning_only_length_twice_fails_typed_naming_the_retry() {
    for streaming in [true, false] {
        let server = MockLlmServer::builder()
            .with_finished_response("", Some("thinking"), "length")
            .with_finished_response("", Some("still thinking"), "length")
            .with_response("never reached")
            .build()
            .await;
        let (result, recorder) = run_reasoning_recovery_case(streaming, &server).await;
        let err = match result {
            Ok(r) => panic!("streaming={streaming}: must fail, got {:?}", r.content),
            Err(e) => e,
        };
        match crate::errors::reasoning_budget_exhaustion(&err) {
            Some((_, Some(note))) => assert!(
                note.contains("retried once with reasoning_effort xhigh -> high"),
                "streaming={streaming}: {note}"
            ),
            other => panic!("streaming={streaming}: expected typed exhaustion with a retry note, got {other:?} / {err:?}"),
        }
        assert!(
            err.to_string()
                .contains("retried once with reasoning_effort xhigh -> high"),
            "message must name the retry and its effort: {err}"
        );
        assert_eq!(
            server.captured_request_bodies().await.len(),
            2,
            "streaming={streaming}: bounded to ONE retry"
        );
        let kinds: Vec<String> = reasoning_retry_events(&recorder)
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(
            kinds,
            vec!["reasoning_budget_retry", "reasoning_budget_retry_failed"],
            "streaming={streaming}"
        );
        server.stop().await;
    }
}

#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn length_truncation_with_content_is_not_retried() {
    for streaming in [true, false] {
        let server = MockLlmServer::builder()
            .with_finished_response("A partial answer that ran out", Some("brief"), "length")
            .with_response("should not be requested")
            .build()
            .await;
        let (_result, recorder) = run_reasoning_recovery_case(streaming, &server).await;
        assert_eq!(
            server.captured_request_bodies().await.len(),
            1,
            "streaming={streaming}: a truncation that produced content is not a reasoning-budget trap"
        );
        assert!(reasoning_retry_events(&recorder).is_empty());
        server.stop().await;
    }
}

/// A content-less turn's reasoning is promoted to content only when it
/// carries a tool-call ATTEMPT. Reasoning that quotes the syntax in markdown
/// code (as the parser reads code: quoted, never a call) is a monologue and
/// must not become the final answer text.
#[test]
fn reasoning_quoting_tool_syntax_in_code_is_not_promoted() {
    for quoted in [
        "The parser accepts `<tool><name>x</name><arguments>{}</arguments></tool>`.",
        "Qwen emits `<function=file_read>` and `<tool_call>` wrappers.",
        "Example:\n```xml\n<tool>\n<name>file_read</name>\n</tool>\n```\nDone thinking.",
    ] {
        assert!(!reasoning_carries_tool_markup(quoted), "{quoted}");
    }
    for live in [
        "Reading it.\n<tool>\n<name>file_read</name>\n<arguments>{\"path\": \"a\"}</arguments>\n</tool>",
        "<function=file_read>\n<parameter=path>a</parameter>\n</function>",
        "<tool_call>{\"name\": \"git_diff\"}</tool_call>",
        "<|open|>call tool=\"file_read\"",
        "`quoted` first, then\n<tool>\n<name>file_read</name>",
    ] {
        assert!(reasoning_carries_tool_markup(live), "{live}");
    }
}
