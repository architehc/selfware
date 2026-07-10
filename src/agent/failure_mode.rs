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
            FailureKind::MaxIterations => "MAX_ITERATIONS",
            FailureKind::FakeComplete => "FAKE_COMPLETE",
            FailureKind::Unknown => "UNKNOWN",
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, FailureKind::Success)
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
        let circuit_open = agent.prefill_breaker_open();
        let prefill_400s = agent.prefill_400_count();

        match outcome {
            RunOutcome::NaturalCompletion => {
                if mutating == 0 && has_final_answer_marker {
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
                // Reaching here with 0 mutating calls means: no "Final answer"
                // marker AND the task was not detected as mutation-requiring —
                // i.e. a legitimate read-only / Q&A completion. It changed
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
                if reason.contains("Max iterations") {
                    return classify_max_iter_failure(
                        mutating,
                        progress_guard,
                        no_action_consecutive,
                        permanently_blocked,
                        total_calls,
                        final_answer_len,
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
                        advice: "increase --max-budget-tokens or shrink the task scope"
                            .to_string(),
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
                )
            }
            RunOutcome::Partial => classify_max_iter_failure(
                mutating,
                progress_guard,
                no_action_consecutive,
                permanently_blocked,
                total_calls,
                final_answer_len,
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

fn classify_max_iter_failure(
    mutating: usize,
    progress_guard: usize,
    no_action_consecutive: usize,
    permanently_blocked: usize,
    total_calls: usize,
    final_answer_len: usize,
) -> FailureMode {
    // Order matters: most specific signals first.

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

    // 4) Fake-complete (caught by gate, not by natural completion).
    if mutating == 0 && final_answer_len > 0 {
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
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_config() -> Config {
        Config {
            endpoint: "http://localhost:0/v1".to_string(),
            model: "mock-model".to_string(),
            context_length: 500_000,
            max_tokens: 8192,
            agent: crate::config::AgentConfig {
                max_iterations: 8,
                step_timeout_secs: 30,
                streaming: false,
                native_function_calling: false,
                min_completion_steps: 0,
                require_verification_before_completion: false,
                ..Default::default()
            },
            safety: crate::config::SafetyConfig {
                allowed_paths: vec!["./**".to_string(), "/**".to_string()],
                ..Default::default()
            },
            execution_mode: crate::config::ExecutionMode::Yolo,
            ..Default::default()
        }
    }

    async fn make_agent() -> Agent {
        Agent::new(test_config()).await.expect("agent::new")
    }

    #[tokio::test]
    async fn classify_success_when_mutating_calls_exist() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(3);
        agent.test_set_total_tool_calls(12);
        agent.test_set_last_assistant_response("Done.".to_string());

        let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
        assert_eq!(mode.kind, FailureKind::Success);
        assert!(
            mode.evidence.contains("3 mutating tool calls"),
            "evidence: {}",
            mode.evidence
        );
    }

    #[tokio::test]
    async fn classify_fake_complete_on_natural_with_final_answer_no_mutation() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(0);
        agent.test_set_total_tool_calls(4);
        agent
            .test_set_last_assistant_response("Final answer: implementation complete.".to_string());

        let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
        assert_eq!(mode.kind, FailureKind::FakeComplete);
        assert!(mode.evidence.contains("0 mutating"));
    }

    #[tokio::test]
    async fn classify_nonterm_prose_when_consecutive_no_action_high() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(0);
        agent.test_set_consecutive_no_action(30);
        let big = "x".repeat(47_000);
        agent.test_set_last_assistant_response(big);

        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "Max iterations exceeded".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::NontermProse);
        assert!(
            mode.evidence.contains("30 consecutive"),
            "evidence: {}",
            mode.evidence
        );
        assert!(
            mode.evidence.contains("KB of text"),
            "evidence: {}",
            mode.evidence
        );
    }

    #[tokio::test]
    async fn classify_read_loop_when_progress_guard_fired_no_mutations() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(0);
        agent.test_set_progress_guard_fires(2);
        agent.test_set_total_tool_calls(15);

        let mode = FailureMode::classify(&agent, RunOutcome::Partial);
        assert_eq!(mode.kind, FailureKind::ReadLoop);
        assert!(mode.evidence.contains("progress guard fired 2"));
    }

    #[tokio::test]
    async fn classify_retry_loop_on_permanently_blocked_tool_calls() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(1);
        agent.test_set_permanently_blocked(3);
        agent.test_set_total_tool_calls(20);

        let mode = FailureMode::classify(&agent, RunOutcome::Partial);
        assert_eq!(mode.kind, FailureKind::RetryLoop);
        assert!(mode.evidence.contains("3 tool call"));
    }

    #[tokio::test]
    async fn classify_prefill_breaker_when_circuit_open() {
        let mut agent = make_agent().await;
        agent.test_set_prefill_400s(5);
        agent.test_set_prefill_breaker_open(true);

        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "prefill incompatible".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::PrefillBreaker);
        assert!(mode.evidence.contains("5 prefill-incompatible"));
    }

    #[tokio::test]
    async fn classify_timeout_when_reason_contains_timeout() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(2);

        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "wall-clock timeout".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::Timeout);
    }

    #[tokio::test]
    async fn classify_selfware_error_on_panic_reason() {
        let agent = make_agent().await;
        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "internal: panicked at src/foo.rs:42".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::SelfwareError);
    }

    #[tokio::test]
    async fn classify_max_iterations_when_no_clear_signal() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(2);
        agent.test_set_total_tool_calls(20);

        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "Max iterations exceeded".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::MaxIterations);
        assert!(mode.evidence.contains("max_iterations"));
    }

    #[tokio::test]
    async fn write_artifact_emits_failure_mode_json() {
        let mode = FailureMode {
            kind: FailureKind::ReadLoop,
            evidence: "ev".to_string(),
            advice: "ad".to_string(),
        };
        let dir = tempfile::tempdir().unwrap();
        mode.write_artifact(dir.path()).await.unwrap();
        let path = dir.path().join("failure_mode.json");
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("ReadLoop"));
        assert!(contents.contains("\"evidence\""));
    }

    #[test]
    fn cli_banner_uses_tag_and_evidence() {
        let mode = FailureMode {
            kind: FailureKind::NontermProse,
            evidence: "30 consecutive prose-only turns".to_string(),
            advice: "try a smaller context budget".to_string(),
        };
        let banner = mode.cli_banner();
        assert!(banner.contains("NONTERM_PROSE"));
        assert!(banner.contains("30 consecutive"));
        assert!(banner.contains("smaller context"));

        let success = FailureMode {
            kind: FailureKind::Success,
            evidence: "all good".to_string(),
            advice: "-".to_string(),
        };
        assert!(success.cli_banner().contains("REAL_EDIT"));
        assert!(success.cli_banner().contains("✅"));
    }

    #[test]
    fn failure_kind_serializes_to_json() {
        let mode = FailureMode {
            kind: FailureKind::PrefillBreaker,
            evidence: "x".to_string(),
            advice: "y".to_string(),
        };
        let json = serde_json::to_string(&mode).unwrap();
        assert!(json.contains("PrefillBreaker"));
        assert!(json.contains("\"kind\""));
    }

    #[tokio::test]
    async fn classify_no_change_when_natural_completion_zero_mutations() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(0);
        agent.test_set_total_tool_calls(2);
        agent.test_set_last_assistant_response(
            "Here is the explanation you asked for.".to_string(),
        );
        let mode = FailureMode::classify(&agent, RunOutcome::NaturalCompletion);
        assert_eq!(mode.kind, FailureKind::NoChange);
        let banner = mode.cli_banner();
        assert!(!banner.contains("REAL_EDIT"), "banner: {banner}");
        assert!(!banner.contains("❌"), "banner: {banner}");
        assert!(banner.contains("NO_CHANGES"), "banner: {banner}");
    }

    #[tokio::test]
    async fn classify_budget_exhausted_distinct_from_timeout() {
        let mut agent = make_agent().await;
        agent.test_set_mutating_count(1);
        let mode = FailureMode::classify(
            &agent,
            RunOutcome::Failed {
                reason: "token budget exhausted".to_string(),
            },
        );
        assert_eq!(mode.kind, FailureKind::BudgetExhausted);
        assert!(mode.advice.contains("max-budget-tokens"), "advice: {}", mode.advice);
    }

    #[test]
    fn cli_banner_no_change_is_neither_success_nor_abort() {
        let m = FailureMode {
            kind: FailureKind::NoChange,
            evidence: "e".to_string(),
            advice: "a".to_string(),
        };
        let b = m.cli_banner();
        assert!(b.contains("NO_CHANGES"));
        assert!(!b.contains("REAL_EDIT"));
        assert!(!b.contains("❌"));
    }

    #[test]
    fn advice_is_operator_facing_not_selfware_internal() {
        // ReadLoop branch: progress guard fired, zero mutations.
        let read_loop = classify_max_iter_failure(0, 1, 0, 0, 5, 0);
        assert_eq!(read_loop.kind, FailureKind::ReadLoop);
        // RetryLoop branch: a tool was permanently blocked.
        let retry_loop = classify_max_iter_failure(0, 0, 0, 1, 5, 0);
        assert_eq!(retry_loop.kind, FailureKind::RetryLoop);
        for m in [&read_loop, &retry_loop] {
            let a = m.advice.to_lowercase();
            assert!(!a.contains("selfware"), "advice leaks selfware internals: {}", m.advice);
            assert!(!a.contains("completion gate"), "advice leaks selfware internals: {}", m.advice);
            assert!(!a.contains("block threshold"), "advice leaks selfware internals: {}", m.advice);
        }
    }

    #[test]
    fn truncate_splits_at_char_boundaries() {
        // "αβγδ" is 4 Greek letters; slicing at byte index 3 would panic.
        let s = "αβγδ";
        assert_eq!(truncate(s, 4), "αβγδ");
        assert_eq!(truncate(s, 3), "αβγ…");
        assert_eq!(truncate(s, 0), "…");

        // ASCII fallback.
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 3), "hel…");
    }
}
