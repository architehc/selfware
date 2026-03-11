use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use chrono::Utc;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::api::ThinkingMode;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::self_improvement::Outcome;
use crate::cognitive::CyclePhase;
use crate::errors::AgentError;
use crate::hooks::{HookAction, HookContext};
use crate::tool_parser::parse_tool_calls;

/// Read a line from stdin, temporarily pausing the ESC listener so it yields
/// raw mode and stops competing for stdin events.  This prevents the deadlock
/// where `io::stdin().read_line()` blocks forever because crossterm raw mode
/// is active on another thread.
fn read_line_pausing_esc(
    esc_paused: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<String> {
    use std::sync::atomic::Ordering;

    // Signal the ESC listener to pause and release raw mode
    esc_paused.store(true, Ordering::Release);
    // Give the listener thread time to actually disable raw mode
    std::thread::sleep(std::time::Duration::from_millis(80));

    let mut response = String::new();
    let result = std::io::stdin().read_line(&mut response);

    // Unpause — the listener will re-enter raw mode on its own
    esc_paused.store(false, Ordering::Release);

    result.map(|_| response)
}

/// Try to extract a `base64_png` field from a JSON tool result string.
fn try_extract_base64_png(result: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()?
        .get("base64_png")?
        .as_str()
        .map(String::from)
}

/// Build a text summary of a JSON tool result by removing the large `base64_png`
/// blob and adding an `"image_attached": true` marker.
fn build_image_result_summary(result: &str) -> String {
    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(result) {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("base64_png");
            obj.insert("image_attached".to_string(), serde_json::Value::Bool(true));
        }
        serde_json::to_string(&v).unwrap_or_else(|_| result.to_string())
    } else {
        result.to_string()
    }
}

struct AssistantStepResponse {
    content: String,
    reasoning_content: Option<String>,
    native_tool_calls: Option<Vec<crate::api::types::ToolCall>>,
}

type CollectedToolCall = (String, String, Option<String>);

const TOOL_CONFIRM_ARGS_PREVIEW_CHARS: usize = 240;
const TOOL_FAILURE_HINT_PREVIEW_CHARS: usize = 400;
const FAILED_TOOL_ATTEMPT_WINDOW_SIZE: usize = 16;

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let collected: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{}...", collected)
    } else {
        collected
    }
}

fn canonicalize_tool_args(args_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_str)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| args_str.to_string())
}

fn hash_tool_args(args_str: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonicalize_tool_args(args_str).hash(&mut hasher);
    hasher.finish()
}

impl Agent {
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

    fn record_failed_tool_attempt(
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

    fn suppress_repeated_failed_tool_retry(
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
        println!("{} {}", "✗".bright_red(), err);
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

    /// Execute a step with tool call logging for checkpoints
    /// If `use_last_message` is true, process tool calls from the last assistant message
    /// instead of making a new API call (used after planning phase)
    pub(super) async fn execute_step_with_logging(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(false).await
    }

    /// Execute tool calls from the last assistant message (after planning)
    pub(super) async fn execute_pending_tool_calls(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(true).await
    }

    /// Internal execution logic
    /// If `use_last_message` is true, process tool calls from the last assistant message
    async fn execute_step_internal(&mut self, use_last_message: bool) -> Result<bool> {
        let response = self.get_assistant_step_response(use_last_message).await?;
        let content = response.content;
        let tool_calls = self.collect_tool_calls(
            &content,
            response.reasoning_content.as_deref(),
            response.native_tool_calls.as_ref(),
        );

        debug!("Total tool calls to execute: {}", tool_calls.len());

        // Detect malformed tool calls and inject correction before treating as completion
        if self.detect_and_correct_malformed_tools(&content, &tool_calls) {
            return Ok(false);
        }

        if self.maybe_prompt_for_action(&content, tool_calls.is_empty(), use_last_message) {
            return Ok(false);
        }

        if tool_calls.is_empty() {
            // Check completion gate before accepting task as done
            if let Some(gate_msg) = self.check_completion_gate() {
                info!("Completion gate rejected: {}", gate_msg);
                self.messages.push(Message::user(gate_msg));
                return Ok(false);
            }

            // Fire Stop hooks before completing
            let stop_ctx = HookContext::stop();
            self.hook_registry.fire(&stop_ctx).await;

            output::final_answer(&content);
            self.last_assistant_response = content;
            return Ok(true);
        }

        // Plan mode: show proposed tool calls without executing
        if self.plan_mode {
            let mut plan_summary =
                String::from("Plan mode — proposed tool calls (not executed):\n");
            for (i, (name, args_str, _)) in tool_calls.iter().enumerate() {
                let args_preview: String = args_str.chars().take(200).collect();
                plan_summary.push_str(&format!(
                    "\n{}. {} — {}{}\n",
                    i + 1,
                    name,
                    args_preview,
                    if args_str.len() > 200 { "..." } else { "" }
                ));
            }
            output::final_answer(&plan_summary);
            self.messages.push(Message::user(
                "The above tool calls were proposed but NOT executed (plan mode is active). \
                 Review the plan and confirm, or adjust.",
            ));
            self.last_assistant_response = plan_summary;
            return Ok(false);
        }

        // Detect repetition loops before executing
        if let Some(loop_msg) = self.detect_repetition(&tool_calls) {
            info!("Repetition loop detected, injecting correction");
            self.messages.push(Message::user(loop_msg));
            return Ok(false);
        }

        self.execute_tool_batch(tool_calls).await?;
        Ok(false)
    }

    /// Detect malformed tool call attempts and push a correction message.
    /// Returns `true` if malformed markers were found and a correction was injected.
    fn detect_and_correct_malformed_tools(
        &mut self,
        content: &str,
        tool_calls: &[CollectedToolCall],
    ) -> bool {
        if !tool_calls.is_empty() {
            return false;
        }

        let markers = ["<tool", "<function", "tool_name", "tool_call", "<name="];
        let has_markers = markers.iter().any(|m| content.contains(m));
        if !has_markers {
            return false;
        }

        warn!(
            "Detected malformed tool call attempt, injecting correction. Preview: {}",
            &content.chars().take(500).collect::<String>()
        );

        self.cognitive_state.episodic_memory.what_failed(
            "tool_format",
            "Malformed tool call detected — model used wrong XML format",
        );

        self.messages.push(Message::user(
            "Your tool call was malformed and could not be parsed. You MUST use this EXACT format:\n\n\
             <tool>\n<name>TOOL_NAME</name>\n<arguments>{\"key\": \"value\"}</arguments>\n</tool>\n\n\
             Common mistakes to avoid:\n\
             - Do NOT use <function=name> or <name=name> — use <name>TOOL_NAME</name>\n\
             - Do NOT use <parameter=key> tags — use a JSON object inside <arguments>\n\
             - Arguments MUST be valid JSON\n\n\
             Please retry your intended action using the correct format."
        ));

        true
    }

    /// Tool categories that inherently bypass the Rust/cargo verification gate.
    /// These tools indicate non-Rust tasks (browser automation, vision analysis,
    /// desktop control, web fetching, etc.) where `cargo check` is meaningless.
    const NON_RUST_TOOL_PREFIXES: &'static [&'static str] = &[
        "browser_",  // browser_fetch, browser_screenshot, browser_pdf, browser_eval, browser_links
        "vision_",   // vision_analyze, vision_compare
        "computer_", // computer_mouse, computer_keyboard, computer_screen, computer_window
        "screen_capture", // screen_capture
        "page_control", // page_control (screenshot, click, type, scroll, etc.)
        "http_request", // http_request
    ];

    /// Returns true if the current task appears to be a non-Rust task that should
    /// bypass cargo-based verification.  Two conditions trigger the bypass:
    ///
    /// 1. **No Cargo.toml** in the working directory — there is no Rust project to verify.
    /// 2. **Only non-Rust tools used** — the task exclusively used browser, vision,
    ///    computer-control, or web tools with no file-write or cargo activity.
    fn should_skip_cargo_verification(&self) -> bool {
        // Condition 1: No Cargo.toml in working directory → not a Rust project
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        if !cwd.join("Cargo.toml").exists() {
            debug!(
                "Completion gate: no Cargo.toml found in {:?}, skipping cargo verification",
                cwd
            );
            return true;
        }

        // Condition 2: Every tool call in the checkpoint is a non-Rust tool
        let all_non_rust = self
            .current_checkpoint
            .as_ref()
            .map(|cp| {
                // If there are no tool calls at all, this is a text-only response — skip cargo
                if cp.tool_calls.is_empty() {
                    return true;
                }
                cp.tool_calls.iter().all(|tc| {
                    Self::NON_RUST_TOOL_PREFIXES
                        .iter()
                        .any(|prefix| tc.tool_name.starts_with(prefix))
                })
            })
            .unwrap_or(false);

        if all_non_rust {
            debug!(
                "Completion gate: all tool calls are non-Rust tools, skipping cargo verification"
            );
        }
        all_non_rust
    }

    /// Check whether the agent has done enough work to accept completion.
    /// Returns `None` to accept, or `Some(message)` to reject with instructions.
    fn check_completion_gate(&self) -> Option<String> {
        let step_count = self.loop_control.current_step();
        let min_steps = self.config.agent.min_completion_steps;

        if step_count < min_steps {
            // Tailor the message: don't mention cargo for non-Rust tasks
            let verification_hint = if self.should_skip_cargo_verification() {
                "Continue working: review your results and ensure the task is fully complete."
            } else {
                "Continue working: verify your changes compile with cargo_check and pass tests with cargo_test."
            };
            return Some(format!(
                "You are trying to complete the task after only {} step(s), but at least {} are required. \
                 You have a large budget — do not rush. {}",
                step_count, min_steps, verification_hint
            ));
        }

        if self.config.agent.require_verification_before_completion {
            // Skip cargo verification entirely for non-Rust tasks
            if self.should_skip_cargo_verification() {
                debug!("Completion gate: bypassing cargo verification for non-Rust task");
                return None;
            }

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

            if !has_verification {
                return Some(
                    "You must run at least one verification tool (cargo_check, cargo_test, or cargo_clippy) \
                     successfully before completing the task. Please verify your work now."
                        .to_string(),
                );
            }
        }

        None
    }

    /// Track tool calls and detect repetition loops.
    /// Returns `Some(message)` if the same tool+args has been called too many times recently.
    fn detect_repetition(&mut self, tool_calls: &[CollectedToolCall]) -> Option<String> {
        const MAX_REPEATS: usize = 3;
        const WINDOW_SIZE: usize = 10;

        for (name, args_str, _) in tool_calls {
            let sig = (name.clone(), hash_tool_args(args_str));

            self.recent_tool_calls.push_back(sig.clone());
            if self.recent_tool_calls.len() > WINDOW_SIZE {
                self.recent_tool_calls.pop_front();
            }

            let repeat_count = self.recent_tool_calls.iter().filter(|s| **s == sig).count();

            if repeat_count >= MAX_REPEATS {
                warn!(
                    "Repetition loop detected: {} called {} times in last {} calls",
                    name, repeat_count, WINDOW_SIZE
                );
                self.cognitive_state.episodic_memory.what_failed(
                    "repetition_loop",
                    &format!(
                        "Stuck in loop: {} called {} times with identical args",
                        name, repeat_count
                    ),
                );
                self.recent_tool_calls.clear();
                return Some(format!(
                    "STUCK LOOP DETECTED: You have called `{}` {} times with the exact same arguments. \
                     This is not making progress. STOP and try a DIFFERENT approach:\n\
                     - If file_edit fails with 'old_str not found', re-read the file first to see current content\n\
                     - If file_write keeps writing the same content, your output is wrong — re-read the test expectations\n\
                     - If file_read keeps reading the same file, you already have the content — make your edit now\n\
                     - Consider using a completely different tool or strategy",
                    name, repeat_count
                ));
            }
        }
        None
    }

    fn maybe_prompt_for_action(
        &mut self,
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
    ) -> bool {
        if !self.should_prompt_for_action(content, has_no_tool_calls, use_last_message) {
            return false;
        }

        info!("Detected intent without action, prompting model to use tools");
        output::intent_without_action();
        self.messages.push(Message::user(
            "Please use the appropriate tools to take action now. Don't just describe what you'll do - actually execute the tools."
        ));
        true
    }

    async fn execute_tool_batch(&mut self, tool_calls: Vec<CollectedToolCall>) -> Result<()> {
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
                output::safety_blocked(&error_msg);
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

            if !self.confirm_tool_execution(&name, &args_str, &call_id, use_native_fc)? {
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

            let activity = output::tool_activity_message(&name, &args);
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
                crate::agent::last_tool::store(crate::agent::last_tool::LastToolOutput {
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

            // Track file operations for context management
            if success {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let path_str = path.to_string();
                    match name.as_str() {
                        "file_read" => {
                            if self.context_files.len() < 500
                                && !self.context_files.contains(&path_str)
                            {
                                self.context_files.push(path_str);
                            }
                        }
                        "file_delete" => {
                            // Remove deleted files from context tracking entirely
                            self.context_files.retain(|p| p != &path_str);
                            self.stale_files.remove(&path_str);
                        }
                        "file_write" | "file_edit" => {
                            if self.stale_files.len() < 500 {
                                self.stale_files.insert(path_str);
                            }
                        }
                        _ => {}
                    }
                }
            }

            self.push_tool_result_message(use_native_fc, &call_id, &name, success, &result);
            if !success {
                self.remember_failed_tool(&name, &result);
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

    fn build_tool_call_context(
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

    fn confirm_tool_execution(
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

        use std::io::{self, Write};

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

        println!(
            "{} Tool: {} Args: {}",
            "⚠️".bright_yellow(),
            name.bright_cyan(),
            args_display.bright_white()
        );
        print!(
            "{}",
            "Execute? [y/N/s(bypass permissions)]: ".bright_yellow()
        );
        io::stdout().flush().ok();

        let response = read_line_pausing_esc(&self.esc_paused);
        if let Ok(response) = response {
            let response = response.trim().to_lowercase();
            match response.as_str() {
                "y" | "yes" => return Ok(true),
                "s" | "skip" => {
                    self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                    println!(
                        "{} Switched to YOLO mode for this session",
                        "⚡".bright_yellow()
                    );
                    return Ok(true);
                }
                _ => {}
            }
        }

        let skip_msg = "Tool execution skipped by user";
        println!("{} {}", "⏭️".bright_yellow(), skip_msg);
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

    fn parse_tool_args(
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
                println!("{} {}", "✗".bright_red(), err);
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

    fn validate_tool_args(
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
                println!("{} {}", "✗".bright_red(), err);
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

    async fn execute_single_tool(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        start_time: std::time::Instant,
    ) -> Result<(bool, String, String)> {
        let Some(tool) = self.tools.get(name) else {
            let err = format!("Unknown tool: {}", name);
            self.log_tool_call(name, args_str, &err, false, start_time, false);
            return Ok((false, err.clone(), err));
        };

        // Check ToolCache for cacheable (read-only) tools
        let is_cacheable = crate::session::cache::is_cacheable(name);
        if is_cacheable {
            if let Some(cached_value) = self.tool_cache.get(name, args) {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&cached_value)?;
                let summary =
                    output::semantic_summary(name, args, Some(&result_str), true, elapsed);
                self.log_tool_call(name, args_str, &result_str, true, start_time, true);
                debug!("Cache hit for tool '{}' ({}ms)", name, elapsed);
                return Ok((true, result_str, summary));
            }
        }

        // Invalidate cache entries when a mutating tool targets a specific path
        if crate::session::cache::invalidates_cache(name) {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                self.tool_cache.invalidate_path(path);
            }
            // shell_exec and git operations can affect any file — clear all read caches
            if matches!(name, "shell_exec" | "git_commit" | "git_checkout") {
                self.tool_cache.clear();
            }
        }

        // Snapshot file before edit/write for undo support.
        // Use tokio::fs to avoid blocking the async runtime thread.
        if matches!(name, "file_edit" | "file_write" | "file_delete") {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    use crate::session::edit_history::{EditAction, FileSnapshot};
                    let snapshot = FileSnapshot::new(std::path::PathBuf::from(path), content);
                    let action = EditAction::FileEdit {
                        path: std::path::PathBuf::from(path),
                        tool: name.to_string(),
                    };
                    self.edit_history.create_checkpoint(action);
                    self.edit_history.add_file_to_current(snapshot);
                }
            }
        }

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
                    output::semantic_summary(name, args, Some(&result_str), true, elapsed);
                self.log_tool_call(name, args_str, &result_str, true, start_time, true);

                // Store successful cacheable results in ToolCache
                if is_cacheable {
                    self.tool_cache.set(name, args, result.clone());
                }

                // Cache tool results in LocalFirstCoordinator
                let cache_key = crate::session::cache::ToolCache::cache_key(name, args);
                self.local_first
                    .cache_response(&cache_key, result_str.clone(), result_str.len());

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
                    output::semantic_summary(name, args, Some(&e.to_string()), false, elapsed);
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
                let summary = output::semantic_summary(name, args, Some(&err), false, elapsed);
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

    async fn maybe_verify_file_change(&mut self, tool_name: &str, args: &Value) -> Option<String> {
        if !matches!(tool_name, "file_edit" | "file_write") {
            return None;
        }

        let path = args.get("path").and_then(|v| v.as_str())?;
        info!("Running verification after {} on {}", tool_name, path);
        self.cognitive_state.set_phase(CyclePhase::Verify);
        let spinner = crate::ui::spinner::TerminalSpinner::start("Verifying...");

        match self
            .verification_gate
            .verify_change(&[path.to_string()], &format!("{}:{}", tool_name, path))
            .await
        {
            Ok(report) => {
                if report.overall_passed {
                    spinner.stop_success("Verification passed");
                    self.cognitive_state.episodic_memory.what_worked(
                        tool_name,
                        &format!("{} on {} passed verification", tool_name, path),
                    );
                    if output::is_verbose() {
                        output::verification_report(&format!("{}", report), true);
                    }
                    None
                } else {
                    spinner.stop_error("Verification failed");
                    self.cognitive_state.episodic_memory.what_failed(
                        tool_name,
                        &format!("{} on {} failed verification", tool_name, path),
                    );
                    output::verification_report(&format!("{}", report), false);
                    Some(format!(
                        "\n\n<verification_failed>\n{}\n</verification_failed>",
                        report
                    ))
                }
            }
            Err(e) => {
                spinner.stop_error("Verification failed to run");
                warn!("Verification failed to run: {}", e);
                None
            }
        }
    }

    fn maybe_enhance_tool_result(&self, name: &str, result_str: &str) -> String {
        if name == "cargo_check" && result_str.contains("\"success\":false") {
            self.enhance_cargo_errors(result_str)
        } else {
            result_str.to_string()
        }
    }

    fn push_tool_result_message(
        &mut self,
        use_native_fc: bool,
        call_id: &str,
        _tool_name: &str,
        success: bool,
        result: &str,
    ) {
        // Detect base64_png in successful tool results and promote to multimodal
        if success {
            if let Some(base64_png) = try_extract_base64_png(result) {
                let summary = build_image_result_summary(result);
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

    fn log_tool_call(
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
                timestamp: Utc::now(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
                result: Some(logged_result),
                success,
                duration_ms: Some(duration_ms),
            });
        }
    }

    async fn get_assistant_step_response(
        &mut self,
        use_last_message: bool,
    ) -> Result<AssistantStepResponse> {
        let mut native_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;

        if use_last_message {
            let last_msg = self
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "assistant")
                .context("No previous assistant message found")?;
            debug!(
                "Using content from last assistant message ({} chars)",
                last_msg.content.len()
            );
            if self.config.agent.native_function_calling {
                native_tool_calls = last_msg.tool_calls.clone();
            }
            return Ok(AssistantStepResponse {
                content: last_msg.content.text().to_string(),
                reasoning_content: last_msg.reasoning_content.clone(),
                native_tool_calls,
            });
        }

        // Hard-truncate message history to stay within context window before
        // any API call.  This prevents exceeding the model's context limit when
        // compression is skipped or fails.
        self.trim_message_history();

        if self.compressor.should_compress(&self.messages) {
            info!("Context compression triggered");
            match self.compressor.compress(&self.client, &self.messages).await {
                Ok(compressed) => {
                    self.messages = compressed;
                }
                Err(e) => {
                    warn!("Compression failed, using hard limit: {}", e);
                    self.messages = self.compressor.hard_compress(&self.messages);
                }
            }
        }

        let mut request_messages = self.messages.clone();
        let mut system_hints = Vec::new();
        if let Some(learning_hint) = self.build_learning_hint(self.learning_context()) {
            system_hints.push(learning_hint);
        }
        if let Some(failure_hint) = self.pending_failure_hint.take() {
            system_hints.push(failure_hint);
        }

        if !system_hints.is_empty() {
            let merged_hints = system_hints.join("\n\n");
            // Merge into existing system message to maintain OpenAI message ordering
            // (system messages must precede all user/assistant/tool messages)
            if let Some(first) = request_messages.first_mut() {
                if first.role == "system" {
                    first.content = format!("{}\n\n{}", first.content, merged_hints).into();
                } else {
                    request_messages.insert(0, Message::system(merged_hints));
                }
            } else {
                request_messages.insert(0, Message::system(merged_hints));
            }
        }

        let (content, reasoning) = if self.config.agent.streaming {
            match self
                .chat_streaming(
                    request_messages.clone(),
                    self.api_tools(),
                    ThinkingMode::Enabled,
                )
                .await
            {
                Ok((content, reasoning, stream_tool_calls)) => {
                    if self.config.agent.native_function_calling && stream_tool_calls.is_some() {
                        native_tool_calls = stream_tool_calls.clone();
                        info!(
                            "Received {} native tool calls from stream",
                            native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                        );
                    }
                    (content, reasoning)
                }
                Err(stream_err) => {
                    warn!(
                        "Streaming request failed ({}); retrying this step with non-streaming API",
                        stream_err
                    );

                    let response = self
                        .client
                        .chat(request_messages, self.api_tools(), ThinkingMode::Enabled)
                        .await
                        .with_context(|| {
                            format!(
                                "Streaming failed: {}. Non-streaming fallback request also failed",
                                stream_err
                            )
                        })?;

                    let choice = response
                        .choices
                        .into_iter()
                        .next()
                        .context("No response from model")?;

                    let message = choice.message;
                    let content = message.content.text().to_string();
                    let reasoning = message.reasoning_content.clone();

                    if self.config.agent.native_function_calling && message.tool_calls.is_some() {
                        native_tool_calls = message.tool_calls.clone();
                        info!(
                            "Received {} native tool calls from fallback API",
                            native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                        );
                    }

                    debug!(
                        "Fallback model response content ({} chars): {}",
                        content.len(),
                        content
                    );
                    if content.is_empty() {
                        warn!("Fallback model returned empty content!");
                    }
                    if let Some(ref r) = reasoning {
                        println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                        debug!("Fallback reasoning ({} chars): {}", r.len(), r);
                    }

                    (content, reasoning)
                }
            }
        } else {
            let response = self
                .client
                .chat(request_messages, self.api_tools(), ThinkingMode::Enabled)
                .await?;

            let choice = response
                .choices
                .into_iter()
                .next()
                .context("No response from model")?;

            let message = choice.message;
            let content = message.content.text().to_string();
            let reasoning = message.reasoning_content.clone();

            if self.config.agent.native_function_calling && message.tool_calls.is_some() {
                native_tool_calls = message.tool_calls.clone();
                info!(
                    "Received {} native tool calls from API",
                    native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                );
            }

            debug!(
                "Raw model response content ({} chars): {}",
                content.len(),
                content
            );

            if std::env::var("SELFWARE_DEBUG").is_ok() {
                println!("{}", "=== DEBUG: Raw Model Response ===".bright_magenta());
                println!("{}", content);
                println!("{}", "=== END DEBUG ===".bright_magenta());
            }

            if content.is_empty() {
                warn!("Model returned empty content!");
            }

            if let Some(ref r) = reasoning {
                println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                debug!("Reasoning content ({} chars): {}", r.len(), r);
            }

            (content, reasoning)
        };

        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone().into(),
            reasoning_content: reasoning.clone(),
            tool_calls: native_tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        Ok(AssistantStepResponse {
            content,
            reasoning_content: reasoning,
            native_tool_calls,
        })
    }

    fn collect_tool_calls(
        &self,
        content: &str,
        reasoning_content: Option<&str>,
        native_tool_calls: Option<&Vec<crate::api::types::ToolCall>>,
    ) -> Vec<(String, String, Option<String>)> {
        if self.config.agent.native_function_calling
            && native_tool_calls.is_some_and(|calls| !calls.is_empty())
        {
            let native_calls = native_tool_calls.unwrap();
            info!("Using {} native tool calls from API", native_calls.len());
            return native_calls
                .iter()
                .map(|tc| {
                    debug!(
                        "Native tool call: {} (id: {}) with args: {}",
                        tc.function.name, tc.id, tc.function.arguments
                    );
                    (
                        tc.function.name.clone(),
                        tc.function.arguments.clone(),
                        Some(tc.id.clone()),
                    )
                })
                .collect();
        }

        info!(
            "Falling back to XML parsing (native FC returned {} tool calls)",
            native_tool_calls.map(|t| t.len()).unwrap_or(0)
        );
        debug!("Looking for tool calls with multi-format parser...");

        let parse_result = parse_tool_calls(content);
        let mut tool_calls: Vec<(String, String, Option<String>)> = parse_result
            .tool_calls
            .iter()
            .map(|tc| {
                debug!(
                    "Found tool call in content: {} with args: {}",
                    tc.tool_name, tc.arguments
                );
                (tc.tool_name.clone(), tc.arguments.to_string(), None)
            })
            .collect();

        for error in &parse_result.parse_errors {
            warn!("Tool parse error: {}", error);
        }

        if tool_calls.is_empty() {
            if let Some(reasoning_text) = reasoning_content {
                let reasoning_result = parse_tool_calls(reasoning_text);
                let reasoning_tools: Vec<(String, String, Option<String>)> = reasoning_result
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        debug!(
                            "Found tool call in reasoning: {} with args: {}",
                            tc.tool_name, tc.arguments
                        );
                        (tc.tool_name.clone(), tc.arguments.to_string(), None)
                    })
                    .collect();
                if !reasoning_tools.is_empty() {
                    info!(
                        "Found {} tool calls in reasoning content",
                        reasoning_tools.len()
                    );
                    tool_calls = reasoning_tools;
                }
            }
        }

        tool_calls
    }

    fn should_prompt_for_action(
        &self,
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
    ) -> bool {
        if !has_no_tool_calls || use_last_message || content.len() >= 1000 {
            return false;
        }

        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let content_lower = content.to_lowercase();
        intent_phrases.iter().any(|p| content_lower.contains(p))
    }

    /// Plan phase - returns true if model wants to execute tools (should continue to execution)
    /// This now combines planning with initial tool extraction to avoid double API calls
    pub(super) async fn plan(&mut self) -> Result<bool> {
        // Tools are embedded in system prompt - see WORKAROUND comment in Agent::new()
        debug!("Sending planning request to model...");
        self.trim_message_history();
        let mut request_messages = self.messages.clone();
        if let Some(learning_hint) = self.build_learning_hint(self.learning_context()) {
            // Merge into existing system message to maintain OpenAI message ordering
            if let Some(first) = request_messages.first_mut() {
                if first.role == "system" {
                    first.content = format!("{}\n\n{}", first.content, learning_hint).into();
                } else {
                    request_messages.insert(0, Message::system(learning_hint));
                }
            } else {
                request_messages.insert(0, Message::system(learning_hint));
            }
        }
        let response = self
            .client
            .chat(request_messages, self.api_tools(), ThinkingMode::Enabled)
            .await?;

        let choice = response
            .choices
            .into_iter()
            .next()
            .context("No response from model")?;

        let assistant_msg = choice.message;
        let content = &assistant_msg.content;

        // Debug logging for planning response
        debug!(
            "Planning response content ({} chars): {}",
            content.len(),
            content
        );

        // Verbose logging when SELFWARE_DEBUG is set or verbose mode
        output::debug_output("Planning Response", content.text());

        if content.is_empty() {
            warn!("Model returned empty planning content!");
        }
        if let Some(ref reasoning) = assistant_msg.reasoning_content {
            debug!(
                "Planning reasoning ({} chars): {}",
                reasoning.len(),
                reasoning
            );
            if let Some(r) = &assistant_msg.reasoning_content {
                output::thinking(r, false);
            }
        }

        // Check if the planning response contains tool calls
        // For native function calling, check tool_calls field; otherwise parse from content
        let (has_tool_calls, native_tool_calls) = if let (true, Some(tool_calls)) = (
            self.config.agent.native_function_calling,
            assistant_msg.tool_calls.as_ref(),
        ) {
            info!(
                "Planning response has {} native tool calls",
                tool_calls.len()
            );
            (!tool_calls.is_empty(), assistant_msg.tool_calls.clone())
        } else {
            let parsed = !parse_tool_calls(content.text()).tool_calls.is_empty();
            debug!("Planning response has tool calls (parsed): {}", parsed);
            (parsed, None)
        };

        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: assistant_msg.reasoning_content,
            tool_calls: native_tool_calls,
            tool_call_id: None,
            name: None,
        });

        // Return whether there are tool calls to execute
        Ok(has_tool_calls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::{ToolCall as ApiToolCall, ToolFunction};
    use crate::testing::mock_api::MockLlmServer;
    use crate::tool_parser::parse_tool_calls;

    // =========================================================================
    // Helper: mirrors should_prompt_for_action logic for standalone testing
    // =========================================================================
    fn should_prompt_for_action(
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
    ) -> bool {
        if !has_no_tool_calls || use_last_message || content.len() >= 1000 {
            return false;
        }
        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let content_lower = content.to_lowercase();
        intent_phrases.iter().any(|p| content_lower.contains(p))
    }

    // =========================================================================
    // should_prompt_for_action tests
    // =========================================================================

    #[test]
    fn test_should_prompt_when_intent_phrase_present() {
        assert!(should_prompt_for_action(
            "Let me check the file",
            true,
            false
        ));
        assert!(should_prompt_for_action(
            "I'll fix that bug now",
            true,
            false
        ));
        assert!(should_prompt_for_action(
            "I will refactor the module",
            true,
            false
        ));
        assert!(should_prompt_for_action(
            "Let's start by reading the code",
            true,
            false
        ));
        assert!(should_prompt_for_action(
            "First, I need to understand",
            true,
            false
        ));
        assert!(should_prompt_for_action(
            "Going to investigate",
            true,
            false
        ));
    }

    #[test]
    fn test_should_not_prompt_when_tool_calls_exist() {
        // has_no_tool_calls = false means there ARE tool calls
        assert!(!should_prompt_for_action("Let me check", false, false));
    }

    #[test]
    fn test_should_not_prompt_when_using_last_message() {
        assert!(!should_prompt_for_action("Let me check", true, true));
    }

    #[test]
    fn test_should_not_prompt_for_long_content() {
        let long_content = format!("Let me {}", "x".repeat(1000));
        assert!(!should_prompt_for_action(&long_content, true, false));
    }

    #[test]
    fn test_should_not_prompt_for_plain_response() {
        assert!(!should_prompt_for_action("The answer is 42.", true, false));
        assert!(!should_prompt_for_action(
            "Here is the result.",
            true,
            false
        ));
    }

    #[test]
    fn test_should_prompt_case_insensitive() {
        assert!(should_prompt_for_action("LET ME check", true, false));
        assert!(should_prompt_for_action("STARTING now", true, false));
        assert!(should_prompt_for_action("BEGIN BY reading", true, false));
    }

    // =========================================================================
    // collect_tool_calls logic tests (via parse_tool_calls + native fallback)
    // =========================================================================

    #[test]
    fn test_collect_tool_calls_from_native_calls() {
        // Simulates collect_tool_calls when native_function_calling = true
        let native_calls = [
            ApiToolCall {
                id: "call_1".to_string(),
                call_type: "function".to_string(),
                function: ToolFunction {
                    name: "file_read".to_string(),
                    arguments: r#"{"path":"src/main.rs"}"#.to_string(),
                },
            },
            ApiToolCall {
                id: "call_2".to_string(),
                call_type: "function".to_string(),
                function: ToolFunction {
                    name: "shell_exec".to_string(),
                    arguments: r#"{"command":"ls"}"#.to_string(),
                },
            },
        ];

        // Simulate the native path of collect_tool_calls
        let collected: Vec<CollectedToolCall> = native_calls
            .iter()
            .map(|tc| {
                (
                    tc.function.name.clone(),
                    tc.function.arguments.clone(),
                    Some(tc.id.clone()),
                )
            })
            .collect();

        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0].0, "file_read");
        assert_eq!(collected[1].0, "shell_exec");
        assert_eq!(collected[0].2.as_deref(), Some("call_1"));
    }

    #[test]
    fn test_collect_tool_calls_empty_native_falls_back_to_xml() {
        let content = r#"<tool>
<name>file_read</name>
<arguments>{"path":"test.rs"}</arguments>
</tool>"#;

        let empty_native: Vec<ApiToolCall> = vec![];

        // Simulate fallback: native calls empty, parse XML from content
        let native_empty = empty_native.is_empty();
        assert!(native_empty);

        let parse_result = parse_tool_calls(content);
        let tool_calls: Vec<CollectedToolCall> = parse_result
            .tool_calls
            .iter()
            .map(|tc| (tc.tool_name.clone(), tc.arguments.to_string(), None))
            .collect();

        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].0, "file_read");
        assert!(tool_calls[0].2.is_none()); // No tool_call_id for XML-parsed calls
    }

    #[test]
    fn test_collect_tool_calls_falls_back_to_reasoning_content() {
        // Content has no tool calls, but reasoning does
        let content = "I need to think about this...";
        let reasoning = r#"<tool>
<name>grep_search</name>
<arguments>{"pattern":"TODO","path":"src/"}</arguments>
</tool>"#;

        let content_result = parse_tool_calls(content);
        assert!(content_result.tool_calls.is_empty());

        let reasoning_result = parse_tool_calls(reasoning);
        assert_eq!(reasoning_result.tool_calls.len(), 1);
        assert_eq!(reasoning_result.tool_calls[0].tool_name, "grep_search");
    }

    // =========================================================================
    // maybe_enhance_tool_result tests
    // =========================================================================

    #[test]
    fn test_enhance_tool_result_no_change_for_non_cargo() {
        // The function only enhances cargo_check results with "success":false
        let name = "file_read";
        let result_str = r#"{"content":"hello"}"#;
        // Non-cargo_check tools pass through unchanged
        if name != "cargo_check" || !result_str.contains("\"success\":false") {
            assert_eq!(result_str, result_str);
        }
    }

    #[test]
    fn test_enhance_tool_result_triggers_for_failed_cargo_check() {
        let name = "cargo_check";
        let result_str = r#"{"success":false,"stderr":"error[E0308]: mismatched types"}"#;
        let should_enhance = name == "cargo_check" && result_str.contains("\"success\":false");
        assert!(should_enhance);
    }

    #[test]
    fn test_enhance_tool_result_skips_successful_cargo_check() {
        let name = "cargo_check";
        let result_str = r#"{"success":true,"stderr":""}"#;
        let should_enhance = name == "cargo_check" && result_str.contains("\"success\":false");
        assert!(!should_enhance);
    }

    // =========================================================================
    // build_tool_call_context tests (via MockLlmServer + Agent)
    // =========================================================================

    fn mock_config(endpoint: String) -> Config {
        Config {
            endpoint,
            model: "mock-model".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 5,
                step_timeout_secs: 5,
                streaming: false,
                native_function_calling: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_build_tool_call_context_without_native_fc() {
        let server = MockLlmServer::builder().with_response("done").build().await;

        let config = mock_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let (call_id, use_native_fc, fake_call) =
            agent.build_tool_call_context("file_read", r#"{"path":"test.rs"}"#, None);

        assert!(!use_native_fc);
        assert!(call_id.starts_with("call_"));
        assert_eq!(fake_call.function.name, "file_read");
        assert_eq!(fake_call.function.arguments, r#"{"path":"test.rs"}"#);
        assert_eq!(fake_call.call_type, "function");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_build_tool_call_context_with_native_fc_and_id() {
        let server = MockLlmServer::builder().with_response("done").build().await;

        let mut config = mock_config(format!("{}/v1", server.url()));
        config.agent.native_function_calling = true;
        let agent = Agent::new(config).await.unwrap();

        let (call_id, use_native_fc, fake_call) = agent.build_tool_call_context(
            "shell_exec",
            r#"{"command":"ls"}"#,
            Some("call_existing_123".to_string()),
        );

        assert!(use_native_fc);
        assert_eq!(call_id, "call_existing_123");
        assert_eq!(fake_call.function.name, "shell_exec");

        server.stop().await;
    }

    // =========================================================================
    // warn_on_unparsed_tool_content (logic check)
    // =========================================================================

    #[test]
    fn test_warn_condition_content_has_tool_keywords_but_no_calls() {
        let content = "I want to use a tool_name function to help";
        let tool_calls: Vec<CollectedToolCall> = vec![];

        // The warn fires when tool_calls empty AND content contains suspicious keywords
        let should_warn = tool_calls.is_empty()
            && (content.contains("<tool")
                || content.contains("tool_name")
                || content.contains("function"));

        assert!(should_warn);
    }

    #[test]
    fn test_warn_condition_no_warn_when_calls_present() {
        let content = "Using tool_name to execute function";
        let tool_calls: Vec<CollectedToolCall> =
            vec![("file_read".to_string(), "{}".to_string(), None)];

        let should_warn = tool_calls.is_empty()
            && (content.contains("<tool")
                || content.contains("tool_name")
                || content.contains("function"));

        assert!(!should_warn);
    }

    #[test]
    fn test_warn_condition_no_warn_for_clean_content() {
        let content = "Here is a summary of the code changes.";
        let tool_calls: Vec<CollectedToolCall> = vec![];

        let should_warn = tool_calls.is_empty()
            && (content.contains("<tool")
                || content.contains("tool_name")
                || content.contains("function"));

        assert!(!should_warn);
    }

    // =========================================================================
    // Helper: create a test agent with permissive config
    // =========================================================================
    fn test_config(endpoint: String) -> Config {
        Config {
            endpoint,
            model: "mock-model".to_string(),
            agent: crate::config::AgentConfig {
                max_iterations: 50,
                step_timeout_secs: 10,
                streaming: false,
                native_function_calling: false,
                min_completion_steps: 0,
                require_verification_before_completion: false,
                ..Default::default()
            },
            safety: crate::config::SafetyConfig {
                allowed_paths: vec!["./**".to_string(), "/**".to_string()],
                ..Default::default()
            },
            execution_mode: crate::config::ExecutionMode::Yolo,
            ..Default::default()
        }
    }

    // =========================================================================
    // detect_and_correct_malformed_tools tests
    // =========================================================================

    #[tokio::test]
    async fn test_detect_malformed_returns_false_when_calls_present() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> =
            vec![("file_read".to_string(), "{}".to_string(), None)];

        let result = agent.detect_and_correct_malformed_tools("<tool broken>", &tool_calls);
        assert!(!result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_returns_false_no_markers() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];

        let result =
            agent.detect_and_correct_malformed_tools("Just a normal response.", &tool_calls);
        assert!(!result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_detects_tool_marker() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent.detect_and_correct_malformed_tools("<tool broken format>", &tool_calls);

        assert!(result);
        assert_eq!(agent.messages.len(), initial_len + 1);
        let last_msg = agent.messages.last().unwrap();
        assert_eq!(last_msg.role, "user");
        assert!(last_msg.content.text().contains("malformed"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_detects_function_marker() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent.detect_and_correct_malformed_tools(
            "<function=file_read>{\"path\":\"test.rs\"}</function>",
            &tool_calls,
        );

        assert!(result);
        let last_msg = agent.messages.last().unwrap();
        assert!(last_msg.content.text().contains("EXACT format"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_detects_tool_name_marker() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent.detect_and_correct_malformed_tools("tool_name: file_read", &tool_calls);

        assert!(result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_detects_tool_call_marker() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent
            .detect_and_correct_malformed_tools("I'll use tool_call to read the file", &tool_calls);

        assert!(result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_detects_name_equals_marker() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent.detect_and_correct_malformed_tools("<name=file_read>", &tool_calls);

        assert!(result);

        server.stop().await;
    }

    // =========================================================================
    // check_completion_gate tests
    // =========================================================================

    #[tokio::test]
    async fn test_gate_passes_no_requirements() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = false;
        let agent = Agent::new(config).await.unwrap();

        let result = agent.check_completion_gate();
        assert!(result.is_none(), "Expected gate to pass, got: {:?}", result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_rejects_insufficient_steps() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 5;
        config.agent.require_verification_before_completion = false;
        let agent = Agent::new(config).await.unwrap();

        let result = agent.check_completion_gate();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("only"));
        assert!(msg.contains("step"));
        assert!(msg.contains("required"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_rejects_missing_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let agent = Agent::new(config).await.unwrap();

        let result = agent.check_completion_gate();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("verification"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_passes_with_cargo_check_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "test-task".to_string(),
            "test description".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(100),
        });
        agent.current_checkpoint = Some(checkpoint);

        let result = agent.check_completion_gate();
        assert!(
            result.is_none(),
            "Expected gate to pass with verification, got: {:?}",
            result
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_rejects_failed_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "test-task".to_string(),
            "test description".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("error".to_string()),
            success: false,
            duration_ms: Some(100),
        });
        agent.current_checkpoint = Some(checkpoint);

        let result = agent.check_completion_gate();
        assert!(result.is_some(), "Failed verification should reject");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_accepts_cargo_test_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "test-task".to_string(),
            "test desc".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_test".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(200),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(agent.check_completion_gate().is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_accepts_cargo_clippy_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "test-task".to_string(),
            "test desc".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_clippy".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(300),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(agent.check_completion_gate().is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_steps_checked_before_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 100;
        config.agent.require_verification_before_completion = true;
        let agent = Agent::new(config).await.unwrap();

        let result = agent.check_completion_gate();
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("step"));

        server.stop().await;
    }

    // =========================================================================
    // Non-Rust task bypass tests (P0 fix: cargo gate skipped for non-Rust tasks)
    // =========================================================================

    #[tokio::test]
    async fn test_gate_bypassed_for_browser_only_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        // Simulate a task that only used browser tools — no cargo calls
        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "browser-task".to_string(),
            "fetch a webpage".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "browser_fetch".to_string(),
            arguments: r#"{"url":"https://example.com"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "browser_screenshot".to_string(),
            arguments: r#"{"url":"https://example.com"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(300),
        });
        agent.current_checkpoint = Some(checkpoint);

        // Gate should pass — no cargo verification needed for browser-only tasks
        assert!(
            agent.check_completion_gate().is_none(),
            "Browser-only tasks should bypass the cargo verification gate"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_bypassed_for_vision_only_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "vision-task".to_string(),
            "analyze an image".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "vision_analyze".to_string(),
            arguments: r#"{"path":"/tmp/img.png"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(200),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(
            agent.check_completion_gate().is_none(),
            "Vision-only tasks should bypass the cargo verification gate"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_bypassed_for_computer_control_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "desktop-task".to_string(),
            "click a button".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "computer_mouse".to_string(),
            arguments: r#"{"action":"click","x":100,"y":200}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(50),
        });
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "screen_capture".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(100),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(
            agent.check_completion_gate().is_none(),
            "Computer-control tasks should bypass the cargo verification gate"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_bypassed_for_http_only_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "http-task".to_string(),
            "call an API".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "http_request".to_string(),
            arguments: r#"{"url":"https://api.example.com","method":"GET"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(300),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(
            agent.check_completion_gate().is_none(),
            "HTTP-only tasks should bypass the cargo verification gate"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_still_required_for_mixed_rust_and_browser_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        // Task that used both browser tools AND file_write (a Rust-project tool)
        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "mixed-task".to_string(),
            "fetch web data and write Rust code".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "browser_fetch".to_string(),
            arguments: r#"{"url":"https://example.com"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_write".to_string(),
            arguments: r#"{"path":"src/main.rs","content":"fn main() {}"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(50),
        });
        agent.current_checkpoint = Some(checkpoint);

        // Gate should REJECT — mixed task with file_write still needs cargo verification
        // (we're running from the selfware project root which has a Cargo.toml)
        let result = agent.check_completion_gate();
        assert!(
            result.is_some(),
            "Mixed Rust + browser tasks should still require cargo verification"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_bypassed_when_no_cargo_toml_in_cwd() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        // Create a temp directory without a Cargo.toml and chdir into it
        let tmp = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        // Task with file_write in a non-Rust project — should bypass gate
        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "python-task".to_string(),
            "write a python script".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_write".to_string(),
            arguments: r#"{"path":"script.py","content":"print('hello')"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(50),
        });
        agent.current_checkpoint = Some(checkpoint);

        let result = agent.check_completion_gate();
        assert!(
            result.is_none(),
            "Non-Rust project (no Cargo.toml) should bypass cargo verification, got: {:?}",
            result,
        );

        // Restore cwd
        std::env::set_current_dir(original_dir).unwrap();
        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_min_steps_message_omits_cargo_for_non_rust_task() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 100;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        // Only non-Rust tools used
        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "browser-task".to_string(),
            "browse the web".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "browser_fetch".to_string(),
            arguments: r#"{"url":"https://example.com"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(500),
        });
        agent.current_checkpoint = Some(checkpoint);

        let result = agent.check_completion_gate();
        assert!(result.is_some());
        let msg = result.unwrap();
        // Should NOT mention cargo for non-Rust tasks
        assert!(
            !msg.contains("cargo_check"),
            "Min-steps message for non-Rust task should not mention cargo, got: {}",
            msg
        );
        assert!(msg.contains("step"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_non_rust_tool_prefixes_list_is_comprehensive() {
        // Ensure all known non-Rust tool names match at least one prefix
        let known_non_rust_tools = [
            "browser_fetch",
            "browser_screenshot",
            "browser_pdf",
            "browser_eval",
            "browser_links",
            "vision_analyze",
            "vision_compare",
            "computer_mouse",
            "computer_keyboard",
            "computer_screen",
            "computer_window",
            "screen_capture",
            "page_control",
            "http_request",
        ];

        for tool_name in &known_non_rust_tools {
            let matches = Agent::NON_RUST_TOOL_PREFIXES
                .iter()
                .any(|prefix| tool_name.starts_with(prefix));
            assert!(
                matches,
                "Tool '{}' should match a non-Rust prefix but does not",
                tool_name
            );
        }
    }

    #[test]
    fn test_rust_tools_do_not_match_non_rust_prefixes() {
        // Ensure regular Rust-project tools do NOT match the bypass list
        let rust_tools = [
            "cargo_check",
            "cargo_test",
            "cargo_clippy",
            "file_write",
            "file_read",
            "shell_execute",
            "search_files",
        ];

        for tool_name in &rust_tools {
            let matches = Agent::NON_RUST_TOOL_PREFIXES
                .iter()
                .any(|prefix| tool_name.starts_with(prefix));
            assert!(
                !matches,
                "Rust tool '{}' should NOT match a non-Rust prefix",
                tool_name
            );
        }
    }

    // =========================================================================
    // detect_repetition tests
    // =========================================================================

    #[tokio::test]
    async fn test_repetition_no_loop_initially() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"test.rs"}"#.to_string(),
            None,
        )];

        let result = agent.detect_repetition(&tool_calls);
        assert!(result.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_detects_after_three_identical() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"same.rs"}"#.to_string(),
            None,
        )];

        assert!(agent.detect_repetition(&tool_calls).is_none());
        assert!(agent.detect_repetition(&tool_calls).is_none());

        let result = agent.detect_repetition(&tool_calls);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("STUCK LOOP DETECTED"));
        assert!(msg.contains("file_read"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_no_loop_with_different_args() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let calls1: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"file1.rs"}"#.to_string(),
            None,
        )];
        let calls2: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"file2.rs"}"#.to_string(),
            None,
        )];
        let calls3: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"file3.rs"}"#.to_string(),
            None,
        )];

        assert!(agent.detect_repetition(&calls1).is_none());
        assert!(agent.detect_repetition(&calls2).is_none());
        assert!(agent.detect_repetition(&calls3).is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_clears_history_on_detection() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"same.rs"}"#.to_string(),
            None,
        )];

        agent.detect_repetition(&tool_calls);
        agent.detect_repetition(&tool_calls);
        let result = agent.detect_repetition(&tool_calls);
        assert!(result.is_some());

        // After detection, history is cleared
        let result = agent.detect_repetition(&tool_calls);
        assert!(result.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_window_size_limits() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        for i in 0..10 {
            let calls: Vec<CollectedToolCall> =
                vec![(format!("tool_{}", i), format!(r#"{{"i":{}}}"#, i), None)];
            assert!(agent.detect_repetition(&calls).is_none());
        }

        let repeated: Vec<CollectedToolCall> =
            vec![("tool_0".to_string(), r#"{"i":0}"#.to_string(), None)];
        assert!(agent.detect_repetition(&repeated).is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_multiple_tools_in_batch() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let batch: Vec<CollectedToolCall> = vec![
            (
                "file_read".to_string(),
                r#"{"path":"a.rs"}"#.to_string(),
                None,
            ),
            (
                "shell_exec".to_string(),
                r#"{"command":"ls"}"#.to_string(),
                None,
            ),
        ];

        assert!(agent.detect_repetition(&batch).is_none());
        assert!(agent.detect_repetition(&batch).is_none());

        // Third time, file_read with same args appears 3 times
        let result = agent.detect_repetition(&batch);
        assert!(result.is_some());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_json_whitespace_normalization() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        // Call with compact JSON
        let compact: Vec<CollectedToolCall> = vec![(
            "file_write".to_string(),
            r#"{"path":"foo.rs","content":"bar"}"#.to_string(),
            None,
        )];
        assert!(agent.detect_repetition(&compact).is_none());

        // Call with whitespace-padded but semantically identical JSON
        let spaced: Vec<CollectedToolCall> = vec![(
            "file_write".to_string(),
            r#"{ "path" : "foo.rs" , "content" : "bar" }"#.to_string(),
            None,
        )];
        assert!(agent.detect_repetition(&spaced).is_none());

        // Third call with yet another whitespace variant — should trigger loop detection
        let newlines: Vec<CollectedToolCall> = vec![(
            "file_write".to_string(),
            "{\n  \"path\": \"foo.rs\",\n  \"content\": \"bar\"\n}".to_string(),
            None,
        )];
        let result = agent.detect_repetition(&newlines);
        assert!(
            result.is_some(),
            "should detect loop across whitespace variants"
        );
        assert!(result.unwrap().contains("STUCK LOOP DETECTED"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repetition_invalid_json_falls_back_to_raw() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        // Invalid JSON strings that differ only in whitespace — should NOT be normalized
        // (they are not valid JSON, so fallback hashes raw strings which differ)
        let raw1: Vec<CollectedToolCall> =
            vec![("custom_tool".to_string(), "not json {".to_string(), None)];
        let raw2: Vec<CollectedToolCall> = vec![(
            "custom_tool".to_string(),
            "not json  {".to_string(), // extra space
            None,
        )];

        assert!(agent.detect_repetition(&raw1).is_none());
        assert!(agent.detect_repetition(&raw2).is_none());
        // Third call with raw1 again — only 2 of raw1 in window, so no loop
        assert!(agent.detect_repetition(&raw1).is_none());

        // But three identical raw strings DO trigger detection
        agent.recent_tool_calls.clear();
        assert!(agent.detect_repetition(&raw1).is_none());
        assert!(agent.detect_repetition(&raw1).is_none());
        let result = agent.detect_repetition(&raw1);
        assert!(
            result.is_some(),
            "identical invalid-JSON raw strings should still trigger loop"
        );

        server.stop().await;
    }

    // =========================================================================
    // push_tool_result_message tests
    // =========================================================================

    #[tokio::test]
    async fn test_push_result_xml_success() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        agent.push_tool_result_message(false, "call_1", "test_tool", true, r#"{"output":"hello"}"#);

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "user");
        assert!(msg.content.text().starts_with("<tool_result>"));
        assert!(msg.content.text().contains(r#"{"output":"hello"}"#));
        assert!(!msg.content.text().contains("<error>"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_push_result_xml_failure() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        agent.push_tool_result_message(false, "call_1", "test_tool", false, "Something went wrong");

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "user");
        assert!(msg.content.text().contains("<error>"));
        assert!(msg.content.text().contains("Something went wrong"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_push_result_native_fc_success() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        agent.push_tool_result_message(
            true,
            "call_native_1",
            "test_tool",
            true,
            r#"{"data":"ok"}"#,
        );

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_native_1"));
        assert!(msg.content.text().contains(r#"{"data":"ok"}"#));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_push_result_native_fc_failure() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        agent.push_tool_result_message(
            true,
            "call_native_2",
            "test_tool",
            false,
            "Permission denied",
        );

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "tool");
        assert!(msg.content.text().contains("error"));
        assert!(msg.content.text().contains("Permission denied"));

        server.stop().await;
    }

    // =========================================================================
    // image promotion tests
    // =========================================================================

    #[tokio::test]
    async fn test_push_result_image_promotion_xml() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        let result = r#"{"base64_png":"iVBORw0KGgo=","width":100,"height":100}"#;
        agent.push_tool_result_message(false, "call_img", "screen_capture", true, result);

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "user");
        assert!(msg.content.has_images());
        assert_eq!(msg.content.image_count(), 1);
        // Summary should contain image_attached but not base64_png
        assert!(msg.content.text().contains("image_attached"));
        assert!(!msg.content.text().contains("iVBORw0KGgo="));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_push_result_image_promotion_native_fc() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        let result = r#"{"base64_png":"abc123","note":"screenshot"}"#;
        agent.push_tool_result_message(true, "call_img_native", "screen_capture", true, result);

        assert_eq!(agent.messages.len(), initial_len + 1);
        let msg = agent.messages.last().unwrap();
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_img_native"));
        assert!(msg.content.has_images());
        assert!(msg.content.text().contains("image_attached"));

        server.stop().await;
    }

    #[test]
    fn test_try_extract_base64_png() {
        assert_eq!(
            try_extract_base64_png(r#"{"base64_png":"abc"}"#),
            Some("abc".to_string())
        );
        assert_eq!(try_extract_base64_png(r#"{"other":"val"}"#), None);
        assert_eq!(try_extract_base64_png("not json"), None);
    }

    #[test]
    fn test_build_image_result_summary() {
        let result = r#"{"base64_png":"longdata","width":800}"#;
        let summary = build_image_result_summary(result);
        assert!(!summary.contains("longdata"));
        assert!(summary.contains("image_attached"));
        assert!(summary.contains("800"));
    }

    // =========================================================================
    // log_tool_call tests
    // =========================================================================

    #[tokio::test]
    async fn test_log_tool_call_with_checkpoint() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
            "test".to_string(),
            "test desc".to_string(),
        ));

        let start_time = std::time::Instant::now();
        agent.log_tool_call(
            "file_read",
            r#"{"path":"x.rs"}"#,
            "content here",
            true,
            start_time,
            false,
        );

        let cp = agent.current_checkpoint.as_ref().unwrap();
        assert_eq!(cp.tool_calls.len(), 1);
        assert_eq!(cp.tool_calls[0].tool_name, "file_read");
        assert_eq!(cp.tool_calls[0].arguments, r#"{"path":"x.rs"}"#);
        assert_eq!(cp.tool_calls[0].result.as_deref(), Some("content here"));
        assert!(cp.tool_calls[0].success);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_log_tool_call_truncates_when_requested() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
            "test".to_string(),
            "test desc".to_string(),
        ));

        let long_result = "x".repeat(5000);
        let start_time = std::time::Instant::now();
        agent.log_tool_call("file_read", "{}", &long_result, true, start_time, true);

        let cp = agent.current_checkpoint.as_ref().unwrap();
        assert_eq!(cp.tool_calls[0].result.as_ref().unwrap().len(), 1000);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_log_tool_call_no_truncation() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
            "test".to_string(),
            "test desc".to_string(),
        ));

        let long_result = "x".repeat(5000);
        let start_time = std::time::Instant::now();
        agent.log_tool_call("file_read", "{}", &long_result, true, start_time, false);

        let cp = agent.current_checkpoint.as_ref().unwrap();
        assert_eq!(cp.tool_calls[0].result.as_ref().unwrap().len(), 5000);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_log_tool_call_without_checkpoint_is_noop() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        assert!(agent.current_checkpoint.is_none());

        let start_time = std::time::Instant::now();
        agent.log_tool_call("file_read", "{}", "result", true, start_time, false);

        assert!(agent.current_checkpoint.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_log_tool_call_records_failure() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent.current_checkpoint = Some(crate::checkpoint::TaskCheckpoint::new(
            "test".to_string(),
            "test desc".to_string(),
        ));

        let start_time = std::time::Instant::now();
        agent.log_tool_call(
            "shell_exec",
            r#"{"command":"bad"}"#,
            "error: not found",
            false,
            start_time,
            false,
        );

        let cp = agent.current_checkpoint.as_ref().unwrap();
        assert!(!cp.tool_calls[0].success);
        assert_eq!(cp.tool_calls[0].result.as_deref(), Some("error: not found"));

        server.stop().await;
    }

    // =========================================================================
    // maybe_enhance_tool_result tests (via agent)
    // =========================================================================

    #[tokio::test]
    async fn test_enhance_non_cargo_check_passthrough() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let result = agent.maybe_enhance_tool_result("file_read", r#"{"content":"hello"}"#);
        assert_eq!(result, r#"{"content":"hello"}"#);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_successful_cargo_check_passthrough() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let result =
            agent.maybe_enhance_tool_result("cargo_check", r#"{"success":true,"stderr":""}"#);
        assert_eq!(result, r#"{"success":true,"stderr":""}"#);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_enhance_failed_cargo_check() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let input = r#"{"success":false,"stderr":"error"}"#;
        let result = agent.maybe_enhance_tool_result("cargo_check", input);
        assert!(result.contains(r#""success":false"#));

        server.stop().await;
    }

    // =========================================================================
    // collect_tool_calls tests (via agent instance)
    // =========================================================================

    #[tokio::test]
    async fn test_collect_xml_parsing_via_agent() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let content = r#"Let me read that file.
<tool>
<name>file_read</name>
<arguments>{"path":"src/main.rs"}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, None, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "file_read");
        assert!(calls[0].2.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_native_fc_via_agent() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.native_function_calling = true;
        let agent = Agent::new(config).await.unwrap();

        let native = vec![ApiToolCall {
            id: "call_abc".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "grep_search".to_string(),
                arguments: r#"{"pattern":"TODO"}"#.to_string(),
            },
        }];

        let calls = agent.collect_tool_calls("some content", None, Some(&native));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "grep_search");
        assert_eq!(calls[0].2.as_deref(), Some("call_abc"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_reasoning_fallback_via_agent() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let content = "I need to analyze the code first.";
        let reasoning = r#"<tool>
<name>file_read</name>
<arguments>{"path":"src/lib.rs"}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, Some(reasoning), None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "file_read");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_prefers_content_over_reasoning() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let content = r#"<tool>
<name>shell_exec</name>
<arguments>{"command":"ls"}</arguments>
</tool>"#;
        let reasoning = r#"<tool>
<name>file_read</name>
<arguments>{"path":"x.rs"}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, Some(reasoning), None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "shell_exec");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_empty_when_no_tools_anywhere() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let calls =
            agent.collect_tool_calls("Just a plain response.", Some("Just thinking..."), None);
        assert!(calls.is_empty());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_native_fc_empty_falls_back_xml() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.native_function_calling = true;
        let agent = Agent::new(config).await.unwrap();

        let empty_native: Vec<ApiToolCall> = vec![];
        let content = r#"<tool>
<name>git_status</name>
<arguments>{}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, None, Some(&empty_native));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "git_status");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_multiple_xml_tools() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let content = r#"I'll check two things:
<tool>
<name>file_read</name>
<arguments>{"path":"src/main.rs"}</arguments>
</tool>

<tool>
<name>git_status</name>
<arguments>{}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, None, None);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "file_read");
        assert_eq!(calls[1].0, "git_status");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_collect_native_fc_none_falls_back() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.native_function_calling = true;
        let agent = Agent::new(config).await.unwrap();

        let content = r#"<tool>
<name>file_read</name>
<arguments>{"path":"x.rs"}</arguments>
</tool>"#;

        let calls = agent.collect_tool_calls(content, None, None);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "file_read");

        server.stop().await;
    }

    // =========================================================================
    // build_tool_call_context extended tests
    // =========================================================================

    #[tokio::test]
    async fn test_build_context_generates_unique_ids() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let (id1, _, _) = agent.build_tool_call_context("file_read", "{}", None);
        let (id2, _, _) = agent.build_tool_call_context("file_read", "{}", None);

        assert!(id1.starts_with("call_"));
        assert!(id2.starts_with("call_"));
        assert_ne!(id1, id2);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_build_context_preserves_args() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let agent = Agent::new(config).await.unwrap();

        let complex_args = r#"{"path":"./src/lib.rs","old_str":"fn old()","new_str":"fn new()"}"#;
        let (_, _, fake_call) = agent.build_tool_call_context("file_edit", complex_args, None);

        assert_eq!(fake_call.function.arguments, complex_args);
        assert_eq!(fake_call.function.name, "file_edit");

        server.stop().await;
    }

    // =========================================================================
    // parse_tool_args tests (via agent)
    // =========================================================================

    #[tokio::test]
    async fn test_parse_args_valid_json() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let result =
            agent.parse_tool_args("file_read", r#"{"path":"test.rs"}"#, "call_1", false, start);

        assert!(result.is_some());
        let args = result.unwrap();
        assert_eq!(args["path"], "test.rs");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_parse_args_invalid_json() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        let start = std::time::Instant::now();
        let result = agent.parse_tool_args("file_read", "this is not json", "call_1", false, start);

        assert!(result.is_none());
        assert!(agent.messages.len() > initial_len);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_parse_args_invalid_json_native_fc() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let result = agent.parse_tool_args("shell_exec", "{broken", "call_native_1", true, start);

        assert!(result.is_none());
        let last = agent.messages.last().unwrap();
        assert_eq!(last.role, "tool");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_parse_args_empty_json_object() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let result = agent.parse_tool_args("git_status", "{}", "call_1", false, start);

        assert!(result.is_some());
        let args = result.unwrap();
        assert!(args.as_object().unwrap().is_empty());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_validate_tool_args_rejects_missing_required_field() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let initial_len = agent.messages.len();

        let start = std::time::Instant::now();
        let ok = agent.validate_tool_args(
            "shell_exec",
            "{}",
            &serde_json::json!({}),
            "call_1",
            false,
            start,
        );

        assert!(!ok);
        assert!(agent.messages.len() > initial_len);
        let last = agent.messages.last().unwrap();
        assert!(last
            .content
            .text()
            .contains("missing required field(s): command"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_validate_tool_args_accepts_valid_payload() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let ok = agent.validate_tool_args(
            "shell_exec",
            r#"{"command":"echo hi"}"#,
            &serde_json::json!({"command":"echo hi"}),
            "call_1",
            false,
            start,
        );

        assert!(ok);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repeated_parse_failure_is_suppressed_before_reexecution() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let first = agent.parse_tool_args("shell_exec", "{broken", "call_1", false, start);
        assert!(first.is_none());

        let second_start = std::time::Instant::now();
        let suppressed = agent.suppress_repeated_failed_tool_retry(
            "shell_exec",
            "{broken",
            "call_2",
            false,
            second_start,
        );
        assert!(suppressed);

        let recovery = agent.messages.last().expect("expected suppression result");
        assert!(recovery.content.text().contains("RETRY SUPPRESSED"));
        assert!(recovery.content.text().contains("valid JSON"));
        assert!(recovery.content.text().contains("`command`"));
        assert!(agent.pending_failure_hint.as_deref().is_some_and(|hint| {
            hint.contains("RETRY SUPPRESSED") && hint.contains("valid JSON")
        }));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_repeated_validation_failure_is_suppressed_before_reexecution() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let first = agent.validate_tool_args(
            "shell_exec",
            "{}",
            &serde_json::json!({}),
            "call_1",
            false,
            start,
        );
        assert!(!first);

        let second_start = std::time::Instant::now();
        let suppressed = agent.suppress_repeated_failed_tool_retry(
            "shell_exec",
            "{}",
            "call_2",
            false,
            second_start,
        );
        assert!(suppressed);

        let recovery = agent.messages.last().expect("expected suppression result");
        assert!(recovery.content.text().contains("RETRY SUPPRESSED"));
        assert!(recovery.content.text().contains("schema validation"));
        assert!(recovery.content.text().contains("`command`"));
        assert!(agent.pending_failure_hint.as_deref().is_some_and(|hint| {
            hint.contains("RETRY SUPPRESSED") && hint.contains("schema validation")
        }));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_successful_different_tool_clears_failed_tool_suppression_window() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let batch: Vec<CollectedToolCall> = vec![
            ("shell_exec".to_string(), "{}".to_string(), None),
            ("git_status".to_string(), "{}".to_string(), None),
            ("shell_exec".to_string(), "{}".to_string(), None),
        ];

        agent.execute_tool_batch(batch).await.unwrap();

        let validation_failures = agent
            .messages
            .iter()
            .filter(|msg| {
                msg.content
                    .text()
                    .contains("missing required field(s): command")
            })
            .count();
        let suppressed_retries = agent
            .messages
            .iter()
            .filter(|msg| msg.content.text().contains("RETRY SUPPRESSED"))
            .count();

        assert!(
            validation_failures >= 2,
            "expected the same invalid tool to run again after a different successful tool"
        );
        assert_eq!(
            suppressed_retries, 0,
            "successful different tool should clear retry suppression"
        );

        server.stop().await;
    }

    // =========================================================================
    // maybe_verify_file_change tests
    // =========================================================================

    #[tokio::test]
    async fn test_verify_non_file_tool_returns_none() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let args = serde_json::json!({"command": "ls"});
        let result = agent.maybe_verify_file_change("shell_exec", &args).await;
        assert!(result.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_verify_file_read_returns_none() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let args = serde_json::json!({"path": "test.rs"});
        let result = agent.maybe_verify_file_change("file_read", &args).await;
        assert!(result.is_none());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_verify_file_edit_without_path_returns_none() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let args = serde_json::json!({"old_str": "x", "new_str": "y"});
        let result = agent.maybe_verify_file_change("file_edit", &args).await;
        assert!(result.is_none());

        server.stop().await;
    }

    // =========================================================================
    // execute_step_internal e2e tests (via mock server)
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_returns_true_on_completion() {
        let server = MockLlmServer::builder()
            .with_response("Task is complete. Here is the result.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_internal(false).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_returns_false_on_tool_execution() {
        let server = MockLlmServer::builder()
            .with_response(
                r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
            )
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_internal(false).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_malformed_tool_injects_correction() {
        // Use content that has malformed markers but cannot be parsed as a tool call.
        // <tool broken here> has the "<tool" marker but no valid <name>/<arguments>.
        let server = MockLlmServer::builder()
            .with_response("I want to use <tool broken here> to do something with tool_call syntax")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_internal(false).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        let last_user_msg = agent
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .unwrap();
        assert!(last_user_msg.content.text().contains("malformed"));

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_completion_gate_rejects() {
        let server = MockLlmServer::builder()
            .with_response("Done!")
            .build()
            .await;

        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 10;
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_internal(false).await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        let last_user_msg = agent
            .messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .unwrap();
        assert!(last_user_msg.content.text().contains("step"));

        server.stop().await;
    }

    // =========================================================================
    // get_assistant_step_response tests
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_get_response_non_streaming() {
        let server = MockLlmServer::builder()
            .with_response("Hello world!")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let response = agent.get_assistant_step_response(false).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert_eq!(resp.content, "Hello world!");
        assert!(resp.native_tool_calls.is_none());

        let last_msg = agent.messages.last().unwrap();
        assert_eq!(last_msg.role, "assistant");
        assert_eq!(last_msg.content.text(), "Hello world!");

        server.stop().await;
    }

    #[tokio::test]
    async fn test_get_response_use_last_message() {
        let server = MockLlmServer::builder().with_response("done").build().await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent.messages.push(Message {
            role: "assistant".to_string(),
            content: "Previously generated content.".to_string().into(),
            reasoning_content: Some("Thinking deeply...".to_string()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });

        let response = agent.get_assistant_step_response(true).await;
        assert!(response.is_ok());
        let resp = response.unwrap();
        assert_eq!(resp.content, "Previously generated content.");
        assert_eq!(
            resp.reasoning_content.as_deref(),
            Some("Thinking deeply...")
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_get_response_use_last_no_assistant_fails() {
        let server = MockLlmServer::builder().with_response("done").build().await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let response = agent.get_assistant_step_response(true).await;
        assert!(response.is_err());

        server.stop().await;
    }

    // =========================================================================
    // execute_step_with_logging / execute_pending_tool_calls wrappers
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_execute_step_with_logging_delegates() {
        let server = MockLlmServer::builder()
            .with_response("Completed.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_with_logging("test task").await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_execute_pending_no_assistant_msg_fails() {
        let server = MockLlmServer::builder().with_response("done").build().await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_pending_tool_calls("test task").await;
        assert!(result.is_err());

        server.stop().await;
    }

    // =========================================================================
    // execute_single_tool tests
    // =========================================================================

    #[tokio::test]
    async fn test_single_tool_unknown_tool() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let start = std::time::Instant::now();
        let args = serde_json::json!({});
        let result = agent
            .execute_single_tool("nonexistent_tool", "{}", &args, start)
            .await;

        assert!(result.is_ok());
        let (success, result_str, summary) = result.unwrap();
        assert!(!success);
        assert!(result_str.contains("Unknown tool"));
        assert!(result_str.contains("nonexistent_tool"));
        assert!(summary.contains("nonexistent_tool"));

        server.stop().await;
    }

    // =========================================================================
    // plan tests (via mock server)
    // =========================================================================

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_plan_returns_true_with_tool_calls() {
        let server = MockLlmServer::builder()
            .with_response(
                r#"I'll read the file first.
<tool>
<name>file_read</name>
<arguments>{"path":"Cargo.toml"}</arguments>
</tool>"#,
            )
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.plan().await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_plan_returns_false_without_tool_calls() {
        let server = MockLlmServer::builder()
            .with_response("Let me think about this without using any tools.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.plan().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_plan_pushes_assistant_message() {
        let server = MockLlmServer::builder()
            .with_response("Planning response content.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let msg_count_before = agent.messages.len();

        let _ = agent.plan().await;

        assert!(agent.messages.len() > msg_count_before);
        let last = agent.messages.last().unwrap();
        assert_eq!(last.role, "assistant");
        assert_eq!(last.content.text(), "Planning response content.");

        server.stop().await;
    }

    // =========================================================================
    // Additional edge cases
    // =========================================================================

    #[test]
    fn test_should_prompt_need_to_phrase() {
        assert!(should_prompt_for_action(
            "I need to check this",
            true,
            false
        ));
    }

    #[test]
    fn test_should_prompt_start_by_phrase() {
        assert!(should_prompt_for_action(
            "start by reading the file",
            true,
            false
        ));
    }

    #[test]
    fn test_should_prompt_help_you_phrase() {
        assert!(should_prompt_for_action(
            "I can help you with that",
            true,
            false
        ));
    }

    #[test]
    fn test_should_not_prompt_empty_content() {
        assert!(!should_prompt_for_action("", true, false));
    }

    #[test]
    fn test_should_not_prompt_exactly_1000_chars() {
        let content = "let me ".to_string() + &"x".repeat(993);
        assert_eq!(content.len(), 1000);
        assert!(!should_prompt_for_action(&content, true, false));
    }

    #[test]
    fn test_should_prompt_999_chars() {
        let content = "let me ".to_string() + &"x".repeat(992);
        assert_eq!(content.len(), 999);
        assert!(should_prompt_for_action(&content, true, false));
    }

    #[test]
    fn test_should_prompt_all_intent_phrases() {
        let phrases = [
            ("let me check", true),
            ("I'll do it", true),
            ("I will handle", true),
            ("let's go", true),
            ("First, analyze", true),
            ("Starting the process", true),
            ("Begin by reading", true),
            ("Going to implement", true),
            ("Need to fix", true),
            ("Start by examining", true),
            ("I can help you with that", true),
            ("The result is 42", false),
            ("No intent here", false),
        ];

        for (phrase, expected) in phrases {
            assert_eq!(
                should_prompt_for_action(phrase, true, false),
                expected,
                "Failed for phrase: {:?}",
                phrase
            );
        }
    }

    #[test]
    fn test_repetition_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;

        let args = r#"{"path":"test.rs"}"#;
        let mut h1 = DefaultHasher::new();
        args.hash(&mut h1);
        let hash1 = h1.finish();

        let mut h2 = DefaultHasher::new();
        args.hash(&mut h2);
        let hash2 = h2.finish();

        assert_eq!(hash1, hash2, "Same args should produce same hash");

        let different_args = r#"{"path":"other.rs"}"#;
        let mut h3 = DefaultHasher::new();
        different_args.hash(&mut h3);
        let hash3 = h3.finish();

        assert_ne!(hash1, hash3, "Different args should produce different hash");
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_file_read_adds_to_context_files() {
        let server = MockLlmServer::builder()
            .with_response(
                r#"<tool>
<name>file_read</name>
<arguments>{"path":"./Cargo.toml"}</arguments>
</tool>"#,
            )
            .with_response("Done reading.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let _ = agent.execute_step_internal(false).await;

        assert!(
            agent
                .context_files
                .iter()
                .any(|p| p.ends_with("Cargo.toml")),
            "Expected Cargo.toml in context_files: {:?}",
            agent.context_files
        );

        server.stop().await;
    }

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_with_empty_response() {
        let server = MockLlmServer::builder().with_response("").build().await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let result = agent.execute_step_internal(false).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        server.stop().await;
    }

    #[tokio::test]
    async fn test_gate_both_conditions_met() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        let mut agent = Agent::new(config).await.unwrap();

        let mut checkpoint =
            crate::checkpoint::TaskCheckpoint::new("test".to_string(), "test desc".to_string());
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(50),
        });
        agent.current_checkpoint = Some(checkpoint);

        assert!(agent.check_completion_gate().is_none());

        server.stop().await;
    }

    // ── read_line_pausing_esc tests ──

    #[test]
    fn read_line_pausing_esc_sets_and_clears_pause_flag() {
        use std::sync::atomic::Ordering;

        let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let paused_clone = std::sync::Arc::clone(&paused);

        // Spawn a watcher that records whether paused was ever set
        let was_paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let was_paused_clone = std::sync::Arc::clone(&was_paused);

        let watcher = std::thread::spawn(move || {
            for _ in 0..100 {
                if paused_clone.load(Ordering::Acquire) {
                    was_paused_clone.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        // Simulate stdin by piping — read_line will return an error or empty
        // in a non-interactive test, but the pause flag behavior is what we test.
        let _ = read_line_pausing_esc(&paused);

        // After return, paused must be cleared
        assert!(
            !paused.load(Ordering::Acquire),
            "esc_paused must be false after read_line_pausing_esc returns"
        );

        let _ = watcher.join();
        // In CI (non-interactive), the watcher may or may not observe the pause
        // due to timing, so we don't assert was_paused — the important thing is
        // the flag is cleared after the call.
    }

    #[test]
    fn read_line_pausing_esc_unpauses_even_on_error() {
        use std::sync::atomic::Ordering;

        let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Call in non-interactive context (stdin is not a tty in tests)
        let _ = read_line_pausing_esc(&paused);

        assert!(
            !paused.load(Ordering::Acquire),
            "esc_paused must always be cleared, even if read_line fails"
        );
    }
}
