use super::result_compaction::ContextPresence;
use crate::api::client::SideCall;
use crate::api::types::{Message, Usage};
use crate::api::ApiClient;
use crate::token_count::estimate_tokens_with_overhead;
use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use tracing::{debug, info, warn};

/// Per-message overhead tokens (role header, formatting, separators).
const MESSAGE_OVERHEAD_TOKENS: usize = 4;

/// Side-call purpose of the automatic compressor's summary (typed timeout
/// label and `side_call:<purpose>` heartbeat phase).
pub(crate) const CONTEXT_SUMMARY_PURPOSE: &str = "context_summary";
/// Wall-time cap of that side call.
pub(crate) const CONTEXT_SUMMARY_TIME_CAP_SECS: u64 = 90;

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

/// Summarizer instruction for a per-file findings section, parsed back by
/// [`WorkLedger::absorb_summary`] (shared with `compression::auto_compact`).
pub(crate) const PER_FILE_FINDINGS_INSTRUCTION: &str = "End with a section `FILES READ:` \
     containing one line per file whose contents were read, formatted exactly as \
     `- <path>: <1-2 sentence key finding relevant to the task>`. Only list files that \
     were actually read above.";

/// The native tool calls an assistant message made, rendered for the
/// summarizer (`[called file_read {"path":…}]`). Without it a native-FC
/// history shows the summarizer empty assistant turns and bare result JSON,
/// so it cannot tell WHICH file a result came from — no per-file findings.
pub(crate) fn summarizer_tool_call_suffix(m: &Message) -> String {
    let Some(calls) = m.tool_calls.as_deref().filter(|c| !c.is_empty()) else {
        return String::new();
    };
    calls
        .iter()
        .map(|c| {
            let args: String = c.function.arguments.chars().take(200).collect();
            format!(" [called {} {}]", c.function.name, args)
        })
        .collect()
}

/// Hard upper limit on message count. If the message list exceeds this,
/// `should_compress` returns true regardless of token estimate, so the
/// conversation is always bounded.
const MAX_MESSAGE_COUNT: usize = 512;

/// Tokens a context summary is assumed to cost when predicting whether one
/// can help: the p90 of the 23 real summaries in the val083 runs, measured
/// with `estimate_content_tokens` (c24: 160–623, median ~300; long_review:
/// 522–1,028).
pub(crate) const SUMMARY_TOKENS_ESTIMATE: usize = 720;

/// Smallest summarizable portion (history minus system prompt and kept
/// tail) worth a summary call. A call took 9–68 s (c24: 16 calls, 470 s,
/// 32% of the run) and returns up to ~720 tokens (p90), so below ~2x that
/// the expected saving is under one small read; the c24 calls that saved
/// 160–256 tokens were of this kind.
pub(crate) const MIN_SUMMARIZABLE_TOKENS: usize = 1_500;

/// How a history splits for a summary: what would be summarized and what
/// is kept verbatim (system prompt + recent tail), both measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummarySplit {
    pub summarizable_tokens: usize,
    pub kept_tokens: usize,
}

pub struct ContextCompressor {
    compression_threshold: usize,
    min_messages_to_keep: usize,
    /// Summarizable size at the last summary that was rejected for leaving
    /// the history above the threshold; no new call until the summarizable
    /// portion has grown by `MIN_SUMMARIZABLE_TOKENS` (val083 c24: 7 of 16
    /// summaries left the history above the threshold and ran again the
    /// next turn).
    summary_backoff: Option<usize>,
    /// Root the ledger's path keys are computed against: the agent's own
    /// workspace root (`Agent::new` sets it). A standalone compressor falls
    /// back to the current project root.
    key_root: Option<std::path::PathBuf>,
    /// Progress that must outlive every trim/compaction (see [`WorkLedger`]).
    /// Behind a mutex so the `&self` compression paths can record what they
    /// are about to drop before dropping it.
    ledger: Mutex<WorkLedger>,
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
            summary_backoff: None,
            key_root: None,
            ledger: Mutex::new(WorkLedger::new()),
        }
    }

    fn with_ledger<R>(&self, f: impl FnOnce(&mut WorkLedger) -> R) -> R {
        // A poisoned ledger is still valid data (every update is a plain
        // field write); recover it rather than losing the progress record.
        let mut guard = self
            .ledger
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut guard)
    }

    /// Record the successful tool results (and note sources) in `messages`
    /// into the work ledger. Idempotent; called before every trim/compaction
    /// so nothing is dropped unrecorded.
    pub fn observe_work(&self, messages: &[Message]) {
        let root = self.key_root();
        self.with_ledger(|l| l.observe(messages, Some(root.as_path())));
    }

    /// Fix the root the ledger's path keys are computed against.
    pub fn set_key_root(&mut self, root: std::path::PathBuf) {
        self.key_root = Some(root);
    }

    /// The root for path keys (see `key_root`).
    pub fn key_root(&self) -> std::path::PathBuf {
        self.key_root
            .clone()
            .unwrap_or_else(super::current_project_root)
    }

    /// Start a new model turn for the ledger (resets it on a task change).
    pub fn begin_ledger_turn(&self, task: Option<&str>) {
        self.with_ledger(|l| l.begin_turn(task));
    }

    /// The rendered, bounded work ledger (`None` when empty).
    pub fn render_work_ledger(&self, max_tokens: usize) -> Option<String> {
        self.with_ledger(|l| l.render(max_tokens))
    }

    /// The rendered ledger for a request whose history is `messages`: each
    /// file says whether its content is in that history (see
    /// `WorkLedger::render_in_context`).
    pub fn render_work_ledger_for(
        &self,
        max_tokens: usize,
        messages: &[Message],
    ) -> Option<String> {
        let root = self.key_root();
        let presence = ContextPresence::from_messages(messages, &|p| {
            WorkLedger::normalize_path(p, Some(root.as_path()))
        });
        self.with_ledger(|l| l.render_in_context(max_tokens, &presence))
    }

    /// The ledger's recorded finding for a file (stub text for in-place
    /// result compaction).
    pub fn file_finding(&self, path: &str) -> Option<String> {
        let root = self.key_root();
        let key = WorkLedger::normalize_path(path, Some(root.as_path()));
        self.with_ledger(|l| l.file_finding(&key))
    }

    /// The ledger's current model turn (the numbering its entries use).
    pub fn work_ledger_turn(&self) -> usize {
        self.with_ledger(|l| l.turn())
    }

    /// A copy of the current ledger (inspection / tests).
    pub fn work_ledger(&self) -> WorkLedger {
        self.with_ledger(|l| l.clone())
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

    /// Whether `messages` exceed the hard message-count cap (which
    /// [`Self::should_compress`] enforces regardless of tokens).
    pub fn over_message_cap(&self, messages: &[Message]) -> bool {
        messages.len() > MAX_MESSAGE_COUNT
    }

    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        messages.iter().map(estimate_message_tokens).sum()
    }

    pub fn compression_threshold(&self) -> usize {
        self.compression_threshold
    }

    /// Whether [`Self::compress_with_task`] returns `messages` unchanged
    /// WITHOUT a summarizer call because the history is at most the kept
    /// tail (plus the system message). Callers use it to name the real
    /// reason a summary compaction fell back to the hard limit.
    pub fn too_few_to_summarize(&self, messages: &[Message]) -> bool {
        messages.len() <= self.min_messages_to_keep + 1
    }

    /// The split [`Self::compress_with_task`] would make: the messages
    /// between the first one and the kept tail are summarized. `None` when
    /// it would make no summarizer call.
    /// `kept_tokens` includes the task anchor the summary re-adds when the
    /// kept tail does not carry the task.
    pub fn summary_split(&self, messages: &[Message], task: Option<&str>) -> Option<SummarySplit> {
        if self.too_few_to_summarize(messages) {
            return None;
        }
        let recent_start = safe_tail_start(
            messages,
            messages.len().saturating_sub(self.min_messages_to_keep),
        );
        if recent_start <= 1 {
            return None;
        }
        let summarizable_tokens = self.estimate_tokens(&messages[1..recent_start]);
        let anchor_tokens = resolve_task_text(messages, task)
            .filter(|t| {
                !messages[recent_start..]
                    .iter()
                    .any(|r| r.role == "user" && r.content.text().contains(t.as_str()))
            })
            .map_or(0, |t| {
                estimate_message_tokens(&Message::user(
                    super::context_management::task_anchor_text(&t),
                ))
            });
        let kept_tokens = self
            .estimate_tokens(messages)
            .saturating_sub(summarizable_tokens)
            + anchor_tokens;
        Some(SummarySplit {
            summarizable_tokens,
            kept_tokens,
        })
    }

    /// Why a summary call should NOT be made for `messages` (a reason for
    /// the `kept` event), or `None` when one can bring the history under
    /// the compression threshold: the summarizable part is at least
    /// `MIN_SUMMARIZABLE_TOKENS`, the kept part plus a
    /// `SUMMARY_TOKENS_ESTIMATE` summary fits the threshold, and a
    /// rejected summary is not simply being repeated.
    pub fn summary_skip_reason(&self, messages: &[Message], task: Option<&str>) -> Option<String> {
        let Some(split) = self.summary_split(messages, task) else {
            return Some(format!(
                "too few messages to summarize ({}); no summary call made",
                messages.len()
            ));
        };
        let s = split.summarizable_tokens;
        if s < MIN_SUMMARIZABLE_TOKENS {
            return Some(format!(
                "summarizable part ~{s} tokens is below the {MIN_SUMMARIZABLE_TOKENS}-token \
                 floor worth a summary call; no summary call made"
            ));
        }
        let predicted = split.kept_tokens + SUMMARY_TOKENS_ESTIMATE;
        if predicted > self.compression_threshold {
            return Some(format!(
                "a summary cannot bring the history under the threshold: kept part ~{} + \
                 summary ~{SUMMARY_TOKENS_ESTIMATE} > {} tokens; no summary call made",
                split.kept_tokens, self.compression_threshold
            ));
        }
        if let Some(rejected_at) = self.summary_backoff {
            if s < rejected_at + MIN_SUMMARIZABLE_TOKENS {
                return Some(format!(
                    "the last summary (of ~{rejected_at} tokens) left the history above the \
                     threshold and the summarizable part has grown only to ~{s}; no summary \
                     call made"
                ));
            }
        }
        None
    }

    /// Remember a summary that was rejected for leaving the history above
    /// the threshold (see [`Self::summary_skip_reason`]).
    pub fn note_summary_rejected(&mut self, summarizable_tokens: usize) {
        self.summary_backoff = Some(summarizable_tokens);
    }

    /// A summary was accepted: the backoff no longer applies.
    pub fn note_summary_accepted(&mut self) {
        self.summary_backoff = None;
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
        // Record progress before anything is summarized away.
        self.observe_work(messages);
        if self.too_few_to_summarize(messages) {
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
            "Summarize these previous interactions concisely. Preserve key facts, decisions, and file paths. Omit routine tool outputs unless they indicate errors.\n{}\n\n{}",
            PER_FILE_FINDINGS_INSTRUCTION,
            to_summarize.iter().enumerate().map(|(i, m)| {
                // Use char-based truncation to avoid UTF-8 boundary issues
                let content = if m.content.chars().count() > 500 {
                    format!("{}...[truncated]", m.content.chars().take(500).collect::<String>())
                } else {
                    m.content.text().to_string()
                };
                format!("[{}] {}: {}{}", i, m.role, content, summarizer_tool_call_suffix(m))
            }).collect::<Vec<_>>().join("\n\n")
        );

        let summary_request = vec![
            Message::system("You are a context summarizer. Compress conversation history while preserving critical information for task completion."),
            Message::user(summary_content)
        ];

        // A bounded side call, like the other compaction summaries
        // (compression::auto_compact / full_compact): streamed, no tools,
        // output capped at COMPACT_SUMMARY_MAX_TOKENS instead of the session
        // max_tokens, session reasoning effort lowered, and a hard wall cap
        // that fails as the typed `SideCallTimeout`. The previous
        // `client.chat` went out non-streaming with the session's
        // max_tokens and extra_body (enable_thinking/xhigh) under a 120 s
        // outer timeout (external review 2026-09-25 wire capture). 90 s
        // matches full_compact, which likewise summarizes the whole
        // pre-tail history.
        let response = client
            .side_chat(
                summary_request,
                SideCall::new(CONTEXT_SUMMARY_PURPOSE)
                    .max_tokens(super::compression::COMPACT_SUMMARY_MAX_TOKENS)
                    .time_cap_secs(CONTEXT_SUMMARY_TIME_CAP_SECS),
            )
            .await?;

        let summary = response
            .choices
            .first()
            .map(|c| c.message.content.text().to_string())
            .unwrap_or_else(|| "[Context compression failed: empty API response]".to_string());
        info!("Generated summary: {} chars", summary.len());
        // Per-file findings feed the work ledger (only for files it already
        // knows were read — the summary cannot add a read).
        self.with_ledger(|l| l.absorb_summary(&summary));
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
        let mut compressed = super::Agent::apply_tool_call_pair_invariants(compressed);
        // A kept "unchanged since turn N" note whose earlier result was
        // summarized away now says the content is gone.
        let root = self.key_root();
        super::result_compaction::repoint_orphaned_unchanged_notes(
            &mut compressed,
            Some(root.as_path()),
        );

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
        // Emergency compaction keeps only a 3-message tail: record progress
        // first so the ledger still lists what was read.
        self.observe_work(messages);
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
/// `path` with `.` and `..` resolved lexically (no filesystem access): `..`
/// pops a normal component, is dropped at the filesystem root, and is kept
/// at the start of a relative path.
fn lexical_normalize(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out: Vec<Component<'_>> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    out.iter().map(|c| c.as_os_str()).collect()
}

/// ONE canonical key per file for every path-keyed record of the agent (the
/// work ledger's reads, writes, modification tracking and summary findings,
/// compaction stubs, the unchanged re-read note, the re-read tracker, the
/// context map): `.` and `..` resolved lexically and anchored at `root`, so
/// `example.rs`, `./example.rs`, `sub/../example.rs` and `<root>/example.rs`
/// are the same key. A path inside `root` becomes root-relative with `/`
/// separators (`.` for the root itself); a path outside stays absolute.
/// No filesystem access (symlinks are not resolved). External review
/// 2026-09-25: reading `example.rs` and then writing `sub/../example.rs`
/// made two ledger identities, and the stale coverage survived the edit.
pub(crate) fn canonical_workspace_path(path: &str, root: Option<&std::path::Path>) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let raw = std::path::Path::new(trimmed);
    let root = root.map(lexical_normalize);
    let resolved = match (&root, raw.is_absolute()) {
        (Some(root), false) => lexical_normalize(&root.join(raw)),
        _ => lexical_normalize(raw),
    };
    let render = |p: &std::path::Path| -> String {
        p.components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    if let Some(root) = &root {
        if let Ok(rel) = resolved.strip_prefix(root) {
            let rel = render(rel);
            return if rel.is_empty() { ".".to_string() } else { rel };
        }
        return resolved.to_string_lossy().into_owned();
    }
    if resolved.is_absolute() {
        return resolved.to_string_lossy().into_owned();
    }
    let rel = render(&resolved);
    if rel.is_empty() {
        ".".to_string()
    } else {
        rel
    }
}

/// `canonical_workspace_path` as an absolute path (`root` joined to a
/// relative path, `.`/`..` resolved lexically): the key for internal maps
/// that are never shown to the model. An absolute path's key does not
/// depend on the root, so a record made under one root still matches after
/// the root changes (worktree switch, tests sharing one process).
pub(crate) fn canonical_absolute_path(path: &str, root: Option<&std::path::Path>) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let raw = std::path::Path::new(trimmed);
    let joined = match root {
        Some(root) if !raw.is_absolute() => root.join(raw),
        _ => raw.to_path_buf(),
    };
    lexical_normalize(&joined).to_string_lossy().into_owned()
}

fn resolve_task_text(messages: &[Message], task: Option<&str>) -> Option<String> {
    match task.filter(|t| !t.trim().is_empty()) {
        Some(t) => Some(super::Agent::task_anchor_core(t).to_string()),
        None => super::Agent::original_task_anchor(messages).map(|m| m.content.text().to_string()),
    }
}

// =============================================================================
// Work ledger
// =============================================================================
//
// Small context windows lost their PROGRESS on every trim/compaction: the task
// anchor survived, but which files had been read and what was learned did
// not, so the model re-read the same files after each trim (agents10 c24: 23
// trims, the same files re-read each time, wall-cap kill; external run at
// 65,536: 4 trims, four files read 5x, one test file 10x, no report).
//
// The ledger is built ONLY from successful tool results the agent actually
// received (`file_read`, `grep_search`, and the file-mutating tools for the
// deliverable list), lives outside the message history (so no trim can drop
// it), and is rendered — bounded, measured, oldest entries dropped first — at
// the END of each request. It never lands in the system message, which stays
// byte-stable between turns.

/// Heading that opens the rendered ledger inside the request tail.
pub(crate) const WORK_LEDGER_HEADER: &str = "## Work ledger (survives context trimming)";

/// Stored entries (oldest evicted first). Rendering is bounded by tokens
/// separately; these only bound memory.
const LEDGER_MAX_FILES: usize = 128;
const LEDGER_MAX_SEARCHES: usize = 32;
const LEDGER_MAX_WRITES: usize = 64;
/// Longest per-file note kept (chars): 1–3 lines of findings.
const LEDGER_NOTE_MAX_CHARS: usize = 240;
/// Per-file remembered range hashes (oldest evicted first).
const LEDGER_MAX_RANGE_HASHES: usize = 32;
/// Per-file remembered symbols (digest), by line.
const LEDGER_MAX_SYMBOLS: usize = 64;
/// Longest rendered symbol digest per file line (chars).
const LEDGER_SYMBOLS_MAX_CHARS: usize = 360;
/// Remembered processed-result fingerprints (bounded FIFO).
const LEDGER_SEEN_CAP: usize = 4096;

/// JSON key that marks a `file_read` result answered with an "unchanged
/// since turn N" note instead of the content (see
/// `Agent::unchanged_reread_note`).
pub(crate) const UNCHANGED_REREAD_NOTE_KEY: &str = "unchanged_since_turn";

/// Token cap for the rendered ledger at a given context budget: 1/16 of the
/// window, between 150 and 2,000 tokens (24k window -> 1,500).
pub(crate) fn work_ledger_token_cap(max_context_tokens: usize) -> usize {
    (max_context_tokens / 16).clamp(150, 2_000)
}

/// Where a per-file note came from — rendered, so the model knows whether it
/// is reading its own words or a summarizer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerNoteSource {
    /// The model's own text in a later assistant message naming the file.
    ModelNote,
    /// A per-file line of a compaction summary.
    Summary,
}

#[derive(Debug, Clone)]
pub struct LedgerFileEntry {
    pub path: String,
    /// A read without `line_range` returned the whole file.
    pub whole_file: bool,
    /// Merged, sorted inclusive line ranges seen via `line_range` reads.
    pub ranges: Vec<(usize, usize)>,
    pub total_lines: Option<usize>,
    pub reads: u32,
    pub last_read_turn: usize,
    /// FNV-1a 64 (16 hex chars) of the `content` the LATEST read returned:
    /// the whole file for a whole-file read, only the requested slice for a
    /// `line_range` read. It identifies that result, not the file version —
    /// two different ranges hash differently without the file changing.
    /// Change detection therefore compares only reads of the SAME exact
    /// range (see `read_hashes`), never a range hash against a whole-file one.
    pub content_hash: String,
    /// The latest read's result was not parseable in full (truncated in
    /// context): the model saw only part of it.
    pub partial: bool,
    pub note: Option<(LedgerNoteSource, usize, String)>,
    /// Turn of the latest successful write/edit to this path that no read
    /// has covered in full since. Cleared only when a reread covers the
    /// whole new version (a whole-file read, or ranges spanning every line).
    pub modified_turn: Option<usize>,
    /// With `modified_turn` set: `true` once a partial reread has started
    /// fresh coverage of the new version (`ranges` then lists only lines seen
    /// AFTER the edit); `false` while the recorded coverage and note still
    /// describe the pre-edit version.
    pub reread_since_modified: bool,
    /// Content hash per exact read range (`None` = whole file) for the
    /// version the current coverage describes; bounded.
    read_hashes: Vec<(Option<(usize, usize)>, String)>,
    /// Digest of what was read: key definitions with their line numbers,
    /// recorded from the full result BEFORE any trim or compaction can drop
    /// it (merged across reads of the same version; bounded). Rendered for
    /// files whose content is no longer in context.
    pub symbols: Vec<(usize, String)>,
    seq: u64,
}

#[derive(Debug, Clone)]
pub struct LedgerSearch {
    pub pattern: String,
    pub path: String,
    pub matches: Option<u64>,
    pub turn: usize,
    seq: u64,
}

#[derive(Debug, Clone)]
pub struct LedgerWrite {
    pub path: String,
    pub tool: String,
    pub count: u32,
    pub last_turn: usize,
    seq: u64,
}

#[derive(Debug, Clone, Default)]
pub struct WorkLedger {
    turn: usize,
    task_key: Option<u64>,
    files: Vec<LedgerFileEntry>,
    searches: Vec<LedgerSearch>,
    writes: Vec<LedgerWrite>,
    seen: HashSet<u64>,
    seen_order: VecDeque<u64>,
    seq: u64,
}

/// The ledger's content hash (FNV-1a 64) of one read's `content`, shared
/// with the result-compaction stubs so a stub's `content_hash` matches the
/// ledger line.
pub(crate) fn content_fingerprint(content: &str) -> u64 {
    fnv1a64(&[content])
}

/// Deterministic FNV-1a 64 (stable across runs, unlike `DefaultHasher`).
fn fnv1a64(parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.as_bytes().iter().chain(std::iter::once(&0xffu8)) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn arg_path(args: &serde_json::Value) -> Option<String> {
    ["path", "file_path", "file", "filepath"]
        .iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
        .map(str::to_string)
}

fn truncate_note(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= LEDGER_NOTE_MAX_CHARS {
        collapsed
    } else {
        let cut: String = collapsed.chars().take(LEDGER_NOTE_MAX_CHARS).collect();
        format!("{cut}…")
    }
}

/// Merge `(start, end)` into a sorted, non-overlapping range list.
fn merge_range(ranges: &mut Vec<(usize, usize)>, range: (usize, usize)) {
    let range = if range.0 <= range.1 {
        range
    } else {
        (range.1, range.0)
    };
    ranges.push(range);
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges.drain(..) {
        match merged.last_mut() {
            Some(last) if s <= last.1.saturating_add(1) => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    *ranges = merged;
}

/// Payload of a SUCCESSFUL tool result, or `None` for a failure. `xml` is the
/// text tool-calling envelope (`<tool_result>…</tool_result>`, escaped).
fn successful_payload(text: &str, xml: bool) -> Option<String> {
    if xml {
        let start = text.find("<tool_result>")? + "<tool_result>".len();
        let rest = &text[start..];
        let end = rest.rfind("</tool_result>").unwrap_or(rest.len());
        let inner = rest[..end].trim();
        if inner.starts_with("<error>") {
            return None;
        }
        Some(
            inner
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        )
    } else {
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(text)
        {
            if map.contains_key("error") && map.len() == 1 {
                return None;
            }
        }
        Some(text.to_string())
    }
}

/// A sentence of the model's own text that states something about the file:
/// it names the file, is not an intention to (re-)read it, and carries no
/// tool markup.
fn note_for_path(text: &str, path: &str) -> Option<String> {
    let base = std::path::Path::new(path)
        .file_name()
        .map(|b| b.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());
    if base.len() < 3 {
        return None;
    }
    const INTENTIONS: &[&str] = &[
        "let me",
        "i'll",
        "i will",
        "i need to",
        "i should",
        "i'm going to",
        "next",
        "now ",
        "first",
        "read ",
        "reading",
        "re-read",
    ];
    for line in text.lines() {
        for sentence in line.split_inclusive(". ") {
            let s = sentence.trim();
            if !s.contains(base.as_str()) || s.chars().count() < 20 {
                continue;
            }
            let lower = s.trim_start_matches(['-', '*', ' ']).to_ascii_lowercase();
            if INTENTIONS.iter().any(|p| lower.starts_with(p))
                || s.contains('<')
                || s.contains("{\"")
            {
                continue;
            }
            return Some(truncate_note(s));
        }
    }
    None
}

impl WorkLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advance the turn counter (one per model request). Resets the ledger
    /// when the active task changed (interactive sessions run several tasks
    /// through one agent; the ledger then re-learns from whatever history is
    /// still present).
    pub fn begin_turn(&mut self, task: Option<&str>) {
        let key = task.map(|t| fnv1a64(&[t]));
        if key.is_some() && self.task_key.is_some() && key != self.task_key {
            *self = Self::default();
        }
        if key.is_some() {
            self.task_key = key;
        }
        self.turn += 1;
    }

    pub fn turn(&self) -> usize {
        self.turn
    }

    pub fn files(&self) -> &[LedgerFileEntry] {
        &self.files
    }

    pub fn searches(&self) -> &[LedgerSearch] {
        &self.searches
    }

    pub fn writes(&self) -> &[LedgerWrite] {
        &self.writes
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.searches.is_empty() && self.writes.is_empty()
    }

    fn mark_seen(&mut self, fp: u64) -> bool {
        if !self.seen.insert(fp) {
            return false;
        }
        self.seen_order.push_back(fp);
        while self.seen_order.len() > LEDGER_SEEN_CAP {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen.remove(&old);
            }
        }
        true
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// The ledger key of a path: `canonical_workspace_path`.
    pub(crate) fn normalize_path(path: &str, root: Option<&std::path::Path>) -> String {
        canonical_workspace_path(path, root)
    }

    /// Record every not-yet-seen successful tool result in `messages` and
    /// every new note source (assistant text naming a read file, compaction
    /// summary lines). Idempotent: re-observing the same history adds nothing.
    pub fn observe(&mut self, messages: &[Message], root: Option<&std::path::Path>) {
        use std::collections::HashMap;
        let mut native_calls: HashMap<String, (String, String)> = HashMap::new();
        let mut xml_calls: VecDeque<(String, String, u64)> = VecDeque::new();

        for message in messages {
            let text = message.content.text();
            match message.role.as_str() {
                "assistant" => {
                    let reasoning = message.reasoning_content.as_deref().unwrap_or_default();
                    let msg_fp = fnv1a64(&["assistant", text, reasoning]);
                    if self.mark_seen(msg_fp) {
                        self.absorb_model_notes(&format!("{text}\n{reasoning}"));
                    }
                    match message.tool_calls.as_deref() {
                        Some(calls) if !calls.is_empty() => {
                            xml_calls.clear();
                            for call in calls {
                                native_calls.insert(
                                    call.id.clone(),
                                    (call.function.name.clone(), call.function.arguments.clone()),
                                );
                            }
                        }
                        _ => {
                            xml_calls = if text.contains('<') {
                                crate::api::tool_calling::extract_tool_calls_from_text(text)
                                    .into_iter()
                                    .map(|c| (c.function.name, c.function.arguments, msg_fp))
                                    .collect()
                            } else {
                                VecDeque::new()
                            };
                        }
                    }
                }
                "tool" => {
                    let Some(id) = message.tool_call_id.as_deref() else {
                        continue;
                    };
                    let Some((name, args)) = native_calls.get(id).cloned() else {
                        continue;
                    };
                    if !self.mark_seen(fnv1a64(&["tool", id, &name, &args, text])) {
                        continue;
                    }
                    if let Some(payload) = successful_payload(text, false) {
                        self.record_result(&name, &args, &payload, root);
                    }
                }
                "user" => {
                    if text.contains("<tool_result>") {
                        let Some((name, args, owner)) = xml_calls.pop_front() else {
                            continue;
                        };
                        let owner = format!("{owner:x}");
                        if !self.mark_seen(fnv1a64(&["xml", &owner, &name, &args, text])) {
                            continue;
                        }
                        if let Some(payload) = successful_payload(text, true) {
                            self.record_result(&name, &args, &payload, root);
                        }
                    } else if (text.contains("[CONTEXT SUMMARY")
                        || text.contains("[AUTO-COMPACT SUMMARY")
                        || text.contains("[STRUCTURED SUMMARY"))
                        && self.mark_seen(fnv1a64(&["summary", text]))
                    {
                        self.absorb_summary(text);
                    }
                }
                _ => {}
            }
        }
    }

    fn record_result(
        &mut self,
        name: &str,
        args: &str,
        payload: &str,
        root: Option<&std::path::Path>,
    ) {
        // A result compacted in place (stub / truncated head) is not a new
        // read: the full result was recorded before it was compacted.
        if super::result_compaction::is_compacted_payload(payload) {
            return;
        }
        let args: serde_json::Value = serde_json::from_str(args).unwrap_or_default();
        match name {
            "file_read" => {
                let Some(path) = arg_path(&args) else {
                    return;
                };
                let path = Self::normalize_path(&path, root);
                let range = args
                    .get("line_range")
                    .and_then(|r| r.as_array())
                    .and_then(|r| Some((r.first()?.as_u64()?, r.get(1)?.as_u64()?)))
                    .map(|(a, b)| (a as usize, b as usize));
                let parsed = serde_json::from_str::<serde_json::Value>(payload).ok();
                // A whole read delivered as its first chunk covers only the
                // lines it showed.
                let range = range.or_else(|| {
                    let v = parsed.as_ref()?;
                    v.get(super::result_compaction::CHUNKED_WHOLE_READ_KEY)?;
                    let r = v.get("shown_line_range")?.as_array()?;
                    Some((r.first()?.as_u64()? as usize, r.get(1)?.as_u64()? as usize))
                });
                // An "unchanged re-read" note (tool_dispatch) carries no
                // content: the earlier full read is already recorded, and the
                // note must not turn it into a partial read.
                if parsed
                    .as_ref()
                    .is_some_and(|v| v.get(UNCHANGED_REREAD_NOTE_KEY).is_some())
                {
                    return;
                }
                // Hash the file text, not file_read's line-number prefixes:
                // the same range read raw and numbered is the same version.
                let content = parsed
                    .as_ref()
                    .and_then(crate::tools::line_numbers::raw_file_read_content);
                let content = content.as_deref();
                let total_lines = parsed
                    .as_ref()
                    .and_then(|v| v.get("total_lines"))
                    .and_then(|t| t.as_u64())
                    .map(|t| t as usize);
                let (hash, partial) = match content {
                    Some(c) => (fnv1a64(&[c]), false),
                    None => (fnv1a64(&[payload]), true),
                };
                let symbols = content
                    .map(|c| {
                        super::result_compaction::symbol_digest(c, range.map_or(1, |r| r.0.max(1)))
                    })
                    .unwrap_or_default();
                self.record_file_read(path, range, total_lines, hash, partial, symbols);
            }
            "grep_search" => {
                let pattern = args
                    .get("pattern")
                    .and_then(|p| p.as_str())
                    .unwrap_or_default()
                    .to_string();
                if pattern.is_empty() {
                    return;
                }
                let path = Self::normalize_path(
                    args.get("path").and_then(|p| p.as_str()).unwrap_or("."),
                    root,
                );
                let matches = serde_json::from_str::<serde_json::Value>(payload)
                    .ok()
                    .and_then(|v| {
                        v.get("total_matches")
                            .or_else(|| v.get("count"))
                            .and_then(|n| n.as_u64())
                    });
                let seq = self.next_seq();
                let turn = self.turn;
                self.searches
                    .retain(|s| !(s.pattern == pattern && s.path == path));
                self.searches.push(LedgerSearch {
                    pattern,
                    path,
                    matches,
                    turn,
                    seq,
                });
                if self.searches.len() > LEDGER_MAX_SEARCHES {
                    self.searches.remove(0);
                }
            }
            // Every file-mutating tool, with the dispatcher's own path
            // extraction: `file_multi_edit` names its paths inside `edits`
            // and `patch_apply` inside the diff — a top-level `path` lookup
            // missed both, so their edits never invalidated read coverage.
            "file_write" | "file_edit" | "file_multi_edit" | "file_delete" | "file_fim_edit"
            | "patch_apply" => {
                let mut paths: Vec<String> =
                    super::tool_dispatch::helpers::written_paths_for_tool_call(name, &args)
                        .into_iter()
                        .map(|p| p.to_string_lossy().into_owned())
                        .collect();
                if paths.is_empty() {
                    paths.extend(arg_path(&args));
                }
                let mut recorded = HashSet::new();
                for path in paths {
                    let path = Self::normalize_path(&path, root);
                    if recorded.insert(path.clone()) {
                        self.record_write(name, path);
                    }
                }
            }
            _ => {}
        }
    }

    fn record_write(&mut self, name: &str, path: String) {
        let turn = self.turn;
        let seq = self.next_seq();
        if let Some(entry) = self.files.iter_mut().find(|f| f.path == path) {
            // The recorded coverage (including any post-edit partial
            // coverage) now describes an older version.
            entry.modified_turn = Some(turn);
            entry.reread_since_modified = false;
        }
        if let Some(w) = self.writes.iter_mut().find(|w| w.path == path) {
            w.count += 1;
            w.last_turn = turn;
            w.tool = name.to_string();
            w.seq = seq;
        } else {
            self.writes.push(LedgerWrite {
                path,
                tool: name.to_string(),
                count: 1,
                last_turn: turn,
                seq,
            });
            if self.writes.len() > LEDGER_MAX_WRITES {
                self.writes.remove(0);
            }
        }
    }

    fn record_file_read(
        &mut self,
        path: String,
        range: Option<(usize, usize)>,
        total_lines: Option<usize>,
        hash: u64,
        partial: bool,
        symbols: Vec<(usize, String)>,
    ) {
        let seq = self.next_seq();
        let turn = self.turn;
        let hash = format!("{hash:016x}");
        let range = range.map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        if let Some(entry) = self.files.iter_mut().find(|f| f.path == path) {
            // The recorded coverage belongs to another version when the SAME
            // exact range (or the whole file) now returns different content,
            // or when the file was modified and nothing was reread since.
            // Different ranges are never compared with each other, and a
            // range hash is never compared with a whole-file hash.
            let content_changed = entry
                .read_hashes
                .iter()
                .any(|(k, h)| *k == range && *h != hash);
            let edited_unseen = entry.modified_turn.is_some() && !entry.reread_since_modified;
            if content_changed || edited_unseen {
                // Fresh coverage of the new version: the old ranges,
                // whole-file flag, line count and findings are invalid.
                entry.whole_file = false;
                entry.ranges.clear();
                entry.read_hashes.clear();
                entry.total_lines = None;
                entry.note = None;
                entry.symbols.clear();
            }
            Self::merge_symbols(&mut entry.symbols, symbols);
            match range {
                Some(r) => merge_range(&mut entry.ranges, r),
                None => entry.whole_file = true,
            }
            entry.total_lines = total_lines.or(entry.total_lines);
            entry.reads += 1;
            entry.last_read_turn = turn;
            entry.content_hash = hash.clone();
            entry.partial = partial;
            entry.read_hashes.retain(|(k, _)| *k != range);
            entry.read_hashes.push((range, hash));
            if entry.read_hashes.len() > LEDGER_MAX_RANGE_HASHES {
                entry.read_hashes.remove(0);
            }
            // Ranges spanning every line of a version whose line count is
            // known cover the whole file.
            if !entry.whole_file && !partial {
                if let (Some(n), [(1, end)]) = (entry.total_lines, entry.ranges.as_slice()) {
                    if n > 0 && *end >= n {
                        entry.whole_file = true;
                    }
                }
            }
            if entry.whole_file && !partial {
                // The current version has been read in full.
                entry.modified_turn = None;
                entry.reread_since_modified = false;
            } else if entry.modified_turn.is_some() {
                entry.reread_since_modified = true;
            }
            entry.seq = seq;
        } else {
            let mut ranges = Vec::new();
            if let Some(r) = range {
                merge_range(&mut ranges, r);
            }
            self.files.push(LedgerFileEntry {
                path,
                whole_file: range.is_none(),
                ranges,
                total_lines,
                reads: 1,
                last_read_turn: turn,
                content_hash: hash.clone(),
                partial,
                note: None,
                modified_turn: None,
                reread_since_modified: false,
                read_hashes: vec![(range, hash)],
                symbols: {
                    let mut merged = Vec::new();
                    Self::merge_symbols(&mut merged, symbols);
                    merged
                },
                seq,
            });
            if self.files.len() > LEDGER_MAX_FILES {
                // Oldest activity first.
                if let Some(oldest) = self
                    .files
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, f)| f.seq)
                    .map(|(i, _)| i)
                {
                    self.files.remove(oldest);
                }
            }
        }
    }

    /// Merge `new` into `into` by line (sorted, deduplicated, bounded).
    fn merge_symbols(into: &mut Vec<(usize, String)>, new: Vec<(usize, String)>) {
        for (line, text) in new {
            if !into.iter().any(|(l, _)| *l == line) {
                into.push((line, text));
            }
        }
        into.sort_by_key(|(l, _)| *l);
        into.truncate(LEDGER_MAX_SYMBOLS);
    }

    /// The recorded finding for `path` (model note or summary line), if any.
    pub fn file_finding(&self, path: &str) -> Option<String> {
        let path = path.trim().trim_start_matches("./");
        self.files
            .iter()
            .find(|f| f.path == path || f.path.ends_with(&format!("/{path}")))
            .and_then(|f| f.note.as_ref().map(|(_, _, n)| n.clone()))
    }

    fn absorb_model_notes(&mut self, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        let turn = self.turn;
        for entry in &mut self.files {
            if let Some(note) = note_for_path(text, &entry.path) {
                entry.note = Some((LedgerNoteSource::ModelNote, turn, note));
            }
        }
    }

    /// Per-file lines of a compaction summary (`- path: finding`) attach to
    /// files the ledger KNOWS were read — a summary can never add a file.
    pub fn absorb_summary(&mut self, text: &str) {
        let turn = self.turn;
        for line in text.lines() {
            let line = line.trim();
            let Some(body) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) else {
                continue;
            };
            let Some((raw_path, finding)) = body.split_once(": ") else {
                continue;
            };
            let candidate = raw_path.trim().trim_matches(['`', '*']).trim();
            let candidate = canonical_workspace_path(candidate, None);
            let candidate = candidate.as_str();
            let finding = finding.trim();
            if candidate.is_empty() || finding.chars().count() < 8 {
                continue;
            }
            let Some(entry) = self.files.iter_mut().find(|f| {
                f.path == candidate
                    || f.path.ends_with(&format!("/{candidate}"))
                    || candidate.ends_with(&format!("/{}", f.path))
            }) else {
                continue;
            };
            // The model's own note is at least as current; a summary fills
            // an empty slot, replaces an older summary, or a model note that
            // predates the latest read.
            let replace = match &entry.note {
                None | Some((LedgerNoteSource::Summary, _, _)) => true,
                Some((LedgerNoteSource::ModelNote, t, _)) => *t < entry.last_read_turn,
            };
            if replace {
                // A summary cannot say which version of the file its finding
                // describes. While an edit has not been reread in full, it
                // may describe the pre-edit content: say so.
                let finding = match entry.modified_turn {
                    Some(t) => format!("(may predate your edit at turn {t}) {finding}"),
                    None => finding.to_string(),
                };
                entry.note = Some((LedgerNoteSource::Summary, turn, truncate_note(&finding)));
            }
        }
    }

    fn render_file_line(f: &LedgerFileEntry, presence: Option<&ContextPresence>) -> String {
        let coverage = if f.whole_file {
            match f.total_lines {
                Some(n) => format!("whole file ({n} lines)"),
                None => "whole file".to_string(),
            }
        } else {
            let ranges = f
                .ranges
                .iter()
                .map(|(a, b)| format!("{a}-{b}"))
                .collect::<Vec<_>>()
                .join(", ");
            match f.total_lines {
                Some(n) => format!("lines {ranges} of {n}"),
                None => format!("lines {ranges}"),
            }
        };
        let mut line = format!(
            "- {} — {}; read {}x, last turn {}, hash {}",
            f.path,
            coverage,
            f.reads,
            f.last_read_turn,
            &f.content_hash[..8.min(f.content_hash.len())]
        );
        if f.whole_file && !f.ranges.is_empty() {
            let ranges = f
                .ranges
                .iter()
                .map(|(a, b)| format!("{a}-{b}"))
                .collect::<Vec<_>>()
                .join(", ");
            line.push_str(&format!(" (also ranges {ranges})"));
        }
        if f.partial {
            line.push_str(" (result was truncated: partial view)");
        }
        if let Some(t) = f.modified_turn {
            if !f.reread_since_modified {
                line.push_str(&format!(
                    " [you modified it at turn {t} — the coverage and notes here describe the \
                     pre-edit version; re-read if you need the new content]"
                ));
            } else if f.ranges.is_empty() {
                line.push_str(&format!(
                    " [you modified it at turn {t}; the re-read since was truncated — parts of \
                     the current version not seen]"
                ));
            } else {
                let ranges = f
                    .ranges
                    .iter()
                    .map(|(a, b)| format!("{a}-{b}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                line.push_str(&format!(
                    " [you modified it at turn {t}; only lines {ranges} re-read since — other \
                     lines not seen in their current version]"
                ));
            }
        }
        if let Some(presence) = presence {
            let fmt_ranges = |r: &[(usize, usize)]| {
                r.iter()
                    .map(|(a, b)| format!("{a}-{b}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let in_ranges = presence.ranges(&f.path);
            if presence.whole(&f.path) {
                line.push_str(" [content in context]");
            } else if !in_ranges.is_empty() {
                line.push_str(&format!(
                    " [in context: lines {} only — other lines NOT in context]",
                    fmt_ranges(&in_ranges)
                ));
            } else {
                line.push_str(" [content NOT in context]");
            }
            if !presence.whole(&f.path) && !f.symbols.is_empty() {
                let mut digest = String::new();
                let mut shown = 0usize;
                for (n, sym) in &f.symbols {
                    let item = format!("{n}: {sym}");
                    if !digest.is_empty()
                        && digest.chars().count() + item.chars().count() + 2
                            > LEDGER_SYMBOLS_MAX_CHARS
                    {
                        break;
                    }
                    if !digest.is_empty() {
                        digest.push_str("; ");
                    }
                    digest.push_str(&item);
                    shown += 1;
                }
                let more = f.symbols.len() - shown;
                if more > 0 {
                    digest.push_str(&format!(" (+{more} more)"));
                }
                line.push_str(&format!("\n  symbols (index, not code): {digest}"));
            }
        }
        if let Some((source, _, note)) = &f.note {
            let label = match source {
                LedgerNoteSource::ModelNote => "your note",
                LedgerNoteSource::Summary => "summary",
            };
            line.push_str(&format!("\n  {label}: {note}"));
        }
        line
    }

    /// Render the ledger within `max_tokens` (measured with
    /// `estimate_content_tokens`). Deliverables first, then files newest
    /// first, then searches; the oldest entries are dropped first when the
    /// cap is hit, and the omission is stated. `None` when there is nothing
    /// to report or not even the header fits.
    pub fn render(&self, max_tokens: usize) -> Option<String> {
        self.render_with(max_tokens, None)
    }

    /// [`Self::render`] for a request whose history is `presence`: every
    /// file line says whether its content is still in context, and files
    /// whose content is gone carry their symbol digest — so the model knows
    /// which files it must re-read (a range) before citing, instead of
    /// answering from memory of dropped content.
    pub(crate) fn render_in_context(
        &self,
        max_tokens: usize,
        presence: &ContextPresence,
    ) -> Option<String> {
        self.render_with(max_tokens, Some(presence))
    }

    fn render_with(&self, max_tokens: usize, presence: Option<&ContextPresence>) -> Option<String> {
        use crate::token_count::estimate_content_tokens;
        if self.is_empty() {
            return None;
        }
        // Per-line costs approximate the joined text; verify the real
        // measure and re-render against a tighter limit when the join came
        // out larger (so the omission footer is never the part that is cut).
        let mut limit = max_tokens;
        for _ in 0..8 {
            let out = self.render_within(limit, presence)?;
            let measured = estimate_content_tokens(&out);
            if measured <= max_tokens {
                return Some(out);
            }
            limit = limit.checked_sub(measured - max_tokens + 8)?;
        }
        None
    }

    fn render_within(
        &self,
        max_tokens: usize,
        presence: Option<&ContextPresence>,
    ) -> Option<String> {
        use crate::token_count::estimate_content_tokens;
        let header = if presence.is_some() {
            format!(
                "{WORK_LEDGER_HEADER} — turn {}\n\
                 Built from your own successful tool results in this task. Each file says \
                 whether its content is still in your context. [content in context]: use it, \
                 do not read it again. [content NOT in context]: only this record survives — \
                 the symbol index gives definitions and line numbers, not code. Before quoting \
                 code or citing exact lines from such a file, re-read just the line range you \
                 need (file_read with line_range); never answer from memory of content that \
                 is not in context. Do not re-read whole files only to rebuild this record.{}",
                self.turn,
                if presence.is_some_and(|p| self.files.iter().any(|f| !p.whole(&f.path))) {
                    // Live 65,536 rerun: the model tried to hold all 14 files
                    // at once before writing (98 reads, 14 distinct). Say
                    // plainly that the window cannot, and how to proceed.
                    "\nYour context cannot hold every file at once. Work one part at a time: \
                     read what that part needs, then WRITE its findings — with exact \
                     path:line citations — in your reply right away (your own text stays in \
                     context), and move on. Do not re-read files you have already written \
                     up; do not wait until everything is in context before writing."
                } else {
                    ""
                }
            )
        } else {
            format!(
                "{WORK_LEDGER_HEADER} — turn {}\n\
                 Built from your own successful tool results in this task. Your earlier \
                 findings are summarised here; they are not the file contents. Before quoting \
                 code or citing exact lines, make sure those lines are in your context — \
                 re-read just the range you need if they are not.",
                self.turn
            )
        };
        let mut writes: Vec<&LedgerWrite> = self.writes.iter().collect();
        writes.sort_by_key(|w| std::cmp::Reverse(w.seq));
        let mut files: Vec<&LedgerFileEntry> = self.files.iter().collect();
        files.sort_by_key(|f| std::cmp::Reverse(f.seq));
        let mut searches: Vec<&LedgerSearch> = self.searches.iter().collect();
        searches.sort_by_key(|s| std::cmp::Reverse(s.seq));

        let sections: [(&str, Vec<String>); 3] = [
            (
                "Deliverables (files you wrote/edited successfully):",
                writes
                    .iter()
                    .map(|w| {
                        format!(
                            "- {} — {} x{}, last turn {}",
                            w.path, w.tool, w.count, w.last_turn
                        )
                    })
                    .collect(),
            ),
            (
                "Files already read (newest first):",
                files
                    .iter()
                    .map(|f| Self::render_file_line(f, presence))
                    .collect(),
            ),
            (
                "Searches already run:",
                searches
                    .iter()
                    .map(|s| {
                        let matches = s
                            .matches
                            .map(|m| format!("{m} matches"))
                            .unwrap_or_else(|| "results".to_string());
                        format!(
                            "- grep {:?} in {} → {}, turn {}",
                            s.pattern, s.path, matches, s.turn
                        )
                    })
                    .collect(),
            ),
        ];

        // Room kept for the omission footer.
        const FOOTER_RESERVE: usize = 16;
        let mut used = estimate_content_tokens(&header);
        if used + FOOTER_RESERVE > max_tokens {
            return None;
        }
        let mut out = header;
        let mut omitted = 0usize;
        for (title, lines) in sections {
            let title_cost = estimate_content_tokens(title) + 1;
            let mut section = String::new();
            for (i, line) in lines.iter().enumerate() {
                let cost = estimate_content_tokens(line)
                    + 1
                    + if section.is_empty() { title_cost } else { 0 };
                if used + cost + FOOTER_RESERVE > max_tokens {
                    omitted += lines.len() - i;
                    break;
                }
                if section.is_empty() {
                    section.push_str(title);
                }
                section.push('\n');
                section.push_str(line);
                used += cost;
            }
            if !section.is_empty() {
                out.push_str("\n\n");
                out.push_str(&section);
            }
        }
        if omitted > 0 {
            out.push_str(&format!(
                "\n({omitted} older ledger entr{} omitted to stay within budget)",
                if omitted == 1 { "y" } else { "ies" }
            ));
        }
        Some(out)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/context/context_test.rs"]
mod tests;
