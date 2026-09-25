//! Deadline wrap-up for wall-clock budgets, and the labelled partial result
//! a run carries when it still hits the deadline without a final answer.
//!
//! Evidence (external review 2026-09-25, live replay on llm.selfware.design):
//! two read-only reviews with a 15-minute budget both ended TIMEOUT with no
//! final report. Completed model calls took a median of 28–45 s (slowest
//! 84 s / 164 s), and the call in flight at the deadline had been reasoning
//! for over three minutes. The existing nudges track iterations, tokens or
//! fixed wall fractions (the 65% / 85% commit-mode bands) — none of them
//! knows how long THIS run's model calls take, so on a slow endpoint the
//! last nudge landed with less time left than one call needs.

use super::failure_mode::FailureKind;
use super::Agent;
use crate::api::types::Message;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

/// Multiplier on the slowest model call measured so far this run.
///
/// Why 2: after the wrap-up lands, the run needs one answer-writing call —
/// the output-heaviest turn, so budget it at the slowest call seen — and the
/// deterministic completion gates (citation check, requirements audit) can
/// bounce that answer back for one correction round, which is a second call.
/// Two slowest calls cover answer + one correction. On the live replays this
/// gives a 329 s reserve (slowest 164.4 s; wrap-up at ~571 s of 900) and a
/// 168 s reserve (slowest 83.6 s; wrap-up at ~732 s) — both runs would have
/// been told to answer while an answer still fit.
pub(crate) const WRAP_UP_LATENCY_MARGIN: u64 = 2;

/// Lower bound on the reserve, in seconds.
///
/// Why 30: before any call completes there is no measurement, and on a fast
/// endpoint (sub-second calls) 2 × slowest would leave only a second or two,
/// less than the tool execution, gates and verification between turns take.
/// 30 s is about one typical call on the slowest endpoint measured (the live
/// replays' lower median was 28 s).
pub(crate) const WRAP_UP_RESERVE_FLOOR_SECS: u64 = 30;

/// Upper bound on the reserve: at most `1 / WRAP_UP_RESERVE_CAP_DIVISOR` of
/// the wall budget.
///
/// Why half: a larger reserve would put the run into wrap-up before it has
/// spent half its time exploring. A run whose slowest call already exceeds a
/// quarter of the budget cannot fit both exploration and an answer, and
/// wrapping up at the half-way mark is the best it can do. The cap wins over
/// the floor for budgets under a minute.
pub(crate) const WRAP_UP_RESERVE_CAP_DIVISOR: u64 = 2;

/// Seconds of wall budget to hold back for the final answer, from the
/// slowest model call measured so far this run (`slowest_call_ms`, the
/// client's measured `CallLatencyStats::max_ms`; 0 when none completed).
///
/// The slowest call is used rather than a percentile: the client keeps
/// count / total / max, a run makes a few dozen calls (13–30 in the live
/// replays) so a p90 sits at or next to the max anyway, and the call that
/// matters — the final answer — is tail-shaped (output-heavy).
pub(crate) fn wrap_up_reserve_secs(slowest_call_ms: u64, max_wall_secs: u64) -> u64 {
    let measured = slowest_call_ms
        .saturating_mul(WRAP_UP_LATENCY_MARGIN)
        .div_ceil(1000);
    let cap = max_wall_secs / WRAP_UP_RESERVE_CAP_DIVISOR;
    measured.max(WRAP_UP_RESERVE_FLOOR_SECS).min(cap)
}

/// Note a completion gate's status carries when it stepped aside at the
/// deadline instead of feeding its rejection back for a correction round.
pub const CITATIONS_NOT_CORRECTED_DEADLINE: &str = "citations not corrected: deadline";

/// Why a completion-gate correction round no longer fits the wall budget,
/// or `None` when it does.
///
/// A correction round is one more model call (the model fixes the answer)
/// plus the gate re-check. It does not fit when:
/// - the remaining time is at or below the wrap-up reserve
///   ([`wrap_up_reserve_secs`], 2 × the slowest call measured this run) —
///   the same line the wrap-up directive uses, so the gate and the wrap-up
///   agree on when the run is in its last answer; or
/// - the remaining time is below ONE slowest measured call (possible when
///   the reserve is capped at half the budget).
///
/// Evidence (val083 b2_350000): the gate rejected a 4,442-char review at
/// ~360 s of 900; the correction round's re-reads plus one 231 s call left
/// 139 s against a slowest call of 232 s, and the run timed out with no
/// report.
pub(crate) fn correction_round_no_fit(
    remaining_secs: u64,
    slowest_call_ms: u64,
    max_wall_secs: u64,
) -> Option<String> {
    let reserve = wrap_up_reserve_secs(slowest_call_ms, max_wall_secs);
    if remaining_secs <= reserve {
        return Some(format!(
            "{remaining_secs}s of the wall budget left <= reserve {reserve}s"
        ));
    }
    if one_call_no_fit(remaining_secs, slowest_call_ms) {
        return Some(format!(
            "{remaining_secs}s of the wall budget left < slowest model call {}s",
            slowest_call_ms.div_ceil(1000)
        ));
    }
    None
}

/// Whether one more model call can no longer fit: a slowest call has been
/// measured and the remaining time is below it. Stricter than
/// [`correction_round_no_fit`] — used to finish WITHOUT another model call.
pub(crate) fn one_call_no_fit(remaining_secs: u64, slowest_call_ms: u64) -> bool {
    slowest_call_ms > 0 && remaining_secs.saturating_mul(1000) < slowest_call_ms
}

/// Minimum prose length (chars, tool-call markup stripped) for an assistant
/// message to count as answer text in a partial result.
///
/// Why 200: measured on the val083 replays (b2_163840, b2_350000,
/// b3_review), every "next I will read X" / "continuing" narration turn
/// carried 48–168 chars of prose, and every write-up segment 204–4,440
/// chars. 200 keeps every write-up and drops every narration line.
pub(crate) const PARTIAL_TEXT_MIN_CHARS: usize = 200;

/// The prose of an assistant message: think blocks stripped, executed tool
/// calls removed, and any remaining line that still carries tool-call
/// markup outside inline code dropped. Never returns tool-call markup.
pub(crate) fn answer_prose(content: &str) -> String {
    let text = super::recovery::strip_think_blocks(content);
    let text = crate::tool_parser::parse_tool_calls(&text).text_content;
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| !carries_tool_markup(line))
        .collect();
    kept.join("\n").trim().to_string()
}

/// Tool-call markup on a line, ignoring `inline code` spans (a review that
/// quotes `` `<tool>` `` is prose).
fn carries_tool_markup(line: &str) -> bool {
    let outside: String = line.split('`').step_by(2).collect::<Vec<_>>().concat();
    const MARKERS: &[&str] = &[
        "<tool>",
        "</tool>",
        "<tool_call",
        "</tool_call",
        "<function",
        "</function",
        "<arguments",
        "</arguments",
        "<parameter",
        "</parameter",
        "<name>",
        "</name>",
        "<|open|>",
        "<|close|>",
    ];
    MARKERS.iter().any(|m| outside.contains(m))
}

/// Label carried by the partial result of a read-only (review/report) run.
pub const PARTIAL_REVIEW_LABEL: &str = "PARTIAL — NOT A COMPLETED REVIEW";
/// Label carried by the partial result of any other run.
pub const PARTIAL_TASK_LABEL: &str = "PARTIAL — NOT A COMPLETED TASK";

/// Bound on the last assistant text carried in a partial result.
const PARTIAL_TEXT_MAX_CHARS: usize = 16_000;
/// Token cap for the work ledger rendered into a partial result.
const PARTIAL_LEDGER_TOKEN_CAP: usize = 4_000;

/// Best progress of a run that hit its wall-clock deadline WITHOUT a final
/// answer. The run stays a failure (TIMEOUT, non-zero exit); this is only
/// the evidence it gathered, labelled so no consumer mistakes it for a
/// completed answer (AGENTS.md rule 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialProgress {
    /// [`PARTIAL_REVIEW_LABEL`] or [`PARTIAL_TASK_LABEL`].
    pub label: String,
    /// Why the run stopped (the terminal error, e.g. `Wall-clock timeout:
    /// 905s >= 900s`).
    pub reason: String,
    /// The most recent substantive answer text (prose only, tool-call markup
    /// stripped, bounded): the draft the completion gate last rejected when
    /// there is one, else the write-up segments the model produced (each at
    /// least [`PARTIAL_TEXT_MIN_CHARS`]), oldest first. Unfinished work, not
    /// an accepted answer. `None` when no such text exists (the ledger then
    /// stands alone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_assistant_text: Option<String>,
    /// The work ledger: files read (with ranges and notes), searches run,
    /// files written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_ledger: Option<String>,
}

impl PartialProgress {
    /// Human-readable block for stdout (text mode).
    pub fn render(&self) -> String {
        let mut out = format!(
            "==== {} ====\nThe run stopped before a final answer ({}). What follows is \
             unfinished progress, not a result.\n",
            self.label, self.reason
        );
        if let Some(ledger) = &self.work_ledger {
            out.push_str("\n-- work ledger --\n");
            out.push_str(ledger.trim_end());
            out.push('\n');
        }
        if let Some(text) = &self.last_assistant_text {
            out.push_str("\n-- last assistant text (intermediate) --\n");
            out.push_str(text.trim_end());
            out.push('\n');
        }
        if self.work_ledger.is_none() && self.last_assistant_text.is_none() {
            out.push_str("(no progress was recorded)\n");
        }
        out.push_str(&format!("==== end {} ====", self.label));
        out
    }
}

impl Agent {
    /// One-time deadline wrap-up: when the remaining wall budget drops to
    /// the reserve measured from this run's own model-call latency (see
    /// [`wrap_up_reserve_secs`]), tell the model to stop exploring and write
    /// the final answer now, labelling unfinished areas. No-op without a
    /// wall budget. A call already in flight when the reserve is crossed is
    /// left alone; the directive lands on the next turn. Latch reset in
    /// `run_task`.
    pub(super) fn maybe_inject_deadline_wrap_up(&mut self) {
        let Some(max_wall) = self.config.agent.max_wall_secs.filter(|&s| s > 0) else {
            return;
        };
        if self.deadline_wrap_up_fired.load(Ordering::Relaxed) {
            return;
        }
        let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
        let stats = self.client.call_latency_stats();
        let reserve = wrap_up_reserve_secs(stats.max_ms, max_wall);
        if remaining > reserve {
            return;
        }
        self.deadline_wrap_up_fired.store(true, Ordering::Relaxed);
        let measured = if stats.call_count > 0 {
            format!(
                "the slowest model call in this run took {}s",
                stats.max_ms.div_ceil(1000)
            )
        } else {
            "no model call has been timed yet".to_string()
        };
        tracing::info!(
            remaining_secs = remaining,
            reserve_secs = reserve,
            slowest_call_ms = stats.max_ms,
            "deadline wrap-up directive injected"
        );
        self.messages.push(Message::user(format!(
            "<selfware_system_directive>\n\
             DEADLINE WRAP-UP: about {remaining}s of the wall-clock budget remain and \
             {measured} — there is time for roughly one more answer. Stop exploring: do not \
             start new reads, searches or approaches. Write your FINAL ANSWER NOW from what \
             you already have. Label every area you did not finish as UNFINISHED (not \
             checked), and do not present unchecked areas as reviewed.\n\
             </selfware_system_directive>"
        )));
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: "deadline_wrap_up".to_string(),
            detail: format!(
                "{remaining}s left <= reserve {reserve}s ({}x slowest call {}ms, floor {}s, cap 1/{} of {}s)",
                WRAP_UP_LATENCY_MARGIN,
                stats.max_ms,
                WRAP_UP_RESERVE_FLOOR_SECS,
                WRAP_UP_RESERVE_CAP_DIVISOR,
                max_wall
            ),
        });
    }

    /// Why a completion-gate correction round no longer fits this run's
    /// wall budget (see [`correction_round_no_fit`]); `None` without a wall
    /// budget or while a round still fits.
    pub(super) fn completion_gate_deadline_step_aside(&self) -> Option<String> {
        let max_wall = self.config.agent.max_wall_secs.filter(|&s| s > 0)?;
        let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
        correction_round_no_fit(remaining, self.client.call_latency_stats().max_ms, max_wall)
    }

    /// Deadline acceptance of a citation-rejected draft: when a correction
    /// round is still pending and not even one more model call fits
    /// ([`one_call_no_fit`]), take the draft the gate rejected as the final
    /// answer instead of starting a call the deadline will cut off. The
    /// grounding status keeps the wrong counts and problems and gains the
    /// "not corrected: deadline" note, so the banner is ⚠️ and the outcome
    /// is not green (rule 3). Never accepts a draft judged before a later
    /// edit. Returns the accepted text.
    pub(super) fn take_rejected_draft_at_deadline(&mut self) -> Option<String> {
        let max_wall = self.config.agent.max_wall_secs.filter(|&s| s > 0)?;
        let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
        let slowest_ms = self.client.call_latency_stats().max_ms;
        if !one_call_no_fit(remaining, slowest_ms) {
            return None;
        }
        let draft = {
            let mut state = self.citation_gate.lock().unwrap_or_else(|e| e.into_inner());
            let fresh = state.rejected_draft.as_ref().is_some_and(|d| {
                d.mutation_sequence == self.mutation_sequence && !d.text.trim().is_empty()
            });
            if !fresh {
                return None;
            }
            let mut draft = state.rejected_draft.take()?;
            draft.status.not_corrected_deadline = true;
            state.status = Some(draft.status.clone());
            draft
        };
        let text = draft.text.trim().to_string();
        tracing::warn!(
            remaining_secs = remaining,
            slowest_call_ms = slowest_ms,
            "deadline: accepting the citation-rejected draft — one more model call does not fit"
        );
        crate::output::citation_check(&format!(
            "{} of {} wrong — {CITATIONS_NOT_CORRECTED_DEADLINE} ({remaining}s left < slowest call \
             {}s); completing with this warning",
            draft.status.problem_count(),
            draft.status.total,
            slowest_ms.div_ceil(1000)
        ));
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: "deadline_accept_draft".to_string(),
            detail: format!(
                "{remaining}s left < slowest call {slowest_ms}ms: accepting the draft the citation \
                 gate rejected ({}); {CITATIONS_NOT_CORRECTED_DEADLINE}",
                draft.status.grounding_line()
            ),
        });
        self.last_assistant_response = text.clone();
        Some(text)
    }

    /// Answer text for a timeout partial (see
    /// [`PartialProgress::last_assistant_text`]): the draft the citation
    /// gate last rejected, else every assistant write-up segment of at least
    /// [`PARTIAL_TEXT_MIN_CHARS`] prose chars, oldest first — reviews told
    /// to write up part by part spread the answer over several turns, and
    /// the newest segment alone would drop the earlier parts. Bounded to
    /// [`PARTIAL_TEXT_MAX_CHARS`], keeping the newest text.
    fn partial_answer_text(&self) -> Option<String> {
        let gate_draft = self
            .citation_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rejected_draft
            .as_ref()
            .map(|d| answer_prose(&d.text))
            .filter(|t| t.chars().count() >= PARTIAL_TEXT_MIN_CHARS);
        let text = gate_draft.or_else(|| {
            let segments: Vec<String> = self
                .messages
                .iter()
                .filter(|m| m.role == "assistant")
                .map(|m| answer_prose(&m.content.text_all()))
                .filter(|t| t.chars().count() >= PARTIAL_TEXT_MIN_CHARS)
                .collect();
            (!segments.is_empty()).then(|| segments.join("\n\n"))
        })?;
        let total = text.chars().count();
        if total <= PARTIAL_TEXT_MAX_CHARS {
            return Some(text);
        }
        let tail: String = text.chars().skip(total - PARTIAL_TEXT_MAX_CHARS).collect();
        Some(format!("[…earlier text truncated]\n{tail}"))
    }

    /// The labelled partial progress of a run that ended in a wall-clock
    /// TIMEOUT, a CALL_TIME_CAP abort or a token/cost BUDGET_EXHAUSTED stop
    /// without a final answer; `None` for every other outcome.
    /// Never turns the failure into a success: callers attach it next to the
    /// unchanged failure result and exit status.
    pub fn partial_progress(&self, run_result: &anyhow::Result<()>) -> Option<PartialProgress> {
        let Err(err) = run_result else {
            return None;
        };
        // The wall-clock deadline, a single call aborted at the per-call cap,
        // or the token/cost budget: each ends the run mid-work with no
        // answer. (Replay of val083 b2_163840 on the fixed build hit the 3M
        // token cap at 678 s with areas 1–6 written up and carried nothing.)
        let stopped_mid_work = self.last_run_failure_mode().is_some_and(|fm| {
            matches!(
                fm.kind,
                FailureKind::Timeout | FailureKind::CallTimeCap | FailureKind::BudgetExhausted
            )
        });
        if !stopped_mid_work {
            return None;
        }
        let last_assistant_text = self.partial_answer_text();
        // Record whatever the history still holds before rendering (the
        // ledger is otherwise only fed ahead of trims/compactions).
        self.compressor.observe_work(&self.messages);
        let work_ledger = self
            .compressor
            .render_work_ledger(PARTIAL_LEDGER_TOKEN_CAP)
            .filter(|l| !l.trim().is_empty());
        let label = if self.task_is_read_only {
            PARTIAL_REVIEW_LABEL
        } else {
            PARTIAL_TASK_LABEL
        };
        Some(PartialProgress {
            label: label.to_string(),
            reason: crate::observability::telemetry::redact_secrets(&err.to_string()),
            last_assistant_text,
            work_ledger,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/deadline/deadline_test.rs"]
mod tests;
