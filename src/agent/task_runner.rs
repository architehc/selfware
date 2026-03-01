use anyhow::{Context, Result};
use colored::*;
use tracing::warn;

use super::*;

use super::tui_events::AgentEvent;

impl Agent {
    pub async fn run_task(&mut self, task: &str) -> Result<()> {
        // Reset loop state so queued tasks don't inherit the previous
        // task's iteration counter and hit the max-iterations limit.
        self.loop_control.reset_for_task();
        let task_description = task.to_string();

        let cancel_token = self.cancel_token();
        let ctrl_c_handle = tokio::spawn(async move {
            if let Ok(()) = tokio::signal::ctrl_c().await {
                cancel_token.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("\n🦊 Received shutdown signal. Gracefully stopping agent and saving checkpoint...");
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

        println!("{}", "🦊 Selfware starting task...".bright_cyan());
        println!("Task: {}", task.bright_white());

        // Initialize checkpoint if not resuming
        if self.current_checkpoint.is_none() {
            let task_id = uuid::Uuid::new_v4().to_string();
            self.current_checkpoint = Some(TaskCheckpoint::new(task_id, task.to_string()));
        }
        let learning_session_id = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.start_learning_session(&learning_session_id, &task_description);
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

        let msg = Message::user(task);
        self.memory.add_message(&msg);
        self.messages.push(msg);

        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;

        // Initialize multi-phase progress tracker
        let mut progress = output::TaskProgress::new(&["Planning", "Executing"]);
        progress.start_phase();

        while let Some(state) = self.loop_control.next_state() {
            // Trim message history before each iteration to stay within
            // the token budget.
            self.trim_message_history();

            if self.is_cancelled() {
                println!("{}", "\n⚡ Interrupted".bright_yellow());
                self.messages
                    .push(Message::user("[Task interrupted by user]"));
                self.record_task_outcome(
                    &task_description,
                    Outcome::Abandoned,
                    Some("Task interrupted by user"),
                );
                return Ok(());
            }

            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition("Start", "Planning");
                    output::phase_transition("Start", "Planning");

                    self.emit_event(AgentEvent::Status {
                        message: "Planning...".to_string(),
                    });

                    // Set cognitive state to Plan phase
                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    // Plan returns true if the response contains tool calls
                    let has_tool_calls = match self.plan().await {
                        Ok(has_tool_calls) => has_tool_calls,
                        Err(e) => {
                            self.emit_event(AgentEvent::Error {
                                message: format!("Planning failed: {}", e),
                            });

                            self.record_task_outcome(
                                &task_description,
                                Outcome::Failure,
                                Some(&e.to_string()),
                            );
                            return Err(e);
                        }
                    };

                    // Transition to Do phase
                    record_state_transition("Planning", "Executing");
                    output::phase_transition("Planning", "Executing");

                    self.emit_event(AgentEvent::Status {
                        message: "Executing...".to_string(),
                    });
                    progress.complete_phase(); // Complete planning phase
                    self.cognitive_state.set_phase(CyclePhase::Do);
                    self.loop_control
                        .set_state(AgentState::Executing { step: 0 });

                    // If planning response contained tool calls, execute them now
                    if has_tool_calls {
                        output::step_start(1, "Executing");
                        match self.execute_pending_tool_calls(&task_description).await {
                            Ok(completed) => {
                                if self.is_cancelled() {
                                    continue;
                                }
                                if completed {
                                    record_state_transition("Executing", "Completed");
                                    output::task_completed();
                                    self.record_task_outcome(
                                        &task_description,
                                        Outcome::Success,
                                        None,
                                    );

                                    self.emit_event(AgentEvent::Completed {
                                        message: "Task completed successfully".to_string(),
                                    });

                                    if let Err(e) = self.complete_checkpoint() {
                                        warn!("Failed to save completed checkpoint: {}", e);
                                    }
                                    return Ok(());
                                }
                                #[cfg(feature = "resilience")]
                                {
                                    recovery_attempts = 0;
                                }
                                self.loop_control.increment_step();
                                self.reflect_on_step(1).await;
                            }
                            Err(e) => {
                                warn!("Initial execution failed: {}", e);

                                // Check for confirmation error - these are fatal in non-interactive mode
                                if is_confirmation_error(&e) {
                                    record_state_transition("Planning", "Failed");
                                    if let Some(ref mut checkpoint) = self.current_checkpoint {
                                        checkpoint.log_error(0, e.to_string(), false);
                                    }
                                    self.loop_control.set_state(AgentState::Failed {
                                        reason: e.to_string(),
                                    });
                                    continue;
                                }

                                self.cognitive_state
                                    .working_memory
                                    .fail_step(1, &e.to_string());
                                self.cognitive_state
                                    .fail_operational_step(1, &e.to_string());
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(0, e.to_string(), true);
                                }
                                self.loop_control.set_state(AgentState::ErrorRecovery {
                                    error: e.to_string(),
                                });
                            }
                        }
                    }

                    // Save checkpoint after planning
                    if let Err(e) = self.save_checkpoint(&task_description) {
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
                    // Inject periodic progress/budget-awareness messages
                    if let Some(progress_msg) = self.build_progress_injection(step) {
                        self.messages.push(Message::system(progress_msg));
                    }
                    // Update progress based on step
                    let step_progress = ((step + 1) as f64 * 0.1).min(0.9);
                    progress.update_progress(step_progress);
                    match self.execute_step_with_logging(&task_description).await {
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
                                progress.complete_phase();
                                output::task_completed();
                                self.record_task_outcome(&task_description, Outcome::Success, None);
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            self.loop_control.increment_step();
                            self.reflect_on_step(step + 1).await;

                            // Save checkpoint after each step
                            if let Err(e) = self.save_checkpoint(&task_description) {
                                warn!("Failed to save checkpoint: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);

                            self.emit_event(AgentEvent::Error {
                                message: format!("Step {} failed: {}", step + 1, e),
                            });

                            // Check for confirmation error - these are fatal in non-interactive mode
                            if is_confirmation_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.loop_control.set_state(AgentState::Failed {
                                    reason: e.to_string(),
                                });
                                continue;
                            }

                            record_state_transition("Executing", "ErrorRecovery");

                            // Record failure in cognitive state
                            self.cognitive_state
                                .working_memory
                                .fail_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .fail_operational_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .episodic_memory
                                .what_failed("execution", &e.to_string());

                            // Log error in checkpoint
                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(step, e.to_string(), true);
                            }
                            self.loop_control.set_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            });
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    let _span = enter_agent_step("ErrorRecovery", self.loop_control.current_step());

                    self.emit_event(AgentEvent::Status {
                        message: "Recovering from error...".to_string(),
                    });

                    println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);

                    #[cfg(feature = "resilience")]
                    let mut recovered = false;
                    #[cfg(not(feature = "resilience"))]
                    let recovered = false;
                    #[cfg(feature = "resilience")]
                    {
                        if recovery_attempts < self.config.continuous_work.max_recovery_attempts {
                            recovery_attempts += 1;
                            recovered = self.try_self_healing_recovery(&error, "run_task");
                        } else {
                            warn!(
                                "Auto-recovery attempts exhausted ({})",
                                self.config.continuous_work.max_recovery_attempts
                            );
                        }
                    }

                    if recovered {
                        record_state_transition("ErrorRecovery", "Executing");
                        self.loop_control.set_state(AgentState::Executing {
                            step: self.loop_control.current_step(),
                        });
                        continue;
                    }

                    // Add cognitive context about the error
                    let cognitive_summary = self.cognitive_state.summary();
                    self.messages.push(Message::user(format!(
                        "The previous action failed with error: {}. Please try a different approach.\n\n{}",
                        error, cognitive_summary
                    )));

                    record_state_transition("ErrorRecovery", "Executing");
                    self.loop_control.set_state(AgentState::Executing {
                        step: self.loop_control.current_step(),
                    });
                }
                AgentState::Completed => {
                    record_state_transition("Executing", "Completed");
                    progress.complete_phase();
                    output::task_completed();
                    self.record_task_outcome(&task_description, Outcome::Success, None);
                    if let Err(e) = self.complete_checkpoint() {
                        warn!("Failed to save completed checkpoint: {}", e);
                    }
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    record_state_transition("Executing", "Failed");
                    progress.fail_phase();

                    self.emit_event(AgentEvent::Error {
                        message: format!("Task failed: {}", reason),
                    });

                    println!("{} {}", "❌ Task failed:".bright_red(), reason);
                    self.record_task_outcome(&task_description, Outcome::Failure, Some(&reason));
                    if let Err(e) = self.fail_checkpoint(&reason) {
                        warn!("Failed to save failed checkpoint: {}", e);
                    }
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }

            // Iteration tracking is handled by loop_control.next_state()
            // which increments and checks max_iterations each loop turn.
        }

        self.record_task_outcome(
            &task_description,
            Outcome::Partial,
            Some("Execution stopped before completion"),
        );
        Ok(())
    }

    pub(super) async fn run_swarm_task(&mut self, task: &str) -> Result<()> {
        use crate::orchestration::swarm::{create_dev_swarm, AgentRole, SwarmTask};

        let mut swarm = create_dev_swarm();
        let mut agents = swarm.list_agents();
        agents.sort_by_key(|a| std::cmp::Reverse(a.role.priority()));

        println!(
            "{} Swarm initialized: {} agents",
            "🐝".bright_cyan(),
            agents.len()
        );
        for agent in &agents {
            println!(
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

        println!(
            "{} Queued {} phases for orchestrated execution",
            "🐝".bright_cyan(),
            phases.len()
        );

        // Process tasks from the swarm queue in priority order.
        // Each phase uses the specialist agent's system prompt to guide
        // the LLM, then records the result back into the swarm.
        let mut phase_num = 0usize;
        while let Some(sub_task) = swarm.next_task() {
            phase_num += 1;
            let task_id = sub_task.id.clone();
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

            println!(
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
        println!(
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

        #[cfg(feature = "resilience")]
        let mut recovery_attempts = 0u32;

        while let Some(state) = self.loop_control.next_state() {
            // Trim message history before each iteration to stay within
            // the token budget.
            self.trim_message_history();

            if self.is_cancelled() {
                println!("{}", "\n⚡ Interrupted".bright_yellow());
                self.messages
                    .push(Message::user("[Task interrupted by user]"));
                self.record_task_outcome(
                    &task_description,
                    Outcome::Abandoned,
                    Some("Task interrupted by user"),
                );
                return Ok(());
            }

            match state {
                AgentState::Planning => {
                    let _span = enter_agent_step("Planning", 0);
                    record_state_transition("Resume", "Planning");
                    println!("{}", "📋 Planning...".bright_yellow());
                    self.cognitive_state.set_phase(CyclePhase::Plan);

                    if let Err(e) = self.plan().await {
                        self.record_task_outcome(
                            &task_description,
                            Outcome::Failure,
                            Some(&e.to_string()),
                        );
                        return Err(e);
                    }
                    if self.is_cancelled() {
                        continue;
                    }
                    self.loop_control
                        .set_state(AgentState::Executing { step: 0 });
                    self.cognitive_state.set_phase(CyclePhase::Do);

                    if let Err(e) = self.save_checkpoint(&task_description) {
                        warn!("Failed to save checkpoint: {}", e);
                    }
                }
                AgentState::Executing { step } => {
                    let _span = enter_agent_step("Executing", step);
                    println!(
                        "{} Executing...",
                        format!("📝 Step {}", step + 1).bright_blue()
                    );
                    if let Some(task_id) =
                        self.current_checkpoint.as_ref().map(|c| c.task_id.clone())
                    {
                        self.cognitive_state.start_operational_step(
                            &task_id,
                            step + 1,
                            &format!("Execution step {}", step + 1),
                        );
                    }
                    match self.execute_step_with_logging(&task_description).await {
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
                                output::task_completed();
                                self.record_task_outcome(&task_description, Outcome::Success, None);
                                if let Err(e) = self.complete_checkpoint() {
                                    warn!("Failed to save completed checkpoint: {}", e);
                                }
                                return Ok(());
                            }
                            self.loop_control.increment_step();

                            // Reflect and continue
                            self.reflect_on_step(step + 1).await;

                            if let Err(e) = self.save_checkpoint(&task_description) {
                                warn!("Failed to save checkpoint: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Step failed: {}", e);

                            // Check for confirmation error - these are fatal in non-interactive mode
                            if is_confirmation_error(&e) {
                                record_state_transition("Executing", "Failed");
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_error(step, e.to_string(), false);
                                }
                                self.loop_control.set_state(AgentState::Failed {
                                    reason: e.to_string(),
                                });
                                continue;
                            }

                            record_state_transition("Executing", "ErrorRecovery");
                            self.cognitive_state
                                .working_memory
                                .fail_step(step + 1, &e.to_string());
                            self.cognitive_state
                                .fail_operational_step(step + 1, &e.to_string());

                            if let Some(ref mut checkpoint) = self.current_checkpoint {
                                checkpoint.log_error(step, e.to_string(), true);
                            }
                            self.loop_control.set_state(AgentState::ErrorRecovery {
                                error: e.to_string(),
                            });
                        }
                    }
                }
                AgentState::ErrorRecovery { error } => {
                    let _span = enter_agent_step("ErrorRecovery", self.loop_control.current_step());

                    println!("{} {}", "⚠️ Recovering from error:".bright_red(), error);

                    #[cfg(feature = "resilience")]
                    let mut recovered = false;
                    #[cfg(not(feature = "resilience"))]
                    let recovered = false;
                    #[cfg(feature = "resilience")]
                    {
                        if recovery_attempts < self.config.continuous_work.max_recovery_attempts {
                            recovery_attempts += 1;
                            recovered =
                                self.try_self_healing_recovery(&error, "continue_execution");
                        } else {
                            warn!(
                                "Auto-recovery attempts exhausted ({})",
                                self.config.continuous_work.max_recovery_attempts
                            );
                        }
                    }

                    if recovered {
                        record_state_transition("ErrorRecovery", "Executing");
                        self.loop_control.set_state(AgentState::Executing {
                            step: self.loop_control.current_step(),
                        });
                        continue;
                    }

                    let cognitive_summary = self.cognitive_state.summary();
                    self.messages.push(Message::user(format!(
                        "The previous action failed with error: {}. Please try a different approach.\n\n{}",
                        error, cognitive_summary
                    )));

                    record_state_transition("ErrorRecovery", "Executing");
                    self.loop_control.set_state(AgentState::Executing {
                        step: self.loop_control.current_step(),
                    });
                }
                AgentState::Completed => {
                    record_state_transition("Executing", "Completed");
                    output::task_completed();
                    self.record_task_outcome(&task_description, Outcome::Success, None);
                    if let Err(e) = self.complete_checkpoint() {
                        warn!("Failed to save completed checkpoint: {}", e);
                    }
                    return Ok(());
                }
                AgentState::Failed { reason } => {
                    record_state_transition("Executing", "Failed");
                    println!("{} {}", "❌ Task failed:".bright_red(), reason);
                    self.record_task_outcome(&task_description, Outcome::Failure, Some(&reason));
                    if let Err(e) = self.fail_checkpoint(&reason) {
                        warn!("Failed to save failed checkpoint: {}", e);
                    }
                    anyhow::bail!("Agent failed: {}", reason);
                }
            }

            // Iteration tracking is handled by loop_control.next_state()
        }

        self.record_task_outcome(
            &task_description,
            Outcome::Partial,
            Some("Execution stopped before completion"),
        );
        Ok(())
    }
}
