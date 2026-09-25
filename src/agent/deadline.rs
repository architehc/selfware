//! Wrap-up for the wall-clock, token and cost budgets (one latch, whichever
//! limit's measured reserve is reached first), the completion-gate
//! step-aside inside those reserves, and the labelled partial result a run
//! carries when it still hits a limit without a final answer.
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

/// Which limit put the run into its last answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapUpCause {
    /// The wall-clock budget (`max_wall_secs`).
    Deadline,
    /// The token budget (`max_budget_tokens`).
    TokenBudget,
    /// The cost budget (`max_cost_usd`).
    CostBudget,
}

impl WrapUpCause {
    /// Short name for logs and progress events.
    pub fn label(self) -> &'static str {
        match self {
            WrapUpCause::Deadline => "deadline",
            WrapUpCause::TokenBudget => "token_budget",
            WrapUpCause::CostBudget => "cost_budget",
        }
    }

    /// The word the "citations not corrected: …" note uses.
    pub fn note_word(self) -> &'static str {
        match self {
            WrapUpCause::Deadline => "deadline",
            WrapUpCause::TokenBudget | WrapUpCause::CostBudget => "budget",
        }
    }
}

/// `citations not corrected: deadline` / `citations not corrected: budget`
/// — the note a completion gate's status carries when it stepped aside for
/// a limit instead of feeding its rejection back for a correction round.
pub fn citations_not_corrected_note(cause: WrapUpCause) -> String {
    format!("citations not corrected: {}", cause.note_word())
}

/// A limit inside whose reserve a completion gate steps aside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepAside {
    pub cause: WrapUpCause,
    /// Measured numbers, e.g. `139s of the wall budget left <= reserve 450s`.
    pub detail: String,
}

impl std::fmt::Display for StepAside {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.cause.note_word(), self.detail)
    }
}

/// Per-task wrap-up latch and the per-turn usage it is measured from.
#[derive(Debug, Default)]
pub(crate) struct WrapUpState {
    /// The ONE wrap-up of this task and the limit that triggered it; the
    /// deadline and the budgets share it so they never both inject.
    pub issued: Option<WrapUpCause>,
    /// (accounted total tokens, cost) at the previous loop turn; `None`
    /// until the first observation (the baseline).
    pub last_seen: Option<(usize, f64)>,
    /// Largest token use of one loop turn this run (every call of the turn:
    /// the whole prompt is re-sent each call, plus the completion).
    pub max_turn_tokens: usize,
    /// Largest cost of one loop turn this run (0 while no cost is reported).
    pub max_turn_cost: f64,
}

/// Multiplier on the largest measured turn for the budget reserve — same
/// reasoning as [`WRAP_UP_LATENCY_MARGIN`]: one answer-writing turn plus one
/// correction round a completion gate can bounce it into.
pub(crate) const BUDGET_WRAP_UP_TURN_MARGIN: usize = 2;

/// Lower bound on the token reserve.
///
/// Why 40,000: before a turn completes there is no measurement, and every
/// turn re-sends at least the system prompt, tool schemas and task. That
/// base measured 20,638 prompt tokens on the first request of val083
/// b2_350000 (the `llm_request_sent` event), so no turn costs less than
/// ~20k; two turns (answer + correction) is ~40k.
pub(crate) const TOKEN_WRAP_UP_RESERVE_FLOOR: usize = 40_000;

/// Token reserve: `2 × largest turn`, at least the floor, at most half the
/// token budget (same cap reasoning as [`WRAP_UP_RESERVE_CAP_DIVISOR`]: a
/// larger reserve would wrap up before half the budget was spent exploring).
pub(crate) fn token_wrap_up_reserve(max_turn_tokens: usize, max_budget_tokens: usize) -> usize {
    max_turn_tokens
        .saturating_mul(BUDGET_WRAP_UP_TURN_MARGIN)
        .max(TOKEN_WRAP_UP_RESERVE_FLOOR)
        .min(max_budget_tokens / WRAP_UP_RESERVE_CAP_DIVISOR as usize)
}

/// Cost reserve: `2 × most expensive turn`, at most half the cost budget.
/// No floor: endpoints that report no cost give nothing to measure, and a
/// guessed price would be an estimate (rule 4) — with no measured cost the
/// cost reserve is 0 and only the token/wall reserves apply.
pub(crate) fn cost_wrap_up_reserve(max_turn_cost: f64, max_cost_usd: f64) -> f64 {
    (max_turn_cost * BUDGET_WRAP_UP_TURN_MARGIN as f64)
        .min(max_cost_usd / WRAP_UP_RESERVE_CAP_DIVISOR as f64)
}

/// Why a correction round no longer fits the token budget: tokens left at
/// or below the reserve, or below one largest turn.
pub(crate) fn token_round_no_fit(
    remaining_tokens: usize,
    max_turn_tokens: usize,
    max_budget_tokens: usize,
) -> Option<String> {
    let reserve = token_wrap_up_reserve(max_turn_tokens, max_budget_tokens);
    if remaining_tokens <= reserve {
        return Some(format!(
            "{remaining_tokens} tokens of the budget left <= reserve {reserve} \
             ({BUDGET_WRAP_UP_TURN_MARGIN}x largest turn {max_turn_tokens}, floor \
             {TOKEN_WRAP_UP_RESERVE_FLOOR}, cap 1/{WRAP_UP_RESERVE_CAP_DIVISOR} of \
             {max_budget_tokens})"
        ));
    }
    if max_turn_tokens > 0 && remaining_tokens < max_turn_tokens {
        return Some(format!(
            "{remaining_tokens} tokens of the budget left < largest turn {max_turn_tokens}"
        ));
    }
    None
}

/// Why a correction round no longer fits the cost budget (see
/// [`cost_wrap_up_reserve`]); never fires without a measured turn cost.
pub(crate) fn cost_round_no_fit(
    remaining_usd: f64,
    max_turn_cost: f64,
    max_cost_usd: f64,
) -> Option<String> {
    if max_turn_cost <= 0.0 {
        return None;
    }
    let reserve = cost_wrap_up_reserve(max_turn_cost, max_cost_usd);
    if remaining_usd <= reserve || remaining_usd < max_turn_cost {
        return Some(format!(
            "${remaining_usd:.4} of the cost budget left <= reserve ${reserve:.4} \
             ({BUDGET_WRAP_UP_TURN_MARGIN}x largest turn ${max_turn_cost:.4})"
        ));
    }
    None
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
    /// least `PARTIAL_TEXT_MIN_CHARS`), oldest first. Unfinished work, not
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
    /// Fold the usage of the turn that just ended into this run's per-turn
    /// maxima ([`WrapUpState`]). Called once per loop turn, before the
    /// wrap-up checks. The first call after a reset only records the
    /// baseline (a resumed run's seeded total is not one turn).
    pub(super) fn observe_turn_usage(&self) {
        let usage = self.client.accounted_usage();
        let cost = usage.cost.unwrap_or(0.0);
        let mut state = self.wrap_up.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((last_tokens, last_cost)) = state.last_seen {
            let turn_tokens = usage.total_tokens.saturating_sub(last_tokens);
            let turn_cost = (cost - last_cost).max(0.0);
            state.max_turn_tokens = state.max_turn_tokens.max(turn_tokens);
            if turn_cost > state.max_turn_cost {
                state.max_turn_cost = turn_cost;
            }
        }
        state.last_seen = Some((usage.total_tokens, cost));
    }

    /// The limit whose reserve this run is inside, if any: the wall-clock
    /// deadline first (a timeout loses the in-flight answer outright), then
    /// the token budget, then the cost budget. `None` while every configured
    /// limit is outside its reserve.
    fn limit_in_reserve(&self) -> Option<StepAside> {
        if let Some(max_wall) = self.config.agent.max_wall_secs.filter(|&s| s > 0) {
            let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
            if let Some(detail) = correction_round_no_fit(
                remaining,
                self.client.call_latency_stats().max_ms,
                max_wall,
            ) {
                return Some(StepAside {
                    cause: WrapUpCause::Deadline,
                    detail,
                });
            }
        }
        let usage = self.client.accounted_usage();
        let (max_turn_tokens, max_turn_cost) = {
            let s = self.wrap_up.lock().unwrap_or_else(|e| e.into_inner());
            (s.max_turn_tokens, s.max_turn_cost)
        };
        if let Some(max_tokens) = self.config.agent.max_budget_tokens.filter(|&b| b > 0) {
            let remaining = max_tokens.saturating_sub(usage.total_tokens);
            if let Some(detail) = token_round_no_fit(remaining, max_turn_tokens, max_tokens) {
                return Some(StepAside {
                    cause: WrapUpCause::TokenBudget,
                    detail,
                });
            }
        }
        if let Some(max_cost) = self.config.agent.max_cost_usd.filter(|&c| c > 0.0) {
            let remaining = (max_cost - usage.cost.unwrap_or(0.0)).max(0.0);
            if let Some(detail) = cost_round_no_fit(remaining, max_turn_cost, max_cost) {
                return Some(StepAside {
                    cause: WrapUpCause::CostBudget,
                    detail,
                });
            }
        }
        None
    }

    /// One-time wrap-up, whichever limit comes first: when the remaining
    /// wall time drops to the reserve measured from this run's call latency
    /// ([`wrap_up_reserve_secs`]), or the remaining token / cost budget to
    /// the reserve measured from this run's per-turn usage
    /// ([`token_wrap_up_reserve`], [`cost_wrap_up_reserve`]), tell the model
    /// to stop exploring and write the final answer now, labelling
    /// unfinished areas. ONE latch for all limits (reason recorded), so the
    /// deadline and the budget can never both inject. A call already in
    /// flight is left alone; the directive lands on the next turn. Latch
    /// reset in `run_task`.
    pub(super) fn maybe_inject_wrap_up(&mut self) {
        self.observe_turn_usage();
        if self.wrap_up_issued().is_some() {
            return;
        }
        let Some(window) = self.limit_in_reserve() else {
            return;
        };
        self.wrap_up
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .issued = Some(window.cause);
        let (headline, room) = match window.cause {
            WrapUpCause::Deadline => {
                let stats = self.client.call_latency_stats();
                let max_wall = self.config.agent.max_wall_secs.unwrap_or(0);
                let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
                let measured = if stats.call_count > 0 {
                    format!(
                        "the slowest model call in this run took {}s",
                        stats.max_ms.div_ceil(1000)
                    )
                } else {
                    "no model call has been timed yet".to_string()
                };
                (
                    "DEADLINE WRAP-UP",
                    format!(
                        "about {remaining}s of the wall-clock budget remain and {measured} — \
                         there is time for roughly one more answer"
                    ),
                )
            }
            WrapUpCause::TokenBudget | WrapUpCause::CostBudget => {
                let usage = self.client.accounted_usage();
                let s = self.wrap_up.lock().unwrap_or_else(|e| e.into_inner());
                let left = if window.cause == WrapUpCause::TokenBudget {
                    format!(
                        "about {} tokens of the token budget remain and the largest turn in this \
                         run used {} tokens",
                        self.config
                            .agent
                            .max_budget_tokens
                            .unwrap_or(0)
                            .saturating_sub(usage.total_tokens),
                        s.max_turn_tokens
                    )
                } else {
                    format!(
                        "about ${:.4} of the cost budget remain and the largest turn in this run \
                         cost ${:.4}",
                        (self.config.agent.max_cost_usd.unwrap_or(0.0) - usage.cost.unwrap_or(0.0))
                            .max(0.0),
                        s.max_turn_cost
                    )
                };
                (
                    "BUDGET WRAP-UP",
                    format!("{left} — there is room for roughly one more answer"),
                )
            }
        };
        tracing::info!(
            cause = window.cause.label(),
            detail = %window.detail,
            "wrap-up directive injected"
        );
        self.messages.push(Message::user(format!(
            "<selfware_system_directive>\n\
             {headline}: {room}. Stop exploring: do not start new reads, searches or \
             approaches. Write your FINAL ANSWER NOW from what you already have. Label every \
             area you did not finish as UNFINISHED (not checked), and do not present \
             unchecked areas as reviewed.\n\
             </selfware_system_directive>"
        )));
        let decision = match window.cause {
            WrapUpCause::Deadline => "deadline_wrap_up",
            WrapUpCause::TokenBudget | WrapUpCause::CostBudget => "budget_wrap_up",
        };
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: decision.to_string(),
            detail: window.detail,
        });
    }

    /// Which limit's wrap-up was issued this task, if any.
    pub fn wrap_up_issued(&self) -> Option<WrapUpCause> {
        self.wrap_up
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .issued
    }

    /// Why a completion-gate correction round no longer fits this run's
    /// wall, token or cost budget (see [`correction_round_no_fit`],
    /// [`token_round_no_fit`], [`cost_round_no_fit`]); `None` while a round
    /// still fits every configured limit.
    pub(super) fn completion_gate_step_aside(&self) -> Option<StepAside> {
        self.limit_in_reserve()
    }

    /// Whether not even one more turn fits any configured limit: time left
    /// below the slowest call, or tokens / cost left below the largest turn.
    fn one_turn_no_fit(&self) -> Option<(WrapUpCause, String)> {
        if let Some(max_wall) = self.config.agent.max_wall_secs.filter(|&s| s > 0) {
            let remaining = max_wall.saturating_sub(self.budget_elapsed_secs());
            let slowest_ms = self.client.call_latency_stats().max_ms;
            if one_call_no_fit(remaining, slowest_ms) {
                return Some((
                    WrapUpCause::Deadline,
                    format!(
                        "{remaining}s left < slowest call {}s",
                        slowest_ms.div_ceil(1000)
                    ),
                ));
            }
        }
        let usage = self.client.accounted_usage();
        let (max_turn_tokens, max_turn_cost) = {
            let s = self.wrap_up.lock().unwrap_or_else(|e| e.into_inner());
            (s.max_turn_tokens, s.max_turn_cost)
        };
        if let Some(max_tokens) = self.config.agent.max_budget_tokens.filter(|&b| b > 0) {
            let remaining = max_tokens.saturating_sub(usage.total_tokens);
            if max_turn_tokens > 0 && remaining < max_turn_tokens {
                return Some((
                    WrapUpCause::TokenBudget,
                    format!("{remaining} tokens left < largest turn {max_turn_tokens} tokens"),
                ));
            }
        }
        if let Some(max_cost) = self.config.agent.max_cost_usd.filter(|&c| c > 0.0) {
            let remaining = (max_cost - usage.cost.unwrap_or(0.0)).max(0.0);
            if max_turn_cost > 0.0 && remaining < max_turn_cost {
                return Some((
                    WrapUpCause::CostBudget,
                    format!("${remaining:.4} left < largest turn ${max_turn_cost:.4}"),
                ));
            }
        }
        None
    }

    /// Limit acceptance of a citation-rejected draft: when a correction
    /// round is still pending and not even one more turn fits the deadline
    /// or the token / cost budget, take the draft the gate rejected as the
    /// final answer instead of starting a turn the limit will cut off. The
    /// grounding status keeps the wrong counts and problems and gains the
    /// "citations not corrected: deadline|budget" note, so the banner is ⚠️
    /// and the outcome is not green (rule 3). Never accepts a draft judged
    /// before a later edit. Returns the accepted text.
    pub(super) fn take_rejected_draft_at_limit(&mut self) -> Option<String> {
        let (cause, why) = self.one_turn_no_fit()?;
        let draft = {
            let mut state = self.citation_gate.lock().unwrap_or_else(|e| e.into_inner());
            let fresh = state.rejected_draft.as_ref().is_some_and(|d| {
                d.mutation_sequence == self.mutation_sequence && !d.text.trim().is_empty()
            });
            if !fresh {
                return None;
            }
            let mut draft = state.rejected_draft.take()?;
            draft.status.not_corrected = Some(cause.note_word().to_string());
            state.status = Some(draft.status.clone());
            draft
        };
        let text = draft.text.trim().to_string();
        let note = citations_not_corrected_note(cause);
        tracing::warn!(
            cause = cause.label(),
            "{why}: accepting the citation-rejected draft — one more turn does not fit"
        );
        crate::output::citation_check(&format!(
            "{} of {} wrong — {note} ({why}); completing with this warning",
            draft.status.problem_count(),
            draft.status.total,
        ));
        self.emit_progress(super::progress::ProgressEvent::TurnDecision {
            decision: format!("{}_accept_draft", cause.note_word()),
            detail: format!(
                "{why}: accepting the draft the citation gate rejected ({}); {note}",
                draft.status.grounding_line()
            ),
        });
        self.last_assistant_response = text.clone();
        Some(text)
    }

    /// Answer text for a timeout partial (see
    /// [`PartialProgress::last_assistant_text`]): the draft the citation
    /// gate last rejected, else every assistant write-up segment of at least
    /// `PARTIAL_TEXT_MIN_CHARS` prose chars, oldest first — reviews told
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
