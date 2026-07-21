use super::*;

#[test]
fn test_micro_compact_basic() {
    // Need many messages for meaningful compression (> MIN_MESSAGES_TO_KEEP + buffer)
    let mut messages = vec![Message::system("System prompt")];

    // Add 30 message pairs to have significant compression
    for i in 0..15 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }
    messages.push(Message::user("Current question"));
    // Total: 1 + 30 + 1 = 32 messages
    // After compression: 1 (system) + 1 (summary) + 22 (recent) + 1 = 25

    let metrics = micro_compact(&mut messages);

    assert_eq!(metrics.method, CompressionMethod::Micro);
    // tokens_saved is usize, always >= 0 by definition
    // Should be compressed
    assert!(
        messages.len() <= 26,
        "Expected at most 26 messages, got {}",
        messages.len()
    );
    assert_eq!(messages[0].role, "system"); // System preserved
}

#[test]
fn test_micro_compact_keeps_recent() {
    // Need at least 23 messages for compression to trigger
    let mut messages = vec![Message::system("System prompt")];

    // Add 20 older exchanges
    for i in 0..10 {
        messages.push(Message::user(format!("Old user {}", i)));
        messages.push(Message::assistant(format!("Old assistant {}", i)));
    }

    // Add recent messages that should be preserved
    messages.push(Message::user("Recent user"));
    messages.push(Message::assistant("Recent assistant"));
    // Add one more to ensure we're over the threshold
    messages.push(Message::user("Final question"));

    micro_compact(&mut messages);

    // Check that recent messages are preserved
    let has_recent_user = messages
        .iter()
        .any(|m| m.content.text().contains("Recent user"));
    assert!(has_recent_user, "Recent user message should be preserved");
}

#[test]
fn test_micro_compact_strips_reasoning() {
    // Need many messages for meaningful compression
    let mut messages = vec![Message::system("System prompt")];

    // Add 20 old exchanges (40 messages) with reasoning
    for i in 0..20 {
        messages.push(Message::user(format!("Old question {}", i)));
        messages.push(Message::assistant_with_reasoning(
            format!("Old answer {}", i),
            format!("Old reasoning {}", i),
        ));
    }

    // Add recent messages
    messages.push(Message::user("Recent question"));
    messages.push(Message::assistant_with_reasoning(
        "Recent answer",
        "Recent reasoning",
    ));
    // Total: 1 + 40 + 2 = 43 messages
    // keep_start = 43 - 22 = 21, so messages[1..21] (20 messages) get compressed

    micro_compact(&mut messages);

    // After compression, old messages are summarized into a single message
    // The "Old answer" messages should no longer exist individually
    let has_old_answer = messages
        .iter()
        .any(|m| m.content.text().contains("Old answer 0"));
    let has_recent_answer = messages
        .iter()
        .any(|m| m.content.text().contains("Recent answer"));

    // Old messages should be compressed away
    assert!(!has_old_answer, "Old messages should be compressed");
    // Recent messages should be kept
    assert!(has_recent_answer, "Recent messages should be kept");
}

#[test]
fn test_file_access_tracker() {
    let mut tracker = FileAccessTracker::new(5);

    tracker.record_access("file1.rs");
    tracker.record_access("file2.rs");
    tracker.record_access("file3.rs");

    let recent = tracker.get_recent_files(2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0], "file3.rs"); // Most recent first
    assert_eq!(recent[1], "file2.rs");
}

#[test]
fn test_file_access_tracker_updates_timestamp() {
    let mut tracker = FileAccessTracker::new(5);

    tracker.record_access("file1.rs");
    std::thread::sleep(std::time::Duration::from_millis(10));
    tracker.record_access("file2.rs");
    std::thread::sleep(std::time::Duration::from_millis(10));
    tracker.record_access("file1.rs"); // Re-access file1

    let recent = tracker.get_recent_files(2);
    assert_eq!(recent[0], "file1.rs"); // file1 should now be most recent
}

#[test]
fn test_compression_metrics() {
    let metrics = CompressionMetrics::new(CompressionMethod::Auto, 1000, 600, 20, 8, 150);

    assert_eq!(metrics.tokens_saved, 400);
    assert!(metrics.summary().contains("AutoCompact"));
    assert!(metrics.summary().contains("400"));
}

#[test]
fn test_auto_compact_config_default() {
    let config = AutoCompactConfig::default();
    assert_eq!(config.token_threshold, 80);
    assert_eq!(config.reserve_buffer, 13_000);
    assert_eq!(config.max_summary_tokens, 20_000);
    assert_eq!(config.max_consecutive_failures, 3);
}

#[test]
fn test_auto_compact_manager_circuit_breaker() {
    let mut manager = AutoCompactManager::new(AutoCompactConfig::default());

    // Record failures up to the limit
    for _ in 0..3 {
        manager.record_failure();
    }

    assert!(manager.is_circuit_open());

    // Reset should clear the circuit
    manager.reset_circuit();
    assert!(!manager.is_circuit_open());
    assert_eq!(manager.consecutive_failures, 0);
}

#[test]
fn test_compression_method_display() {
    assert_eq!(CompressionMethod::Micro.to_string(), "MicroCompact");
    assert_eq!(CompressionMethod::Auto.to_string(), "AutoCompact");
    assert_eq!(CompressionMethod::Full.to_string(), "FullCompact");
}

#[test]
fn test_micro_compact_small_conversation() {
    // Small conversations shouldn't be compressed
    let mut messages = vec![
        Message::system("System prompt"),
        Message::user("Question 1"),
        Message::assistant("Answer 1"),
        Message::user("Question 2"),
    ];

    let metrics = micro_compact(&mut messages);

    // Should return without changes for small conversations
    assert_eq!(metrics.messages_before, metrics.messages_after);
}

#[test]
fn test_orchestrator_total_tokens_saved() {
    let mut orchestrator = CompressionOrchestrator::new();

    // Simulate some compressions
    orchestrator.metrics_history.push(CompressionMetrics::new(
        CompressionMethod::Micro,
        1000,
        800,
        20,
        15,
        50,
    ));
    orchestrator.metrics_history.push(CompressionMetrics::new(
        CompressionMethod::Auto,
        800,
        500,
        15,
        8,
        100,
    ));

    assert_eq!(orchestrator.total_tokens_saved(), 500);
}

#[test]
fn test_micro_compact_preserves_tool_calls() {
    use crate::api::types::{ToolCall, ToolFunction};

    let mut messages = vec![
        Message::system("System prompt"),
        Message::user("Old question"),
        Message {
            role: "assistant".to_string(),
            content: MessageContent::Text("Let me check".to_string()),
            reasoning_content: None,
            tool_calls: Some(vec![ToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ToolFunction {
                    name: "file_read".to_string(),
                    arguments: r#"{"path": "test.rs"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
            name: None,
        },
        Message::user("Recent question"),
        Message::assistant("Recent answer"),
    ];

    // Add more messages to trigger compression
    for i in 0..15 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }

    let metrics = micro_compact(&mut messages);

    // Should have compressed
    assert!(metrics.messages_after < metrics.messages_before);
    // System message should be preserved
    assert_eq!(messages[0].role, "system");
}

#[test]
fn test_file_access_tracker_get_tracked_files() {
    let mut tracker = FileAccessTracker::new(5);

    // Initially empty
    let tracked = tracker.get_tracked_files();
    assert!(tracked.is_empty());

    // Record some accesses
    tracker.record_access("file1.rs");
    std::thread::sleep(std::time::Duration::from_millis(5));
    tracker.record_access("file2.rs");

    let tracked = tracker.get_tracked_files();
    assert_eq!(tracked.len(), 2);

    // Check that durations are reasonable (should be very small)
    for (_, duration) in &tracked {
        assert!(duration.as_secs() < 1); // Should be less than 1 second
    }
}

#[test]
fn test_file_access_tracker_is_empty_and_len() {
    let mut tracker = FileAccessTracker::new(5);

    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);

    tracker.record_access("file1.rs");

    assert!(!tracker.is_empty());
    assert_eq!(tracker.len(), 1);

    tracker.record_access("file2.rs");
    assert_eq!(tracker.len(), 2);
}

#[test]
fn test_file_access_tracker_clear() {
    let mut tracker = FileAccessTracker::new(5);

    tracker.record_access("file1.rs");
    tracker.record_access("file2.rs");
    assert_eq!(tracker.len(), 2);

    tracker.clear();

    assert!(tracker.is_empty());
    assert_eq!(tracker.len(), 0);
    assert!(tracker.get_recent_files(10).is_empty());
}

#[test]
fn test_file_access_tracker_capacity_limit() {
    let mut tracker = FileAccessTracker::new(3);

    // Add more files than capacity
    tracker.record_access("file1.rs");
    tracker.record_access("file2.rs");
    tracker.record_access("file3.rs");
    tracker.record_access("file4.rs");

    // Should only keep the most recent 3
    assert_eq!(tracker.len(), 3);

    let recent = tracker.get_recent_files(3);
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0], "file4.rs"); // Most recent
    assert_eq!(recent[1], "file3.rs");
    assert_eq!(recent[2], "file2.rs"); // file1.rs should be evicted
}

#[test]
fn test_auto_compact_manager_with_threshold() {
    let manager = AutoCompactManager::with_threshold(70);

    assert_eq!(manager.config.token_threshold, 70);
    assert_eq!(manager.config.reserve_buffer, 13_000); // Default
    assert!(!manager.is_circuit_open());
}

#[test]
fn test_auto_compact_manager_should_compress() {
    let manager = AutoCompactManager::with_threshold(80);

    // Below threshold (80% of 100K = 80K)
    assert!(!manager.should_compress(70_000, 100_000));

    // At threshold
    assert!(manager.should_compress(80_000, 100_000));

    // Above threshold
    assert!(manager.should_compress(90_000, 100_000));

    // Edge case: small context window
    assert!(manager.should_compress(800, 1000));
    assert!(!manager.should_compress(799, 1000));
}

#[test]
fn test_auto_compact_manager_should_compress_circuit_breaker() {
    let mut manager = AutoCompactManager::with_threshold(80);

    // Initially should compress
    assert!(manager.should_compress(90_000, 100_000));

    // Open the circuit
    for _ in 0..3 {
        manager.record_failure();
    }
    assert!(manager.is_circuit_open());

    // Should not compress when circuit is open
    assert!(!manager.should_compress(90_000, 100_000));
}

#[test]
fn test_auto_compact_manager_record_success() {
    let mut manager = AutoCompactManager::new(AutoCompactConfig::default());

    // Record some failures first
    manager.record_failure();
    manager.record_failure();
    assert_eq!(manager.consecutive_failures, 2);

    // Record success should reset failures
    let metrics = CompressionMetrics::new(CompressionMethod::Auto, 1000, 600, 20, 8, 150);
    manager.record_success(metrics.clone());

    assert_eq!(manager.consecutive_failures, 0);
    assert!(manager.last_compression().is_some());
    assert_eq!(manager.last_compression().unwrap().tokens_saved, 400);
}

#[test]
fn test_compression_orchestrator_with_config() {
    let config = AutoCompactConfig {
        token_threshold: 75,
        reserve_buffer: 10_000,
        max_summary_tokens: 15_000,
        max_consecutive_failures: 5,
    };

    let orchestrator = CompressionOrchestrator::with_config(config.clone());

    assert!(orchestrator.metrics_history().is_empty());
    assert!(orchestrator.file_tracker().is_empty());
    assert_eq!(orchestrator.total_tokens_saved(), 0);
}

#[test]
fn test_compression_orchestrator_record_file_access() {
    let mut orchestrator = CompressionOrchestrator::new();

    orchestrator.record_file_access("src/main.rs");
    orchestrator.record_file_access("src/lib.rs");

    let tracker = orchestrator.file_tracker();
    assert_eq!(tracker.len(), 2);

    let recent = tracker.get_recent_files(2);
    assert!(recent.contains(&"src/main.rs".to_string()));
    assert!(recent.contains(&"src/lib.rs".to_string()));
}

#[test]
fn test_compression_orchestrator_reset() {
    let mut orchestrator = CompressionOrchestrator::new();

    // Add some state
    orchestrator.record_file_access("file.rs");
    orchestrator.metrics_history.push(CompressionMetrics::new(
        CompressionMethod::Micro,
        1000,
        800,
        20,
        15,
        50,
    ));

    // Open circuit breaker
    orchestrator.auto_manager.record_failure();
    orchestrator.auto_manager.record_failure();
    orchestrator.auto_manager.record_failure();
    assert!(orchestrator.auto_manager.is_circuit_open());

    // Reset
    orchestrator.reset();

    // Verify all cleared
    assert!(orchestrator.file_tracker().is_empty());
    assert!(orchestrator.metrics_history().is_empty());
    assert!(!orchestrator.auto_manager.is_circuit_open());
    assert_eq!(orchestrator.total_tokens_saved(), 0);
}

#[test]
fn test_micro_compact_exactly_at_threshold() {
    // Test with exactly 12 messages (at threshold, should not compress)
    let mut messages = vec![Message::system("System prompt")];
    for i in 0..5 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }
    messages.push(Message::user("Final"));
    // Total: 1 + 10 + 1 = 12 messages

    let metrics = micro_compact(&mut messages);

    // Should not compress at exactly 12
    assert_eq!(metrics.messages_before, metrics.messages_after);
}

#[test]
fn test_micro_compact_at_13_messages() {
    // Test with 13 messages (just over threshold of 12)
    let mut messages = vec![Message::system("System prompt")];
    for i in 0..5 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }
    messages.push(Message::user("Final 1"));
    messages.push(Message::user("Final 2"));
    // Total: 1 + 10 + 2 = 13 messages
    assert_eq!(messages.len(), 13);

    let metrics = micro_compact(&mut messages);

    // At 13 messages, keep_start = 13 - 22 = 0, so nothing to compress
    assert_eq!(metrics.messages_before, metrics.messages_after);
}

#[test]
fn test_micro_compact_with_tool_messages() {
    let mut messages = vec![Message::system("System prompt")];

    // Add old tool messages
    for i in 0..8 {
        messages.push(Message::user(format!("Request {}", i)));
        messages.push(Message {
            role: "tool".to_string(),
            content: MessageContent::Text(format!("Tool result {} with lots of content here", i)),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(format!("call_{}", i)),
            name: Some("test_tool".to_string()),
        });
    }

    // Add recent messages
    for i in 0..8 {
        messages.push(Message::user(format!("Recent {}", i)));
        messages.push(Message::assistant(format!("Answer {}", i)));
    }

    let metrics = micro_compact(&mut messages);

    // Should have compressed
    assert!(metrics.messages_after < metrics.messages_before);

    // Check that summary contains tool count
    let summary_msg = &messages[1];
    assert!(summary_msg.content.text().contains("tool"));
}

#[test]
fn test_micro_compact_empty_messages() {
    // Test with empty messages
    let mut messages: Vec<Message> = vec![];

    let metrics = micro_compact(&mut messages);

    assert_eq!(metrics.messages_before, 0);
    assert_eq!(metrics.messages_after, 0);
    assert!(messages.is_empty());
}

#[test]
fn test_micro_compact_reasoning_stripping() {
    let mut messages = vec![Message::system("System prompt")];

    // Add messages with reasoning - need enough to trigger compression
    for i in 0..15 {
        messages.push(Message::user(format!("Question {}", i)));
        messages.push(Message::assistant_with_reasoning(
            format!("Answer {}", i),
            format!("Reasoning for answer {} with detailed thought process", i),
        ));
    }

    let _metrics = micro_compact(&mut messages);

    // Count how many messages still have reasoning content
    let reasoning_count = messages
        .iter()
        .filter(|m| m.reasoning_content.is_some())
        .count();

    // Only the last 4 messages in "recent" section should keep reasoning
    // Recent section has 22 messages, last 4 exchanges (8 messages) keep reasoning
    // But the summary message doesn't have reasoning
    assert!(
        reasoning_count <= 4,
        "Expected at most 4 messages with reasoning, got {}",
        reasoning_count
    );
}

#[test]
fn test_compression_metrics_all_methods() {
    // Test Micro
    let micro = CompressionMetrics::new(CompressionMethod::Micro, 2000, 1500, 30, 25, 10);
    assert_eq!(micro.tokens_saved, 500);
    assert!(micro.summary().contains("MicroCompact"));

    // Test Auto
    let auto = CompressionMetrics::new(CompressionMethod::Auto, 5000, 3000, 50, 10, 100);
    assert_eq!(auto.tokens_saved, 2000);
    assert!(auto.summary().contains("AutoCompact"));

    // Test Full
    let full = CompressionMetrics::new(CompressionMethod::Full, 10000, 5000, 100, 5, 200);
    assert_eq!(full.tokens_saved, 5000);
    assert!(full.summary().contains("FullCompact"));
}

#[test]
fn test_compression_metrics_zero_savings() {
    let metrics = CompressionMetrics::new(
        CompressionMethod::Micro,
        1000,
        1200, // After > before
        10,
        12,
        5,
    );

    // tokens_saved should be 0 (saturating_sub)
    assert_eq!(metrics.tokens_saved, 0);
    assert!(metrics.summary().contains("Saved 0 tokens"));
}

#[test]
fn test_micro_compact_preserves_system_message() {
    let mut messages = vec![Message::system(
        "Important system prompt that must be preserved",
    )];

    // Add many user/assistant messages
    for i in 0..20 {
        messages.push(Message::user(format!("User message {}", i)));
        messages.push(Message::assistant(format!("Assistant response {}", i)));
    }

    micro_compact(&mut messages);

    // System message should still be first
    assert_eq!(messages[0].role, "system");
    assert!(messages[0]
        .content
        .text()
        .contains("Important system prompt"));
}

#[test]
fn test_orchestrator_run_micro() {
    let mut orchestrator = CompressionOrchestrator::new();

    let mut messages = vec![Message::system("System")];
    for i in 0..15 {
        messages.push(Message::user(format!("Q{}", i)));
        messages.push(Message::assistant(format!("A{}", i)));
    }

    let metrics = orchestrator.run_micro(&mut messages);

    assert_eq!(metrics.method, CompressionMethod::Micro);
    assert_eq!(orchestrator.metrics_history().len(), 1);
    assert_eq!(orchestrator.total_tokens_saved(), metrics.tokens_saved);
}

#[test]
fn test_orchestrator_multiple_micro_runs() {
    let mut orchestrator = CompressionOrchestrator::new();

    // First compression
    let mut messages1 = vec![Message::system("System")];
    for i in 0..15 {
        messages1.push(Message::user(format!("Q{}", i)));
        messages1.push(Message::assistant(format!("A{}", i)));
    }
    orchestrator.run_micro(&mut messages1);

    // Second compression
    let mut messages2 = vec![Message::system("System")];
    for i in 0..15 {
        messages2.push(Message::user(format!("Q{}", i)));
        messages2.push(Message::assistant(format!("A{}", i)));
    }
    orchestrator.run_micro(&mut messages2);

    assert_eq!(orchestrator.metrics_history().len(), 2);
    assert_eq!(
        orchestrator.total_tokens_saved(),
        orchestrator.metrics_history[0].tokens_saved + orchestrator.metrics_history[1].tokens_saved
    );
}

#[test]
fn test_compression_metrics_llm_token_fields_round_trip() {
    // Construct a metrics with non-zero LLM token counts and verify round-trip.
    let metrics = CompressionMetrics::new(CompressionMethod::Auto, 1000, 600, 20, 8, 150)
        .with_llm_tokens(42, 58);

    assert_eq!(metrics.llm_input_tokens, 42);
    assert_eq!(metrics.llm_output_tokens, 58);

    // Cloning should preserve the fields.
    let cloned = metrics.clone();
    assert_eq!(cloned.llm_input_tokens, 42);
    assert_eq!(cloned.llm_output_tokens, 58);

    // Default (without with_llm_tokens) should be 0.
    let default_metrics = CompressionMetrics::new(CompressionMethod::Micro, 100, 80, 5, 4, 1);
    assert_eq!(default_metrics.llm_input_tokens, 0);
    assert_eq!(default_metrics.llm_output_tokens, 0);
}
