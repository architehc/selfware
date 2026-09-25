use tracing::info;

use super::*;

/// The text that accompanies a recorded task outcome, typed by what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutcomeDetail<'a> {
    /// A status line — a completion verdict (`[green] [REAL_EDIT]`,
    /// `no file changes [NO_CHANGES]`) or an external stop ("Task
    /// interrupted by user"). Logged with the outcome; never an error.
    Status(&'a str),
    /// A real failure of the run: logged AND taught to the error learner.
    Failure(&'a str),
}

impl<'a> OutcomeDetail<'a> {
    pub(super) fn text(&self) -> &'a str {
        match *self {
            OutcomeDetail::Status(text) | OutcomeDetail::Failure(text) => text,
        }
    }
}

/// The platform data dir + `selfware`: where `improvement_engine.json`,
/// `metrics/snapshots.jsonl` and the episodic memory live.
///
/// Unit-test builds use a per-process temp dir instead. Every terminal
/// outcome now saves the learning state, so a test run would otherwise load
/// and overwrite the developer's real learning files with mock-task results.
pub(super) fn default_learning_data_dir() -> std::path::PathBuf {
    #[cfg(test)]
    {
        std::env::temp_dir().join(format!("selfware-test-learning-{}", std::process::id()))
    }
    #[cfg(not(test))]
    {
        dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware")
    }
}

impl Agent {
    pub(super) fn infer_task_type(task: &str) -> &'static str {
        let task_lower = task.to_lowercase();
        if task_lower.contains("review") {
            "code_review"
        } else if task_lower.contains("test") {
            "testing"
        } else if task_lower.contains("refactor") {
            "refactor"
        } else if task_lower.contains("fix") || task_lower.contains("bug") {
            "bug_fix"
        } else if task_lower.contains("document") || task_lower.contains("readme") {
            "documentation"
        } else if [
            "implement",
            "create",
            "add ",
            "edit",
            "modify",
            "update",
            "make ",
            "write ",
        ]
        .iter()
        .any(|k| task_lower.contains(k))
        {
            // Previously fell into "general", coarsening prompt-stats for the most
            // common (code-writing) task class (LEARN-TYPE-GENERAL).
            "implementation"
        } else {
            "general"
        }
    }

    pub(super) fn classify_error_type(error: &str) -> &'static str {
        let lower = error.to_lowercase();
        if lower.contains("timeout") || lower.contains("timed out") {
            "timeout"
        } else if lower.contains("permission") || lower.contains("denied") {
            "permission"
        } else if lower.contains("safety") || lower.contains("blocked") {
            "safety"
        } else if lower.contains("json") || lower.contains("parse") || lower.contains("invalid") {
            "parsing"
        } else if lower.contains("network") || lower.contains("connection") {
            "network"
        } else {
            "execution"
        }
    }

    pub(super) fn outcome_quality(outcome: Outcome) -> f32 {
        match outcome {
            Outcome::Success => 1.0,
            Outcome::Partial => 0.65,
            Outcome::Failure => 0.0,
            Outcome::Abandoned => 0.2,
        }
    }

    pub(super) fn learning_context(&self) -> &str {
        if self.current_task_context.is_empty() {
            "general"
        } else {
            &self.current_task_context
        }
    }

    /// The task context with the injected "requires these tools" appendix
    /// stripped, for MUTATION CLASSIFICATION only. `task_learning_context`
    /// appends lines like `` - `file_edit` `` when a task declares required
    /// tools; the "edit" substring in that appendix would otherwise flip an
    /// otherwise read-only task to mutation-required, mis-gating read-only
    /// reviews/analyses into the edit-completion path (churn).
    pub(super) fn task_context_for_classification(&self) -> &str {
        let ctx = self.learning_context();
        match ctx.find("\n\nThis task explicitly requires these tools") {
            Some(i) => ctx[..i].trim_end(),
            None => ctx,
        }
    }

    pub(super) fn start_learning_session(&mut self, session_id: &str, task_context: &str) {
        self.current_task_context = task_context.to_string();
        // Task-aware policy: classify read-only ONCE at task start so every
        // guard/gate consults the same stored decision.
        self.classify_task_policy();
        self.self_improvement.start_session(session_id);
        self.publish_phi_activity(crate::phi::activity::ActivityPhase::Running);
    }

    /// Record the task's outcome in telemetry and the learning engine.
    ///
    /// `detail` says what the accompanying text IS: only
    /// [`OutcomeDetail::Failure`] reaches the error learner. Every site used
    /// to pass its text as `error: Option<&str>`, and any `Some` was recorded
    /// as an unrecovered `task_execution` error — including the completion
    /// status `[green] [REAL_EDIT]` / `no file changes [NO_CHANGES]` of every
    /// successful run (8 of 14 persisted error records in the val083 audit).
    pub(super) fn record_task_outcome(
        &mut self,
        task_prompt: &str,
        outcome: Outcome,
        detail: OutcomeDetail<'_>,
    ) {
        self.sync_api_usage();
        self.log_task_outcome_event(task_prompt, outcome, Some(detail.text()));
        self.publish_phi_activity(match outcome {
            Outcome::Success => crate::phi::activity::ActivityPhase::Completed,
            Outcome::Partial => crate::phi::activity::ActivityPhase::Partial,
            Outcome::Failure => crate::phi::activity::ActivityPhase::Failed,
            Outcome::Abandoned => crate::phi::activity::ActivityPhase::Abandoned,
        });

        let task_type = Self::infer_task_type(task_prompt);
        self.self_improvement.record_prompt(
            task_prompt,
            task_type,
            outcome,
            Self::outcome_quality(outcome),
            // Same counter as `SessionResult.usage.total` (synced above).
            self.cumulative_token_usage.total,
        );
        // "Completed" for the usage analyzer: a positive outcome that is not
        // a failure. `Partial` alone also covers budget and wall-clock stops
        // (recorded with a `Failure` detail), which used to count as
        // completed tasks.
        self.self_improvement
            .record_task(outcome.is_positive() && matches!(detail, OutcomeDetail::Status(_)));

        if let OutcomeDetail::Failure(err) = detail {
            self.self_improvement.record_error(
                err,
                Self::classify_error_type(err),
                self.learning_context(),
                crate::cognitive::self_improvement::TASK_EXECUTION_ACTION,
                None,
            );
        }

        self.self_improvement.end_session(None);
    }

    /// Where the persisted learning state lives: the test override, else the
    /// platform data dir + `selfware` (the path `Agent::new` loads from).
    pub(super) fn learning_data_dir(&self) -> std::path::PathBuf {
        self.learning_data_dir
            .clone()
            .unwrap_or_else(default_learning_data_dir)
    }

    /// Measured facts about the run that just ended, from the counters the
    /// terminal result reports: `loop_turns` is `SessionResult.num_turns`
    /// (`current_iteration`), `llm_total_tokens` is `SessionResult.usage.total`
    /// (the API-usage accumulator, after draining pending usage).
    #[cfg(feature = "self-improvement")]
    pub(super) fn terminal_run_stats(
        &mut self,
        result: &Result<()>,
    ) -> crate::cognitive::metrics::TerminalRunStats {
        self.sync_api_usage();
        let verdict = self.last_run_failure_mode.clone();
        let outcome = classify_terminal_outcome(result, self.is_cancelled(), verdict.as_ref());
        let failure_mode = match (&verdict, result) {
            (Some(fm), _) => Some(fm.kind.tag().to_string()),
            // Stopped from outside before any verdict: nothing to classify.
            (None, _) if outcome == crate::cognitive::metrics::TerminalOutcome::Interrupted => None,
            // An error that bypassed the loop's classification (planning
            // failure, a fatal loop error). Not re-classified here: for a
            // reason it does not recognise `FailureMode::classify` falls back
            // to the iteration-cap counters, which named a 401 at planning
            // `MAX_ITERATIONS`. `UNKNOWN` is the honest tag.
            (None, Err(_)) => Some(super::failure_mode::FailureKind::Unknown.tag().to_string()),
            (None, Ok(())) => None,
        };
        let (tool_calls, errors_total, errors_recovered, first_verification_passed) = self
            .current_checkpoint
            .as_ref()
            .map(|cp| {
                let first_verification = cp
                    .tool_calls
                    .iter()
                    .find(|tc| {
                        super::tool_dispatch::tool_call_is_verification(
                            &tc.tool_name,
                            &tc.arguments,
                        )
                    })
                    .map(|tc| tc.success);
                (
                    cp.tool_calls.len(),
                    cp.errors.len(),
                    cp.errors.iter().filter(|e| e.recovered).count(),
                    first_verification,
                )
            })
            .unwrap_or((0, 0, 0, None));
        // The gate's own report is also verification evidence: when it ran
        // but no verification-shaped tool call did, the first check the run
        // saw is unknown, yet the final verdict still exists.
        let final_verification_passed = self
            .credited_verification_summary()
            .map(|(passed, _)| passed);
        crate::cognitive::metrics::TerminalRunStats {
            outcome,
            failure_mode,
            loop_turns: self.loop_control.current_iteration(),
            tool_calls,
            errors_total,
            errors_recovered,
            first_verification_passed,
            final_verification_passed,
            llm_total_tokens: self.cumulative_token_usage.total as u64,
        }
    }

    /// Write the run's terminal outcome to the learning stores, once per run:
    /// one performance snapshot (with the real outcome and failure-mode tag)
    /// and the improvement-engine state. Called on EVERY exit of the
    /// execution loop; before this, only `complete_checkpoint` wrote either,
    /// so failed, timed-out and interrupted runs vanished from the statistics
    /// (val083: 8 of 13 runs had a snapshot, all 100% success).
    #[cfg_attr(not(feature = "self-improvement"), allow(unused_variables))]
    pub(super) fn record_terminal_telemetry(&mut self, result: &Result<()>) {
        if self.terminal_telemetry_recorded {
            return;
        }
        self.terminal_telemetry_recorded = true;
        let data_dir = self.learning_data_dir();

        #[cfg(feature = "self-improvement")]
        {
            let stats = self.terminal_run_stats(result);
            let snapshot =
                crate::cognitive::metrics::PerformanceSnapshot::from_terminal_run(&stats);
            let store = crate::cognitive::metrics::MetricsStore::with_path(
                data_dir.join("metrics").join("snapshots.jsonl"),
            );
            match store.record(&snapshot) {
                Ok(()) => info!(
                    "Recorded performance snapshot ({:?}, {} turns, {} LLM tokens)",
                    stats.outcome, stats.loop_turns, stats.llm_total_tokens
                ),
                Err(e) => tracing::warn!("Failed to record performance metrics: {}", e),
            }
        }

        let engine_path = data_dir.join("improvement_engine.json");
        match self.self_improvement.save(&engine_path) {
            Ok(()) => info!("Saved self-improvement engine state"),
            Err(e) => tracing::warn!("Failed to save improvement engine state: {}", e),
        }
    }

    pub(super) fn build_learning_hint(&self, task_prompt: &str) -> Option<String> {
        if task_prompt.trim().is_empty() {
            return None;
        }

        let mut hints: Vec<String> = Vec::new();

        let preferred_tools: Vec<String> = self
            .self_improvement
            .best_tools_for(task_prompt)
            .into_iter()
            .filter(|(_, score)| *score >= 0.6)
            .take(3)
            .map(|(tool, score)| format!("{} ({:.0}% confidence)", tool, score * 100.0))
            .collect();
        if !preferred_tools.is_empty() {
            hints.push(format!(
                "Prefer previously effective tools: {}.",
                preferred_tools.join(", ")
            ));
        }

        let warnings = self
            .self_improvement
            .check_for_errors("task_execution", task_prompt);
        if let Some(warning) = warnings.into_iter().next().filter(|w| w.likelihood >= 0.6) {
            hints.push(format!(
                "Avoid recurring {} pattern (likelihood {:.0}%).",
                warning.error_type,
                warning.likelihood * 100.0
            ));
            if !warning.prevention.is_empty() {
                hints.push(format!(
                    "Prevention guidance: {}.",
                    warning
                        .prevention
                        .into_iter()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }

        if hints.is_empty() {
            None
        } else {
            Some(format!(
                "Self-improvement guidance from prior outcomes:\n- {}",
                hints.join("\n- ")
            ))
        }
    }

    /// Reflect on a completed step: record lessons, update learner, inject hints
    pub(super) async fn reflect_on_step(&mut self, step: usize) {
        self.cognitive_state.set_phase(CyclePhase::Reflect);

        // 1. Check for verification failures in the last step and record lessons
        if let Some(ref checkpoint) = self.current_checkpoint {
            let step_errors: Vec<_> = checkpoint
                .errors
                .iter()
                .filter(|e| e.step == step)
                .collect();
            for error in &step_errors {
                if error.recovered {
                    self.cognitive_state.episodic_memory.what_worked(
                        "error_recovery",
                        &format!("Step {}: recovered from: {}", step, error.error),
                    );
                    // Record recovery strategy in improvement engine
                    self.self_improvement.record_error(
                        &error.error,
                        "step_error",
                        self.learning_context(),
                        &format!("step_{}", step),
                        Some("automatic_recovery".to_string()),
                    );
                } else {
                    self.cognitive_state.episodic_memory.what_failed(
                        "step_execution",
                        &format!("Step {}: unrecovered error: {}", step, error.error),
                    );
                }
            }
        }

        // 2. Query tool learner for recommendations and inject hint into working memory
        let context = self.current_task_context.clone();
        let best_tools = self.self_improvement.best_tools_for(&context);
        if let Some((tool, score)) = best_tools.first() {
            if *score >= 0.7 {
                let hint = format!(
                    "Based on learning: tool '{}' has {:.0}% effectiveness for this context",
                    tool,
                    score * 100.0
                );
                self.cognitive_state.working_memory.add_fact(&hint);
            }
        }

        // 3. LLM Functional Reflection (Every 5 steps)
        if step > 0 && step.is_multiple_of(5) {
            info!("Triggering functional reflection for step {}", step);
            let reflection_prompt = format!(
                "You have just completed step {}. Reflect on the last 5 steps.
                What did you learn? What would you do differently? What surprised you?
                Be concise. Output your reflection as a single paragraph.",
                step
            );

            let mut messages = self.messages.clone();
            messages.push(crate::api::types::Message::user(reflection_prompt));

            // Bounded side call: a one-paragraph reflection needs neither
            // the session's reasoning effort nor its 64k output budget.
            if let Ok(response) = self
                .client
                .side_chat(
                    messages,
                    crate::api::client::SideCall::new("reflection")
                        .max_tokens(1024)
                        .time_cap_secs(60),
                )
                .await
            {
                // Account the reflection call's token usage against the budget.
                // Delta-add (never total = input + output): after a resume,
                // `total` carries the restored prior-run budget whose
                // input/output split was not persisted.
                self.sync_api_usage();

                if let Some(choice) = response.choices.first() {
                    let text = choice.message.content.clone();
                    if !text.is_empty() {
                        let lesson = crate::cognitive::Lesson {
                            category: crate::cognitive::LessonCategory::Discovery,
                            content: format!("Reflection at step {}: {}", step, text),
                            context: "".to_string(),
                            tags: vec!["reflection".to_string()],
                            timestamp: chrono::Utc::now(),
                        };
                        self.cognitive_state.episodic_memory.record_lesson(lesson);
                        self.cognitive_state
                            .working_memory
                            .add_fact(&format!("Reflection (Step {}): {}", step, text));
                    }
                }
            }
        }

        // 4. Mark the plan step complete with notes
        let notes = format!("Step {} completed", step);
        self.cognitive_state
            .working_memory
            .complete_step(step, Some(notes));
        self.cognitive_state
            .complete_operational_step(step, Some(format!("Step {} completed", step)));

        self.cognitive_state.set_phase(CyclePhase::Do);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/learning/learning_test.rs"]
mod tests;

/// How the run ended, for the performance snapshot. `result` is the loop's
/// final result (a failure verdict on an `Ok` exit has already been turned
/// into an error by `failure_verdict_as_error`).
#[cfg(feature = "self-improvement")]
pub(super) fn classify_terminal_outcome(
    result: &Result<()>,
    cancelled: bool,
    verdict: Option<&super::failure_mode::FailureMode>,
) -> crate::cognitive::metrics::TerminalOutcome {
    use super::failure_mode::FailureKind;
    use crate::cognitive::metrics::TerminalOutcome;
    use crate::errors::AgentError;

    let err = match result {
        Ok(()) => {
            return match verdict {
                Some(fm) if !fm.kind.is_nonfailure() => TerminalOutcome::Failed,
                _ => TerminalOutcome::Completed,
            }
        }
        Err(e) => e,
    };
    for cause in err.chain() {
        if let Some(agent_error) = cause.downcast_ref::<AgentError>() {
            match agent_error {
                // The internal run timeout cancels with this reason.
                AgentError::CancelledWithReason(reason) if reason == "timeout" => {
                    return TerminalOutcome::Timeout
                }
                AgentError::Cancelled
                | AgentError::Terminated(_)
                | AgentError::CancelledWithReason(_) => return TerminalOutcome::Interrupted,
                _ => {}
            }
        }
        if cause
            .downcast_ref::<crate::api::client::WallClockBudgetExceeded>()
            .is_some()
            || cause
                .downcast_ref::<crate::api::client::CallTimeBudgetExceeded>()
                .is_some()
        {
            return TerminalOutcome::Timeout;
        }
        if cause
            .downcast_ref::<crate::api::client::UsageBudgetExceeded>()
            .is_some()
        {
            return TerminalOutcome::BudgetStop;
        }
    }
    match verdict.map(|fm| &fm.kind) {
        Some(FailureKind::Timeout | FailureKind::CallTimeCap) => TerminalOutcome::Timeout,
        Some(FailureKind::BudgetExhausted) => TerminalOutcome::BudgetStop,
        _ if cancelled => TerminalOutcome::Interrupted,
        _ => TerminalOutcome::Failed,
    }
}

#[cfg(all(test, feature = "self-improvement"))]
mod terminal_outcome_tests {
    use super::*;
    use crate::cognitive::metrics::TerminalOutcome;

    fn verdict(
        kind: super::super::failure_mode::FailureKind,
    ) -> super::super::failure_mode::FailureMode {
        super::super::failure_mode::FailureMode {
            kind,
            evidence: String::new(),
            advice: String::new(),
            restored_files: Vec::new(),
        }
    }

    #[test]
    fn every_terminal_disposition_maps_to_its_own_outcome() {
        use super::super::failure_mode::FailureKind;
        use crate::errors::AgentError;
        let ok: Result<()> = Ok(());
        assert_eq!(
            classify_terminal_outcome(&ok, false, Some(&verdict(FailureKind::NoChange))),
            TerminalOutcome::Completed
        );
        let failed: Result<()> = Err(anyhow::anyhow!("Agent failed: Max iterations exceeded"));
        assert_eq!(
            classify_terminal_outcome(&failed, false, Some(&verdict(FailureKind::MaxIterations))),
            TerminalOutcome::Failed
        );
        let wall: Result<()> = Err(anyhow::anyhow!("Wall-clock timeout: 600s >= 600s"));
        assert_eq!(
            classify_terminal_outcome(&wall, false, Some(&verdict(FailureKind::Timeout))),
            TerminalOutcome::Timeout
        );
        let budget: Result<()> = Err(anyhow::anyhow!("Token budget exhausted: 10 >= 5 tokens"));
        assert_eq!(
            classify_terminal_outcome(&budget, false, Some(&verdict(FailureKind::BudgetExhausted))),
            TerminalOutcome::BudgetStop
        );
        let user: Result<()> = Err(AgentError::Cancelled.into());
        assert_eq!(
            classify_terminal_outcome(&user, true, None),
            TerminalOutcome::Interrupted
        );
        let sigterm: Result<()> = Err(AgentError::Terminated("SIGTERM".into()).into());
        assert_eq!(
            classify_terminal_outcome(&sigterm, true, None),
            TerminalOutcome::Interrupted
        );
        let run_timeout: Result<()> = Err(AgentError::CancelledWithReason("timeout".into()).into());
        assert_eq!(
            classify_terminal_outcome(&run_timeout, true, None),
            TerminalOutcome::Timeout
        );
        // A deep abort that surfaced as an untyped error while the shutdown
        // latch is set is still an interruption.
        let deep: Result<()> = Err(anyhow::anyhow!("request aborted"));
        assert_eq!(
            classify_terminal_outcome(&deep, true, None),
            TerminalOutcome::Interrupted
        );
        let planning: Result<()> = Err(anyhow::anyhow!("401 Unauthorized"));
        assert_eq!(
            classify_terminal_outcome(&planning, false, None),
            TerminalOutcome::Failed
        );
    }
}
