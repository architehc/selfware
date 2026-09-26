//! Wrap-up for the wall-clock, token and cost budgets (one latch, whichever
//! limit is reached first), the completion-gate step-aside inside those
//! windows, and the labelled partial result a run carries when it still
//! hits a limit without a final answer.
//!
//! The window is FORECAST one turn ahead from this run's measured calls
//! ([`super::call_forecast`]): the wrap-up fires when what is left after
//! the next ordinary call would no longer fit the final answer, so it can
//! never land after the last call that fits.

use super::call_forecast::CallForecast;
use super::failure_mode::FailureKind;
use super::Agent;
use crate::api::types::Message;
use serde::{Deserialize, Serialize};

/// Lower bound on the time window, in seconds.
///
/// Why 30: before any call completes the forecast has only its fallbacks,
/// and on a fast endpoint the forecast shrinks to seconds — less than the
/// tool execution, gates and verification between turns take. 30 s is
/// about one typical call on the slowest endpoint measured (the live
/// replays' lower median was 28 s).
pub(crate) const WRAP_UP_RESERVE_FLOOR_SECS: u64 = 30;

/// Upper bound on the time window: at most `NUM / DEN` of the wall budget.
///
/// Why two thirds: the measured final answer on llm.selfware.design takes
/// up to half of a 900 s budget by itself (b2_65536: 305 s; the forecast
/// for the largest measured report is ~450 s), plus the next ordinary call.
/// With the former half cap the b2_65536 window (504 s) was clipped to
/// 450 s and the wrap-up fired only after the 189 s write-up call, when
/// the answer no longer fit. Two thirds keeps a third of the budget for
/// exploration.
pub(crate) const WRAP_UP_RESERVE_CAP_NUM: u64 = 2;
/// See [`WRAP_UP_RESERVE_CAP_NUM`].
pub(crate) const WRAP_UP_RESERVE_CAP_DEN: u64 = 3;

/// Upper bound on the token/cost window: half the budget (a larger window
/// would wrap up before half the budget was spent exploring; one final
/// answer is far below half of any budget that fits a review).
pub(crate) const BUDGET_RESERVE_CAP_DIVISOR: u64 = 2;

/// Lower bound on the token window.
///
/// Why 40,000: before a call completes there is no measured prompt, and
/// every call re-sends at least the system prompt, tool schemas and task —
/// 20,638 prompt tokens on the first request of val083 b2_350000 — so the
/// next call plus an answer is at least ~40k.
pub(crate) const TOKEN_WRAP_UP_RESERVE_FLOOR: u64 = 40_000;

/// Wall seconds to hold back: the next ordinary call plus the final answer
/// (forecast), at least the floor, at most the cap.
pub(crate) fn time_window_secs(forecast: &CallForecast, max_wall_secs: u64) -> u64 {
    let cap = max_wall_secs.saturating_mul(WRAP_UP_RESERVE_CAP_NUM) / WRAP_UP_RESERVE_CAP_DEN;
    (forecast.next_call_secs() + forecast.answer_secs())
        .max(WRAP_UP_RESERVE_FLOOR_SECS)
        .min(cap)
}

/// Tokens to hold back: the next ordinary call plus the final answer
/// (forecast), at least the floor, at most half the budget.
pub(crate) fn token_window(forecast: &CallForecast, max_budget_tokens: u64) -> u64 {
    (forecast.next_call_tokens() + forecast.answer_tokens())
        .max(TOKEN_WRAP_UP_RESERVE_FLOOR)
        .min(max_budget_tokens / BUDGET_RESERVE_CAP_DIVISOR)
}

/// Why the run is inside the wall-clock window (a correction round or
/// another exploration call no longer fits before the final answer), or
/// `None`.
pub(crate) fn time_window_reached(
    remaining_secs: u64,
    forecast: &CallForecast,
    max_wall_secs: u64,
) -> Option<String> {
    let window = time_window_secs(forecast, max_wall_secs);
    (remaining_secs < window).then(|| {
        format!(
            "{remaining_secs}s of the wall budget left < {window}s (next call ~{}s + final answer \
             ~{}s: {} answer tokens at {:.1} tok/s, prompt {} at {:.3} ms/token)",
            forecast.next_call_secs(),
            forecast.answer_secs(),
            forecast.answer_completion_tokens,
            forecast.decode_tok_per_sec,
            forecast.prompt_tokens,
            forecast.prefill_ms_per_token
        )
    })
}

/// Why the run is inside the token window, or `None`.
pub(crate) fn token_window_reached(
    remaining_tokens: u64,
    forecast: &CallForecast,
    max_budget_tokens: u64,
) -> Option<String> {
    let window = token_window(forecast, max_budget_tokens);
    (remaining_tokens < window).then(|| {
        format!(
            "{remaining_tokens} tokens of the budget left < {window} (next call ~{} + final \
             answer ~{} tokens)",
            forecast.next_call_tokens(),
            forecast.answer_tokens()
        )
    })
}

/// Why the run is inside the cost window, or `None`. The cost of a token is
/// this run's measured accounted cost / accounted tokens; without a
/// reported cost there is nothing to measure and no cost window (rule 4).
pub(crate) fn cost_window_reached(
    remaining_usd: f64,
    usd_per_token: f64,
    forecast: &CallForecast,
    max_cost_usd: f64,
) -> Option<String> {
    if usd_per_token <= 0.0 {
        return None;
    }
    let window = ((forecast.next_call_tokens() + forecast.answer_tokens()) as f64 * usd_per_token)
        .min(max_cost_usd / BUDGET_RESERVE_CAP_DIVISOR as f64);
    (remaining_usd < window).then(|| {
        format!(
            "${remaining_usd:.4} of the cost budget left < ${window:.4} (next call + final \
             answer at ${usd_per_token:.8}/token)"
        )
    })
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

/// Remaining (wall secs, max), (tokens, max) and (usd, usd per token, max)
/// for each configured limit.
type RemainingLimits = (
    Option<(u64, u64)>,
    Option<(u64, u64)>,
    Option<(f64, f64, f64)>,
);

/// Per-task wrap-up latch and the draft measurement the forecast uses.
#[derive(Debug, Default)]
pub(crate) struct WrapUpState {
    /// The ONE wrap-up of this task and the limit that triggered it; the
    /// deadline and the budgets share it so they never both inject.
    pub issued: Option<WrapUpCause>,
    /// Calls already attributed (index into the client's call shapes).
    pub seen_calls: usize,
    /// Largest completion size of a call that produced a draft (a tool-less
    /// answer of at least [`PARTIAL_TEXT_MIN_CHARS`] prose chars) this run.
    pub draft_completion_tokens: Option<u64>,
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
    /// Attribute the calls completed since the last loop turn: when the
    /// newest assistant message is a draft (tool-less, at least
    /// [`PARTIAL_TEXT_MIN_CHARS`] of prose), its call's completion size is
    /// a measured final-answer size for the forecast.
    pub(super) fn observe_turn_usage(&self) {
        let calls = self.client.call_shapes();
        let mut state = self.wrap_up.lock().unwrap_or_else(|e| e.into_inner());
        if calls.len() <= state.seen_calls {
            return;
        }
        state.seen_calls = calls.len();
        let Some(latest) = self.messages.iter().rev().find(|m| m.role == "assistant") else {
            return;
        };
        let text = latest.content.text_all();
        let tool_less = latest.tool_calls.as_ref().is_none_or(|t| t.is_empty())
            && crate::tool_parser::parse_tool_calls(&text)
                .tool_calls
                .is_empty();
        if tool_less && answer_prose(&text).chars().count() >= PARTIAL_TEXT_MIN_CHARS {
            let tokens = calls.last().map(|c| c.completion_tokens).unwrap_or(0);
            if tokens > 0 {
                state.draft_completion_tokens =
                    Some(state.draft_completion_tokens.unwrap_or(0).max(tokens));
            }
        }
    }

    /// The forecast of the next call and the final answer from this run's
    /// measured calls ([`CallForecast`]).
    pub(super) fn call_forecast(&self) -> CallForecast {
        let draft = self
            .wrap_up
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .draft_completion_tokens;
        CallForecast::from_calls(&self.client.call_shapes(), draft)
    }

    /// Remaining wall seconds, tokens and dollars for each configured limit.
    fn remaining_limits(&self) -> RemainingLimits {
        let wall = self
            .config
            .agent
            .max_wall_secs
            .filter(|&s| s > 0)
            .map(|max| (max.saturating_sub(self.budget_elapsed_secs()), max));
        let usage = self.client.accounted_usage();
        let tokens = self
            .config
            .agent
            .max_budget_tokens
            .filter(|&b| b > 0)
            .map(|max| {
                (
                    (max as u64).saturating_sub(usage.total_tokens as u64),
                    max as u64,
                )
            });
        let cost = self
            .config
            .agent
            .max_cost_usd
            .filter(|&c| c > 0.0)
            .map(|max| {
                let spent = usage.cost.unwrap_or(0.0);
                let per_token = if usage.total_tokens > 0 {
                    spent / usage.total_tokens as f64
                } else {
                    0.0
                };
                ((max - spent).max(0.0), per_token, max)
            });
        (wall, tokens, cost)
    }

    /// The limit whose window this run is inside, if any: the wall-clock
    /// deadline first (a timeout loses the in-flight answer outright), then
    /// the token budget, then the cost budget.
    fn limit_in_reserve(&self) -> Option<StepAside> {
        let forecast = self.call_forecast();
        let (wall, tokens, cost) = self.remaining_limits();
        if let Some((remaining, max)) = wall {
            if let Some(detail) = time_window_reached(remaining, &forecast, max) {
                return Some(StepAside {
                    cause: WrapUpCause::Deadline,
                    detail,
                });
            }
        }
        if let Some((remaining, max)) = tokens {
            if let Some(detail) = token_window_reached(remaining, &forecast, max) {
                return Some(StepAside {
                    cause: WrapUpCause::TokenBudget,
                    detail,
                });
            }
        }
        if let Some((remaining, per_token, max)) = cost {
            if let Some(detail) = cost_window_reached(remaining, per_token, &forecast, max) {
                return Some(StepAside {
                    cause: WrapUpCause::CostBudget,
                    detail,
                });
            }
        }
        None
    }

    /// One-time wrap-up, whichever limit comes first, evaluated ONE TURN
    /// AHEAD before each ordinary call: when the time, tokens or cost left
    /// after the forecast next call would no longer fit the forecast final
    /// answer ([`time_window_reached`], [`token_window_reached`],
    /// [`cost_window_reached`]), tell the model to stop exploring and write
    /// the final answer now, labelling unfinished areas. ONE latch for all
    /// limits (reason recorded), so the deadline and the budget can never
    /// both inject. Latch reset in `run_task`.
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
        let forecast = self.call_forecast();
        let (wall, tokens, cost) = self.remaining_limits();
        let (headline, left) = match window.cause {
            WrapUpCause::Deadline => (
                "DEADLINE WRAP-UP",
                format!(
                    "about {}s of the wall-clock budget remain, and a final answer is forecast \
                     to take ~{}s on this endpoint",
                    wall.map(|w| w.0).unwrap_or(0),
                    forecast.answer_secs()
                ),
            ),
            WrapUpCause::TokenBudget => (
                "BUDGET WRAP-UP",
                format!(
                    "about {} tokens of the token budget remain, and a final answer is forecast \
                     to use ~{} tokens",
                    tokens.map(|t| t.0).unwrap_or(0),
                    forecast.answer_tokens()
                ),
            ),
            WrapUpCause::CostBudget => (
                "BUDGET WRAP-UP",
                format!(
                    "about ${:.4} of the cost budget remain, and a final answer is forecast to \
                     use ~{} tokens",
                    cost.map(|c| c.0).unwrap_or(0.0),
                    forecast.answer_tokens()
                ),
            ),
        };
        tracing::info!(
            cause = window.cause.label(),
            detail = %window.detail,
            "wrap-up directive injected"
        );
        self.messages.push(Message::user(format!(
            "<selfware_system_directive>\n\
             {headline}: {left} — there is room for one more answer, not for more \
             exploration. Stop exploring: do not start new reads, searches or approaches. \
             Write your FINAL ANSWER NOW from what you already have. Label every area you did \
             not finish as UNFINISHED (not checked), and do not present unchecked areas as \
             reviewed.\n\
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

    /// Why a completion-gate correction round no longer fits: the run is
    /// inside the wrap-up window of the deadline, the token budget or the
    /// cost budget (a correction round is at least one more call before
    /// the answer). `None` while every configured limit still has room.
    pub(super) fn completion_gate_step_aside(&self) -> Option<StepAside> {
        self.limit_in_reserve()
    }

    /// Why not even the final answer fits any more (`one_turn_no_fit`), as
    /// a step-aside. Used ONLY where the caller has already concluded that
    /// no turn fits (the draft-at-limit acceptance). It is uncapped, so it
    /// must not feed `completion_gate_step_aside`: with a budget smaller
    /// than one forecast answer it holds from the first step and would turn
    /// off every correction round for the whole run (review of C4, 0.9.1).
    pub(super) fn answer_no_fit_step_aside(&self) -> Option<StepAside> {
        self.one_turn_no_fit()
            .map(|(cause, detail)| StepAside { cause, detail })
    }

    /// Whether not even the final answer itself fits any configured limit
    /// any more (time / tokens / cost left below its forecast).
    fn one_turn_no_fit(&self) -> Option<(WrapUpCause, String)> {
        let forecast = self.call_forecast();
        let (wall, tokens, cost) = self.remaining_limits();
        if let Some((remaining, _)) = wall {
            let need = forecast.answer_secs();
            if remaining < need {
                return Some((
                    WrapUpCause::Deadline,
                    format!("{remaining}s left < forecast final answer {need}s"),
                ));
            }
        }
        if let Some((remaining, _)) = tokens {
            let need = forecast.answer_tokens();
            if remaining < need {
                return Some((
                    WrapUpCause::TokenBudget,
                    format!("{remaining} tokens left < forecast final answer {need} tokens"),
                ));
            }
        }
        if let Some((remaining, per_token, _)) = cost {
            let need = forecast.answer_tokens() as f64 * per_token;
            if per_token > 0.0 && remaining < need {
                return Some((
                    WrapUpCause::CostBudget,
                    format!("${remaining:.4} left < forecast final answer ${need:.4}"),
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
    /// before a later edit, nor one a newer write-up superseded. Returns the
    /// accepted text.
    pub(super) fn take_rejected_draft_at_limit(&mut self) -> Option<String> {
        let (cause, why) = self.one_turn_no_fit()?;
        let draft = {
            let mut state = self.citation_gate.lock().unwrap_or_else(|e| e.into_inner());
            if state
                .rejected_draft
                .as_ref()
                .is_some_and(|d| !self.rejected_draft_is_newest_write_up(&d.text))
            {
                // A newer write-up superseded it: never deliver the older one.
                state.rejected_draft = None;
                return None;
            }
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

    /// Whether the citation gate's kept draft is still the run's newest
    /// write-up: walking the history newest first, the draft's own prose is
    /// met before any other assistant message with at least
    /// [`PARTIAL_TEXT_MIN_CHARS`] of prose. A newer write-up the gate never
    /// judged (the run stopped first) supersedes it. With the draft compacted
    /// out of the history and nothing newer, it still stands.
    fn rejected_draft_is_newest_write_up(&self, draft: &str) -> bool {
        let draft = answer_prose(draft);
        for m in self.messages.iter().rev().filter(|m| m.role == "assistant") {
            let prose = answer_prose(&m.content.text_all());
            if prose == draft {
                return true;
            }
            if prose.chars().count() >= PARTIAL_TEXT_MIN_CHARS {
                return false;
            }
        }
        true
    }

    /// Answer text for a timeout partial (see
    /// [`PartialProgress::last_assistant_text`]): the draft the citation
    /// gate last rejected while it is still the newest write-up, else every assistant write-up segment of at least
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
            .filter(|d| self.rejected_draft_is_newest_write_up(&d.text))
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
