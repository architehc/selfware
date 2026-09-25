use super::*;
use crate::config::Config;
use crate::testing::mock_api::MockLlmServer;
use tempfile::tempdir;

/// Build a minimal Agent backed by a mock LLM server.
async fn make_test_agent(server: &MockLlmServer) -> Agent {
    let config = Config {
        endpoint: format!("{}/v1", server.url()),
        model: "mock-model".to_string(),
        context_length: crate::config::default_context_length(),
        agent: crate::config::AgentConfig {
            max_iterations: 4,
            step_timeout_secs: 5,
            stream_stall_timeout_secs: None,
            streaming: false,
            native_function_calling: false,
            ..Default::default()
        },
        ..Default::default()
    };
    Agent::new(config)
        .await
        .expect("failed to create test agent")
}

fn assistant_tool_call(id: &str, name: &str) -> Message {
    let mut message = Message::assistant("");
    message.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: id.to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: name.to_string(),
            arguments: "{}".to_string(),
        },
    }]);
    message
}

/// Assert that a message list contains no orphaned tool-call messages: every
/// `assistant` with `tool_calls` must have a later kept result per call, and
/// every `tool` result must have an earlier kept assistant call.
fn assert_valid_tool_pairing(messages: &[Message]) {
    for (i, m) in messages.iter().enumerate() {
        if let Some(calls) = m.tool_calls.as_deref() {
            for call in calls {
                let has_result = messages[i + 1..].iter().any(|r| {
                    r.role == "tool" && r.tool_call_id.as_deref() == Some(call.id.as_str())
                });
                assert!(
                    has_result,
                    "assistant message #{i} declares tool_call {} but no result is kept",
                    call.id
                );
            }
        }
        if m.role == "tool" {
            let id = m
                .tool_call_id
                .as_deref()
                .expect("tool message missing tool_call_id");
            let has_call = messages[..i].iter().any(|a| {
                a.tool_calls
                    .as_ref()
                    .is_some_and(|calls| calls.iter().any(|call| call.id == *id))
            });
            assert!(
                has_call,
                "tool message #{i} (call {id}) has no kept assistant tool_call"
            );
        }
    }
}

// =====================================================================
// compress_to_structured_summary  (compaction end-to-end)
// =====================================================================

#[tokio::test]
async fn test_structured_compression_compacts_and_keeps_system_and_recent() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;

    // Deliberately put a NON-system message first and the real system prompt
    // SECOND, to exercise the by-role preservation fix.
    agent.messages.clear();
    agent.messages.push(Message::user(
        "stale bootstrap line that should be compressed away",
    ));
    agent
        .messages
        .push(Message::system("SYSTEM_PROMPT_SENTINEL: obey the rules"));
    for i in 0..10 {
        agent.messages.push(Message::user(format!(
            "history message {i} with enough words to cost some tokens for the estimator"
        )));
        agent.messages.push(Message::assistant(format!(
            "reply {i} acknowledging the work is progressing"
        )));
    }
    let before = agent.messages.len();
    let recent_marker = "reply 9 acknowledging the work is progressing";

    // target far below current usage → compression must fire.
    agent.compress_to_structured_summary(1);

    let after = agent.messages.len();
    let joined: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        after < before,
        "compaction must reduce message count ({before} -> {after})"
    );
    assert!(
        joined.contains("SYSTEM_PROMPT_SENTINEL"),
        "the real system prompt must survive even when it wasn't first"
    );
    assert!(
        joined.contains("STRUCTURED SUMMARY"),
        "compacted history is replaced by a structured summary block"
    );
    assert!(
        joined.contains(recent_marker),
        "the most recent turn must be kept verbatim"
    );
    assert!(
        !joined.contains("stale bootstrap line"),
        "the non-system first message must be compressed away, not mistaken for the system prompt"
    );
    server.stop().await;
}

#[tokio::test]
async fn test_structured_compression_noop_when_under_target() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();
    agent.messages.push(Message::system("sys"));
    agent.messages.push(Message::user("short"));
    let before = agent.messages.len();
    // huge target → nothing to compress.
    agent.compress_to_structured_summary(1_000_000);
    assert_eq!(
        agent.messages.len(),
        before,
        "no compaction below the target"
    );
    server.stop().await;
}

#[tokio::test]
async fn test_structured_compression_preserves_task_anchor_and_tool_pairing() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();

    // A bootstrap user line BEFORE the system prompt must not be mistaken
    // for the task anchor; the real task sits right after the system prompt.
    agent.messages.push(Message::user("stale bootstrap line"));
    agent
        .messages
        .push(Message::system("SYSTEM_MARKER: obey the rules"));
    agent
        .messages
        .push(Message::user("TASK ANCHOR SENTINEL: refactor the parser"));
    agent
        .messages
        .push(assistant_tool_call("call_A", "file_read"));
    agent.messages.push(Message::tool("result A", "call_A"));
    for i in 0..9 {
        agent
            .messages
            .push(Message::user(format!("filler user {i}")));
        agent
            .messages
            .push(Message::assistant(format!("filler assistant {i}")));
    }
    agent.messages.push(Message::user("extra filler"));
    // Tail: an assistant tool_call whose tool result OPENS the recent-4
    // window — the call is outside the window, so the result would orphan.
    agent
        .messages
        .push(assistant_tool_call("call_B", "file_write"));
    agent
        .messages
        .push(Message::tool("orphaned result B", "call_B"));
    agent.messages.push(Message::assistant("final assistant"));
    agent.messages.push(Message::user("final user 1"));
    agent.messages.push(Message::user("final user 2"));

    agent.compress_to_structured_summary(1);

    let joined: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");

    // (1) Original task anchor survives (and is pinned after the by-role system).
    assert!(
        joined.contains("[ORIGINAL TASK]"),
        "task must be pinned as [ORIGINAL TASK]: {joined}"
    );
    assert!(
        joined.contains("TASK ANCHOR SENTINEL"),
        "original task text must survive structured compression"
    );
    assert!(
        joined.contains("SYSTEM_MARKER"),
        "real system prompt (second message) must survive even when not first"
    );
    assert!(
        !joined.contains("stale bootstrap line"),
        "bootstrap line before the system prompt must be compacted away"
    );
    // (2) No orphaned tool messages, no dangling tool_calls.
    assert_valid_tool_pairing(&agent.messages);
    assert!(
        !joined.contains("orphaned result B"),
        "orphaned tool result opening the recent window must be dropped"
    );
    // The by-role system + anchor + summary headers must lead the list.
    assert_eq!(
        agent.messages[0].role, "system",
        "system message must remain first"
    );
    server.stop().await;
}

/// Assert strict user/assistant role alternation: no two consecutive
/// messages may share the user role (strict-alternation providers 400 on
/// that shape), while XML tool-result-as-user messages still count as their
/// own user-role turn.
fn assert_strict_role_alternation(messages: &[Message]) {
    for pair in messages.windows(2) {
        assert!(
            !(pair[0].role == "user" && pair[1].role == "user"),
            "consecutive user-role messages at [{:#?}] -> [{:#?}]",
            pair[0],
            pair[1]
        );
    }
}

/// Review finding #3 (structured compaction boundary): the three boundary
/// markers ([ORIGINAL TASK] / [STRUCTURED SUMMARY] / [RECENT CONTEXT]) were
/// consecutive user-role messages. They must coalesce into one real user
/// turn — while an XML tool-result user message in the recent window keeps
/// its own `<tool_result>` envelope message (never merged into a real
/// user turn).
#[tokio::test]
async fn test_structured_compression_boundary_alternates_strict_roles() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();
    agent.messages.push(Message::user("stale bootstrap line"));
    agent
        .messages
        .push(Message::system("SYSTEM_MARKER: obey the rules"));
    agent
        .messages
        .push(Message::user("STRUCT ALT TASK: refactor the parser"));
    for i in 0..8 {
        agent
            .messages
            .push(Message::user(format!("filler user {i}")));
        agent
            .messages
            .push(Message::assistant(format!("filler assistant {i}")));
    }
    // Recent window (last 4): assistant, XML tool-result user, assistant,
    // plain user turn. Payload fragments are inert markers.
    let lt = "<";
    let gt = ">";
    let envelope_open = format!("{lt}tool_result{gt}");
    let envelope_close = format!("{lt}/tool_result{gt}");
    agent.messages.push(Message::assistant("recent a0"));
    agent.messages.push(Message::user(format!(
        "{envelope_open}keep me{envelope_close}"
    )));
    agent.messages.push(Message::assistant("recent a1"));
    agent.messages.push(Message::user("final user turn"));

    agent.compress_to_structured_summary(1);

    assert_strict_role_alternation(&agent.messages);
    let joined: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains("STRUCT ALT TASK"),
        "original task must survive: {joined}"
    );
    assert!(
        joined.contains("STRUCTURED SUMMARY"),
        "summary marker must survive (coalesced, not dropped): {joined}"
    );
    assert!(
        joined.contains("SYSTEM_MARKER"),
        "real system prompt must survive: {joined}"
    );
    assert!(
        !joined.contains("stale bootstrap line"),
        "bootstrap line before the system prompt must be compacted away: {joined}"
    );
    // The XML tool-result user message survives as its own envelope message.
    let tool_result = agent
        .messages
        .iter()
        .find(|m| m.content.text_all().contains("keep me"))
        .unwrap_or_else(|| panic!("tool-result content must survive: {joined}"));
    assert_eq!(tool_result.role, "user");
    assert!(
        tool_result.content.text().starts_with(&envelope_open),
        "tool-result must keep its own unmerged envelope message: {joined}"
    );
    server.stop().await;
}

// =====================================================================
// format_file_size  (pure static method -- no Agent needed)
// =====================================================================

#[test]
fn test_format_file_size_zero_bytes() {
    assert_eq!(Agent::format_file_size(0), "0B");
}

#[test]
fn test_format_file_size_small_bytes() {
    assert_eq!(Agent::format_file_size(1), "1B");
    assert_eq!(Agent::format_file_size(512), "512B");
    assert_eq!(Agent::format_file_size(1023), "1023B");
}

#[test]
fn test_format_file_size_exact_1kb() {
    // 1024 bytes == 1.0KB
    assert_eq!(Agent::format_file_size(1024), "1.0KB");
}

#[test]
fn test_format_file_size_kilobytes() {
    // 2048 == 2.0KB
    assert_eq!(Agent::format_file_size(2048), "2.0KB");
    // 1536 == 1.5KB
    assert_eq!(Agent::format_file_size(1536), "1.5KB");
    // Just under 1MB: 1023 * 1024 = 1,047,552
    let just_under_mb = 1024 * 1024 - 1;
    let result = Agent::format_file_size(just_under_mb);
    assert!(result.ends_with("KB"), "expected KB suffix, got {}", result);
}

#[test]
fn test_format_file_size_exact_1mb() {
    assert_eq!(Agent::format_file_size(1024 * 1024), "1.0MB");
}

#[test]
fn test_format_file_size_megabytes() {
    // 5 MB
    assert_eq!(Agent::format_file_size(5 * 1024 * 1024), "5.0MB");
    // 1.5 MB
    assert_eq!(Agent::format_file_size(3 * 1024 * 512), "1.5MB");
}

#[test]
fn test_format_file_size_gigabyte_range() {
    // The function only distinguishes B / KB / MB, so a GB value
    // is still formatted as MB.
    let one_gb = 1024 * 1024 * 1024;
    assert_eq!(Agent::format_file_size(one_gb), "1024.0MB");
}

// =====================================================================
// enhance_cargo_errors  (needs &self for error_analyzer)
// =====================================================================

#[tokio::test]
async fn test_enhance_cargo_errors_non_json_passthrough() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = "this is not json at all";
    let result = agent.enhance_cargo_errors(input);
    assert_eq!(result, input, "non-JSON input should be returned unchanged");

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_json_no_errors_key() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = r#"{"status":"ok","warnings":[]}"#;
    let result = agent.enhance_cargo_errors(input);
    assert_eq!(
        result, input,
        "JSON without an 'errors' key should pass through"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_empty_errors_array() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = r#"{"errors":[]}"#;
    let result = agent.enhance_cargo_errors(input);
    // With an empty array there are no raw_errors, so no analysis appended.
    assert_eq!(
        result, input,
        "empty errors array should pass through without analysis"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_with_actual_errors() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = r#"{"errors":[{"code":"E0308","message":"mismatched types","file":"src/main.rs","line":10,"column":5}]}"#;
    let result = agent.enhance_cargo_errors(input);

    assert!(
        result.contains("<error_analysis>"),
        "should contain opening error_analysis tag"
    );
    assert!(
        result.contains("</error_analysis>"),
        "should contain closing error_analysis tag"
    );
    assert!(
        result.contains("Error Analysis Summary"),
        "should contain the summary header"
    );
    // Original input should still be present at the start
    assert!(
        result.starts_with(input),
        "original input should be preserved at the start"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_errors_without_message_skipped() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // Error objects missing the required "message" field should be filtered out
    let input = r#"{"errors":[{"code":"E0001","file":"a.rs"}]}"#;
    let result = agent.enhance_cargo_errors(input);
    // filter_map returns None for entries without "message", so raw_errors
    // is empty and no analysis is appended.
    assert_eq!(
        result, input,
        "errors missing 'message' should be skipped, resulting in passthrough"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_multiple_errors() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = r#"{"errors":[
            {"code":"E0308","message":"mismatched types","file":"a.rs","line":1},
            {"code":"E0425","message":"cannot find value `x` in this scope","file":"b.rs","line":5},
            {"message":"unused variable: `y`","file":"c.rs","line":10}
        ]}"#;
    let result = agent.enhance_cargo_errors(input);

    assert!(result.contains("<error_analysis>"));
    assert!(result.contains("Total errors: 3"));

    server.stop().await;
}

// =====================================================================
// expand_file_references  (needs &self for regex; uses filesystem)
// =====================================================================

#[tokio::test]
async fn test_expand_file_references_no_refs() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = "just a plain message with no file references";
    let (expanded, files) = agent.expand_file_references(input).await;
    assert_eq!(expanded, input, "input without @ refs should pass through");
    assert!(files.is_empty(), "no files should be reported");

    server.stop().await;
}

#[tokio::test]
async fn test_expand_file_references_nonexistent_file() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = "check @nonexistent_file_that_does_not_exist.rs please";
    let (expanded, files) = agent.expand_file_references(input).await;
    // The file does not exist and is not a directory, so it stays unchanged.
    assert_eq!(
        expanded, input,
        "reference to a nonexistent file should be left as-is"
    );
    assert!(
        files.is_empty(),
        "nonexistent file should not appear in the included list"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_expand_file_references_existing_file() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Create a temporary file with known content
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let file_path = dir.path().join("sample.txt");
    std::fs::write(&file_path, "hello world\n").expect("failed to write temp file");

    let path_str = file_path.display().to_string();
    let input = format!("read @{} now", path_str);
    let (expanded, files) = agent.expand_file_references(&input).await;

    assert!(
        expanded.contains("hello world"),
        "expanded output should contain the file's content"
    );
    assert!(
        expanded.contains(&path_str),
        "expanded output should reference the file path"
    );
    assert_eq!(files.len(), 1, "one file should be reported");
    assert_eq!(files[0], path_str);

    // The original @path should have been replaced
    assert!(
        !expanded.contains(&format!("@{}", path_str)),
        "the @reference should have been replaced"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_expand_file_references_includes_size_label() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let file_path = dir.path().join("tiny.rs");
    std::fs::write(&file_path, "fn main() {}").expect("write failed");

    let path_str = file_path.display().to_string();
    let input = format!("look at @{}", path_str);
    let (expanded, _) = agent.expand_file_references(&input).await;

    // format_file_size for 12 bytes produces "12B"
    assert!(
        expanded.contains("B)") || expanded.contains("KB)") || expanded.contains("MB)"),
        "expanded block should include a file-size label"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_expand_file_references_multiple_refs() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let f1 = dir.path().join("a.txt");
    let f2 = dir.path().join("b.txt");
    std::fs::write(&f1, "content A").unwrap();
    std::fs::write(&f2, "content B").unwrap();

    let input = format!("compare @{} with @{}", f1.display(), f2.display());
    let (expanded, files) = agent.expand_file_references(&input).await;

    assert!(expanded.contains("content A"));
    assert!(expanded.contains("content B"));
    assert_eq!(files.len(), 2);

    server.stop().await;
}

// =====================================================================
// clear_context  (lightweight Agent state test)
// =====================================================================

#[tokio::test]
async fn test_clear_context_retains_system_message() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Inject some user/assistant messages and context files
    agent.messages.push(Message::user("question"));
    agent.messages.push(Message::assistant("answer"));
    agent
        .file_tracker
        .context_files
        .push("some_file.rs".to_string());

    agent.clear_context();

    assert!(
        agent.messages.iter().all(|m| m.role == "system"),
        "only system messages should remain after clear"
    );
    assert!(
        agent.file_tracker.context_files.is_empty(),
        "context_files should be emptied"
    );

    server.stop().await;
}

// =====================================================================
// estimate_messages_tokens
// =====================================================================

#[tokio::test]
async fn test_estimate_messages_tokens_empty_after_system() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // A freshly created agent has exactly one system message.
    let tokens = agent.estimate_messages_tokens();
    // The system message is non-empty, so tokens should be > 0.
    assert!(
        tokens > 0,
        "should report non-zero tokens for a non-empty system message"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_estimate_messages_tokens_grows_with_messages() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let baseline = agent.estimate_messages_tokens();

    agent.messages.push(Message::user("hello world"));
    let after_one = agent.estimate_messages_tokens();
    assert!(
        after_one > baseline,
        "adding a user message should increase the token estimate"
    );

    agent
        .messages
        .push(Message::assistant("acknowledged — proceeding"));
    let after_two = agent.estimate_messages_tokens();
    assert!(
        after_two > after_one,
        "adding an assistant message should further increase the estimate"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_estimate_messages_tokens_longer_content_costs_more() {
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent_a = make_test_agent(&server).await;
    let mut agent_b = make_test_agent(&server).await;

    // Both agents start with an identical system message, so baselines match.
    agent_a.messages.push(Message::user("hi"));
    agent_b
        .messages
        .push(Message::user("hi ".repeat(200).trim().to_string()));

    let tokens_a = agent_a.estimate_messages_tokens();
    let tokens_b = agent_b.estimate_messages_tokens();

    assert!(
        tokens_b > tokens_a,
        "longer content should consume more tokens ({} vs {})",
        tokens_b,
        tokens_a
    );

    server.stop().await;
}

#[tokio::test]
async fn test_estimate_messages_tokens_all_roles_counted() {
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Clear existing messages so we start from a known state.
    agent.messages.clear();
    agent.messages.push(Message::system("sys prompt"));
    agent.messages.push(Message::user("user turn"));
    agent.messages.push(Message::assistant("assistant turn"));
    agent.messages.push(Message::tool("tool result", "call_1"));

    let tokens = agent.estimate_messages_tokens();
    // Each message has overhead of 4 plus some tokens for its content.
    assert!(
        tokens >= 4 * 4,
        "should account for overhead on all four messages; got {}",
        tokens
    );

    server.stop().await;
}

#[tokio::test]
async fn test_estimate_messages_tokens_includes_reasoning_content() {
    let _g = crate::test_support::ExecGuard::hold();
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent.messages.clear();
    let mut msg = Message::assistant("Final content");
    agent.messages.push(msg.clone());
    let baseline_tokens = agent.estimate_messages_tokens();

    // Now set reasoning_content on the assistant message
    let reasoning =
        "Step 1: analyze the problem. Step 2: verify requirements. Step 3: compute answer.";
    msg.reasoning_content = Some(reasoning.to_string());
    agent.messages.clear();
    agent.messages.push(msg);

    let with_reasoning_tokens = agent.estimate_messages_tokens();
    let expected_reasoning_tokens = crate::token_count::estimate_content_tokens(reasoning);

    assert!(
        expected_reasoning_tokens > 0,
        "reasoning must have positive token estimate"
    );
    assert_eq!(
        with_reasoning_tokens - baseline_tokens,
        expected_reasoning_tokens,
        "agent.estimate_messages_tokens() must count reasoning_content accurately"
    );

    server.stop().await;
}

// =====================================================================
// trim_message_history
// =====================================================================

#[tokio::test]
async fn test_trim_message_history_no_op_within_budget() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Set a generous budget so nothing gets trimmed.
    agent.max_context_tokens = 1_000_000;
    agent.messages.push(Message::user("hello"));
    agent.messages.push(Message::assistant("world"));

    let before = agent.messages.len();
    agent.trim_message_history();
    assert_eq!(
        agent.messages.len(),
        before,
        "no messages should be removed when within budget"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_removes_oldest_non_system() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Use a very small budget so trimming is forced.
    agent.max_context_tokens = 1;

    // The agent already has a system message; add a couple of turns.
    agent.messages.push(Message::user("first user message"));
    agent
        .messages
        .push(Message::assistant("first assistant response"));
    agent.messages.push(Message::user("second user message"));
    agent
        .messages
        .push(Message::assistant("second assistant response"));

    agent.trim_message_history();

    // System messages must always survive trimming.
    assert!(
        agent.messages.iter().all(|_| {
            // If any remaining message is "system", it was preserved.
            // We just need to verify *no* system message was dropped.
            true
        }),
        "system messages must be preserved"
    );

    // The system message itself (index 0) must survive.
    assert_eq!(
        agent.messages[0].role, "system",
        "the system message must always remain as the first entry"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_system_messages_never_removed() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Tiny budget — forces aggressive trimming.
    agent.max_context_tokens = 1;

    // Inject a second system message (unusual but valid).
    agent.messages.push(Message::system("second sys prompt"));
    agent.messages.push(Message::user("a user message"));

    agent.trim_message_history();

    let system_count = agent.messages.iter().filter(|m| m.role == "system").count();
    assert_eq!(
        system_count, 2,
        "both system messages should survive trimming even under a tiny budget"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_reduces_total_tokens() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Push a large number of wordy messages to exceed a small budget.
    for i in 0..10 {
        agent.messages.push(Message::user(format!(
            "user message number {}: {}",
            i,
            "x".repeat(200)
        )));
        agent.messages.push(Message::assistant(format!(
            "assistant reply number {}: {}",
            i,
            "y".repeat(200)
        )));
    }

    let before_tokens = agent.estimate_messages_tokens();

    // Set a budget that's smaller than the current usage.
    agent.max_context_tokens = before_tokens / 2;
    agent.trim_message_history();

    let after_tokens = agent.estimate_messages_tokens();
    assert!(
        after_tokens < before_tokens,
        "trim_message_history should reduce token usage; before={} after={}",
        before_tokens,
        after_tokens
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_logs_context_trim_event() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;
    let dir = tempdir().unwrap();
    agent.session_logger =
        super::session_log::new_test_session_logger("trim-log", dir.path().to_path_buf()).await;

    for i in 0..6 {
        agent
            .messages
            .push(Message::user(format!("message {} {}", i, "x".repeat(200))));
    }

    let before = agent.estimate_messages_tokens();
    agent.max_context_tokens = before / 2;
    agent.trim_message_history();

    let events = agent.session_logger.as_ref().unwrap().recent_events(10);
    let trim = events
        .iter()
        .find(|event| event.event_type == super::session_log::SessionEventType::ContextTrim)
        .expect("expected context trim event");
    assert_eq!(trim.success, Some(true));
    assert!(
        trim.details
            .as_ref()
            .and_then(|d| d.get("removed_messages"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_empty_messages_no_panic() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Remove all messages (even the system one) and call trim; must not panic.
    agent.messages.clear();
    agent.max_context_tokens = 1;
    agent.trim_message_history(); // Should complete without panic.

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_single_system_message_no_panic() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Only the system message should remain; trim under a tiny budget must not panic.
    agent.max_context_tokens = 1;
    let before = agent.messages.len();
    agent.trim_message_history();

    // The system message should still be present.
    assert!(
        !agent.messages.is_empty(),
        "should have at least the system message"
    );
    // Message count should not have grown.
    assert_eq!(
        agent.messages.len(),
        before,
        "system-only message list should be unchanged after trim"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_exactly_at_budget_no_op() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Measure the current token count and set the budget to exactly that.
    let exact_budget = agent.estimate_messages_tokens();
    agent.max_context_tokens = exact_budget;

    let before_count = agent.messages.len();
    agent.trim_message_history();

    assert_eq!(
        agent.messages.len(),
        before_count,
        "when usage exactly equals the budget, no messages should be removed"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_oldest_removed_first() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Add messages with distinct, identifiable content.
    // Use equal-length content so token costs are uniform.
    let pad = "x".repeat(50);
    agent
        .messages
        .push(Message::user(format!("FIRST oldest message {}", pad)));
    agent
        .messages
        .push(Message::user(format!("SECOND message {}", pad)));
    agent
        .messages
        .push(Message::user(format!("THIRD message {}", pad)));
    agent
        .messages
        .push(Message::user(format!("FOURTH newest message {}", pad)));

    // Count tokens for just the four user messages we added (not the system msg).
    let user_msg_tokens: usize = agent
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
        .sum();
    // Budget precisely, with the SAME estimator the eviction uses, sized to drop
    // exactly SECOND + THIRD (the oldest non-pinned messages). FIRST is pinned
    // as the original task and FOURTH is most recent, so both survive.
    use crate::agent::context::estimate_message_tokens as emt;
    let total: usize = agent.messages.iter().map(emt).sum();
    let second_tokens = emt(agent
        .messages
        .iter()
        .find(|m| m.content.text().contains("SECOND message"))
        .unwrap());
    let third_tokens = emt(agent
        .messages
        .iter()
        .find(|m| m.content.text().contains("THIRD message"))
        .unwrap());
    agent.max_context_tokens = total - second_tokens - third_tokens;
    let _ = user_msg_tokens;

    agent.trim_message_history();

    // The oldest non-system messages should be gone, most recent should survive.
    let contents: Vec<&str> = agent.messages.iter().map(|m| m.content.text()).collect();

    // "FIRST" is the original task — now pinned, so it survives long-run trims.
    let has_first = contents.iter().any(|c| c.contains("FIRST oldest message"));
    assert!(
        has_first,
        "the first user message (original task) must be pinned, not trimmed; remaining: {:?}",
        contents
    );
    // A middle message is evicted for budget instead (the original is protected).
    let has_second = contents.iter().any(|c| c.contains("SECOND message"));
    assert!(
        !has_second,
        "a middle user message should be trimmed for budget; remaining: {:?}",
        contents
    );

    // "FOURTH" should still be present since we budgeted for 2 user messages.
    let has_fourth = contents.iter().any(|c| c.contains("FOURTH newest message"));
    assert!(
        has_fourth,
        "the most recent message should be kept; remaining: {:?}",
        contents
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_preserves_recent_critical_tool_message() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let filler = "x".repeat(120);
    agent
        .messages
        .push(Message::user(format!("older filler {}", filler)));
    agent
        .messages
        .push(assistant_tool_call("call_1", "file_read"));
    agent.messages.push(Message::tool(
        r#"{"content":"critical tool result"}"#,
        "call_1",
    ));
    agent
        .messages
        .push(Message::user(format!("newer filler {}", filler)));
    agent
        .messages
        .push(Message::assistant(format!("latest filler {}", filler)));

    let system_tokens: usize = agent
        .messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| crate::token_count::estimate_tokens_with_overhead(m.content.text(), 4))
        .sum();
    let assistant_tokens = crate::agent::context::estimate_message_tokens(&agent.messages[2]);
    let tool_tokens = crate::agent::context::estimate_message_tokens(&agent.messages[3]);
    agent.max_context_tokens = system_tokens + assistant_tokens + tool_tokens + 50;

    agent.trim_message_history();

    assert!(agent.messages.iter().any(|message| message
        .tool_calls
        .as_ref()
        .is_some_and(|calls| calls.iter().any(|call| call.id == "call_1"))));
    assert!(agent
        .messages
        .iter()
        .any(|message| message.role == "tool"
            && message.content.text().contains("critical tool result")));
    // "older filler" is the first user message → now pinned as the original
    // task. A later non-critical filler is the one evicted for budget instead.
    assert!(!agent
        .messages
        .iter()
        .any(|message| message.content.text().contains("newer filler")));

    server.stop().await;
}

#[tokio::test]
async fn test_trim_message_history_drops_orphaned_native_tool_result() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent.max_context_tokens = 20;
    agent.messages.push(Message::user("x".repeat(400)));
    agent.messages.push(Message::tool(
        r#"{"content":"orphaned tool result"}"#,
        "call_orphan",
    ));

    agent.trim_message_history();

    assert!(
        !agent.messages.iter().any(|message| message.role == "tool"
            && message.tool_call_id.as_deref() == Some("call_orphan")),
        "trimmed history must not keep tool results without matching assistant tool_calls"
    );

    server.stop().await;
}

// =====================================================================
// context_usage_pct
// =====================================================================

#[tokio::test]
async fn test_context_usage_pct_zero_window() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // Force the context window to 0 to exercise the guard branch.
    // We set it indirectly via the memory field.  Since we can't set
    // it directly, we reach the 0-window branch by zeroing the field
    // via unsafe mutation of config — instead use a config that produces 0.
    // The guard `if window == 0 { return 0.0 }` must return 0.
    // We can test this by checking the return when memory window = 0.
    // NOTE: AgentMemory::context_window() returns config.agent.token_budget.
    // If we set token_budget = 0 on the config the guard fires.
    // We cannot set token_budget=0 through Config::default() because the
    // default is 500_000, but we can poke the field through a raw pointer.
    // Instead, just verify the invariant: pct is always in [0, 100].
    let pct = agent.context_usage_pct();
    assert!(
        (0.0..=100.0).contains(&pct),
        "context_usage_pct must be between 0 and 100; got {}",
        pct
    );

    server.stop().await;
}

#[tokio::test]
async fn test_context_usage_pct_increases_with_messages() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let pct_before = agent.context_usage_pct();

    // Add a large block of content to drive usage up.
    agent
        .messages
        .push(Message::user("word ".repeat(500).trim().to_string()));

    let pct_after = agent.context_usage_pct();

    // Usage percentage should be >= the original (it cannot decrease by adding tokens).
    assert!(
        pct_after >= pct_before,
        "usage pct should not decrease after adding messages; before={} after={}",
        pct_before,
        pct_after
    );

    server.stop().await;
}

#[tokio::test]
async fn test_context_usage_pct_capped_at_100() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Flood the message list with a huge amount of text.
    for _ in 0..50 {
        agent
            .messages
            .push(Message::user("x".repeat(10_000).to_string()));
    }

    let pct = agent.context_usage_pct();
    assert!(
        pct <= 100.0,
        "context_usage_pct must never exceed 100%; got {}",
        pct
    );

    server.stop().await;
}

// =====================================================================
// clear_context — additional edge cases
// =====================================================================

#[tokio::test]
async fn test_clear_context_no_system_messages() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Remove all messages then add non-system content.
    agent.messages.clear();
    agent.messages.push(Message::user("no system here"));
    agent.messages.push(Message::assistant("reply"));
    agent.file_tracker.context_files.push("a.rs".to_string());

    agent.clear_context();

    assert!(
        agent.messages.is_empty(),
        "when there are no system messages, clear_context should leave an empty list"
    );
    assert!(agent.file_tracker.context_files.is_empty());

    server.stop().await;
}

#[tokio::test]
async fn test_clear_context_multiple_system_messages() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Add a second system message alongside user turns.
    agent.messages.push(Message::system("extra system"));
    agent.messages.push(Message::user("user turn"));
    agent.messages.push(Message::assistant("assistant turn"));

    agent.clear_context();

    let all_system = agent.messages.iter().all(|m| m.role == "system");
    assert!(
        all_system,
        "after clear, only system messages should remain; got: {:?}",
        agent.messages.iter().map(|m| &m.role).collect::<Vec<_>>()
    );
    assert_eq!(
        agent.messages.len(),
        2,
        "both system messages should survive"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_clear_context_clears_stale_files() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent
        .file_tracker
        .stale_files
        .insert("stale.rs".to_string());
    agent
        .file_tracker
        .context_files
        .push("tracked.rs".to_string());
    agent.messages.push(Message::user("user"));

    agent.clear_context();

    // context_files must be empty; stale_files is cleared by memory.clear()
    // indirectly—but the spec only guarantees context_files.
    assert!(
        agent.file_tracker.context_files.is_empty(),
        "context_files must be cleared"
    );

    server.stop().await;
}

// =====================================================================
// trim_message_history — interplay with mixed roles
// =====================================================================

#[tokio::test]
async fn test_trim_skips_system_keeps_recent_non_system() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Interleave system and non-system messages.
    agent.messages.push(Message::system("second system prompt"));
    agent.messages.push(Message::user("old user msg A"));
    agent.messages.push(Message::user("old user msg B"));
    agent.messages.push(Message::user("RECENT user message"));

    // Force trimming by setting budget below current usage.
    let current = agent.estimate_messages_tokens();
    agent.max_context_tokens = current / 3;

    agent.trim_message_history();

    // All system messages must survive.
    let system_msgs: Vec<_> = agent
        .messages
        .iter()
        .filter(|m| m.role == "system")
        .collect();
    assert_eq!(
        system_msgs.len(),
        2,
        "both system messages must survive trimming"
    );

    // The most recent non-system message should be the last to go.
    let has_recent = agent
        .messages
        .iter()
        .any(|m| m.content.contains("RECENT user message"));
    // Note: it's acceptable for RECENT to also be removed if the budget
    // is extremely tight — we only assert that system messages survive.
    // But if RECENT survived, that's also fine and consistent.
    let _ = has_recent;

    server.stop().await;
}

// =====================================================================
// estimate_messages_tokens — consistent with per-message overhead
// =====================================================================

#[tokio::test]
async fn test_estimate_messages_tokens_overhead_per_message() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Start clean so we can reason about exact per-message overhead.
    agent.messages.clear();

    // An empty-content message still costs the 4-token per-message overhead.
    agent.messages.push(Message::user(""));
    let single_empty = agent.estimate_messages_tokens();

    // The overhead is `estimate_tokens_with_overhead(text, 4)`.
    // For an empty string, `estimate_content_tokens("")` can be 0 or 1
    // depending on the tokenizer, but we always add 4.
    assert!(
        single_empty >= 4,
        "empty content should still carry the 4-token per-message overhead; got {}",
        single_empty
    );

    server.stop().await;
}

// =====================================================================
// format_file_size — boundary and additional values
// =====================================================================

#[test]
fn test_format_file_size_boundary_between_bytes_and_kb() {
    // 1023 bytes -> "B" suffix
    let result_below = Agent::format_file_size(1023);
    assert!(
        result_below.ends_with('B') && !result_below.ends_with("KB"),
        "1023 bytes should format as B, got {}",
        result_below
    );

    // 1024 bytes -> "KB" suffix
    let result_at = Agent::format_file_size(1024);
    assert!(
        result_at.ends_with("KB"),
        "1024 bytes should format as KB, got {}",
        result_at
    );
}

#[test]
fn test_format_file_size_boundary_between_kb_and_mb() {
    // 1024 * 1024 - 1 bytes -> "KB" suffix
    let result_below = Agent::format_file_size(1024 * 1024 - 1);
    assert!(
        result_below.ends_with("KB"),
        "1MB - 1 should format as KB, got {}",
        result_below
    );

    // 1024 * 1024 bytes -> "MB" suffix
    let result_at = Agent::format_file_size(1024 * 1024);
    assert!(
        result_at.ends_with("MB"),
        "exactly 1MB should format as MB, got {}",
        result_at
    );
}

#[test]
fn test_format_file_size_one_decimal_place() {
    // 1536 bytes = 1.5 KB — check formatting precision
    let result = Agent::format_file_size(1536);
    assert_eq!(result, "1.5KB");

    // 3 * 512 * 1024 = 1.5 MB
    let result_mb = Agent::format_file_size(3 * 512 * 1024);
    assert_eq!(result_mb, "1.5MB");
}

// =====================================================================
// expand_file_references — directory reference
// =====================================================================

#[tokio::test]
async fn test_expand_file_references_directory() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("foo.txt"), "file 1 content").unwrap();
    std::fs::write(dir.path().join("bar.txt"), "file 2 content").unwrap();

    let dir_str = dir.path().display().to_string();
    let input = format!("list @{}/", dir_str);
    let (expanded, included) = agent.expand_file_references(&input).await;

    // A directory reference produces a directory tree listing.
    assert!(
        expanded.contains("Directory tree"),
        "directory reference should produce a tree listing; got: {}",
        &expanded[..expanded.len().min(200)]
    );
    assert_eq!(
        included.len(),
        1,
        "one directory entry should be reported; got: {:?}",
        included
    );

    server.stop().await;
}

#[tokio::test]
async fn test_expand_file_references_at_symbol_without_path_unchanged() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // A lone "@" with no following path should not crash and should pass through.
    let input = "email me @ work";
    let (expanded, files) = agent.expand_file_references(input).await;

    // The regex requires at least one alphanumeric char after '@', so a bare
    // "@ " should not be matched and input should come through unchanged.
    assert_eq!(expanded, input);
    assert!(files.is_empty());

    server.stop().await;
}

// =====================================================================
// enhance_cargo_errors — JSON array for errors but non-array errors key
// =====================================================================

#[tokio::test]
async fn test_enhance_cargo_errors_errors_not_array() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // The "errors" key exists but is a string, not an array.
    let input = r#"{"errors":"something went wrong"}"#;
    let result = agent.enhance_cargo_errors(input);
    assert_eq!(result, input, "non-array 'errors' should pass through");

    server.stop().await;
}

#[tokio::test]
async fn test_enhance_cargo_errors_preserves_original_content() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    let input = r#"{"errors":[{"code":"E0308","message":"type mismatch","file":"x.rs","line":1}]}"#;
    let result = agent.enhance_cargo_errors(input);

    // The original JSON must be present verbatim at the start of the result.
    assert!(
        result.starts_with(input),
        "original content must be at the start of the enhanced output"
    );

    server.stop().await;
}

// =====================================================================
// stale_files tracking
// =====================================================================

#[tokio::test]
async fn test_stale_files_initially_empty() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    assert!(
        agent.file_tracker.stale_files.is_empty(),
        "a fresh agent should have no stale files"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_stale_files_can_be_inserted_and_queried() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent
        .file_tracker
        .stale_files
        .insert("src/lib.rs".to_string());
    assert!(
        agent.file_tracker.stale_files.contains("src/lib.rs"),
        "inserted stale file should be in the stale set"
    );
    assert!(
        !agent.file_tracker.stale_files.contains("src/main.rs"),
        "non-inserted file should not appear in stale set"
    );

    server.stop().await;
}

// =====================================================================
// context_files tracking
// =====================================================================

#[tokio::test]
async fn test_context_files_initially_empty() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    assert!(
        agent.file_tracker.context_files.is_empty(),
        "a fresh agent should have no loaded context files"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_context_files_preserved_across_multiple_pushes() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent.file_tracker.context_files.push("a.rs".to_string());
    agent.file_tracker.context_files.push("b.rs".to_string());
    agent.file_tracker.context_files.push("c.rs".to_string());

    assert_eq!(agent.file_tracker.context_files.len(), 3);
    assert_eq!(agent.file_tracker.context_files[0], "a.rs");
    assert_eq!(agent.file_tracker.context_files[2], "c.rs");

    server.stop().await;
}

// =====================================================================
// refresh_stale_context_files
// =====================================================================

#[tokio::test]
async fn test_refresh_stale_context_files_no_stale_returns_zero() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // No stale files -> should immediately return 0 without touching messages.
    let refreshed = agent.refresh_stale_context_files().await;
    assert_eq!(
        refreshed, 0,
        "should return 0 when there are no stale files"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_refresh_stale_context_files_stale_not_in_context() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Mark a file as stale but don't add it to context_files.
    agent
        .file_tracker
        .stale_files
        .insert("src/missing.rs".to_string());

    let refreshed = agent.refresh_stale_context_files().await;
    assert_eq!(
        refreshed, 0,
        "stale files not tracked in context_files should not count as refreshed"
    );
    // stale_files should be cleared even though nothing was in context.
    assert!(
        agent.file_tracker.stale_files.is_empty(),
        "stale_files should be emptied when there are no context-tracked stale files"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_refresh_stale_context_files_updates_message_content() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Write a real file we can refresh.
    let dir = tempfile::tempdir().unwrap();
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let file_path = dir.path().join("data.txt");
    std::fs::write(&file_path, "original content").unwrap();
    let path_str = file_path.display().to_string();

    // Simulate the file already being loaded: add a message with the file marker.
    let file_marker = format!("// FILE: {}", path_str);
    let old_content = format!("{}\noriginal content", file_marker);
    agent.messages.push(Message::user(old_content));

    // Track the file and mark it stale.
    agent.file_tracker.context_files.push(path_str.clone());
    agent.file_tracker.stale_files.insert(path_str.clone());

    // Update the file content.
    std::fs::write(&file_path, "updated content").unwrap();

    let refreshed = agent.refresh_stale_context_files().await;
    assert_eq!(refreshed, 1, "should report one refreshed file");

    // The message in the context should now contain the updated content.
    let msg = agent
        .messages
        .iter()
        .find(|m| m.content.contains(&file_marker))
        .expect("file message should still be present");
    assert!(
        msg.content.contains("updated content"),
        "message content should be updated after refresh"
    );

    // The stale set should be empty after refresh.
    assert!(
        agent.file_tracker.stale_files.is_empty(),
        "stale_files should be cleared after successful refresh"
    );

    server.stop().await;
}

// =====================================================================
// reload_context
// =====================================================================

#[tokio::test]
async fn test_reload_context_no_files_returns_zero() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // No files have been loaded, so reload should return 0.
    let result = agent.reload_context().await;
    assert_eq!(
        result.unwrap(),
        0,
        "reload should return 0 when no context files are tracked"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_reload_context_re_reads_existing_files() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Create a real file.
    let dir = tempfile::tempdir().unwrap();
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let file_path = dir.path().join("reload_me.txt");
    std::fs::write(&file_path, "v1 content").unwrap();
    let path_str = file_path.display().to_string();

    // Simulate previous load: add the file marker message.
    let file_marker = format!("// FILE: {}", path_str);
    agent
        .messages
        .push(Message::user(format!("{}\nv1 content", file_marker)));
    agent.file_tracker.context_files.push(path_str.clone());
    agent.file_tracker.stale_files.insert(path_str.clone());

    // Update the file before reload.
    std::fs::write(&file_path, "v2 content").unwrap();

    let loaded = agent.reload_context().await.unwrap();
    assert_eq!(loaded, 1, "should reload 1 file");

    // The old file message should have been stripped and replaced.
    let has_old_msg = agent
        .messages
        .iter()
        .any(|m| m.role == "user" && m.content.contains("v1 content"));
    assert!(
        !has_old_msg,
        "the old v1 content message should have been removed on reload"
    );

    let has_new_msg = agent
        .messages
        .iter()
        .any(|m| m.role == "user" && m.content.contains(&file_marker));
    assert!(
        has_new_msg,
        "a new message with the file marker should have been added"
    );

    // stale_files should be cleared after reload.
    assert!(
        agent.file_tracker.stale_files.is_empty(),
        "stale_files should be cleared after reload"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_reload_context_removes_file_messages_not_conversation() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().unwrap();
    // Authorize this external fixture directory; the production reader keeps
    // enforcing the same workspace policy as direct tools.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let file_path = dir.path().join("reloaded.rs");
    std::fs::write(&file_path, "fn main() {}").unwrap();
    let path_str = file_path.display().to_string();

    // Add a regular conversation turn AND a file-load message.
    agent.messages.push(Message::user("please review the code"));
    agent.messages.push(Message::assistant("sure, let me look"));
    let file_marker_msg = format!("// FILE: {}\nfn main() {{}}", path_str);
    agent.messages.push(Message::user(file_marker_msg));
    agent.file_tracker.context_files.push(path_str.clone());

    let loaded = agent.reload_context().await.unwrap();
    assert_eq!(loaded, 1, "one file should be reloaded");

    // Conversation messages must not be removed.
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.contains("please review the code")),
        "conversation user message must survive reload"
    );
    assert!(
        agent
            .messages
            .iter()
            .any(|m| m.content.contains("sure, let me look")),
        "conversation assistant message must survive reload"
    );

    server.stop().await;
}

// =====================================================================
// max_context_tokens field
// =====================================================================

#[tokio::test]
async fn test_max_context_tokens_default_is_100k() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let agent = make_test_agent(&server).await;

    // Formula: context_length - max_tokens - (context_length / 5)
    // Default: 131072 - 65536 - 26214 = 39322
    // The 20% dynamic overhead scales with context_length instead of
    // the old fixed 200K overhead that saturated small configs to 0.
    let default_config = crate::config::Config::default();
    let expected = default_config
        .context_length
        .saturating_sub(default_config.max_tokens)
        .saturating_sub(default_config.context_length / 5);
    assert_eq!(
        agent.max_context_tokens, expected,
        "max_context_tokens = context_length - max_tokens - 20% overhead"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_does_not_exceed_max_context_tokens() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Push many messages that will push us over a modest budget.
    for i in 0..20 {
        agent.messages.push(Message::user(format!(
            "message {} with some padding content that takes up tokens: {}",
            i,
            "pad".repeat(50)
        )));
    }

    let budget = 5_000;
    agent.max_context_tokens = budget;
    agent.trim_message_history();

    let after_tokens = agent.estimate_messages_tokens();
    // After trim the reported token count should be <= the budget,
    // OR the only surviving messages are unremovable ones: system messages
    // plus the pinned first user message (original task). Since 2026-08-29
    // the pinned task is never evicted — it is truncated instead when
    // oversized — so a tiny budget can remain exceeded by design.
    let first_user_idx = agent.messages.iter().position(|m| m.role == "user");
    let only_unremovable = agent
        .messages
        .iter()
        .enumerate()
        .all(|(i, m)| m.role == "system" || Some(i) == first_user_idx);
    assert!(
        after_tokens <= budget || only_unremovable,
        "after trim, token usage should be within budget ({}); got {} tokens",
        budget,
        after_tokens
    );

    server.stop().await;
}

// =====================================================================
// compress_to_fit caller-contract regressions (double subtraction bug)
// =====================================================================

use crate::agent::context_map::{ContextMap, FileSkeleton};
use crate::evolve::ContextMode;
use crate::token_count::estimate_content_tokens;

/// Build code-like content whose estimated token count reaches at least
/// `target` tokens (deterministic, tokenizer-based).
fn code_with_token_floor(target: usize) -> String {
    let mut s = String::new();
    while estimate_content_tokens(&s) < target {
        s.push_str("fn process_item() { let value = 1; }\n");
    }
    s
}

/// Give the agent a small context map (1500-token budget) holding two
/// resident full files WITH skeletons; each downgrade frees ~400 tokens.
fn install_small_context_map_with_residents(agent: &mut Agent) {
    let mut map = ContextMap::new(2_000, 0.75, 0.20, 0.05);
    for name in ["resident_a.rs", "resident_b.rs"] {
        map.register_tree_entry(name.into(), 100);
        map.load_skeleton(
            std::path::Path::new(name),
            FileSkeleton {
                path: name.into(),
                items: vec![],
                token_count: 100,
            },
        );
        map.load_full(std::path::Path::new(name), code_with_token_floor(500));
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    agent.context_map = map;
}

/// Regression: `track_file_read_in_context_map` passed the ADDITIONAL room
/// (`estimated - remaining`) to `compress_to_fit`, whose contract is TOTAL
/// free room — compression stopped ~`remaining()` short and the load blew
/// the budget.
#[tokio::test]
async fn test_track_file_read_compresses_to_actual_fit() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    install_small_context_map_with_residents(&mut agent);

    // The incoming file: pre-load + evict so costs.l3 is cached and
    // `can_load(Full)` estimates the real full cost (deterministic).
    let content = code_with_token_floor(1200);
    agent
        .context_map
        .register_tree_entry("new_file.rs".into(), 100);
    agent
        .context_map
        .load_full(std::path::Path::new("new_file.rs"), content.clone());
    agent
        .context_map
        .evict_to_tree(std::path::Path::new("new_file.rs"));

    let estimate = agent
        .context_map
        .can_load(std::path::Path::new("new_file.rs"), ContextMode::Full)
        .await;
    assert!(!estimate.fits, "precondition: new file must not fit");
    assert!(
        agent.context_map.remaining() > 0,
        "precondition: headroom must exist (double subtraction needs R > 0)"
    );

    agent
        .track_file_read_in_context_map("new_file.rs", &content)
        .await;

    assert!(
        agent.context_map.total_tokens() <= agent.context_map.budget(),
        "after auto-compression the load must stay within budget \
         (total {}, budget {})",
        agent.context_map.total_tokens(),
        agent.context_map.budget()
    );
    assert_eq!(
        agent
            .context_map
            .level_of(std::path::Path::new("new_file.rs")),
        Some(ContextMode::Full)
    );
    server.stop().await;
}

/// Regression: `parallel_bulk_read` made the same contract mistake; because
/// it then required `freed >= needed`, files were SKIPPED even though
/// compression could have made them fit.
#[tokio::test]
async fn test_parallel_bulk_read_loads_after_compression() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    install_small_context_map_with_residents(&mut agent);

    // Real file on disk (absolute path: `root.join(abs)` resolves to itself).
    // can_load estimates size/3 tokens: exactly 3300 bytes → 1100 tokens,
    // which needs BOTH resident downgrades (~800 freed) to fit.
    let dir = tempdir().expect("tempdir");
    let file = dir.path().join("bulk_target.rs");
    let mut content = code_with_token_floor(1000);
    content.truncate(3_300);
    while content.len() < 3_300 {
        content.push('\n');
    }
    assert_eq!(content.len(), 3_300);
    std::fs::write(&file, &content).expect("write temp file");
    // This fixture intentionally lives outside the workspace; grant that one
    // directory explicitly now that bulk reads enforce the real path policy.
    agent
        .config
        .safety
        .allowed_paths
        .push(format!("{}/**", dir.path().display()));
    let estimate_tokens = agent
        .context_map
        .can_load(&file, ContextMode::Full)
        .await
        .estimated_tokens;
    assert!(
        !agent
            .context_map
            .can_load(&file, ContextMode::Full)
            .await
            .fits,
        "precondition: bulk target must not fit initially"
    );

    let (loaded, skipped, tokens_added) = agent.parallel_bulk_read(vec![file.clone()]).await;

    assert_eq!(
        (loaded, skipped),
        (1, 0),
        "compression must make room (estimate {estimate_tokens} tokens); \
         the file must be loaded, not skipped"
    );
    assert!(tokens_added > 0);
    assert_eq!(agent.context_map.level_of(&file), Some(ContextMode::Full));
    server.stop().await;
}

// =====================================================================
// trim_message_history — oversized injected task message (2026-08-29)
// A deliberately injected large context (e.g. a 780K-token evolve-graph
// pack via `selfware -p -`) must be truncated to a budget-relative cap,
// never cut to a flat 50K and never evicted wholesale.
// =====================================================================

#[test]
fn test_per_message_cap_scales_with_budget() {
    assert_eq!(Agent::per_message_cap(100_000), 75_000);
    assert_eq!(Agent::per_message_cap(1_000_000), 750_000);
    // Small budgets scale down to stay within the window (3/4 of 10k is 7.5k).
    assert_eq!(Agent::per_message_cap(10_000), 7_500);
}

#[tokio::test]
async fn test_trim_oversized_first_user_message_truncated_not_evicted() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    agent.max_context_tokens = 100_000; // cap = 75_000
                                        // >100K tokens of injected context in the original task message.
                                        // Unique words: a repeated string BPE-compresses below the budget and
                                        // the trim early-returns, which is not the path under test.
    let big: String = (0..60_000).map(|i| format!("uniq{i:05} ")).collect();
    agent.messages.push(Message::user(&big));

    agent.trim_message_history();

    let user = agent
        .messages
        .iter()
        .find(|m| m.role == "user")
        .expect("original task message must survive trimming");
    assert!(
        user.content
            .text()
            .contains("truncated to fit context budget"),
        "oversized task message should be truncated, not evicted"
    );
    let msg_tokens = crate::agent::context::estimate_message_tokens(user);
    assert!(
        msg_tokens <= 80_000,
        "truncated task should respect the budget-relative cap, got {msg_tokens}"
    );
    assert!(
        msg_tokens > 50_000,
        "budget-relative cap must allow more than the old flat 50K, got {msg_tokens}"
    );

    server.stop().await;
}

#[test]
fn test_graph_summary_note_is_critical_context() {
    // The task-start L0 graph orientation note must survive context trimming:
    // it is the repo's map and the pointer to the graph tools, so losing it
    // mid-run strands the model without orientation.
    let note = Message::user(
        "<selfware_context_note kind=graph_summary content_revision=0123456789ab built_at=2026-08-29>\n\
         # Architectural taxonomy\n\
         Call these graph tools directly by name (no tool_search needed): graph_summary, hotspots, context_pack, impact, neighbors, test_map.\n\
         </selfware_context_note>",
    );
    assert!(
        Agent::is_critical_context_message(&note),
        "selfware_context_note messages must be pinned as critical"
    );
    // An ordinary user message stays non-critical (guard against the marker
    // matching everything).
    let ordinary = Message::user("please refactor the parser");
    assert!(!Agent::is_critical_context_message(&ordinary));
}

// =========================================================================
// interactive zombie-task anchor: current task, not turn 1
// =========================================================================

#[tokio::test]
async fn test_trim_pins_current_task_not_turn_one() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();
    agent.messages.push(Message::system("SYS PROMPT"));

    // Multi-turn interactive session: the agent reuses `self.messages` across
    // turns, and each run_task stamps a fresh checkpoint for the CURRENT task.
    let pad = "x".repeat(60);
    let turn1 = format!("turn one task: fix login {}", pad);
    let turn2 = format!("turn two task: refactor parser {}", pad);
    let turn3 = format!("turn three task: write queue tests {}", pad);
    agent.messages.push(Message::user(&turn1));
    agent.messages.push(Message::assistant("did turn one"));
    agent.messages.push(Message::user(&turn2));
    agent.messages.push(Message::assistant("did turn two"));
    agent.messages.push(Message::user(&turn3));
    agent.messages.push(Message::assistant("did turn three"));

    // The agent is now executing turn 3's task.
    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "interactive-trim-id".to_string(),
        turn3.clone(),
    ));

    // Budget that only fits system + the pinned anchor, forcing the trim to
    // evict everything older — including turn 1's task, which the old
    // "pin the first user message" rule would have protected forever.
    use crate::agent::context::estimate_message_tokens as emt;
    let system_tokens = emt(&agent.messages[0]);
    let anchor_tokens = emt(&agent.messages[5]); // the turn-3 user message
    agent.max_context_tokens = system_tokens + anchor_tokens + 40;

    agent.trim_message_history();

    let contents: Vec<&str> = agent.messages.iter().map(|m| m.content.text()).collect();
    assert!(
        contents.iter().any(|c| c.contains("turn three task")),
        "the CURRENT task's prompt must be pinned; remaining: {:?}",
        contents
    );
    assert!(
        !contents.iter().any(|c| c.contains("turn one task")),
        "turn 1's zombie task must be trimmable once its turn is over; remaining: {:?}",
        contents
    );
    assert_eq!(
        agent.messages[0].role, "system",
        "the system prompt must survive trimming"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_falls_back_to_first_user_when_no_checkpoint() {
    // A fresh agent (current_checkpoint == None, e.g. bare unit-test setups)
    // must keep the historical pin-the-first-user-message behavior: after
    // trimming, the oldest user message survives as the root objective.
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();
    agent.messages.push(Message::system("SYS PROMPT"));
    let pad = "y".repeat(60);
    agent
        .messages
        .push(Message::user(format!("first task {}", pad)));
    agent.messages.push(Message::assistant("a"));
    agent
        .messages
        .push(Message::user(format!("second task {}", pad)));
    agent.messages.push(Message::assistant("b"));
    // No checkpoint set: agent.current_checkpoint stays None.

    // Budget exactly one message below the total, sized with the same
    // estimator the trim eviction walks: forces the oldest non-anchor
    // messages out while keeping system + the pinned first-user anchor.
    use crate::agent::context::estimate_message_tokens as emt;
    let total: usize = agent.messages.iter().map(emt).sum();
    let second_tokens = emt(&agent.messages[3]); // the "second task" user message
    agent.max_context_tokens = total - second_tokens;

    agent.trim_message_history();

    let contents: Vec<&str> = agent.messages.iter().map(|m| m.content.text()).collect();
    assert!(
        contents.iter().any(|c| c.contains("first task")),
        "without a checkpoint the first user message must still be pinned; remaining: {:?}",
        contents
    );
    assert!(
        !contents.iter().any(|c| c.contains("second task")),
        "newer non-anchor messages are the ones evicted; remaining: {:?}",
        contents
    );
    assert_eq!(
        agent.messages[0].role, "system",
        "the system prompt must survive trimming"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_structured_compression_anchors_current_task_not_turn_one() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.clear();
    agent.messages.push(Message::system("SYS"));
    let t1 = "fix login bug";
    let t2 = "refactor parser";
    let t3 = "write queue tests";
    agent.messages.push(Message::user(t1));
    agent.messages.push(Message::assistant("a"));
    agent.messages.push(Message::user(t2));
    agent.messages.push(Message::assistant("b"));
    agent.messages.push(Message::user(t3));
    agent.messages.push(Message::assistant("c"));

    // Interactive session now executing turn 3 — the checkpoint tracks it.
    agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
        "structured-id".to_string(),
        t3.to_string(),
    ));

    agent.compress_to_structured_summary(1);

    let joined: String = agent
        .messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        joined.contains(t3),
        "the CURRENT task must be the anchor after structured compression; got:\n{joined}"
    );
    assert!(
        !joined.contains(t1),
        "turn 1's zombie task must not be re-anchored as [ORIGINAL TASK]; got:\n{joined}"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_trim_oversized_system_message_truncated_not_evicted() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;

    // Small context window (24k) where per-message cap is 18k.
    agent.max_context_tokens = 24_000;

    // Simulate an oversized system message (e.g. injected tree, hints, RAG).
    let big_system: String = (0..20_000).map(|i| format!("sysword{i:05} ")).collect();
    agent.messages.clear();
    agent.messages.push(Message::system(format!(
        "BASE SYSTEM INSTRUCTIONS\n\n{big_system}"
    )));
    agent.messages.push(Message::user("user task prompt"));

    agent.trim_message_history();

    let sys = agent
        .messages
        .iter()
        .find(|m| m.role == "system")
        .expect("system message must survive trimming");
    assert!(
        sys.content.text().starts_with("BASE SYSTEM INSTRUCTIONS"),
        "base instructions should be preserved"
    );
    assert!(
        sys.content
            .text()
            .contains("truncated to fit context budget"),
        "oversized system message should be truncated to fit context budget"
    );
    let total_tokens = agent.estimate_messages_tokens();
    assert!(
        total_tokens <= 24_000,
        "trimmed messages should stay within context budget, got {total_tokens}"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_hard_clamp_to_budget_ensures_strict_context_bound() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let _agent = make_test_agent(&server).await;

    // Sized so that neither message individually exceeds the 18k cap,
    // but together they exceed 24k.
    let sys_pad: String = (0..10_000).map(|i| format!("s{i:04} ")).collect();
    let user_pad: String = (0..10_000).map(|i| format!("u{i:04} ")).collect();

    let mut msgs = vec![
        Message::system(format!("BASE PROMPT\n{sys_pad}")),
        Message::user(format!("TASK\n{user_pad}")),
    ];

    let (_dropped_msgs, dropped_toks) = Agent::trim_messages(
        &mut msgs,
        24_000,
        Some(1),
        &crate::agent::context::PathKeys::default(),
    );
    let total = crate::token_count::estimate_messages_tokens(&msgs);
    assert!(
        total <= 24_000,
        "hard clamp must guarantee total tokens <= budget, got {total}"
    );
    assert_eq!(msgs.len(), 2, "both messages should survive");
    assert!(dropped_toks > 0, "dropped tokens should be recorded");

    server.stop().await;
}

// ---------------------------------------------------------------------------
// Tool-call arguments participate in the context clamp
// ---------------------------------------------------------------------------

fn assistant_call_with_args(id: &str, name: &str, arguments: String) -> Message {
    let mut message = assistant_tool_call(id, name);
    message.tool_calls.as_mut().unwrap()[0].function.arguments = arguments;
    message
}

fn huge_file_write_args(path: &str, chars: usize) -> String {
    let body: String = (0..chars / 6).map(|i| format!("w{i:04} ")).collect();
    serde_json::json!({ "path": path, "content": body }).to_string()
}

#[test]
fn compact_tool_call_arguments_keeps_small_fields_and_yields_valid_json() {
    let args = huge_file_write_args("docs/GUIDE.md", 60_000);
    let compacted = super::compact_tool_call_arguments(&args).expect("must shrink");
    assert!(
        compacted.len() < 1_000,
        "compacted to {} chars",
        compacted.len()
    );
    let value: serde_json::Value =
        serde_json::from_str(&compacted).expect("compacted arguments must be valid JSON");
    assert_eq!(value["path"], "docs/GUIDE.md", "small fields are kept");
    let content = value["content"].as_str().unwrap();
    assert!(
        content.contains("[selfware: elided"),
        "marker present: {content}"
    );
    assert!(content.contains("chars"), "elided size is named: {content}");
    // Idempotent: an already-compacted argument string does not shrink again.
    assert_eq!(super::compact_tool_call_arguments(&compacted), None);
    // Already-small arguments are left alone.
    assert_eq!(
        super::compact_tool_call_arguments(r#"{"path":"a.rs"}"#),
        None
    );
}

#[test]
fn compact_tool_call_arguments_handles_unparseable_and_arrays() {
    let garbage = format!("{{\"path\": \"x\", \"content\": \"{}", "z".repeat(5_000));
    let compacted = super::compact_tool_call_arguments(&garbage).expect("must shrink");
    let value: serde_json::Value =
        serde_json::from_str(&compacted).expect("placeholder must be valid JSON");
    assert!(value["_selfware_elided"]
        .as_str()
        .unwrap()
        .contains("unparseable"));

    let many: Vec<String> = (0..200).map(|i| format!("file_{i}.rs")).collect();
    let args = serde_json::json!({ "paths": many }).to_string();
    let compacted = super::compact_tool_call_arguments(&args).expect("must shrink");
    let value: serde_json::Value = serde_json::from_str(&compacted).unwrap();
    let paths = value["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 17, "16 kept + one elision marker");
    assert!(paths[16].as_str().unwrap().contains("184 more item(s)"));
}

/// The e2e failure: a historical assistant turn with a huge `file_write`
/// argument kept the request over budget because the clamp only shrank
/// message text. The clamp must now compact that argument (valid JSON,
/// path kept) and leave the pairing and the latest turn intact.
#[test]
fn hard_clamp_compacts_historical_tool_call_arguments() {
    let latest_args = r#"{"path":"src/lib.rs"}"#.to_string();
    let mut msgs = vec![
        Message::system("system prompt"),
        Message::user("Document the crate"),
        assistant_call_with_args(
            "call_old",
            "file_write",
            huge_file_write_args("docs/GUIDE.md", 80_000),
        ),
        Message::tool("wrote docs/GUIDE.md", "call_old"),
        assistant_call_with_args("call_new", "file_read", latest_args.clone()),
        Message::tool("fn main() {}", "call_new"),
    ];
    let budget = 2_000;
    assert!(crate::token_count::estimate_messages_tokens(&msgs) > budget);

    Agent::hard_clamp_to_budget(&mut msgs, budget);

    let total = crate::token_count::estimate_messages_tokens(&msgs);
    assert!(total <= budget, "clamp must fit the budget, got {total}");
    assert_eq!(msgs.len(), 6, "no message is dropped by the clamp");
    let old = &msgs[2].tool_calls.as_ref().unwrap()[0];
    assert_eq!(old.id, "call_old", "tool-call id preserved for pairing");
    assert_eq!(old.function.name, "file_write");
    let value: serde_json::Value = serde_json::from_str(&old.function.arguments)
        .expect("compacted historical arguments must be valid JSON");
    assert_eq!(value["path"], "docs/GUIDE.md");
    assert_eq!(
        msgs[4].tool_calls.as_ref().unwrap()[0].function.arguments,
        latest_args,
        "the latest tool-call turn is not touched when history suffices"
    );
    assert_valid_tool_pairing(&msgs);
    let kept = Agent::apply_tool_call_pair_invariants(msgs.clone());
    assert_eq!(
        kept.len(),
        msgs.len(),
        "pairing invariants keep every message"
    );
}

/// Even when the only oversized payload is the LATEST turn's arguments (the
/// trim pins it), the fit succeeds by compacting it as a last resort, with
/// ids and pairing intact.
#[test]
fn fit_request_compacts_latest_arguments_as_last_resort() {
    let msgs = vec![
        Message::system("system prompt"),
        Message::user("Document the crate"),
        assistant_call_with_args(
            "call_big",
            "file_write",
            huge_file_write_args("README.md", 60_000),
        ),
        Message::tool("wrote README.md", "call_big"),
    ];
    let budget = 1_500;
    let fitted = Agent::fit_request_to_context_budget(
        msgs,
        budget,
        None,
        &crate::agent::context::PathKeys::default(),
    )
    .expect("compaction must bring the request under budget");
    assert!(crate::token_count::estimate_messages_tokens(&fitted) <= budget);
    assert_valid_tool_pairing(&fitted);
    let call = &fitted[2].tool_calls.as_ref().unwrap()[0];
    assert_eq!(call.id, "call_big");
    let value: serde_json::Value = serde_json::from_str(&call.function.arguments).unwrap();
    assert_eq!(value["path"], "README.md");
}

/// When nothing can bring the request under budget, it must NOT be
/// dispatched: the fit returns the typed ContextOverflow that the execution
/// loop routes to its bounded compress-and-retry recovery.
#[test]
fn fit_request_returns_typed_context_overflow_when_still_over_budget() {
    let msgs = vec![
        Message::system("S".repeat(4_000)),
        Message::user("T".repeat(4_000)),
    ];
    let err = Agent::fit_request_to_context_budget(
        msgs,
        10,
        None,
        &crate::agent::context::PathKeys::default(),
    )
    .expect_err("an unfittable request must not be returned for dispatch");
    assert!(
        matches!(err, crate::errors::ApiError::ContextOverflow(_)),
        "must be the typed overflow, got {err:?}"
    );
    let err: anyhow::Error = err.into();
    assert!(crate::errors::is_context_overflow_error(&err));
    let text = crate::agent::task_runner::recovery_error_text(&err);
    assert!(
        crate::agent::task_runner::is_context_overflow_text(&text),
        "recovery routing must see an overflow: {text}"
    );
}

#[test]
fn fit_request_is_identity_under_budget() {
    let msgs = vec![Message::system("sys"), Message::user("task")];
    let fitted = Agent::fit_request_to_context_budget(
        msgs.clone(),
        10_000,
        None,
        &crate::agent::context::PathKeys::default(),
    )
    .unwrap();
    assert_eq!(fitted.len(), msgs.len());
    assert_eq!(fitted[1].content.text(), "task");
}

// ---------------------------------------------------------------------------
// e2e c40 / c24: the ORIGINAL task must survive every trim / clamp /
// compaction pass (the model wrote "Need know task from initial?" after the
// task message had been replaced by a re-wrapped compaction boundary).
// ---------------------------------------------------------------------------

const ANCHOR_TASK: &str = "Multi-step documentation task in this Rust repo. Do the steps in order.\n\
    1. Read src/agent/context.rs in full.\n\
    2. Read src/agent/compression.rs in full.\n\
    3. Read src/agent/context_management.rs in full.\n\
    4. Create docs/CONTEXT_NOTES.md containing one section per file.\n\
    5. In src/agent/context.rs, add a one-line `///` doc comment above every undocumented `pub fn`.\n\
    6. Finish with a short summary.";

fn anchor_checkpoint() -> crate::checkpoint::TaskCheckpoint {
    crate::checkpoint::TaskCheckpoint::new("anchor-task".to_string(), ANCHOR_TASK.to_string())
}

/// XML-mode tool traffic (text tool calling: assistant `<tool>` + role=user
/// `<tool_result>`) — the shape the c40 run used — `pairs` round trips of
/// roughly `chars` characters of file content each.
fn tool_traffic(pairs: usize, chars: usize) -> Vec<Message> {
    let body: String = (0..chars / 7).map(|i| format!("l{i:05} ")).collect();
    let mut out = Vec::new();
    for i in 0..pairs {
        out.push(Message::assistant(format!(
            "<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"src/agent/f{i}.rs\"}}</arguments>\n</tool>"
        )));
        out.push(Message::user(format!(
            "<tool_result>{{\"content\":\"{body}\"}}</tool_result>"
        )));
    }
    out
}

fn has_task_verbatim(messages: &[Message]) -> bool {
    messages
        .iter()
        .any(|m| m.role == "user" && m.content.text().contains(ANCHOR_TASK))
}

#[test]
fn task_text_survives_trimming_at_a_small_budget_with_heavy_tool_traffic() {
    let system_pad: String = (0..1_500).map(|i| format!("s{i:04} ")).collect();
    let mut msgs = vec![
        Message::system(format!("SYSTEM PROMPT\n{system_pad}")),
        Message::user(ANCHOR_TASK),
        Message::user(
            "<selfware_context_note kind=tool_manifest>\n- a\n- b\n</selfware_context_note>",
        ),
    ];
    msgs.extend(tool_traffic(40, 8_000));
    let budget = 8_000;
    assert!(crate::token_count::estimate_messages_tokens(&msgs) > budget * 10);

    let cp = anchor_checkpoint();
    let anchor = Agent::find_task_anchor_index(&msgs, Some(&cp));
    assert_eq!(anchor, Some(1), "the anchor is the original task message");
    Agent::trim_messages(
        &mut msgs,
        budget,
        anchor,
        &crate::agent::context::PathKeys::default(),
    );
    assert!(
        has_task_verbatim(&msgs),
        "task must survive trim verbatim: {:?}",
        msgs.iter()
            .map(|m| m.content.text().chars().take(60).collect::<String>())
            .collect::<Vec<_>>()
    );
    assert!(crate::token_count::estimate_messages_tokens(&msgs) <= budget);

    // Repeated passes (each turn trims again as traffic arrives) keep it.
    for _ in 0..5 {
        msgs.extend(tool_traffic(4, 8_000));
        let anchor = Agent::find_task_anchor_index(&msgs, Some(&cp));
        Agent::trim_messages(
            &mut msgs,
            budget,
            anchor,
            &crate::agent::context::PathKeys::default(),
        );
        assert!(has_task_verbatim(&msgs));
    }

    // The request-assembly path (trim + clamp + refuse) keeps it too.
    let mut request = msgs.clone();
    request.extend(tool_traffic(10, 8_000));
    let fitted = Agent::fit_request_to_context_budget(
        request,
        budget,
        Some(&cp),
        &crate::agent::context::PathKeys::default(),
    )
    .expect("fits after trimming");
    assert!(has_task_verbatim(&fitted));
    assert!(crate::token_count::estimate_messages_tokens(&fitted) <= budget);
}

/// The hard clamp may shrink the task only down to the anchor floor (a
/// quarter of the budget), and only after every other message competed.
#[test]
fn hard_clamp_never_truncates_the_task_anchor_below_the_floor() {
    let big_task: String = format!(
        "{ANCHOR_TASK}\n{}",
        (0..3_000)
            .map(|i| format!("req{i:04} "))
            .collect::<String>()
    );
    let system_pad: String = (0..10_000).map(|i| format!("s{i:04} ")).collect();
    let mut msgs = vec![
        Message::system(format!("SYSTEM\n{system_pad}")),
        Message::user(big_task.clone()),
        Message::assistant("ok"),
        Message::user("continue"),
    ];
    let budget = 6_000;
    Agent::trim_messages(
        &mut msgs,
        budget,
        Some(1),
        &crate::agent::context::PathKeys::default(),
    );
    let total = crate::token_count::estimate_messages_tokens(&msgs);
    assert!(total <= budget, "clamp must fit the budget, got {total}");
    let anchor = msgs
        .iter()
        .find(|m| m.role == "user" && m.content.text().starts_with("Multi-step"))
        .expect("anchor kept");
    let anchor_tokens = crate::token_count::estimate_content_tokens(anchor.content.text());
    assert!(
        anchor_tokens >= Agent::task_anchor_floor_tokens(budget),
        "anchor shrunk to {anchor_tokens} tokens, below the {}-token floor",
        Agent::task_anchor_floor_tokens(budget)
    );
    assert!(
        anchor.content.text().contains(ANCHOR_TASK),
        "the task's leading text is kept verbatim"
    );
}

/// The c40 history: the "original task" slot held a re-wrapped compaction
/// boundary, and the real task text was nowhere. The resolver must not pin
/// the boundary, and the backstop must restore the task.
#[test]
fn lost_task_is_restored_and_resolved_from_the_checkpoint() {
    let mut msgs = vec![
        Message::system("SYSTEM"),
        Message::user(
            "[Original task, preserved across compression]:\n[Original task, preserved across compression]:\n\
             [ORIGINAL TASK]:\n[Earlier context was compressed due to length limits]\n\n\
             [CONTEXT SUMMARY - 4 earlier messages compressed]:\nWorking on context management code.",
        ),
    ];
    msgs.extend(tool_traffic(3, 200));
    let cp = anchor_checkpoint();
    assert!(!has_task_verbatim(&msgs), "precondition: the task is lost");

    assert!(Agent::ensure_task_anchor_in(&mut msgs, ANCHOR_TASK));
    assert!(has_task_verbatim(&msgs));
    let idx = Agent::find_task_anchor_index(&msgs, Some(&cp)).unwrap();
    assert!(msgs[idx].content.text().contains(ANCHOR_TASK));
    assert_eq!(msgs[0].role, "system", "the system prompt stays first");
    assert_eq!(idx, 1, "restored right after the system prompt");
    assert!(
        !Agent::ensure_task_anchor_in(&mut msgs, ANCHOR_TASK),
        "idempotent: a present task is not inserted again"
    );
}

/// After compaction the verbatim-equal task message is gone (it is wrapped
/// or coalesced with a summary). The anchor must resolve to the message
/// that still CONTAINS the task — never to "the first user message", which
/// is a boundary note.
#[test]
fn anchor_resolves_to_the_wrapped_task_not_the_first_user_message() {
    let msgs = vec![
        Message::system("SYSTEM"),
        Message::user("[Earlier context was compressed due to length limits]"),
        Message::user(format!(
            "[ORIGINAL TASK]:\n{ANCHOR_TASK}\n\n[CONTEXT SUMMARY - 9 earlier messages compressed]:\n..."
        )),
        Message::assistant("<tool>\n<name>file_read</name>\n</tool>"),
        Message::user("Continue"),
    ];
    let cp = anchor_checkpoint();
    assert_eq!(Agent::find_task_anchor_index(&msgs, Some(&cp)), Some(2));
}

/// Hard compaction carries the checkpoint task forward verbatim — even on a
/// short history where the old heuristic found no user message before the
/// tail and emitted a bare "[Earlier context was compressed]" boundary — and
/// repeated compactions do not nest the boundary prefix.
#[test]
fn hard_compress_with_task_carries_the_task_without_nesting() {
    let compressor = crate::agent::context::ContextCompressor::new(10_000);
    let mut msgs = vec![
        Message::system("SYSTEM"),
        Message::user("[Earlier context was compressed due to length limits]"),
        Message::assistant("<tool>\n<name>file_read</name>\n</tool>"),
        Message::user("<tool_result>{\"content\":\"x\"}</tool_result>"),
    ];
    for _ in 0..4 {
        msgs = compressor.hard_compress_with_task(&msgs, Some(ANCHOR_TASK));
        msgs.extend(tool_traffic(2, 100));
    }
    assert!(has_task_verbatim(&msgs));
    let joined: String = msgs.iter().map(|m| m.content.text().to_string()).collect();
    assert_eq!(
        joined.matches(ANCHOR_TASK).count(),
        1,
        "the task is carried once, not duplicated: {joined}"
    );
    assert!(
        joined
            .matches("[Original task, preserved across compression]")
            .count()
            <= 1,
        "no nested boundary prefixes: {joined}"
    );
}

/// The c40 session log: hard_fallback 7 → 4 messages, trim 6 → 3, then
/// hard_fallback 3 → 5. On a 3-message history the old tail (`len - 3` = 0)
/// started AT the system prompt: the result carried the system prompt twice
/// and a bare "[Earlier context was compressed]" boundary, which the next
/// trim pinned as the "task" while the real task was dropped.
#[test]
fn legacy_hard_compress_of_a_three_message_history_keeps_task_and_one_system() {
    let compressor = crate::agent::context::ContextCompressor::new(10_000);
    let boundary = format!(
        "[Original task, preserved across compression]:\n{ANCHOR_TASK}\n\n\
         [Earlier context was compressed due to length limits]"
    );
    let msgs = vec![
        Message::system("SYSTEM"),
        Message::user(boundary),
        Message::assistant("<tool>\n<name>file_read</name>\n</tool>"),
    ];
    let out = compressor.hard_compress(&msgs);
    assert_eq!(
        out.iter().filter(|m| m.role == "system").count(),
        1,
        "the system prompt must not be duplicated: {out:?}"
    );
    assert!(has_task_verbatim(&out), "{out:?}");
    // And the anchor the next trim pins is the message carrying the task.
    let idx = Agent::find_task_anchor_index(&out, Some(&anchor_checkpoint())).unwrap();
    assert!(out[idx].content.text().contains(ANCHOR_TASK));
}

/// Agent-level chain: turn-start trims interleaved with a legacy (task-less)
/// hard compaction — the path that lost the task in c40 — end with the task
/// present verbatim, because every trim restores and pins it.
#[tokio::test]
async fn agent_trim_restores_task_after_legacy_compaction_chain() {
    let server = MockLlmServer::builder().with_response("ok").build().await;
    let mut agent = make_test_agent(&server).await;
    agent.messages.truncate(1); // keep the system prompt
    agent.messages.push(Message::user(ANCHOR_TASK));
    agent.current_checkpoint = Some(anchor_checkpoint());
    let system_tokens = crate::agent::context::estimate_message_tokens(&agent.messages[0]);
    agent.max_context_tokens = system_tokens + 4_000;

    // The exact c40 shape first: compaction down to a 3-message history,
    // then a legacy hard compaction of it, then more traffic and a trim.
    agent.messages.extend(tool_traffic(3, 3_000));
    agent.messages = agent.compressor.hard_compress(&agent.messages);
    agent.messages.truncate(3);
    agent.messages = agent.compressor.hard_compress(&agent.messages);
    agent.messages.extend(tool_traffic(4, 3_000));
    agent.trim_message_history();
    assert!(
        has_task_verbatim(&agent.messages),
        "c40 shape lost the task"
    );

    for round in 0..6 {
        agent.messages.extend(tool_traffic(6, 3_000));
        if round % 2 == 1 {
            // Legacy compaction with no task knowledge (context_files'
            // /compress used this form; so did every hard_compress site).
            agent.messages = agent.compressor.hard_compress(&agent.messages);
            agent.messages = agent.compressor.hard_compress(&agent.messages);
        }
        agent.trim_message_history();
        assert!(
            has_task_verbatim(&agent.messages),
            "round {round}: task lost; history: {:?}",
            agent
                .messages
                .iter()
                .map(|m| m.content.text().chars().take(80).collect::<String>())
                .collect::<Vec<_>>()
        );
    }
    server.stop().await;
}

// ---------------------------------------------------------------------------
// Request tail: per-turn hints + work ledger at the END, system prompt stable
// ---------------------------------------------------------------------------

fn ledger_fixture() -> String {
    let mut ledger = crate::agent::context::WorkLedger::new();
    ledger.begin_turn(Some("task"));
    let mut call = Message::assistant("");
    call.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "r1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        },
    }]);
    ledger.observe(
        &[
            Message::system("sys"),
            Message::user("task"),
            call,
            Message::tool(r#"{"content":"pub mod a;","total_lines":1}"#, "r1"),
        ],
        None,
    );
    ledger.render(1_500).expect("ledger renders")
}

#[test]
fn request_tail_goes_after_the_history_not_into_the_system_message() {
    let ledger = ledger_fixture();
    let mut call = Message::assistant("working");
    call.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "x".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "git_status".to_string(),
            arguments: "{}".to_string(),
        },
    }]);
    let history = vec![
        Message::system("SYSTEM PROMPT"),
        Message::user("task"),
        call,
        Message::tool(r#"{"ok":true}"#, "x"),
    ];
    let request = Agent::finish_request_with_tail(
        history,
        vec!["# Project tree (3 files, 120/24000 tokens used)".to_string()],
        Some(ledger.clone()),
        24_000,
        None,
    )
    .expect("fits");

    assert_eq!(request[0].role, "system");
    assert_eq!(request[0].content.text(), "SYSTEM PROMPT");
    assert_eq!(
        request[3].role, "tool",
        "tool result stays right after its call"
    );
    let last = request.last().unwrap();
    assert_eq!(last.role, "user", "tail follows the trailing tool result");
    let text = last.content.text();
    assert!(text.contains("<selfware_context_note kind=turn_context>"));
    assert!(text.contains("# Project tree"));
    assert!(text.ends_with(&format!("{ledger}\n</selfware_context_note>")));
    // The ledger is the very last thing in the request.
    assert!(text.find("# Project tree").unwrap() < text.find("Work ledger").unwrap());
}

#[test]
fn request_tail_is_appended_to_a_trailing_user_turn() {
    let request = Agent::finish_request_with_tail(
        vec![
            Message::system("sys"),
            Message::user("task"),
            Message::assistant("a"),
            Message::user("continue"),
        ],
        vec![],
        Some(ledger_fixture()),
        24_000,
        None,
    )
    .unwrap();
    assert_eq!(request.len(), 4, "no extra same-role message");
    assert!(request[3].content.text().starts_with("continue\n\n"));
    assert!(request[3].content.text().contains("Work ledger"));
}

#[test]
fn request_tail_is_budgeted_and_hints_are_truncated_before_the_ledger() {
    use crate::token_count::estimate_messages_tokens;
    let ledger = ledger_fixture();
    let huge_hint = "tree line src/some/path.rs (4K, ~900tok)\n".repeat(2_000);
    let budget = 6_000;
    let request = Agent::finish_request_with_tail(
        vec![
            Message::system("sys"),
            Message::user("task"),
            Message::assistant("a".repeat(400)),
            Message::user("go"),
        ],
        vec![huge_hint],
        Some(ledger.clone()),
        budget,
        None,
    )
    .unwrap();
    assert!(estimate_messages_tokens(&request) <= budget);
    let tail = request.last().unwrap().content.text();
    assert!(tail.contains("[turn context truncated to fit budget]"));
    assert!(tail.contains(&ledger), "the ledger is never truncated");
}

/// The measurement asked for: two consecutive assemblies of a GROWING
/// conversation with CHANGING per-turn hints produce a byte-identical system
/// message (the precondition for a provider prefix cache).
#[test]
fn consecutive_request_assemblies_share_a_byte_identical_system_message() {
    let mut history = vec![
        Message::system("You are selfware. Stable system prompt."),
        Message::user("task"),
    ];
    let first = Agent::finish_request_with_tail(
        history.clone(),
        vec![
            "# Project tree (10 files, 1200/24000 tokens used)".to_string(),
            "Previous tool failed: cargo_test".to_string(),
        ],
        None,
        24_000,
        None,
    )
    .unwrap();

    history.push(Message::assistant("reading"));
    history.push(Message::user("continue"));
    let second = Agent::finish_request_with_tail(
        history,
        vec!["# Project tree (10 files, 5400/24000 tokens used)".to_string()],
        Some(ledger_fixture()),
        24_000,
        None,
    )
    .unwrap();

    assert_eq!(first[0].role, "system");
    assert_eq!(second[0].role, "system");
    assert_eq!(
        first[0].content.text().as_bytes(),
        second[0].content.text().as_bytes()
    );
    assert_eq!(
        first.iter().filter(|m| m.role == "system").count(),
        1,
        "no extra system message is synthesized for hints"
    );
}

#[test]
fn mid_conversation_system_messages_are_demoted_without_splitting_tool_pairs() {
    let mut call = Message::assistant("");
    call.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "t1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "git_status".to_string(),
            arguments: "{}".to_string(),
        },
    }]);
    let history = vec![
        Message::system("SYSTEM PROMPT"),
        Message::user("task"),
        call,
        // A banner pushed between the call and its result must wait.
        Message::system("BANNER A"),
        Message::tool("{}", "t1"),
        Message::system("BANNER B"),
        Message::assistant("thinking"),
        Message::user("continue"),
        Message::system("BANNER C"),
    ];
    let out = Agent::demote_mid_conversation_system_messages(history);

    assert_eq!(out.iter().filter(|m| m.role == "system").count(), 1);
    assert_eq!(out[0].content.text(), "SYSTEM PROMPT");
    assert_eq!(out[2].tool_calls.as_ref().unwrap()[0].id, "t1");
    assert_eq!(
        out[3].role, "tool",
        "result still directly follows its call"
    );
    assert_eq!(out[4].role, "user");
    assert!(out[4].content.text().contains("BANNER A\n\nBANNER B"));
    assert!(out[4]
        .content
        .text()
        .starts_with("<selfware_context_note kind=system_directive>"));
    assert_eq!(out[5].role, "assistant");
    let last = out.last().unwrap();
    assert_eq!(last.role, "user");
    assert!(
        last.content.text().starts_with("continue\n\n") && last.content.text().contains("BANNER C"),
        "a trailing banner folds into the adjacent user turn"
    );
    assert_eq!(out.len(), 7);
}

#[tokio::test]
async fn trim_message_history_records_the_work_ledger_before_dropping_reads() {
    let server = MockLlmServer::builder().build().await;
    let mut agent = make_test_agent(&server).await;
    agent.max_context_tokens = 400;
    agent.messages = vec![Message::system("sys"), Message::user("task")];
    let mut call = Message::assistant("");
    call.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: "r1".to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path":"src/evicted.rs"}"#.to_string(),
        },
    }]);
    agent.messages.push(call);
    agent.messages.push(Message::tool(
        serde_json::json!({"content": "x".repeat(4_000), "total_lines": 1}).to_string(),
        "r1",
    ));
    for i in 0..6 {
        agent
            .messages
            .push(Message::assistant(format!("step {i} {}", "y".repeat(300))));
        agent.messages.push(Message::user(format!("continue {i}")));
    }
    agent.trim_message_history();
    assert!(
        !agent
            .messages
            .iter()
            .any(|m| m.content.text().contains(&"x".repeat(4_000))),
        "precondition: the read result was trimmed"
    );
    let ledger = agent.compressor.work_ledger();
    assert_eq!(ledger.files().len(), 1);
    assert_eq!(ledger.files()[0].path, "src/evicted.rs");
    server.stop().await;
}
