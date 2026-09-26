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
        crate::agent::Agent::trim_messages(
            &mut messages,
            60,
            None,
            &crate::agent::context::PathKeys::default(),
        );
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
    // The header no longer forbids re-reading (val082: the reads' CONTENT
    // was gone while the ledger said "do not re-read"); it says findings are
    // not contents and exact lines must be in context before citing.
    assert!(rendered.contains("they are not the file contents"));
    assert!(rendered.contains("re-read just the range you need if they are not"));
    assert!(!rendered.contains("Do not re-read a file listed here"));
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
    assert!(rendered.contains("re-read just the range you need"));
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

// --- One canonical path per file (external review 2026-09-25): reading
// `example.rs` and then writing `sub/../example.rs` made two ledger
// identities, and the stale whole-file coverage and summary finding
// survived the edit with no modification warning. ---

#[test]
#[cfg(unix)]
fn canonical_workspace_path_resolves_aliases_lexically() {
    let root = std::path::Path::new("/work");
    let c = |p: &str| crate::agent::context::canonical_workspace_path(p, Some(root));
    assert_eq!(c("example.rs"), "example.rs");
    assert_eq!(c("./example.rs"), "example.rs");
    assert_eq!(c("sub/../example.rs"), "example.rs");
    assert_eq!(c("/work/example.rs"), "example.rs");
    assert_eq!(c("/work/sub/../example.rs"), "example.rs");
    assert_eq!(c("./a/../b.rs"), "b.rs");
    assert_eq!(c("src/./agent/../lib.rs"), "src/lib.rs");
    assert_eq!(c("."), ".");
    assert_eq!(c("/work"), ".");
    // Outside the workspace stays absolute (and is never confused with a
    // workspace file of the same name).
    assert_eq!(c("../other/x.rs"), "/other/x.rs");
    assert_eq!(c("/etc/passwd"), "/etc/passwd");
    // Without a root: lexical only.
    let n = |p: &str| crate::agent::context::canonical_workspace_path(p, None);
    assert_eq!(n("./a/../b.rs"), "b.rs");
    assert_eq!(n("../x.rs"), "../x.rs");
}

#[cfg(unix)]
fn alias_probe_ledger(write_path: &str, read_path: &str) -> WorkLedger {
    let root = std::path::Path::new("/work");
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("review"));
    ledger.observe(
        &[
            call("r1", "file_read", serde_json::json!({"path": read_path})),
            read_result("r1", "// old header\npub const ALLOW_ALL: bool = true;\n"),
        ],
        Some(root),
    );
    ledger.absorb_summary("- example.rs: old policy permits every operation");
    ledger.begin_turn(Some("review"));
    ledger.observe(
        &[
            call(
                "w1",
                "file_write",
                serde_json::json!({"path": write_path, "content": "// new header\n"}),
            ),
            Message::tool(serde_json::json!({"success": true}).to_string(), "w1"),
        ],
        Some(root),
    );
    ledger.begin_turn(Some("review"));
    ledger.observe(
        &[
            call(
                "r2",
                "file_read",
                serde_json::json!({"path": read_path, "line_range": [1, 1]}),
            ),
            Message::tool(
                serde_json::json!({"content": "1\t// new header", "total_lines": 2}).to_string(),
                "r2",
            ),
        ],
        Some(root),
    );
    ledger
}

#[test]
#[cfg(unix)]
fn an_edit_through_a_path_alias_invalidates_the_earlier_read() {
    for (write_path, read_path) in [
        ("example.rs", "example.rs"),
        ("sub/../example.rs", "example.rs"),
        ("/work/example.rs", "example.rs"),
        ("example.rs", "/work/example.rs"),
        ("./a/../example.rs", "./example.rs"),
    ] {
        let ledger = alias_probe_ledger(write_path, read_path);
        assert_eq!(
            ledger.files().len(),
            1,
            "{write_path} / {read_path}: one identity"
        );
        let rendered = ledger.render(2_000).unwrap();
        assert!(
            rendered.contains("- example.rs — lines 1-1 of 2;"),
            "{write_path} / {read_path}: old whole-file coverage invalidated:\n{rendered}"
        );
        assert!(
            rendered.contains("you modified it at turn 2; only lines 1-1 re-read since"),
            "{write_path} / {read_path}: modification warning kept:\n{rendered}"
        );
        assert!(
            rendered.contains("- example.rs — file_write x1"),
            "{write_path} / {read_path}: the deliverable is the same file:\n{rendered}"
        );
    }
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

// ---------------------------------------------------------------------------
// Work ledger: edits invalidate coverage and findings of the older version
// (external review 2026-09-25, P2: a partial reread after an edit advertised
// the whole pre-edit file as read, kept its old finding and dropped the
// modification warning).
// ---------------------------------------------------------------------------

fn tool_pair(
    id: &str,
    name: &str,
    args: serde_json::Value,
    result: serde_json::Value,
) -> [Message; 2] {
    [call(id, name, args), Message::tool(result.to_string(), id)]
}

/// Whole read -> summary finding -> file_write. Returns the history so far.
fn read_summarize_write(ledger: &mut WorkLedger) -> Vec<Message> {
    let mut history = vec![Message::system("sys"), Message::user("review")];
    ledger.begin_turn(Some("review"));
    history.extend(tool_pair(
        "r1",
        "file_read",
        serde_json::json!({"path": "example.rs"}),
        serde_json::json!({"content": "old header\nold policy\n", "total_lines": 2, "truncated": false}),
    ));
    ledger.observe(&history, None);
    ledger.absorb_summary("- example.rs: old policy permits every operation");
    ledger.begin_turn(Some("review"));
    history.extend(tool_pair(
        "w1",
        "file_write",
        serde_json::json!({"path": "example.rs", "content": "new header\nnew policy\n"}),
        serde_json::json!({"success": true}),
    ));
    ledger.observe(&history, None);
    history
}

fn partial_reread_line_1(history: &mut Vec<Message>) {
    history.extend(tool_pair(
        "r2",
        "file_read",
        serde_json::json!({"path": "example.rs", "line_range": [1, 1]}),
        serde_json::json!({"content": "new header\n", "lines_returned": 1, "total_lines": null,
                           "has_more": true, "truncated": true}),
    ));
}

fn assert_partial_post_edit_view(rendered: &str) {
    assert!(
        !rendered.contains("example.rs — whole file"),
        "a partial reread of the new version must not claim the whole file: {rendered}"
    );
    assert!(
        rendered.contains("example.rs — lines 1-1"),
        "coverage is only the reread range: {rendered}"
    );
    assert!(
        !rendered.contains("summary: old policy permits every operation"),
        "the pre-edit finding must not be presented as current: {rendered}"
    );
    assert!(
        rendered.contains("you modified it at turn 2")
            && rendered.contains("only lines 1-1 re-read since"),
        "the modification warning must survive a partial reread: {rendered}"
    );
}

#[test]
fn work_ledger_partial_reread_after_edit_starts_fresh_coverage() {
    let mut ledger = WorkLedger::new();
    let mut history = read_summarize_write(&mut ledger);
    let before = ledger.render(2_000).unwrap();
    assert!(before.contains("you modified it at turn 2"), "{before}");
    assert!(
        before.contains("pre-edit version"),
        "before any reread, the old coverage is labelled as the pre-edit version: {before}"
    );

    ledger.begin_turn(Some("review"));
    partial_reread_line_1(&mut history);
    ledger.observe(&history, None);

    let entry = &ledger.files()[0];
    assert!(!entry.whole_file);
    assert_eq!(entry.ranges, vec![(1, 1)]);
    assert!(
        entry.note.is_none(),
        "old finding dropped: {:?}",
        entry.note
    );
    assert_eq!(entry.modified_turn, Some(2));
    assert!(entry.reread_since_modified);
    assert_partial_post_edit_view(&ledger.render(2_000).unwrap());
}

#[test]
fn work_ledger_whole_reread_after_edit_clears_the_warning() {
    let mut ledger = WorkLedger::new();
    let mut history = read_summarize_write(&mut ledger);
    ledger.begin_turn(Some("review"));
    partial_reread_line_1(&mut history);
    ledger.observe(&history, None);
    ledger.begin_turn(Some("review"));
    history.extend(tool_pair(
        "r3",
        "file_read",
        serde_json::json!({"path": "example.rs"}),
        serde_json::json!({"content": "new header\nnew policy\n", "total_lines": 2, "truncated": false}),
    ));
    ledger.observe(&history, None);

    let entry = &ledger.files()[0];
    assert!(entry.whole_file);
    assert_eq!(entry.modified_turn, None);
    assert!(!entry.reread_since_modified);
    let rendered = ledger.render(2_000).unwrap();
    assert!(
        rendered.contains("example.rs — whole file (2 lines)"),
        "{rendered}"
    );
    // (The header's generic "or you modified it since" stays.)
    assert!(!rendered.contains("[you modified it"), "{rendered}");
    assert!(!rendered.contains("old policy permits"), "{rendered}");
}

#[test]
fn work_ledger_ranges_that_cover_the_new_version_clear_the_warning() {
    let mut ledger = WorkLedger::new();
    let mut history = read_summarize_write(&mut ledger);
    ledger.begin_turn(Some("review"));
    partial_reread_line_1(&mut history);
    history.extend(tool_pair(
        "r3",
        "file_read",
        serde_json::json!({"path": "example.rs", "line_range": [2, 2]}),
        serde_json::json!({"content": "new policy\n", "lines_returned": 1, "total_lines": 2,
                           "truncated": false}),
    ));
    ledger.observe(&history, None);
    let entry = &ledger.files()[0];
    assert!(
        entry.whole_file,
        "lines 1-2 of 2 cover the whole new version"
    );
    assert_eq!(entry.modified_turn, None);
}

#[test]
fn work_ledger_same_range_with_new_content_invalidates_old_coverage() {
    // A change the ledger did not see (shell edit, another process): the
    // same range now returns different content, so the older coverage and
    // findings describe another version.
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task"));
    let mut history = vec![Message::system("sys"), Message::user("task")];
    history.extend(tool_pair(
        "a",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [1, 10]}),
        serde_json::json!({"content": "v1 a\n", "total_lines": null, "has_more": true}),
    ));
    history.extend(tool_pair(
        "b",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [20, 30]}),
        serde_json::json!({"content": "v1 b\n", "total_lines": null, "has_more": true}),
    ));
    ledger.observe(&history, None);
    ledger.absorb_summary("- src/x.rs: version one has the retry loop");
    assert_eq!(ledger.files()[0].ranges, vec![(1, 10), (20, 30)]);

    ledger.begin_turn(Some("task"));
    history.extend(tool_pair(
        "c",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [1, 10]}),
        serde_json::json!({"content": "v2 a\n", "total_lines": null, "has_more": true}),
    ));
    ledger.observe(&history, None);
    let entry = &ledger.files()[0];
    assert_eq!(
        entry.ranges,
        vec![(1, 10)],
        "only the range seen in the new version"
    );
    assert!(entry.note.is_none());
}

#[test]
fn work_ledger_hashes_raw_text_not_line_number_prefixes() {
    // The same range read numbered (the default) and then raw
    // (line_numbers: false) is the same version of the file: the prefixes
    // are metadata and must not look like a content change.
    let mut ledger = WorkLedger::new();
    ledger.begin_turn(Some("task"));
    let mut history = vec![Message::system("sys"), Message::user("task")];
    history.extend(tool_pair(
        "a",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [10, 11]}),
        serde_json::json!({"content": "    10\tfn a() {}\n    11\tfn b() {}", "line_numbers": true,
                           "total_lines": null, "has_more": true}),
    ));
    history.extend(tool_pair(
        "b",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [30, 31]}),
        serde_json::json!({"content": "    30\tfn c() {}\n    31\tfn d() {}", "line_numbers": true,
                           "total_lines": null, "has_more": true}),
    ));
    ledger.observe(&history, None);
    let numbered_hash = ledger.files()[0].content_hash.clone();

    ledger.begin_turn(Some("task"));
    history.extend(tool_pair(
        "c",
        "file_read",
        serde_json::json!({"path": "src/x.rs", "line_range": [10, 11], "line_numbers": false}),
        serde_json::json!({"content": "fn a() {}\nfn b() {}", "line_numbers": false,
                           "total_lines": null, "has_more": true}),
    ));
    ledger.observe(&history, None);
    let entry = &ledger.files()[0];
    assert_eq!(
        entry.ranges,
        vec![(10, 11), (30, 31)],
        "a raw re-read of a numbered range is not a new version"
    );
    // The raw read hashes the same text as the numbered read of that range.
    let mut numbered_only = WorkLedger::new();
    numbered_only.begin_turn(Some("task"));
    numbered_only.observe(&history[..4], None);
    assert_eq!(
        numbered_only.files()[0].content_hash,
        entry.content_hash,
        "numbered and raw reads of the same text hash equal"
    );
    assert_ne!(numbered_hash, entry.content_hash, "different ranges differ");
}

#[test]
fn work_ledger_multi_edit_and_patch_apply_mark_the_file_modified() {
    for (name, args) in [
        (
            "file_multi_edit",
            serde_json::json!({"edits": [{"path": "src/a.rs", "old_str": "a", "new_str": "b"}]}),
        ),
        (
            "patch_apply",
            serde_json::json!({"diff": "--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\n"}),
        ),
        (
            "file_fim_edit",
            serde_json::json!({"path": "src/a.rs", "prefix": "a", "suffix": "b"}),
        ),
    ] {
        let mut ledger = WorkLedger::new();
        ledger.begin_turn(Some("task"));
        let mut history = vec![Message::system("sys"), Message::user("task")];
        history.extend(tool_pair(
            "r",
            "file_read",
            serde_json::json!({"path": "src/a.rs"}),
            serde_json::json!({"content": "a\n", "total_lines": 1}),
        ));
        history.extend(tool_pair(
            "w",
            name,
            args,
            serde_json::json!({"success": true}),
        ));
        ledger.observe(&history, None);
        assert_eq!(
            ledger.files()[0].modified_turn,
            Some(1),
            "{name} must mark the read file modified"
        );
        assert!(
            ledger.writes().iter().any(|w| w.path == "src/a.rs"),
            "{name} must list the deliverable"
        );
    }
}

#[tokio::test]
async fn work_ledger_edit_then_partial_reread_stays_honest_through_compaction() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response(
            "Reviewed example.rs.\n\nFILES READ:\n- example.rs: old policy permits every operation",
        )
        .build()
        .await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let client = ApiClient::new(&config).unwrap();
    let compressor = ContextCompressor::new(1_000_000);

    let mut scratch = WorkLedger::new();
    let mut history = read_summarize_write(&mut scratch);
    partial_reread_line_1(&mut history);
    for i in 0..6 {
        history.push(Message::assistant(format!("step {i}")));
        history.push(Message::user(format!("continue {i}")));
    }
    compressor.begin_ledger_turn(Some("review"));
    let (compressed, _usage) = compressor
        .compress_with_task(&client, &history, Some("review"))
        .await
        .unwrap();
    assert!(
        compressed
            .iter()
            .any(|m| m.content.text().contains("[CONTEXT SUMMARY")),
        "precondition: summary compaction happened"
    );
    let compressed = compressor.hard_compress_with_task(&compressed, Some("review"));
    compressor.observe_work(&compressed);

    let rendered = compressor.render_work_ledger(2_000).unwrap();
    assert!(!rendered.contains("example.rs — whole file"), "{rendered}");
    assert!(rendered.contains("example.rs — lines 1-1"), "{rendered}");
    assert!(
        rendered.contains("only lines 1-1 re-read since"),
        "{rendered}"
    );
    // The summarizer's finding arrived while the edit was not fully reread:
    // it is labelled as possibly describing the pre-edit version.
    assert!(
        rendered.contains("summary: (may predate your edit at turn"),
        "{rendered}"
    );
    server.stop().await;
}

#[test]
fn work_ledger_resume_round_trip_keeps_the_invalidation() {
    // The work ledger is not persisted: a resumed agent rebuilds it from the
    // checkpointed messages. The rebuilt ledger must show the same
    // post-edit view as the live one.
    let mut live = WorkLedger::new();
    let mut history = read_summarize_write(&mut live);
    live.begin_turn(Some("review"));
    partial_reread_line_1(&mut history);
    live.observe(&history, None);

    let mut checkpoint =
        crate::session::checkpoint::TaskCheckpoint::new("t1".to_string(), "review".to_string());
    checkpoint.messages = history.clone();
    let json = serde_json::to_string(&checkpoint).unwrap();
    let restored: crate::session::checkpoint::TaskCheckpoint = serde_json::from_str(&json).unwrap();

    let mut rebuilt = WorkLedger::new();
    rebuilt.begin_turn(Some("review"));
    rebuilt.observe(&restored.messages, None);
    let entry = &rebuilt.files()[0];
    assert!(!entry.whole_file);
    assert_eq!(entry.ranges, vec![(1, 1)]);
    assert!(entry.modified_turn.is_some());
    let rendered = rebuilt.render(2_000).unwrap();
    assert!(!rendered.contains("example.rs — whole file"), "{rendered}");
    assert!(
        rendered.contains("only lines 1-1 re-read since"),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------
// Automatic compressor summary goes through the bounded side-call path
// (external review 2026-09-25, P2: compression-wire-probe captured
// stream:false, max_tokens 24576, enable_thinking:true, reasoning_effort
// xhigh — the session extra_body merged into an ordinary chat request).
// ---------------------------------------------------------------------------

/// The review probe's session: xhigh thinking pinned in extra_body and a
/// 24k main-turn output budget.
fn xhigh_session_config(endpoint: &str) -> crate::config::Config {
    let mut config = crate::test_support::mock_agent_config(endpoint);
    config.max_tokens = 24_576;
    config.context_length = 163_840;
    config.extra_body = Some(
        serde_json::from_value(serde_json::json!({
            "chat_template_kwargs": {
                "enable_thinking": true,
                "reasoning_effort": "xhigh",
                "preserve_thinking": false
            }
        }))
        .unwrap(),
    );
    config
}

/// The probe's history: 18 messages, enough to summarize.
fn probe_history() -> Vec<Message> {
    let mut m = vec![
        Message::system("system"),
        Message::user("Review the repo without edits."),
    ];
    for i in 0..8 {
        m.push(Message::assistant(format!("Reviewed example{i}.rs")));
        m.push(Message::user(format!("Read result {i}")));
    }
    m
}

#[tokio::test]
async fn compressor_summary_is_a_bounded_streamed_side_call_on_the_wire() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("- example.rs: already reviewed the implementation.")
        .with_usage(11, 7, 18)
        .build()
        .await;
    let config = xhigh_session_config(&format!("{}/v1", server.url()));
    let client = ApiClient::new(&config).unwrap();
    let (compressed, usage) = ContextCompressor::new(32_000)
        .compress_with_task(
            &client,
            &probe_history(),
            Some("Review the repo without edits."),
        )
        .await
        .expect("summary compaction succeeds");
    assert!(compressed
        .iter()
        .any(|m| m.content.text().contains("[CONTEXT SUMMARY")));
    // Usage of the summarizer call is still carried out to the caller.
    assert_eq!(usage.prompt_tokens, 11, "{usage:?}");
    assert_eq!(usage.completion_tokens, 7, "{usage:?}");

    let bodies = server.captured_request_bodies().await;
    assert_eq!(bodies.len(), 1);
    let sent: serde_json::Value = serde_json::from_str(&bodies[0]).unwrap();
    assert_eq!(sent["stream"], true, "the summary must stream: {sent}");
    assert_eq!(
        sent["max_tokens"],
        crate::agent::compression::COMPACT_SUMMARY_MAX_TOKENS,
        "side-call output budget, not the session's 24576: {sent}"
    );
    assert_eq!(
        sent["chat_template_kwargs"]["reasoning_effort"], "low",
        "session xhigh effort must be lowered: {sent}"
    );
    assert!(sent.get("tools").is_none(), "{sent}");
    server.stop().await;
}

#[tokio::test]
async fn compressor_summary_slow_response_is_the_typed_side_call_timeout() {
    // Headers only after 10 s: the shape of an xhigh summary behind a
    // gateway. `agent.max_call_secs` tightens the side-call cap to 1 s.
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("late")
        .with_latency(10_000)
        .build()
        .await;
    let mut config = xhigh_session_config(&format!("{}/v1", server.url()));
    config.agent.max_call_secs = Some(1);
    let client = ApiClient::new(&config).unwrap();
    let started = std::time::Instant::now();
    let err = ContextCompressor::new(32_000)
        .compress_with_task(&client, &probe_history(), None)
        .await
        .expect_err("a summary over its cap must fail");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "cut at the cap, took {:?}",
        started.elapsed()
    );
    let typed = err
        .chain()
        .find_map(|c| c.downcast_ref::<crate::api::client::SideCallTimeout>())
        .unwrap_or_else(|| panic!("expected SideCallTimeout, got {err:?}"));
    assert_eq!(typed.purpose, CONTEXT_SUMMARY_PURPOSE);
    assert_eq!(typed.limit_secs, 1);
    server.stop().await;
}

#[tokio::test]
async fn compressor_summary_emits_side_call_waiting_heartbeats() {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("- example.rs: reviewed.")
        .with_latency(400)
        .build()
        .await;
    let config = xhigh_session_config(&format!("{}/v1", server.url()));
    let recorder = std::sync::Arc::new(crate::agent::progress::RecordingProgressEmitter::new());
    let mut client = ApiClient::new(&config).unwrap();
    client.with_progress_emitter(recorder.clone());
    client.side_call_wait_tick_override = Some(std::time::Duration::from_millis(50));
    ContextCompressor::new(32_000)
        .compress_with_task(&client, &probe_history(), None)
        .await
        .unwrap();
    let phases: Vec<String> = recorder
        .snapshot()
        .into_iter()
        .filter_map(|e| match e {
            crate::agent::progress::ProgressEvent::LlmWaiting {
                phase,
                tokens_source,
                ..
            } => {
                assert_eq!(tokens_source, "none", "nothing observable is claimed");
                Some(phase)
            }
            _ => None,
        })
        .collect();
    assert!(
        phases.len() >= 2,
        "a 400 ms side call at a 50 ms cadence must tick: {phases:?}"
    );
    assert!(
        phases.iter().all(|p| p == "side_call:context_summary"),
        "{phases:?}"
    );
    server.stop().await;
}

#[tokio::test]
async fn compressor_with_too_few_messages_makes_no_call() {
    // c24 (0.8.2 validation, 24k window): 10 of 12 "no reduction" hard
    // fallbacks came from this path — the history was already at most the
    // kept tail, so no summarizer call was made at all. The side-call
    // routing cannot change that; the caller now names this reason.
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("unused")
        .build()
        .await;
    let client = ApiClient::new(&xhigh_session_config(&format!("{}/v1", server.url()))).unwrap();
    let compressor = ContextCompressor::new(100);
    let history = vec![
        Message::system("x".repeat(4_000)),
        Message::user("task"),
        Message::assistant("a"),
        Message::user("b"),
    ];
    assert!(compressor.should_compress(&history));
    assert!(compressor.too_few_to_summarize(&history));
    let (out, usage) = compressor
        .compress_with_task(&client, &history, None)
        .await
        .unwrap();
    assert_eq!(out.len(), history.len());
    assert_eq!(usage.total_tokens, 0);
    assert!(server.captured_request_bodies().await.is_empty());
    server.stop().await;
}

// --- N5: a summary call only when it can bring the history under the
// threshold. val083 c24 (24k window: history budget 11,008, threshold
// 8,256, system prompt ~5.3k): 16 summary calls took 470 s, and 7 left the
// history above the threshold (e.g. ~10,984 -> ~10,824), so the next turn
// summarized again. ---

/// Varied text of about `chars` characters (repeated single letters
/// tokenize far below their length).
fn prose(chars: usize) -> String {
    let words = [
        "ledger",
        "compaction",
        "summary",
        "threshold",
        "stub",
        "range",
        "turn",
    ];
    let mut out = String::new();
    let mut i = 0usize;
    while out.len() < chars {
        out.push_str(words[i % words.len()]);
        out.push_str(&format!(" {i} "));
        i += 1;
    }
    out
}

/// The c24 request shape: system prompt, task, older small turns, and a
/// kept tail whose latest read alone is ~3.5k tokens.
fn c24_history(older_turns: usize, older_chars: usize) -> Vec<Message> {
    let mut history = vec![
        Message::system("You are selfware, a careful coding agent.\n".repeat(520)),
        Message::user("Document every pub fn in src/agent/context.rs."),
    ];
    for i in 0..older_turns {
        history.push(Message::assistant(format!(
            "step {i}: {}",
            prose(older_chars)
        )));
        history.push(Message::user(format!(
            "<tool_result>{}</tool_result>",
            prose(older_chars)
        )));
    }
    history.push(Message::assistant("reading context.rs 113-284"));
    history.push(Message::user(format!(
        "<tool_result>{}</tool_result>",
        "pub fn ledger_entry(x: usize) -> usize { x + 1 }\n".repeat(280)
    )));
    history.push(Message::assistant("reading context.rs 285-390"));
    history.push(Message::user(format!(
        "<tool_result>{}</tool_result>",
        "pub fn render(y: &str) -> String { y.to_string() }\n".repeat(60)
    )));
    history.push(Message::assistant("now editing"));
    history.push(Message::user("continue"));
    history
}

#[test]
fn summary_is_skipped_when_the_kept_tail_alone_is_over_the_threshold() {
    let compressor = ContextCompressor::new(11_008);
    let history = c24_history(3, 1_200);
    let split = compressor.summary_split(&history, None).expect("a split");
    assert!(
        split.kept_tokens + crate::agent::context::SUMMARY_TOKENS_ESTIMATE
            > compressor.compression_threshold(),
        "precondition: {split:?}"
    );
    let reason = compressor
        .summary_skip_reason(&history, None)
        .expect("no call can help");
    assert!(
        reason.contains("cannot bring the history under the threshold"),
        "{reason}"
    );
}

#[test]
fn summary_is_skipped_below_the_summarizable_floor() {
    let compressor = ContextCompressor::new(11_008);
    let mut history = c24_history(1, 200);
    // Shrink the tail so only the floor decides.
    history.truncate(history.len() - 6);
    for i in 0..3 {
        history.push(Message::assistant(format!("short {i}")));
        history.push(Message::user(format!("ok {i}")));
    }
    let split = compressor.summary_split(&history, None).expect("a split");
    assert!(
        split.summarizable_tokens < crate::agent::context::MIN_SUMMARIZABLE_TOKENS,
        "precondition: {split:?}"
    );
    let reason = compressor.summary_skip_reason(&history, None).unwrap();
    assert!(reason.contains("below the 1500-token floor"), "{reason}");
}

#[test]
fn a_summary_that_can_fit_is_allowed_and_a_rejected_one_is_not_repeated() {
    // long_review shape: many older turns, a small kept tail.
    let compressor = ContextCompressor::new(44_237);
    let mut history = vec![
        Message::system("You are selfware.\n".repeat(900)),
        Message::user("Review the agent loop."),
    ];
    for i in 0..40 {
        history.push(Message::assistant(format!(
            "stage {i} notes {}",
            prose(900)
        )));
        history.push(Message::user(format!(
            "<tool_result>{}</tool_result>",
            prose(1_800)
        )));
    }
    for i in 0..3 {
        history.push(Message::assistant(format!("tail {i}")));
        history.push(Message::user(format!("tail result {i}")));
    }
    assert!(compressor.summary_skip_reason(&history, None).is_none());
    let mut compressor = compressor;
    let s = compressor
        .summary_split(&history, None)
        .unwrap()
        .summarizable_tokens;
    compressor.note_summary_rejected(s);
    let reason = compressor.summary_skip_reason(&history, None).unwrap();
    assert!(
        reason.contains("left the history above the threshold"),
        "{reason}"
    );
    // Enough new material re-enables a call; an accepted summary clears it.
    for i in 0..4 {
        history.insert(3, Message::assistant(format!("more {i} {}", prose(4_000))));
        history.insert(4, Message::user(format!("more result {i}")));
    }
    assert!(compressor.summary_skip_reason(&history, None).is_none());
    compressor.note_summary_accepted();
    assert!(compressor.summary_skip_reason(&history, None).is_none());
    // Review (0.9.1): a FAILED summary call backs off the same way, and the
    // skip reason says it failed.
    let s = compressor
        .summary_split(&history, None)
        .unwrap()
        .summarizable_tokens;
    compressor.note_summary_failed(s);
    let reason = compressor.summary_skip_reason(&history, None).unwrap();
    assert!(reason.contains("failed"), "{reason}");
    compressor.note_summary_accepted();
    assert!(compressor.summary_skip_reason(&history, None).is_none());
}
