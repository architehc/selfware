use anyhow::{Context, Result};
use colored::*;
use tracing::warn;

use super::*;

use super::failure_mode::{FailureKind, FailureMode, RunOutcome};
use super::tui_events::AgentEvent;

enum PlannedToolExecution {
    NoToolCalls,
    Completed,
    Continued,
    Interrupted,
}

/// Distinguishes fresh task execution from checkpoint resume.
/// Controls minor behavioral differences (event emission, progress display,
/// planning transition label) without duplicating the loop body.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    NewTask,
    Resume,
}

pub(super) fn is_fatal_loop_error(error: &anyhow::Error) -> bool {
    let msg = error.to_string();
    msg.contains("READ_LOOP_NO_EDIT")
        || msg.contains("EDIT_FAILURE_LOOP_AFTER_EDIT")
        || msg.contains("VERIFICATION_LOOP_AFTER_EDIT")
        // Repeated fake-complete on a mutation task: the model has demonstrated it
        // will not edit; auto-recovery nudges just multiply the cost (observed 10×).
        || msg.contains("FAKE_COMPLETE_LOOP")
        // NONTERM hard-stop: intended as a terminal abort, but it was neither in
        // this list nor matched by is_no_action_error, so self-healing "recovered"
        // it and retried at the same frozen step ~7× (LOOP-NONTERM-NOTFATAL).
        || msg.contains("NONTERM_PROSE_NO_TOOL")
}

impl Agent {
    fn set_loop_state(&mut self, state: AgentState) -> Result<()> {
        self.loop_control.transition_to(state).map_err(Into::into)
    }

    fn transition_from_planning_to_executing(&mut self) -> Result<()> {
        self.cognitive_state.set_phase(CyclePhase::Do);
        self.set_loop_state(AgentState::Executing { step: 0 })
    }

    async fn execute_planned_tool_calls_if_any(
        &mut self,
        task_description: &str,
        has_tool_calls: bool,
        step_for_reflection: usize,
    ) -> Result<PlannedToolExecution> {
        if !has_tool_calls {
            return Ok(PlannedToolExecution::NoToolCalls);
        }

        let completed = self.execute_pending_tool_calls(task_description).await?;
        if self.is_cancelled() {
            return Ok(PlannedToolExecution::Interrupted);
        }
        if completed {
            return Ok(PlannedToolExecution::Completed);
        }

        self.loop_control.increment_step()?;
        self.reflect_on_step(step_for_reflection).await;
        Ok(PlannedToolExecution::Continued)
    }

    /// Execute a single task end-to-end: plan → execute → verify → complete.
    ///
    /// Resets loop state, creates a checkpoint, builds the context map, then
    /// enters the execution loop. A Ctrl+C handler is installed to allow
    /// graceful cancellation with checkpoint persistence. The task string is
    /// added as a user message and drives all subsequent LLM turns.
    pub async fn run_task(&mut self, task: &str) -> Result<()> {
        // Reset loop state so queued tasks don't inherit the previous
        // task's iteration counter and hit the max-iterations limit.
        self.loop_control.reset_for_task();
        self.clear_failed_tool_attempts();
        self.clear_task_state_memory();
        self.reset_no_action_prompt_state();
        self.total_no_action_prompts = 0;
        self.requirements_audit_done
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.leak_check_done
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.input_census_note = None;
        self.input_census_suspicious.clear();
        self.failed_install_streak = 0;
        self.verification_deadline_directive_done
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.probe_pivot_done
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.probe_command_counts.clear();
        self.reset_failure_mode_counters();
        self.required_task_tools.clear();
        self.cumulative_token_usage = crate::observability::dashboard::TokenUsage::default();
        self.cumulative_cost_usd = 0.0;
        self.task_start_time = std::time::Instant::now();
        // Fresh task → no prior segments; the budget starts at zero.
        self.prior_elapsed_secs = 0;
        // Arm the single-terminal-event guard for this run.
        self.terminal_event_emitted = false;
        self.last_run_failure_mode = None;
        // Capture the set of paths already dirty relative to HEAD so the
        // completion gate can exclude pre-existing uncommitted changes.
        self.capture_baseline_dirty_paths();
        let task_description = task.to_string();
        let available_tool_names: Vec<String> = self
            .tools
            .list()
            .into_iter()
            .map(|tool| tool.name().to_string())
            .collect();
        let explicit_task_tools = super::tool_dispatch::extract_explicit_requested_tools(
            &task_description,
            available_tool_names.iter().map(String::as_str),
        );
        self.required_task_tools = explicit_task_tools.clone();
        for tool_name in &explicit_task_tools {
            self.tools.activate(tool_name);
        }

        // Signal ownership lives solely in `main` (the single SIGINT/SIGTERM
        // handler). It sets the process-global shutdown latch, which
        // `is_cancelled()` observes alongside the agent's programmatic
        // `self.cancelled` token — so the agent no longer installs its own
        // competing signal handler here.

        self.emit_event(AgentEvent::Started);
        self.emit_event(AgentEvent::Status {
            message: "Starting task...".to_string(),
        });

        cli_println!("{}", "🦊 Selfware starting task...".bright_cyan());
        cli_println!("Task: {}", task.bright_white());

        // Always start with a fresh checkpoint for a genuinely new task. When an
        // Agent is reused for a queued follow-up, current_checkpoint is still
        // Some from the previous task, so the new task's checkpoint description
        // (and thus verification/gate) would reference the OLD task. Resetting
        // here ensures each top-level run_task call gets its own checkpoint.
        // Resume/continue_execution is unaffected — it does not go through
        // run_task and keeps its existing checkpoint.
        self.current_checkpoint = None;
        if self.current_checkpoint.is_none() {
            let task_id = uuid::Uuid::new_v4().to_string();
            self.current_checkpoint = Some(TaskCheckpoint::new(task_id, task.to_string()));
        }
        self.log_task_start_event(task);
        let learning_session_id = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let task_learning_context = if explicit_task_tools.is_empty() {
            task_description.clone()
        } else {
            let required_tool_lines = explicit_task_tools
                .iter()
                .map(|tool| format!("- `{}`", tool))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}\n\nThis task explicitly requires these tools before answering:\n{}\nDo not answer until each required tool has been called successfully.",
                task_description, required_tool_lines
            )
        };
        self.start_learning_session(&learning_session_id, &task_learning_context);
        self.cognitive_state.upsert_strategic_goal(
            "strategic-agent-reliability",
            "Improve long-term autonomous task reliability and production readiness",
        );
        self.cognitive_state.set_active_tactical_plan(
            format!("tactical-{}", learning_session_id),
            format!("Execute task: {}", task_description),
            vec![learning_session_id.clone()],
        );
        self.cognitive_state.set_operational_plan(
            learning_session_id.clone(),
            vec![
                "Plan approach and identify files to modify".to_string(),
                "Implement changes".to_string(),
                "Run cargo_check to verify compilation".to_string(),
                "Run cargo_test to verify correctness".to_string(),
                "Review and finalize result".to_string(),
            ],
        );

        // Initialize hierarchical context map: set modality + build L1 tree.
        self.context_map.set_modality_from_task(&task_description);
        self.build_l1_project_tree().await;

        // For Review modality: auto-load L2 skeletons so the model
        // doesn't need to read 100+ files one by one.
        if matches!(
            self.context_map.modality(),
            Some(super::context_map::ContextModality::Review)
        ) {
            self.auto_load_skeletons_for_review().await;
        }

        // Inject task-focused preamble into the system prompt itself
        // so the model sees it BEFORE the tool list, not after.
        let task_type = crate::tools::task_focus::classify_task(&task_description);
        let preamble = task_type.preamble();
        if !preamble.is_empty() && !self.messages.is_empty() && self.messages[0].role == "system" {
            // Extract file paths mentioned in the task for explicit targeting.
            let mentioned_files: Vec<&str> = task_description
                .split_whitespace()
                .filter(|w| w.contains('/') && (w.contains('.') || w.ends_with('/')))
                .collect();
            let file_hint = if !mentioned_files.is_empty() {
                format!(
                    "\nThe user mentioned these files: {}. Start with those.",
                    mentioned_files.join(", ")
                )
            } else {
                String::new()
            };
            let explicit_tool_guidance = if explicit_task_tools.is_empty() {
                String::new()
            } else {
                let activated_tools = explicit_task_tools
                    .iter()
                    .filter_map(|name| self.tools.get(name))
                    .map(|tool| {
                        format!(
                            r#"<tool name="{}">
  <description>{}</description>
  <parameters>{}</parameters>
</tool>"#,
                            tool.name(),
                            tool.description(),
                            tool.schema()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "\n\n## EXPLICIT TOOL REQUIREMENT\nThe user explicitly required these tools for this task: {}.\nYou MUST call each required tool successfully before giving a final answer.\nDo NOT infer results from filenames, paths, or prior knowledge.\nThese tools are activated for this session:\n{}",
                    explicit_task_tools
                        .iter()
                        .map(|tool| format!("`{}`", tool))
                        .collect::<Vec<_>>()
                        .join(", "),
                    activated_tools
                )
            };

            let focus_block = format!(
                "\n\n## TASK FOCUS (READ THIS FIRST)\n{}{}{}\n\nPrimary tools for this task: {}\nUse these tools FIRST. Do NOT start with git_status, context_status, or process_list.\n",
                preamble,
                file_hint,
                explicit_tool_guidance,
                task_type.primary_tools().join(", ")
            );
            let current = self.messages[0].content.to_string();
            self.messages[0] = Message::system(format!("{}{}", focus_block, current));
        }

        let msg = Message::user(task);
        self.memory.add_message(&msg);
        self.messages.push(msg);

        // Input census (loop 7): the environment's data contract, enumerated
        // deterministically at task start. The hidden verifier grades the full
        // contract — the instruction text is a subset (measured: the cargo and
        // bun TB 3.0 misses were fields the instruction never names).
        let census = super::input_census::census_task_inputs(&super::current_project_root());
        self.input_census_suspicious = census.suspicious_identifiers.clone();
        self.input_census_note = census.render();
        if let Some(note) = &self.input_census_note {
            let note_msg = Message::user(format!(
                "<selfware_system_directive>\n{note}\nAccount for every field above: consume it or consciously waive it. Fields the instruction never mentions still count.\n</selfware_system_directive>"
            ));
            self.messages.push(note_msg);
        }

        self.run_execution_loop(&task_description, LoopMode::NewTask)
            .await
    }

    pub(super) async fn run_swarm_task(&mut self, task: &str) -> Result<()> {
        use crate::orchestration::swarm::{create_dev_swarm, AgentRole, SwarmTask};

        let mut swarm = create_dev_swarm();
        let mut agents = swarm.list_agents();
        agents.sort_by_key(|a| std::cmp::Reverse(a.role.priority()));

        cli_println!(
            "{} Swarm initialized: {} agents",
            "🐝".bright_cyan(),
            agents.len()
        );
        for agent in &agents {
            cli_println!(
                "  {} {} ({})",
                "→".bright_black(),
                agent.name.bright_white(),
                agent.role.name().dimmed()
            );
        }

        // Build role-specific sub-tasks and queue them in the swarm in
        // priority order: Architect -> Coder -> Tester -> Reviewer.
        let phases: Vec<(AgentRole, &str, u8)> = vec![
            (
                AgentRole::Architect,
                "Design the architecture and plan the implementation",
                10,
            ),
            (
                AgentRole::Coder,
                "Implement the changes based on the architecture plan",
                8,
            ),
            (
                AgentRole::Tester,
                "Write and run tests to verify the implementation",
                6,
            ),
            (
                AgentRole::Reviewer,
                "Review the code changes for quality and correctness",
                4,
            ),
        ];

        for (role, phase_desc, priority) in &phases {
            let sub_task = SwarmTask::new(format!("{}: {}", phase_desc, task))
                .with_role(*role)
                .with_priority(*priority);
            if let Err(e) = swarm.queue_task(sub_task) {
                tracing::warn!("Failed to queue swarm task: {}", e);
            }
        }

        cli_println!(
            "{} Queued {} phases for orchestrated execution",
            "🐝".bright_cyan(),
            phases.len()
        );

        // Process tasks from the swarm queue in priority order.
        // Each phase uses the specialist agent's system prompt to guide
        // the LLM, then records the result back into the swarm.
        let mut phase_num = 0usize;
        while let Some(task_id) = swarm.next_task() {
            phase_num += 1;

            // Get the task details from active_tasks
            let sub_task = match swarm.get_task(&task_id) {
                Some(t) => t.clone(),
                None => {
                    warn!("Task {} not found after next_task()", task_id);
                    break;
                }
            };

            let assigned = swarm.assign_task(&task_id);

            // Determine the lead agent for this sub-task
            let lead_agent_prompt = if let Some(agent_id) = assigned.first() {
                swarm
                    .get_agent(agent_id)
                    .map(|a| a.system_prompt().to_string())
                    .unwrap_or_default()
            } else {
                // No idle agent matched; fall back to role-based prompt
                sub_task
                    .required_roles
                    .first()
                    .map(|r| r.system_prompt().to_string())
                    .unwrap_or_default()
            };

            let role_name = sub_task
                .required_roles
                .first()
                .map(|r| r.name())
                .unwrap_or("General");

            cli_println!(
                "\n{} Phase {}/{}: {} ({})",
                "🐝".bright_cyan(),
                phase_num,
                phases.len(),
                sub_task.description.bright_white(),
                role_name.bright_yellow()
            );

            // Build a role-specific prompt that includes specialist guidance
            let role_prompt = format!(
                "{}\n\n\
                 You are acting as the {} in a development swarm.\n\
                 Previous phases have already contributed to the conversation context.\n\
                 Focus specifically on your role's responsibilities.\n\
                 After completing your work, verify with cargo_check if you made code changes.\n\n\
                 Task: {}",
                lead_agent_prompt, role_name, sub_task.description
            );

            let result = self.run_task(&role_prompt).await;

            // Record completion back in the swarm
            let (success, result_msg) = match &result {
                Ok(()) => (true, "Phase completed successfully".to_string()),
                Err(e) => (false, e.to_string()),
            };

            for agent_id in &assigned {
                swarm.complete_task(&task_id, agent_id, &result_msg, success);
            }

            if !success {
                warn!(
                    "Swarm phase '{}' failed: {}; continuing with remaining phases",
                    role_name, result_msg
                );
            }
        }

        // Print swarm statistics
        let stats = swarm.stats();
        cli_println!(
            "\n{} Swarm complete: {} agents, avg trust {:.0}%",
            "🐝".bright_green(),
            stats.total_agents,
            stats.average_trust * 100.0
        );

        Ok(())
    }

    /// Build a progress injection message for periodic budget awareness.
    /// Returns `Some(message)` every 5 steps to remind the agent of budget and status.
    fn build_progress_injection(&self, step: usize) -> Option<String> {
        if step == 0 || !(step + 1).is_multiple_of(5) {
            return None;
        }

        let max_steps = self.config.agent.max_iterations;
        let pct = ((step + 1) as f64 / max_steps as f64 * 100.0).min(100.0);

        let has_verification = self
            .current_checkpoint
            .as_ref()
            .map(|cp| {
                cp.tool_calls.iter().any(|tc| {
                    tc.success
                        && (matches!(
                            tc.tool_name.as_str(),
                            "cargo_check" | "cargo_test" | "cargo_clippy"
                        ) || (matches!(tc.tool_name.as_str(), "shell_exec" | "pty_shell")
                            && Self::shell_command_is_verification(&tc.arguments)))
                })
            })
            .unwrap_or(false);

        let verification_status = if has_verification {
            "Verification: PASSED"
        } else {
            "Verification: NOT YET RUN (required before completion)"
        };

        let guidance = if pct < 30.0 {
            "You have plenty of budget remaining. Be thorough — read relevant code, \
             implement carefully, and verify each change."
        } else if pct < 70.0 {
            "Good progress. Continue implementing and make sure to verify with cargo_check/cargo_test."
        } else {
            "You are using most of your budget. Wrap up: ensure all changes compile \
             and tests pass, then provide your final summary."
        };

        Some(format!(
            "[Progress: step {}/{} ({:.0}% budget used) | {}]\n{}",
            step + 1,
            max_steps,
            pct,
            verification_status,
            guidance
        ))
    }

    /// Heuristic: does a shell_exec/pty_shell argument JSON invoke a recognized
    /// test or type-check runner? Used only to soften the "Verification: NOT YET
    /// RUN" progress nudge for non-Rust projects — it does NOT affect any hard
    /// completion gate.
    fn shell_command_is_verification(arguments_json: &str) -> bool {
        let cmd = serde_json::from_str::<serde_json::Value>(arguments_json)
            .ok()
            .and_then(|v| {
                v.get("command")
                    .and_then(|c| c.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default()
            .to_lowercase();
        if cmd.is_empty() {
            return false;
        }
        const RUNNERS: &[&str] = &[
            "pytest",
            "py.test",
            "unittest",
            "npm test",
            "npm run test",
            "yarn test",
            "pnpm test",
            "jest",
            "mocha",
            "vitest",
            "go test",
            "cargo test",
            "cargo check",
            "cargo clippy",
            "tox",
            "make test",
            "make check",
            "gradle test",
            "mvn test",
            "mvn verify",
            "rspec",
            "phpunit",
            "dotnet test",
            "ctest",
        ];
        RUNNERS.iter().any(|r| cmd.contains(r))
    }

    pub async fn analyze(&mut self, path: &str) -> Result<()> {
        let task = Planner::analyze_prompt(path);
        self.run_task(&task).await
    }

    /// Review code in a specific file
    pub async fn review(&mut self, file_path: &str) -> Result<()> {
        // Read the file first
        let content = tokio::fs::read_to_string(file_path)
            .await
            .with_context(|| format!("Failed to read file: {}", file_path))?;

        let task = Planner::review_prompt(file_path, &content);
        self.run_task(&task).await
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> (usize, usize, bool) {
        (
            self.memory.len(),
            self.memory.total_tokens(),
            self.memory.is_near_limit(),
        )
    }

    /// List all saved tasks
    pub fn list_tasks() -> Result<Vec<crate::checkpoint::TaskSummary>> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.list_tasks()
    }

    /// Get status of a specific task
    pub fn task_status(task_id: &str) -> Result<crate::checkpoint::TaskCheckpoint> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.load(task_id)
    }

    /// Delete a saved task
    pub fn delete_task(task_id: &str) -> Result<()> {
        let manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;
        manager.delete(task_id)
    }

    /// Continue execution from current state (for resuming tasks)
    pub async fn continue_execution(&mut self) -> Result<()> {
        // Reset the wall-clock budget baseline to the resume point. Without this,
        // the loop's max-seconds timeout is measured from the ORIGINAL run_task
        // start (or agent creation if resumed in a fresh process), so a task
        // resumed after any real pause could time out immediately (found by
        // GLM-5.2 reviewing task_runner.rs; verified + fixed by Claude).
        self.task_start_time = std::time::Instant::now();
        // Arm the single-terminal-event guard for this resumed run.
        self.terminal_event_emitted = false;
        let task_description = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_description.clone())
            .unwrap_or_default();
        let learning_session_id = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.start_learning_session(&learning_session_id, &task_description);
        if self.cognitive_state.active_tactical_plan.is_none() {
            self.cognitive_state.set_active_tactical_plan(
                format!("tactical-{}", learning_session_id),
                format!("Resume task: {}", task_description),
                vec![learning_session_id.clone()],
            );
        }
        if self.cognitive_state.active_operational_plan.is_none() {
            self.cognitive_state.set_operational_plan(
                learning_session_id.clone(),
                vec![
                    "Resume planning and identify remaining work".to_string(),
                    "Resume implementation".to_string(),
                    "Run cargo_check to verify compilation".to_string(),
                    "Run cargo_test to verify correctness".to_string(),
                    "Review and finalize result".to_string(),
                ],
            );
        }

        self.run_execution_loop(&task_description, LoopMode::Resume)
            .await
    }

    /// Shared execution loop used by both `run_task` and `continue_execution`.
    ///
    /// This is the single source of truth for the Planning → Executing →
    /// ErrorRecovery → Completed/Failed state machine. `mode` controls minor
    /// behavioral differences (event emission, progress tracking, labels).
    /// Run the execution loop and emit exactly one authoritative terminal event
    /// (`Completed` with the final answer, or `Error`) so the TUI / RunSupervisor
    /// can always finalize the run — commit+clear the live stream, show the
    /// final answer, unlock input. Previously most success exits emitted no
    /// terminal event (leaving the stream dangling) and the one that did sent
    /// failure-mode evidence instead of the answer.
    async fn run_execution_loop(&mut self, task_description: &str, mode: LoopMode) -> Result<()> {
        let result = self.run_execution_loop_inner(task_description, mode).await;
        match &result {
            Ok(()) => {
                // Carry the final answer (may be empty if it already streamed
                // live and was committed turn-by-turn — the TUI treats an empty
                // message as "just finalize", avoiding a duplicate/placeholder
                // line).
                let message = self.last_assistant_response.trim().to_string();
                self.emit_terminal_event_once(AgentEvent::Completed { message });
            }
            Err(e) => {
                // A cancellation can be raised from deep in the loop (e.g. a
                // provider call aborted by shutdown), not only at the top-of-loop
                // check. Persist resumable state here so EVERY cancellation exit
                // saves exactly one checkpoint before the terminal event.
                if self.is_cancelled() {
                    if let Err(ce) = self.save_checkpoint(task_description) {
                        warn!("Failed to save cancelled checkpoint: {}", ce);
                    }
                }
                self.emit_terminal_event_once(AgentEvent::Error {
                    message: e.to_string(),
                });
            }
        }
        result
    }

    /// Enforce the hard budget caps (token count, USD cost, wall-clock).
    ///
    /// Returns `Ok(())` when no cap is exceeded. When a cap is crossed the
    /// run is recorded as `Outcome::Partial`, the failure mode is finalized,
    /// and the method bails with the same reason string the inline checks
    /// used previously — so callers see an `Err` containing the canonical
    /// message (e.g. "Token budget exhausted: ...").
    ///
    /// Called both at the top of each loop iteration (before a billable step
    /// runs) **and** after a billable step completes, so a final LLM call that
    /// pushes cumulative usage past a cap stops the run instead of letting the
    /// over-budget response report success.
    pub(super) async fn enforce_hard_budgets(&mut self, task_description: &str) -> Result<()> {
        if let Some(max_budget) = self.config.agent.max_budget_tokens {
            let total = self.cumulative_token_usage.total;
            if total >= max_budget {
                let reason = format!("Token budget exhausted: {} >= {} tokens", total, max_budget);
                warn!("{}", reason);
                self.record_task_outcome(task_description, Outcome::Partial, Some(&reason));
                self.finalize_failure_mode(RunOutcome::Failed {
                    reason: reason.clone(),
                })
                .await;
                anyhow::bail!("{}", reason);
            }
        }
        if let Some(max_cost) = self.config.agent.max_cost_usd {
            if self.cumulative_cost_usd >= max_cost {
                let reason = format!(
                    "Cost budget exhausted: ${:.4} >= ${:.4}",
                    self.cumulative_cost_usd, max_cost
                );
                warn!("{}", reason);
                self.record_task_outcome(task_description, Outcome::Partial, Some(&reason));
                self.finalize_failure_mode(RunOutcome::Failed {
                    reason: reason.clone(),
                })
                .await;
                anyhow::bail!("{}", reason);
            }
        }
        if let Some(max_secs) = self.config.agent.max_wall_secs {
            let elapsed = self.budget_elapsed_secs();
            if elapsed >= max_secs {
                let reason = format!("Wall-clock timeout: {}s >= {}s", elapsed, max_secs);
                warn!("{}", reason);
                self.record_task_outcome(task_description, Outcome::Partial, Some(&reason));
                self.finalize_failure_mode(RunOutcome::Failed {
                    reason: reason.clone(),
                })
                .await;
                anyhow::bail!("{}", reason);
            }
        }
        Ok(())
    }

    async fn run_execution_loop_inner(
        &mut self,
        task_description: &str,
        mode: LoopMode,
    ) -> Result<()> {
        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;
        // Ungated hard backstop: count error-recovery entries between successful
        // tool continuations. Independent of the resilience feature and of the
        // per-branch recovery paths (context-overflow / visual / generic), so a
        // recurring error that keeps returning to Executing terminates instead of
        // burning the entire max_iterations budget.
        let mut consecutive_error_recoveries = 0u32;
        const MAX_CONSECUTIVE_ERROR_RECOVERIES: u32 = 12;

        // Bounded guard: track consecutive planning turns that make no progress
        // (no tool calls / no state transition). Planning does not consume the
        // iteration budget, so without this guard the model could spin forever
        // in the Planning state re-entering planning when the LLM returns plain
        // text with no tool calls. After a small cap, force a transition to
        // Executing so the normal iteration budget and progress guards apply.
        let mut consecutive_planning_no_progress = 0u32;
        const MAX_CONSECUTIVE_PLANNING_NO_PROGRESS: u32 = 4;
        // Planning is the run's first, most fragile turn. A transient provider
        // outage that outlasts the client's per-request retry budget must not
        // instantly kill an unattended run — retry a few times with backoff.
        const MAX_PLANNING_RETRIES: u32 = 3;

        let mut progress = output::TaskProgress::new(&["Planning", "Executing"]);
        if mode == LoopMode::NewTask {
            progress.start_phase();
        }

        let planning_label = if mode == LoopMode::NewTask {
            "Start"
        } else {
            "Resume"
        };

        // Push each iteration-limit warning band (80%, 90%) at most once, instead
        // of every iteration past the threshold (which piled up duplicate system
        // messages and wasted context — GLM-5.2 finding on task_runner.rs).
        let mut last_warned_band = 0u8;
        while let Some(state) = self.loop_control.next_state() {
            // Liveness heartbeat: a turning loop stays healthy on the health
            // endpoint; a hung process stops pinging and goes stale so a
            // systemd/k8s watchdog can restart it.
            crate::supervision::health::record_heartbeat();
            let band = self.loop_control.approaching_limit_band();
            if band > last_warned_band {
                if let Some(warning) = self.loop_control.approaching_limit_warning() {
                    self.messages.push(Message::system(warning));
                }
                last_warned_band = band;
            }
            // Loop 12: one-time verification-deadline directive — at 60% of the
            // iteration budget with no passing verification, converge NOW.
            self.maybe_inject_verification_deadline_directive();
            self.trim_message_history();

            // Surface the current step in the live TUI status bar so a
            // converging run is distinguishable from a stalled one (the TUI had
            // no step signal — step/iteration never reached it). Reuses the
            // existing Status event path; a no-op when there is no TUI emitter.
            self.emit_event(AgentEvent::Status {
                message: format!(
                    "Step {}/{}",
                    self.loop_control.current_step(),
                    self.config.agent.max_iterations
                ),
            });
            // Hard limits: token budget, USD cost, and wall-clock timeout
            self.enforce_hard_budgets(task_description).await?;

            if self.is_cancelled() {
                cli_println!("{}", "\n⚡ Interrupted".bright_yellow());
                self.messages
                    .push(Message::user("[Task interrupted by user]"));
                self.record_task_outcome(
                    task_description,
                    Outcome::Abandoned,
                    Some("Task interrupted by user"),
                );
                // Return a typed cancellation. `run_execution_loop` maps errors
                // to the single authoritative terminal Error event AND saves the
                // resumable checkpoint (one save for every cancellation exit);
                // returning Ok here would emit a misleading Completed event and
                // make headless callers exit 0.
                return Err(crate::errors::AgentError::Cancelled.into());
            }

            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition(planning_label, "Planning");
                    if mode == LoopMode::NewTask {
                        output::phase_transition(planning_label, "Planning");
                        self.emit_event(AgentEvent::Status {
                            message: "Planning...".to_string(),
                        });
                    } else {
                        cli_println!("{}", "📋 Planning...".bright_yellow());
                    }

                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    let has_tool_calls = {
                        let mut planning_attempt = 0u32;
                        loop {
                            match self.plan().await {
                                Ok(has_tool_calls) => break has_tool_calls,
                                Err(e) => {
                                    // Persist resumable state before we risk giving up,
                                    // so the task can be resumed even if planning
                                    // ultimately fails on the first turn.
                                    if let Err(ce) = self.save_checkpoint(task_description) {
                                        warn!("Failed to checkpoint after planning error: {}", ce);
                                    }
                                    planning_attempt += 1;
                                    // A terminal 4xx from the API (e.g. 401
                                    // from a missing/invalid key) can never
                                    // succeed on retry — fail fast so the user
                                    // sees the remediation hint immediately
                                    // instead of after duplicated retries.
                                    let terminal_client_error =
                                        super::assistant_response::is_terminal_api_client_error(&e);
                                    if self.is_cancelled()
                                        || planning_attempt >= MAX_PLANNING_RETRIES
                                        || terminal_client_error
                                    {
                                        if mode == LoopMode::NewTask {
                                            self.emit_event(AgentEvent::Error {
                                                message: format!("Planning failed: {}", e),
                                            });
                                        }
                                        self.record_task_outcome(
                                            task_description,
                                            Outcome::Failure,
                                            Some(&e.to_string()),
                                        );
                                        return Err(e);
                                    }
                                    let backoff = std::time::Duration::from_secs(
                                        2u64.saturating_pow(planning_attempt).min(30),
                                    );
                                    warn!(
                                        "Planning failed (attempt {}/{}): {} — retrying in {:?}",
                                        planning_attempt, MAX_PLANNING_RETRIES, e, backoff
                                    );
                                    tokio::time::sleep(backoff).await;
                                }
                            }
                        }
                    };

                    if self.is_cancelled() {
                        continue;
                    }

                    // A planning response with no tool calls may already BE the
                    // final answer for a plain chat / analysis task. Accept it
                    // right here — under the same sanity gates the execution
                    // path applies (execution.rs:650-687) — instead of falling
                    // through to Executing, which would spend a second streaming
                    // call only for the model to repeat the same answer (the TUI
                    // then renders it twice).
                    if !has_tool_calls {
                        if let Some(answer) = self.planning_answer_ready_to_finalize().await {
                            // The planning call was billable; a crossed cap must
                            // stop the run rather than report success over budget
                            // (same post-step enforcement the Executing arm applies).
                            self.enforce_hard_budgets(task_description).await?;
                            output::final_answer(&answer);
                            self.last_assistant_response = answer;
                            record_state_transition("Planning", "Completed");
                            if mode == LoopMode::NewTask {
                                progress.complete_phase();
                            }
                            self.finalize_natural_completion(task_description).await;
                            if let Err(e) = self.complete_checkpoint() {
                                warn!("Failed to save completed checkpoint: {}", e);
                            }
                            return Ok(());
                        }
                        // Bounded planning guard: if planning returned no tool calls,
                        // count it as a no-progress turn. After a small cap, force a
                        // transition to Executing instead of re-planning forever
                        // (Planning does not consume the iteration budget).
                        consecutive_planning_no_progress += 1;
                        if consecutive_planning_no_progress >= MAX_CONSECUTIVE_PLANNING_NO_PROGRESS
                        {
                            warn!(
                                "Planning made no progress {} consecutive turns — forcing transition to Executing",
                                consecutive_planning_no_progress
                            );
                            consecutive_planning_no_progress = 0;
                            // Inject a directive so the model knows to act.
                            self.messages.push(Message::user(
                                "<selfware_system_directive>\n\
                                 Planning has not produced any tool calls after multiple attempts. \
                                 Stop planning and start executing now: read a file, run a search, \
                                 or take any concrete action.\n\
                                 </selfware_system_directive>"
                                    .to_string(),
                            ));
                            // Fall through to the normal Planning→Executing
                            // transition so the iteration budget applies.
                        }
                    } else {
                        consecutive_planning_no_progress = 0;
                    }

                    record_state_transition("Planning", "Executing");
                    if mode == LoopMode::NewTask {
                        output::phase_transition("Planning", "Executing");
                        self.emit_event(AgentEvent::Status {
                            message: "Executing...".to_string(),
                        });
                        progress.complete_phase();
                    }
                    self.transition_from_planning_to_executing()?;

                    // A planning turn is billable (plan() records tokens + cost).
                    // If it pushed us over a hard cap, stop BEFORE executing the
                    // planned tool calls — those can mutate files. Without this,
                    // the only budget check is at the top of the *next* iteration,
                    // a full tool batch too late.
                    self.enforce_hard_budgets(task_description).await?;

                    match self
                        .execute_planned_tool_calls_if_any(task_description, has_tool_calls, 1)
                        .await
                    {
                        Ok(PlannedToolExecution::Interrupted) => continue,
                        Ok(PlannedToolExecution::Completed) => {
                            record_state_transition("Executing", "Completed");
                            let fm = self.finalize_natural_completion(task_description).await;
                            {
                                let _ = &fm;
                                let message = self.last_assistant_response.trim().to_string();
                                self.emit_terminal_event_once(AgentEvent::Completed { message });
                            }
                            if let Err(e) = self.complete_checkpoint() {
                                warn!("Failed to save completed checkpoint: {}", e);
                            }
                            return Ok(());
                        }
                        Ok(PlannedToolExecution::Continued) => {
                            #[cfg(feature = "resilience")]
                            {
                                recovery_attempts = 0;
                                self.reset_self_healing_retry();
                            }
                            consecutive_error_recoveries = 0;
                            self.rigor_mode = false;
                            self.rigor_directive_injected = false;
                        }
                        Ok(PlannedToolExecution::NoToolCalls) => {}
                        Err(e) => {
                            warn!("Initial execution failed: {}", e);

                            if is_confirmation_error(&e) {
                                record_state_transition("Planning", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(0, e.to_string(), false);
                                }
                                self.set_loop_state(AgentState::Failed {
                                    reason: e.to_string(),
                                })?;
                                continue;
                            }

                            if is_fatal_loop_error(&e) {
                                record_state_transition("Planning", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(0, e.to_string(), false);
                                }
                                self.set_loop_state(AgentState::Failed {
                                    reason: e.to_string(),
                                })?;
                                continue;
                            }

                            // No-action during planning is recoverable — reset and retry
                            if is_no_action_error(&e) {
                                warn!("No-action during planning — resetting and retrying");
                                self.reset_no_action_prompt_state();
                                self.messages.push(Message::user(
                                    "<selfware_system_directive>\n\
                                     Planning failed because you described intent without calling tools. \
                                     Start by reading a relevant file or searching the codebase.\n\
                                     </selfware_system_directive>"
                                        .to_string(),
                                ));
                            }

                            self.cognitive_state
                                .working_memory
                                .fail_step(1, &e.to_string());
                            self.cognitive_state
                                .fail_operational_step(1, &e.to_string());
                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(0, e.to_string(), true);
                            }
                            self.set_loop_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            })?;
                        }
                    }

                    if let Err(e) = self.save_checkpoint(task_description) {
                        warn!("Failed to save checkpoint: {}", e);
                    }
                }
                AgentState::Executing { step } => {
                    let _span = enter_agent_step("Executing", step);

                    // Rigor mode directive injection (Increment 3): when
                    // rigor_mode is active and we haven't yet injected the
                    // careful-mode directive for this episode, inject it now
                    // so the model sees it before its next action. The
                    // directive is injected exactly once per rigor episode.
                    if self.rigor_mode && !self.rigor_directive_injected {
                        self.messages
                            .push(Message::system(Self::careful_mode_directive().to_string()));
                        self.rigor_directive_injected = true;
                    }

                    // The descriptive step line ("📝 Step N → <action>") is printed
                    // inside execute_step_internal once the model's response is known;
                    // a generic "Executing" here would just be uninformative noise.
                    if let Some(task_id) =
                        self.current_checkpoint.as_ref().map(|c| c.task_id.clone())
                    {
                        self.cognitive_state.start_operational_step(
                            &task_id,
                            step + 1,
                            &format!("Execution step {}", step + 1),
                        );
                    }
                    if let Some(progress_msg) = self.build_progress_injection(step) {
                        self.messages.push(Message::system(progress_msg));
                    }
                    if mode == LoopMode::NewTask {
                        let step_progress = ((step + 1) as f64 * 0.1).min(0.9);
                        progress.update_progress(step_progress);
                    }
                    // Phase-2 synthesis: if the model gathered data but can't
                    // produce a text answer, make a tool-free API call.
                    if let Some(ref synthesis_task) = self.pending_synthesis.take() {
                        info!("Running phase-2 synthesis for stuck model");
                        match self.synthesize_answer(synthesis_task).await {
                            Ok(Some(answer)) => {
                                // synthesize_answer is billable. Enforce hard
                                // budgets BEFORE the auto-write branch below —
                                // otherwise an over-budget synthesis writes
                                // generated code to disk (execute_tool_batch)
                                // before ever being stopped.
                                self.enforce_hard_budgets(task_description).await?;
                                // Check if synthesis produced code that should be
                                // written to a file instead of accepted as text.
                                if super::execution::contains_unwritten_code(&answer) {
                                    if let Some((path, code)) =
                                        super::execution::extract_code_and_path(&answer).await
                                    {
                                        info!("Synthesis produced code — auto-writing to {}", path);
                                        let calls: Vec<super::execution::CollectedToolCall> =
                                            vec![(
                                                "file_write".to_string(),
                                                serde_json::json!({"path": path, "content": code})
                                                    .to_string(),
                                                None,
                                            )];
                                        self.consecutive_read_only_steps = 0;
                                        self.seen_read_targets.clear();
                                        self.has_written_any_file = true;
                                        self.terminal_guard_hits = 0;
                                        if let Err(e) = self.execute_tool_batch(calls).await {
                                            warn!("Auto-write from synthesis failed: {}", e);
                                        }
                                        self.messages.push(Message::user(
                                            "<selfware_system_directive>\n\
                                             Code from your response was auto-written to file. \
                                             Now run cargo check or cargo test to verify.\n\
                                             </selfware_system_directive>"
                                                .to_string(),
                                        ));
                                        // Don't complete — let the agent verify
                                        continue;
                                    }
                                    // Has code but can't extract path — skip synthesis completion
                                    info!("Synthesis produced unwritable code — continuing loop");
                                    continue;
                                }
                                // A synthesized answer must clear the same gates as a normal final
                                // answer, otherwise phase-2 synthesis is a backdoor that completes a task
                                // the completion gate would reject.
                                if super::recovery::strip_think_blocks(&answer)
                                    .trim()
                                    .is_empty()
                                {
                                    info!("Synthesis produced an empty answer after stripping think blocks — continuing normal loop");
                                    continue;
                                }
                                if let Some(gate_msg) = self.check_completion_gate().await {
                                    info!(
                                        "Synthesis answer rejected by completion gate: {}",
                                        gate_msg
                                    );
                                    self.messages.push(Message::user(format!(
                                        "<selfware_system_directive>\n{}\n</selfware_system_directive>",
                                        gate_msg
                                    )));
                                    continue;
                                }
                                // Synthesis made a billable LLM call; a crossed
                                // cap must stop the run rather than let an
                                // over-budget synthesized answer report success
                                // (the post-step check on the normal path does
                                // not cover this synthesis-completion branch).
                                self.enforce_hard_budgets(task_description).await?;
                                output::final_answer(&answer);
                                self.last_assistant_response = answer;
                                record_state_transition("Executing", "Completed");
                                if mode == LoopMode::NewTask {
                                    progress.complete_phase();
                                }
                                self.finalize_natural_completion(task_description).await;
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            Ok(None) => {
                                info!("Synthesis returned empty — continuing normal loop");
                            }
                            Err(e) => {
                                warn!("Synthesis failed: {} — continuing normal loop", e);
                            }
                        }
                    }

                    match self.execute_step_with_logging(task_description).await {
                        Ok(completed) => {
                            if self.is_cancelled() {
                                continue;
                            }
                            #[cfg(feature = "resilience")]
                            {
                                recovery_attempts = 0;
                                self.reset_self_healing_retry();
                            }
                            consecutive_error_recoveries = 0;
                            self.rigor_mode = false;
                            self.rigor_directive_injected = false;
                            // A billable step just ran; a crossed cap must stop the run rather than
                            // let a final response that overshot the budget still report success.
                            self.enforce_hard_budgets(task_description).await?;
                            if completed {
                                record_state_transition("Executing", "Completed");
                                if mode == LoopMode::NewTask {
                                    progress.complete_phase();
                                }
                                self.finalize_natural_completion(task_description).await;
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            self.loop_control.increment_step()?;
                            self.reflect_on_step(step + 1).await;

                            if let Err(e) = self.save_checkpoint(task_description) {
                                warn!("Failed to save checkpoint: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);

                            if mode == LoopMode::NewTask {
                                self.emit_event(AgentEvent::Error {
                                    message: format!("Step {} failed: {}", step + 1, e),
                                });
                            }

                            // Confirmation errors (user denied) are truly fatal
                            if is_confirmation_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.set_loop_state(AgentState::Failed {
                                    reason: e.to_string(),
                                })?;
                                continue;
                            }

                            if is_fatal_loop_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.set_loop_state(AgentState::Failed {
                                    reason: e.to_string(),
                                })?;
                                continue;
                            }

                            // No-action errors are recoverable — reset counters
                            // and inject a context nudge so the model can try again.
                            if is_no_action_error(&e) {
                                cli_println!(
                                    "{} {}",
                                    "⚠️ Agent stuck in action loop:".bright_yellow(),
                                    e
                                );
                                cli_println!(
                                    "{}",
                                    "Attempting recovery — injecting context refresh."
                                        .bright_yellow()
                                );
                                self.reset_no_action_prompt_state();
                                self.messages.push(Message::user(
                                    "<selfware_system_directive>\n\
                                     You were stuck describing intent without calling tools. \
                                     Your counters have been reset. Start fresh: read a file, \
                                     run a search, or take any concrete action now.\n\
                                     </selfware_system_directive>"
                                        .to_string(),
                                ));
                            }

                            record_state_transition("Executing", "ErrorRecovery");
                            self.cognitive_state
                                .working_memory
                                .fail_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .fail_operational_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .episodic_memory
                                .what_failed("execution", &e.to_string());

                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(step, e.to_string(), true);
                            }
                            self.set_loop_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            })?;
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    let _span = enter_agent_step("ErrorRecovery", self.loop_control.current_step());

                    self.emit_event(AgentEvent::Status {
                        message: "Recovering from error...".to_string(),
                    });
                    cli_println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);

                    // Hard terminal for recurring errors (any recovery branch).
                    consecutive_error_recoveries += 1;
                    if consecutive_error_recoveries >= MAX_CONSECUTIVE_ERROR_RECOVERIES {
                        warn!(
                            "Error recovery exhausted: {} consecutive recoveries with no successful progress — failing task",
                            consecutive_error_recoveries
                        );
                        record_state_transition("ErrorRecovery", "Failed");
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_error(0, error.to_string(), false);
                        }
                        self.set_loop_state(AgentState::Failed {
                            reason: format!(
                                "Error recovery exhausted after {} consecutive attempts: {}",
                                consecutive_error_recoveries, error
                            ),
                        })?;
                        continue;
                    }

                    // --- Recovery tree (INCREMENT 2) ---
                    // When auto_recovery is enabled, classify the error and
                    // walk the offline-first resolution tree.  If a resolver
                    // produces a concrete RecoveryAction, apply it and retry.
                    // This sits BETWEEN the consecutive_error_recoveries
                    // terminal and the existing nudge/advice logic so the
                    // terminal guard still fires at MAX.
                    #[cfg(feature = "resilience")]
                    if self.config.continuous_work.auto_recovery {
                        let kind = crate::self_healing::recovery_tree::classify(&error);
                        let signal = crate::self_healing::FailureSignal {
                            kind,
                            message: error.clone(),
                        };
                        let ctx = crate::self_healing::RecoveryContext {
                            config: &self.config,
                            message: &error,
                        };
                        let outcome = crate::self_healing::RecoveryTree::with_defaults()
                            .resolve(&signal, &ctx);
                        if let crate::self_healing::ResolutionOutcome::Resolved(directive) = outcome
                        {
                            match self.apply_recovery_action(&directive).await {
                                Ok(true) => {
                                    tracing::info!(
                                        "Recovery tree resolved failure (kind={:?}) — retrying execution",
                                        kind
                                    );
                                    record_state_transition("ErrorRecovery", "Executing");
                                    self.set_loop_state(AgentState::Executing {
                                        step: self.loop_control.current_step(),
                                    })?;
                                    continue;
                                }
                                Ok(false) | Err(_) => {
                                    // Action not handled or failed to apply —
                                    // fall through to existing nudge/advice.
                                }
                            }
                        } else if Self::should_enter_rigor(&outcome) {
                            // Escalate / Unresolvable: auto-recovery could NOT
                            // resolve this failure. Enter rigor mode so the
                            // next attempt gets a careful-mode directive and
                            // a tightened completion gate.
                            if !self.rigor_mode {
                                tracing::warn!(
                                    "Auto-recovery failed (outcome={:?}, kind={:?}) — entering rigor mode",
                                    outcome, kind
                                );
                                self.rigor_mode = true;
                                self.rigor_directive_injected = false;
                            }
                        }
                        // Escalate / Unresolvable with auto_recovery disabled:
                        // fall through to existing nudge/advice behavior unchanged.
                    }

                    #[cfg(feature = "resilience")]
                    let mut recovered = false;
                    #[cfg(not(feature = "resilience"))]
                    let recovered = false;
                    #[cfg(feature = "resilience")]
                    {
                        if recovery_attempts < self.config.continuous_work.max_recovery_attempts {
                            recovery_attempts += 1;
                            recovered = self
                                .try_self_healing_recovery(&error, "run_execution_loop")
                                .await;
                        } else {
                            warn!(
                                "Auto-recovery attempts exhausted ({})",
                                self.config.continuous_work.max_recovery_attempts
                            );
                        }
                    }

                    if recovered {
                        record_state_transition("ErrorRecovery", "Executing");
                        self.set_loop_state(AgentState::Executing {
                            step: self.loop_control.current_step(),
                        })?;
                        continue;
                    }

                    // Context overflow: the message history is too large.
                    // Hard-compress before retrying — adding more messages would
                    // only make things worse.
                    if error.contains("CONTEXT OVERFLOW") || error.contains("Context overflow") {
                        warn!("Context overflow detected — hard-compressing before retry");
                        self.messages = self.compressor.hard_compress(&self.messages);
                        self.trim_message_history();
                    } else if error.contains("Visual assertion failed") {
                        // Visual assertion failure: provide specific recovery guidance
                        warn!("Visual assertion failure detected — entering visual recovery mode");

                        // Use pending visual assertion for smarter recovery if available
                        // First, extract the data we need from the checkpoint
                        let assertion_data = self.current_checkpoint.as_ref().and_then(|cp| {
                            cp.pending_visual_assertion.as_ref().map(|a| {
                                let hash = a
                                    .verification_result
                                    .as_ref()
                                    .map(|vr| vr.screenshot_hash.clone())
                                    .unwrap_or_default();
                                let tool =
                                    a.tool_name.clone().unwrap_or_else(|| "unknown".to_string());
                                let expected =
                                    a.expected.clone().unwrap_or_else(|| "unknown".to_string());
                                let observed =
                                    a.observed.clone().unwrap_or_else(|| "unknown".to_string());
                                (hash, tool, expected, observed)
                            })
                        });

                        // Check for visual stuck loop if we have assertion data with a hash
                        let stuck_recovery =
                            if let Some((hash, tool, _, _)) = assertion_data.as_ref() {
                                if !hash.is_empty() {
                                    self.detect_visual_stuck_loop_advanced(hash, tool, false)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                        // Format stuck loop info using the dedicated handler
                        let stuck_info = stuck_recovery
                            .as_ref()
                            .map(|strategy| self.handle_visual_stuck_loop(strategy));

                        let pending_assertion_info = assertion_data.map(|(_, tool, expected, observed)| {
                            format!(
                                "\nPending assertion details:\n- Tool: {}\n- Expected: {}\n- Observed: {}{}",
                                tool, expected, observed,
                                stuck_info.as_deref().map(|s| format!("\n{}", s)).unwrap_or_default()
                            )
                        });

                        // Clear the pending visual assertion since we're handling the recovery
                        if self.current_checkpoint.is_some() {
                            // Pending assertion preserved for retry — cleared only on successful re-verification
                        }

                        cli_println!(
                            "{}",
                            "🖥️ Visual assertion failed — action did not produce expected UI state"
                                .bright_red()
                        );

                        let recovery_msg = format!(
                            "VISUAL ASSERTION FAILED: The previous action did not produce the expected visual result.\n\n\
                            This is a HARD GATE — the UI state must be correct before continuing.\n\n\
                            RECOVERY STRATEGY:\n\
                            1. Use computer_screen to capture the current state\n\
                            2. Analyze what actually happened vs what was expected\n\
                            3. Try a different approach — the previous action did not work\n\
                            4. Consider: was the element not clickable? Was the window not ready?\n\n\
                            Error: {}\n{}\n{}",
                            error,
                            pending_assertion_info.as_deref().unwrap_or(""),
                            self.cognitive_state.summary()
                        );
                        self.messages.push(Message::user(recovery_msg));
                    } else {
                        // Generic, non-self-recoverable error. Under resilience, once
                        // recovery attempts are exhausted, stop nudging — a generic
                        // "try a different approach" just burns the remaining
                        // iterations to MAX_ITERATIONS. Fail the task instead.
                        #[cfg(feature = "resilience")]
                        if recovery_attempts >= self.config.continuous_work.max_recovery_attempts {
                            warn!(
                                "Auto-recovery exhausted ({} attempts) on a non-recoverable error — failing task instead of nudging",
                                self.config.continuous_work.max_recovery_attempts
                            );
                            record_state_transition("ErrorRecovery", "Failed");
                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(0, error.to_string(), false);
                            }
                            self.set_loop_state(AgentState::Failed {
                                reason: format!("Auto-recovery exhausted: {}", error),
                            })?;
                            continue;
                        }
                        let cognitive_summary = self.cognitive_state.summary();
                        // Derive structured failure-mode advice for the current
                        // recovery situation and inject it so the model gets a
                        // concrete next step instead of only a generic nudge.
                        let failure_advice = self.recovery_failure_advice(&error);
                        self.messages.push(Message::user(format!(
                            "The previous action failed with error: {}. Please try a different approach.\n\n{}{}",
                            error, cognitive_summary, failure_advice
                        )));
                    }

                    record_state_transition("ErrorRecovery", "Executing");
                    self.set_loop_state(AgentState::Executing {
                        step: self.loop_control.current_step(),
                    })?;
                }
                AgentState::Completed => {
                    record_state_transition("Executing", "Completed");
                    if mode == LoopMode::NewTask {
                        progress.complete_phase();
                    }
                    self.finalize_natural_completion(task_description).await;
                    if let Err(e) = self.complete_checkpoint() {
                        warn!("Failed to save completed checkpoint: {}", e);
                    }
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    record_state_transition("Executing", "Failed");
                    if mode == LoopMode::NewTask {
                        progress.fail_phase();
                    }

                    let fm = self
                        .finalize_failure_mode(RunOutcome::Failed {
                            reason: reason.clone(),
                        })
                        .await;
                    self.emit_terminal_event_once(AgentEvent::Error {
                        message: format!("Task aborted ({}): {}", fm.kind.tag(), fm.evidence),
                    });
                    self.record_task_outcome(
                        task_description,
                        Outcome::Failure,
                        Some(&format!("{} [{}]", reason, fm.kind.tag())),
                    );
                    if let Err(e) = self.fail_checkpoint(&reason) {
                        warn!("Failed to save failed checkpoint: {}", e);
                    }
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }
        }

        // Classify the outcome for instrumentation. Use the canonical
        // `FailureMode::classify` so the partial-exit log line matches the
        // structured artifact and CLI banner emitted just below.
        let fm = FailureMode::classify(self, RunOutcome::Partial);
        let detail = format!(
            "Execution stopped: {} (step {})",
            fm.kind.tag(),
            self.loop_control.current_step()
        );
        self.record_task_outcome(task_description, Outcome::Partial, Some(&detail));
        self.finalize_failure_mode(RunOutcome::Partial).await;
        Ok(())
    }

    /// Derive a concrete failure-mode advice string for the current mid-recovery
    /// situation.
    ///
    /// `FailureMode::classify` is designed to run at run-end with a
    /// [`RunOutcome`].  Mid-recovery we don't have a terminal outcome yet, but
    /// we *do* have the error string and all the observational agent state that
    /// `classify` consumes.  This helper constructs a synthetic
    /// `RunOutcome::Failed { reason: error }` and calls `classify` to obtain
    /// the structured verdict, then formats the advice as a recovery guidance
    /// block suitable for injection into the next attempt's message context.
    ///
    /// The classification logic itself is not modified — we only *consume* its
    /// advice here.  If the advice is empty or "-" (e.g. for success), an empty
    /// string is returned so the recovery message stays clean.
    fn recovery_failure_advice(&self, error: &str) -> String {
        let fm = FailureMode::classify(
            self,
            RunOutcome::Failed {
                reason: error.to_string(),
            },
        );
        if fm.advice.is_empty() || fm.advice == "-" {
            return String::new();
        }
        format!(
            "\n\n## Failure mode guidance [{}]\n\
             Evidence: {}\n\
             Advice: {}",
            fm.kind.tag(),
            fm.evidence,
            fm.advice
        )
    }

    /// Build a `FailureMode` for the current run state and surface it on the
    /// CLI. Also writes a `failure_mode.json` artifact next to the agent's
    /// checkpoint directory (best-effort; never fails the run).
    /// Finalize a natural completion: classify the run FIRST, then record the
    /// telemetry outcome from the verdict. Previously every natural-completion
    /// site recorded `Outcome::Success` *before* classification ran, so a no-op
    /// run (NoChange — zero mutating tool calls) was logged as a full success,
    /// inflating the success rate (#43). NoChange (and any future non-Success
    /// non-failure) is recorded as `Partial`: honest completion, but not a real
    /// edit. Returns the classified `FailureMode`.
    async fn finalize_natural_completion(&mut self, task_description: &str) -> FailureMode {
        let fm = self
            .finalize_failure_mode(RunOutcome::NaturalCompletion)
            .await;
        let (outcome, detail) = if fm.kind == FailureKind::Success {
            (Outcome::Success, format!("[green] [{}]", fm.kind.tag()))
        } else {
            // NoChange and other non-failure completions: completed cleanly but
            // changed nothing — don't claim a full success.
            (
                Outcome::Partial,
                format!("no file changes [{}]", fm.kind.tag()),
            )
        };
        self.record_task_outcome(task_description, outcome, Some(&detail));
        fm
    }

    async fn finalize_failure_mode(&mut self, outcome: RunOutcome) -> FailureMode {
        let mode = FailureMode::classify(self, outcome);
        self.last_run_failure_mode = Some(mode.clone());
        // Wire the classified verdict/advice into the recovery path: set it
        // as a pending failure hint so the next execution turn (whether from
        // self-healing recovery, a retry, or a subsequent task) receives the
        // post-mortem guidance as a system hint.
        if !mode.advice.is_empty() && mode.advice != "-" {
            self.pending_failure_hint = Some(format!(
                "Failure mode guidance [{}]: {} — Advice: {}",
                mode.kind.tag(),
                mode.evidence,
                mode.advice
            ));
        }
        // Emit a structured progress event so non-TUI consumers can see the
        // run's terminal state. Successful runs emit `TaskCompleted`; every
        // other classification emits `TaskFailed { reason }`.
        // NoChange is an honest non-failure (its banner is "✅ Completed — no
        // file changes"), so it must emit TaskCompleted, not TaskFailed —
        // keeping the banner, the exit status, and the event stream consistent.
        if mode.kind.is_nonfailure() {
            self.emit_progress(super::progress::ProgressEvent::TaskCompleted {
                outcome: mode.kind.tag().to_string(),
            });
        } else {
            self.emit_progress(super::progress::ProgressEvent::TaskFailed {
                reason: format!("{}: {}", mode.kind.tag(), mode.evidence),
            });
        }
        cli_println!("{}", mode.cli_banner());
        // Best-effort artifact write so the SWE-bench Pro harness can pick it up.
        if let Some(dir) = self.failure_mode_artifact_dir() {
            if let Err(e) = mode.write_artifact(&dir).await {
                warn!(
                    "Failed to write failure_mode.json to {}: {}",
                    dir.display(),
                    e
                );
            }
        }
        mode
    }

    /// Resolve the directory `failure_mode.json` should be written to.
    ///
    /// Order:
    /// 1. `SELFWARE_RESULT_DIR` env var (set by the SWE-bench Pro harness
    ///    next to its `result.json`).
    /// 2. The current task's checkpoint directory under `~/.selfware/...`.
    /// 3. None — silently skip the artifact write.
    fn failure_mode_artifact_dir(&self) -> Option<std::path::PathBuf> {
        if let Ok(dir) = std::env::var("SELFWARE_RESULT_DIR") {
            if !dir.is_empty() {
                return Some(std::path::PathBuf::from(dir));
            }
        }
        let task_id = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())?;
        let home = dirs::home_dir()?;
        Some(home.join(".selfware").join("checkpoints").join(task_id))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/task_runner/task_runner_test.rs"]
mod tests;
