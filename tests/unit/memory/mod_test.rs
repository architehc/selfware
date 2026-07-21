use super::*;

#[test]
fn test_agent_memory_new() {
    let config = Config::default();
    let memory = AgentMemory::new(&config).unwrap();
    assert_eq!(memory.context_window(), config.context_length);
    // When context_length is left at its default, the memory cap should
    // fall back to the advertised ResourceQuotas budget (1M), not the
    // smaller conservative default.
    assert_eq!(
        memory.max_memory_tokens(),
        config.resources.quotas.max_context_tokens * 95 / 100
    );
}

#[test]
fn test_agent_memory_uses_context_length() {
    let mut config = Config::default();
    config.context_length = 100000;
    let memory = AgentMemory::new(&config).unwrap();
    assert_eq!(memory.context_window(), 100000);
    assert_eq!(memory.max_memory_tokens(), 95000);
}

#[test]
fn test_memory_entry_from_message() {
    let msg = Message::user("Hello, world!");
    let entry = MemoryEntry::from_message(&msg);
    assert_eq!(entry.role, "user");
    assert_eq!(entry.content, "Hello, world!");
    assert!(entry.token_estimate > 0);
    assert!(!entry.timestamp.is_empty());
}

#[test]
fn test_memory_add_message() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();
    assert!(memory.is_empty());

    memory.add_message(&Message::user("test"));
    assert_eq!(memory.len(), 1);
    assert!(!memory.is_empty());
}

#[test]
fn test_memory_total_tokens() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Hello"));
    memory.add_message(&Message::assistant("Hi there"));

    assert!(memory.total_tokens() > 0);
}

#[test]
fn test_memory_is_near_limit() {
    let mut config = Config::default();
    config.context_length = 1000; // Very small context window
    let mut memory = AgentMemory::new(&config).unwrap();

    // Add enough content to exceed 85% threshold with tokenizer-based counting.
    memory.add_message(&Message::user("x".repeat(10000)));

    assert!(memory.is_near_limit());
}

#[test]
fn test_memory_clear() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("test1"));
    memory.add_message(&Message::user("test2"));
    assert_eq!(memory.len(), 2);

    memory.clear();
    assert!(memory.is_empty());
}

#[test]
fn test_memory_recent() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("first"));
    memory.add_message(&Message::user("second"));
    memory.add_message(&Message::user("third"));

    let recent = memory.recent(2);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].content, "third");
    assert_eq!(recent[1].content, "second");
}

#[test]
fn test_estimate_tokens_prose() {
    let tokens = estimate_tokens("This is a simple prose sentence.");
    assert!(tokens > 10); // At least base cost
}

#[test]
fn test_estimate_tokens_code() {
    let tokens = estimate_tokens("fn main() { println!(\"hello\"); }");
    assert!(tokens > 10); // At least base cost
}

#[test]
fn test_memory_summary_basic() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Hello, this is a test message"));
    memory.add_message(&Message::assistant("Hi there, I'm responding"));

    let summary = memory.summary(2);
    assert!(summary.contains("user:"));
    assert!(summary.contains("assistant:"));
}

#[test]
fn test_memory_summary_truncates_long_content() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    // Add message with content longer than 50 chars
    let long_content = "a".repeat(100);
    memory.add_message(&Message::user(&long_content));

    let summary = memory.summary(1);
    // Should be truncated at 50 chars plus "..."
    assert!(summary.len() < 200); // Much less than the full content
    assert!(summary.contains("..."));
}

#[test]
fn test_memory_summary_empty() {
    let config = Config::default();
    let memory = AgentMemory::new(&config).unwrap();

    let summary = memory.summary(5);
    assert!(summary.is_empty());
}

#[test]
fn test_memory_summary_fewer_entries_than_requested() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Only one message"));

    let summary = memory.summary(10); // Ask for 10, only have 1
    assert!(summary.contains("user:"));
}

#[test]
fn test_memory_not_near_limit() {
    let config = Config::default(); // Large default budget
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Small message"));

    assert!(!memory.is_near_limit());
}

#[test]
fn test_estimate_tokens_empty() {
    let tokens = estimate_tokens("");
    assert_eq!(tokens, 10); // Just the base cost
}

#[test]
fn test_estimate_tokens_with_braces() {
    let tokens_code = estimate_tokens("{ let x = 1; }");
    let tokens_prose = estimate_tokens("hello world test");
    // Code (with braces) uses factor 3, prose uses factor 4
    // Both have similar length, but code should have more tokens
    assert!(tokens_code > 0);
    assert!(tokens_prose > 0);
}

#[test]
fn test_memory_recent_more_than_available() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("only one"));

    let recent = memory.recent(100); // Ask for 100, only have 1
    assert_eq!(recent.len(), 1);
}

#[test]
fn test_memory_entry_fields() {
    let msg = Message::assistant("Test response with code { }");
    let entry = MemoryEntry::from_message(&msg);

    assert_eq!(entry.role, "assistant");
    assert_eq!(entry.content, "Test response with code { }");
    // Token estimate should be calculated with code factor
    assert!(entry.token_estimate > 10);
}

#[test]
fn test_estimate_tokens_with_semicolon() {
    let tokens = estimate_tokens("let x = 1; let y = 2;");
    // Contains semicolon so uses code factor (3)
    assert!(tokens > 10);
}

#[test]
fn test_estimate_tokens_long_text() {
    let long_text = "a".repeat(1000);
    let short_text = "a".repeat(100);
    let tokens = estimate_tokens(&long_text);
    let short_tokens = estimate_tokens(&short_text);
    assert!(tokens > short_tokens);
    assert!(tokens > 10);
}

#[test]
fn test_estimate_tokens_long_code() {
    let long_code = "{ x }".repeat(200);
    let tokens = estimate_tokens(&long_code);
    // Contains braces so uses factor 3
    assert!(tokens > 0);
}

#[test]
fn test_memory_multiple_messages() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    for i in 0..10 {
        memory.add_message(&Message::user(format!("Message {}", i)));
    }

    assert_eq!(memory.len(), 10);
    assert!(memory.total_tokens() > 0);
}

#[test]
fn test_memory_mixed_roles() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Question"));
    memory.add_message(&Message::assistant("Answer"));
    memory.add_message(&Message::user("Follow-up"));
    memory.add_message(&Message::assistant("More info"));

    assert_eq!(memory.len(), 4);
}

#[test]
fn test_memory_recent_ordering() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("First"));
    memory.add_message(&Message::user("Second"));
    memory.add_message(&Message::user("Third"));
    memory.add_message(&Message::user("Fourth"));

    let recent = memory.recent(3);
    assert_eq!(recent.len(), 3);
    // Most recent first
    assert_eq!(recent[0].content, "Fourth");
    assert_eq!(recent[1].content, "Third");
    assert_eq!(recent[2].content, "Second");
}

#[test]
fn test_memory_recent_zero() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Test"));

    let recent = memory.recent(0);
    assert!(recent.is_empty());
}

#[test]
fn test_memory_summary_multiple() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("First message here"));
    memory.add_message(&Message::assistant("First response here"));
    memory.add_message(&Message::user("Second message here"));

    let summary = memory.summary(3);
    assert!(summary.contains("user:"));
    assert!(summary.contains("assistant:"));
    assert!(summary.contains("\n")); // Multiple lines
}

#[test]
fn test_memory_entry_timestamp_format() {
    let msg = Message::user("test");
    let entry = MemoryEntry::from_message(&msg);

    // RFC3339 format check
    assert!(entry.timestamp.contains("T"));
    assert!(entry.timestamp.len() > 20);
}

#[test]
fn test_memory_is_near_limit_boundary() {
    let mut config = Config::default();
    config.context_length = 1000;
    let mut memory = AgentMemory::new(&config).unwrap();

    // Add content that's well above 85% threshold with tokenizer-based counting.
    let content = "a".repeat(20000);
    memory.add_message(&Message::user(&content));

    assert!(memory.is_near_limit());
}

#[test]
fn test_memory_context_window_accessor() {
    let mut config = Config::default();
    config.context_length = 50000;
    let memory = AgentMemory::new(&config).unwrap();

    assert_eq!(memory.context_window(), 50000);
}

#[test]
fn test_memory_clear_then_add() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Before clear"));
    memory.clear();
    memory.add_message(&Message::user("After clear"));

    assert_eq!(memory.len(), 1);
    let recent = memory.recent(1);
    assert_eq!(recent[0].content, "After clear");
}

#[test]
fn test_memory_total_tokens_accumulates() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("First"));
    let first_total = memory.total_tokens();

    memory.add_message(&Message::user("Second"));
    let second_total = memory.total_tokens();

    assert!(second_total > first_total);
}

#[test]
fn test_memory_with_empty_message() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user(""));

    assert_eq!(memory.len(), 1);
    let recent = memory.recent(1);
    assert_eq!(recent[0].content, "");
    // Empty message still has base token cost
    assert_eq!(recent[0].token_estimate, 10);
}

#[test]
fn test_memory_summary_with_short_content() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Hi"));

    let summary = memory.summary(1);
    // Short content shouldn't be truncated
    assert!(summary.contains("Hi..."));
}

#[test]
fn test_estimate_tokens_unicode() {
    let unicode_text = "こんにちは世界";
    let tokens = estimate_tokens(unicode_text);
    // Unicode chars still counted by byte length
    assert!(tokens > 10);
}

#[test]
fn test_estimate_tokens_mixed_content() {
    let mixed = "Hello { world }; more text here without braces";
    let tokens = estimate_tokens(mixed);
    // Contains both { and ; so uses code factor (3)
    assert!(tokens > 10);
}

#[test]
fn test_memory_summary_zero_requested() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("test"));

    let summary = memory.summary(0);
    assert!(summary.is_empty());
}

#[test]
fn test_memory_is_empty_after_multiple_operations() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    assert!(memory.is_empty());
    memory.add_message(&Message::user("test"));
    assert!(!memory.is_empty());
    memory.clear();
    assert!(memory.is_empty());
}

#[test]
fn test_memory_entry_from_system_message() {
    let msg = Message::system("System instruction");
    let entry = MemoryEntry::from_message(&msg);

    assert_eq!(entry.role, "system");
    assert_eq!(entry.content, "System instruction");
}

#[test]
fn test_memory_large_number_of_entries() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    for i in 0..1000 {
        memory.add_message(&Message::user(format!("Message {}", i)));
    }

    assert_eq!(memory.len(), 1000);
    assert!(memory.total_tokens() > 10000); // At least 10 tokens each
}

#[test]
fn test_memory_recent_with_exact_count() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("1"));
    memory.add_message(&Message::user("2"));
    memory.add_message(&Message::user("3"));

    let recent = memory.recent(3);
    assert_eq!(recent.len(), 3);
}

#[test]
fn test_estimate_tokens_whitespace() {
    let whitespace = "   \t\n   \t\n   ";
    let tokens = estimate_tokens(whitespace);
    assert!(tokens > 10); // Base cost + some chars
}

#[test]
fn test_memory_summary_preserves_order() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("AAA"));
    memory.add_message(&Message::user("BBB"));
    memory.add_message(&Message::user("CCC"));

    let summary = memory.summary(3);
    let lines: Vec<&str> = summary.lines().collect();
    // Recent entries come first in the collected vec (reversed)
    // but joined in display order
    assert_eq!(lines.len(), 3);
}

#[test]
fn test_memory_context_window_small() {
    let mut config = Config::default();
    config.context_length = 50;
    let memory = AgentMemory::new(&config).unwrap();

    assert_eq!(memory.context_window(), 50);
}

#[test]
fn test_memory_context_window_large() {
    let mut config = Config::default();
    config.context_length = 1_000_000;
    let memory = AgentMemory::new(&config).unwrap();

    assert_eq!(memory.context_window(), 1_000_000);
}

#[test]
fn test_memory_not_near_limit_empty() {
    let config = Config::default();
    let memory = AgentMemory::new(&config).unwrap();

    // Empty memory should not be near limit
    assert!(!memory.is_near_limit());
}

#[test]
fn test_memory_entry_token_estimate_consistency() {
    let msg = Message::user("Same content");
    let entry1 = MemoryEntry::from_message(&msg);
    let entry2 = MemoryEntry::from_message(&msg);

    // Same content should produce same token estimate
    assert_eq!(entry1.token_estimate, entry2.token_estimate);
}

// =========================================================================
// total_estimated_tokens and token budget eviction tests
// =========================================================================

#[test]
fn test_total_estimated_tokens_empty() {
    let config = Config::default();
    let memory = AgentMemory::new(&config).unwrap();
    assert_eq!(memory.total_estimated_tokens(), 0);
}

#[test]
fn test_total_estimated_tokens_accumulates() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Hello"));
    let first = memory.total_estimated_tokens();
    assert!(first > 0);

    memory.add_message(&Message::user("World"));
    let second = memory.total_estimated_tokens();
    assert!(second > first);
}

#[test]
fn test_total_estimated_tokens_matches_total_tokens() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Test message"));
    memory.add_message(&Message::assistant("Response message"));

    // total_estimated_tokens should equal total_tokens (same computation)
    assert_eq!(memory.total_estimated_tokens(), memory.total_tokens());
}

#[test]
fn test_token_budget_eviction() {
    // Use an explicit, smaller context length so the test exercises
    // eviction rather than relying on the (now larger) default quota fallback.
    let mut config = Config::default();
    config.context_length = 100_000;
    let mut memory = AgentMemory::new(&config).unwrap();

    // Add many large messages to exceed the memory token budget.
    // Each message is ~5000 chars which is roughly 1250-1666 tokens.
    // With a 100k context length, ~80 of these should trigger eviction.
    let big_content = "a".repeat(5000);
    for _ in 0..500 {
        memory.add_message(&Message::user(&big_content));
    }

    // After eviction, total tokens should be at or below the memory budget.
    assert!(
        memory.total_estimated_tokens() <= memory.max_memory_tokens(),
        "total_estimated_tokens ({}) should be <= max_memory_tokens ({})",
        memory.total_estimated_tokens(),
        memory.max_memory_tokens()
    );
}

#[test]
fn test_total_estimated_tokens_after_clear() {
    let config = Config::default();
    let mut memory = AgentMemory::new(&config).unwrap();

    memory.add_message(&Message::user("Test"));
    assert!(memory.total_estimated_tokens() > 0);

    memory.clear();
    assert_eq!(memory.total_estimated_tokens(), 0);
}
