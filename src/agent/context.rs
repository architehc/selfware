use crate::api::types::Message;
use crate::api::ApiClient;
use crate::api::ThinkingMode;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use tracing::{debug, info, warn};

pub struct ContextCompressor {
    #[allow(dead_code)] // Used for absolute budget enforcement (planned feature)
    token_budget: usize,
    compression_threshold: usize,
    min_messages_to_keep: usize,
}

impl ContextCompressor {
    pub fn new(token_budget: usize) -> Self {
        Self {
            token_budget,
            compression_threshold: (token_budget as f32 * 0.85) as usize,
            min_messages_to_keep: 6,
        }
    }

    pub fn should_compress(&self, messages: &[Message]) -> bool {
        let estimated = self.estimate_tokens(messages);
        debug!(
            "Estimated tokens: {}/{}",
            estimated, self.compression_threshold
        );
        estimated > self.compression_threshold
    }

    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .map(|m| estimate_tokens_with_overhead(&m.content, 50))
            .sum()
    }

    pub async fn compress(&self, client: &ApiClient, messages: &[Message]) -> Result<Vec<Message>> {
        if messages.len() <= self.min_messages_to_keep + 1 {
            warn!("Too few messages to compress, returning as-is");
            return Ok(messages.to_vec());
        }

        info!("Compressing context: {} messages", messages.len());

        let system_msg = messages.first().cloned();
        let recent_start = messages.len().saturating_sub(self.min_messages_to_keep);
        let recent_msgs: Vec<Message> = messages[recent_start..].to_vec();
        let to_summarize = &messages[1..recent_start];

        if to_summarize.is_empty() {
            return Ok(messages.to_vec());
        }

        let summary_content = format!(
            "Summarize these previous interactions concisely. Preserve key facts, decisions, and file paths. Omit routine tool outputs unless they indicate errors.\n\n{}",
            to_summarize.iter().enumerate().map(|(i, m)| {
                // Use char-based truncation to avoid UTF-8 boundary issues
                let content = if m.content.chars().count() > 500 {
                    format!("{}...[truncated]", m.content.chars().take(500).collect::<String>())
                } else {
                    m.content.clone()
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

        let summary = response.choices[0].message.content.clone();
        info!("Generated summary: {} chars", summary.len());

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

        let new_estimate = self.estimate_tokens(&compressed);
        info!(
            "Compression complete: {} -> {} messages, ~{} tokens",
            messages.len(),
            compressed.len(),
            new_estimate
        );

        Ok(compressed)
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
        assert_eq!(compressor.token_budget, 100000);
        assert_eq!(compressor.compression_threshold, 85000);
        assert_eq!(compressor.min_messages_to_keep, 6);
    }

    #[test]
    fn test_estimate_tokens_simple() {
        let compressor = ContextCompressor::new(100000);
        let messages = vec![
            Message::system("Hello world"), // 11 chars / 4 + 50 = 52
        ];
        let estimate = compressor.estimate_tokens(&messages);
        assert!(estimate > 50); // At minimum, base cost per message
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

        // Both should have reasonable estimates
        assert!(code_estimate > 50);
        assert!(text_estimate > 50);
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
        assert!(estimate > 150); // 3 messages * ~50 base
    }

    // Additional tests for improved coverage

    #[test]
    fn test_compression_threshold_calculation() {
        let compressor = ContextCompressor::new(10000);
        // Threshold should be 85% of budget
        assert_eq!(compressor.compression_threshold, 8500);
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
        assert!(estimate > 50 && estimate < 100);
    }

    #[test]
    fn test_estimate_tokens_with_braces() {
        let compressor = ContextCompressor::new(100000);
        // Code with braces uses factor 3
        let messages = vec![Message::user("fn main() { println!(\"hello\"); }")];
        let estimate = compressor.estimate_tokens(&messages);
        assert!(estimate > 50 && estimate < 100);
    }

    #[test]
    fn test_estimate_tokens_plain_text() {
        let compressor = ContextCompressor::new(100000);
        // Plain text without code markers uses factor 4
        let messages = vec![Message::user("This is plain text without any code")];
        let estimate = compressor.estimate_tokens(&messages);
        // Should be chars/4 + 50
        assert!(estimate > 50);
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
        assert!(estimate > 50);
    }

    #[test]
    fn test_estimate_tokens_unicode() {
        let compressor = ContextCompressor::new(100000);
        // Unicode characters should be counted properly
        let messages = vec![Message::user("日本語テスト 🦀 Rust")];

        let estimate = compressor.estimate_tokens(&messages);
        // Should not crash and give reasonable estimate
        assert!(estimate > 50);
    }
}
