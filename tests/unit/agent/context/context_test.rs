use super::*;

/// Build a bare assistant message that declares a single tool call.
fn tool_call_message(id: &str, name: &str) -> Message {
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

#[test]
fn test_context_compressor_new() {
    let compressor = ContextCompressor::new(100000);
    assert_eq!(compressor.compression_threshold, 75000); // 75% content zone
    assert_eq!(compressor.min_messages_to_keep, 6);
}

#[test]
fn test_estimate_tokens_simple() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("Hello world"), // ~3 tokens + MESSAGE_OVERHEAD_TOKENS
    ];
    let estimate = compressor.estimate_tokens(&messages);
    assert!(estimate > 0, "should produce a non-zero estimate");
    assert!(estimate < 100, "single short message shouldn't be huge");
}

#[test]
fn test_estimate_tokens_code_content() {
    let compressor = ContextCompressor::new(100000);

    // Code content (with {}) uses factor 3
    let code_messages = vec![Message::user("fn main() { println!(\"hello\"); }")];

    // Plain text uses factor 4
    let text_messages = vec![Message::user("This is plain text without code")];

    let code_estimate = compressor.estimate_tokens(&code_messages);
    let text_estimate = compressor.estimate_tokens(&text_messages);

    // Both should produce positive estimates
    assert!(code_estimate > 0);
    assert!(text_estimate > 0);
}

#[test]
fn test_should_compress_small_context() {
    let compressor = ContextCompressor::new(100000);
    let small: Vec<Message> = vec![Message::system("test")];
    assert!(!compressor.should_compress(&small));
}

#[test]
fn test_should_compress_large_context() {
    let compressor = ContextCompressor::new(1000); // Small budget
    let mut large = vec![Message::system("test".repeat(10000))];
    for _ in 0..20 {
        large.push(Message::user("more content here".repeat(100)));
    }
    assert!(compressor.should_compress(&large));
}

#[test]
fn test_hard_compress_preserves_system() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system prompt"),
        Message::user("old1"),
        Message::assistant("response1"),
        Message::user("old2"),
        Message::assistant("response2"),
        Message::user("recent1"),
        Message::assistant("response3"),
        Message::user("recent2"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // First message should be system
    assert_eq!(compressed[0].role, "system");
    assert_eq!(compressed[0].content, "system prompt");
}

#[test]
fn test_hard_compress_keeps_recent() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("old1"),
        Message::user("old2"),
        Message::user("recent1"),
        Message::user("recent2"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Should keep system + compression note + last 3 messages
    assert!(compressed.len() >= 4);
    assert_eq!(compressed[0].role, "system");
}

#[test]
fn test_hard_compress_ends_with_user() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("user msg"),
        Message::assistant("assistant msg"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Should end with user message
    let last = compressed.last().unwrap();
    assert_eq!(last.role, "user");
}

#[test]
fn test_hard_compress_avoids_consecutive_assistants() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::assistant("response1"),
        Message::assistant("response2"), // consecutive
        Message::user("user msg"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Check no consecutive assistants
    for i in 0..compressed.len() - 1 {
        if compressed[i].role == "assistant" {
            assert_ne!(compressed[i + 1].role, "assistant");
        }
    }
}

#[test]
fn test_hard_compress_empty_messages() {
    let compressor = ContextCompressor::new(100000);
    let messages: Vec<Message> = vec![];

    let compressed = compressor.hard_compress(&messages);

    // Should handle empty gracefully
    assert!(compressed.is_empty() || compressed[0].role == "user");
}

#[test]
fn test_hard_compress_single_message() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![Message::system("only system")];

    let compressed = compressor.hard_compress(&messages);

    // Should keep system and add user prompt
    assert!(!compressed.is_empty());
}

#[test]
fn test_estimate_tokens_multiple_messages() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("System prompt"),
        Message::user("User question"),
        Message::assistant("Assistant response"),
    ];

    let estimate = compressor.estimate_tokens(&messages);

    // Should be sum of individual estimates
    assert!(estimate > 10); // 3 messages with short content
}

// Additional tests for improved coverage

#[test]
fn test_compression_threshold_calculation() {
    let compressor = ContextCompressor::new(10000);
    // Threshold should be 75% of budget (content zone)
    assert_eq!(compressor.compression_threshold, 7500);
}

#[test]
fn test_min_messages_to_keep() {
    let compressor = ContextCompressor::new(100000);
    assert_eq!(compressor.min_messages_to_keep, 6);
}

#[test]
fn test_estimate_tokens_empty() {
    let compressor = ContextCompressor::new(100000);
    let messages: Vec<Message> = vec![];
    let estimate = compressor.estimate_tokens(&messages);
    assert_eq!(estimate, 0);
}

#[test]
fn test_estimate_tokens_with_semicolons() {
    let compressor = ContextCompressor::new(100000);
    // Code with semicolons uses factor 3
    let messages = vec![Message::user("let x = 1; let y = 2; let z = 3;")];
    let estimate = compressor.estimate_tokens(&messages);
    // 31 chars / 3 + 50 = ~60
    assert!(estimate > 0 && estimate < 100);
}

#[test]
fn test_estimate_tokens_with_braces() {
    let compressor = ContextCompressor::new(100000);
    // Code with braces uses factor 3
    let messages = vec![Message::user("fn main() { println!(\"hello\"); }")];
    let estimate = compressor.estimate_tokens(&messages);
    assert!(estimate > 0 && estimate < 100);
}

#[test]
fn test_estimate_tokens_plain_text() {
    let compressor = ContextCompressor::new(100000);
    // Plain text without code markers uses factor 4
    let messages = vec![Message::user("This is plain text without any code")];
    let estimate = compressor.estimate_tokens(&messages);
    // Should be chars/4 + 50
    assert!(estimate > 0);
}

#[test]
fn test_should_compress_exact_threshold() {
    let compressor = ContextCompressor::new(1000);
    // Threshold is 850 tokens

    // Create a message that's right at the threshold
    let messages = vec![
        Message::user("a".repeat(3200)), // ~850 tokens with factor 4
    ];

    // Should trigger compression at or above threshold
    let estimate = compressor.estimate_tokens(&messages);
    let should = compressor.should_compress(&messages);
    if estimate > 850 {
        assert!(should);
    }
}

#[test]
fn test_hard_compress_only_assistants() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::assistant("response1"),
        Message::assistant("response2"),
        Message::assistant("response3"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Should end with user (continuation prompt)
    let last = compressed.last().unwrap();
    assert_eq!(last.role, "user");
}

#[test]
fn test_hard_compress_alternating() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::assistant("a2"),
        Message::user("u3"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // Should maintain proper structure
    assert!(!compressed.is_empty());
    assert_eq!(compressed[0].role, "system");
}

#[test]
fn test_hard_compress_two_messages() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![Message::system("system"), Message::user("question")];

    let compressed = compressor.hard_compress(&messages);

    // With only 2 messages, should keep both plus possible additions
    assert!(compressed.len() >= 2);
}

#[test]
fn test_hard_compress_user_only() {
    let compressor = ContextCompressor::new(100000);
    let messages = vec![
        Message::system("system"),
        Message::user("q1"),
        Message::user("q2"),
        Message::user("q3"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // All users should be preserved or compressed appropriately
    assert!(!compressed.is_empty());
}

#[test]
fn test_hard_compress_long_conversation() {
    let compressor = ContextCompressor::new(100000);
    let mut messages = vec![Message::system("system")];

    // Create a long conversation
    for i in 0..20 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }

    let compressed = compressor.hard_compress(&messages);

    // Should compress significantly
    assert!(compressed.len() < messages.len());
    // Should keep system
    assert_eq!(compressed[0].role, "system");
    // Should end with user
    assert_eq!(compressed.last().unwrap().role, "user");
}

#[test]
fn safe_tail_start_skips_leading_tool_results() {
    // [system, user, assistant, tool, tool, user, assistant] — 7 messages.
    let messages = vec![
        Message::system("system"),
        Message::user("question"),
        Message::assistant("let me call a tool"),
        Message::tool("tool result 1", "call_1"),
        Message::tool("tool result 2", "call_2"),
        Message::user("follow-up"),
        Message::assistant("done"),
    ];

    // desired = 3 lands on a tool message at index 3; should skip both
    // tool messages (indices 3 and 4) and land on the user at index 5.
    assert_eq!(safe_tail_start(&messages, 3), 5);

    // desired = 1 lands on a user message — no skip needed.
    assert_eq!(safe_tail_start(&messages, 1), 1);
}

#[test]
fn hard_compress_does_not_start_tail_on_tool_result() {
    // [system, user, assistant(tool_calls), tool, assistant, user]
    // len = 6, len-3 = 3 → index 3 is the orphan tool result whose
    // matching assistant (index 2) would be compacted away.
    let messages = vec![
        Message::system("system"),
        Message::user("please run a tool"),
        Message::assistant("calling tool now"),
        Message::tool("tool result", "call_1"),
        Message::assistant("the result was good"),
        Message::user("thanks"),
    ];

    let compressed = ContextCompressor::new(100000).hard_compress(&messages);

    // Find the "[Earlier context was compressed" note and verify the very
    // next message is not an orphan tool result.
    let note_idx = compressed
        .iter()
        .position(|m| m.content.text().contains("[Earlier context was compressed"));
    assert!(
        note_idx.is_some(),
        "compression note not found in: {:?}",
        compressed
            .iter()
            .map(|m| m.role.clone())
            .collect::<Vec<_>>()
    );
    let note_idx = note_idx.unwrap();

    // There must be at least one message after the note (the tail), and
    // the first one must NOT be a bare tool result.
    assert!(
        note_idx + 1 < compressed.len(),
        "no messages after compression note"
    );
    assert_ne!(
        compressed[note_idx + 1].role,
        "tool",
        "hard_compress kept an orphan tool result as the first tail message: {:?}",
        compressed
            .iter()
            .map(|m| m.role.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_estimate_tokens_large_message() {
    let compressor = ContextCompressor::new(100000);
    let large_content = "a".repeat(10000);
    let small_content = "a".repeat(100);
    let messages = vec![Message::user(large_content)];
    let small_messages = vec![Message::user(small_content)];

    let estimate = compressor.estimate_tokens(&messages);
    let small_estimate = compressor.estimate_tokens(&small_messages);
    assert!(estimate > small_estimate);
    assert!(estimate > 0);
}

#[test]
fn test_estimate_tokens_unicode() {
    let compressor = ContextCompressor::new(100000);
    // Unicode characters should be counted properly
    let messages = vec![Message::user("日本語テスト 🦀 Rust")];

    let estimate = compressor.estimate_tokens(&messages);
    // Should not crash and give reasonable estimate
    assert!(estimate > 0);
}

#[test]
fn test_hard_compress_preserves_task_objective() {
    let compressor = ContextCompressor::new(100000);
    // 10 messages: system + first user task + alternating so the task
    // falls well outside the last-3 tail window.
    let messages = vec![
        Message::system("sys"),
        Message::user("THE ORIGINAL TASK: fix the bug"),
        Message::assistant("Let me start by reading the file."),
        Message::user("Here is the file."),
        Message::assistant("I see the issue."),
        Message::user("Can you fix it?"),
        Message::assistant("Working on it now."),
        Message::user("Is it done?"),
        Message::assistant("Almost there."),
        Message::user("Please finish up."),
    ];

    let compressed = compressor.hard_compress(&messages);

    // The original task objective must survive the compaction.
    assert!(
        compressed
            .iter()
            .any(|m| m.content.text().contains("THE ORIGINAL TASK")),
        "hard_compress dropped the original task objective: {:?}",
        compressed
            .iter()
            .map(|m| m.content.text().to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_hard_compress_drops_dangling_tool_calls() {
    let compressor = ContextCompressor::new(100000);
    // The kept 3-message tail closes on an assistant that declares a tool
    // call whose result never arrived (interrupted final turn). The hard
    // fallback must drop it — a dangling tool_call 400s every provider.
    let messages = vec![
        Message::system("sys"),
        Message::user("THE ORIGINAL TASK: fix the bug"),
        Message::user("u1"),
        Message::assistant("a1"),
        Message::user("u2"),
        Message::user("u3"),
        tool_call_message("call_Z", "file_write"),
    ];

    let compressed = compressor.hard_compress(&messages);

    // (1) Task objective survives the emergency compaction.
    assert!(
        compressed
            .iter()
            .any(|m| m.content.text().contains("THE ORIGINAL TASK")),
        "hard_compress dropped the original task objective: {:?}",
        compressed
            .iter()
            .map(|m| m.content.text().to_string())
            .collect::<Vec<_>>()
    );
    // (2) No orphaned tool-call messages anywhere in the result.
    assert_valid_tool_pairing(&compressed);
    assert!(
        compressed
            .iter()
            .all(|m| m.tool_calls.as_ref().is_none_or(|calls| calls.is_empty())),
        "dangling assistant tool_call (call_Z) must be dropped: {:?}",
        compressed
            .iter()
            .map(|m| m.role.clone())
            .collect::<Vec<_>>()
    );
    // The list must still end with a user prompt for the next assistant turn.
    assert_eq!(
        compressed.last().map(|m| m.role.as_str()),
        Some("user"),
        "hard_compress output must end with a user message"
    );
}

#[tokio::test]
async fn test_compress_summarize_preserves_task_anchor_and_pairing() {
    // The autonomous-execution summarize path (ContextCompressor::compress):
    // must keep the original task pinned AND drop both the orphaned tool
    // result opening the recent window and the dangling trailing tool_call.
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("Summary of earlier work")
        .build()
        .await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let client = ApiClient::new(&config).unwrap();
    let compressor = ContextCompressor::new(1_000_000);

    let mut messages = vec![
        Message::system("sys"),
        Message::user(format!(
            "CONTEXT COMPRESSOR TASK SENTINEL: audit the codebase {}",
            "x".repeat(300)
        )),
        tool_call_message("call_A", "file_read"),
        Message::tool(format!("result A {}", "y".repeat(500)), "call_A"),
        tool_call_message("call_B", "file_write"),
        Message::tool("orphaned result B", "call_B"),
        Message::user("recent u0"),
        Message::assistant("recent a0"),
        Message::user("recent u1"),
        Message::assistant("recent a1"),
        tool_call_message("call_Z", "file_write"),
    ];

    let (compressed, _usage) = compressor.compress(&client, &messages).await.unwrap();
    messages = compressed;

    let joined = messages
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");

    // (1) Original task anchor survives the summarize path.
    assert!(
        joined.contains("CONTEXT COMPRESSOR TASK SENTINEL"),
        "original task text must survive summarize compression"
    );
    // (2) No orphaned tool messages, no dangling tool_calls.
    assert_valid_tool_pairing(&messages);
    assert!(
        messages.iter().all(|m| {
            m.tool_calls
                .as_ref()
                .is_none_or(|calls| calls.iter().all(|c| c.id != "call_Z"))
                && !m.content.text().contains("orphaned result B")
        }),
        "dangling call_Z and orphaned result B must both be gone: {joined}"
    );
    server.stop().await;
}

/// Assert a message list alternates user/assistant roles strictly: no two
/// consecutive messages may share the user role (providers enforce this
/// with a 400), while tool-result-as-user messages still count as their own
/// user-role turn.
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

/// Review finding #3 (summarize path): the boundary markers
/// ([ORIGINAL TASK] / [CONTEXT SUMMARY] / [RECENT CONTEXT] / continue) were
/// up to four consecutive user-role messages. They must now coalesce into
/// one real user turn so strict role-alternation providers accept the
/// rebuilt history.
#[tokio::test]
async fn test_compress_boundary_alternates_strict_roles() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("Summary of earlier work")
        .build()
        .await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let client = ApiClient::new(&config).unwrap();
    let compressor = ContextCompressor::new(1_000_000);

    let mut messages = vec![
        Message::system("sys"),
        Message::user(format!(
            "ALTERNATION TASK SENTINEL: audit the codebase {}",
            "x".repeat(300)
        )),
    ];
    for i in 0..10 {
        messages.push(Message::user(format!("filler user {i}")));
        messages.push(Message::assistant(format!("filler assistant {i}")));
    }
    // Recent window (last 6): [.., assistant, plain user turn].
    messages.push(Message::assistant("recent a0"));
    messages.push(Message::user("recent u0"));

    let (compressed, _usage) = compressor.compress(&client, &messages).await.unwrap();
    let joined = compressed
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");

    assert_strict_role_alternation(&compressed);
    assert!(
        joined.contains("ALTERNATION TASK SENTINEL"),
        "original task must survive: {joined}"
    );
    assert!(
        joined.contains("[RECENT CONTEXT]") && joined.contains("Based on the above summary"),
        "boundary markers must remain present (coalesced, not dropped): {joined}"
    );
    server.stop().await;
}

/// Review finding #3 (summarize path, XML tool-calling mode): a tool-result
/// user message must never be merged into the boundary markers — the
/// `<tool_result>` envelope keeps its own role=user message — and the recent
/// window may not OPEN on one (its assistant tool_use partner sat outside
/// the window). `safe_tail_start` skips it into the summarized region, so
/// the boundary always alternates.
#[tokio::test]
async fn test_compress_boundary_xml_tool_result_never_merged() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("Summary of earlier work")
        .build()
        .await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let client = ApiClient::new(&config).unwrap();
    let compressor = ContextCompressor::new(1_000_000);

    let mut messages = vec![
        Message::system("sys"),
        Message::user(format!(
            "XML TOOL TASK SENTINEL: audit the codebase {}",
            "x".repeat(300)
        )),
    ];
    for i in 0..8 {
        messages.push(Message::user(format!("filler user {i}")));
        messages.push(Message::assistant(format!("filler assistant {i}")));
    }
    // The final six messages: the window's opening message is a tool-result
    // user message whose assistant tool_use partner lies OUTSIDE the window.
    // Payload fragments are inert markers, not hostile text.
    let lt = "<";
    let gt = ">";
    let envelope_open = format!("{lt}tool_result{gt}");
    let envelope_close = format!("{lt}/tool_result{gt}");
    messages.push(Message::user(format!(
        "{envelope_open}alpha{envelope_close}"
    )));
    messages.push(Message::assistant("recent a0"));
    messages.push(Message::user(format!(
        "{envelope_open}beta{envelope_close}"
    )));
    messages.push(Message::assistant("recent a1"));
    messages.push(Message::user("final user 1"));
    messages.push(Message::user("final user 2"));

    let (compressed, _usage) = compressor.compress(&client, &messages).await.unwrap();
    let joined = compressed
        .iter()
        .map(|m| m.content.text_all())
        .collect::<Vec<_>>()
        .join("\n");

    assert_strict_role_alternation(&compressed);
    // The window-opening orphan tool-result is summarized away (its partner
    // was compacted out), while the tool-result INSIDE the window survives
    // as its own unmerged user-role message.
    assert!(
        !joined.contains("alpha"),
        "window-opening orphan tool-result must be skipped into the summary region: {joined}"
    );
    let beta = compressed
        .iter()
        .find(|m| m.content.text_all().contains("beta"))
        .unwrap_or_else(|| panic!("tool-result content must survive: {joined}"));
    assert_eq!(beta.role, "user");
    assert!(
        beta.content.text().starts_with(&envelope_open),
        "tool-result must keep its own envelope message, unmerged: {joined}"
    );
    // The two plain final turns coalesce into one (they were consecutive
    // user messages in the fixture).
    assert!(
        joined.contains("final user 1\n\nfinal user 2"),
        "adjacent plain user turns must coalesce: {joined}"
    );
    server.stop().await;
}

/// The coalescer itself must never touch XML tool-result user messages or
/// image-bearing user messages, only plain text user turns.
#[test]
fn test_coalesce_adjacent_user_turns_preserves_tool_results() {
    let lt = "<";
    let gt = ">";
    let envelope_open = format!("{lt}tool_result{gt}");
    let envelope_close = format!("{lt}/tool_result{gt}");
    let messages = vec![
        Message::user("plain a"),
        Message::user("plain b"),
        Message::user(format!("{envelope_open}result{envelope_close}")),
        Message::assistant("assistant reply"),
        Message::user("plain c"),
        Message::user("plain d"),
    ];

    let coalesced = crate::agent::Agent::coalesce_adjacent_user_turns(messages);

    assert_eq!(coalesced.len(), 4);
    assert_eq!(coalesced[0].role, "user");
    assert_eq!(coalesced[0].content.text(), "plain a\n\nplain b");
    assert_eq!(
        coalesced[1].content.text(),
        format!("{envelope_open}result{envelope_close}")
    );
    assert_eq!(coalesced[2].role, "assistant");
    assert_eq!(coalesced[3].content.text(), "plain c\n\nplain d");
}

/// `safe_tail_start` must skip both native (`role = "tool"`) orphan results
/// AND XML tool-result user messages when picking the recent-window opening.
#[test]
fn test_safe_tail_start_skips_xml_tool_result_user_messages() {
    let lt = "<";
    let gt = ">";
    let envelope_open = format!("{lt}tool_result{gt}");
    let envelope_close = format!("{lt}/tool_result{gt}");
    let messages = vec![
        Message::user("plain u"),
        Message::tool("native result", "call_1"),
        Message::user(format!("{envelope_open}xml result{envelope_close}")),
        Message::assistant("recent a"),
        Message::user("recent u"),
    ];

    assert_eq!(super::safe_tail_start(&messages, 1), 3);
}

// ---------------------------------------------------------------------------
// Work ledger
// ---------------------------------------------------------------------------

fn call(id: &str, name: &str, args: serde_json::Value) -> Message {
    let mut message = Message::assistant("");
    message.tool_calls = Some(vec![crate::api::types::ToolCall {
        id: id.to_string(),
        call_type: "function".to_string(),
        function: crate::api::types::ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }]);
    message
}

fn read_result(id: &str, content: &str) -> Message {
    Message::tool(
        serde_json::json!({
            "content": content,
            "total_lines": content.lines().count(),
            "truncated": false,
        })
        .to_string(),
        id,
    )
}

/// A realistic slice of a long run: reads, a finding note, a search, a
/// failed read, and an edit — the progress the c24 run kept losing.
fn progress_history(task: &str) -> Vec<Message> {
    vec![
        Message::system("You are selfware."),
        Message::user(task),
        call(
            "c1",
            "file_read",
            serde_json::json!({"path": "src/parser.rs"}),
        ),
        read_result("c1", "fn parse() {}\nfn lex() {}\nstruct Token;"),
        Message::assistant(
            "The parser.rs module has parse() and lex(); the bug is in lex() skipping whitespace.",
        ),
        call(
            "c2",
            "file_read",
            serde_json::json!({"path": "./tests/parser_test.rs", "line_range": [1, 40]}),
        ),
        read_result("c2", "#[test] fn t() {}"),
        call(
            "c3",
            "grep_search",
            serde_json::json!({"pattern": "fn lex", "path": "src"}),
        ),
        Message::tool(
            serde_json::json!({"matches": [], "count": 2, "total_matches": 2}).to_string(),
            "c3",
        ),
        call(
            "c4",
            "file_read",
            serde_json::json!({"path": "src/missing.rs"}),
        ),
        Message::tool(
            serde_json::json!({"error": "No such file"}).to_string(),
            "c4",
        ),
        call(
            "c5",
            "file_edit",
            serde_json::json!({"path": "src/parser.rs", "old_str": "a", "new_str": "b"}),
        ),
        Message::tool(serde_json::json!({"success": true}).to_string(), "c5"),
        Message::user("continue"),
    ]
}

#[test]
fn work_ledger_survives_repeated_trims_and_lists_read_files() {
    let task = "Fix the lexer whitespace bug in src/parser.rs";
    let compressor = ContextCompressor::new(24_000);
    let mut messages = progress_history(task);

    // Simulate the c24 shape: emergency compaction every step (3-message
    // tail) plus hard trims — the reads themselves are long gone afterwards.
    for _ in 0..5 {
        compressor.begin_ledger_turn(Some(task));
        messages = compressor.hard_compress_with_task(&messages, Some(task));
        crate::agent::Agent::trim_messages(&mut messages, 60, None);
    }
    assert!(
        !messages
            .iter()
            .any(|m| m.content.text().contains("fn parse() {}")),
        "precondition: the read results were trimmed away"
    );

    let ledger = compressor.work_ledger();
    let paths: Vec<&str> = ledger.files().iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["src/parser.rs", "tests/parser_test.rs"]);
    assert!(
        !paths.contains(&"src/missing.rs"),
        "a FAILED read must never be listed"
    );

    let rendered = compressor
        .render_work_ledger(work_ledger_token_cap(24_000))
        .expect("ledger renders");
    assert!(rendered.contains(WORK_LEDGER_HEADER));
    assert!(rendered.contains(
        "Do not re-read a file listed here unless you need a specific line range you have not seen"
    ));
    assert!(rendered.contains("src/parser.rs — whole file (3 lines)"));
    assert!(rendered.contains("tests/parser_test.rs — lines 1-40"));
    assert!(rendered.contains("your note: The parser.rs module has parse() and lex()"));
    assert!(rendered.contains("grep \"fn lex\" in src → 2 matches"));
    assert!(rendered.contains("src/parser.rs — file_edit x1"));
    assert!(
        rendered.contains("you modified it at turn 1"),
        "an edit after the read must be flagged so the model re-reads the new content"
    );
}

#[test]
fn work_ledger_observe_is_idempotent() {
    let task = "task";
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some(task));
    let history = progress_history(task);
    ledger.observe(&history, None);
    ledger.observe(&history, None);
    ledger.observe(&history[..6], None);
    let parser = &ledger.files()[0];
    assert_eq!(
        parser.reads, 1,
        "re-observing the same history adds nothing"
    );
    assert_eq!(ledger.writes()[0].count, 1);
}

#[test]
fn work_ledger_reread_of_unchanged_file_with_new_range_is_recorded() {
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task"));
    let body = "line\n".repeat(50);
    let mut history = vec![
        Message::system("sys"),
        Message::user("task"),
        call(
            "r1",
            "file_read",
            serde_json::json!({"path": "src/big.rs", "line_range": [1, 50]}),
        ),
        read_result("r1", &body),
    ];
    ledger.observe(&history, None);
    let first_hash = ledger.files()[0].content_hash.clone();

    // Same file, a range not seen yet: must be recorded (ranges merged), not
    // treated as a duplicate.
    ledger.begin_turn(Some("task"));
    history.push(call(
        "r2",
        "file_read",
        serde_json::json!({"path": "src/big.rs", "line_range": [100, 150]}),
    ));
    history.push(read_result("r2", &body));
    ledger.observe(&history, None);

    let entry = &ledger.files()[0];
    assert_eq!(entry.reads, 2);
    assert_eq!(entry.ranges, vec![(1, 50), (100, 150)]);
    assert_eq!(entry.content_hash, first_hash, "same content, same hash");
    assert_eq!(entry.last_read_turn, 2);
    let rendered = ledger.render(1_500).unwrap();
    assert!(rendered.contains("lines 1-50, 100-150"));
    assert!(rendered.contains("Reading a new range is fine."));
}

#[test]
fn work_ledger_render_stays_bounded_and_drops_oldest_first() {
    use crate::token_count::estimate_content_tokens;
    let mut ledger = WorkLedger::new();
    let mut history = vec![Message::system("sys"), Message::user("task")];
    for i in 0..200 {
        ledger.begin_turn(Some("task"));
        let id = format!("f{i}");
        history.push(call(
            &id,
            "file_read",
            serde_json::json!({"path": format!("src/module_{i:03}/file_{i:03}.rs")}),
        ));
        history.push(read_result(&id, &format!("contents of file {i}")));
        history.push(Message::assistant(format!(
            "file_{i:03}.rs defines the handler for route {i} and validates its input."
        )));
        ledger.observe(&history, None);
    }
    // Memory bound.
    assert!(ledger.files().len() <= 128);

    for cap in [150usize, 600, 1_500, 2_000] {
        let rendered = ledger.render(cap).expect("renders at every cap");
        let measured = estimate_content_tokens(&rendered);
        assert!(
            measured <= cap,
            "rendered ledger is {measured} tokens, over the {cap}-token cap"
        );
    }
    let rendered = ledger.render(1_500).unwrap();
    let shown: Vec<usize> = (0..200)
        .filter(|i| rendered.contains(&format!("src/module_{i:03}/file_{i:03}.rs —")))
        .collect();
    assert!(!shown.is_empty() && shown.len() < 128, "shown: {shown:?}");
    // Exactly the newest entries survive: a contiguous run ending at 199.
    let expected: Vec<usize> = (200 - shown.len()..200).collect();
    assert_eq!(shown, expected, "oldest entries are dropped first");
    assert!(rendered.contains("older ledger entries omitted"));
}

#[test]
fn work_ledger_takes_per_file_findings_from_a_summary_but_never_adds_files() {
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task"));
    ledger.observe(
        &[
            Message::system("sys"),
            Message::user("task"),
            call("a", "file_read", serde_json::json!({"path": "src/a.rs"})),
            read_result("a", "fn a() {}"),
        ],
        None,
    );
    ledger.begin_turn(Some("task"));
    ledger.observe(
        &[Message::user(
            "[CONTEXT SUMMARY - 9 earlier messages compressed]:\nWork so far.\n\nFILES READ:\n\
             - `src/a.rs`: a() is the retry entry point; backoff constant is 3.\n\
             - src/invented.rs: the summarizer made this one up",
        )],
        None,
    );
    let files = ledger.files();
    assert_eq!(files.len(), 1, "a summary line can never add a read");
    let (source, _, note) = files[0].note.clone().expect("summary finding attached");
    assert_eq!(source, LedgerNoteSource::Summary);
    assert!(note.starts_with("a() is the retry entry point"));
    assert!(ledger
        .render(1_500)
        .unwrap()
        .contains("summary: a() is the retry"));
}

#[test]
fn work_ledger_reads_xml_mode_results_and_skips_xml_errors() {
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task"));
    let lt = "<";
    let open = format!("{lt}tool_result>");
    let close = format!("{lt}/tool_result>");
    let payload = serde_json::json!({"content": "x < y && z", "total_lines": 1}).to_string();
    let escaped = payload
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let calls = format!(
        "{lt}tool>\n{lt}name>file_read{lt}/name>\n{lt}arguments>{{\"path\": \"src/x.rs\"}}{lt}/arguments>\n{lt}/tool>\n\
         {lt}tool>\n{lt}name>file_read{lt}/name>\n{lt}arguments>{{\"path\": \"src/gone.rs\"}}{lt}/arguments>\n{lt}/tool>"
    );
    let history = vec![
        Message::system("sys"),
        Message::user("task"),
        Message::assistant(calls),
        Message::user(format!("{open}{escaped}{close}")),
        Message::user(format!("{open}{lt}error>not found{lt}/error>{close}")),
    ];
    ledger.observe(&history, None);
    let files = ledger.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "src/x.rs");
    assert_eq!(files[0].total_lines, Some(1));
    assert!(!files[0].partial, "the unescaped payload parsed in full");
}

#[test]
fn work_ledger_resets_when_the_task_changes() {
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task one"));
    ledger.observe(&progress_history("task one"), None);
    assert!(!ledger.is_empty());
    ledger.begin_turn(Some("task two"));
    assert!(ledger.is_empty());
    assert_eq!(ledger.turn(), 1);
}

#[test]
fn summarizer_input_names_the_native_tool_calls() {
    let m = call("c1", "file_read", serde_json::json!({"path": "src/a.rs"}));
    let suffix = summarizer_tool_call_suffix(&m);
    assert!(suffix.contains("[called file_read {\"path\":\"src/a.rs\"}]"));
    assert!(summarizer_tool_call_suffix(&Message::assistant("text")).is_empty());
    assert!(PER_FILE_FINDINGS_INSTRUCTION.contains("- <path>: <1-2 sentence key finding"));
}
