use super::*;

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
