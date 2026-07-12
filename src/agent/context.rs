use crate::api::types::{Message, Usage};
use crate::api::ApiClient;
use crate::api::ThinkingMode;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use tracing::{debug, info, warn};

/// Per-message overhead tokens (role header, formatting, separators).
const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// Estimate the token cost of a single message, including text, images,
/// per-message overhead, and tool calls. This is the single source of truth
/// for message-level token estimation — both `ContextCompressor` and
/// `trim_message_history` use it.
pub fn estimate_message_tokens(m: &Message) -> usize {
    let mut total = estimate_tokens_with_overhead(&m.content.text_all(), MESSAGE_OVERHEAD_TOKENS)
        + m.content.image_count() * crate::tokens::DEFAULT_IMAGE_TOKEN_ESTIMATE;
    // Include tool calls if present (must match estimate_messages_tokens in tokens.rs)
    if let Some(ref tool_calls) = m.tool_calls {
        for call in tool_calls {
            total += 10; // Overhead per tool call
            total += crate::token_count::estimate_content_tokens(&call.function.name);
            total += crate::token_count::estimate_content_tokens(&call.function.arguments);
        }
    }
    total
}

/// Hard upper limit on message count. If the message list exceeds this,
/// `should_compress` returns true regardless of token estimate, so the
/// conversation is always bounded.
const MAX_MESSAGE_COUNT: usize = 512;

pub struct ContextCompressor {
    compression_threshold: usize,
    min_messages_to_keep: usize,
}

impl ContextCompressor {
    pub fn new(token_budget: usize) -> Self {
        // Default: compress at 75% (content zone), leaving 20% headroom + 5% thinking.
        Self::with_content_ratio(token_budget, 0.75)
    }

    /// Create with a custom content ratio (fraction of budget that triggers compression).
    pub fn with_content_ratio(token_budget: usize, content_ratio: f32) -> Self {
        Self {
            compression_threshold: (token_budget as f32 * content_ratio) as usize,
            min_messages_to_keep: 6,
        }
    }

    pub fn should_compress(&self, messages: &[Message]) -> bool {
        // Hard cap on message count to prevent unbounded Vec growth.
        if messages.len() > MAX_MESSAGE_COUNT {
            warn!(
                "Message count {} exceeds hard limit {}, forcing compression",
                messages.len(),
                MAX_MESSAGE_COUNT
            );
            return true;
        }

        let estimated = self.estimate_tokens(messages);
        debug!(
            "Estimated tokens: {}/{}",
            estimated, self.compression_threshold
        );
        estimated > self.compression_threshold
    }

    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(estimate_message_tokens).sum()
    }

    pub fn compression_threshold(&self) -> usize {
        self.compression_threshold
    }

    /// Returns the (possibly) compressed messages and the token usage the
    /// summarizer LLM call consumed (zero when no call was made), so the caller
    /// can account it against the budget.
    pub async fn compress(
        &self,
        client: &ApiClient,
        messages: &[Message],
    ) -> Result<(Vec<Message>, Usage)> {
        let zero_usage = || Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost: None,
        };
        if messages.len() <= self.min_messages_to_keep + 1 {
            warn!("Too few messages to compress, returning as-is");
            return Ok((messages.to_vec(), zero_usage()));
        }

        info!("Compressing context: {} messages", messages.len());

        let system_msg = messages.first().cloned();
        let recent_start = messages.len().saturating_sub(self.min_messages_to_keep);
        let recent_msgs: Vec<Message> = messages[recent_start..].to_vec();
        let to_summarize = &messages[1..recent_start];

        if to_summarize.is_empty() {
            return Ok((messages.to_vec(), zero_usage()));
        }

        let summary_content = format!(
            "Summarize these previous interactions concisely. Preserve key facts, decisions, and file paths. Omit routine tool outputs unless they indicate errors.\n\n{}",
            to_summarize.iter().enumerate().map(|(i, m)| {
                // Use char-based truncation to avoid UTF-8 boundary issues
                let content = if m.content.chars().count() > 500 {
                    format!("{}...[truncated]", m.content.chars().take(500).collect::<String>())
                } else {
                    m.content.text().to_string()
                };
                format!("[{}] {}: {}", i, m.role, content)
            }).collect::<Vec<_>>().join("\n\n")
        );

        let summary_request = vec![
            Message::system("You are a context summarizer. Compress conversation history while preserving critical information for task completion."),
            Message::user(summary_content)
        ];

        let response = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            client.chat(summary_request, None, ThinkingMode::Disabled),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Context compression API call timed out after 120s"))??;

        let summary = response
            .choices
            .first()
            .map(|c| c.message.content.text().to_string())
            .unwrap_or_else(|| "[Context compression failed: empty API response]".to_string());
        info!("Generated summary: {} chars", summary.len());
        // The summarizer call already spent tokens — carry them out on every
        // post-call return path (including the "compression didn't help" one).
        let usage = response.usage.clone();

        let mut compressed = Vec::new();
        if let Some(sys) = system_msg {
            compressed.push(sys);
        }

        compressed.push(Message::user(format!(
            "[CONTEXT SUMMARY - {} earlier messages compressed]:\n{}",
            to_summarize.len(),
            summary
        )));

        compressed.push(Message::user("[RECENT CONTEXT]:"));
        compressed.push(Message::user(
            "Based on the above summary, please continue the task.",
        ));
        // Keep messages in chronological order (recent_msgs is already chronological)
        compressed.extend(recent_msgs);

        let original_estimate = self.estimate_tokens(messages);
        let new_estimate = self.estimate_tokens(&compressed);

        if new_estimate >= original_estimate {
            warn!(
                "Compression increased token count ({} -> {}), returning original",
                original_estimate, new_estimate
            );
            return Ok((messages.to_vec(), usage));
        }

        info!(
            "Compression saved {} tokens ({} -> {}), {} messages ({} -> {})",
            original_estimate - new_estimate,
            original_estimate,
            new_estimate,
            messages.len(),
            compressed.len(),
            messages.len()
        );

        Ok((compressed, usage))
    }

    pub fn hard_compress(&self, messages: &[Message]) -> Vec<Message> {
        let mut result = Vec::new();
        if let Some(first) = messages.first() {
            result.push(first.clone()); // System
        }

        // Add a note about compression
        result.push(Message::user(
            "[Earlier context was compressed due to length limits]",
        ));

        // Keep only last few messages (must end with user for next assistant response)
        let start = messages.len().saturating_sub(3);
        for msg in &messages[start..] {
            // Skip if this would create consecutive assistants
            if let Some(last) = result.last() {
                if last.role == "assistant" && msg.role == "assistant" {
                    continue; // Skip duplicate assistant
                }
            }
            result.push(msg.clone());
        }

        // Always end with user message to prompt assistant
        if result.last().map(|m| m.role.as_str()) != Some("user") {
            result.push(Message::user(
                "[Continue the task based on the summary above]",
            ));
        }

        result
    }
}

#[cfg(test)]
mod tests {
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
}
