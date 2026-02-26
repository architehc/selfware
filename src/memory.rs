//! Agent Memory Management
//!
//! Tracks token usage and manages the agent's conversation context.
//! Features:
//! - Token estimation for messages
//! - Context window limit awareness
//! - Message summarization for long conversations
//! - Memory statistics for monitoring

use crate::api::types::Message;
use crate::config::Config;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use chrono::Utc;

/// Maximum number of memory entries before eviction kicks in.
const MAX_MEMORY_ENTRIES: usize = 10_000;

/// Maximum total estimated tokens across all memory entries.
/// When exceeded, the oldest entries are evicted until under budget.
/// This is complementary to the `MAX_MEMORY_ENTRIES` limit.
const MAX_MEMORY_TOKENS: usize = 500_000;

pub struct AgentMemory {
    context_window: usize,
    entries: Vec<MemoryEntry>,
}

pub struct MemoryEntry {
    pub timestamp: String,
    pub role: String,
    pub content: String,
    pub token_estimate: usize,
}

impl MemoryEntry {
    pub fn from_message(msg: &Message) -> Self {
        let token_estimate = estimate_tokens(&msg.content);
        Self {
            timestamp: Utc::now().to_rfc3339(),
            role: msg.role.clone(),
            content: msg.content.clone(),
            token_estimate,
        }
    }
}

fn estimate_tokens(content: &str) -> usize {
    estimate_tokens_with_overhead(content, 10)
}

impl AgentMemory {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            context_window: config.agent.token_budget,
            entries: Vec::new(),
        })
    }

    /// Add a message to memory.
    ///
    /// Two eviction strategies are applied in order:
    /// 1. **Entry count limit** -- when the number of entries reaches
    ///    `MAX_MEMORY_ENTRIES`, the oldest 25 % of entries are removed.
    /// 2. **Token budget** -- when the total estimated tokens (including the
    ///    new entry) would exceed `MAX_MEMORY_TOKENS`, the oldest entries are
    ///    removed one at a time until the total is under budget.
    pub fn add_message(&mut self, msg: &Message) {
        // Entry count eviction
        if self.entries.len() >= MAX_MEMORY_ENTRIES {
            let remove_count = MAX_MEMORY_ENTRIES / 4;
            self.entries.drain(..remove_count);
        }

        let new_entry = MemoryEntry::from_message(msg);

        // Token budget eviction -- evict oldest entries while over budget.
        let new_tokens = new_entry.token_estimate;
        while self.total_estimated_tokens() + new_tokens > MAX_MEMORY_TOKENS && !self.entries.is_empty() {
            self.entries.remove(0);
        }

        self.entries.push(new_entry);
    }

    /// Estimate total token usage across all memory entries.
    pub fn total_estimated_tokens(&self) -> usize {
        self.entries.iter().map(|e| e.token_estimate).sum()
    }

    /// Get total estimated tokens in memory
    pub fn total_tokens(&self) -> usize {
        self.entries.iter().map(|e| e.token_estimate).sum()
    }

    /// Check if memory is approaching the context window limit
    pub fn is_near_limit(&self) -> bool {
        self.total_tokens().saturating_mul(100) > self.context_window.saturating_mul(85)
    }

    /// Get the context window size
    pub fn context_window(&self) -> usize {
        self.context_window
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get recent entries (last n)
    pub fn recent(&self, n: usize) -> Vec<&MemoryEntry> {
        self.entries.iter().rev().take(n).collect()
    }

    /// Get a formatted summary of recent memory
    pub fn summary(&self, n: usize) -> String {
        let recent = self.recent(n);
        recent
            .iter()
            .map(|e| {
                format!(
                    "[{}] {}: {}...",
                    e.timestamp,
                    e.role,
                    &e.content[..e.content.len().min(50)]
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_memory_new() {
        let config = Config::default();
        let memory = AgentMemory::new(&config).unwrap();
        assert_eq!(memory.context_window(), config.agent.token_budget);
    }

    #[test]
    fn test_agent_memory_uses_token_budget() {
        let mut config = Config::default();
        config.agent.token_budget = 100000;
        let memory = AgentMemory::new(&config).unwrap();
        assert_eq!(memory.context_window(), 100000);
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
        config.agent.token_budget = 100; // Very small budget
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
        config.agent.token_budget = 1000;
        let mut memory = AgentMemory::new(&config).unwrap();

        // Add content that's well above 85% threshold with tokenizer-based counting.
        let content = "a".repeat(20000);
        memory.add_message(&Message::user(&content));

        assert!(memory.is_near_limit());
    }

    #[test]
    fn test_memory_context_window_accessor() {
        let mut config = Config::default();
        config.agent.token_budget = 50000;
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
        config.agent.token_budget = 50;
        let memory = AgentMemory::new(&config).unwrap();

        assert_eq!(memory.context_window(), 50);
    }

    #[test]
    fn test_memory_context_window_large() {
        let mut config = Config::default();
        config.agent.token_budget = 1_000_000;
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
        let config = Config::default();
        let mut memory = AgentMemory::new(&config).unwrap();

        // Add many large messages to exceed the MAX_MEMORY_TOKENS budget.
        // Each message is ~5000 chars which is roughly 1250-1666 tokens.
        // MAX_MEMORY_TOKENS = 500_000, so ~300-400 of these should trigger eviction.
        let big_content = "a".repeat(5000);
        for _ in 0..500 {
            memory.add_message(&Message::user(&big_content));
        }

        // After eviction, total tokens should be at or below MAX_MEMORY_TOKENS
        assert!(
            memory.total_estimated_tokens() <= MAX_MEMORY_TOKENS,
            "total_estimated_tokens ({}) should be <= MAX_MEMORY_TOKENS ({})",
            memory.total_estimated_tokens(),
            MAX_MEMORY_TOKENS
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
}
