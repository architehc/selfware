use crate::api::types::{Message, Usage};
use crate::api::ApiClient;
use crate::api::ThinkingMode;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use tracing::{debug, info, warn};

/// Per-message overhead tokens (role header, formatting, separators).
const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// Advance a tail start index past any leading `role == "tool"` messages and
/// any leading XML tool-result user messages (`<tool_result>` markup) so the
/// kept tail never begins on an orphan tool result (whose matching assistant
/// tool_call was compacted away). Those leading tool results move into the
/// summarized/dropped region instead, keeping tool-call pairs intact and
/// avoiding unpaired-tool_calls 400s. Skipping the XML tool-result USER
/// variant is also required by the strict role-alternation guarantee: the
/// boundary marker must never sit directly before a tool-result user message,
/// because coalescing would break the tool_use/tool_result pairing.
fn safe_tail_start(messages: &[Message], desired: usize) -> usize {
    let mut start = desired.min(messages.len());
    while start < messages.len()
        && (messages[start].role == "tool"
            || super::Agent::is_tool_result_user_message(&messages[start]))
    {
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
        + m.content.image_count() * crate::token_count::DEFAULT_IMAGE_TOKEN_ESTIMATE;
    // Include tool calls if present (must match estimate_messages_tokens in token_count.rs)
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
        self.compress_with_task(client, messages, None).await
    }

    /// [`Self::compress`] carrying `task` (the active checkpoint's task
    /// description — authoritative) forward verbatim instead of guessing it
    /// from "the first user message after the system prompt", which after an
    /// earlier compaction is a summary/boundary note, not the task.
    pub async fn compress_with_task(
        &self,
        client: &ApiClient,
        messages: &[Message],
        task: Option<&str>,
    ) -> Result<(Vec<Message>, Usage)> {
        let zero_usage = Usage::default;
        if messages.len() <= self.min_messages_to_keep + 1 {
            warn!("Too few messages to compress, returning as-is");
            return Ok((messages.to_vec(), zero_usage()));
        }

        info!("Compressing context: {} messages", messages.len());

        // Preserve the system message BY ROLE — the first message is not
        // guaranteed to BE the system prompt (bootstrap noise can precede it),
        // mirroring the hardened compaction path.
        let system_msg = messages
            .iter()
            .find(|m| m.role == "system")
            .cloned()
            .or_else(|| messages.first().cloned());
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

        // Preserve the ORIGINAL TASK — the first user message after the
        // system prompt — so summarize-based compression of an autonomous
        // long run can't erase the root objective (the anchor lives inside
        // `to_summarize` otherwise and survives only at the summarizer's whim).
        if let Some(task) = resolve_task_text(messages, task) {
            if !recent_msgs
                .iter()
                .any(|r| r.role == "user" && r.content.text().contains(task.as_str()))
            {
                compressed.push(Message::user(super::context_management::task_anchor_text(
                    &task,
                )));
            }
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

        // The boundary markers above are up to FOUR consecutive user-role
        // messages; strict role-alternation providers reject that shape with
        // a 400. Coalesce adjacent PLAIN user turns (never XML tool-result
        // user messages — `safe_tail_start` keeps those out of the window
        // opening) so the rebuilt boundary alternates.
        let compressed = super::Agent::coalesce_adjacent_user_turns(compressed);

        // Tool-call pairing invariants: `safe_tail_start` already stops the
        // recent window from OPENING on an orphan tool result, but a dangling
        // assistant `tool_calls` at the TAIL (e.g. an interrupted final turn)
        // still 400s. Enforce the same invariants as the hardened compaction
        // path so every compression route yields an API-valid message list.
        let compressed = super::Agent::apply_tool_call_pair_invariants(compressed);

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
        self.hard_compress_with_task(messages, None)
    }

    /// [`Self::hard_compress`] carrying `task` (the active checkpoint's task
    /// description) forward verbatim. Without it the boundary falls back to
    /// the first user message before the kept tail — and to a bare
    /// "[Earlier context was compressed]" note when the history is short,
    /// which is how the e2e c40 run lost its task.
    pub fn hard_compress_with_task(
        &self,
        messages: &[Message],
        task: Option<&str>,
    ) -> Vec<Message> {
        let mut result = Vec::new();
        // Preserve the system message BY ROLE — a non-system bootstrap line can
        // otherwise masquerade as the "system" message and the real prompt is
        // dropped (mirrors the hardened compaction path).
        let sys_idx = messages
            .iter()
            .position(|m| m.role == "system")
            .or_else(|| (!messages.is_empty()).then_some(0));
        if let Some(idx) = sys_idx {
            result.push(messages[idx].clone());
        }

        // The kept tail never reaches back over the system prompt. On a
        // history of <= 3 messages the old `len - 3` tail started AT the
        // system prompt: it was pushed a second time, and the "original task"
        // search window was empty, so the boundary became a bare
        // "[Earlier context was compressed]" note that the next trim pinned
        // as the "task" while the real task (now behind a duplicate system
        // message) was dropped — the e2e c40 loss, reconstructed from its
        // session log (7 → 4 → 3 → 5 messages).
        let body_start = sys_idx.map_or(0, |i| i + 1);
        let tail_start = messages.len().saturating_sub(3).max(body_start);

        // Preserve the original task objective so an emergency compaction
        // doesn't make the model forget the task on a long run: the explicit
        // (checkpoint) task when given, else the first user message before
        // the kept tail.
        let task_text = match task.filter(|t| !t.trim().is_empty()) {
            Some(t) => Some(super::Agent::task_anchor_core(t).to_string()),
            None => messages
                .iter()
                .enumerate()
                .find(|(idx, m)| *idx >= body_start && *idx < tail_start && m.role == "user")
                .map(|(_, m)| m.content.text().to_string()),
        };
        // Already a carried-forward boundary: do not wrap it again.
        let task_text = task_text.map(|t| {
            t.strip_prefix("[Original task, preserved across compression]:\n")
                .map(str::to_string)
                .unwrap_or(t)
        });
        let tail_begin = safe_tail_start(messages, tail_start);
        // The kept tail already carries the task verbatim: don't duplicate it.
        let tail_carries_task = |t: &str| {
            messages[tail_begin..]
                .iter()
                .any(|m| m.role == "user" && m.content.text().contains(t))
        };
        let task_text = task_text.filter(|t| !(task.is_some() && tail_carries_task(t)));

        // ONE boundary message combining the task anchor and the compression
        // note. The previous shape emitted two consecutive user-role markers
        // (plus the trailing continue prompt); strict role-alternation
        // providers reject consecutive same-role messages, so the markers are
        // folded together at source instead.
        result.push(Message::user(match task_text {
            Some(task) => format!(
                "[Original task, preserved across compression]:\n{task}\n\n\
                 [Earlier context was compressed due to length limits]"
            ),
            None => "[Earlier context was compressed due to length limits]".to_string(),
        }));

        // Keep only last few messages (must end with user for next assistant response)
        let mut first_tail = true;
        for msg in messages[tail_begin..].iter() {
            // The system prompt is already first; never duplicate it.
            if msg.role == "system" {
                continue;
            }
            // Skip if this would create consecutive assistants
            if let Some(last) = result.last() {
                if last.role == "assistant" && msg.role == "assistant" {
                    continue; // Skip duplicate assistant
                }
            }
            // Fold a leading real user turn into the boundary message so the
            // tail cannot open on two consecutive user-role messages. XML
            // tool-result user messages and image-bearing turns are never
            // folded (the tool_use/tool_result pairing must keep its own
            // user message).
            let fold = first_tail && super::Agent::is_mergeable_user_turn(msg);
            first_tail = false;
            if fold {
                if let Some(last) = result.last_mut() {
                    let prev = last.content.text().to_string();
                    last.content = crate::api::types::MessageContent::Text(format!(
                        "{prev}\n\n{}",
                        msg.content.text()
                    ));
                    continue;
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

        // The consecutive-assistant pruning above can strand a `tool` result
        // whose assistant tool_call was skipped — drop any orphan, same
        // invariants as every other compression path.
        super::Agent::apply_tool_call_pair_invariants(result)
    }
}

/// The task text a compressor carries forward: the explicit (checkpoint)
/// task when given — its pinned prefix for oversized tasks — else the
/// messages-only heuristic (first user message after the system prompt).
fn resolve_task_text(messages: &[Message], task: Option<&str>) -> Option<String> {
    match task.filter(|t| !t.trim().is_empty()) {
        Some(t) => Some(super::Agent::task_anchor_core(t).to_string()),
        None => super::Agent::original_task_anchor(messages).map(|m| m.content.text().to_string()),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/context/context_test.rs"]
mod tests;
