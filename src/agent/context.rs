use crate::api::types::{Message, Usage};
use crate::api::ApiClient;
use crate::api::ThinkingMode;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use tracing::{debug, info, warn};

/// Per-message overhead tokens (role header, formatting, separators).
const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// Advance a tail start index past any leading `role == "tool"` messages so
/// the kept tail never begins on an orphan tool result (whose matching
/// assistant tool_call was compacted away). Those leading tool results move
/// into the summarized/dropped region instead, keeping tool-call pairs
/// intact and avoiding unpaired-tool_calls 400s.
fn safe_tail_start(messages: &[Message], desired: usize) -> usize {
    let mut start = desired.min(messages.len());
    while start < messages.len() && messages[start].role == "tool" {
        start += 1;
    }
    start
}

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
        let recent_start = safe_tail_start(
            messages,
            messages.len().saturating_sub(self.min_messages_to_keep),
        );
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

        // Preserve the original task objective (the first user message) so an
        // emergency compaction doesn't make the model forget the task on a long run.
        let tail_start = messages.len().saturating_sub(3);
        if let Some((idx, task_msg)) = messages.iter().enumerate().find(|(_, m)| m.role == "user") {
            if idx < tail_start {
                result.push(Message::user(format!(
                    "[Original task, preserved across compression]:\n{}",
                    task_msg.content.text()
                )));
            }
        }

        // Add a note about compression
        result.push(Message::user(
            "[Earlier context was compressed due to length limits]",
        ));

        // Keep only last few messages (must end with user for next assistant response)
        let start = safe_tail_start(messages, messages.len().saturating_sub(3));
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
#[path = "../../tests/unit/agent/context/context_test.rs"]
mod tests;
