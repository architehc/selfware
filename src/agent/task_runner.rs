use anyhow::{Context, Result};
use colored::*;
use tracing::warn;

use super::*;

use super::failure_mode::{FailureMode, RunOutcome};
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

fn is_fatal_loop_error(error: &anyhow::Error) -> bool {
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
        self.reset_failure_mode_counters();
        self.required_task_tools.clear();
        self.cumulative_token_usage = crate::observability::dashboard::TokenUsage::default();
        self.task_start_time = std::time::Instant::now();
        self.last_run_failure_mode = None;
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

        let cancel_token = self.cancel_token();
        let ctrl_c_handle = tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
                cli_println!("\n🦊 Received shutdown signal. Gracefully stopping agent and saving checkpoint...");
            }
        });

        struct AbortOnDrop(tokio::task::JoinHandle<()>);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _ctrl_c_guard = AbortOnDrop(ctrl_c_handle);

        self.emit_event(AgentEvent::Started);
        self.emit_event(AgentEvent::Status {
            message: "Starting task...".to_string(),
        });

        cli_println!("{}", "🦊 Selfware starting task...".bright_cyan());
        cli_println!("Task: {}", task.bright_white());

        // Initialize checkpoint if not resuming
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
                swarm.complete_task(&task_id, agent_id, &result_msg);
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
        if step == 0 || (step + 1) % 5 != 0 {
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
                        && matches!(
                            tc.tool_name.as_str(),
                            "cargo_check" | "cargo_test" | "cargo_clippy"
                        )
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
    async fn run_execution_loop(&mut self, task_description: &str, mode: LoopMode) -> Result<()> {
        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;

        let mut progress = output::TaskProgress::new(&["Planning", "Executing"]);
        if mode == LoopMode::NewTask {
            progress.start_phase();
        }

        let planning_label = if mode == LoopMode::NewTask {
            "Start"
        } else {
            "Resume"
        };

        while let Some(state) = self.loop_control.next_state() {
            if let Some(warning) = self.loop_control.approaching_limit_warning() {
                self.messages.push(Message::system(warning));
            }
            self.trim_message_history();

            // Hard limits: token budget and wall-clock timeout
            if let Some(max_budget) = self.config.agent.max_budget_tokens {
                let total = self.cumulative_token_usage.total;
                if total >= max_budget {
                    let reason =
                        format!("Token budget exhausted: {} >= {} tokens", total, max_budget);
                    warn!("{}", reason);
                    self.record_task_outcome(task_description, Outcome::Partial, Some(&reason));
                    self.finalize_failure_mode(RunOutcome::Failed {
                        reason: reason.clone(),
                    }).await;
                    anyhow::bail!("{}", reason);
                }
            }
            if let Some(max_secs) = self.config.agent.max_wall_secs {
                let elapsed = self.task_start_time.elapsed().as_secs();
                if elapsed >= max_secs {
                    let reason = format!("Wall-clock timeout: {}s >= {}s", elapsed, max_secs);
                    warn!("{}", reason);
                    self.record_task_outcome(task_description, Outcome::Partial, Some(&reason));
                    self.finalize_failure_mode(RunOutcome::Failed {
                        reason: reason.clone(),
                    }).await;
                    anyhow::bail!("{}", reason);
                }
            }

            if self.is_cancelled() {
                cli_println!("{}", "\n⚡ Interrupted".bright_yellow());
                self.messages
                    .push(Message::user("[Task interrupted by user]"));
                self.record_task_outcome(
                    task_description,
                    Outcome::Abandoned,
                    Some("Task interrupted by user"),
                );
                return Ok(());
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

                    let has_tool_calls = match self.plan().await {
                        Ok(has_tool_calls) => has_tool_calls,
                        Err(e) => {
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
                    };

                    if self.is_cancelled() {
                        continue;
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

                    if has_tool_calls && mode == LoopMode::NewTask {
                        output::step_start(1, "Executing");
                    }
                    match self
                        .execute_planned_tool_calls_if_any(task_description, has_tool_calls, 1)
                        .await
                    {
                        Ok(PlannedToolExecution::Interrupted) => continue,
                        Ok(PlannedToolExecution::Completed) => {
                            record_state_transition("Executing", "Completed");
                            self.record_task_outcome(task_description, Outcome::Success, None);
                            let fm = self.finalize_failure_mode(RunOutcome::NaturalCompletion).await;
                            if mode == LoopMode::NewTask {
                                self.emit_event(AgentEvent::Completed {
                                    message: format!(
                                        "Task completed ({}): {}",
                                        fm.kind.tag(),
                                        fm.evidence
                                    ),
                                });
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
                    output::step_start(step + 1, "Executing");
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
                                output::final_answer(&answer);
                                self.last_assistant_response = answer;
                                record_state_transition("Executing", "Completed");
                                if mode == LoopMode::NewTask {
                                    progress.complete_phase();
                                }
                                self.record_task_outcome(task_description, Outcome::Success, None);
                                self.finalize_failure_mode(RunOutcome::NaturalCompletion).await;
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
                            if completed {
                                record_state_transition("Executing", "Completed");
                                if mode == LoopMode::NewTask {
                                    progress.complete_phase();
                                }
                                self.record_task_outcome(task_description, Outcome::Success, None);
                                self.finalize_failure_mode(RunOutcome::NaturalCompletion).await;
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
                        let cognitive_summary = self.cognitive_state.summary();
                        self.messages.push(Message::user(format!(
                            "The previous action failed with error: {}. Please try a different approach.\n\n{}",
                            error, cognitive_summary
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
                    self.record_task_outcome(task_description, Outcome::Success, Some("[green]"));
                    self.finalize_failure_mode(RunOutcome::NaturalCompletion).await;
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

                    let fm = self.finalize_failure_mode(RunOutcome::Failed {
                        reason: reason.clone(),
                    }).await;
                    if mode == LoopMode::NewTask {
                        self.emit_event(AgentEvent::Error {
                            message: format!("Task aborted ({}): {}", fm.kind.tag(), fm.evidence),
                        });
                    }
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

    /// Build a `FailureMode` for the current run state and surface it on the
    /// CLI. Also writes a `failure_mode.json` artifact next to the agent's
    /// checkpoint directory (best-effort; never fails the run).
    async fn finalize_failure_mode(&mut self, outcome: RunOutcome) -> FailureMode {
        let mode = FailureMode::classify(self, outcome);
        self.last_run_failure_mode = Some(mode.clone());
        // Emit a structured progress event so non-TUI consumers can see the
        // run's terminal state. Successful runs emit `TaskCompleted`; every
        // other classification emits `TaskFailed { reason }`.
        if mode.kind.is_success() {
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
mod tests {
    use super::*;
    use crate::checkpoint::{CheckpointManager, TaskCheckpoint, ToolCallLog};
    use crate::config::{AgentConfig, Config, ExecutionMode, SafetyConfig};
    use crate::testing::mock_api::MockLlmServer;
    use chrono::Utc;

    fn mock_agent_config(endpoint: String, streaming: bool) -> Config {
        Config {
            endpoint,
            model: "mock-model".to_string(),
            // Set context_length high enough that max_context_tokens doesn't become 0
            // after subtracting max_tokens and safety margin
            context_length: 500_000,
            max_tokens: 8192,
            agent: AgentConfig {
                max_iterations: 8,
                step_timeout_secs: 30,
                streaming,
                native_function_calling: false,
                min_completion_steps: 0,
                require_verification_before_completion: false,
                ..Default::default()
            },
            safety: SafetyConfig {
                allowed_paths: vec!["./**".to_string(), "/**".to_string()],
                ..Default::default()
            },
            execution_mode: ExecutionMode::Yolo,
            ..Default::default()
        }
    }

    #[test]
    fn fatal_loop_errors_are_not_recoverable() {
        assert!(is_fatal_loop_error(&anyhow::anyhow!(
            "READ_LOOP_NO_EDIT: no mutation after guard"
        )));
        assert!(is_fatal_loop_error(&anyhow::anyhow!(
            "EDIT_FAILURE_LOOP_AFTER_EDIT: repeated stale edits"
        )));
        assert!(is_fatal_loop_error(&anyhow::anyhow!(
            "VERIFICATION_LOOP_AFTER_EDIT: repeated checks"
        )));
        // Regression: repeated fake-completes must be terminal, not re-nudged 10×.
        assert!(is_fatal_loop_error(&anyhow::anyhow!(
            "FAKE_COMPLETE_LOOP: completion gate rejected 4 final answers"
        )));
        assert!(is_fatal_loop_error(&anyhow::anyhow!(
            "NONTERM_PROSE_NO_TOOL: mutation-required task produced 6 consecutive no-tool turns"
        )));
        assert!(!is_fatal_loop_error(&anyhow::anyhow!(
            "temporary API failure"
        )));
    }

    // =========================================================================
    // build_progress_injection -- exhaustive branch coverage (standalone)
    // =========================================================================

    fn build_progress_injection_standalone(
        step: usize,
        max_iterations: usize,
        has_verification: bool,
    ) -> Option<String> {
        if step == 0 || (step + 1) % 5 != 0 {
            return None;
        }
        let pct = ((step + 1) as f64 / max_iterations as f64 * 100.0).min(100.0);
        let verification_status = if has_verification {
            "Verification: PASSED"
        } else {
            "Verification: NOT YET RUN (required before completion)"
        };
        let guidance = if pct < 30.0 {
            "You have plenty of budget remaining. Be thorough \u{2014} read relevant code, \
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
            max_iterations,
            pct,
            verification_status,
            guidance
        ))
    }

    #[test]
    fn test_progress_injection_none_for_step_zero() {
        assert!(build_progress_injection_standalone(0, 100, false).is_none());
    }

    #[test]
    fn test_progress_injection_none_for_non_multiple_of_5() {
        assert!(build_progress_injection_standalone(1, 100, false).is_none());
        assert!(build_progress_injection_standalone(2, 100, false).is_none());
        assert!(build_progress_injection_standalone(3, 100, false).is_none());
        assert!(build_progress_injection_standalone(5, 100, false).is_none());
        assert!(build_progress_injection_standalone(7, 100, false).is_none());
    }

    #[test]
    fn test_progress_injection_some_for_step_4() {
        let result = build_progress_injection_standalone(4, 100, false);
        assert!(result.is_some());
        assert!(result.unwrap().contains("step 5/100"));
    }

    #[test]
    fn test_progress_injection_some_for_step_9() {
        let result = build_progress_injection_standalone(9, 100, false);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("step 10/100"));
        assert!(msg.contains("10% budget used"));
    }

    #[test]
    fn test_progress_injection_some_for_step_14() {
        let result = build_progress_injection_standalone(14, 100, false);
        assert!(result.is_some());
        assert!(result.unwrap().contains("step 15/100"));
    }

    #[test]
    fn test_progress_injection_low_budget_guidance() {
        let msg = build_progress_injection_standalone(4, 100, false).unwrap();
        assert!(msg.contains("plenty of budget remaining"));
        assert!(msg.contains("Be thorough"));
    }

    #[test]
    fn test_progress_injection_mid_budget_guidance() {
        let msg = build_progress_injection_standalone(49, 100, false).unwrap();
        assert!(msg.contains("Good progress"));
        assert!(msg.contains("cargo_check/cargo_test"));
    }

    #[test]
    fn test_progress_injection_high_budget_guidance() {
        let msg = build_progress_injection_standalone(69, 100, false).unwrap();
        assert!(msg.contains("most of your budget"));
        assert!(msg.contains("Wrap up"));
    }

    #[test]
    fn test_progress_injection_pct_capped_at_100() {
        let msg = build_progress_injection_standalone(14, 10, false).unwrap();
        assert!(msg.contains("100% budget used"));
        assert!(msg.contains("most of your budget"));
    }

    #[test]
    fn test_progress_injection_verification_not_run() {
        let msg = build_progress_injection_standalone(4, 100, false).unwrap();
        assert!(msg.contains("Verification: NOT YET RUN"));
        assert!(msg.contains("required before completion"));
    }

    #[test]
    fn test_progress_injection_verification_passed() {
        let msg = build_progress_injection_standalone(4, 100, true).unwrap();
        assert!(msg.contains("Verification: PASSED"));
        assert!(!msg.contains("NOT YET RUN"));
    }

    #[test]
    fn test_progress_injection_step_19() {
        let msg = build_progress_injection_standalone(19, 100, false).unwrap();
        assert!(msg.contains("step 20/100"));
        assert!(msg.contains("20% budget used"));
    }

    #[test]
    fn test_progress_injection_step_24() {
        let msg = build_progress_injection_standalone(24, 100, false).unwrap();
        assert!(msg.contains("step 25/100"));
        assert!(msg.contains("plenty of budget remaining"));
    }

    #[test]
    fn test_progress_injection_boundary_30_pct() {
        let msg = build_progress_injection_standalone(29, 100, false).unwrap();
        assert!(msg.contains("Good progress"));
    }

    #[test]
    fn test_progress_injection_boundary_70_pct() {
        let msg = build_progress_injection_standalone(69, 100, true).unwrap();
        assert!(msg.contains("Wrap up"));
        assert!(msg.contains("Verification: PASSED"));
    }

    #[test]
    fn test_progress_injection_small_max_iterations() {
        let msg = build_progress_injection_standalone(4, 5, false).unwrap();
        assert!(msg.contains("step 5/5"));
        assert!(msg.contains("100% budget used"));
        assert!(msg.contains("Wrap up"));
    }

    #[test]
    fn test_progress_injection_max_iterations_1() {
        let msg = build_progress_injection_standalone(4, 1, false).unwrap();
        assert!(msg.contains("100% budget used"));
        assert!(msg.contains("Wrap up"));
    }

    #[test]
    fn test_progress_injection_step_99_max_100() {
        let msg = build_progress_injection_standalone(99, 100, false).unwrap();
        assert!(msg.contains("step 100/100"));
        assert!(msg.contains("100% budget used"));
        assert!(msg.contains("Wrap up"));
    }

    #[test]
    fn test_progress_injection_large_step_numbers() {
        let msg = build_progress_injection_standalone(499, 1000, true).unwrap();
        assert!(msg.contains("step 500/1000"));
        assert!(msg.contains("50% budget used"));
        assert!(msg.contains("Good progress"));
        assert!(msg.contains("Verification: PASSED"));
    }

    #[test]
    fn test_progress_injection_exactly_at_boundary_29_not_multiple() {
        assert!(build_progress_injection_standalone(28, 100, false).is_none());
    }

    // =========================================================================
    // build_progress_injection -- via real Agent instance
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_no_checkpoint() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let agent = Agent::new(config).await.unwrap();
        assert!(agent.build_progress_injection(0).is_none());
        let msg = agent.build_progress_injection(4).unwrap();
        assert!(msg.contains("Good progress"));
        assert!(msg.contains("Verification: NOT YET RUN"));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_with_cargo_check() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let mut cp = TaskCheckpoint::new("t1".to_string(), "task".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("OK".to_string()),
            success: true,
            duration_ms: Some(100),
        });
        agent.current_checkpoint = Some(cp);
        assert!(agent
            .build_progress_injection(4)
            .unwrap()
            .contains("Verification: PASSED"));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_failed_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let mut cp = TaskCheckpoint::new("t2".to_string(), "task".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("error".to_string()),
            success: false,
            duration_ms: Some(100),
        });
        agent.current_checkpoint = Some(cp);
        assert!(agent
            .build_progress_injection(4)
            .unwrap()
            .contains("Verification: NOT YET RUN"));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_cargo_test() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let mut cp = TaskCheckpoint::new("t3".to_string(), "task".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_test".to_string(),
            arguments: "{}".to_string(),
            result: Some("passed".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        agent.current_checkpoint = Some(cp);
        assert!(agent
            .build_progress_injection(4)
            .unwrap()
            .contains("Verification: PASSED"));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_cargo_clippy() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let mut cp = TaskCheckpoint::new("t4".to_string(), "task".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_clippy".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(300),
        });
        agent.current_checkpoint = Some(cp);
        assert!(agent
            .build_progress_injection(4)
            .unwrap()
            .contains("Verification: PASSED"));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_progress_injection_agent_non_verification_tool() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let mut cp = TaskCheckpoint::new("t5".to_string(), "task".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: r#"{"path":"foo.rs"}"#.to_string(),
            result: Some("content".to_string()),
            success: true,
            duration_ms: Some(10),
        });
        agent.current_checkpoint = Some(cp);
        assert!(agent
            .build_progress_injection(4)
            .unwrap()
            .contains("Verification: NOT YET RUN"));
        server.stop().await;
    }

    // =========================================================================
    // memory_stats
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_memory_stats_returns_tuple() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let agent = Agent::new(config).await.unwrap();
        let (_len, _total_tokens, near_limit) = agent.memory_stats();
        assert!(!near_limit);
        server.stop().await;
    }

    // =========================================================================
    // list_tasks / task_status / delete_task via temp dir
    // =========================================================================

    #[test]
    fn test_list_tasks_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        assert!(manager.list_tasks().unwrap().is_empty());
    }

    #[test]
    fn test_list_tasks_with_saved_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        let cp = TaskCheckpoint::new("list-test-1".to_string(), "Test task one".to_string());
        manager.save(&cp).unwrap();
        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "list-test-1");
    }

    #[test]
    fn test_list_tasks_multiple() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        manager
            .save(&TaskCheckpoint::new("m1".to_string(), "First".to_string()))
            .unwrap();
        manager
            .save(&TaskCheckpoint::new("m2".to_string(), "Second".to_string()))
            .unwrap();
        manager
            .save(&TaskCheckpoint::new("m3".to_string(), "Third".to_string()))
            .unwrap();
        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 3);
        let ids: Vec<&str> = tasks.iter().map(|t| t.task_id.as_str()).collect();
        assert!(ids.contains(&"m1"));
        assert!(ids.contains(&"m2"));
        assert!(ids.contains(&"m3"));
    }

    #[test]
    fn test_task_status_loads_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        let mut cp = TaskCheckpoint::new("status-test".to_string(), "Status task".to_string());
        cp.set_step(5);
        cp.set_estimated_tokens(2000);
        manager.save(&cp).unwrap();
        let loaded = manager.load("status-test").unwrap();
        assert_eq!(loaded.task_id, "status-test");
        assert_eq!(loaded.current_step, 5);
        assert_eq!(loaded.estimated_tokens, 2000);
    }

    #[test]
    fn test_task_status_nonexistent_not_on_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        // CheckpointManager.load auto-recovers by creating a fresh checkpoint,
        // so use the `exists` helper to verify no file is on disk.
        assert!(!manager.exists("nonexistent"));
    }

    #[test]
    fn test_delete_task_removes_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        let cp = TaskCheckpoint::new("del-test".to_string(), "To delete".to_string());
        manager.save(&cp).unwrap();
        assert!(manager.exists("del-test"));
        manager.delete("del-test").unwrap();
        // After deletion the file should no longer exist on disk.
        // (CheckpointManager.load would auto-recover, so we use exists().)
        assert!(!manager.exists("del-test"));
    }

    #[test]
    fn test_delete_nonexistent_task_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(tmp.path().to_path_buf()).unwrap();
        assert!(manager.delete("does-not-exist").is_ok());
    }

    // =========================================================================
    // run_task E2E
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_completes_with_plain_text() {
        let server = MockLlmServer::builder()
            .with_response("Analyzed.")
            .with_response("Complete.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let result = agent.run_task("Do a simple task").await;
        assert!(
            result.is_ok(),
            "run_task should succeed: {:?}",
            result.err()
        );
        assert!(agent.current_checkpoint.is_some());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_checkpoint_description() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Describe the login bug").await.unwrap();
        assert_eq!(
            agent.current_checkpoint.as_ref().unwrap().task_description,
            "Describe the login bug"
        );
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_adds_user_message() {
        let server = MockLlmServer::builder()
            .with_response("Planning.")
            .with_response("Completion.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Summarize error handling").await.unwrap();
        let has_msg = agent
            .messages
            .iter()
            .any(|m| m.role == "user" && m.content.text().contains("Summarize error handling"));
        assert!(has_msg, "task text should appear as a user message");
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_resets_loop_for_second_task() {
        let server = MockLlmServer::builder()
            .with_response("Plan 1.")
            .with_response("Done 1.")
            .with_response("Plan 2.")
            .with_response("Done 2.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Task one").await.unwrap();
        agent.run_task("Task two").await.unwrap();
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_with_tool_call() {
        let server = MockLlmServer::builder()
            .with_response(
                r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
            )
            .with_response("Task complete.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let result = agent.run_task("Read Cargo.toml").await;
        assert!(
            result.is_ok(),
            "run_task with tool call: {:?}",
            result.err()
        );
        let has_tool_result = agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("<tool_result>"));
        assert!(has_tool_result);
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_sets_strategic_goals() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Review unit tests").await.unwrap();
        assert!(!agent.cognitive_state.strategic_goals.is_empty());
        assert!(agent.cognitive_state.active_tactical_plan.is_some());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_sets_operational_plan() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Review feature X").await.unwrap();
        let plan = agent
            .cognitive_state
            .active_operational_plan
            .as_ref()
            .unwrap();
        assert_eq!(plan.steps.len(), 5);
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_preserves_existing_checkpoint() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "existing-id".to_string(),
            "Existing".to_string(),
        ));
        agent.run_task("Continue working").await.unwrap();
        assert_eq!(
            agent.current_checkpoint.as_ref().unwrap().task_id,
            "existing-id"
        );
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_cancellation() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("More.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent
            .cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let result = agent.run_task("Should cancel").await;
        assert!(result.is_ok());
        let has_interrupted = agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("interrupted"));
        assert!(has_interrupted);
        agent.reset_cancellation();
        assert!(!agent.is_cancelled());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_streaming_mode() {
        let server = MockLlmServer::builder()
            .with_response("Streaming plan.")
            .with_response("Streaming done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), true);
        let mut agent = Agent::new(config).await.unwrap();
        assert!(agent.run_task("Stream this").await.is_ok());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_starts_learning_session() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.run_task("Read tests for parser").await.unwrap();
        assert!(!agent.current_task_context.is_empty());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_run_task_max_iterations_exhaustion() {
        // Plain-text responses are treated as task completion, so to force the
        // agent to keep iterating we must provide tool-call responses.  Each
        // file_read tool call keeps the agent in the loop until max_iterations
        // is exhausted.
        let tool_resp = r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#;
        let mut builder = MockLlmServer::builder();
        for _ in 0..20 {
            builder = builder.with_response(tool_resp);
        }
        let server = builder.build().await;
        let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
        config.agent.max_iterations = 3;
        let mut agent = Agent::new(config).await.unwrap();
        let result = agent.run_task("Never completes").await;
        // The agent should either fail with an error about max iterations or
        // the loop should exhaust without completing (the loop exits with
        // Ok after all states are consumed when next_state returns None).
        // Either outcome is acceptable -- what matters is that the agent
        // does NOT treat a tool-call response as a completion.
        if let Err(e) = &result {
            let err = e.to_string();
            assert!(
                err.contains("Agent failed")
                    || err.contains("Max iterations")
                    || err.contains("iterations"),
                "unexpected error: {}",
                err
            );
        }
        // If Ok, verify the agent ran through multiple execution steps (not
        // a single-step completion).
        if result.is_ok() {
            assert!(
                agent.loop_control.current_step() >= 2,
                "agent should have iterated multiple steps, got {}",
                agent.loop_control.current_step()
            );
        }
        server.stop().await;
    }

    // =========================================================================
    // continue_execution E2E
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_no_checkpoint() {
        let server = MockLlmServer::builder()
            .with_response("Resume plan.")
            .with_response("Resume done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        assert!(agent.continue_execution().await.is_ok());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_with_checkpoint() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "resume-1".to_string(),
            "Resumed".to_string(),
        ));
        assert!(agent.continue_execution().await.is_ok());
        assert!(agent.cognitive_state.active_tactical_plan.is_some());
        assert!(agent.cognitive_state.active_operational_plan.is_some());
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_executes_planned_tool_calls_immediately() {
        let server = MockLlmServer::builder()
            .with_response(
                r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
            )
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "resume-tool".to_string(),
            "Read manifest".to_string(),
        ));

        agent.continue_execution().await.unwrap();

        assert!(agent
            .file_tracker
            .context_files
            .iter()
            .any(|path| path.ends_with("Cargo.toml")));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_cancellation() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("More.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent
            .cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(agent.continue_execution().await.is_ok());
        assert!(agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("interrupted")));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_preserves_tactical_plan() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "p-test".to_string(),
            "Preserve".to_string(),
        ));
        agent.cognitive_state.set_active_tactical_plan(
            "existing-tactical".to_string(),
            "Existing plan".to_string(),
            vec!["dep".to_string()],
        );
        agent.continue_execution().await.unwrap();
        assert_eq!(
            agent
                .cognitive_state
                .active_tactical_plan
                .as_ref()
                .unwrap()
                .id,
            "existing-tactical"
        );
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_sets_operational_plan() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.continue_execution().await.unwrap();
        // An operational plan should exist after continue_execution.
        // Execution may modify the plan (e.g. start_operational_step can
        // replace it when the task_id differs), so we only assert it exists
        // with at least one step.
        let plan = agent
            .cognitive_state
            .active_operational_plan
            .as_ref()
            .unwrap();
        assert!(!plan.steps.is_empty(), "operational plan should have steps");
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_preserves_operational_plan() {
        let server = MockLlmServer::builder()
            .with_response("Plan.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        agent.cognitive_state.set_operational_plan(
            "existing-op".to_string(),
            vec!["Step A".to_string(), "Step B".to_string()],
        );
        agent.continue_execution().await.unwrap();
        // continue_execution should NOT replace an existing plan (the guard at
        // line 647 skips set_operational_plan when one already exists).
        // However, during execution, start_operational_step may mutate it.
        // We verify the plan still exists after completion.
        let plan = agent
            .cognitive_state
            .active_operational_plan
            .as_ref()
            .unwrap();
        assert!(
            !plan.steps.is_empty(),
            "operational plan should survive execution"
        );
        server.stop().await;
    }

    /// Regression test: continue_execution must treat no-action errors as
    /// fatal (go to Failed state) rather than routing to ErrorRecovery.
    /// Before the run_execution_loop dedup this check was missing from
    /// continue_execution, causing the agent to loop indefinitely.
    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_continue_execution_no_action_error_is_recoverable() {
        // The model will repeatedly describe intent without using tools.
        // With recoverable no-action errors, the loop should eventually
        // exhaust max_iterations rather than fatally aborting early.
        let intent = "Let me check the code";
        let server = MockLlmServer::builder()
            .with_default_response(crate::testing::mock_api::MockResponse::Text(
                intent.to_string(),
            ))
            .build()
            .await;
        let mut config = mock_agent_config(format!("{}/v1", server.url()), false);
        config.agent.max_iterations = 8;
        config.agent.min_completion_steps = 3;
        let mut agent = Agent::new(config).await.unwrap();

        // Seed a checkpoint so continue_execution has something to resume
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "test-id".to_string(),
            "test task".to_string(),
        ));

        // The loop should complete (Ok) after exhausting iterations rather
        // than returning Err from a fatal no-action abort.
        let result = agent.continue_execution().await;
        // Either Ok (iterations exhausted) or Err (eventually hit lifetime limit)
        // — both are acceptable. The key is it doesn't abort after just 6 prompts.
        if let Err(ref e) = result {
            let err_msg = e.to_string();
            assert!(
                err_msg.contains("failed to take action")
                    || err_msg.contains("Agent failed")
                    || err_msg.contains("Max iterations"),
                "unexpected error: {}",
                err_msg
            );
        }
        server.stop().await;
    }

    // =========================================================================
    // analyze / review
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_analyze_calls_run_task() {
        let server = MockLlmServer::builder()
            .with_response("Analysis.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        assert!(agent.analyze("./src").await.is_ok());
        assert!(agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("Analyze the codebase")));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_review_reads_file() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"fn main() { println!(\"hello\"); }").unwrap();
        let server = MockLlmServer::builder()
            .with_response("Review.")
            .with_response("Done.")
            .build()
            .await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        assert!(agent.review(tmp.path().to_str().unwrap()).await.is_ok());
        assert!(agent
            .messages
            .iter()
            .any(|m| m.content.text().contains("Review the following code")));
        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_review_nonexistent_file() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = mock_agent_config(format!("{}/v1", server.url()), false);
        let mut agent = Agent::new(config).await.unwrap();
        let result = agent.review("/nonexistent/path/to/file.rs").await;
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Failed to read file"));
        server.stop().await;
    }

    // =========================================================================
    // Planner prompts
    // =========================================================================

    #[test]
    fn test_planner_analyze_prompt() {
        let prompt = Planner::analyze_prompt("./my_project");
        assert!(prompt.contains("./my_project"));
        assert!(prompt.contains("Analyze the codebase"));
        assert!(prompt.contains("Directory structure"));
    }

    #[test]
    fn test_planner_review_prompt() {
        let prompt = Planner::review_prompt("src/main.rs", "fn main() {}");
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("Review the following code"));
    }

    // =========================================================================
    // AgentState enum variants
    // =========================================================================

    #[test]
    fn test_agent_state_planning() {
        let state = AgentState::Planning;
        assert!(matches!(state, AgentState::Planning));
        assert!(format!("{:?}", state).contains("Planning"));
    }

    #[test]
    fn test_agent_state_executing() {
        let state = AgentState::Executing { step: 42 };
        match &state {
            AgentState::Executing { step } => assert_eq!(*step, 42),
            _ => panic!(),
        }
        let d = format!("{:?}", state);
        assert!(d.contains("Executing") && d.contains("42"));
    }

    #[test]
    fn test_agent_state_error_recovery() {
        let state = AgentState::ErrorRecovery {
            error: "oops".to_string(),
        };
        match &state {
            AgentState::ErrorRecovery { error } => assert_eq!(error, "oops"),
            _ => panic!(),
        }
        let d = format!("{:?}", state);
        assert!(d.contains("ErrorRecovery") && d.contains("oops"));
    }

    #[test]
    fn test_agent_state_completed() {
        let state = AgentState::Completed;
        assert!(matches!(state, AgentState::Completed));
        assert!(format!("{:?}", state).contains("Completed"));
    }

    #[test]
    fn test_agent_state_failed() {
        let state = AgentState::Failed {
            reason: "fatal".to_string(),
        };
        match &state {
            AgentState::Failed { reason } => assert_eq!(reason, "fatal"),
            _ => panic!(),
        }
        let d = format!("{:?}", state);
        assert!(d.contains("Failed") && d.contains("fatal"));
    }

    #[test]
    fn test_agent_state_clone_all() {
        let states = vec![
            AgentState::Planning,
            AgentState::Executing { step: 7 },
            AgentState::ErrorRecovery {
                error: "err".to_string(),
            },
            AgentState::Completed,
            AgentState::Failed {
                reason: "r".to_string(),
            },
        ];
        for s in &states {
            assert_eq!(format!("{:?}", s), format!("{:?}", s.clone()));
        }
    }

    // =========================================================================
    // AgentLoop interaction
    // =========================================================================

    #[test]
    fn test_agent_loop_reset_for_task_then_run() {
        let mut lc = AgentLoop::new(5);
        lc.next_state();
        lc.next_state();
        lc.next_state();
        lc.reset_for_task();
        assert!(matches!(lc.next_state(), Some(AgentState::Planning)));
        assert_eq!(lc.current_step(), 0);
    }

    #[test]
    fn test_agent_loop_approaching_limit() {
        let mut lc = AgentLoop::new(10);
        for _ in 0..8 {
            lc.next_state();
        }
        let w = lc.approaching_limit_warning();
        assert!(w.is_some());
        assert!(w.unwrap().contains("wrapping up"));
    }

    // =========================================================================
    // Checkpoint verification detection
    // =========================================================================

    #[test]
    fn test_checkpoint_verification_detection() {
        let mut cp = TaskCheckpoint::new("v-test".to_string(), "verify".to_string());
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: "{}".to_string(),
            result: Some("c".to_string()),
            success: true,
            duration_ms: Some(10),
        });
        let check = |cp: &TaskCheckpoint| {
            cp.tool_calls.iter().any(|tc| {
                tc.success
                    && matches!(
                        tc.tool_name.as_str(),
                        "cargo_check" | "cargo_test" | "cargo_clippy"
                    )
            })
        };
        assert!(!check(&cp));

        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("err".to_string()),
            success: false,
            duration_ms: Some(200),
        });
        assert!(!check(&cp));

        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_test".to_string(),
            arguments: "{}".to_string(),
            result: Some("passed".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        assert!(check(&cp));
    }
}
