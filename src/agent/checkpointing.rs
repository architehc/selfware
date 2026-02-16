use anyhow::{Context, Result};
use colored::*;
use std::time::Instant;
use tracing::{debug, info};
#[cfg(feature = "resilience")]
use tracing::warn;

use super::*;
use crate::checkpoint::{capture_git_state, CheckpointManager, TaskCheckpoint, TaskStatus};
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
        agent.loop_control.set_state(AgentState::Executing {
            step: checkpoint.current_step,
        });

        // Set current step by calling increment_step the right number of times
        for _ in 0..checkpoint.current_step {
            agent.loop_control.increment_step();
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
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Completed);
            if let Some(ref manager) = self.checkpoint_manager {
                manager.save(checkpoint)?;
                self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
                self.last_checkpoint_persisted_at = Instant::now();
                self.checkpoint_persisted_once = true;
            }
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
                "Self-healing strategy '{}' restored agent state",
                execution.strategy
            );
        } else {
            info!(
                "Self-healing strategy '{}' succeeded without state restore",
                execution.strategy
            );
        }

        true
    }
}
