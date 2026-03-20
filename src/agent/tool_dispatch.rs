use std::hash::{Hash, Hasher};

use anyhow::Result;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::api::types::Message;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::self_improvement::Outcome;
use crate::errors::AgentError;
use crate::hooks::HookContext;

pub(super) const TOOL_CONFIRM_ARGS_PREVIEW_CHARS: usize = 240;
pub(super) const TOOL_FAILURE_HINT_PREVIEW_CHARS: usize = 400;
pub(super) const FAILED_TOOL_ATTEMPT_WINDOW_SIZE: usize = 16;

pub(super) fn truncate_chars(s: &str, max_chars: usize) -> String {
    let collected: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{}...", collected)
    } else {
        collected
    }
}

pub(super) fn canonicalize_tool_args(args_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_str)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| args_str.to_string())
}

pub(super) fn hash_tool_args(args_str: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonicalize_tool_args(args_str).hash(&mut hasher);
    hasher.finish()
}

impl Agent {
    pub(super) fn push_task_state_note(&mut self, note: String) {
        if self.task_state_notes.back() == Some(&note) {
            return;
        }
        if self.task_state_notes.len() == TASK_STATE_NOTE_LIMIT {
            self.task_state_notes.pop_front();
        }
        self.task_state_notes.push_back(note);
    }

    pub(super) fn clear_task_state_memory(&mut self) {
        self.file_tracker.read_state.clear();
        self.task_state_notes.clear();
    }

    fn remember_failed_tool(&mut self, tool_name: &str, error: &str) {
        let error_preview = truncate_chars(error, TOOL_FAILURE_HINT_PREVIEW_CHARS);
        self.pending_failure_hint = Some(format!(
            "Warning: the previous tool call `{}` failed. Do not claim success, observation, or completion unless a later tool actually confirms it. Failure details: {}",
            tool_name, error_preview
        ));
    }

    fn build_failed_tool_retry_suppressed_message(&self, failure: &FailedToolAttempt) -> String {
        let schema_hint = self
            .tools
            .get(&failure.tool_name)
            .and_then(|tool| {
                let required: Vec<String> = tool
                    .schema()
                    .get("required")
                    .and_then(|value| value.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|value| value.as_str())
                    .map(|field| format!("`{}`", field))
                    .collect();
                (!required.is_empty())
                    .then(|| format!(" Required top-level fields: {}.", required.join(", ")))
            })
            .unwrap_or_default();

        match failure.failure_kind {
            "parsing" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed because the arguments were not valid JSON.{} Change the JSON before retrying. Last error: {}",
                failure.tool_name, schema_hint, failure.error_preview
            ),
            "validation" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed schema validation.{} Change the arguments before retrying. Last error: {}",
                failure.tool_name, schema_hint, failure.error_preview
            ),
            "safety" => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed the safety check. Change the tool or arguments before retrying. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            other => format!(
                "RETRY SUPPRESSED: `{}` with these exact arguments already failed due to {}. Do not rerun it until a different successful tool call changes the situation or you change the inputs. Last error: {}",
                failure.tool_name, other, failure.error_preview
            ),
        }
    }

    pub(super) fn record_failed_tool_attempt(
        &mut self,
        tool_name: &str,
        args_str: &str,
        failure_kind: &'static str,
        error: &str,
    ) {
        let args_hash = hash_tool_args(args_str);
        let error_preview = truncate_chars(error, TOOL_FAILURE_HINT_PREVIEW_CHARS);
        self.recent_failed_tool_attempts.retain(|existing| {
            !(existing.tool_name == tool_name
                && existing.args_hash == args_hash
                && existing.failure_kind == failure_kind)
        });
        self.recent_failed_tool_attempts
            .push_back(FailedToolAttempt {
                tool_name: tool_name.to_string(),
                args_hash,
                failure_kind,
                error_preview,
            });
        if self.recent_failed_tool_attempts.len() > FAILED_TOOL_ATTEMPT_WINDOW_SIZE {
            self.recent_failed_tool_attempts.pop_front();
        }
    }

    pub(super) fn clear_failed_tool_attempts(&mut self) {
        self.recent_failed_tool_attempts.clear();
    }

    pub(super) fn maybe_block_redundant_reread(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        if name != "file_read" {
            return false;
        }

        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            return false;
        };
        let Some(state) = self.file_tracker.read_state.get(path) else {
            return false;
        };
        if state.unchanged_read_count < 1 || self.file_tracker.stale_files.contains(path) {
            return false;
        }

        let current_mtime = std::fs::metadata(path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());

        if current_mtime != state.last_modified {
            return false;
        }

        let err = format!(
            "Repeated unchanged reread blocked: `{}` has already been read unchanged {} times in this task. Use the content already in context or make the edit now instead of reading it again.",
            path,
            state.unchanged_read_count + 1
        );
        self.push_task_state_note(format!(
            "Blocked redundant reread of `{}` after {} unchanged reads",
            path,
            state.unchanged_read_count + 1
        ));
        self.pending_failure_hint = Some(err.clone());
        self.push_tool_result_message(use_native_fc, call_id, name, false, &err);
        self.log_tool_call(name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(name, &err);
        self.record_failed_tool_attempt(name, args_str, "task_state", &err);
        true
    }

    pub(super) fn track_task_state_after_tool(
        &mut self,
        name: &str,
        args: &Value,
        result: &str,
        success: bool,
    ) {
        if !success {
            return;
        }

        let Some(path) = args.get("path").and_then(|v| v.as_str()) else {
            return;
        };
        let path_str = path.to_string();

        match name {
            "file_read" => {
                let Ok(json) = serde_json::from_str::<Value>(result) else {
                    return;
                };
                let Some(content) = json.get("content").and_then(|v| v.as_str()) else {
                    return;
                };
                let total_lines = json
                    .get("total_lines")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let content_hash = super::recovery::hash_text_signature(content);
                let last_modified = std::fs::metadata(&path_str)
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs());

                let mut unchanged_count = 0;
                if let Some(state) = self.file_tracker.read_state.get_mut(&path_str) {
                    if state.content_hash == content_hash
                        && state.last_modified == last_modified
                        && !self.file_tracker.stale_files.contains(&path_str)
                    {
                        state.unchanged_read_count += 1;
                        unchanged_count = state.unchanged_read_count;
                    } else {
                        state.content_hash = content_hash;
                        state.total_lines = total_lines;
                        state.last_modified = last_modified;
                        state.unchanged_read_count = 0;
                    }
                } else {
                    self.file_tracker.read_state.insert(
                        path_str.clone(),
                        FileReadState {
                            content_hash,
                            total_lines,
                            last_modified,
                            unchanged_read_count: 0,
                        },
                    );
                }

                if unchanged_count > 0 {
                    self.push_task_state_note(format!(
                        "Reread unchanged file `{}` ({}x consecutive unchanged reads)",
                        path_str,
                        unchanged_count + 1
                    ));
                }

                if unchanged_count >= 1 {
                    self.pending_failure_hint = Some(format!(
                        "You have reread unchanged file `{}` {} times in this task. Unless something outside the agent changed it, use the content already in context or make the edit now instead of reading it again.",
                        path_str,
                        unchanged_count + 1
                    ));
                }
            }
            "file_write" | "file_edit" => {
                self.file_tracker.mark_written(&path_str);
                self.push_task_state_note(format!(
                    "Marked `{}` as changed; future rereads should expect new content",
                    path_str
                ));
            }
            "file_delete" => {
                self.file_tracker.remove_deleted(&path_str);
                self.push_task_state_note(format!(
                    "Removed deleted file `{}` from task-state tracking",
                    path_str
                ));
            }
            _ => {}
        }
    }

    pub(super) fn suppress_repeated_failed_tool_retry(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        let args_hash = hash_tool_args(args_str);
        let Some(failure) = self
            .recent_failed_tool_attempts
            .iter()
            .rev()
            .find(|attempt| attempt.tool_name == tool_name && attempt.args_hash == args_hash)
            .cloned()
        else {
            return false;
        };

        let err = self.build_failed_tool_retry_suppressed_message(&failure);
        warn!(
            "Suppressing repeated failed tool call for '{}' after prior {} failure",
            tool_name, failure.failure_kind
        );
        cli_println!("{} {}", "✗".bright_red(), err);
        self.push_tool_result_message(use_native_fc, call_id, tool_name, false, &err);
        self.log_tool_call(tool_name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(tool_name, &err);
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.self_improvement.record_tool(
            tool_name,
            self.learning_context(),
            Outcome::Failure,
            duration_ms,
            Some(err.clone()),
        );
        self.self_improvement.record_error(
            &err,
            "retry_suppressed",
            self.learning_context(),
            tool_name,
            None,
        );
        true
    }

    pub(super) async fn execute_tool_batch(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<()> {
        use crate::hooks::HookAction;
        use super::tui_events::AgentEvent;

        for (name, args_str, tool_call_id) in tool_calls {
            if self.is_cancelled() {
                break;
            }

            let start_time = std::time::Instant::now();
            if let Some(warning) = self
                .self_improvement
                .check_for_errors(&name, self.learning_context())
                .into_iter()
                .next()
                .filter(|w| w.likelihood >= 0.7)
            {
                warn!(
                    "Self-improvement warning before {}: potential {} pattern ({}%)",
                    name,
                    warning.error_type,
                    (warning.likelihood * 100.0) as u32
                );
            }

            let (call_id, use_native_fc, fake_call) =
                self.build_tool_call_context(&name, &args_str, tool_call_id);

            if self.suppress_repeated_failed_tool_retry(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
            ) {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if let Err(e) = self.safety.check_tool_call(&fake_call) {
                let error_msg = format!("Safety check failed: {}", e);
                let spinner = crate::ui::spinner::TerminalSpinner::start(&error_msg);
                spinner.stop_error(&error_msg);
                crate::output::safety_blocked(&error_msg);
                // Audit: log safety block
                if let Some(ref logger) = self.audit_logger {
                    logger.log_safety_block(&name, &error_msg);
                }
                self.push_tool_result_message(use_native_fc, &call_id, &name, false, &error_msg);
                self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                self.remember_failed_tool(&name, &error_msg);
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.self_improvement.record_tool(
                    &name,
                    self.learning_context(),
                    Outcome::Failure,
                    duration_ms,
                    Some(error_msg.clone()),
                );
                self.self_improvement.record_error(
                    &error_msg,
                    "safety",
                    self.learning_context(),
                    &name,
                    None,
                );
                self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
                continue;
            }

            let args =
                match self.parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time) {
                    Some(args) => args,
                    None => {
                        self.emit_event(AgentEvent::ToolCompleted {
                            name: name.clone(),
                            success: false,
                            duration_ms: start_time.elapsed().as_millis() as u64,
                        });
                        continue;
                    }
                };

            if !self.validate_tool_args(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            ) {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if self.maybe_block_redundant_reread(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            ) {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if !self.confirm_tool_execution(&name, &args_str, &call_id, use_native_fc).await? {
                continue;
            }

            // Fire PreToolUse hooks (may skip execution)
            let pre_ctx = HookContext::pre_tool(&name, &args_str);
            if let HookAction::Skip { reason } = self.hook_registry.fire(&pre_ctx).await {
                let skip_msg = format!("Tool skipped by PreToolUse hook: {}", reason);
                info!("{}", skip_msg);
                self.push_tool_result_message(use_native_fc, &call_id, &name, false, &skip_msg);
                continue;
            }

            self.emit_event(AgentEvent::ToolStarted { name: name.clone() });

            let activity = crate::output::tool_activity_message(&name, &args);
            let spinner = crate::ui::spinner::TerminalSpinner::start(&activity);
            let (success, result, summary) = self
                .execute_single_tool(&name, &args_str, &args, start_time)
                .await?;

            let duration_ms = start_time.elapsed().as_millis() as u64;
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success,
                duration_ms,
            });

            if success {
                spinner.stop_success(&summary);
            } else {
                spinner.stop_error(&summary);
            }

            // Store for progressive disclosure via /last
            {
                let exit_code = serde_json::from_str::<serde_json::Value>(&result)
                    .ok()
                    .and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()))
                    .map(|c| c as i32);
                self.store_last_tool_output(crate::agent::last_tool::LastToolOutput {
                    tool_name: name.clone(),
                    summary: summary.clone(),
                    full_output: result.clone(),
                    success,
                    exit_code,
                    duration_ms,
                });
            }

            let tool_outcome = if success {
                Outcome::Success
            } else {
                Outcome::Failure
            };
            let tool_error = (!success).then(|| result.clone());
            self.self_improvement.record_tool(
                &name,
                self.learning_context(),
                tool_outcome,
                duration_ms,
                tool_error.clone(),
            );
            if let Some(error_text) = tool_error {
                self.self_improvement.record_error(
                    &error_text,
                    Self::classify_error_type(&error_text),
                    self.learning_context(),
                    &name,
                    None,
                );
            }
            if success {
                self.clear_failed_tool_attempts();
            } else {
                self.record_failed_tool_attempt(&name, &args_str, "execution", &result);
            }

            self.track_task_state_after_tool(&name, &args, &result, success);

            // Track file operations for context management
            if success {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let path_str = path.to_string();
                    match name.as_str() {
                        "file_read" => {
                            if self.file_tracker.context_files.len() < 500
                                && !self.file_tracker.context_files.contains(&path_str)
                            {
                                self.file_tracker.context_files.push(path_str.clone());
                            }
                            // Track in hierarchical context map for budget-aware management.
                            if let Some(content) = serde_json::from_str::<serde_json::Value>(&result)
                                .ok()
                                .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(String::from))
                            {
                                self.track_file_read_in_context_map(&path_str, &content);
                            }
                        }
                        "file_delete" => {
                            self.file_tracker.remove_deleted(&path_str);
                        }
                        "file_write" | "file_edit" => {
                            self.file_tracker.mark_stale(&path_str);
                        }
                        _ => {}
                    }
                }
            }

            self.push_tool_result_message(use_native_fc, &call_id, &name, success, &result);
            if !success {
                self.remember_failed_tool(&name, &result);
            }

            // Reset no-action counter - the model attempted to use a tool
            // (even if it failed, this counts as taking action)
            self.reset_no_action_prompt_state();

            // Add post-error guidance for failed tools to help model recover
            if !success {
                let recovery_hint = self.build_error_recovery_hint(&name, &result);
                self.messages.push(Message::user(recovery_hint));
            }

            // Fire PostToolUse hooks (e.g., auto-format, lint, auto-commit)
            let post_ctx = HookContext::post_tool(&name, &args_str, success, &result);
            self.hook_registry.fire(&post_ctx).await;

            // Audit: log tool execution
            if let Some(ref logger) = self.audit_logger {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                args_str.hash(&mut hasher);
                let args_hash = format!("{:x}", hasher.finish());
                logger.log_tool_execution(&name, &args_hash, success, duration_ms, None);
            }
        }

        Ok(())
    }

    /// Execute a context management tool (operates on agent state, not filesystem).
    fn execute_context_tool(&mut self, name: &str, args: &serde_json::Value) -> serde_json::Value {
        use crate::tools::context::*;

        match name {
            CONTEXT_STATUS => {
                let stats = self.context_map.stats();
                serde_json::json!({
                    "total_tokens": stats.total_tokens,
                    "budget": stats.budget,
                    "usage_pct": format!("{:.1}%", (stats.total_tokens as f64 / stats.budget.max(1) as f64) * 100.0),
                    "remaining": self.context_map.remaining(),
                    "l1_tree": { "count": stats.l1_count, "tokens": stats.l1_tokens },
                    "l2_skeleton": { "count": stats.l2_count, "tokens": stats.l2_tokens },
                    "l3_full": { "count": stats.l3_count, "tokens": stats.l3_tokens },
                    "compression_headroom": self.context_map.compression_headroom(),
                    "thinking_reserve": self.context_map.thinking_reserve(),
                })
            }
            CONTEXT_FOCUS => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let max_files = args
                    .get("max_files")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;

                let to_promote = self.context_map.focus_on_query(query, max_files);

                // Actually load the files that need promoting.
                let root = super::current_project_root();
                let mut loaded = Vec::new();
                for path in &to_promote {
                    let full_path = root.join(path);
                    if let Ok(content) = std::fs::read_to_string(&full_path) {
                        self.context_map.load_full(path, content);
                        loaded.push(path.to_string_lossy().to_string());
                    }
                }

                let stats = self.context_map.stats();
                serde_json::json!({
                    "promoted": loaded,
                    "query": query,
                    "total_tokens_after": stats.total_tokens,
                    "budget": stats.budget,
                })
            }
            CONTEXT_EVICT => {
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let freed = self
                    .context_map
                    .evict_to_tree(std::path::Path::new(path));
                serde_json::json!({
                    "evicted": path,
                    "tokens_freed": freed,
                    "remaining": self.context_map.remaining(),
                })
            }
            CONTEXT_RECOMMEND => {
                let task = args
                    .get("task")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let rec = self.context_map.recommend_context(task);
                serde_json::json!({
                    "modality": rec.modality_description,
                    "potential_savings": rec.potential_token_savings,
                    "promote": rec.promote.iter().map(|s| serde_json::json!({
                        "path": s.path.to_string_lossy(),
                        "from": format!("{:?}", s.current_level),
                        "to": format!("{:?}", s.suggested_level),
                        "reason": s.reason,
                        "estimated_tokens": s.estimated_tokens,
                    })).collect::<Vec<_>>(),
                    "evict": rec.evict.iter().map(|s| serde_json::json!({
                        "path": s.path.to_string_lossy(),
                        "from": format!("{:?}", s.current_level),
                        "to": format!("{:?}", s.suggested_level),
                        "reason": s.reason,
                    })).collect::<Vec<_>>(),
                })
            }
            CONTEXT_LOAD_SKELETON => {
                let path_str = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let path = std::path::Path::new(path_str);
                let root = super::current_project_root();
                let full_path = root.join(path);

                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        let skeleton =
                            super::context_map::extract_rust_skeleton(path, &content);
                        let rendered = skeleton.render();
                        let token_count = skeleton.token_count;
                        self.context_map.load_skeleton(path, skeleton);
                        serde_json::json!({
                            "path": path_str,
                            "skeleton": rendered,
                            "token_count": token_count,
                            "level": "L2",
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "error": format!("Failed to read {}: {}", path_str, e),
                        })
                    }
                }
            }
            _ => serde_json::json!({ "error": format!("Unknown context tool: {}", name) }),
        }
    }

    pub(super) fn build_tool_call_context(
        &self,
        name: &str,
        args_str: &str,
        tool_call_id: Option<String>,
    ) -> (String, bool, crate::api::types::ToolCall) {
        let use_native_fc = self.config.agent.native_function_calling && tool_call_id.is_some();
        let call_id = tool_call_id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
        let fake_call = crate::api::types::ToolCall {
            id: call_id.clone(),
            call_type: "function".to_string(),
            function: crate::api::types::ToolFunction {
                name: name.to_string(),
                arguments: args_str.to_string(),
            },
        };
        (call_id, use_native_fc, fake_call)
    }

    async fn confirm_tool_execution(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
    ) -> Result<bool> {
        if !self.needs_confirmation(name) {
            return Ok(true);
        }

        // When TUI is active, auto-approve — the TUI can't show stdin prompts
        if self.has_tui_renderer() {
            return Ok(true);
        }

        use tokio::io::AsyncWriteExt;

        let args_preview: String = args_str
            .chars()
            .take(TOOL_CONFIRM_ARGS_PREVIEW_CHARS)
            .collect();
        let args_display = if args_str.chars().count() > TOOL_CONFIRM_ARGS_PREVIEW_CHARS {
            format!("{}...", args_preview)
        } else {
            args_preview
        };

        if !self.is_interactive() {
            return Err(AgentError::ConfirmationRequired {
                tool_name: name.to_string(),
            }
            .into());
        }

        cli_println!(
            "{} Tool: {} Args: {}",
            "⚠️".bright_yellow(),
            name.bright_cyan(),
            args_display.bright_white()
        );
        print!(
            "{}",
            "\n\x1b[0m\x1b[1m\x1b[97mExecute? [y/N/s(bypass permissions)]: \x1b[0m"
        );
        let _ = tokio::io::stdout().flush().await;

        let response = super::execution::read_line_pausing_esc(&self.esc_paused, &self.esc_pause_ack).await;
        if let Ok(response) = response {
            let response = response.trim().to_lowercase();
            match response.as_str() {
                "y" | "yes" => return Ok(true),
                "s" | "skip" => {
                    self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                    cli_println!(
                        "{} Switched to YOLO mode for this session",
                        "⚡".bright_yellow()
                    );
                    return Ok(true);
                }
                _ => {}
            }
        }

        let skip_msg = "Tool execution skipped by user";
        cli_println!("{} {}", "⏭️".bright_yellow(), skip_msg);
        if use_native_fc {
            self.messages.push(Message::tool(
                serde_json::json!({"skipped": skip_msg}).to_string(),
                call_id,
            ));
        } else {
            self.messages.push(Message::user(format!(
                "<tool_result><skipped>{}</skipped></tool_result>",
                skip_msg
            )));
        }
        Ok(false)
    }

    pub(super) fn parse_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> Option<Value> {
        match serde_json::from_str(args_str) {
            Ok(args) => {
                debug!("Tool arguments: {}", args);
                Some(args)
            }
            Err(e) => {
                let err = format!("Invalid JSON arguments: {}", e);
                cli_println!("{} {}", "✗".bright_red(), err);
                self.push_tool_result_message(use_native_fc, call_id, name, false, &err);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.log_tool_validation_failure_event(
                    name,
                    args_str,
                    &err,
                    call_id,
                    use_native_fc,
                );
                self.remember_failed_tool(name, &err);
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    duration_ms,
                    Some(err.clone()),
                );
                self.self_improvement.record_error(
                    &err,
                    "parsing",
                    self.learning_context(),
                    name,
                    None,
                );
                self.record_failed_tool_attempt(name, args_str, "parsing", &err);
                None
            }
        }
    }

    pub(super) fn validate_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        let Some(tool) = self.tools.get(name) else {
            return true;
        };

        match crate::tools::validate_tool_arguments_schema(name, &tool.schema(), args) {
            Ok(()) => true,
            Err(e) => {
                let err = e.to_string();
                cli_println!("{} {}", "✗".bright_red(), err);
                self.push_tool_result_message(use_native_fc, call_id, name, false, &err);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.log_tool_validation_failure_event(
                    name,
                    args_str,
                    &err,
                    call_id,
                    use_native_fc,
                );
                self.remember_failed_tool(name, &err);
                let duration_ms = start_time.elapsed().as_millis() as u64;
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    duration_ms,
                    Some(err.clone()),
                );
                self.self_improvement.record_error(
                    &err,
                    "validation",
                    self.learning_context(),
                    name,
                    None,
                );
                self.record_failed_tool_attempt(name, args_str, "validation", &err);
                false
            }
        }
    }

    pub(super) async fn execute_single_tool(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        start_time: std::time::Instant,
    ) -> Result<(bool, String, String)> {
        // Intercept context management tools — they operate on agent state,
        // not the filesystem, so they bypass the normal tool registry.
        if crate::tools::context::is_context_tool(name) {
            let result = self.execute_context_tool(name, args);
            let elapsed = start_time.elapsed().as_millis() as u64;
            let result_str = serde_json::to_string(&result)?;
            let summary =
                crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
            self.log_tool_call(name, args_str, &result_str, true, start_time, true);
            return Ok((true, result_str, summary));
        }

        let Some(tool) = self.tools.get(name) else {
            let err = format!("Unknown tool: {}", name);
            self.log_tool_call(name, args_str, &err, false, start_time, false);
            return Ok((false, err.clone(), err));
        };

        // Check ToolCache for cacheable (read-only) tools
        let is_cacheable = crate::session::cache::is_cacheable(name);
        if is_cacheable {
            if let Some(cached_value) = self.cache_manager.tool_cache.get(name, args) {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&cached_value)?;
                let summary =
                    crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
                self.log_tool_call(name, args_str, &result_str, true, start_time, true);
                debug!("Cache hit for tool '{}' ({}ms)", name, elapsed);
                return Ok((true, result_str, summary));
            }
        }

        // Invalidate cache entries when a mutating tool targets a specific path
        if crate::session::cache::invalidates_cache(name) {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                self.cache_manager.invalidate_path(path);
            }
            // shell_exec and git operations can affect any file — clear all read caches
            if matches!(name, "shell_exec" | "git_commit" | "git_checkout") {
                self.cache_manager.tool_cache.clear();
            }
        }

        // Snapshot file before edit/write for undo support + diff display.
        let pre_edit_content: Option<(String, String)> = if matches!(name, "file_edit" | "file_write" | "file_delete") {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    use crate::session::edit_history::{EditAction, FileSnapshot};
                    let snapshot = FileSnapshot::new(std::path::PathBuf::from(path), content.clone());
                    let action = EditAction::FileEdit {
                        path: std::path::PathBuf::from(path),
                        tool: name.to_string(),
                    };
                    self.edit_history.create_checkpoint(action);
                    self.edit_history.add_file_to_current(snapshot);
                    Some((path.to_string(), content))
                } else {
                    // New file (file_write to nonexistent path)
                    Some((path.to_string(), String::new()))
                }
            } else {
                None
            }
        } else {
            None
        };

        // Acquire concurrency governor permit before executing the tool.
        // The permit is held for the duration of execution and released on drop.
        let _tool_permit = self
            .governor
            .acquire_tool()
            .await
            .map_err(|e| anyhow::anyhow!("concurrency governor error: {}", e))?;

        // Track bash/shell commands for the sticky status bar.
        // The guard decrements on drop regardless of how execution exits.
        let is_bash = matches!(name, "shell_exec" | "pty_shell");
        let _bash_guard: Option<crate::ui::sticky_bar::BashGuard> = if is_bash {
            Some(crate::ui::sticky_bar::BashGuard::new())
        } else {
            None
        };

        let timeout_secs = self.config.agent.step_timeout_secs.max(1);
        let execution = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            tool.execute(args.clone()),
        )
        .await;

        match execution {
            Ok(Ok(result)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&result)?;
                let summary =
                    crate::output::semantic_summary(name, args, Some(&result_str), true, elapsed);
                self.log_tool_call(name, args_str, &result_str, true, start_time, true);

                // Store successful cacheable results in ToolCache
                if is_cacheable {
                    self.cache_manager.tool_cache.set(name, args, result.clone());
                }

                // Cache tool results in LocalFirstCoordinator
                let cache_key = crate::session::cache::ToolCache::cache_key(name, args);
                self.cache_manager.local_first
                    .cache_response(&cache_key, result_str.clone(), result_str.len());

                // Display color-coded diff for file mutations
                if let Some((ref path, ref old_content)) = pre_edit_content {
                    if matches!(name, "file_edit" | "file_write") {
                        if let Ok(new_content) = std::fs::read_to_string(path) {
                            crate::output::display_file_diff(path, old_content, &new_content);
                        }
                    }
                }

                // Record successful tool usage for learning
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Success,
                    elapsed,
                    None,
                );

                let verification_result = self.maybe_verify_file_change(name, args).await;
                let enhanced_result = self.maybe_enhance_tool_result(name, &result_str);
                let final_result = match verification_result {
                    Some(ver_msg) => format!("{}{}", enhanced_result, ver_msg),
                    None => enhanced_result,
                };
                Ok((true, final_result, summary))
            }
            Ok(Err(e)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let summary =
                    crate::output::semantic_summary(name, args, Some(&e.to_string()), false, elapsed);
                self.log_tool_call(name, args_str, &e.to_string(), false, start_time, false);
                self.cognitive_state
                    .episodic_memory
                    .what_failed(name, &e.to_string());

                // Record failed tool usage for learning
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    elapsed,
                    Some(e.to_string()),
                );

                Ok((false, e.to_string(), summary))
            }
            Err(_) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let err = format!("Tool '{}' timed out after {}s", name, timeout_secs);
                let summary = crate::output::semantic_summary(name, args, Some(&err), false, elapsed);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                self.cognitive_state.episodic_memory.what_failed(name, &err);
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    Outcome::Failure,
                    elapsed,
                    Some(err.clone()),
                );
                Ok((false, err, summary))
            }
        }
    }

    pub(super) fn push_tool_result_message(
        &mut self,
        use_native_fc: bool,
        call_id: &str,
        _tool_name: &str,
        success: bool,
        result: &str,
    ) {
        // Detect base64_png in successful tool results and promote to multimodal
        if success {
            if let Some(base64_png) = super::execution::try_extract_base64_png(result) {
                let summary = super::execution::build_image_result_summary(result);
                let content =
                    crate::api::types::MessageContent::from_text(&summary).with_image(&base64_png);
                if use_native_fc {
                    // For native FC, create a tool message with multimodal content
                    self.messages.push(crate::api::types::Message {
                        role: "tool".to_string(),
                        content,
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(call_id.to_string()),
                        name: None,
                    });
                } else {
                    self.messages.push(Message::user_multimodal(content));
                }
                return;
            }
        }

        if use_native_fc {
            let result_json = if success {
                result.to_string()
            } else {
                serde_json::json!({"error": result}).to_string()
            };
            self.messages.push(Message::tool(result_json, call_id));
        } else {
            let formatted = if success {
                format!("<tool_result>{}</tool_result>", result)
            } else {
                format!("<tool_result><error>{}</error></tool_result>", result)
            };
            self.messages.push(Message::user(formatted));
        }
    }

    pub(super) fn log_tool_call(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        success: bool,
        start_time: std::time::Instant,
        truncate_result: bool,
    ) {
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.log_session_tool_call_event(
            tool_name,
            arguments,
            result,
            success,
            duration_ms,
            truncate_result,
        );

        if let Some(ref mut checkpoint) = self.current_checkpoint {
            let logged_result = if truncate_result {
                result.chars().take(1000).collect()
            } else {
                result.to_string()
            };
            checkpoint.log_tool_call(ToolCallLog {
                timestamp: chrono::Utc::now(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
                result: Some(logged_result),
                success,
                duration_ms: Some(duration_ms),
            });
        }
    }
}
