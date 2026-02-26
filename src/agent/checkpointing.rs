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

        println!(
            "{} Resuming task: {}",
            "🔄".bright_cyan(),
            checkpoint.task_description.bright_white()
        );
        println!(
            "   Current step: {}, Status: {:?}",
            checkpoint.current_step, checkpoint.status
        );

        let mut agent = Self::new(config).await?;

        // Restore state from checkpoint
        agent.messages = checkpoint.messages.clone();
        agent.loop_control = AgentLoop::new(agent.config.agent.max_iterations);

        // Restore exact loop progress when available.
        // Older checkpoints may not have an iteration value, so keep fallback logic.
        if checkpoint.current_iteration > 0 {
            agent
                .loop_control
                .restore_progress(checkpoint.current_step, checkpoint.current_iteration);
        } else {
            // Backward-compatible restore for legacy checkpoints.
            for _ in 0..checkpoint.current_step {
                agent.loop_control.next_state(); // consumes one iteration
                agent.loop_control.increment_step();
            }
            agent.loop_control.set_state(AgentState::Executing {
                step: checkpoint.current_step,
            });
        }

        let checkpoint_tool_calls = checkpoint.tool_calls.len();
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
        checkpoint.estimated_tokens = self.memory.total_tokens();

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
