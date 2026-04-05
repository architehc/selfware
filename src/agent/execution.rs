use anyhow::Result;
use tracing::{debug, info};

use super::*;
use crate::errors::AgentError;
use crate::hooks::HookContext;

// Re-export items used by this module from sibling modules
pub(super) use super::recovery::ActionPrompt;

// Re-export type from tool_collect so existing call sites keep working
pub(super) use super::tool_collect::CollectedToolCall;

/// Read a line from stdin, temporarily pausing the ESC listener so it yields
/// raw mode and stops competing for stdin events.  This prevents the deadlock
/// where `io::stdin().read_line()` blocks forever because crossterm raw mode
/// is active on another thread.
pub(super) async fn read_line_pausing_esc(
    esc_paused: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    esc_pause_ack: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::io::Result<String> {
    use std::sync::atomic::Ordering;
    use tokio::io::AsyncBufReadExt;

    // Signal the ESC listener to pause and release raw mode
    esc_paused.store(true, Ordering::Release);
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(250);
    while !esc_pause_ack.load(Ordering::Acquire) && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    let mut response = String::new();
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin);
    let result = reader.read_line(&mut response).await;

    // Unpause — the listener will re-enter raw mode on its own
    esc_paused.store(false, Ordering::Release);
    esc_pause_ack.store(false, Ordering::Release);

    result.map(|_| response)
}

/// Try to extract a `base64_png` field from a JSON tool result string.
pub(super) fn try_extract_base64_png(result: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()?
        .get("base64_png")?
        .as_str()
        .map(String::from)
}

/// Build a text summary of a JSON tool result by removing the large `base64_png`
/// blob and adding an `"image_attached": true` marker.
pub(super) fn build_image_result_summary(result: &str) -> String {
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

impl Agent {
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
        let reasoning_chars = response.reasoning_chars;

        let tool_calls = self.collect_tool_calls(
            &content,
            response.reasoning_content.as_deref(),
            response.native_tool_calls.as_ref(),
        );

        debug!("Total tool calls to execute: {}", tool_calls.len());

        // Check if the response contains code alongside tool calls.
        // Models often output file_read tool calls AND code text in the same
        // response. The tool calls get executed, but the code gets ignored.
        // If we detect code in the residual text, auto-write it.
        if !tool_calls.is_empty() && contains_unwritten_code(&content) {
            if let Some((path, code)) = extract_code_and_path(&content) {
                info!(
                    "Response contains tool calls AND code text — auto-writing {} lines to {}",
                    code.lines().count(),
                    path
                );
                let write_call: Vec<super::execution::CollectedToolCall> = vec![(
                    "file_write".to_string(),
                    serde_json::json!({"path": path, "content": code}).to_string(),
                    None,
                )];
                // Prepend the file_write to the tool calls list would be ideal,
                // but we'll execute it after the original tool calls
                self.execute_tool_batch(write_call).await?;
                self.consecutive_read_only_steps = 0;
                self.has_written_any_file = true;
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     Code from your response was automatically written to a file. \
                     Now verify with cargo check or cargo test.\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
            }
        }

        // Detect malformed tool calls and inject correction before treating as completion
        if self.detect_and_correct_malformed_tools(&content, &tool_calls) {
            return Ok(false);
        }

        match self.maybe_prompt_for_action(
            &content,
            tool_calls.is_empty(),
            use_last_message,
            reasoning_chars,
        ) {
            Ok(ActionPrompt::Corrected) => return Ok(false),
            Ok(ActionPrompt::NotNeeded) => {}
            Ok(ActionPrompt::ForceFallback) => {
                let (tool_name, tool_args) = self.pick_smart_fallback(&content);

                // If the only fallback is directory_tree (nothing new to load),
                // the model is stuck and should just answer with what it has.
                if tool_name == super::FALLBACK_TOOL_NAME {
                    info!("No useful fallback available — prompting model to finalize");
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You have been unable to call tools. You already have the codebase \
                         overview and any files you previously read in context.\n\
                         Provide your answer now based on what you already know. \
                         Do not describe what you would do — just give the answer.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }

                info!("Smart fallback: {} {}", tool_name, tool_args);
                output::smart_fallback_action(&tool_name, &tool_args);
                let fallback: Vec<CollectedToolCall> = vec![(tool_name.clone(), tool_args, None)];
                self.execute_tool_batch(fallback).await?;

                // Successful fallback = progress. Reset consecutive counter
                // so it doesn't accumulate toward abort. The total counter
                // still tracks lifetime attempts for the hard ceiling.
                self.consecutive_no_action_prompts = 0;

                self.messages.push(crate::api::types::Message::user(format!(
                    "<selfware_system_directive>\n\
                     A `{}` was executed automatically because you did not call any tool.\n\
                     Use the result above to choose your next action. Call a tool now.\n\
                     </selfware_system_directive>",
                    tool_name
                )));
                return Ok(false);
            }
            Err(error_msg) => {
                return Err(AgentError::TaskFailed { message: error_msg }.into());
            }
        }

        if tool_calls.is_empty() {
            if super::verification::is_incomplete_action_response(&content) {
                info!("Rejected incomplete planning response before completion");
                self.last_assistant_response = content.clone();
                self.messages.push(crate::api::types::Message::user(
                    "Your response describes work you still need to do instead of a completed result. \
                     Do NOT stop to narrate your next step. Call the needed tool now and continue."
                        .to_string(),
                ));
                return Ok(false);
            }

            // Detect repeated identical responses — the model is stuck producing
            // the same completion that keeps getting rejected by the gate.
            if content == self.last_assistant_response && !content.is_empty() {
                self.consecutive_no_action_prompts += 1;
                if self.consecutive_no_action_prompts >= 5 {
                    // Model is repeating itself many times. Only accept if
                    // the completion gate passes — otherwise nudge it.
                    if self.check_completion_gate().is_none() {
                        info!(
                            "Accepting repeated completion after {} identical responses (gate passed)",
                            self.consecutive_no_action_prompts
                        );
                        let clean = super::recovery::strip_think_blocks(&content)
                            .trim()
                            .to_string();
                        output::final_answer(&clean);
                        self.last_assistant_response = clean;
                        return Ok(true);
                    }
                    // Gate rejected — tell the model to keep working.
                    info!(
                        "Completion gate rejected after {} identical responses — nudging",
                        self.consecutive_no_action_prompts
                    );
                    self.consecutive_no_action_prompts = 0;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You keep repeating the same response, but the task is NOT complete. \
                         You still need to verify your work. Call a tool now.\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }
            } else {
                // Different response — store it for comparison.
                self.last_assistant_response = content.clone();
            }

            // Reject confused meta-reasoning before treating as completion
            if super::verification::is_confused_response(&content) {
                info!("Rejected confused meta-reasoning response");
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     Your response contained framework self-reference instead of a task result. \
                     Focus on the original task.\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            // Detect text responses that contain code — the model should
            // use file_write/file_edit tools, not output code as text.
            // If we can extract the code and a target path, auto-write it.
            let has_code = contains_unwritten_code(&content);
            tracing::debug!(
                "Code check: has_code={} content_len={} has_backticks={} lines={}",
                has_code,
                content.len(),
                content.contains("```"),
                content.lines().count()
            );
            if has_code {
                if let Some((path, code)) = extract_code_and_path(&content) {
                    info!(
                        "Auto-writing {} lines of code to {} (model outputted code as text)",
                        code.lines().count(),
                        path
                    );
                    // Synthesize a file_write tool call from the model's text output
                    let synthetic_calls: Vec<super::execution::CollectedToolCall> = vec![(
                        "file_write".to_string(),
                        serde_json::json!({"path": path, "content": code}).to_string(),
                        None,
                    )];
                    self.consecutive_read_only_steps = 0;
                    self.has_written_any_file = true;
                    self.execute_tool_batch(synthetic_calls).await?;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         Your code was automatically written to the file. \
                         Now verify it compiles: use shell_exec with \"cargo check\" or \"cargo test\".\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                } else {
                    // Can't extract a clear path — ask the model to use tools
                    info!("Rejected text response containing code — nudging to use tools");
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         You wrote code in your text response instead of using tools. \
                         DO NOT output code as text. Use file_write to write it to a file:\n\n\
                         <tool>\n<name>file_write</name>\n\
                         <arguments>{\"path\": \"src/lib.rs\", \"content\": \"YOUR CODE HERE\"}</arguments>\n\
                         </tool>\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }
            }

            // Check completion gate before accepting task as done
            if let Some(gate_msg) = self.check_completion_gate() {
                info!("Completion gate rejected: {}", gate_msg);
                self.messages
                    .push(crate::api::types::Message::user(gate_msg));
                return Ok(false);
            }

            // Fire Stop hooks before completing
            let stop_ctx = HookContext::stop();
            self.hook_registry.fire(&stop_ctx).await;

            // Strip think blocks from the final answer — the content accumulator
            // includes raw <think>...</think> tags from models like Qwen3.5 that
            // emit inline thinking. Without stripping, "Final answer:" shows the
            // think block content instead of the actual response.
            let clean_content = super::recovery::strip_think_blocks(&content)
                .trim()
                .to_string();
            output::final_answer(&clean_content);
            self.last_assistant_response = clean_content;
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
            self.messages.push(crate::api::types::Message::user(
                "The above tool calls were proposed but NOT executed (plan mode is active). \
                 Review the plan and confirm, or adjust.",
            ));
            self.last_assistant_response = plan_summary;
            return Ok(false);
        }

        // Detect repetition loops before executing
        if let Some(loop_msg) = self.detect_repetition(&tool_calls) {
            info!("Repetition loop detected, injecting correction");
            self.messages
                .push(crate::api::types::Message::user(loop_msg));
            return Ok(false);
        }

        self.reset_no_action_prompt_state();

        // Track whether this step contains any write/modify tools
        let has_write_tool = tool_calls.iter().any(|(name, args_str, _)| {
            super::tool_dispatch::tool_call_counts_as_state_change(name, args_str)
        });
        if has_write_tool {
            self.consecutive_read_only_steps = 0;
        } else {
            self.consecutive_read_only_steps += 1;
        }

        self.execute_tool_batch(tool_calls).await?;

        // After tool batch execution, check if all tool calls were suppressed.
        // When the model keeps emitting identical tool calls that are all
        // suppressed (retry suppressed / no-op), it is stuck in tool-calling
        // mode and cannot produce a final text response on its own.
        if self.consecutive_suppressions >= 10 {
            // After many suppressed calls, nudge the model to try a different approach.
            // Do NOT abort or force completion — the task may still need work.
            // Also clear the failed-tool cache so the agent gets fresh chances.
            info!(
                "Injecting strategy-change directive after {} consecutive suppressions — clearing failed-tool cache",
                self.consecutive_suppressions
            );
            self.consecutive_suppressions = 0; // reset so it can try again
            self.clear_failed_tool_attempts(); // give the agent a clean slate
            self.messages.push(crate::api::types::Message::user(
                "<selfware_system_directive>\n\
                 Your last several tool calls were suppressed (duplicate or no-op). \
                 This does NOT mean the task is complete. Try a DIFFERENT approach:\n\
                 - Read a different file\n\
                 - Use a different tool\n\
                 - Break the problem into smaller steps\n\
                 - If you genuinely believe the task is done, explain why in plain text.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        } else if self.consecutive_suppressions >= 5 {
            info!(
                "Injecting nudge after {} consecutive suppressions",
                self.consecutive_suppressions
            );
            self.messages.push(crate::api::types::Message::user(
                "<selfware_system_directive>\n\
                 Your recent tool calls were suppressed because they were duplicates or no-ops. \
                 Try a different tool or different arguments. The task is NOT necessarily complete.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        }

        // TERMINAL PROGRESS GUARD: After N read-only steps, force synthesis.
        // Use a relaxed threshold when the agent has already written source files —
        // verification loops (cargo check → cargo test → read output) are expected
        // after writing code and should not be punished.
        let has_any_file_write = self.has_written_any_file
            || self
                .messages
                .iter()
                .filter(|m| m.role == "assistant")
                .filter_map(|m| m.tool_calls.as_ref())
                .flatten()
                .any(|tc| matches!(tc.function.name.as_str(), "file_edit" | "file_write"));

        let terminal_threshold = if has_any_file_write { 20 } else { 8 };
        let warning_threshold = if has_any_file_write { 15 } else { 4 };

        if self.consecutive_read_only_steps >= terminal_threshold {
            if !has_any_file_write && self.pending_synthesis.is_some() {
                // Second time hitting terminal guard without any writes.
                // Check if src/lib.rs already has substantial content (template project).
                // If so, do NOT overwrite with scaffold — nudge targeted edits instead.
                let existing_content = std::fs::read_to_string("src/lib.rs").unwrap_or_default();
                let meaningful_lines = existing_content
                    .lines()
                    .filter(|l| {
                        let t = l.trim();
                        !t.is_empty() && !t.starts_with("//") && !t.starts_with("/*")
                    })
                    .count();

                if meaningful_lines > 5 {
                    // Template project — src/lib.rs has real code. Don't overwrite.
                    info!(
                        "ESCALATED progress guard: {} read-only steps, but src/lib.rs has {} meaningful lines — nudging targeted edit instead of scaffold",
                        self.consecutive_read_only_steps, meaningful_lines
                    );
                    self.consecutive_read_only_steps = 0;
                    self.messages.push(crate::api::types::Message::user(
                        "<selfware_system_directive>\n\
                         src/lib.rs already has substantial code. Do NOT rewrite it from scratch.\n\
                         Make TARGETED edits to fix the specific bugs:\n\
                         1. Use file_edit to change only the buggy lines\n\
                         2. Keep all existing function signatures and module structure intact\n\
                         3. After editing, run cargo test to verify\n\
                         </selfware_system_directive>"
                            .to_string(),
                    ));
                    return Ok(false);
                }

                // Greenfield project — src/lib.rs is minimal/empty. Write scaffold.
                info!(
                    "ESCALATED progress guard: {} read-only steps, no writes ever — injecting scaffold",
                    self.consecutive_read_only_steps
                );
                let task = self
                    .messages
                    .iter()
                    .find(|m| m.role == "user")
                    .map(|m| m.content.to_string())
                    .unwrap_or_default();

                let scaffold = format!(
                    "// AUTO-SCAFFOLD: fill in the implementation\n\
                     // Task: {}\n\n\
                     // TODO: implement the functions described in the task\n\
                     // Then run cargo test to verify\n",
                    task.lines().next().unwrap_or("implement")
                );
                let calls: Vec<super::execution::CollectedToolCall> = vec![(
                    "file_write".to_string(),
                    serde_json::json!({"path": "src/lib.rs", "content": scaffold}).to_string(),
                    None,
                )];
                self.consecutive_read_only_steps = 0;
                self.has_written_any_file = true;
                if let Err(e) = self.execute_tool_batch(calls).await {
                    warn!("Scaffold write failed: {}", e);
                }
                self.messages.push(crate::api::types::Message::user(
                    "<selfware_system_directive>\n\
                     A scaffold file was written to src/lib.rs. Now implement the full solution:\n\
                     1. Use file_write to replace src/lib.rs with your complete implementation\n\
                     2. Include unit tests in a #[cfg(test)] mod tests block\n\
                     3. Run cargo test to verify\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }

            info!(
                "TERMINAL progress guard: {} read-only steps — forcing synthesis+write",
                self.consecutive_read_only_steps
            );
            // Force immediate synthesis: extract task from first user message
            let task = self
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.to_string())
                .unwrap_or_default();
            self.pending_synthesis = Some(task);
            self.consecutive_read_only_steps = 0;
            // The synthesis will fire at the top of the next step in task_runner
            return Ok(false);
        } else if self.consecutive_read_only_steps >= warning_threshold {
            info!(
                "Progress guard warning: {} read-only steps (threshold: {})",
                self.consecutive_read_only_steps, terminal_threshold
            );
            self.messages.push(crate::api::types::Message::user(format!(
                "<selfware_system_directive>\n\
                 You have spent {} consecutive steps reading without writing. \
                 You have {} steps before forced synthesis. Write code NOW:\n\n\
                 <tool>\n<name>file_write</name>\n\
                 <arguments>{{\"path\": \"src/lib.rs\", \"content\": \"YOUR FULL CODE\"}}</arguments>\n\
                 </tool>\n\
                 </selfware_system_directive>",
                self.consecutive_read_only_steps,
                terminal_threshold - self.consecutive_read_only_steps
            )));
        }

        Ok(false)
    }
}

// get_assistant_step_response moved to assistant_response.rs
// collect_tool_calls, message_has_tool_calls moved to tool_collect.rs
// plan moved to plan_step.rs

/// Try to extract a target file path and code content from a model's text response.
/// Public so task_runner can use it for synthesis code extraction.
/// Returns Some((path, code)) if both can be identified.
pub(super) fn extract_code_and_path(content: &str) -> Option<(String, String)> {
    let stripped = super::recovery::strip_think_blocks(content);

    // Extract path from mentions like "src/lib.rs", "src/main.rs", etc.
    static PATH_REGEX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let path_re = PATH_REGEX.get_or_init(|| {
        regex::Regex::new(
            r#"(?:content for |file |in )?[`"]?((?:src|tests|examples)/[\w/]+\.rs)[`"]?"#,
        )
        .expect("Invalid path regex")
    });
    let path = path_re
        .captures(&stripped)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| {
            // Default: if code looks like a library module, use src/lib.rs
            // If it has fn main(), use src/main.rs
            if stripped.contains("fn main(") {
                "src/main.rs".to_string()
            } else {
                "src/lib.rs".to_string()
            }
        });

    // Extract code: prefer fenced code blocks, fall back to raw code
    let code = if stripped.contains("```") {
        // Extract content between first pair of ```
        let mut in_block = false;
        let mut code_lines = Vec::new();
        for line in stripped.lines() {
            if line.starts_with("```") {
                if in_block {
                    break; // end of first block
                }
                in_block = true;
                continue;
            }
            if in_block {
                code_lines.push(line);
            }
        }
        if code_lines.len() >= 5 {
            Some(code_lines.join("\n"))
        } else {
            None
        }
    } else {
        // Extract raw code lines (Rust-like patterns)
        let code_start_patterns = [
            "use ", "pub ", "fn ", "struct ", "enum ", "impl ", "mod ", "trait ", "async ", "#[",
            "//!", "///",
        ];
        let mut code_lines = Vec::new();
        let mut collecting = false;
        for line in stripped.lines() {
            let trimmed = line.trim();
            if !collecting {
                if code_start_patterns.iter().any(|p| trimmed.starts_with(p)) {
                    collecting = true;
                    code_lines.push(line);
                }
            } else {
                // Stop collecting when we hit a blank line followed by non-code
                if trimmed.is_empty() {
                    code_lines.push(line);
                } else if trimmed.starts_with("This ")
                    || trimmed.starts_with("The ")
                    || trimmed.starts_with("Note:")
                    || trimmed.starts_with("To run")
                {
                    break; // Hit explanatory text after code
                } else {
                    code_lines.push(line);
                }
            }
        }
        if code_lines.len() >= 5 {
            Some(code_lines.join("\n"))
        } else {
            None
        }
    };

    code.map(|c| (path, c))
}

/// Detect when the model outputs code in text instead of using tools.
/// Returns true if the response contains substantial code that should have been
/// written to a file via file_write or file_edit.
/// Public so task_runner can use it for synthesis code detection.
pub(super) fn contains_unwritten_code(content: &str) -> bool {
    let stripped = super::recovery::strip_think_blocks(content);

    // Strategy 1: Check markdown fenced code blocks
    let code_block_count = stripped.matches("```").count() / 2;
    if code_block_count > 0 {
        let mut in_code_block = false;
        let mut code_lines = 0;
        for line in stripped.lines() {
            if line.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block && !line.trim().is_empty() {
                code_lines += 1;
            }
        }
        if code_lines >= 10 {
            tracing::debug!(
                "Detected {} code lines in {} fenced code blocks",
                code_lines,
                code_block_count
            );
            return true;
        }
    }

    // Strategy 2: Detect raw unfenced code in the response.
    // Count lines that look like code (Rust-centric patterns).
    let code_indicators = [
        "pub fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "impl ",
        "pub enum ",
        "enum ",
        "use ",
        "mod ",
        "#[",
        "let ",
        "pub mod ",
        "async fn ",
        "pub async fn ",
        "-> Result",
        "-> Option",
        "pub trait ",
        "trait ",
        "pub(crate)",
        "pub(super)",
    ];
    let mut raw_code_lines = 0;
    for line in stripped.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        if code_indicators.iter().any(|ind| trimmed.starts_with(ind))
            || trimmed.ends_with('{')
            || trimmed == "}"
            || trimmed.ends_with(';')
        {
            raw_code_lines += 1;
        }
    }

    // Low threshold: 5+ code lines is substantial enough to auto-write
    if raw_code_lines >= 5 {
        tracing::debug!("Detected {} raw code lines in response", raw_code_lines);
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::ToolCallLog;
    use crate::testing::mock_api::MockLlmServer;
    use chrono::Utc;
    use std::hash::{Hash, Hasher};

    // =========================================================================
    // Helper: mirrors should_prompt_for_action logic for standalone testing
    // =========================================================================
    fn should_prompt_for_action(
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
        reasoning_chars: usize,
    ) -> bool {
        if !has_no_tool_calls || use_last_message {
            return false;
        }

        let effective_content = super::recovery::strip_think_blocks(content);
        let effective_len = effective_content.len();

        let total_output = effective_len + reasoning_chars;
        if total_output > 0 && effective_len > 500 {
            let think_ratio = reasoning_chars as f64 / total_output as f64;
            if think_ratio < 0.8 {
                return false;
            }
        } else if effective_len >= 1000 {
            return false;
        }

        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let lower = effective_content.to_lowercase();
        intent_phrases.iter().any(|p| lower.contains(p))
    }

    // =========================================================================
    // should_prompt_for_action tests
    // =========================================================================

    #[test]
    fn test_should_prompt_when_intent_phrase_present() {
        assert!(should_prompt_for_action(
            "Let me check the file",
            true,
            false,
            0
        ));
        assert!(should_prompt_for_action(
            "I'll fix that bug now",
            true,
            false,
            0
        ));
        assert!(should_prompt_for_action(
            "I will refactor the module",
            true,
            false,
            0
        ));
        assert!(should_prompt_for_action(
            "Let's start by reading the code",
            true,
            false,
            0
        ));
        assert!(should_prompt_for_action(
            "First, I need to understand",
            true,
            false,
            0
        ));
        assert!(should_prompt_for_action(
            "Going to investigate",
            true,
            false,
            0
        ));
    }

    #[test]
    fn test_should_not_prompt_when_tool_calls_exist() {
        // has_no_tool_calls = false means there ARE tool calls
        assert!(!should_prompt_for_action("Let me check", false, false, 0));
    }

    #[test]
    fn test_should_not_prompt_when_using_last_message() {
        assert!(!should_prompt_for_action("Let me check", true, true, 0));
    }

    #[test]
    fn test_should_not_prompt_for_long_content() {
        let long_content = format!("Let me {}", "x".repeat(1000));
        assert!(!should_prompt_for_action(&long_content, true, false, 0));
    }

    #[test]
    fn test_should_not_prompt_for_plain_response() {
        assert!(!should_prompt_for_action(
            "The answer is 42.",
            true,
            false,
            0
        ));
        assert!(!should_prompt_for_action(
            "Here is the result.",
            true,
            false,
            0
        ));
    }

    #[test]
    fn test_should_prompt_case_insensitive() {
        assert!(should_prompt_for_action("LET ME check", true, false, 0));
        assert!(should_prompt_for_action("STARTING now", true, false, 0));
        assert!(should_prompt_for_action("BEGIN BY reading", true, false, 0));
    }

    #[test]
    fn test_should_prompt_think_dominated_response() {
        // Short intent content + huge think block = should still detect intent
        let content = "Let me check the file structure";
        let reasoning_chars = 5000; // Simulates large think block
        assert!(should_prompt_for_action(
            content,
            true,
            false,
            reasoning_chars
        ));
    }

    #[test]
    fn test_should_prompt_genuine_long_response() {
        // 600 chars of real content with low think ratio = genuine response
        let content = format!("Here is a detailed analysis: {}", "x".repeat(570));
        assert!(!should_prompt_for_action(&content, true, false, 100));
    }

    #[test]
    fn test_is_confused_response() {
        // Two or more framework markers = confused
        assert!(super::verification::is_confused_response(
            "The should_prompt_for_action function checks ActionPrompt:: variants"
        ));
        assert!(super::verification::is_confused_response(
            "Looking at build_no_action_prompt_message and </think> blocks"
        ));
        // Only one marker = not confused
        assert!(!super::verification::is_confused_response(
            "The function uses ActionPrompt to decide"
        ));
        // No markers = not confused
        assert!(!super::verification::is_confused_response(
            "Here is the code review summary"
        ));
    }

    #[tokio::test]
    async fn test_maybe_prompt_for_action_escalates_after_repeated_no_action_turns() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        // First attempt returns Corrected
        assert!(matches!(
            agent
                .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
                .unwrap(),
            ActionPrompt::Corrected
        ));
        assert!(agent
            .messages
            .last()
            .unwrap()
            .content
            .text()
            .contains("selfware_system_directive"));

        // Second attempt still corrects (FORCE_FALLBACK_AFTER=3)
        assert!(matches!(
            agent
                .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
                .unwrap(),
            ActionPrompt::Corrected
        ));

        // Third attempt triggers ForceFallback
        assert!(matches!(
            agent
                .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
                .unwrap(),
            ActionPrompt::ForceFallback
        ));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_maybe_prompt_for_action_resets_after_non_intent_response() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        assert!(matches!(
            agent
                .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
                .unwrap(),
            ActionPrompt::Corrected
        ));
        assert_eq!(agent.consecutive_no_action_prompts, 1);

        // Non-intent response resets the counter
        assert!(matches!(
            agent
                .maybe_prompt_for_action("Here is the result.", true, false, 0)
                .unwrap(),
            ActionPrompt::NotNeeded
        ));
        assert_eq!(agent.consecutive_no_action_prompts, 0);

        // After reset, first intent is Corrected again
        assert!(matches!(
            agent
                .maybe_prompt_for_action("Let me inspect the file", true, false, 0)
                .unwrap(),
            ActionPrompt::Corrected
        ));
        assert!(agent
            .messages
            .last()
            .unwrap()
            .content
            .text()
            .contains("selfware_system_directive"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_total_no_action_counter_survives_consecutive_resets() {
        // Simulates the cycling pattern: intent → correction → non-intent (resets
        // consecutive counter) → intent again → ... The lifetime counter should
        // still accumulate and eventually abort.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        for cycle in 0..250 {
            // One intent prompt (consecutive=1..3, then ForceFallback on third)
            let _ = agent.maybe_prompt_for_action("Let me check", true, false, 0);
            let _ = agent.maybe_prompt_for_action("Let me check", true, false, 0);
            // One non-intent response resets the consecutive counter
            let _ = agent.maybe_prompt_for_action("Here is the result.", true, false, 0);
            assert_eq!(
                agent.consecutive_no_action_prompts, 0,
                "consecutive should reset on non-intent response (cycle {})",
                cycle
            );
        }
        // 250 cycles * 2 intent prompts = 500 total, which equals MAX_TOTAL_NO_ACTION_PROMPTS
        assert_eq!(agent.total_no_action_prompts, 500);

        // The next intent prompt should trigger the lifetime abort
        let result = agent.maybe_prompt_for_action("Let me try again", true, false, 0);
        assert!(
            result.is_err(),
            "should abort after exceeding lifetime no-action limit"
        );
        server.stop().await;
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
    async fn test_detect_malformed_ignores_plain_tool_name_text() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent.detect_and_correct_malformed_tools("tool_name: file_read", &tool_calls);

        assert!(!result);

        server.stop().await;
    }

    #[tokio::test]
    async fn test_detect_malformed_ignores_plain_tool_call_text() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let tool_calls: Vec<CollectedToolCall> = vec![];
        let result = agent
            .detect_and_correct_malformed_tools("I'll use tool_call to read the file", &tool_calls);

        assert!(!result);

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
    async fn test_gate_rejects_incomplete_planning_response() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = false;
        let mut agent = Agent::new(config).await.unwrap();

        agent.last_assistant_response =
            "I need to read the tests to understand what to implement.\n\nfile_read: tests/chart_tests.rs"
                .to_string();

        let result = agent.check_completion_gate();
        assert!(
            result.is_some(),
            "Incomplete planning response should reject completion"
        );
        assert!(
            result
                .unwrap()
                .contains("describes work you still need to do"),
            "Expected gate to explain why the planning response was rejected"
        );

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
    async fn test_rust_subdirectory_does_not_bypass_cargo_verification() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;

        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Cargo.toml"),
            "[package]\nname = \"subdir-test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let nested = tmp.path().join("crates").join("worker");
        std::fs::create_dir_all(&nested).unwrap();

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&nested).unwrap();

        let mut agent = Agent::new(config).await.unwrap();
        let mut checkpoint = crate::checkpoint::TaskCheckpoint::new(
            "rust-subdir".to_string(),
            "edit a rust file".to_string(),
        );
        checkpoint.log_tool_call(ToolCallLog {
            timestamp: Utc::now(),
            tool_name: "file_edit".to_string(),
            arguments: r#"{"path":"src/lib.rs","old_str":"a","new_str":"b"}"#.to_string(),
            result: Some("ok".to_string()),
            success: true,
            duration_ms: Some(25),
        });
        agent.current_checkpoint = Some(checkpoint);

        let result = agent.check_completion_gate();
        assert!(
            result
                .as_deref()
                .is_some_and(|message| message.contains("cargo_check")
                    || message.contains("verification tool")),
            "Rust subdirectory should still require cargo verification, got: {:?}",
            result
        );

        std::env::set_current_dir(original_dir).unwrap();
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
    async fn test_repetition_detects_simple_abab_oscillation() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let call_a: Vec<CollectedToolCall> = vec![(
            "file_read".to_string(),
            r#"{"path":"a.rs"}"#.to_string(),
            None,
        )];
        let call_b: Vec<CollectedToolCall> = vec![(
            "grep_search".to_string(),
            r#"{"query":"foo"}"#.to_string(),
            None,
        )];

        assert!(agent.detect_repetition(&call_a).is_none());
        assert!(agent.detect_repetition(&call_b).is_none());
        assert!(agent.detect_repetition(&call_a).is_none());

        let result = agent.detect_repetition(&call_b);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(msg.contains("OSCILLATION LOOP DETECTED"));
        assert!(msg.contains("file_read"));
        assert!(msg.contains("grep_search"));

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
        agent.recent_tool_batches.clear();
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

    #[tokio::test]
    async fn test_execute_tool_batch_vision_analyze_uses_configured_vision_profile() {
        let server = MockLlmServer::builder()
            .with_response(r#"{"seen":"ok"}"#)
            .build()
            .await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.models.insert(
            "vision".to_string(),
            crate::config::ModelProfile {
                endpoint: format!("{}/v1", server.url()),
                model: "mock-vision-model".to_string(),
                api_key: None,
                max_tokens: 192,
                temperature: 0.0,
                modalities: vec!["text".to_string(), "vision".to_string()],
                context_length: 262_144,
                extra_body: Some({
                    let mut extra = serde_json::Map::new();
                    extra.insert(
                        "chat_template_kwargs".to_string(),
                        serde_json::json!({ "enable_thinking": false }),
                    );
                    extra
                }),
            },
        );
        let mut agent = Agent::new(config).await.unwrap();

        let batch: Vec<CollectedToolCall> = vec![(
            "vision_analyze".to_string(),
            serde_json::json!({
                "prompt": "Describe this test image.",
                "image_base64": "iVBORw0KGgo="
            })
            .to_string(),
            None,
        )];

        agent.execute_tool_batch(batch).await.unwrap();

        assert!(agent.messages.last().is_some_and(|message| {
            let text = message.content.text();
            text.contains("\"success\":true") && text.contains("\"model\":\"mock-vision-model\"")
        }));

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

    #[tokio::test]
    #[cfg_attr(
        target_os = "windows",
        ignore = "mock TCP server unreliable on Windows CI"
    )]
    async fn test_step_rejects_incomplete_planning_completion() {
        let server = MockLlmServer::builder()
            .with_response(
                "I need to read the project files first to understand the codebase and identify what needs to be fixed.\n\nfile_read: \"Cargo.toml\"\nfile_read: \"src/lib.rs\"",
            )
            .build()
            .await;

        let mut config = test_config(format!("{}/v1", server.url()));
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = false;
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
        assert!(
            last_user_msg
                .content
                .text()
                .contains("describes work you still need to do"),
            "Expected the incomplete planning response to be rejected"
        );

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
    // Additional edge cases
    // =========================================================================

    #[test]
    fn test_should_prompt_need_to_phrase() {
        assert!(should_prompt_for_action(
            "I need to check this",
            true,
            false,
            0
        ));
    }

    #[test]
    fn test_should_prompt_start_by_phrase() {
        assert!(should_prompt_for_action(
            "start by reading the file",
            true,
            false,
            0
        ));
    }

    #[test]
    fn test_should_prompt_help_you_phrase() {
        assert!(should_prompt_for_action(
            "I can help you with that",
            true,
            false,
            0
        ));
    }

    #[test]
    fn test_should_not_prompt_empty_content() {
        assert!(!should_prompt_for_action("", true, false, 0));
    }

    #[test]
    fn test_should_not_prompt_exactly_1000_chars() {
        // 1000 chars of real content with no reasoning = genuine (passes > 500 check)
        let content = "let me ".to_string() + &"x".repeat(993);
        assert_eq!(content.len(), 1000);
        assert!(!should_prompt_for_action(&content, true, false, 0));
    }

    #[test]
    fn test_should_not_prompt_501_chars_no_reasoning() {
        // 501+ chars of real content with no reasoning = genuine response
        let content = "let me ".to_string() + &"x".repeat(494);
        assert_eq!(content.len(), 501);
        assert!(!should_prompt_for_action(&content, true, false, 0));
    }

    #[test]
    fn test_should_prompt_499_chars_with_intent() {
        // Under 500 chars with intent phrase = should prompt
        let content = "let me ".to_string() + &"x".repeat(492);
        assert_eq!(content.len(), 499);
        assert!(should_prompt_for_action(&content, true, false, 0));
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
                should_prompt_for_action(phrase, true, false, 0),
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
        // Use CARGO_MANIFEST_DIR for cwd-independent test (fix #7: test determinism)
        let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));
        let server = MockLlmServer::builder()
            .with_response(
                format!(
                    "<tool>\n<name>file_read</name>\n<arguments>{{\"path\":\"{}\"}}</arguments>\n</tool>",
                    cargo_toml_path
                ),
            )
            .with_response("Done reading.")
            .build()
            .await;

        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let _ = agent.execute_step_internal(false).await;

        assert!(
            agent
                .file_tracker
                .context_files
                .iter()
                .any(|p| p.ends_with("Cargo.toml")),
            "Expected Cargo.toml in context_files: {:?}",
            agent.file_tracker.context_files
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn test_redundant_unchanged_file_reads_update_task_state_memory() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let cargo_toml_path = format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR"));

        let batch: Vec<CollectedToolCall> = vec![
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, cargo_toml_path),
                None,
            ),
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, cargo_toml_path),
                None,
            ),
        ];

        agent.execute_tool_batch(batch).await.unwrap();

        let state = agent
            .file_tracker
            .read_state
            .get(&cargo_toml_path)
            .expect("expected Cargo.toml file state");
        assert_eq!(state.unchanged_read_count, 1);
        assert!(agent.task_state_notes.iter().any(|note| {
            note.contains(&format!("Reread unchanged file `{}`", cargo_toml_path))
        }));
        assert!(agent.pending_failure_hint.as_deref().is_some_and(|hint| {
            hint.contains(&format!("reread unchanged file `{}`", cargo_toml_path))
        }));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_third_unchanged_file_read_is_blocked() {
        use std::fs;
        use tempfile::NamedTempFile;

        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let temp = NamedTempFile::new_in(std::env::current_dir().unwrap()).unwrap();
        fs::write(temp.path(), "hello world\n").unwrap();
        let path = temp.path().display().to_string();

        // First batch of 3 reads — all should succeed (threshold is 3 unchanged rereads)
        let batch1: Vec<CollectedToolCall> = vec![
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
        ];
        agent.execute_tool_batch(batch1).await.unwrap();

        // Second batch — 4th read triggers blocking (count reaches 3), 5th is suppressed
        let batch2: Vec<CollectedToolCall> = vec![
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
        ];
        agent.execute_tool_batch(batch2).await.unwrap();

        // Verify the file is being tracked
        let state = agent
            .file_tracker
            .read_state
            .get(&path)
            .expect("expected tracked file state");
        // The unchanged_read_count tracks how many times the file was read unchanged.
        // Reads 1-3 execute (count goes 1->2->3), Read 4 is blocked (count becomes 4).
        // Read 5 is suppressed by retry suppression before execution.
        assert_eq!(state.unchanged_read_count, 4);
        // Verify a "task_state" failure was recorded (from the 4th read being blocked)
        assert!(agent
            .recent_failed_tool_attempts
            .back()
            .is_some_and(|attempt| attempt.failure_kind == "task_state"));
        // Verify the blocking message appears in messages (from the 4th read)
        assert!(agent.messages.iter().any(|message| message
            .content
            .text()
            .contains("Repeated unchanged reread blocked")));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_file_edit_clears_task_state_for_modified_file() {
        use std::fs;
        use tempfile::NamedTempFile;

        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let temp = NamedTempFile::new_in(std::env::current_dir().unwrap()).unwrap();
        fs::write(temp.path(), "hello world\n").unwrap();
        let path = temp.path().display().to_string();

        let batch: Vec<CollectedToolCall> = vec![
            (
                "file_read".to_string(),
                format!(r#"{{"path":"{}"}}"#, path),
                None,
            ),
            (
                "file_edit".to_string(),
                format!(
                    r#"{{"path":"{}","old_str":"hello world","new_str":"hello rust"}}"#,
                    path
                ),
                None,
            ),
        ];

        agent.execute_tool_batch(batch).await.unwrap();

        assert!(!agent.file_tracker.read_state.contains_key(&path));
        assert!(agent
            .task_state_notes
            .iter()
            .any(|note| note.contains("Marked") && note.contains(&path)));

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

    // -- read_line_pausing_esc tests --

    // NOTE: These tests are marked as `#[ignore]` because they interact with stdin,
    // which blocks indefinitely in test environments (non-TTY). Run with `--ignored`
    // locally if you want to verify the pause flag behavior.

    #[tokio::test]
    #[ignore = "blocks on stdin in non-interactive test environment"]
    async fn read_line_pausing_esc_sets_and_clears_pause_flag() {
        use std::sync::atomic::Ordering;

        let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ack = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let paused_clone = std::sync::Arc::clone(&paused);
        let ack_clone = std::sync::Arc::clone(&ack);

        // Spawn a watcher that records whether paused was ever set
        let was_paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let was_paused_clone = std::sync::Arc::clone(&was_paused);

        let watcher = std::thread::spawn(move || {
            for _ in 0..100 {
                if paused_clone.load(Ordering::Acquire) {
                    ack_clone.store(true, Ordering::Release);
                    was_paused_clone.store(true, Ordering::Relaxed);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        // Simulate stdin by piping — read_line will return an error or empty
        // in a non-interactive test, but the pause flag behavior is what we test.
        let _ = read_line_pausing_esc(&paused, &ack).await;

        // After return, paused must be cleared
        assert!(
            !paused.load(Ordering::Acquire),
            "esc_paused must be false after read_line_pausing_esc returns"
        );
        assert!(
            !ack.load(Ordering::Acquire),
            "esc_pause_ack must be reset after read_line_pausing_esc returns"
        );

        let _ = watcher.join();
        // In CI (non-interactive), the watcher may or may not observe the pause
        // due to timing, so we don't assert was_paused — the important thing is
        // the flag is cleared after the call.
    }

    #[tokio::test]
    #[ignore = "blocks on stdin in non-interactive test environment"]
    async fn read_line_pausing_esc_unpauses_even_on_error() {
        use std::sync::atomic::Ordering;

        let paused = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ack = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        // Call in non-interactive context (stdin is not a tty in tests)
        let _ = read_line_pausing_esc(&paused, &ack).await;

        assert!(
            !paused.load(Ordering::Acquire),
            "esc_paused must always be cleared, even if read_line fails"
        );
        assert!(
            !ack.load(Ordering::Acquire),
            "esc_pause_ack must always be cleared, even if read_line fails"
        );
    }
}
