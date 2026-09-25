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
    /// ONE model call exceeded the per-call cap (`agent.max_call_secs`) and
    /// was aborted. Distinct from `Timeout` (the run's wall budget) and from
    /// `MaxIterations`: neither more wall time nor more iterations helps —
    /// the cap, the output budget or the reasoning effort must change.
    CallTimeCap,
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
    /// A tool call required interactive confirmation/approval that a
    /// non-interactive run could not provide (e.g. `shell_exec` in a
    /// headless AutoEdit run). The run stopped on purpose, NOT because
    /// iterations ran out — the remedy is operator action (approve the tool,
    /// switch mode, grant permission), never "raise max_iterations".
    PermissionRequired,
    /// `max_iterations` hit without any other distinguishing signal.
    MaxIterations,
    /// Model emitted "Final answer:" without ever mutating a tool.
    FakeComplete,
    /// The run ended (naturally, often on its last permitted iteration) on a
    /// task that REQUIRED file changes, but no file change reached disk —
    /// mutating-classified calls were only probes/builds/tests. This is the
    /// failure twin of `NoChange`: a read-only task that changes nothing is
    /// done; an edit task that changes nothing is not (24k-context e2e: a
    /// documentation task spent 40/40 iterations re-reading files and
    /// rendered "✅ Completed — no file changes made", exit 0).
    RequiredEditMissing,
    /// The run ended naturally, but the verification it credited did not
    /// pass on the final tree (a failing check with no later covering pass).
    /// A failure: a "completed" label over failed checks claims a verified
    /// result that does not exist (AGENTS.md rule 3). Read-only tasks that
    /// changed nothing keep `NoChange` (their deliverable is the report), with
    /// the failed verification named in the evidence and banner.
    VerificationFailed,
    /// Outcome could not be classified from available signals.
    Unknown,
}

/// Evidence suffix naming a failed verification on an otherwise honest
/// `NoChange` completion; `cli_banner` keys its non-✅ header on it.
pub(crate) const VERIFICATION_FAILED_NOTE: &str =
    "verification FAILED — no check the run ran passed on the final tree";

/// Evidence note for a completed run whose requirements audit could not run;
/// `cli_banner` keys its non-clean header on it.
pub(crate) const AUDIT_NOT_PERFORMED_NOTE: &str = "requirements audit NOT PERFORMED";

/// Evidence marker for a completed run whose answer (or written deliverable)
/// still carries citations the deterministic check could not verify;
/// `cli_banner` keys its non-clean header on it. The full note reads
/// `citations: N of M could not be verified (W wrong, K without a checkable
/// symbol)`.
pub(crate) const CITATIONS_UNVERIFIED_NOTE: &str = "could not be verified";

/// Evidence marker for a completed review/report whose answer carries no
/// checkable citation at all (`citations: none checkable: ...`): nothing was
/// checked against the files, so no clean ✅ claim either.
pub(crate) const CITATIONS_NONE_CHECKABLE_NOTE: &str =
    crate::agent::citation_check::CITATIONS_NONE_CHECKABLE;

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
            FailureKind::CallTimeCap => "CALL_TIME_CAP",
            FailureKind::SelfwareError => "SELFWARE_ERROR",
            FailureKind::NoChange => "NO_CHANGES",
            FailureKind::BudgetExhausted => "BUDGET_EXHAUSTED",
            FailureKind::BlockedBySafety => "BLOCKED_BY_SAFETY",
            FailureKind::PermissionRequired => "PERMISSION_REQUIRED",
            FailureKind::MaxIterations => "MAX_ITERATIONS",
            FailureKind::FakeComplete => "FAKE_COMPLETE",
            FailureKind::RequiredEditMissing => "NO_CHANGES_REQUIRED_EDIT",
            FailureKind::VerificationFailed => "VERIFICATION_FAILED",
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
    /// Files the failed run restored to the best (last-green) snapshot
    /// after classification. Empty unless a restore actually happened; the
    /// run summary names them so "files changed" never silently lists a
    /// file whose edits were rolled back.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub restored_files: Vec<String>,
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
                let base = (|| -> FailureMode {
                    // A "Final answer" with 0 mutating calls is only fake when the
                    // task was expected to mutate. On a read-only task (review /
                    // analysis / report) the prose answer IS the deliverable —
                    // fall through to the honest NoChange label below.
                    if mutating == 0 && has_final_answer_marker && !read_only {
                        return FailureMode {
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
                        kind: FailureKind::NoChange,
                        evidence: format!(
                            "completed naturally with 0 mutating tool calls ({} total) — no files changed",
                            total_calls
                        ),
                        advice: "if this task needed edits, the model made none; if it was read-only/Q&A, this is expected".to_string(),
                    };
                    }
                    // REAL_EDIT must mean files actually changed (2026-09-22 e2e:
                    // runs whose only "mutations" were read-shaped shell probes
                    // rendered REAL_EDIT with "files changed: none"). With
                    // mutating > 0 but no file evidence — no file-tool write and
                    // no write-shaped shell command — label the run honestly as
                    // NoChange instead of crediting an edit that never landed.
                    if agent.written_paths().is_empty() && !agent.shell_write_evidence() {
                        // On a task that REQUIRED edits, "no file reached disk"
                        // is a failure, not an honest no-op: never render ✅.
                        if agent.current_task_requires_mutation() {
                            return FailureMode {
                            restored_files: Vec::new(),
                            kind: FailureKind::RequiredEditMissing,
                            evidence: format!(
                                "task required file changes but none reached disk: {mutating} mutating-classified call(s) were probes/builds, {total_calls} total, {}/{} iterations used",
                                agent.current_iteration(),
                                agent.loop_control.max_iterations()
                            ),
                            advice: "the model investigated without editing — name the exact file(s) and change to make, or raise the context budget if it kept re-reading files it could not hold".to_string(),
                        };
                        }
                        return FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::NoChange,
                        evidence: format!(
                            "completed naturally; {mutating} mutating tool call(s) but no file reached disk ({total_calls} total)"
                        ),
                        advice: "shell probes and reads do not change files — if the task needed edits, none landed; check the run summary's files-changed line".to_string(),
                    };
                    }
                    let progress_note = if progress_guard > 0 {
                        format!(", {} progress guards", progress_guard)
                    } else {
                        ", 0 progress guards".to_string()
                    };
                    FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::Success,
                        evidence: format!(
                            "{} mutating tool calls, {} total tool calls{}, completed naturally",
                            mutating, total_calls, progress_note
                        ),
                        advice: "-".to_string(),
                    }
                })();
                with_citation_status(
                    with_audit_status(
                        with_verification_verdict(
                            base,
                            agent.credited_verification_summary(),
                            read_only,
                        ),
                        agent.requirements_audit_status().as_ref(),
                    ),
                    agent.grounding_status().as_ref(),
                )
            }
            RunOutcome::Failed { reason } => {
                if circuit_open || prefill_400s >= 3 {
                    return FailureMode {
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
                        kind: FailureKind::NontermProse,
                        evidence: "aborted early: repeated prose-only turns with no tool call".to_string(),
                        advice: "the model narrated instead of acting — ensure native tool-calling works and give a concrete single-goal task; do NOT raise max_iterations".to_string(),
                    };
                }
                if reason.contains("READ_LOOP_NO_EDIT") {
                    return FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::ReadLoop,
                        evidence: "aborted early: read-only tool loop on a mutation task with 0 edits".to_string(),
                        advice: "the model kept reading without editing — point it at the file to change; do NOT raise max_iterations".to_string(),
                    };
                }
                // Permission / operator-approval stop (2026-09-21 review,
                // P2): a tool that requires interactive confirmation in a
                // non-interactive run ends the loop with the TYPED
                // `ConfirmationRequired` error, whose message names the
                // non-interactive mode. Unrecognized, the generic fallback
                // filed it as MAX_ITERATIONS after 3 iterations ("raise
                // max_iterations") although the cap was never approached:
                // the stop was deliberate and the remedy is operator action.
                // Matched before the counter fallbacks exactly like the
                // other explicit loop-abort markers above.
                if reason.contains("requires confirmation but running in non-interactive mode") {
                    return FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::PermissionRequired,
                        evidence: format!(
                            "run stopped: a tool call required interactive approval unavailable in this mode ({} total tool calls, {} mutating)",
                            total_calls, mutating
                        ),
                        advice: "re-run interactively, use --yolo / auto-approve the tool, or pre-grant the permission — do NOT raise max_iterations".to_string(),
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
                // Per-call cap (`agent.max_call_secs`): one call was aborted.
                // Its message ("Per-call time cap exceeded") carries neither
                // "timeout" nor "wall-clock", so it used to fall through to
                // MAX_ITERATIONS with "raise max_iterations" advice (0.8.2
                // live validation D1). Name the real knob.
                if reason.contains("Per-call time cap exceeded")
                    || reason.contains("agent.max_call_secs")
                {
                    return FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::CallTimeCap,
                        evidence: format!(
                            "{} — one model call was aborted at the per-call cap, with {} mutating tool calls completed",
                            truncate(&reason, 120),
                            mutating
                        ),
                        advice: "raise agent.max_call_secs in the config, or lower max_tokens / reasoning effort so one call fits; more iterations or wall time will not help".to_string(),
                    };
                }
                // Cost cap: same kind as the token cap, but name the right knob.
                if reason.contains("Cost budget exhausted") {
                    return FailureMode {
                        restored_files: Vec::new(),
                        kind: FailureKind::BudgetExhausted,
                        evidence: format!(
                            "cost budget exhausted with {} mutating tool calls completed ({})",
                            mutating,
                            truncate(&reason, 80)
                        ),
                        advice: "increase --max-cost-usd or shrink the task scope".to_string(),
                    };
                }
                // Token-budget exhaustion is NOT a wall-clock timeout — the fix
                // is a bigger --max-budget-tokens, not more wall time. Check it
                // first so it isn't misfiled as Timeout with the wrong advice.
                if reason.contains("budget exhausted")
                    || reason.to_lowercase().contains("token budget")
                {
                    return FailureMode {
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
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
                        restored_files: Vec::new(),
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
        let header = if self.kind.is_success() && self.evidence.contains(AUDIT_NOT_PERFORMED_NOTE) {
            // Allowed with an explicit warning: the audit infrastructure
            // failed, so the result was never audited — no clean ✅ claim.
            format!(
                "⚠️ Task completed ({}) — {AUDIT_NOT_PERFORMED_NOTE}; the result was not audited",
                self.kind.tag()
            )
        } else if self.kind.is_nonfailure()
            && self.evidence.contains(CITATIONS_UNVERIFIED_NOTE)
            && !self.evidence.contains(VERIFICATION_FAILED_NOTE)
        {
            // Allowed with an explicit warning: the citation gate stepped
            // aside after its bounded correction rounds — no clean ✅ claim
            // for an answer whose cited evidence does not match the files.
            format!(
                "⚠️ Task completed ({}) — some citations could not be verified; the answer is not fully grounded",
                self.kind.tag()
            )
        } else if self.kind.is_nonfailure()
            && self.evidence.contains(CITATIONS_NONE_CHECKABLE_NOTE)
            && !self.evidence.contains(VERIFICATION_FAILED_NOTE)
        {
            // A review/report answer with nothing checkable: completed, but
            // not grounded — say so instead of a clean ✅.
            format!(
                "⚠️ Task completed ({}) — {CITATIONS_NONE_CHECKABLE_NOTE}; the answer was not checked against the files",
                self.kind.tag()
            )
        } else if self.kind.is_success() {
            format!("✅ Task completed successfully ({})", self.kind.tag())
        } else if matches!(self.kind, FailureKind::NoChange)
            && self.evidence.contains(VERIFICATION_FAILED_NOTE)
        {
            // Read-only deliverable, but its own checks failed: never ✅.
            format!(
                "⚠️ Finished — no file changes made, and verification FAILED ({})",
                self.kind.tag()
            )
        } else if matches!(self.kind, FailureKind::NoChange) {
            // Completed, but made no edits — honest neutral banner, not a
            // "successfully (REAL_EDIT)" claim and not an abort.
            format!("✅ Completed — no file changes made ({})", self.kind.tag())
        } else if matches!(self.kind, FailureKind::RequiredEditMissing) {
            format!(
                "❌ Task incomplete — required file changes were not made ({})",
                self.kind.tag()
            )
        } else if matches!(self.kind, FailureKind::VerificationFailed) {
            format!(
                "❌ Task incomplete — verification failed on the final tree ({})",
                self.kind.tag()
            )
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

/// Fold the run's credited verification (`Agent::credited_verification_summary`:
/// `(passed, checks)`, `None` = nothing ran) into a natural-completion verdict.
///
/// A non-failure verdict over FAILED verification must not render as a
/// completed task (e2e c40: "verification: failed (1 checks)" printed under
/// "✅ Completed" and exit 0):
/// - `Success` (edits landed) → `VerificationFailed` (failure, non-zero exit).
/// - `NoChange` on a task NOT classified read-only → `VerificationFailed`: the
///   run's own checks failed and nothing was changed to address them.
/// - `NoChange` on a read-only task stays `NoChange` (the report is the
///   deliverable; a failing check is a finding about the workspace), but the
///   evidence names the failure so the banner is not ✅.
///
/// Failure verdicts pass through unchanged.
pub(crate) fn with_verification_verdict(
    base: FailureMode,
    verification: Option<(bool, usize)>,
    read_only: bool,
) -> FailureMode {
    let Some((false, checks)) = verification else {
        return base;
    };
    match base.kind {
        FailureKind::Success => FailureMode {
            kind: FailureKind::VerificationFailed,
            evidence: format!(
                "{}; but {checks} verification check(s) did not pass on the final tree",
                base.evidence
            ),
            advice: "the edits landed but the run's own verification failed — fix the failing check and rerun it to green".to_string(),
            restored_files: base.restored_files,
        },
        FailureKind::NoChange if !read_only => FailureMode {
            kind: FailureKind::VerificationFailed,
            evidence: format!(
                "{}; {checks} verification check(s) failed and nothing was changed to address them",
                base.evidence
            ),
            advice: "the run ended with failing checks and no edits — if the task needed changes, none landed".to_string(),
            restored_files: base.restored_files,
        },
        FailureKind::NoChange => FailureMode {
            evidence: format!(
                "{}; {VERIFICATION_FAILED_NOTE} ({checks} check(s))",
                base.evidence
            ),
            ..base
        },
        _ => base,
    }
}

/// Fold a requirements audit that could NOT run into a non-failure verdict's
/// evidence (AGENTS.md rule 3). The kind — and so the exit status — is
/// unchanged: the audit is advisory when its infrastructure fails
/// (gateway timeout, side-call cap, unparseable answer). But the banner and
/// every consumer of the evidence must see that it did not run, instead of
/// the clean "completed successfully" of an audited run. Failure verdicts and
/// performed audits pass through unchanged.
pub(crate) fn with_audit_status(
    base: FailureMode,
    audit: Option<&crate::agent::RequirementsAuditStatus>,
) -> FailureMode {
    match audit {
        Some(crate::agent::RequirementsAuditStatus::NotPerformed(reason))
            if base.kind.is_nonfailure() =>
        {
            FailureMode {
                evidence: format!("{}; {AUDIT_NOT_PERFORMED_NOTE} ({reason})", base.evidence),
                ..base
            }
        }
        _ => base,
    }
}

/// Fold citations that still could not be verified (after the citation
/// gate's bounded correction rounds) into a non-failure verdict's evidence.
/// Like [`with_audit_status`], the kind — and the exit status — is unchanged,
/// but the banner and every evidence consumer see
/// `citations: N of M could not be verified (W wrong, K without a checkable
/// symbol)`.instead of a clean pass
/// (AGENTS.md rule 3). Failure verdicts and fully verified answers pass
/// through unchanged.
pub(crate) fn with_citation_status(
    base: FailureMode,
    grounding: Option<&crate::agent::citation_check::GroundingStatus>,
) -> FailureMode {
    match grounding.and_then(|g| g.warning_note()) {
        Some(note) if base.kind.is_nonfailure() => FailureMode {
            evidence: format!("{}; {}", base.evidence, note),
            ..base
        },
        _ => base,
    }
}

fn blocked_by_safety_failure(blocked: usize, window: usize) -> FailureMode {
    FailureMode {
        restored_files: Vec::new(),
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
            restored_files: Vec::new(),
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
            restored_files: Vec::new(),
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
            restored_files: Vec::new(),
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
            restored_files: Vec::new(),
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
        restored_files: Vec::new(),
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
