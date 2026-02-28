use anyhow::{Context, Result};
use colored::*;
use std::time::Instant;
use tracing::{debug, info, warn};

use super::*;
use crate::checkpoint::{capture_git_state, CheckpointManager, TaskCheckpoint, TaskStatus};
#[cfg(feature = "self-improvement")]
use crate::cognitive::metrics::{MetricsStore, PerformanceSnapshot};
#[cfg(feature = "resilience")]
use crate::self_healing::ErrorOccurrence;

impl Agent {
    /// Resume a task from a checkpoint
    pub async fn resume(config: Config, task_id: &str) -> Result<Self> {
        let checkpoint_manager =
            CheckpointManager::default_path().context("Failed to initialize checkpoint manager")?;

        let checkpoint = checkpoint_manager
            .load(task_id)
            .with_context(|| format!("Failed to load checkpoint for task: {}", task_id))?;

        // Validate checkpoint integrity before attempting restore.
        // This prevents leaving the agent in a half-restored state if the
        // checkpoint data is inconsistent.
        if checkpoint.current_step > 0 && checkpoint.messages.is_empty() {
            anyhow::bail!(
                "Corrupt checkpoint: step {} but no messages (task: {})",
                checkpoint.current_step,
                task_id
            );
        }

        println!(
            "{} Resuming task: {}",
            "🔄".bright_cyan(),
            checkpoint.task_description.bright_white()
        );
        println!(
            "   Current step: {}, Status: {:?}",
            checkpoint.current_step, checkpoint.status
        );

        // Build all restored state in temporary variables first, then commit
        // atomically to the agent. This prevents partial state if any step fails.
        let restored_messages = checkpoint.messages.clone();
        let mut restored_loop = AgentLoop::new(config.agent.max_iterations);

        // Restore exact loop progress when available.
        // Older checkpoints may not have an iteration value, so keep fallback logic.
        if checkpoint.current_iteration > 0 {
            restored_loop.restore_progress(checkpoint.current_step, checkpoint.current_iteration);
        } else {
            // Backward-compatible restore for legacy checkpoints.
            for _ in 0..checkpoint.current_step {
                restored_loop.next_state(); // consumes one iteration
                restored_loop.increment_step();
            }
            restored_loop.set_state(AgentState::Executing {
                step: checkpoint.current_step,
            });
        }

        let checkpoint_tool_calls = checkpoint.tool_calls.len();

        // Create the agent and commit all restored state at once
        let mut agent = Self::new(config).await?;
        agent.messages = restored_messages;
        agent.loop_control = restored_loop;
        agent.current_checkpoint = Some(checkpoint);
        agent.checkpoint_manager = Some(checkpoint_manager);
        agent.last_checkpoint_tool_calls = checkpoint_tool_calls;
        agent.last_checkpoint_persisted_at = Instant::now();
        agent.checkpoint_persisted_once = true;

        // Set cognitive state to Do phase since we're resuming execution
        agent.cognitive_state.set_phase(CyclePhase::Do);

        info!("Agent resumed from checkpoint with cognitive state in Do phase");

        Ok(agent)
    }

    /// Convert current state to a checkpoint
    pub fn to_checkpoint(&self, task_id: &str, task_description: &str) -> TaskCheckpoint {
        let mut checkpoint = if let Some(ref existing) = self.current_checkpoint {
            existing.clone()
        } else {
            TaskCheckpoint::new(task_id.to_string(), task_description.to_string())
        };

        checkpoint.set_step(self.loop_control.current_step());
        checkpoint.set_iteration(self.loop_control.current_iteration());
        checkpoint.set_messages(self.messages.clone());
        checkpoint.set_estimated_tokens(self.memory.total_tokens());

        // Capture git state
        if let Ok(cwd) = std::env::current_dir() {
            checkpoint.git_checkpoint = capture_git_state(cwd.to_string_lossy().as_ref());
        }

        checkpoint
    }

    /// Save current state to checkpoint
    pub(super) fn save_checkpoint(&mut self, task_description: &str) -> Result<()> {
        if let Some(ref manager) = self.checkpoint_manager {
            if !self.should_persist_checkpoint() {
                debug!("Checkpoint skipped by continuous-work policy");
                return Ok(());
            }

            let task_id = self
                .current_checkpoint
                .as_ref()
                .map(|c| c.task_id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

            let checkpoint = self.to_checkpoint(&task_id, task_description);
            manager.save(&checkpoint)?;
            self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
            self.last_checkpoint_persisted_at = Instant::now();
            self.checkpoint_persisted_once = true;
            self.current_checkpoint = Some(checkpoint);
            #[cfg(feature = "resilience")]
            self.record_self_healing_checkpoint(task_description);
            debug!("Checkpoint saved for task: {}", task_id);
        }
        Ok(())
    }

    pub(super) fn should_persist_checkpoint(&self) -> bool {
        if !self.config.continuous_work.enabled {
            return true;
        }

        if !self.checkpoint_persisted_once {
            return true;
        }

        let tools_interval = self.config.continuous_work.checkpoint_interval_tools;
        let secs_interval = self.config.continuous_work.checkpoint_interval_secs;

        if tools_interval == 0 && secs_interval == 0 {
            return true;
        }

        let current_tool_calls = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.tool_calls.len())
            .unwrap_or(0);
        let tool_calls_elapsed = current_tool_calls.saturating_sub(self.last_checkpoint_tool_calls);
        let time_elapsed = self.last_checkpoint_persisted_at.elapsed().as_secs();

        let reached_tool_interval = tools_interval > 0 && tool_calls_elapsed >= tools_interval;
        let reached_time_interval = secs_interval > 0 && time_elapsed >= secs_interval;

        reached_tool_interval || reached_time_interval
    }

    /// Mark current task as completed
    pub(super) fn complete_checkpoint(&mut self) -> Result<()> {
        // Collect metrics before moving the borrow
        #[cfg(feature = "self-improvement")]
        if let Some(ref checkpoint) = self.current_checkpoint {
            let errors_total = checkpoint.errors.len();
            let errors_recovered = checkpoint.errors.iter().filter(|e| e.recovered).count();
            let tool_calls = checkpoint.tool_calls.len();
            let iterations = checkpoint.current_iteration;
            let tokens = checkpoint.estimated_tokens;
            let task_succeeded = true; // we're in complete_checkpoint

            let snapshot = PerformanceSnapshot::from_checkpoint_data(
                iterations,
                tool_calls,
                errors_total,
                errors_recovered,
                errors_total == 0, // first-try verification = no errors
                tokens,
                task_succeeded,
            );

            let metrics_store = MetricsStore::new();
            if let Err(e) = metrics_store.record(&snapshot) {
                warn!("Failed to record performance metrics: {}", e);
            } else {
                info!(
                    "Recorded performance snapshot ({} tool calls, {} errors)",
                    tool_calls, errors_total
                );
            }
        }

        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Completed);
        }
        if let Some(plan) = self.cognitive_state.active_tactical_plan.as_mut() {
            plan.status = crate::cognitive::StepStatus::Completed;
        }
        if let Some(plan) = self.cognitive_state.active_operational_plan.as_mut() {
            for step in &mut plan.steps {
                if matches!(
                    step.status,
                    crate::cognitive::StepStatus::Pending
                        | crate::cognitive::StepStatus::InProgress
                ) {
                    step.status = crate::cognitive::StepStatus::Completed;
                    if step.notes.is_none() {
                        step.notes = Some("Auto-completed at task finalization".to_string());
                    }
                }
            }
        }

        // Generate final summary of what worked and failed
        // (done outside the borrow of current_checkpoint to avoid double borrow)
        self.reflect_and_learn()?;

        if let Some(ref checkpoint) = self.current_checkpoint {
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
                self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
                self.last_checkpoint_persisted_at = Instant::now();
                self.checkpoint_persisted_once = true;
            }
        }
        Ok(())
    }

    /// Reflect on the task outcome and save global lessons
    pub(super) fn reflect_and_learn(&mut self) -> Result<()> {
        // Extract basic lessons based on error history
        if let Some(checkpoint) = &self.current_checkpoint {
            for error in &checkpoint.errors {
                if error.recovered {
                    self.cognitive_state.episodic_memory.what_worked(
                        "error_recovery",
                        &format!(
                            "Successfully recovered from error at step {}: {}",
                            error.step, error.error
                        ),
                    );
                } else {
                    self.cognitive_state.episodic_memory.what_failed(
                        "task_execution",
                        &format!("Failed to recover from error: {}", error.error),
                    );
                }
            }
        }

        let stats = self.self_improvement.get_stats();
        if let Some(tool_stats) = stats.tool_stats {
            if tool_stats.total_records > 0 {
                self.cognitive_state.episodic_memory.what_worked(
                    "self_improvement",
                    &format!(
                        "Tool learning tracked {} executions across {} tools ({} successful).",
                        tool_stats.total_records,
                        tool_stats.unique_tools,
                        tool_stats.successful_records
                    ),
                );
            }
        }
        if let Some(error_stats) = stats.error_stats {
            if error_stats.total_errors > 0 {
                self.cognitive_state.episodic_memory.what_failed(
                    "self_improvement",
                    &format!(
                        "Observed {} errors with {} learned patterns ({} recovered).",
                        error_stats.total_errors,
                        error_stats.pattern_count,
                        error_stats.recovered_count
                    ),
                );
            }
        }

        let preferred_tools: Vec<String> = self
            .self_improvement
            .best_tools_for(self.learning_context())
            .into_iter()
            .filter(|(_, score)| *score >= 0.6)
            .take(3)
            .map(|(tool, score)| format!("{} ({:.0}% confidence)", tool, score * 100.0))
            .collect();
        if !preferred_tools.is_empty() {
            self.cognitive_state.episodic_memory.what_worked(
                "tool_selection",
                &format!(
                    "Preferred tools for similar tasks: {}",
                    preferred_tools.join(", ")
                ),
            );
        }

        // Save global episodic memory
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware");
        let global_memory_path = data_dir.join("global_episodic_memory.json");

        if let Some(parent) = global_memory_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(&self.cognitive_state.episodic_memory)?;
        std::fs::write(&global_memory_path, content)?;
        info!("Saved global episodic memory");

        // Save self-improvement engine state
        let engine_path = data_dir.join("improvement_engine.json");
        if let Err(e) = self.self_improvement.save(&engine_path) {
            warn!("Failed to save improvement engine state: {}", e);
        } else {
            info!("Saved self-improvement engine state");
        }

        Ok(())
    }

    /// Mark current task as failed
    pub(super) fn fail_checkpoint(&mut self, reason: &str) -> Result<()> {
        if let Some(plan) = self.cognitive_state.active_tactical_plan.as_mut() {
            plan.status = crate::cognitive::StepStatus::Failed;
        }
        self.cognitive_state
            .fail_operational_step(self.loop_control.current_step() + 1, reason);
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Failed);
            checkpoint.log_error(self.loop_control.current_step(), reason.to_string(), false);
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
                self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
                self.last_checkpoint_persisted_at = Instant::now();
                self.checkpoint_persisted_once = true;
            }
        }
        Ok(())
    }

    #[cfg(feature = "resilience")]
    pub(super) fn record_self_healing_checkpoint(&self, task_description: &str) {
        if !self.config.continuous_work.auto_recovery {
            return;
        }

        let state = serde_json::json!({
            "task_description": task_description,
            "current_step": self.loop_control.current_step(),
            "messages": self.messages,
        });

        let checkpoint_id = self.self_healing.checkpoint("agent_loop_checkpoint", state);
        debug!("Self-healing checkpoint saved: {}", checkpoint_id);
    }

    #[cfg(feature = "resilience")]
    pub(super) fn restore_from_self_healing_checkpoint(&mut self) -> bool {
        let Some(state) = self.self_healing.restore(None) else {
            return false;
        };

        let Some(messages_value) = state.get("messages").cloned() else {
            return false;
        };

        let Ok(messages) = serde_json::from_value::<Vec<Message>>(messages_value) else {
            return false;
        };
        self.messages = messages;

        if let Some(step) = state.get("current_step").and_then(|v| v.as_u64()) {
            self.loop_control.set_state(AgentState::Executing {
                step: step as usize,
            });
        }

        true
    }

    #[cfg(feature = "resilience")]
    pub(super) fn try_self_healing_recovery(&mut self, error: &str, context: &str) -> bool {
        if !self.config.continuous_work.auto_recovery {
            return false;
        }

        let occurrence = ErrorOccurrence::new("agent_execution_error", error, context);
        let Some(execution) = self.self_healing.handle_error(occurrence) else {
            return false;
        };

        if !execution.success {
            warn!(
                "Self-healing strategy '{}' failed: {:?}",
                execution.strategy, execution.error
            );
            return false;
        }

        let restored = self.restore_from_self_healing_checkpoint();
        if restored {
            info!(
                "Self-healing recovery '{}' restored agent state (actions: {:?})",
                execution.strategy, execution.actions_executed
            );
        } else {
            info!(
                "Self-healing recovery '{}' succeeded without state restore (actions: {:?})",
                execution.strategy, execution.actions_executed
            );
        }

        true
    }

    /// Call after a successful agent step to reset retry backoff state,
    /// so the next failure starts with a fresh retry count.
    #[cfg(feature = "resilience")]
    pub(super) fn reset_self_healing_retry(&self) {
        self.self_healing
            .reset_retry("agent_execution_error", "run_task");
        self.self_healing
            .reset_retry("agent_execution_error", "continue_execution");
    }
}

#[cfg(test)]
mod tests {
    use crate::checkpoint::{
        TaskCheckpoint, TaskStatus, ToolCallLog, ErrorLog, GitCheckpointInfo,
    };
    use crate::api::types::Message;
    use crate::config::ContinuousWorkConfig;
    use chrono::Utc;

    // =========================================================================
    // TaskCheckpoint creation and data integrity
    // =========================================================================

    #[test]
    fn test_checkpoint_new_has_correct_defaults() {
        let cp = TaskCheckpoint::new("task-1".to_string(), "Fix the bug".to_string());
        assert_eq!(cp.task_id, "task-1");
        assert_eq!(cp.task_description, "Fix the bug");
        assert_eq!(cp.status, TaskStatus::InProgress);
        assert_eq!(cp.current_step, 0);
        assert_eq!(cp.current_iteration, 0);
        assert!(cp.messages.is_empty());
        assert!(cp.memory_entries.is_empty());
        assert!(cp.tool_calls.is_empty());
        assert!(cp.errors.is_empty());
        assert!(cp.git_checkpoint.is_none());
        assert_eq!(cp.estimated_tokens, 0);
    }

    #[test]
    fn test_checkpoint_set_step_updates_version() {
        let mut cp = TaskCheckpoint::new("t1".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_step(5);
        assert_eq!(cp.current_step, 5);
        assert!(cp.version > v0, "version should increment after set_step");
    }

    #[test]
    fn test_checkpoint_set_status_and_iteration() {
        let mut cp = TaskCheckpoint::new("t1".to_string(), "desc".to_string());
        cp.set_status(TaskStatus::Completed);
        assert_eq!(cp.status, TaskStatus::Completed);

        cp.set_iteration(42);
        assert_eq!(cp.current_iteration, 42);
    }

    #[test]
    fn test_checkpoint_log_tool_call() {
        let mut cp = TaskCheckpoint::new("t1".to_string(), "desc".to_string());
        assert!(cp.tool_calls.is_empty());

        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            arguments: r#"{"path":"src/main.rs"}"#.to_string(),
            result: Some("file content here".to_string()),
            success: true,
            duration_ms: Some(150),
        });

        assert_eq!(cp.tool_calls.len(), 1);
        assert_eq!(cp.tool_calls[0].tool_name, "file_read");
        assert!(cp.tool_calls[0].success);
        assert_eq!(cp.tool_calls[0].duration_ms, Some(150));
    }

    #[test]
    fn test_checkpoint_log_error() {
        let mut cp = TaskCheckpoint::new("t1".to_string(), "desc".to_string());
        cp.log_error(3, "compile error in main.rs".to_string(), false);
        cp.log_error(4, "retry succeeded".to_string(), true);

        assert_eq!(cp.errors.len(), 2);
        assert_eq!(cp.errors[0].step, 3);
        assert!(!cp.errors[0].recovered);
        assert_eq!(cp.errors[1].step, 4);
        assert!(cp.errors[1].recovered);
    }

    // =========================================================================
    // Checkpoint serialization/deserialization
    // =========================================================================

    #[test]
    fn test_checkpoint_roundtrip_serialization() {
        let mut cp = TaskCheckpoint::new("ser-test".to_string(), "Serialize me".to_string());
        cp.set_step(3);
        cp.set_iteration(10);
        cp.set_status(TaskStatus::Paused);
        cp.set_messages(vec![
            Message::system("sys prompt"),
            Message::user("hello"),
            Message::assistant("hi there"),
        ]);
        cp.set_estimated_tokens(5000);
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "shell_exec".to_string(),
            arguments: r#"{"command":"cargo test"}"#.to_string(),
            result: Some("All tests passed".to_string()),
            success: true,
            duration_ms: Some(2000),
        });
        cp.log_error(2, "timeout".to_string(), true);
        cp.git_checkpoint = Some(GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "abc123def456".to_string(),
            dirty: true,
            staged_files: vec!["src/lib.rs".to_string()],
            modified_files: vec!["Cargo.toml".to_string()],
        });

        let json = serde_json::to_string_pretty(&cp).expect("serialize");
        let restored: TaskCheckpoint = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.task_id, "ser-test");
        assert_eq!(restored.task_description, "Serialize me");
        assert_eq!(restored.current_step, 3);
        assert_eq!(restored.current_iteration, 10);
        assert_eq!(restored.status, TaskStatus::Paused);
        assert_eq!(restored.messages.len(), 3);
        assert_eq!(restored.estimated_tokens, 5000);
        assert_eq!(restored.tool_calls.len(), 1);
        assert_eq!(restored.errors.len(), 1);
        assert!(restored.errors[0].recovered);
        let git = restored.git_checkpoint.unwrap();
        assert_eq!(git.branch, "main");
        assert!(git.dirty);
        assert_eq!(git.staged_files.len(), 1);
    }

    #[test]
    fn test_checkpoint_deserialization_with_missing_optional_fields() {
        // Simulate a legacy checkpoint without version and current_iteration
        let json = r#"{
            "task_id": "legacy-1",
            "task_description": "old task",
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T01:00:00Z",
            "status": "in_progress",
            "current_step": 5,
            "messages": [],
            "memory_entries": [],
            "estimated_tokens": 100,
            "tool_calls": [],
            "errors": [],
            "git_checkpoint": null
        }"#;

        let cp: TaskCheckpoint = serde_json::from_str(json).expect("deserialize legacy");
        assert_eq!(cp.task_id, "legacy-1");
        assert_eq!(cp.current_step, 5);
        // version defaults to 0 for legacy, current_iteration defaults to 0
        assert_eq!(cp.version, 0);
        assert_eq!(cp.current_iteration, 0);
    }

    // =========================================================================
    // should_persist_checkpoint logic (standalone mirror)
    // =========================================================================

    /// Mirrors the `should_persist_checkpoint` logic for standalone testing.
    fn should_persist(
        continuous_work: &ContinuousWorkConfig,
        persisted_once: bool,
        current_tool_calls: usize,
        last_checkpoint_tool_calls: usize,
        time_elapsed_secs: u64,
    ) -> bool {
        if !continuous_work.enabled {
            return true;
        }
        if !persisted_once {
            return true;
        }
        let tools_interval = continuous_work.checkpoint_interval_tools;
        let secs_interval = continuous_work.checkpoint_interval_secs;
        if tools_interval == 0 && secs_interval == 0 {
            return true;
        }
        let tool_calls_elapsed = current_tool_calls.saturating_sub(last_checkpoint_tool_calls);
        let reached_tool_interval = tools_interval > 0 && tool_calls_elapsed >= tools_interval;
        let reached_time_interval = secs_interval > 0 && time_elapsed_secs >= secs_interval;
        reached_tool_interval || reached_time_interval
    }

    #[test]
    fn test_should_persist_when_continuous_work_disabled() {
        let config = ContinuousWorkConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(should_persist(&config, true, 0, 0, 0));
    }

    #[test]
    fn test_should_persist_first_time() {
        let config = ContinuousWorkConfig::default();
        // First checkpoint should always be persisted
        assert!(should_persist(&config, false, 0, 0, 0));
    }

    #[test]
    fn test_should_persist_when_both_intervals_zero() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 0,
            checkpoint_interval_secs: 0,
            ..Default::default()
        };
        assert!(should_persist(&config, true, 5, 5, 10));
    }

    #[test]
    fn test_should_persist_by_tool_interval() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 5,
            checkpoint_interval_secs: 0,
            ..Default::default()
        };
        // Not enough tool calls
        assert!(!should_persist(&config, true, 3, 0, 0));
        // Exactly at interval
        assert!(should_persist(&config, true, 5, 0, 0));
        // Over interval
        assert!(should_persist(&config, true, 10, 3, 0));
    }

    #[test]
    fn test_should_persist_by_time_interval() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 0,
            checkpoint_interval_secs: 60,
            ..Default::default()
        };
        // Not enough time
        assert!(!should_persist(&config, true, 0, 0, 30));
        // At interval
        assert!(should_persist(&config, true, 0, 0, 60));
        // Over interval
        assert!(should_persist(&config, true, 0, 0, 120));
    }

    // =========================================================================
    // compute_delta / apply_delta
    // =========================================================================

    #[test]
    fn test_checkpoint_delta_roundtrip() {
        let mut base = TaskCheckpoint::new("delta-test".to_string(), "test delta".to_string());
        base.set_step(1);
        base.set_messages(vec![Message::system("sys"), Message::user("q1")]);

        let mut updated = base.clone();
        updated.set_step(3);
        updated.set_status(TaskStatus::Completed);
        updated.set_messages(vec![
            Message::system("sys"),
            Message::user("q1"),
            Message::assistant("a1"),
            Message::user("q2"),
        ]);
        updated.set_estimated_tokens(8000);

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some(), "delta should exist when there are changes");
        let delta = delta.unwrap();
        assert_eq!(delta.task_id, "delta-test");
        assert_eq!(delta.status, Some(TaskStatus::Completed));
        assert_eq!(delta.current_step, Some(3));
        assert_eq!(delta.new_messages.len(), 2); // a1 + q2
        assert_eq!(delta.updated_tokens, Some(8000));

        // Apply delta to base and verify
        let mut restored = base.clone();
        restored.apply_delta(&delta).expect("apply delta");
        assert_eq!(restored.current_step, 3);
        assert_eq!(restored.status, TaskStatus::Completed);
        assert_eq!(restored.messages.len(), 4);
        assert_eq!(restored.estimated_tokens, 8000);
    }

    #[test]
    fn test_checkpoint_delta_no_changes_returns_none() {
        let cp = TaskCheckpoint::new("no-change".to_string(), "same".to_string());
        let delta = cp.compute_delta(&cp);
        assert!(delta.is_none());
    }
}
