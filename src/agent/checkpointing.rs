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
    /// Re-apply today's source policy to persisted tool data. User-authored
    /// instructions are not a sanitization target: legacy XML results require
    /// an adjacent assistant call or an exact execution-log match as provenance.
    pub(super) fn sanitize_restored_tool_messages(
        &mut self,
        messages: &mut [Message],
        logs: &[crate::checkpoint::ToolCallLog],
    ) {
        use crate::api::types::{ContentBlock, MessageContent};
        let mut native_calls = std::collections::HashMap::<String, (String, String)>::new();
        let mut pending_xml = std::collections::VecDeque::<(String, String)>::new();
        for message in messages {
            if message.role == "assistant" {
                pending_xml.clear();
                if let Some(calls) = &message.tool_calls {
                    for call in calls {
                        native_calls.insert(
                            call.id.clone(),
                            (call.function.name.clone(), call.function.arguments.clone()),
                        );
                    }
                }
                if message.tool_calls.as_ref().is_none_or(Vec::is_empty) {
                    for call in
                        crate::tool_parser::parse_tool_calls(&message.content.text_all()).tool_calls
                    {
                        pending_xml.push_back((call.tool_name, call.arguments.to_string()));
                    }
                }
                continue;
            }
            let text = message.content.text_all();
            let xml_body = text
                .strip_prefix("<tool_result>")
                .and_then(|body| body.strip_suffix("</tool_result>"));
            let provenance = if message.role == "tool" {
                Some(
                    message
                        .tool_call_id
                        .as_ref()
                        .and_then(|id| native_calls.get(id))
                        .cloned()
                        .or_else(|| {
                            logs.iter()
                                .rev()
                                .find(|log| log.result.as_deref() == Some(text.as_str()))
                                .map(|log| (log.tool_name.clone(), log.arguments.clone()))
                        })
                        .unwrap_or_else(|| ("restored_tool".to_string(), "{}".to_string())),
                )
            } else if message.role == "user" && (xml_body.is_some() || message.content.has_images())
            {
                pending_xml.pop_front().or_else(|| {
                    let body = xml_body?;
                    let body = body
                        .strip_prefix("<error>")
                        .and_then(|b| b.strip_suffix("</error>"))
                        .unwrap_or(body);
                    logs.iter()
                        .rev()
                        .find(|log| log.result.as_deref() == Some(body))
                        .map(|log| (log.tool_name.clone(), log.arguments.clone()))
                })
            } else {
                pending_xml.clear();
                None
            };
            let Some((tool_name, arguments)) = provenance else {
                continue;
            };
            // A remote/MCP tool's `path` may be an API resource identifier,
            // not a host filesystem source. Only known local readers/writers
            // inherit the workspace path policy.
            let local_paths = tool_name.starts_with("file_")
                || tool_name.starts_with("context_")
                || tool_name.starts_with("git_")
                || tool_name.starts_with("lsp_")
                || matches!(
                    tool_name.as_str(),
                    "directory_tree"
                        | "symbol_search"
                        | "search"
                        | "grep_search"
                        | "glob_find"
                        | "analyze"
                        | "tech_debt_report"
                        | "code_introspect"
                        | "code_query"
                        | "code_plan"
                        | "vision_analyze"
                        | "vision_compare"
                );
            let paths_allowed = !local_paths
                || serde_json::from_str::<serde_json::Value>(&arguments)
                    .ok()
                    .is_none_or(|args| {
                        let mut paths: Vec<&str> = [
                            "path",
                            "file_path",
                            "file",
                            "filename",
                            "target",
                            "image_path",
                            "image_a",
                            "image_b",
                        ]
                        .iter()
                        .filter_map(|key| args.get(*key).and_then(|v| v.as_str()))
                        .collect();
                        // Include legacy bulk-loader arrays and multi-file tool schemas.
                        for key in ["paths", "files"] {
                            if let Some(values) = args.get(key).and_then(|v| v.as_array()) {
                                paths.extend(values.iter().filter_map(|v| v.as_str()));
                            }
                        }
                        if let Some(edits) = args.get("edits").and_then(|v| v.as_array()) {
                            paths.extend(
                                edits
                                    .iter()
                                    .filter_map(|edit| edit.get("path").and_then(|v| v.as_str())),
                            );
                        }
                        paths.into_iter().all(|path| {
                            self.validate_context_path(std::path::Path::new(path))
                                .is_ok()
                        })
                    });
            if !paths_allowed {
                let removed = "[trust-gate: restored tool output withheld because its source path is no longer allowed]";
                message.content = if xml_body.is_some() {
                    format!("<tool_result>{removed}</tool_result>").into()
                } else {
                    removed.into()
                };
                continue;
            }
            let xml_error = xml_body.and_then(|body| {
                body.strip_prefix("<error>")
                    .and_then(|b| b.strip_suffix("</error>"))
            });
            let source_text = xml_error.or(xml_body).unwrap_or(&text);
            let gate = super::tool_dispatch::sanitize_tool_context(
                &tool_name,
                &arguments,
                source_text,
                self.config.safety.trust_gate_tool_results,
            );
            self.trust_gate_findings += gate.sanitized;
            if gate.content == source_text {
                continue;
            }
            let content_to_store = if xml_error.is_some() {
                format!("<tool_result><error>{}</error></tool_result>", gate.content)
            } else if xml_body.is_some() {
                format!("<tool_result>{}</tool_result>", gate.content)
            } else {
                gate.content
            };
            match &mut message.content {
                MessageContent::Text(content) => *content = content_to_store,
                MessageContent::Blocks(blocks) => {
                    // Scan all text together (credentials can span blocks), and
                    // retain allowed images rather than flattening multimodal data.
                    let mut replacement = Some(content_to_store);
                    blocks.retain_mut(|block| match block {
                        ContentBlock::Text { text } => {
                            if let Some(content) = replacement.take() {
                                *text = content;
                                true
                            } else {
                                false
                            }
                        }
                        _ => true,
                    });
                }
            }
        }
    }

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
        let mut restored_messages = checkpoint.messages.clone();
        let mut restored_loop = AgentLoop::new(config.agent.max_iterations);

        // Restore the adaptive budget earned before the checkpoint: the
        // extended cap and the grants already consumed. Without this a resume
        // silently dropped earned extensions — the run restarted at the
        // configured cap, and with the extension ceiling unspent it re-earned
        // grants it had already used (2026-09-22 long-horizon finding). Done
        // BEFORE the step/iteration restore below so even the legacy replay
        // path evaluates the cap trips against the extended budget.
        if let Some(persisted_cap) = checkpoint.effective_max_iterations {
            restored_loop.restore_budget_extension(persisted_cap, checkpoint.extensions_granted);
        }

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
        // The per-segment iteration counter reset above is budget fairness;
        // the chain-wide total must still accumulate across every segment of
        // the task so the end-of-run summary (and the next checkpoint) report
        // the whole chain rather than the final segment alone.
        restored_loop.set_prior_iterations(checkpoint.cumulative_iterations);
        // The auto-continue chain bound is per-TASK, not per-process: a task
        // resumed after chaining continuations keeps counting against
        // `MAX_AUTO_CONTINUES` instead of receiving a fresh budget of 3
        // chains on every restart. Legacy checkpoints predate the field and
        // deserialize it as 0, which grants a fresh budget — acceptable for
        // old data, never for new checkpoints.
        restored_loop.set_auto_continue_count(checkpoint.auto_continue_count);

        let checkpoint_tool_calls = checkpoint.tool_calls.len();

        // Create the agent and commit all restored state at once
        let mut agent = Self::new(config).await?;
        agent.sanitize_restored_tool_messages(&mut restored_messages, &checkpoint.tool_calls);
        agent.messages = restored_messages;
        agent.loop_control = restored_loop;
        agent.current_checkpoint = Some(checkpoint.clone());
        agent.checkpoint_manager = Some(checkpoint_manager);
        // Restore the cumulative budget so the wall-clock / token caps continue
        // accumulating across resume instead of restarting from zero.
        // Outstanding obligations survive a restart. A resume that dropped them
        // would forgive every unread change in the session, which is precisely
        // the "waiting clears debt" failure in a different costume.
        agent.evidence_ledger = checkpoint.evidence_ledger.clone();
        agent.prior_elapsed_secs = checkpoint.elapsed_wall_secs;
        // Only the TOTAL is persisted (the checkpoint format has no
        // input/output split), so `.input`/`.output` restart at 0 while
        // `.total` carries the prior run. Every recompute site therefore
        // DELTA-ADDS each new step's tokens to `.total` instead of
        // recomputing `total = input + output`, which would silently erase
        // this restored budget on the first step after resume.
        agent.cumulative_token_usage.total = checkpoint.cumulative_tokens;
        agent.cumulative_cost_usd = checkpoint.cumulative_cost_usd;
        agent
            .client
            .restore_wall_budget(checkpoint.elapsed_wall_secs);
        agent.client.mark_restored_usage();
        agent
            .client
            .ensure_budget_floor(checkpoint.cumulative_tokens, checkpoint.cumulative_cost_usd);
        // Restore anti-thrash guard counters so a crash-looping task can't reset
        // its way out of the guards on every resume.
        agent.consecutive_no_action_prompts =
            checkpoint.guard_counters.consecutive_no_action_prompts;
        agent.mutation_gate_rejections = checkpoint.guard_counters.mutation_gate_rejections;
        agent.prefill_400_count = checkpoint.guard_counters.prefill_400_count;
        // Restore the verification ledger so UNVERIFIED pre-checkpoint edits
        // stay unverified across resume — resetting the counters to 0 made
        // `last_successful >= mutation_sequence` trivially true and the
        // completion gate accepted stale work (external review finding).
        agent.mutation_sequence = checkpoint.guard_counters.mutation_sequence;
        agent.last_successful_verification_mutation_sequence = checkpoint
            .guard_counters
            .last_successful_verification_mutation_sequence;
        agent.last_failed_verification_mutation_sequence = checkpoint
            .guard_counters
            .last_failed_verification_mutation_sequence;
        agent.last_failed_verification_summary = checkpoint
            .guard_counters
            .last_failed_verification_summary
            .clone();
        agent.verification_failures = checkpoint.guard_counters.verification_failures.clone();
        // Restore the files-changed evidence from earlier segments: without
        // it the end-of-run summary's file list covered only the resumed
        // segment. This mirrors exactly what a single-process run accumulates
        // — `mark_written` marks the path stale, which is also the correct
        // reread-guard treatment (the file WAS written by this task chain, so
        // a re-read is not an unchanged probe).
        let prior_written = prior_segment_written_paths(&checkpoint.tool_calls);
        for path in &prior_written {
            agent.file_tracker.mark_written(path);
        }
        // Workspace refresh (W8a): the restored conversation may end before
        // the last persisted edits were discussed, or have compressed them
        // away — a resumed e2e run did not know `src/entry.rs` already
        // existed and rewrote it blind. Name what earlier segments wrote and
        // tell the model to re-read before rewriting.
        if let Some(note) = workspace_refresh_note(&prior_written) {
            agent.messages.push(Message::user(note));
        }
        // Verification credit is a statement about specific file contents.
        // Another process may have changed them while the task was paused;
        // a credit restored verbatim let the completion gate accept a tree
        // no check ever ran against.
        if let Some(note) = agent.revalidate_restored_verification_credit(
            checkpoint.guard_counters.verification_fingerprint.as_ref(),
            &prior_written,
        ) {
            agent.messages.push(Message::user(note));
        }
        agent.last_checkpoint_tool_calls = checkpoint_tool_calls;
        agent.last_checkpoint_persisted_at = Instant::now();
        agent.checkpoint_persisted_once = true;

        // Restore memory entries from the checkpoint into the agent's memory.
        // The checkpoint stores MemoryEntry records that were accumulated during
        // the previous run. Without this, the agent loses all accumulated context
        // on resume and starts with an empty memory.
        if !checkpoint.memory_entries.is_empty() {
            let mut memory_messages: Vec<Message> = checkpoint
                .memory_entries
                .iter()
                .map(|entry| {
                    let mut message = Message::user(entry.content.clone());
                    message.role = entry.role.clone();
                    message
                })
                .collect();
            agent.sanitize_restored_tool_messages(&mut memory_messages, &checkpoint.tool_calls);
            for (entry, message) in checkpoint.memory_entries.iter().zip(memory_messages) {
                agent.memory.add_raw_entry(
                    entry.timestamp.clone(),
                    entry.role.clone(),
                    message.content.text_all(),
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

        // Restore task context and re-classify task policy so guards and gates
        // have consistent read-only / mutation awareness across resume.
        agent.current_task_context = checkpoint.task_description.clone();
        agent.classify_task_policy();

        info!("Agent resumed from checkpoint with cognitive state in Do phase");

        Ok(agent)
    }

    /// Chain-wide iteration count: iterations consumed by every segment of
    /// the current task (initial run + auto-continue chains + resumes), not
    /// just the segment currently in flight. The per-segment counter
    /// (`current_iteration`) resets on resume for budget fairness; this
    /// total never resets within a task chain.
    pub fn cumulative_iterations(&self) -> usize {
        self.loop_control.accumulated_iterations()
    }

    /// End-of-run summary for a resumed or chained run: identical to
    /// [`Agent::run_summary`], but `iterations` is the chain-wide total
    /// instead of the per-segment loop counter. (Token/cost totals, the
    /// earned iteration-cap extension, and the files-changed evidence are
    /// already restored from the checkpoint by [`Agent::resume`].) On a
    /// fresh run the two are identical, so callers may use this
    /// unconditionally.
    pub fn chain_run_summary(&self) -> crate::agent::RunSummary {
        let mut summary = self.run_summary();
        summary.iterations = self.loop_control.accumulated_iterations();
        summary
    }

    /// Convert current state to a checkpoint
    pub fn to_checkpoint(&self, task_id: &str, task_description: &str) -> TaskCheckpoint {
        self.build_checkpoint(task_id, task_description, true)
    }

    /// [`Self::to_checkpoint`] with the git-state capture optional. The
    /// per-mutation persist skips it: `capture_git_state` runs a full
    /// working-tree status scan (libgit2), which would dominate the cost of
    /// a save that exists to be cheap. The next regular (cadence) save
    /// always captures it.
    fn build_checkpoint(
        &self,
        task_id: &str,
        task_description: &str,
        capture_git: bool,
    ) -> TaskCheckpoint {
        let mut checkpoint = if let Some(ref existing) = self.current_checkpoint {
            existing.clone()
        } else {
            TaskCheckpoint::new(task_id.to_string(), task_description.to_string())
        };
        // Shadow-mode ledger rides with the task. Declaring the field without
        // copying it here meant every resume silently forgave the session's
        // outstanding obligations.
        checkpoint.evidence_ledger = self.evidence_ledger.clone();

        checkpoint.set_step(self.loop_control.current_step());
        checkpoint.set_iteration(self.loop_control.current_iteration());
        // Persist the auto-continue chain count so the per-task chain bound
        // survives a restart (`Agent::resume` restores it onto the new loop).
        checkpoint.auto_continue_count = self.loop_control.auto_continue_count();
        // Persist the adaptive-budget state (effective cap + grants consumed)
        // so a resume restores the EARNED extension instead of silently
        // rebuilding at the configured cap — and the chain-wide iteration
        // total so the resumed run's end-of-run summary reports the whole
        // task chain, not just the final segment.
        checkpoint.effective_max_iterations = Some(self.loop_control.max_iterations());
        checkpoint.extensions_granted = self.loop_control.extensions_granted();
        checkpoint.cumulative_iterations = self.loop_control.accumulated_iterations();
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
        if capture_git {
            let cwd = crate::tools::workspace_root::current_path();
            checkpoint.git_checkpoint = capture_git_state(cwd.to_string_lossy().as_ref());
        }

        // Persist cumulative budget so a resumed run continues from where the
        // budget stood, instead of resetting it (which would let N resumes
        // consume N× the configured token/wall budget).
        checkpoint.cumulative_tokens = self
            .cumulative_token_usage
            .total
            .saturating_add(self.client.pending_usage().total_tokens);
        checkpoint.elapsed_wall_secs = self.budget_elapsed_secs();
        checkpoint.cumulative_cost_usd =
            self.cumulative_cost_usd + self.client.pending_usage().cost.unwrap_or(0.0);
        // Persist anti-thrash guard counters so they survive resume — otherwise
        // an auto-resumed crash-looping task resets them to 0 every restart.
        checkpoint.guard_counters = self.guard_counters_snapshot();

        // Persist the hard budget caps themselves (CLI-only, `#[serde(skip)]` on
        // AgentConfig) so a resumed run keeps its limits instead of running
        // uncapped when the resume command omits the flags.
        checkpoint.max_budget_tokens = self.config.agent.max_budget_tokens;
        checkpoint.max_wall_secs = self.config.agent.max_wall_secs;
        checkpoint.max_cost_usd = self.config.agent.max_cost_usd;

        checkpoint
    }

    /// Task description for a session-exit auto-save, or `None` when the
    /// session carried no user task and must NOT be saved.
    ///
    /// Every session-exit path (rich REPL, basic REPL, TUI) routes through
    /// this so none of them writes a placeholder journal entry ("interactive
    /// basic session exit", "TUI session exit") that `--continue` would later
    /// pick up and run as an instruction-less agent turn. Prefers the active
    /// checkpoint's real description, else the first non-empty user message.
    pub(crate) fn session_exit_task_description(&self) -> Option<String> {
        if let Some(cp) = self.current_checkpoint.as_ref() {
            if !crate::checkpoint::is_placeholder_task_description(&cp.task_description) {
                return Some(cp.task_description.clone());
            }
        }
        self.messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.text_all())
            .find(|text| !text.trim().is_empty())
    }

    /// Save current state to checkpoint (subject to the continuous-work
    /// cadence policy — see [`should_persist_checkpoint`]).
    pub(crate) fn save_checkpoint(&mut self, task_description: &str) -> Result<()> {
        if self.checkpoint_manager.is_none() {
            return Ok(());
        }
        if !self.should_persist_checkpoint() {
            debug!("Checkpoint skipped by continuous-work policy");
            return Ok(());
        }
        self.persist_checkpoint(task_description, false, false)
    }

    /// Persist the checkpoint right after a successful MUTATING tool call
    /// (W8a). The continuous-work cadence (every N tool calls / T seconds)
    /// let a crash lose several steps of edits: a kill at 110 s lost 5
    /// steps, and the resumed run — not knowing `src/entry.rs` was already on
    /// disk — rewrote it. Every successful mutation is now on disk before the
    /// next model turn.
    ///
    /// Cheap by construction: an incremental delta append against the
    /// manager's remembered last write (no base re-read / replay), no git
    /// snapshot. No-op when no checkpoint manager or active checkpoint
    /// exists, or when every logged call since the last persist was
    /// read-only / failed — so it is safe to call after EVERY tool call.
    /// Best-effort like the periodic save: a failure is logged, never fatal.
    ///
    /// Call site: the post-tool-call hook `maybe_verify_file_change`
    /// (verification.rs), which the sequential dispatch path runs for every
    /// successful call after `log_tool_call` appended it to the checkpoint
    /// log. The step-end cadence save also
    /// persists whenever a mutation is pending (see
    /// [`Self::should_persist_checkpoint`]), so a missing per-call hook
    /// degrades to per-step granularity, never to the old interval.
    pub(crate) fn persist_checkpoint_after_mutation(&mut self) {
        if self.checkpoint_manager.is_none() || !self.has_unpersisted_mutation() {
            return;
        }
        let Some(task_description) = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.clone())
        else {
            return;
        };
        if let Err(e) = self.persist_checkpoint(&task_description, false, true) {
            warn!("Failed to persist post-mutation checkpoint: {}", e);
        }
    }

    /// Whether a successful mutating tool call was logged after the last
    /// persisted checkpoint — light or regular. The light watermark lives in
    /// the manager (what its last write put on disk), so light persists do
    /// not disturb the regular cadence counters.
    pub(crate) fn has_unpersisted_mutation(&self) -> bool {
        let Some(cp) = self.current_checkpoint.as_ref() else {
            return false;
        };
        let persisted = self
            .checkpoint_manager
            .as_ref()
            .and_then(|m| m.persisted_tool_call_count(&cp.task_id))
            .unwrap_or(0)
            .max(self.last_checkpoint_tool_calls);
        cp.tool_calls.iter().skip(persisted).any(|call| {
            call.success && {
                let args = serde_json::from_str::<serde_json::Value>(&call.arguments)
                    .unwrap_or(serde_json::Value::Null);
                super::tool_dispatch::tool_call_is_mutating(&call.tool_name, &args)
            }
        })
    }

    /// Save a checkpoint unconditionally, as a FULL write, bypassing the
    /// continuous-work cadence policy.
    ///
    /// Used at auto-continue boundaries (long-task caps): the in-process
    /// chain hands off to the same machinery as a manual `selfware resume`,
    /// so the on-disk state MUST be fresh before the next segment starts —
    /// the same guarantee the cancellation path enforces with its final
    /// save. A mid-chain crash (or an explicit resume) resumes from this
    /// write, not from a stale periodic checkpoint. The full-write (not
    /// delta) form guarantees the boundary-only fields — the persisted
    /// auto-continue chain count and the cumulative wall-clock total — land
    /// in the base checkpoint file rather than a differential log.
    ///
    /// Fails with a typed error when no checkpoint manager is configured
    /// (review finding): the auto-continue boundary is the ONE write whose
    /// absence would let a chained run claim "checkpointed" with nothing on
    /// disk, so unlike the best-effort periodic [`save_checkpoint`] this
    /// caller cannot silently no-op. The caller (maybe_auto_continue) treats
    /// the error as "the chain must not fire — there is no resume point to
    /// hand off to".
    pub(crate) fn save_checkpoint_forced(&mut self, task_description: &str) -> Result<()> {
        if self.checkpoint_manager.is_none() {
            anyhow::bail!(
                "cannot persist a forced checkpoint: no checkpoint manager is configured"
            );
        }
        self.persist_checkpoint(task_description, true, false)
    }

    /// Shared persist half of [`save_checkpoint`] / [`save_checkpoint_forced`].
    /// Callers guarantee a checkpoint manager is configured. `full_write`
    /// forces a complete base-file write ([`CheckpointManager::save_final`])
    /// instead of the differential save. `light` is the per-mutation form:
    /// it skips the repository snapshot and the in-memory self-healing
    /// snapshot (a full serialization of messages + tool calls), and it
    /// leaves the continuous-work cadence counters alone — so the next
    /// regular save still lands on the configured interval and refreshes
    /// both snapshots; a light persist can never postpone it.
    fn persist_checkpoint(
        &mut self,
        task_description: &str,
        full_write: bool,
        light: bool,
    ) -> Result<()> {
        let task_id = self
            .current_checkpoint
            .as_ref()
            .map(|c| c.task_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let checkpoint = self.build_checkpoint(&task_id, task_description, !light);
        // The manager borrow (and its IO) completes before any of the mutable
        // bookkeeping below touches the agent.
        let manager = self
            .checkpoint_manager
            .as_ref()
            .expect("caller checked manager");
        if full_write {
            manager.save_final(&checkpoint)?;
        } else {
            manager.save(&checkpoint)?;
        }
        if !light {
            self.last_checkpoint_tool_calls = checkpoint.tool_calls.len();
            self.last_checkpoint_persisted_at = Instant::now();
            self.checkpoint_persisted_once = true;
        }
        self.current_checkpoint = Some(checkpoint);
        #[cfg(feature = "resilience")]
        if !light {
            self.record_self_healing_checkpoint(task_description);
        }
        debug!("Checkpoint saved for task: {}", task_id);
        Ok(())
    }

    pub(super) fn should_persist_checkpoint(&self) -> bool {
        // Cancellation is the last chance to persist resumable state; normal
        // continuous-work cadence must never suppress this final save.
        if self.is_cancelled() {
            return true;
        }
        if !self.config.continuous_work.enabled {
            return true;
        }

        if !self.checkpoint_persisted_once {
            return true;
        }

        // Every successful mutation is persisted (W8a): the interval cadence
        // below only throttles saves of read-only progress. Losing an edit
        // that is already on disk makes a resumed run rewrite it blind.
        if self.has_unpersisted_mutation() {
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

        self.refresh_persisted_evidence();
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

    /// Stamp the live evidence state onto `current_checkpoint`.
    ///
    /// The terminal saves wrote `current_checkpoint` as it stood, and its
    /// guard counters were last refreshed by `to_checkpoint` — at the previous
    /// periodic save. Every edit and verification after that point was
    /// therefore absent from the final record: a run could fail verification,
    /// complete, and leave a checkpoint claiming the last verification passed.
    /// Both terminal paths call this first so what is persisted is what was
    /// true at the end.
    pub(crate) fn refresh_persisted_evidence(&mut self) {
        let counters = self.guard_counters_snapshot();
        let tokens = self
            .cumulative_token_usage
            .total
            .saturating_add(self.client.pending_usage().total_tokens);
        let wall = self.budget_elapsed_secs();
        let cost = self.cumulative_cost_usd + self.client.pending_usage().cost.unwrap_or(0.0);
        if let Some(checkpoint) = self.current_checkpoint.as_mut() {
            checkpoint.guard_counters = counters;
            checkpoint.cumulative_tokens = tokens;
            checkpoint.elapsed_wall_secs = wall;
            checkpoint.cumulative_cost_usd = cost;
            // The adaptive cap/grants and the chain-wide iteration total move
            // between periodic saves (a grant fires exactly when the cap
            // trips) — stamp them at the terminal write too.
            checkpoint.effective_max_iterations = Some(self.loop_control.max_iterations());
            checkpoint.extensions_granted = self.loop_control.extensions_granted();
            checkpoint.cumulative_iterations = self.loop_control.accumulated_iterations();
            // Same sweep for the remaining fields `build_checkpoint` writes:
            // the chain count and the hard caps must not lag the terminal
            // record either.
            checkpoint.auto_continue_count = self.loop_control.auto_continue_count();
            checkpoint.max_budget_tokens = self.config.agent.max_budget_tokens;
            checkpoint.max_wall_secs = self.config.agent.max_wall_secs;
            checkpoint.max_cost_usd = self.config.agent.max_cost_usd;
        }
    }

    /// The persisted form of the anti-thrash guards and the verification
    /// ledger, including the workspace fingerprint the verification credit
    /// rests on. Single source for every checkpoint write
    /// ([`Self::build_checkpoint`], [`Self::refresh_persisted_evidence`]).
    fn guard_counters_snapshot(&self) -> crate::checkpoint::GuardCounters {
        crate::checkpoint::GuardCounters {
            consecutive_no_action_prompts: self.consecutive_no_action_prompts,
            mutation_gate_rejections: self.mutation_gate_rejections,
            prefill_400_count: self.prefill_400_count,
            mutation_sequence: self.mutation_sequence,
            last_successful_verification_mutation_sequence: self
                .last_successful_verification_mutation_sequence,
            last_failed_verification_mutation_sequence: self
                .last_failed_verification_mutation_sequence,
            last_failed_verification_summary: self.last_failed_verification_summary.clone(),
            verification_failures: self.verification_failures.clone(),
            verification_fingerprint: self.verification_fingerprint(),
        }
    }

    /// Fingerprint of the workspace the current verification credit covers:
    /// repository HEAD plus the content of every file this task wrote (per
    /// the checkpoint log), captured at checkpoint time. `None` while no
    /// credit exists — there is nothing to protect.
    ///
    /// Taken at checkpoint time rather than at the instant of the pass: with
    /// no task mutation after the pass the two are the same tree, and with
    /// later mutations the credit is either already stale (the gate refuses
    /// regardless) or rests on [`Self::fresh_authoritative_pass`]'s doc-only
    /// proof, which is again a statement about the tree at checkpoint time.
    /// What resume must detect is the tree changing after the checkpoint.
    fn verification_fingerprint(&self) -> Option<crate::checkpoint::WorkspaceFingerprint> {
        if self.last_successful_verification_mutation_sequence == 0 {
            return None;
        }
        let written = self
            .current_checkpoint
            .as_ref()
            .map(|cp| prior_segment_written_paths(&cp.tool_calls))
            .unwrap_or_default();
        Some(crate::checkpoint::WorkspaceFingerprint::capture(
            &crate::tools::workspace_root::current_path(),
            &written,
        ))
    }

    /// Resume-time check that the restored verification credit still
    /// describes the files on disk. Recomputes the workspace fingerprint and
    /// compares it with the one persisted next to the credit; on any
    /// difference — or when the checkpoint predates the fingerprint — the
    /// credit is revoked (`last_successful_verification_mutation_sequence`
    /// reset to 0, so the completion gate reports StaleVerification and
    /// [`Self::fresh_authoritative_pass`] has no pass to build on) and a
    /// directive for the model is returned. `None` when there was no credit
    /// or it still holds.
    pub(super) fn revalidate_restored_verification_credit(
        &mut self,
        stored: Option<&crate::checkpoint::WorkspaceFingerprint>,
        written: &[String],
    ) -> Option<String> {
        if self.last_successful_verification_mutation_sequence == 0 {
            return None;
        }
        let current = crate::checkpoint::WorkspaceFingerprint::capture(
            &crate::tools::workspace_root::current_path(),
            written,
        );
        let reason = match stored {
            Some(stored) if *stored == current => return None,
            Some(stored) => {
                let diffs = stored.differences(&current);
                let shown: Vec<String> = diffs.iter().take(10).map(|d| format!("- {d}")).collect();
                let more = diffs.len().saturating_sub(shown.len());
                let more_line = if more > 0 {
                    format!("\n- … and {more} more")
                } else {
                    String::new()
                };
                format!(
                    "The workspace changed while the task was paused:\n{}{more_line}",
                    shown.join("\n")
                )
            }
            None => "This checkpoint does not record which workspace state the earlier \
                     verification covered, so it cannot be trusted after the pause."
                .to_string(),
        };
        warn!(
            "Revoking restored verification credit (mutation #{}): workspace fingerprint mismatch",
            self.last_successful_verification_mutation_sequence
        );
        self.last_successful_verification_mutation_sequence = 0;
        Some(format!(
            "<selfware_system_directive>\n\
             Resumed from a checkpoint. A verification passed before the pause, but that result \
             no longer counts. {reason}\n\
             Re-read the affected files, then re-run this project's verification and let it pass \
             before completing.\n\
             </selfware_system_directive>"
        ))
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
        self.refresh_persisted_evidence();
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
            "tool_calls": self.current_checkpoint.as_ref().map(|checkpoint| &checkpoint.tool_calls),
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

        let Ok(mut messages) = serde_json::from_value::<Vec<Message>>(messages_value) else {
            return false;
        };
        let logs = state
            .get("tool_calls")
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<Vec<crate::checkpoint::ToolCallLog>>(value).ok()
            })
            .unwrap_or_default();
        self.sanitize_restored_tool_messages(&mut messages, &logs);
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

/// Paths written by the successful file-writing calls on record, in first-
/// write order, deduplicated. Source of both the resumed files-changed
/// evidence and the workspace-refresh note.
pub(super) fn prior_segment_written_paths(
    tool_calls: &[crate::checkpoint::ToolCallLog],
) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for tool_call in tool_calls {
        if !tool_call.success {
            continue;
        }
        let Ok(args) = serde_json::from_str::<serde_json::Value>(&tool_call.arguments) else {
            continue;
        };
        for path in super::tool_dispatch::written_paths_for_tool_call(&tool_call.tool_name, &args) {
            let path = path.to_string_lossy().to_string();
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Most paths the workspace-refresh note lists by name; the rest are counted.
const WORKSPACE_REFRESH_MAX_PATHS: usize = 30;

/// The short directive injected into a resumed context listing the files
/// earlier segments wrote, or `None` when they wrote nothing. The listing is
/// what the checkpoint RECORDS as written — the note tells the model to
/// re-read rather than asserting the current on-disk content.
pub(super) fn workspace_refresh_note(written: &[String]) -> Option<String> {
    if written.is_empty() {
        return None;
    }
    let shown: Vec<String> = written
        .iter()
        .take(WORKSPACE_REFRESH_MAX_PATHS)
        .map(|p| format!("- {p}"))
        .collect();
    let more = written.len().saturating_sub(shown.len());
    let more_line = if more > 0 {
        format!("\n- … and {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "<selfware_system_directive>\n\
         Resumed from a checkpoint. Earlier segment(s) of this task already wrote, edited or deleted \
         these files (per the checkpoint log):\n{}{more_line}\n\
         Re-read a file (file_read) before changing it; do NOT recreate or rewrite one of \
         these from scratch without reading its current contents first.\n\
         </selfware_system_directive>",
        shown.join("\n")
    ))
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

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_resume_chain_test.rs"]
mod resume_chain_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_resume_verification_test.rs"]
mod resume_verification_tests;

#[cfg(all(test, feature = "consolidation"))]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_consolidate_utf8_test.rs"]
mod consolidate_utf8_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/checkpointing/checkpointing_trust_test.rs"]
mod trust_tests;
