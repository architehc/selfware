//! Structured failure-mode classification for agent runs.
//!
//! When a `run_task` ends — successfully or otherwise — selfware already
//! tracks all of the signals needed to explain *why*: how many mutating
//! tool calls happened, whether the progress guard fired, whether the
//! model fake-completed, whether a circuit breaker tripped, and so on.
//!
//! Historically this state was discarded as soon as the loop exited,
//! leaving SWE-bench-Pro post-mortems to grep through the log file with
//! a forensic Bash script. This module promotes it to a first-class
//! structured artifact so the harness — and the CLI — can surface a
//! concrete failure category, evidence, and a one-line piece of advice
//! at the end of every run.
//!
//! The classifier is purely *observational*: it inspects already-recorded
//! agent state and returns a verdict. It does not mutate the agent.

use serde::Serialize;
use std::path::Path;

use super::Agent;

/// A category of run outcome.
///
/// `Success` is included so the same artifact format describes both
/// happy and unhappy endings — making downstream histograms trivial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum FailureKind {
    /// Run reached a natural completion with at least one mutating tool call.
    Success,
    /// 3+ "Assistant response prefill incompatible" 400s tripped the breaker.
    PrefillBreaker,
    /// Progress guard fired: long read-only streak ended without writes.
    ReadLoop,
    /// `consecutive_no_action_prompts` hit the abort threshold (prose-only).
    NontermProse,
    /// Hard-block reached after repeated failed retries on the same tool.
    RetryLoop,
    /// Wall-clock budget exhausted while making progress.
    Timeout,
    /// Selfware-side panic, invariant violation, or known bug.
    SelfwareError,
    /// Completed naturally but performed zero mutating tool calls — no files
    /// were changed. Not a failure (e.g. a read-only / Q&A task), but NOT a
    /// real edit either, so it must never be labeled REAL_EDIT.
    NoChange,
    /// Token budget (`max_budget_tokens`) exhausted — distinct from a
    /// wall-clock timeout; the fix is a bigger token budget, not more wall time.
    BudgetExhausted,
    /// Safety policy refused the operations the task depended on, so the run
    /// burned its budget with no way forward. Distinct from `Timeout`: more
    /// wall-clock or iterations cannot fix a (correct) safety refusal — the
    /// task or the safety configuration must change.
    BlockedBySafety,
    /// `max_iterations` hit without any other distinguishing signal.
    MaxIterations,
    /// Model emitted "Final answer:" without ever mutating a tool.
    FakeComplete,
    /// Outcome could not be classified from available signals.
    Unknown,
}

impl FailureKind {
    /// Short uppercase tag suitable for log lines and CLI output.
    pub fn tag(&self) -> &'static str {
        match self {
            FailureKind::Success => "REAL_EDIT",
            FailureKind::PrefillBreaker => "PREFILL_BREAKER",
            FailureKind::ReadLoop => "READ_LOOP",
            FailureKind::NontermProse => "NONTERM_PROSE",
            FailureKind::RetryLoop => "RETRY_LOOP",
            FailureKind::Timeout => "TIMEOUT",
            FailureKind::SelfwareError => "SELFWARE_ERROR",
            FailureKind::NoChange => "NO_CHANGES",
            FailureKind::BudgetExhausted => "BUDGET_EXHAUSTED",
            FailureKind::BlockedBySafety => "BLOCKED_BY_SAFETY",
            FailureKind::MaxIterations => "MAX_ITERATIONS",
            FailureKind::FakeComplete => "FAKE_COMPLETE",
            FailureKind::Unknown => "UNKNOWN",
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, FailureKind::Success)
    }

    /// Whether this outcome is NOT a failure — a real edit (`Success`) or an
    /// honest no-op (`NoChange`). These render a "✅" banner and must emit a
    /// `TaskCompleted` progress event, not `TaskFailed`. Mirrors `cli_banner`'s
    /// ✅-vs-❌ split so the banner and the event stream never disagree.
    pub fn is_nonfailure(&self) -> bool {
        matches!(self, FailureKind::Success | FailureKind::NoChange)
    }
}

/// Structured failure-mode verdict for a single run.
///
/// `evidence` is a one-or-two-sentence summary with concrete numbers
/// (e.g. counter values, byte counts) so a human glancing at the CLI
/// output can immediately see *why* the classifier picked this kind.
/// `advice` is a one-sentence next step.
#[derive(Debug, Clone, Serialize)]
pub struct FailureMode {
    pub kind: FailureKind,
    pub evidence: String,
    pub advice: String,
}

impl FailureMode {
    /// Build a verdict directly from the agent state at run end.
    ///
    /// `outcome` describes how the loop terminated as observed by the
    /// `run_execution_loop`: natural completion, `Failed { reason }`,
    /// or partial (loop exited the iteration ceiling without entering
    /// any terminal state).
    pub fn classify(agent: &Agent, outcome: RunOutcome) -> Self {
        // Pull observational state off the agent. All accessors are
        // cheap and read-only.
        let mutating = agent.mutating_tool_call_count();
        let progress_guard = agent.progress_guard_fire_count();
        let no_action_consecutive = agent.consecutive_no_tool_call_turns();
        let permanently_blocked = agent.permanently_blocked_tool_calls_len();
        let total_calls = agent.total_tool_call_count();
        let final_answer_len = agent.last_assistant_response_len();
        let has_final_answer_marker = agent.last_assistant_response_has_final_answer();
        // Read-only classification stored at task start: on a review /
        // analysis / report task 0 mutating calls is the CORRECT outcome,
        // never a FakeComplete.
        let read_only = agent.current_task_is_read_only();
        let circuit_open = agent.prefill_breaker_open();
        let prefill_400s = agent.prefill_400_count();
        // A run that kept hitting the safety checker and never found another
        // way forward must not be mislabeled TIMEOUT / MAX_ITERATIONS — no
        // budget increase fixes a (correct) safety refusal. Computed once and
        // checked before the timeout/max-iterations mappings below.
        let safety_blocked = safety_blocked_share(agent);

        match outcome {
            RunOutcome::NaturalCompletion => {
                // A "Final answer" with 0 mutating calls is only fake when the
                // task was expected to mutate. On a read-only task (review /
                // analysis / report) the prose answer IS the deliverable —
                // fall through to the honest NoChange label below.
                if mutating == 0 && has_final_answer_marker && !read_only {
                    return FailureMode {
                        kind: FailureKind::FakeComplete,
                        evidence: format!(
                            "model emitted 'Final answer' but performed 0 mutating tool calls across {} total calls",
                            total_calls
                        ),
                        advice: "the model said it was done but changed no files — restate the task with the exact file(s) and change required, or confirm whether it was meant to be read-only".to_string(),
                    };
                }
                // Bug fix: a natural completion that performed zero mutating
                // calls on a task explicitly requiring mutation is also a
                // FakeComplete — even when the model never wrote the literal
                // "Final answer" marker. Without this, runs like
                // `selfware -p "fix the failing test"` that exit cleanly with
                // a chatty no-op response were wrongly tagged Success.
                if mutating == 0 && agent.current_task_requires_mutation() {
                    return FailureMode {
                        kind: FailureKind::FakeComplete,
                        evidence: format!(
                            "task required mutation but agent completed naturally with 0 mutating tool calls (total={})",
                            total_calls
                        ),
                        advice: "the model said it was done but changed no files — restate the task with the exact file(s) and change required, or confirm whether it was meant to be read-only".to_string(),
                    };
                }
                // Reaching here with 0 mutating calls means: the task was
                // read-only (final-answer marker or not) OR there was no
                // marker on a non-mutation task — i.e. a legitimate read-only
                // / Q&A completion. It changed
                // nothing, so it is NOT a REAL_EDIT; label it honestly.
                if mutating == 0 {
                    return FailureMode {
                        kind: FailureKind::NoChange,
                        evidence: format!(
                            "completed naturally with 0 mutating tool calls ({} total) — no files changed",
                            total_calls
                        ),
                        advice: "if this task needed edits, the model made none; if it was read-only/Q&A, this is expected".to_string(),
                    };
                }
                let progress_note = if progress_guard > 0 {
                    format!(", {} progress guards", progress_guard)
                } else {
                    ", 0 progress guards".to_string()
                };
                FailureMode {
                    kind: FailureKind::Success,
                    evidence: format!(
                        "{} mutating tool calls, {} total tool calls{}, completed naturally",
                        mutating, total_calls, progress_note
                    ),
                    advice: "-".to_string(),
                }
            }
            RunOutcome::Failed { reason } => {
                if circuit_open || prefill_400s >= 3 {
                    return FailureMode {
                        kind: FailureKind::PrefillBreaker,
                        evidence: format!(
                            "{} prefill-incompatible 400s tripped the circuit breaker (open={})",
                            prefill_400s, circuit_open
                        ),
                        advice: "disable assistant prefill or switch to a server build that accepts the prefill format".to_string(),
                    };
                }
                // Explicit loop-abort markers carry their own diagnosis. Match them
                // BEFORE the counter fallback (which otherwise misfiles them as
                // MaxIterations with the exact-wrong "raise max_iterations" advice
                // even though the ceiling was never approached — FAIL-MISLABEL-MAXITER).
                if reason.contains("FAKE_COMPLETE_LOOP") {
                    return FailureMode {
                        kind: FailureKind::FakeComplete,
                        evidence: format!(
                            "aborted early: repeated final answers with 0 mutating tool calls ({} total)",
                            total_calls
                        ),
                        advice: "the model will not edit — narrow/clarify the task or supply the target file and exact change; do NOT raise max_iterations".to_string(),
                    };
                }
                if reason.contains("NONTERM_PROSE_NO_TOOL") {
                    return FailureMode {
                        kind: FailureKind::NontermProse,
                        evidence: "aborted early: repeated prose-only turns with no tool call".to_string(),
                        advice: "the model narrated instead of acting — ensure native tool-calling works and give a concrete single-goal task; do NOT raise max_iterations".to_string(),
                    };
                }
                if reason.contains("READ_LOOP_NO_EDIT") {
                    return FailureMode {
                        kind: FailureKind::ReadLoop,
                        evidence: "aborted early: read-only tool loop on a mutation task with 0 edits".to_string(),
                        advice: "the model kept reading without editing — point it at the file to change; do NOT raise max_iterations".to_string(),
                    };
                }
                // Safety-blocked runs burn their whole budget and then get
                // mislabeled TIMEOUT / MAX_ITERATIONS. Check the safety share
                // BEFORE those mappings so exit status stops lying.
                if let Some((blocked, window)) = safety_blocked {
                    return blocked_by_safety_failure(blocked, window);
                }
                if reason.contains("Max iterations") {
                    return classify_max_iter_failure(
                        mutating,
                        progress_guard,
                        no_action_consecutive,
                        permanently_blocked,
                        total_calls,
                        final_answer_len,
                        safety_blocked,
                        read_only,
                    );
                }
                // Token-budget exhaustion is NOT a wall-clock timeout — the fix
                // is a bigger --max-budget-tokens, not more wall time. Check it
                // first so it isn't misfiled as Timeout with the wrong advice.
                if reason.contains("budget exhausted")
                    || reason.to_lowercase().contains("token budget")
                {
                    return FailureMode {
                        kind: FailureKind::BudgetExhausted,
                        evidence: format!(
                            "token budget exhausted with {} mutating tool calls completed",
                            mutating
                        ),
                        advice: "increase --max-budget-tokens or shrink the task scope".to_string(),
                    };
                }
                if reason.to_lowercase().contains("timeout") || reason.contains("wall-clock") {
                    return FailureMode {
                        kind: FailureKind::Timeout,
                        evidence: format!(
                            "wall-clock time budget exhausted with {} mutating tool calls completed",
                            mutating
                        ),
                        advice: "increase --max-wall-secs or shrink the task scope".to_string(),
                    };
                }
                if reason.contains("panic")
                    || reason.contains("invariant")
                    || reason.starts_with("internal:")
                {
                    return FailureMode {
                        kind: FailureKind::SelfwareError,
                        evidence: format!("selfware-side error: {}", truncate(&reason, 160)),
                        advice: "file a bug with the trace attached; this is not a model failure"
                            .to_string(),
                    };
                }
                // Fall through: treat as a classified failure based on counters.
                classify_max_iter_failure(
                    mutating,
                    progress_guard,
                    no_action_consecutive,
                    permanently_blocked,
                    total_calls,
                    final_answer_len,
                    safety_blocked,
                    read_only,
                )
            }
            RunOutcome::Partial => classify_max_iter_failure(
                mutating,
                progress_guard,
                no_action_consecutive,
                permanently_blocked,
                total_calls,
                final_answer_len,
                safety_blocked,
                read_only,
            ),
        }
    }

    /// Serialize the verdict to a `failure_mode.json` next to `result.json`.
    ///
    /// `result_dir` is the directory the SWE-bench Pro harness uses for a
    /// single instance's artifacts. Failures here are non-fatal: artifact
    /// writing is best-effort and must never abort a run.
    pub async fn write_artifact(&self, result_dir: &Path) -> std::io::Result<()> {
        let result_dir = result_dir.to_path_buf();
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let path = result_dir.join("failure_mode.json");
        tokio::fs::create_dir_all(&result_dir).await?;
        tokio::fs::write(&path, json).await
    }

    /// Render a multi-line CLI banner suitable for the end of a non-TUI run.
    pub fn cli_banner(&self) -> String {
        let header = if self.kind.is_success() {
            format!("✅ Task completed successfully ({})", self.kind.tag())
        } else if matches!(self.kind, FailureKind::NoChange) {
            // Completed, but made no edits — honest neutral banner, not a
            // "successfully (REAL_EDIT)" claim and not an abort.
            format!("✅ Completed — no file changes made ({})", self.kind.tag())
        } else {
            format!("❌ Task aborted ({})", self.kind.tag())
        };
        format!(
            "{}\n   evidence: {}\n   advice: {}",
            header, self.evidence, self.advice
        )
    }
}

/// How `run_execution_loop` terminated, from the caller's perspective.
#[derive(Debug, Clone)]
pub enum RunOutcome {
    /// Agent reached `AgentState::Completed` or returned successfully.
    NaturalCompletion,
    /// Agent transitioned to `AgentState::Failed { reason }`.
    Failed { reason: String },
    /// Loop exited without a terminal state (e.g. iteration ceiling fall-through).
    Partial,
}

/// Count of distinct tool calls the safety checker refused during this run.
/// `recent_failed_tool_attempts` dedups identical (tool, args, kind) attempts,
/// so this counts distinct refused operations, not retries of the same one.
fn safety_blocked_count(agent: &Agent) -> usize {
    agent
        .recent_failed_tool_attempts
        .iter()
        .filter(|attempt| attempt.failure_kind == "safety")
        .count()
}

/// `Some((blocked, window))` when safety refusals dominate the recent
/// tool-failure window — the run could not proceed because the safety checker
/// refused the operations the model attempted. `None` otherwise.
fn safety_blocked_share(agent: &Agent) -> Option<(usize, usize)> {
    let window = agent.recent_failed_tool_attempts.len();
    let blocked = safety_blocked_count(agent);
    (blocked >= 2 && blocked * 2 >= window).then_some((blocked, window))
}

fn blocked_by_safety_failure(blocked: usize, window: usize) -> FailureMode {
    FailureMode {
        kind: FailureKind::BlockedBySafety,
        evidence: format!(
            "safety policy refused {} of the last {} failed tool call(s); the run burned its budget with no way forward",
            blocked, window
        ),
        advice: "the safety checker refused the required operations — adjust the task or the safety configuration (allowed paths/commands); do NOT raise max_iterations or the wall-clock budget".to_string(),
    }
}

fn classify_max_iter_failure(
    mutating: usize,
    progress_guard: usize,
    no_action_consecutive: usize,
    permanently_blocked: usize,
    total_calls: usize,
    final_answer_len: usize,
    safety_blocked: Option<(usize, usize)>,
    read_only: bool,
) -> FailureMode {
    // Order matters: most specific signals first.

    // 0) Safety-blocked: the budget burned because the safety checker refused
    // the required operations — not a timeout/iteration problem.
    if let Some((blocked, window)) = safety_blocked {
        return blocked_by_safety_failure(blocked, window);
    }

    // 1) Prose-only termination.
    if no_action_consecutive >= super::recovery::MAX_NO_ACTION_PROMPTS && mutating == 0 {
        return FailureMode {
            kind: FailureKind::NontermProse,
            evidence: format!(
                "{} consecutive prose-only turns; model emitted {}KB of text without tool calls",
                no_action_consecutive,
                final_answer_len / 1024
            ),
            advice: "try a smaller context budget or a stronger quant".to_string(),
        };
    }

    // 2) Read-loop: progress guard fired and no edits ever landed.
    if progress_guard > 0 && mutating == 0 {
        return FailureMode {
            kind: FailureKind::ReadLoop,
            evidence: format!(
                "progress guard fired {} time(s); {} read/verify calls but 0 mutating calls",
                progress_guard, total_calls
            ),
            advice: "the model kept reading without editing — name the file to change and the concrete edit it should make"
                .to_string(),
        };
    }

    // 3) Retry-loop: tools were permanently blocked after repeated failures.
    if permanently_blocked >= 1 {
        return FailureMode {
            kind: FailureKind::RetryLoop,
            evidence: format!(
                "{} tool call(s) hard-blocked after repeated failures (mutating={}, total={})",
                permanently_blocked, mutating, total_calls
            ),
            advice: "a tool call kept failing — check the tool error in the log and adjust the task inputs, or steer the model away from that operation".to_string(),
        };
    }

    // 4) Fake-complete (caught by gate, not by natural completion). On a
    // read-only task prose output is the deliverable — 0 mutating calls is
    // expected, so fall through to the honest MaxIterations label.
    if !read_only && mutating == 0 && final_answer_len > 0 {
        return FailureMode {
            kind: FailureKind::FakeComplete,
            evidence: format!(
                "model produced {}B of final answer text but executed 0 mutating calls",
                final_answer_len
            ),
            advice: "the model said it was done but changed no files — restate the task with the exact file(s) and change required, or confirm whether it was meant to be read-only".to_string(),
        };
    }

    // 5) Otherwise: max iterations with no clear discriminator.
    FailureMode {
        kind: FailureKind::MaxIterations,
        evidence: format!(
            "max_iterations reached with {} mutating, {} total tool calls",
            mutating, total_calls
        ),
        advice: "raise max_iterations or split the task into smaller subtasks".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    // `char_indices()` guarantees we slice at a valid UTF-8 boundary and only
    // walks the string once.
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/failure_mode/failure_mode_test.rs"]
mod tests;
