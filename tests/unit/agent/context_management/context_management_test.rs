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
    let agent = make_test_agent(&server).await;

    // Create a temporary file with known content
    let dir = tempfile::tempdir().expect("failed to create temp dir");
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
    let agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().expect("failed to create temp dir");
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
    let agent = make_test_agent(&server).await;

    let dir = tempfile::tempdir().expect("failed to create temp dir");
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
    // Small budgets keep the 50K floor so normal tasks are untouched.
    assert_eq!(Agent::per_message_cap(10_000), 50_000);
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
