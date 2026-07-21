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
    pub async fn resume(mut config: Config, task_id: &str) -> Result<Self> {
        // Wrap the sync CheckpointManager::default_path() in spawn_blocking to
        // avoid stalling the async runtime with blocking fs I/O.
        let checkpoint_manager = tokio::task::spawn_blocking(CheckpointManager::default_path)
            .await
            .context("Checkpoint manager init task panicked")?
            .context("Failed to initialize checkpoint manager")?;

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

        // Restore the hard budget caps persisted at checkpoint time, unless the
        // resume command re-passed a flag (CLI override wins). Without this a
        // resume that omits the flags would run uncapped even though cumulative
        // consumption is restored — the "uncapped resume = unbounded spend" gap.
        restore_budget_caps_from_checkpoint(&mut config, &checkpoint);
        if checkpoint.max_budget_tokens.is_some()
            || checkpoint.max_wall_secs.is_some()
            || checkpoint.max_cost_usd.is_some()
        {
            println!(
                "   Budget caps: tokens={:?}, wall_secs={:?}, cost_usd={:?}",
                config.agent.max_budget_tokens,
                config.agent.max_wall_secs,
                config.agent.max_cost_usd
            );
        }

        // Build all restored state in temporary variables first, then commit
        // atomically to the agent. This prevents partial state if any step fails.
        let restored_messages = checkpoint.messages.clone();
        let mut restored_loop = AgentLoop::new(config.agent.max_iterations);

        // Restore exact loop progress when available.
        // Older checkpoints may not have an iteration value, so keep fallback logic.
        //
        // Resume fairness: a task checkpointed near its iteration cap would
        // immediately fail with "max iterations" on resume. Instead of
        // restoring the old iteration counter verbatim, we reset it to 0 so
        // the resumed task gets a full budget of additional iterations. The
        // step counter is still restored so the agent knows where it left off.
        // (The wall-clock baseline is reset separately in continue_execution.)
        if checkpoint.current_iteration > 0 {
            restored_loop.restore_progress(checkpoint.current_step, 0);
        } else {
            // Backward-compatible restore for legacy checkpoints.
            for _ in 0..checkpoint.current_step {
                restored_loop.next_state(); // consumes one iteration
                restored_loop
                    .increment_step()
                    .map_err(anyhow::Error::from)?;
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
        agent.current_checkpoint = Some(checkpoint.clone());
        agent.checkpoint_manager = Some(checkpoint_manager);
        // Restore the cumulative budget so the wall-clock / token caps continue
        // accumulating across resume instead of restarting from zero.
        agent.prior_elapsed_secs = checkpoint.elapsed_wall_secs;
        // Only the TOTAL is persisted (the checkpoint format has no
        // input/output split), so `.input`/`.output` restart at 0 while
        // `.total` carries the prior run. Every recompute site therefore
        // DELTA-ADDS each new step's tokens to `.total` instead of
        // recomputing `total = input + output`, which would silently erase
        // this restored budget on the first step after resume.
        agent.cumulative_token_usage.total = checkpoint.cumulative_tokens;
        agent.cumulative_cost_usd = checkpoint.cumulative_cost_usd;
        // Restore anti-thrash guard counters so a crash-looping task can't reset
        // its way out of the guards on every resume.
        agent.consecutive_no_action_prompts =
            checkpoint.guard_counters.consecutive_no_action_prompts;
        agent.mutation_gate_rejections = checkpoint.guard_counters.mutation_gate_rejections;
        agent.prefill_400_count = checkpoint.guard_counters.prefill_400_count;
        agent.last_checkpoint_tool_calls = checkpoint_tool_calls;
        agent.last_checkpoint_persisted_at = Instant::now();
        agent.checkpoint_persisted_once = true;

        // Restore memory entries from the checkpoint into the agent's memory.
        // The checkpoint stores MemoryEntry records that were accumulated during
        // the previous run. Without this, the agent loses all accumulated context
        // on resume and starts with an empty memory.
        if !checkpoint.memory_entries.is_empty() {
            for entry in &checkpoint.memory_entries {
                agent.memory.add_raw_entry(
                    entry.timestamp.clone(),
                    entry.role.clone(),
                    entry.content.clone(),
                    entry.token_estimate,
                );
            }
            info!(
                "Restored {} memory entries from checkpoint",
                checkpoint.memory_entries.len()
            );
        }

        // Restore estimated token count from the checkpoint so the agent's
        // memory budget awareness matches the pre-checkpoint state.
        if checkpoint.estimated_tokens > 0 {
            agent.memory.set_total_tokens(checkpoint.estimated_tokens);
            info!(
                "Restored token estimate ({}) from checkpoint",
                checkpoint.estimated_tokens
            );
        }

        // Restore cognitive state from the checkpoint when serialized state is
        // available. The checkpoint itself does not store the full CognitiveState
        // (it would require a format migration), but we can restore the episodic
        // memory lessons by replaying error/tool history. More importantly, we
        // restore the active plans if they were captured.
        //
        // Note: The checkpoint format does not currently serialize the full
        // CognitiveState (strategic goals, tactical/operational plans, working
        // memory). Those are re-initialized fresh by Self::new(). What we CAN
        // restore from the checkpoint:
        //   - Episodic memory lessons (replayed from error history below)
        //   - The cognitive phase (set to Do since we're resuming execution)
        //
        // Plans and working memory are NOT persisted in the checkpoint format
        // and will be lost on resume. This is noted but not fixed here to avoid
        // a checkpoint format migration (which would be a big new subsystem).
        if !checkpoint.errors.is_empty() {
            for error in &checkpoint.errors {
                if error.recovered {
                    agent.cognitive_state.episodic_memory.what_worked(
                        "error_recovery",
                        &format!(
                            "Recovered from error at step {}: {}",
                            error.step, error.error
                        ),
                    );
                } else {
                    agent.cognitive_state.episodic_memory.what_failed(
                        "task_execution",
                        &format!("Unrecovered error at step {}: {}", error.step, error.error),
                    );
                }
            }
            info!(
                "Replayed {} error lessons into episodic memory from checkpoint",
                checkpoint.errors.len()
            );
        }

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

        // Save memory entries so they can be restored on resume.
        // Convert the agent's internal MemoryEntry format to the checkpoint's
        // serializable MemoryEntry format.
        checkpoint.memory_entries = self
            .memory
            .recent(self.memory.len())
            .into_iter()
            .rev() // restore chronological order
            .map(|e| crate::checkpoint::MemoryEntry {
                timestamp: e.timestamp.clone(),
                role: e.role.clone(),
                content: e.content.clone(),
                token_estimate: e.token_estimate,
            })
            .collect();

        // Capture git state
        if let Ok(cwd) = std::env::current_dir() {
            checkpoint.git_checkpoint = capture_git_state(cwd.to_string_lossy().as_ref());
        }

        // Persist cumulative budget so a resumed run continues from where the
        // budget stood, instead of resetting it (which would let N resumes
        // consume N× the configured token/wall budget).
        checkpoint.cumulative_tokens = self.cumulative_token_usage.total;
        checkpoint.elapsed_wall_secs = self.budget_elapsed_secs();
        checkpoint.cumulative_cost_usd = self.cumulative_cost_usd;
        // Persist anti-thrash guard counters so they survive resume — otherwise
        // an auto-resumed crash-looping task resets them to 0 every restart.
        checkpoint.guard_counters = crate::checkpoint::GuardCounters {
            consecutive_no_action_prompts: self.consecutive_no_action_prompts,
            mutation_gate_rejections: self.mutation_gate_rejections,
            prefill_400_count: self.prefill_400_count,
        };

        // Persist the hard budget caps themselves (CLI-only, `#[serde(skip)]` on
        // AgentConfig) so a resumed run keeps its limits instead of running
        // uncapped when the resume command omits the flags.
        checkpoint.max_budget_tokens = self.config.agent.max_budget_tokens;
        checkpoint.max_wall_secs = self.config.agent.max_wall_secs;
        checkpoint.max_cost_usd = self.config.agent.max_cost_usd;

        checkpoint
    }

    /// Save current state to checkpoint
    pub(crate) fn save_checkpoint(&mut self, task_description: &str) -> Result<()> {
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

        let final_step = self.loop_control.current_step();
        let final_iter = self.loop_control.current_iteration();
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Completed);
            checkpoint.set_step(final_step);
            checkpoint.set_iteration(final_iter);
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

        // Trigger memory consolidation ("sleep") — compact session episodes
        // into long-term storage for future retrieval.
        #[cfg(feature = "consolidation")]
        self.consolidate_session_memory();

        if let Some(ref checkpoint) = self.current_checkpoint {
            if let Some(ref manager) = self.checkpoint_manager {
                // Full write so the base reflects the terminal Completed/step.
                manager.save_final(checkpoint)?;
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

        // Save global episodic memory — using tokio::fs for async I/O
        // to avoid blocking the Tokio executor on synchronous filesystem I/O.
        let data_dir = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("selfware");

        // Serialize in the main thread (cheap), write to disk asynchronously (slow I/O)
        let memory_content = serde_json::to_string_pretty(&self.cognitive_state.episodic_memory)?;

        let engine_path = data_dir.join("improvement_engine.json");
        let engine_save_result = self.self_improvement.save(&engine_path);
        if let Err(e) = &engine_save_result {
            warn!("Failed to save improvement engine state: {}", e);
        } else {
            info!("Saved self-improvement engine state");
        }

        let memory_path = data_dir.join("global_episodic_memory.json");
        let content = memory_content;
        tokio::spawn(async move {
            if let Some(parent) = memory_path.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    tracing::warn!("Failed to create episodic memory dir: {}", e);
                    return;
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
                }
            }
            // Atomic + owner-only: episodic memory holds raw task data. Write to
            // a process-unique temp, chmod 0600 BEFORE it is visible under the
            // real name, then rename over the target.
            static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let tmp_path =
                memory_path.with_extension(format!("tmp.{}.{}", std::process::id(), seq));
            if let Err(e) = tokio::fs::write(&tmp_path, &content).await {
                tracing::warn!("Failed to write episodic memory temp: {}", e);
                return;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) =
                    std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))
                {
                    tracing::warn!("Failed to chmod episodic memory temp: {}", e);
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    return;
                }
            }
            if let Err(e) = tokio::fs::rename(&tmp_path, &memory_path).await {
                tracing::warn!("Failed to rename episodic memory into place: {}", e);
                let _ = tokio::fs::remove_file(&tmp_path).await;
            } else {
                tracing::info!("Saved global episodic memory (background, atomic 0600)");
            }
        });

        Ok(())
    }

    /// Consolidate session memory — convert episodic experiences into
    /// long-term storage. This is the "sleep" cycle that compacts short-term
    /// session data into structured temporal records.
    #[cfg(feature = "consolidation")]
    fn consolidate_session_memory(&self) {
        use crate::consolidation::{CollectedItem, LongTermStore, SourceType};

        let checkpoint = match self.current_checkpoint.as_ref() {
            Some(cp) => cp,
            None => return,
        };

        // Collect tool call data as consolidation items
        let items: Vec<CollectedItem> = checkpoint
            .tool_calls
            .iter()
            .map(|tc| {
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("tool".to_string(), tc.tool_name.clone());
                metadata.insert("success".to_string(), tc.success.to_string());
                if let Some(dur) = tc.duration_ms {
                    metadata.insert("duration_ms".to_string(), dur.to_string());
                }

                CollectedItem {
                    source_id: format!("tc-{}-{}", checkpoint.task_id, tc.timestamp.timestamp()),
                    source_type: SourceType::ToolResult,
                    content: tool_call_content_preview(&tc.tool_name, &tc.arguments, tc.success),
                    timestamp: tc.timestamp,
                    importance: if tc.success { 2 } else { 3 }, // Normal / High
                    tags: vec![tc.tool_name.clone(), checkpoint.task_id.clone()],
                    metadata,
                    related_ids: Vec::new(),
                    session_id: Some(checkpoint.task_id.clone()),
                    file_refs: Vec::new(),
                }
            })
            .collect();

        if items.is_empty() {
            return;
        }

        // Store to disk (non-blocking)
        let store = LongTermStore::new(
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("selfware")
                .join("consolidated_memory"),
        );

        let item_count = items.len();
        let task_id = checkpoint.task_id.clone();

        // Convert items to temporal records directly (skip LLM summarization for speed)
        let now = chrono::Utc::now();
        let records: Vec<crate::consolidation::TemporalRecord> =
            vec![crate::consolidation::TemporalRecord {
                id: format!("session-{}", truncate_bytes_char_boundary(&task_id, 16)),
                created_at: now,
                source_timestamps: items.iter().map(|i| i.timestamp).collect(),
                sequence_order: now.timestamp() as u64,
                causal_parents: Vec::new(),
                causal_children: Vec::new(),
                decay_score: 1.0,
                access_count: 0,
                last_accessed: now,
                content: crate::consolidation::CompactedContent {
                    summary: format!("Session {} with {} tool calls", task_id, item_count),
                    key_facts: items
                        .iter()
                        .filter(|i| !i.tags.is_empty())
                        .take(5)
                        .map(|i| i.content.clone())
                        .collect(),
                    entities: items
                        .iter()
                        .flat_map(|i| i.tags.clone())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect(),
                    actions: Vec::new(),
                    outcomes: Vec::new(),
                    insights: Vec::new(),
                },
                multimodal_refs: Vec::new(),
                source_ids: items.iter().map(|i| i.source_id.clone()).collect(),
                tags: vec!["session".to_string(), task_id.clone()],
                importance: crate::consolidation::RecordImportance::Normal,
                session_id: Some(task_id),
                metadata: std::collections::HashMap::new(),
            }];

        // Save in background
        tokio::spawn(async move {
            match store.store(&records).await {
                Ok(result) => {
                    tracing::info!(
                        "Consolidated {} tool calls into {} records",
                        item_count,
                        result.stored
                    );
                }
                Err(e) => {
                    tracing::warn!("Memory consolidation failed: {}", e);
                }
            }
        });
    }

    /// Mark current task as failed
    pub(super) fn fail_checkpoint(&mut self, reason: &str) -> Result<()> {
        if let Some(plan) = self.cognitive_state.active_tactical_plan.as_mut() {
            plan.status = crate::cognitive::StepStatus::Failed;
        }
        self.cognitive_state
            .fail_operational_step(self.loop_control.current_step() + 1, reason);
        let final_step = self.loop_control.current_step();
        let final_iter = self.loop_control.current_iteration();
        if let Some(ref mut checkpoint) = self.current_checkpoint {
            checkpoint.set_status(TaskStatus::Failed);
            checkpoint.set_step(final_step);
            checkpoint.set_iteration(final_iter);
            checkpoint.log_error(final_step, reason.to_string(), false);
            if let Some(ref manager) = self.checkpoint_manager {
                // Full write so the base reflects the terminal Failed/step.
                manager.save_final(checkpoint)?;
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
    pub(super) async fn try_self_healing_recovery(&mut self, error: &str, context: &str) -> bool {
        if !self.config.continuous_work.auto_recovery {
            return false;
        }

        let occurrence = ErrorOccurrence::new("agent_execution_error", error, context);
        let Some(execution) = self.self_healing.handle_error(occurrence).await else {
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

/// Restore the persisted hard budget caps into `config` on resume, but only for
/// caps the resume command did not itself supply — a re-passed CLI/env flag
/// (already reflected in `config.agent.max_*`) wins over the persisted value.
fn restore_budget_caps_from_checkpoint(
    config: &mut crate::config::Config,
    checkpoint: &TaskCheckpoint,
) {
    if config.agent.max_budget_tokens.is_none() {
        config.agent.max_budget_tokens = checkpoint.max_budget_tokens;
    }
    if config.agent.max_wall_secs.is_none() {
        config.agent.max_wall_secs = checkpoint.max_wall_secs;
    }
    if config.agent.max_cost_usd.is_none() {
        config.agent.max_cost_usd = checkpoint.max_cost_usd;
    }
}

/// Byte-truncate `s` to at most `max_bytes`, backing off to a UTF-8 char
/// boundary. A raw `&s[..n]` byte slice PANICS when `n` lands mid-codepoint
/// — which is exactly what happened to `consolidate_session_memory` on every
/// successful task whose logged tool-call args straddled byte 200.
#[cfg(feature = "consolidation")]
fn truncate_bytes_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// One-line consolidation preview of a logged tool call, with the args
/// truncated char-boundary-safely (multibyte args must never panic).
#[cfg(feature = "consolidation")]
fn tool_call_content_preview(tool_name: &str, arguments: &str, success: bool) -> String {
    format!(
        "Tool: {} | Args: {} | Success: {}",
        tool_name,
        truncate_bytes_char_boundary(arguments, 200),
        success,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_test.rs"]
mod budget_cap_restore_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_main_test.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_resume_budget_test.rs"]
mod resume_budget_tests;

#[cfg(all(test, feature = "consolidation"))]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_consolidate_utf8_test.rs"]
mod consolidate_utf8_tests;
