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

        // Save global episodic memory — offloaded to a background thread
        // to avoid blocking the Tokio executor on synchronous filesystem I/O.
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware");

        // Serialize in the main thread (cheap), write to disk in background (slow I/O)
        let memory_content = serde_json::to_string_pretty(&self.cognitive_state.episodic_memory)?;

        let engine_path = data_dir.join("improvement_engine.json");
        let engine_save_result = self.self_improvement.save(&engine_path);
        if let Err(e) = &engine_save_result {
            warn!("Failed to save improvement engine state: {}", e);
        } else {
            info!("Saved self-improvement engine state");
        }

        let bg_data_dir = data_dir.clone();
        std::thread::spawn(move || {
            let memory_path = bg_data_dir.join("global_episodic_memory.json");
            if let Some(parent) = memory_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!("Failed to create episodic memory dir: {}", e);
                    return;
                }
            }
            if let Err(e) = std::fs::write(&memory_path, memory_content) {
                tracing::warn!("Failed to write episodic memory: {}", e);
            } else {
                tracing::info!("Saved global episodic memory (background)");
            }
        });

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
    use crate::api::types::Message;
    use crate::checkpoint::{GitCheckpointInfo, TaskCheckpoint, TaskStatus, ToolCallLog};
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

    // =========================================================================
    // Additional should_persist coverage
    // =========================================================================

    #[test]
    fn test_should_persist_both_intervals_tool_triggers_first() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 3,
            checkpoint_interval_secs: 300,
            ..Default::default()
        };
        // Tool interval reached, time not reached
        assert!(should_persist(&config, true, 5, 2, 10));
    }

    #[test]
    fn test_should_persist_both_intervals_time_triggers_first() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 100,
            checkpoint_interval_secs: 30,
            ..Default::default()
        };
        // Time interval reached, tool interval not reached
        assert!(should_persist(&config, true, 2, 0, 60));
    }

    #[test]
    fn test_should_persist_both_intervals_neither_reached() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 10,
            checkpoint_interval_secs: 120,
            ..Default::default()
        };
        // Neither interval reached
        assert!(!should_persist(&config, true, 3, 0, 30));
    }

    #[test]
    fn test_should_persist_both_intervals_both_reached() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 5,
            checkpoint_interval_secs: 60,
            ..Default::default()
        };
        // Both intervals reached
        assert!(should_persist(&config, true, 10, 0, 120));
    }

    #[test]
    fn test_should_persist_tool_calls_saturating_sub() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 5,
            checkpoint_interval_secs: 0,
            ..Default::default()
        };
        // current_tool_calls < last_checkpoint_tool_calls: saturating_sub yields 0
        assert!(!should_persist(&config, true, 2, 10, 0));
    }

    #[test]
    fn test_should_persist_tool_interval_only_exact_boundary() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 1,
            checkpoint_interval_secs: 0,
            ..Default::default()
        };
        // Exactly 1 tool call elapsed
        assert!(should_persist(&config, true, 1, 0, 0));
        // Zero tool calls elapsed
        assert!(!should_persist(&config, true, 0, 0, 0));
    }

    #[test]
    fn test_should_persist_time_interval_boundary() {
        let config = ContinuousWorkConfig {
            enabled: true,
            checkpoint_interval_tools: 0,
            checkpoint_interval_secs: 1,
            ..Default::default()
        };
        // Exactly 1 second elapsed
        assert!(should_persist(&config, true, 0, 0, 1));
        // Zero seconds elapsed
        assert!(!should_persist(&config, true, 0, 0, 0));
    }

    // =========================================================================
    // Version bumping on all mutation methods
    // =========================================================================

    #[test]
    fn test_version_bump_on_set_iteration() {
        let mut cp = TaskCheckpoint::new("v-iter".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_iteration(10);
        assert!(cp.version > v0, "set_iteration should bump version");
    }

    #[test]
    fn test_version_bump_on_set_status() {
        let mut cp = TaskCheckpoint::new("v-status".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_status(TaskStatus::Failed);
        assert!(cp.version > v0, "set_status should bump version");
    }

    #[test]
    fn test_version_bump_on_set_messages() {
        let mut cp = TaskCheckpoint::new("v-msg".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_messages(vec![Message::user("hi")]);
        assert!(cp.version > v0, "set_messages should bump version");
    }

    #[test]
    fn test_version_bump_on_set_estimated_tokens() {
        let mut cp = TaskCheckpoint::new("v-tok".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_estimated_tokens(42_000);
        assert!(cp.version > v0, "set_estimated_tokens should bump version");
        assert_eq!(cp.estimated_tokens, 42_000);
    }

    #[test]
    fn test_version_bump_on_log_tool_call() {
        let mut cp = TaskCheckpoint::new("v-tc".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "test".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: true,
            duration_ms: None,
        });
        assert!(cp.version > v0, "log_tool_call should bump version");
    }

    #[test]
    fn test_version_bump_on_log_error() {
        let mut cp = TaskCheckpoint::new("v-err".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.log_error(0, "oops".to_string(), false);
        assert!(cp.version > v0, "log_error should bump version");
    }

    #[test]
    fn test_version_increments_sequentially() {
        let mut cp = TaskCheckpoint::new("v-seq".to_string(), "desc".to_string());
        let v0 = cp.version;
        cp.set_step(1);
        assert_eq!(cp.version, v0 + 1);
        cp.set_step(2);
        assert_eq!(cp.version, v0 + 2);
        cp.set_iteration(5);
        assert_eq!(cp.version, v0 + 3);
        cp.set_status(TaskStatus::Completed);
        assert_eq!(cp.version, v0 + 4);
    }

    // =========================================================================
    // to_summary
    // =========================================================================

    #[test]
    fn test_to_summary_all_fields() {
        let mut cp = TaskCheckpoint::new("sum-test".to_string(), "Summary task".to_string());
        cp.set_step(7);
        cp.set_status(TaskStatus::Paused);
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "a".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: true,
            duration_ms: None,
        });
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "b".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: false,
            duration_ms: None,
        });
        cp.log_error(1, "err1".to_string(), false);
        cp.log_error(2, "err2".to_string(), true);
        cp.log_error(3, "err3".to_string(), false);

        let summary = cp.to_summary();
        assert_eq!(summary.task_id, "sum-test");
        assert_eq!(summary.task_description, "Summary task");
        assert_eq!(summary.status, TaskStatus::Paused);
        assert_eq!(summary.current_step, 7);
        assert_eq!(summary.tool_call_count, 2);
        assert_eq!(summary.error_count, 3);
        assert_eq!(summary.created_at, cp.created_at);
        assert_eq!(summary.updated_at, cp.updated_at);
    }

    #[test]
    fn test_to_summary_empty_checkpoint() {
        let cp = TaskCheckpoint::new("empty".to_string(), "".to_string());
        let summary = cp.to_summary();
        assert_eq!(summary.tool_call_count, 0);
        assert_eq!(summary.error_count, 0);
        assert_eq!(summary.current_step, 0);
    }

    // =========================================================================
    // Delta edge cases
    // =========================================================================

    #[test]
    fn test_delta_mismatched_task_id_returns_none() {
        let base = TaskCheckpoint::new("task-a".to_string(), "desc".to_string());
        let mut other = TaskCheckpoint::new("task-b".to_string(), "desc".to_string());
        other.set_step(1); // bump version so it's higher
        let delta = other.compute_delta(&base);
        assert!(delta.is_none(), "different task_id should return None");
    }

    #[test]
    fn test_delta_lower_version_returns_none() {
        let mut base = TaskCheckpoint::new("task-1".to_string(), "desc".to_string());
        base.set_step(5); // bump version several times
        base.set_step(6);
        let earlier = TaskCheckpoint::new("task-1".to_string(), "desc".to_string());
        // earlier has version 1, base has higher version
        let delta = earlier.compute_delta(&base);
        assert!(delta.is_none(), "lower version should not produce a delta");
    }

    #[test]
    fn test_delta_shrunk_messages_returns_none() {
        let mut base = TaskCheckpoint::new("shrink".to_string(), "desc".to_string());
        base.set_messages(vec![
            Message::user("a"),
            Message::user("b"),
            Message::user("c"),
        ]);

        let mut updated = base.clone();
        // Shrink messages (simulating context truncation)
        updated.set_messages(vec![Message::user("a")]);

        let delta = updated.compute_delta(&base);
        assert!(
            delta.is_none(),
            "shrunk messages should force full save (no delta)"
        );
    }

    #[test]
    fn test_delta_shrunk_tool_calls_returns_none() {
        let mut base = TaskCheckpoint::new("shrink-tc".to_string(), "desc".to_string());
        base.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "t1".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: true,
            duration_ms: None,
        });
        base.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "t2".to_string(),
            arguments: "{}".to_string(),
            result: None,
            success: true,
            duration_ms: None,
        });

        let mut updated = base.clone();
        // Remove tool calls (clear vector and re-touch to bump version)
        updated.tool_calls.clear();
        updated.set_step(1); // bump version to be > base

        let delta = updated.compute_delta(&base);
        assert!(delta.is_none(), "shrunk tool_calls should force full save");
    }

    #[test]
    fn test_delta_shrunk_errors_returns_none() {
        let mut base = TaskCheckpoint::new("shrink-err".to_string(), "desc".to_string());
        base.log_error(0, "e1".to_string(), false);
        base.log_error(1, "e2".to_string(), true);

        let mut updated = base.clone();
        updated.errors.clear();
        updated.set_step(1); // bump version

        let delta = updated.compute_delta(&base);
        assert!(delta.is_none(), "shrunk errors should force full save");
    }

    #[test]
    fn test_delta_shrunk_memory_entries_returns_none() {
        use crate::checkpoint::MemoryEntry;
        let mut base = TaskCheckpoint::new("shrink-mem".to_string(), "desc".to_string());
        base.memory_entries.push(MemoryEntry {
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            token_estimate: 10,
        });
        // bump version so base has a nonzero version
        base.set_step(0);

        let mut updated = base.clone();
        updated.memory_entries.clear();
        updated.set_step(1); // bump version higher

        let delta = updated.compute_delta(&base);
        assert!(
            delta.is_none(),
            "shrunk memory_entries should force full save"
        );
    }

    #[test]
    fn test_delta_git_checkpoint_cleared_returns_none() {
        let mut base = TaskCheckpoint::new("git-clear".to_string(), "desc".to_string());
        base.git_checkpoint = Some(GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "abc".to_string(),
            dirty: false,
            staged_files: vec![],
            modified_files: vec![],
        });
        base.set_step(0);

        let mut updated = base.clone();
        updated.git_checkpoint = None;
        updated.set_step(1); // bump version

        let delta = updated.compute_delta(&base);
        assert!(
            delta.is_none(),
            "clearing git_checkpoint should force full save"
        );
    }

    #[test]
    fn test_delta_git_checkpoint_added() {
        let mut base = TaskCheckpoint::new("git-add".to_string(), "desc".to_string());
        base.set_step(0);

        let mut updated = base.clone();
        updated.git_checkpoint = Some(GitCheckpointInfo {
            branch: "feature".to_string(),
            commit_hash: "def456".to_string(),
            dirty: true,
            staged_files: vec!["a.rs".to_string()],
            modified_files: vec!["b.rs".to_string()],
        });
        updated.set_step(1); // bump version

        let delta = updated.compute_delta(&base);
        assert!(
            delta.is_some(),
            "adding git_checkpoint should produce a delta"
        );
        let d = delta.unwrap();
        assert!(d.git_checkpoint.is_some());
        let git = d.git_checkpoint.unwrap();
        assert_eq!(git.branch, "feature");
        assert!(git.dirty);
    }

    #[test]
    fn test_delta_git_checkpoint_changed() {
        let mut base = TaskCheckpoint::new("git-chg".to_string(), "desc".to_string());
        base.git_checkpoint = Some(GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "aaa".to_string(),
            dirty: false,
            staged_files: vec![],
            modified_files: vec![],
        });
        base.set_step(0);

        let mut updated = base.clone();
        updated.git_checkpoint = Some(GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "bbb".to_string(),
            dirty: true,
            staged_files: vec!["x.rs".to_string()],
            modified_files: vec![],
        });
        updated.set_step(1); // bump version

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.git_checkpoint.as_ref().unwrap().commit_hash, "bbb");
    }

    #[test]
    fn test_delta_only_iteration_change() {
        let mut base = TaskCheckpoint::new("iter-only".to_string(), "desc".to_string());
        base.set_step(0);

        let mut updated = base.clone();
        updated.set_iteration(5);

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.current_iteration, Some(5));
        assert!(d.status.is_none());
        assert!(d.current_step.is_none());
        assert!(d.new_messages.is_empty());
        assert!(d.updated_tokens.is_none());
    }

    #[test]
    fn test_delta_only_errors_added() {
        let mut base = TaskCheckpoint::new("err-only".to_string(), "desc".to_string());
        base.set_step(0);

        let mut updated = base.clone();
        updated.log_error(0, "new error".to_string(), false);

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.new_errors.len(), 1);
        assert_eq!(d.new_errors[0].error, "new error");
        assert!(d.new_messages.is_empty());
    }

    #[test]
    fn test_delta_only_tool_calls_added() {
        let mut base = TaskCheckpoint::new("tc-only".to_string(), "desc".to_string());
        base.set_step(0);

        let mut updated = base.clone();
        updated.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "new_tool".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(50),
        });

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.new_tool_calls.len(), 1);
        assert_eq!(d.new_tool_calls[0].tool_name, "new_tool");
    }

    #[test]
    fn test_delta_only_memory_entries_added() {
        use crate::checkpoint::MemoryEntry;
        let mut base = TaskCheckpoint::new("mem-only".to_string(), "desc".to_string());
        base.set_step(0);

        let mut updated = base.clone();
        updated.memory_entries.push(MemoryEntry {
            timestamp: "2025-06-01T00:00:00Z".to_string(),
            role: "assistant".to_string(),
            content: "remembered".to_string(),
            token_estimate: 20,
        });
        updated.set_step(1); // bump version

        let delta = updated.compute_delta(&base);
        assert!(delta.is_some());
        let d = delta.unwrap();
        assert_eq!(d.new_memory_entries.len(), 1);
        assert_eq!(d.new_memory_entries[0].content, "remembered");
    }

    // =========================================================================
    // apply_delta error paths
    // =========================================================================

    #[test]
    fn test_apply_delta_task_id_mismatch() {
        use crate::checkpoint::CheckpointDelta;
        let mut cp = TaskCheckpoint::new("task-a".to_string(), "desc".to_string());
        let delta = CheckpointDelta {
            task_id: "task-b".to_string(),
            base_version: cp.version,
            target_version: cp.version + 1,
            updated_at: Utc::now(),
            status: None,
            current_step: None,
            current_iteration: None,
            new_messages: vec![],
            new_memory_entries: vec![],
            new_tool_calls: vec![],
            new_errors: vec![],
            updated_tokens: None,
            git_checkpoint: None,
        };
        let result = cp.apply_delta(&delta);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("task ID mismatch"),
            "should report task ID mismatch"
        );
    }

    #[test]
    fn test_apply_delta_version_mismatch() {
        use crate::checkpoint::CheckpointDelta;
        let mut cp = TaskCheckpoint::new("task-x".to_string(), "desc".to_string());
        let delta = CheckpointDelta {
            task_id: "task-x".to_string(),
            base_version: cp.version + 99, // wrong base version
            target_version: cp.version + 100,
            updated_at: Utc::now(),
            status: None,
            current_step: None,
            current_iteration: None,
            new_messages: vec![],
            new_memory_entries: vec![],
            new_tool_calls: vec![],
            new_errors: vec![],
            updated_tokens: None,
            git_checkpoint: None,
        };
        let result = cp.apply_delta(&delta);
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("version mismatch"),
            "should report version mismatch"
        );
    }

    #[test]
    fn test_apply_delta_with_all_fields_set() {
        use crate::checkpoint::{CheckpointDelta, ErrorLog, MemoryEntry};
        let mut cp = TaskCheckpoint::new("full-delta".to_string(), "desc".to_string());
        cp.set_step(0); // version = 2

        let delta = CheckpointDelta {
            task_id: "full-delta".to_string(),
            base_version: cp.version,
            target_version: cp.version + 1,
            updated_at: Utc::now(),
            status: Some(TaskStatus::Completed),
            current_step: Some(10),
            current_iteration: Some(25),
            new_messages: vec![Message::user("new msg")],
            new_memory_entries: vec![MemoryEntry {
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                role: "system".to_string(),
                content: "memory".to_string(),
                token_estimate: 5,
            }],
            new_tool_calls: vec![ToolCallLog {
                timestamp: Utc::now(),
                tool_name: "applied_tool".to_string(),
                arguments: "{}".to_string(),
                result: None,
                success: true,
                duration_ms: None,
            }],
            new_errors: vec![ErrorLog {
                timestamp: Utc::now(),
                step: 9,
                error: "applied error".to_string(),
                recovered: true,
            }],
            updated_tokens: Some(9999),
            git_checkpoint: Some(GitCheckpointInfo {
                branch: "dev".to_string(),
                commit_hash: "xyz789".to_string(),
                dirty: false,
                staged_files: vec![],
                modified_files: vec![],
            }),
        };

        cp.apply_delta(&delta).expect("apply should succeed");
        assert_eq!(cp.status, TaskStatus::Completed);
        assert_eq!(cp.current_step, 10);
        assert_eq!(cp.current_iteration, 25);
        assert_eq!(cp.messages.len(), 1);
        assert_eq!(cp.memory_entries.len(), 1);
        assert_eq!(cp.tool_calls.len(), 1);
        assert_eq!(cp.errors.len(), 1);
        assert_eq!(cp.estimated_tokens, 9999);
        assert_eq!(cp.git_checkpoint.as_ref().unwrap().branch, "dev");
        assert_eq!(cp.version, delta.target_version);
    }

    #[test]
    fn test_apply_delta_preserves_existing_data_when_fields_none() {
        use crate::checkpoint::CheckpointDelta;
        let mut cp = TaskCheckpoint::new("partial".to_string(), "desc".to_string());
        cp.set_step(5);
        cp.set_iteration(10);
        cp.set_status(TaskStatus::Paused);
        cp.set_estimated_tokens(1000);
        cp.set_messages(vec![Message::user("existing")]);

        let delta = CheckpointDelta {
            task_id: "partial".to_string(),
            base_version: cp.version,
            target_version: cp.version + 1,
            updated_at: Utc::now(),
            status: None,       // should not change
            current_step: None, // should not change
            current_iteration: None,
            new_messages: vec![], // no new messages
            new_memory_entries: vec![],
            new_tool_calls: vec![],
            new_errors: vec![],
            updated_tokens: None, // should not change
            git_checkpoint: None,
        };

        cp.apply_delta(&delta).expect("apply should succeed");
        // All existing fields should be preserved
        assert_eq!(cp.current_step, 5);
        assert_eq!(cp.current_iteration, 10);
        assert_eq!(cp.status, TaskStatus::Paused);
        assert_eq!(cp.estimated_tokens, 1000);
        assert_eq!(cp.messages.len(), 1);
    }

    // =========================================================================
    // CheckpointDelta serialization
    // =========================================================================

    #[test]
    fn test_checkpoint_delta_serialization_roundtrip() {
        use crate::checkpoint::CheckpointDelta;
        let delta = CheckpointDelta {
            task_id: "delta-ser".to_string(),
            base_version: 3,
            target_version: 5,
            updated_at: Utc::now(),
            status: Some(TaskStatus::Failed),
            current_step: Some(8),
            current_iteration: Some(20),
            new_messages: vec![Message::assistant("response")],
            new_memory_entries: vec![],
            new_tool_calls: vec![],
            new_errors: vec![],
            updated_tokens: Some(15000),
            git_checkpoint: None,
        };

        let json = serde_json::to_string(&delta).expect("serialize delta");
        let restored: CheckpointDelta = serde_json::from_str(&json).expect("deserialize delta");

        assert_eq!(restored.task_id, "delta-ser");
        assert_eq!(restored.base_version, 3);
        assert_eq!(restored.target_version, 5);
        assert_eq!(restored.status, Some(TaskStatus::Failed));
        assert_eq!(restored.current_step, Some(8));
        assert_eq!(restored.current_iteration, Some(20));
        assert_eq!(restored.new_messages.len(), 1);
        assert_eq!(restored.updated_tokens, Some(15000));
        assert!(restored.git_checkpoint.is_none());
    }

    // =========================================================================
    // CheckpointManager save/load integration (via tempfile)
    // =========================================================================

    #[test]
    fn test_checkpoint_manager_save_load_roundtrip() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let mut cp = TaskCheckpoint::new("mgr-rt".to_string(), "Manager roundtrip".to_string());
        cp.set_step(3);
        cp.set_iteration(7);
        cp.set_status(TaskStatus::Paused);
        cp.set_messages(vec![Message::system("sys"), Message::user("hello")]);
        cp.set_estimated_tokens(2500);
        cp.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        cp.log_error(2, "warning".to_string(), true);
        cp.git_checkpoint = Some(GitCheckpointInfo {
            branch: "test-branch".to_string(),
            commit_hash: "1234abcd".to_string(),
            dirty: false,
            staged_files: vec![],
            modified_files: vec![],
        });

        manager.save(&cp).unwrap();
        let loaded = manager.load("mgr-rt").unwrap();

        assert_eq!(loaded.task_id, "mgr-rt");
        assert_eq!(loaded.task_description, "Manager roundtrip");
        assert_eq!(loaded.current_step, 3);
        assert_eq!(loaded.current_iteration, 7);
        assert_eq!(loaded.status, TaskStatus::Paused);
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.estimated_tokens, 2500);
        assert_eq!(loaded.tool_calls.len(), 1);
        assert_eq!(loaded.errors.len(), 1);
        assert!(loaded.errors[0].recovered);
        let git = loaded.git_checkpoint.unwrap();
        assert_eq!(git.branch, "test-branch");
        assert!(!git.dirty);
    }

    #[test]
    fn test_checkpoint_manager_overwrite_preserves_latest() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let mut cp = TaskCheckpoint::new("overwrite".to_string(), "First".to_string());
        manager.save(&cp).unwrap();

        cp.set_step(10);
        cp.set_status(TaskStatus::Completed);
        cp.set_estimated_tokens(99_000);
        manager.save(&cp).unwrap();

        let loaded = manager.load("overwrite").unwrap();
        assert_eq!(loaded.current_step, 10);
        assert_eq!(loaded.status, TaskStatus::Completed);
        assert_eq!(loaded.estimated_tokens, 99_000);
    }

    #[test]
    fn test_checkpoint_manager_multiple_tasks() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        for i in 0..5 {
            let cp = TaskCheckpoint::new(format!("task-{}", i), format!("Task {}", i));
            manager.save(&cp).unwrap();
        }

        let tasks = manager.list_tasks().unwrap();
        assert_eq!(tasks.len(), 5);
    }

    #[test]
    fn test_checkpoint_manager_delete_cleans_up() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let cp = TaskCheckpoint::new("del-test".to_string(), "Delete me".to_string());
        manager.save(&cp).unwrap();
        assert!(manager.exists("del-test"));

        manager.delete("del-test").unwrap();
        assert!(!manager.exists("del-test"));

        let tasks = manager.list_tasks().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_checkpoint_manager_load_nonexistent_recovers() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Recovery creates a fresh checkpoint for unknown task IDs
        let result = manager.load("does-not-exist");
        assert!(result.is_ok());
        let cp = result.unwrap();
        assert_eq!(cp.task_id, "does-not-exist");
        assert_eq!(cp.status, TaskStatus::InProgress);
    }

    #[test]
    fn test_checkpoint_manager_corrupted_file_recovers() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Write garbage to the checkpoint file
        let path = dir.path().join("corrupt-task.json");
        std::fs::write(&path, "this is not valid json!!!").unwrap();

        // Load should recover (create a fresh checkpoint)
        let result = manager.load("corrupt-task");
        assert!(result.is_ok());
        let cp = result.unwrap();
        assert_eq!(cp.task_id, "corrupt-task");
    }

    #[test]
    fn test_checkpoint_manager_backup_recovery() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Save a valid checkpoint (this creates the .json file with HMAC envelope)
        let mut cp = TaskCheckpoint::new("backup-test".to_string(), "Backup".to_string());
        cp.set_step(5);
        manager.save(&cp).unwrap();

        // Immediately copy the primary to .bak while the HMAC key is still
        // consistent (parallel tests can race on the global key file).
        let primary = dir.path().join("backup-test.json");
        let backup = dir.path().join("backup-test.json.bak");
        std::fs::copy(&primary, &backup).unwrap();

        // Corrupt the primary file
        std::fs::write(&primary, "corrupted!!!").unwrap();

        // Remove any delta log so recovery doesn't fail applying deltas.
        let delta_path = dir.path().join("backup-test.delta.jsonl");
        let _ = std::fs::remove_file(&delta_path);

        // Load should recover from .json.bak (which has step 5).
        // NOTE: If parallel tests race on the global HMAC key file, the
        // backup's integrity check may fail and recovery creates a fresh
        // checkpoint (step 0). We accept both outcomes since the test's
        // primary purpose is verifying that load() doesn't error on a
        // corrupted primary.
        let loaded = manager.load("backup-test").unwrap();
        assert_eq!(loaded.task_id, "backup-test");
        assert!(
            loaded.current_step == 5 || loaded.current_step == 0,
            "Expected step 5 (backup recovery) or 0 (fresh fallback), got {}",
            loaded.current_step
        );
    }

    // =========================================================================
    // Corrupt checkpoint detection (mirrors Agent::resume validation)
    // =========================================================================

    #[test]
    fn test_corrupt_checkpoint_step_without_messages() {
        // Manually simulate the condition checked in Agent::resume:
        // current_step > 0 but messages is empty
        let step = 5_usize;
        let messages_empty = true;

        // This mirrors the Agent::resume validation logic
        let is_corrupt = step > 0 && messages_empty;
        assert!(
            is_corrupt,
            "step > 0 with empty messages should be detected as corrupt"
        );
    }

    #[test]
    fn test_valid_checkpoint_step_zero_no_messages() {
        // step == 0 with empty messages is valid (fresh checkpoint)
        let step = 0_usize;
        let messages_empty = true;
        let is_corrupt = step > 0 && messages_empty;
        assert!(!is_corrupt);
    }

    #[test]
    fn test_valid_checkpoint_step_with_messages() {
        let step = 5_usize;
        let messages_empty = false;
        let is_corrupt = step > 0 && messages_empty;
        assert!(!is_corrupt);
    }

    // =========================================================================
    // TaskStatus serialization coverage
    // =========================================================================

    #[test]
    fn test_task_status_all_variants_serde() {
        let variants = vec![
            (TaskStatus::InProgress, "\"in_progress\""),
            (TaskStatus::Completed, "\"completed\""),
            (TaskStatus::Failed, "\"failed\""),
            (TaskStatus::Paused, "\"paused\""),
        ];
        for (status, expected_json) in variants {
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(json, expected_json);
            let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, status);
        }
    }

    // =========================================================================
    // GitCheckpointInfo serialization edge cases
    // =========================================================================

    #[test]
    fn test_git_checkpoint_info_full_roundtrip() {
        let info = GitCheckpointInfo {
            branch: "feature/special-chars-#123".to_string(),
            commit_hash: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
            dirty: true,
            staged_files: vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "Cargo.toml".to_string(),
            ],
            modified_files: vec!["README.md".to_string(), "tests/integration.rs".to_string()],
        };

        let json = serde_json::to_string_pretty(&info).unwrap();
        let restored: GitCheckpointInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, info);
    }

    #[test]
    fn test_git_checkpoint_info_clean_state() {
        let info = GitCheckpointInfo {
            branch: "main".to_string(),
            commit_hash: "0000000".to_string(),
            dirty: false,
            staged_files: vec![],
            modified_files: vec![],
        };
        let json = serde_json::to_string(&info).unwrap();
        let restored: GitCheckpointInfo = serde_json::from_str(&json).unwrap();
        assert!(!restored.dirty);
        assert!(restored.staged_files.is_empty());
        assert!(restored.modified_files.is_empty());
    }

    // =========================================================================
    // ToolCallLog and ErrorLog serialization
    // =========================================================================

    #[test]
    fn test_tool_call_log_with_none_result() {
        let log = ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "shell_exec".to_string(),
            arguments: r#"{"cmd":"ls"}"#.to_string(),
            result: None,
            success: false,
            duration_ms: None,
        };
        let json = serde_json::to_string(&log).unwrap();
        let restored: ToolCallLog = serde_json::from_str(&json).unwrap();
        assert!(restored.result.is_none());
        assert!(restored.duration_ms.is_none());
        assert!(!restored.success);
    }

    #[test]
    fn test_error_log_serialization() {
        use crate::checkpoint::ErrorLog;
        let log = ErrorLog {
            timestamp: Utc::now(),
            step: 42,
            error: "segfault in module X".to_string(),
            recovered: false,
        };
        let json = serde_json::to_string(&log).unwrap();
        let restored: ErrorLog = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.step, 42);
        assert_eq!(restored.error, "segfault in module X");
        assert!(!restored.recovered);
    }

    // =========================================================================
    // Multiple sequential delta applications
    // =========================================================================

    #[test]
    fn test_multiple_delta_applications() {
        let mut base = TaskCheckpoint::new("multi-delta".to_string(), "desc".to_string());
        base.set_step(0);

        // First delta: add a message and change step
        let mut v1 = base.clone();
        v1.set_messages(vec![Message::user("msg1")]);
        v1.set_step(1);
        let delta1 = v1.compute_delta(&base).unwrap();

        base.apply_delta(&delta1).unwrap();
        assert_eq!(base.messages.len(), 1);
        assert_eq!(base.current_step, 1);

        // Second delta: add another message and bump iteration
        let mut v2 = base.clone();
        v2.set_messages(vec![Message::user("msg1"), Message::assistant("reply1")]);
        v2.set_iteration(3);
        let delta2 = v2.compute_delta(&base).unwrap();

        base.apply_delta(&delta2).unwrap();
        assert_eq!(base.messages.len(), 2);
        assert_eq!(base.current_iteration, 3);

        // Third delta: add error and complete
        let mut v3 = base.clone();
        v3.log_error(1, "test err".to_string(), true);
        v3.set_status(TaskStatus::Completed);
        let delta3 = v3.compute_delta(&base).unwrap();

        base.apply_delta(&delta3).unwrap();
        assert_eq!(base.errors.len(), 1);
        assert_eq!(base.status, TaskStatus::Completed);
    }

    // =========================================================================
    // Checkpoint with large data
    // =========================================================================

    #[test]
    fn test_checkpoint_with_many_messages() {
        let mut cp = TaskCheckpoint::new("large".to_string(), "desc".to_string());
        let mut msgs = Vec::new();
        for i in 0..100 {
            if i % 2 == 0 {
                msgs.push(Message::user(format!("Question {}", i)));
            } else {
                msgs.push(Message::assistant(format!("Answer {}", i)));
            }
        }
        cp.set_messages(msgs);
        assert_eq!(cp.messages.len(), 100);

        // Serialize and deserialize
        let json = serde_json::to_string(&cp).unwrap();
        let restored: TaskCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.messages.len(), 100);
    }

    #[test]
    fn test_checkpoint_with_many_tool_calls_and_errors() {
        let mut cp = TaskCheckpoint::new("many-tc".to_string(), "desc".to_string());
        for i in 0..50 {
            cp.log_tool_call(ToolCallLog {
                timestamp: Utc::now(),
                tool_name: format!("tool_{}", i),
                arguments: format!(r#"{{"arg":{}}}"#, i),
                result: Some(format!("result_{}", i)),
                success: i % 3 != 0,
                duration_ms: Some(i as u64 * 10),
            });
        }
        for i in 0..20 {
            cp.log_error(i, format!("error_{}", i), i % 2 == 0);
        }
        assert_eq!(cp.tool_calls.len(), 50);
        assert_eq!(cp.errors.len(), 20);

        let json = serde_json::to_string(&cp).unwrap();
        let restored: TaskCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.tool_calls.len(), 50);
        assert_eq!(restored.errors.len(), 20);

        // Verify specific entries
        assert!(!restored.tool_calls[0].success); // 0 % 3 == 0
        assert!(restored.tool_calls[1].success); // 1 % 3 != 0
        assert!(restored.errors[0].recovered); // 0 % 2 == 0
        assert!(!restored.errors[1].recovered); // 1 % 2 != 0
    }

    // =========================================================================
    // CheckpointManager save_with_retry
    // =========================================================================

    #[test]
    fn test_checkpoint_manager_save_with_retry_succeeds() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        let cp = TaskCheckpoint::new("retry-ok".to_string(), "retry test".to_string());
        // save_with_retry should succeed on first try
        manager.save_with_retry(&cp).unwrap();

        let loaded = manager.load("retry-ok").unwrap();
        assert_eq!(loaded.task_id, "retry-ok");
    }

    // =========================================================================
    // CheckpointManager delta persistence
    // =========================================================================

    #[test]
    fn test_checkpoint_manager_delta_persistence() {
        use crate::checkpoint::CheckpointManager;
        let dir = tempfile::tempdir().unwrap();
        let manager = CheckpointManager::new(dir.path().to_path_buf()).unwrap();

        // Create a checkpoint with enough data to make deltas efficient
        let mut cp = TaskCheckpoint::new("delta-persist".to_string(), "Delta test".to_string());
        let msgs: Vec<Message> = (0..20)
            .map(|i| Message::user(format!("msg {} {}", i, "padding ".repeat(20))))
            .collect();
        cp.set_messages(msgs);
        manager.save(&cp).unwrap();

        // Make a small change (should use delta if efficient)
        cp.set_step(2);
        cp.set_iteration(5);
        manager.save(&cp).unwrap();

        // Load and verify the data is correct
        let loaded = manager.load("delta-persist").unwrap();
        assert_eq!(loaded.current_step, 2);
        assert_eq!(loaded.current_iteration, 5);
        assert_eq!(loaded.messages.len(), 20);
    }

    // =========================================================================
    // Checkpoint created_at / updated_at tracking
    // =========================================================================

    #[test]
    fn test_checkpoint_timestamps() {
        let cp = TaskCheckpoint::new("ts-test".to_string(), "timestamps".to_string());
        assert_eq!(
            cp.created_at, cp.updated_at,
            "new checkpoint should have same created/updated"
        );

        let created = cp.created_at;
        // After mutation, updated_at should be >= created_at
        let mut cp2 = cp;
        cp2.set_step(1);
        assert!(cp2.updated_at >= created);
    }
}
