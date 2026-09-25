use std::hash::{Hash, Hasher};

use tracing::{debug, info, warn};

use super::*;
use crate::api::types::Message;
use crate::testing::visual_verification::{LoopDetectionResult, RecoveryStrategy};

/// Maximum consecutive no-action prompts before aborting.
/// Set high to support long-running agentic loops with local models that
/// occasionally produce text-only responses between tool calls.
pub(super) const MAX_NO_ACTION_PROMPTS: usize = 20;
/// After this many text-only reprompts, force a deterministic fallback tool call
/// instead of sending another text correction the model will ignore.
pub(super) const FORCE_FALLBACK_AFTER: usize = 3;
/// Absolute lifetime cap on total no-action prompts across the entire task.
/// Set very high to support multi-hour/multi-day agentic sessions.
/// Smart fallbacks make real progress (reading files, exploring code) even
/// though the model itself isn't producing tool calls directly.
pub(super) const MAX_TOTAL_NO_ACTION_PROMPTS: usize = 500;
/// Consecutive empty assistant responses allowed before the empty-response
/// breaker aborts the run. Shared by the execution and planning paths so the
/// two sides of the recovery story cannot drift apart.
pub(super) const MAX_CONSECUTIVE_EMPTY_RESPONSES: usize = 2;

/// Terminal `EMPTY_RESPONSE_LOOP` message, shared by the execution and
/// planning paths. Names what the LAST response actually carried: a
/// reasoning-only response (the model thought but never answered or called a
/// tool) is a different diagnosis from a truly empty one (nothing at all —
/// usually a parser / chat-template problem), and claiming "no reasoning"
/// when reasoning arrived sends the operator after the wrong fault.
pub(super) fn empty_response_loop_message(count: usize, last_reasoning_chars: usize) -> String {
    let shape = if last_reasoning_chars > 0 {
        format!(
            "the provider returned no deliverable content and no tool calls each time \
             (last response was reasoning-only: {last_reasoning_chars} reasoning chars, \
             no answer). The model is thinking without answering: check reasoning \
             effort / max_tokens and the endpoint's reasoning parser."
        )
    } else {
        "the provider returned no content, no reasoning and no tool calls each time. \
         Check the endpoint's parser / chat-template configuration."
            .to_string()
    };
    format!(
        "EMPTY_RESPONSE_LOOP: {count} consecutive empty assistant responses — {shape} \
         The retry already went out non-streaming, so this is not a streaming artifact."
    )
}

/// The nudge injected after an empty execution response — worded for what the
/// response actually was, so the model is not told it produced "reasoning only"
/// when it produced nothing.
pub(super) fn empty_response_nudge(reasoning_chars: usize) -> &'static str {
    if reasoning_chars > 0 {
        "<selfware_system_directive>\n\
         Your last response produced no deliverable content or tool calls (reasoning only). \
         Provide your actual final answer now (a concise summary of the completed work) \
         or call a tool.\n\
         </selfware_system_directive>"
    } else {
        "<selfware_system_directive>\n\
         Your last response was empty (no content, no reasoning, no tool calls). \
         Provide your actual final answer now (a concise summary of the completed work) \
         or call a tool.\n\
         </selfware_system_directive>"
    }
}
pub(super) const FILE_DISCOVERY_TOOLS: &str = "directory_tree, glob_find, or grep_search";

/// Result of the intent-without-action check.
pub(super) enum ActionPrompt {
    /// The model produced tool calls or non-intent content — no correction needed.
    NotNeeded,
    /// A text correction was injected; the caller should re-prompt the LLM.
    Corrected,
    /// Text corrections failed repeatedly; the caller should force-execute a
    /// safe discovery tool (e.g. `directory_tree .`) instead of re-prompting.
    ForceFallback,
}

pub(super) fn normalize_no_action_content(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(super) fn hash_text_signature(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Whether tool-less content holds a tool-call ATTEMPT that no parser
/// accepted. Markdown code (inline spans and fences, as the parser reads
/// them via `crate::tool_parser::outside_markdown_code`) is quoted text: a
/// final answer that explains the `<tool>` syntax in backticks is not a
/// malformed call, exactly as the parser never executes or rejects it.
pub(super) fn looks_like_malformed_tool_xml(content: &str) -> bool {
    let prose = crate::tool_parser::outside_markdown_code(content);
    let trimmed = prose.trim();
    if trimmed.is_empty() {
        return false;
    }

    if trimmed.contains("<function=")
        || trimmed.contains("<name=")
        || trimmed.contains("<parameter=")
        || trimmed.contains("<|open|>tools")
        || trimmed.contains("<|open|>call")
        || trimmed.contains("<tool_call>")
        || trimmed.contains("</tool_call>")
    {
        return true;
    }

    let has_tool_tag = trimmed.contains("<tool");
    let has_tool_close = trimmed.contains("</tool>");
    let has_arguments_tag = trimmed.contains("<arguments>") || trimmed.contains("</arguments>");

    if !has_tool_tag && has_arguments_tag {
        return true;
    }

    if has_tool_tag {
        if !has_tool_close {
            return true;
        }
        let Some(name_start) = trimmed.find("<name>") else {
            return true;
        };
        let after_name = &trimmed[name_start + "<name>".len()..];
        let Some(name_end) = after_name.find("</name>") else {
            return true;
        };
        let tool_name = after_name[..name_end].trim();
        if tool_name.is_empty() {
            return true;
        }

        let Some(args_start) = trimmed.find("<arguments>") else {
            return true;
        };
        let after_args = &trimmed[args_start + "<arguments>".len()..];
        let Some(args_end) = after_args.find("</arguments>") else {
            return true;
        };
        let args_str = after_args[..args_end].trim();
        if serde_json::from_str::<serde_json::Value>(args_str).is_err() {
            return true;
        }

        return false;
    }

    false
}

pub(super) fn detect_oscillating_batch_pair(
    recent_tool_batches: &std::collections::VecDeque<Vec<(String, u64)>>,
) -> Option<((String, u64), (String, u64))> {
    if recent_tool_batches.len() < 4 {
        return None;
    }

    let window: Vec<_> = recent_tool_batches.iter().rev().take(4).collect();
    if window.iter().any(|batch| batch.len() != 1) {
        return None;
    }

    let latest = &window[0][0];
    let previous = &window[1][0];
    let older = &window[2][0];
    let oldest = &window[3][0];

    if latest == older && previous == oldest && latest != previous {
        Some((oldest.clone(), older.clone()))
    } else {
        None
    }
}

/// Reasoning block delimiters a model may emit inline in `content`:
/// Qwen/DeepSeek `<think>…</think>` and gemma `<|channel>thought…<channel|>`.
const REASONING_BLOCKS: &[(&str, &str)] = &[("<think>", "</think>"), ("<|channel>", "<channel|>")];

/// The reasoning block whose opening marker starts exactly at `text`.
fn reasoning_block_at(text: &str) -> Option<(&'static str, &'static str)> {
    REASONING_BLOCKS
        .iter()
        .copied()
        .find(|(open, _)| text.starts_with(open))
}

/// Strip reasoning blocks (`<think>…</think>`, gemma
/// `<|channel>thought…<channel|>`) from content, removing only blocks the
/// model actually opened as reasoning:
///
/// - **Leading blocks** (at the start of the content, after whitespace) are
///   removed. A leading gemma block that never closes is all reasoning (the
///   answer never started), so nothing is left; a leading unclosed `<think>`
///   loses only its marker (Qwen3.5 emits the answer after it).
/// - **After the answer has begun**, only a properly closed pair outside
///   markdown code is removed. Markers inside backticks, code spans or fenced
///   code are quoted text, and an unclosed marker is literal text: neither is
///   ever stripped, and nothing after them is ever dropped. (D10: a review
///   that quoted `` `<|channel>` `` was cut from 9,284 to 731 chars.)
/// - An unmatched closing tag with no opener (Qwen3.5 thinking as plain text,
///   then `</think>`, then the answer) is left untouched.
pub(super) fn strip_think_blocks(content: &str) -> String {
    // Leading reasoning blocks.
    let mut rest = content;
    loop {
        let trimmed = rest.trim_start();
        let Some((open, close)) = reasoning_block_at(trimmed) else {
            break;
        };
        let body = &trimmed[open.len()..];
        match body.find(close) {
            Some(end) => rest = &body[end + close.len()..],
            None if open == "<|channel>" => return String::new(),
            None => {
                rest = body;
                break;
            }
        }
    }

    // Answer text: closed pairs outside markdown code only.
    let code = crate::tool_parser::markdown_code_spans(rest);
    let in_code = |pos: usize| code.iter().any(|span| span.contains(&pos));
    let mut result = String::with_capacity(rest.len());
    let mut cursor = 0;
    let mut search = 0;
    while let Some((pos, open, close)) = REASONING_BLOCKS
        .iter()
        .filter_map(|&(open, close)| {
            let mut from = search;
            while let Some(i) = rest[from..].find(open) {
                let pos = from + i;
                if !in_code(pos) {
                    return Some((pos, open, close));
                }
                from = pos + open.len();
            }
            None
        })
        .min_by_key(|(pos, _, _)| *pos)
    {
        let body_start = pos + open.len();
        let mut from = body_start;
        let mut close_end = None;
        while let Some(i) = rest[from..].find(close) {
            let at = from + i;
            if !in_code(at) {
                close_end = Some(at + close.len());
                break;
            }
            from = at + close.len();
        }
        match close_end {
            Some(end) => {
                result.push_str(&rest[cursor..pos]);
                cursor = end;
                search = end;
            }
            // Unclosed marker inside the answer: literal text, keep it all.
            None => search = body_start,
        }
    }
    result.push_str(&rest[cursor..]);
    result.trim().to_string()
}

impl Agent {
    pub(super) fn reset_no_action_prompt_state(&mut self) {
        self.consecutive_no_action_prompts = 0;
        self.readonly_no_tool_streak = 0;
        self.last_no_action_prompt_hash = None;
    }

    pub(super) fn missing_required_task_tools(&self) -> Vec<String> {
        let attempted_tools: std::collections::BTreeSet<String> = self
            .current_checkpoint
            .as_ref()
            .map(|cp| {
                cp.tool_calls
                    .iter()
                    .map(|tc| tc.tool_name.clone())
                    .collect()
            })
            .unwrap_or_default();

        self.required_task_tools
            .iter()
            .filter(|tool| !attempted_tools.contains(*tool))
            .cloned()
            .collect()
    }

    /// Track a screenshot hash and detect visual stuck loops.
    ///
    /// Maintains a sliding window of the last 10 screenshot hashes.
    /// Returns `true` if the same hash appears 3+ times in the last 5
    /// screenshots, meaning the screen has not changed despite actions.
    pub(super) fn detect_visual_stuck_loop(&mut self, screenshot_hash: u64) -> bool {
        const MAX_HASHES: usize = 10;
        const WINDOW: usize = 5;
        const THRESHOLD: usize = 3;

        self.recent_screenshot_hashes.push_back(screenshot_hash);
        if self.recent_screenshot_hashes.len() > MAX_HASHES {
            self.recent_screenshot_hashes.pop_front();
        }

        // Check the last WINDOW entries for repeated hashes
        let len = self.recent_screenshot_hashes.len();
        let start = len.saturating_sub(WINDOW);
        let window: Vec<u64> = self
            .recent_screenshot_hashes
            .iter()
            .skip(start)
            .copied()
            .collect();

        let count = window.iter().filter(|&&h| h == screenshot_hash).count();
        let stuck = count >= THRESHOLD;
        self.visual_stuck_loop_active = stuck;

        if stuck {
            warn!(
                "Visual stuck loop detected: screenshot hash {} appeared {} times in last {} captures",
                screenshot_hash, count, window.len()
            );
            self.cognitive_state.episodic_memory.what_failed(
                "visual_stuck_loop",
                &format!(
                    "Screen unchanged after {} actions — same visual state repeated {} times",
                    window.len(),
                    count
                ),
            );
        }

        stuck
    }

    /// Advanced visual stuck-loop detection using VisualStateTracker.
    ///
    /// This provides more sophisticated detection with perceptual hashing,
    /// semantic state tracking, and recovery strategy suggestions.
    ///
    /// Returns true if a stuck loop is detected and recorded.
    pub(super) fn detect_visual_stuck_loop_advanced(
        &mut self,
        screenshot_hash: &str,
        action: &str,
        action_succeeded: bool,
    ) -> Option<RecoveryStrategy> {
        let result = self.visual_state_tracker.record_state_with_hash(
            screenshot_hash.to_string(),
            String::new(),
            action.to_string(),
            action_succeeded,
        );

        match result {
            LoopDetectionResult::Stuck {
                loop_pattern,
                suggested_recovery,
            } => {
                let count = loop_pattern.len();
                warn!(
                    "Advanced visual stuck loop detected: {} similar states for action '{}'",
                    count, action
                );
                self.visual_stuck_loop_active = true;
                self.cognitive_state.episodic_memory.what_failed(
                    "visual_stuck_loop_advanced",
                    &format!(
                        "Visual stuck loop: same screen repeated {} times for action '{}'",
                        count, action
                    ),
                );
                Some(suggested_recovery)
            }
            LoopDetectionResult::Warning { similar_states } => {
                debug!(
                    "Visual loop warning: {} similar states detected",
                    similar_states.len()
                );
                None
            }
            LoopDetectionResult::Proceed => None,
        }
    }

    /// Handle visual stuck loop by building appropriate recovery hint
    pub(super) fn handle_visual_stuck_loop(&self, recovery: &RecoveryStrategy) -> String {
        let base_message = format!(
            "VISUAL STUCK LOOP DETECTED: The screen has not changed after repeated attempts. Recovery Strategy: {}",
            recovery
        );

        let specific_guidance = match recovery {
            RecoveryStrategy::TryDifferentAction { alternatives } => {
                format!(
                    " Try one of these alternatives: {}",
                    alternatives.join("; ")
                )
            }
            RecoveryStrategy::WaitAndRetry { delay_ms } => {
                format!(
                    " Wait {}ms for any animations to complete before retrying.",
                    delay_ms
                )
            }
            RecoveryStrategy::ResetToCheckpoint => {
                " Consider resetting to a known good state.".to_string()
            }
            RecoveryStrategy::ReassessWithScreenshot => {
                " Take a fresh screenshot to reassess the current state.".to_string()
            }
            RecoveryStrategy::ChangeInputMethod { suggestion } => {
                format!(" Try changing input method: {}", suggestion)
            }
            RecoveryStrategy::EscalateToUser { reason } => {
                format!(" Escalation required: {}", reason)
            }
        };

        format!("{}{}", base_message, specific_guidance)
    }

    fn no_action_failure_context(&self) -> Option<String> {
        self.recent_failed_tool_attempts.back().map(|failure| {
            format!(
                " Most recent concrete failure: tool `{}` hit {} ({})",
                failure.tool_name, failure.failure_kind, failure.error_preview
            )
        })
    }

    /// True when the FILES:-checklist guard recently discarded a model write
    /// and the re-issue is still pending (no file written since). Recognized
    /// via the marker the discard directive embeds in the message tail, so the
    /// Agent struct carries no extra field for it. The scan is bounded: a
    /// discard older than a few turns was either resolved or the conversation
    /// moved on.
    pub(super) fn files_guard_reissue_pending(&self) -> bool {
        if self.has_written_any_file {
            return false;
        }
        self.messages.iter().rev().take(6).any(|m| {
            m.role == "user"
                && m.content
                    .text()
                    .contains(super::execution::FILES_GUARD_DISCARD_MARKER)
        })
    }

    fn build_no_action_prompt_message(&self) -> String {
        let missing_required_tools = self.missing_required_task_tools();
        if !missing_required_tools.is_empty() {
            let required_tool_list = missing_required_tools
                .iter()
                .map(|tool| format!("`{}`", tool))
                .collect::<Vec<_>>()
                .join(", ");
            return format!(
                "<selfware_system_directive>\nThis task explicitly requires {} before you answer.\nCall the required tool now.\nDo NOT answer from memory, filenames, or prior knowledge.\n</selfware_system_directive>",
                required_tool_list
            );
        }

        // After the FILES:-checklist guard discarded a write, the follow-up
        // guidance must name exactly what WILL be accepted — never a
        // read-only-only tool list (that contradiction burned a greenfield
        // run: the write was thrown away and the nudge pointed at reads).
        if self.files_guard_reissue_pending() {
            let guidance = if self.files_checklist_seen {
                "Your `FILES:` checklist is already recorded — the only thing missing is the \
                 edit itself. RE-ISSUE the `file_edit`/`file_write` tool call NOW; it will be \
                 accepted. Do NOT call read-only tools and do NOT answer in prose."
            } else {
                "Your previous edit was discarded because no `FILES:` line accompanied it. \
                 Do BOTH in ONE response now: (1) output a line `FILES: <path>` naming the \
                 file(s) to change, and (2) the `file_edit`/`file_write` tool call itself. \
                 That combination IS accepted and executed — no read-only tool call is \
                 needed first."
            };
            return format!(
                "<selfware_system_directive>\n{}\n</selfware_system_directive>",
                guidance
            );
        }

        let failure_context = self.no_action_failure_context().unwrap_or_default();
        let tool_options = super::NO_ACTION_TOOL_OPTIONS;

        let guidance = match self.consecutive_no_action_prompts {
            0 | 1 => format!(
                "Your response described what you plan to do but did not call a tool.\n\
                 Choose one of these and execute it:\n\
                 - {}\n\
                 Which one fits your current goal?",
                tool_options.replace(", ", "\n - ")
            ),
            2 => format!(
                "Attempt {}: Still no tool call. Here are concrete next steps:\n\
                 1. `directory_tree` with path \".\" — see project layout\n\
                 2. `glob_find` — locate specific files\n\
                 3. `grep_search` — find code patterns\n\
                 4. `file_read` — read a specific file\n\
                 5. `shell_exec` — run a command\n\
                 Pick the most relevant one.{}",
                self.consecutive_no_action_prompts, failure_context
            ),
            _ => format!(
                "Attempt {} of {}: No tool called yet.\n\
                 If you cannot proceed, provide a summary of what you found so far. \
                 Otherwise, call any tool to continue.{}",
                self.consecutive_no_action_prompts, MAX_NO_ACTION_PROMPTS, failure_context
            ),
        };

        format!(
            "<selfware_system_directive>\n{}\n</selfware_system_directive>",
            guidance
        )
    }

    /// Build the tool-specific recovery guidance for a failed tool call.
    /// Returns guidance text WITHOUT a header — the caller
    /// (`tool_dispatch::tool_error_feedback`) renders it inside the single
    /// `Recovery:` section of the unified error message, so a second header
    /// here would double the recovery blocks the model sees.
    pub(super) fn build_error_recovery_hint(&self, tool_name: &str, error: &str) -> String {
        let error_lower = error.to_lowercase();

        // Command / subprocess timeouts — tool-specific, NOT LLM infrastructure issues
        if matches!(
            tool_name,
            "shell_exec"
                | "pty_shell"
                | "cargo_test"
                | "cargo_check"
                | "cargo_clippy"
                | "cargo_fmt"
                | "npm_run"
                | "npm_install"
                | "pip_install"
                | "pip_list"
                | "pip_freeze"
                | "yarn_install"
        ) && error_lower.contains("timed out")
        {
            return format!(
                "Command in '{}' timed out. \
                 For long-running builds, tests, or installations, retry with a larger timeout \
                 (e.g., add '\"timeout_secs\": 600' to the arguments) or optimize the command.",
                tool_name
            );
        }

        // Endpoint / connection errors — self-healing
        if error_lower.contains("connection refused")
            || error_lower.contains("connection reset")
            || error_lower.contains("502 bad gateway")
            || error_lower.contains("503 service unavailable")
            || error_lower.contains("network unreachable")
            || (error_lower.contains("timed out")
                && (error_lower.contains("llm")
                    || error_lower.contains("endpoint")
                    || error_lower.contains("api")
                    || error_lower.contains("client")))
            || (error_lower.contains("endpoint") && !error_lower.contains("exit"))
        {
            warn!(
                "Endpoint error detected for tool '{}': {}",
                tool_name,
                super::interactive::safe_truncate(error, 200)
            );
            return format!(
                "Endpoint/connection issue detected for '{}'. \
                 The LLM backend may be temporarily unavailable or overloaded. \
                 This is NOT a code problem — it's an infrastructure issue. \
                 Continue working with tools that don't require the LLM endpoint \
                 (file_read, directory_tree, shell_exec, git_status). \
                 The connection may recover automatically on the next attempt.",
                tool_name
            );
        }

        // Rate limit / token budget errors
        if error_lower.contains("rate limit")
            || error_lower.contains("too many requests")
            || error_lower.contains("429")
            || error_lower.contains("quota exceeded")
        {
            warn!(
                "Rate limit detected for tool '{}': {}",
                tool_name,
                super::interactive::safe_truncate(error, 200)
            );
            return "Rate limit or quota exceeded. \
                 Wait a moment, then continue with smaller requests. \
                 Reduce the scope of your next action — read smaller files, \
                 make smaller edits, or use grep_search instead of reading entire files."
                .to_string();
        }

        // Regression detection — tests that were passing now fail
        if (error_lower.contains("test") || error_lower.contains("assert"))
            && (error_lower.contains("fail") || error_lower.contains("error"))
            && tool_name.contains("test")
        {
            warn!("Possible regression detected in test output");
            return "Tests are failing after your change. This may be a regression. \
                 Steps to fix:\
                 1. Read the test error output carefully\
                 2. Use git_diff to review your changes\
                 3. If your edit introduced the failure, use file_edit to fix it\
                 4. Run the tests again to verify\
                 Do NOT add more tests — fix the source code that caused the regression."
                .to_string();
        }

        // File not found errors - suggest alternatives
        if error_lower.contains("file not found") || error_lower.contains("no such file") {
            return format!(
                "The tool '{}' failed because the file was not found. \
Try ONE of these alternatives:\
1. Use directory_tree to explore the directory structure first\
2. Use glob_find to find the correct file path\
3. Use grep_search to locate the content in other files\
4. Create the file if it should exist\
\nDO NOT attempt the same file path again. Choose a different approach now.",
                tool_name
            );
        }

        // Permission errors
        if error_lower.contains("permission denied") || error_lower.contains("access denied") {
            return format!(
                "The tool '{}' failed due to permission issues. \
Try ONE of these alternatives:\
1. Use glob_find or grep_search instead of reading protected files\
2. Check available files with directory_tree\
3. Work with files in the current project directory instead\
\nChoose a different approach now.",
                tool_name
            );
        }

        // Path traversal / safety errors
        if error_lower.contains("path traversal") || error_lower.contains("safety check") {
            return format!(
                "The tool '{}' failed because the path is outside allowed directories. \
Try ONE of these alternatives:\
1. Use a relative path within the project directory\
2. Use directory_tree to see available files\
3. Work with files in the current directory (./)\
\nChoose a different approach with a valid path now.",
                tool_name
            );
        }

        // JSON / argument errors
        if error_lower.contains("json")
            || error_lower.contains("argument")
            || error_lower.contains("parameter")
        {
            return format!(
                "The tool '{}' failed due to invalid arguments. \
Try ONE of these alternatives:\
1. Use a different tool that doesn't require complex arguments\
2. Check the tool schema and try with simpler, valid arguments\
3. Use file_read to examine examples of correct usage\
\nChoose a different approach now.",
                tool_name
            );
        }

        // Generic error recovery hint
        format!(
            "The tool '{}' failed. \
You MUST try a DIFFERENT tool or approach - do not retry the same tool with the same arguments. \
Try ONE of these strategies:\
1. Use a different tool to achieve the same goal\
2. Gather more information first ({})\
3. Break the task into smaller steps with different tools\
4. If stuck, provide a final answer explaining what you learned\n\nTake action with a different tool NOW.",
            tool_name, FILE_DISCOVERY_TOOLS
        )
    }

    /// Check whether the model described intent without calling a tool, and
    /// either inject a text correction or force a deterministic fallback.
    ///
    /// Returns:
    /// - `Ok(ActionPrompt::NotNeeded)` — content looks fine, proceed normally
    /// - `Ok(ActionPrompt::Corrected)` — text correction injected, retry LLM
    /// - `Ok(ActionPrompt::ForceFallback)` — deterministic tool call injected,
    ///   execute it instead of re-prompting (the model can't/won't comply)
    /// - `Err(msg)` — exceeded MAX_NO_ACTION_PROMPTS, task must abort
    pub(super) fn maybe_prompt_for_action(
        &mut self,
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
        reasoning_chars: usize,
    ) -> Result<ActionPrompt, String> {
        if !self.should_prompt_for_action(
            content,
            has_no_tool_calls,
            use_last_message,
            reasoning_chars,
        ) {
            self.reset_no_action_prompt_state();
            return Ok(ActionPrompt::NotNeeded);
        }

        let normalized = normalize_no_action_content(content);
        let signature = hash_text_signature(&normalized);

        debug!(
            "Intent-without-action detection: normalized='{}', signature={}, current_count={}",
            normalized.chars().take(100).collect::<String>(),
            signature,
            self.consecutive_no_action_prompts
        );

        if self.last_no_action_prompt_hash == Some(signature) {
            self.consecutive_no_action_prompts += 1;
            debug!(
                "Same intent pattern detected, incrementing counter to {}",
                self.consecutive_no_action_prompts
            );
        } else if self.consecutive_no_action_prompts >= FORCE_FALLBACK_AFTER {
            // After a forced fallback the model sees new context (e.g. directory
            // listing) and produces different text. Don't reset the counter —
            // the model already proved it can't use tools. Keep incrementing
            // toward the abort threshold.
            self.consecutive_no_action_prompts += 1;
            self.last_no_action_prompt_hash = Some(signature);
            debug!(
                "Post-fallback new pattern, keeping high counter at {}",
                self.consecutive_no_action_prompts
            );
        } else {
            self.consecutive_no_action_prompts = 1;
            self.last_no_action_prompt_hash = Some(signature);
            debug!("New intent pattern detected, starting counter at 1");
        }

        // Track lifetime total (never reset, survives across consecutive resets)
        self.total_no_action_prompts += 1;

        // Exceeded max prompts — abort (check both consecutive AND lifetime)
        if self.consecutive_no_action_prompts > MAX_NO_ACTION_PROMPTS
            || self.total_no_action_prompts > MAX_TOTAL_NO_ACTION_PROMPTS
        {
            let error_msg = format!(
                "Agent failed to take action after {} consecutive / {} total attempts. \
                 The model kept describing intent without using tools. Task aborted.",
                self.consecutive_no_action_prompts, self.total_no_action_prompts
            );
            tracing::error!("{}", error_msg);
            debug!(
                "Intent-without-action loop content (attempt {}): {}",
                self.consecutive_no_action_prompts,
                content.chars().take(500).collect::<String>()
            );
            return Err(error_msg);
        }

        // After FORCE_FALLBACK_AFTER text corrections the model still isn't
        // calling tools — force a deterministic safe action instead of hoping
        // yet another text prompt will work.
        if self.consecutive_no_action_prompts >= FORCE_FALLBACK_AFTER {
            info!(
                "Forcing deterministic fallback tool after {} failed text prompts",
                self.consecutive_no_action_prompts
            );
            crate::output::intent_without_action_detail(
                content,
                "→ Forcing automatic tool execution",
                self.consecutive_no_action_prompts,
                MAX_TOTAL_NO_ACTION_PROMPTS,
            );
            return Ok(ActionPrompt::ForceFallback);
        }

        let correction = self.build_no_action_prompt_message();
        info!(
            "Detected intent without action, prompting model to use tools (count={})",
            self.consecutive_no_action_prompts
        );
        crate::output::intent_without_action_detail(
            content,
            &correction,
            self.consecutive_no_action_prompts,
            MAX_TOTAL_NO_ACTION_PROMPTS,
        );
        self.messages.push(Message::user(correction));
        Ok(ActionPrompt::Corrected)
    }

    pub(super) fn should_prompt_for_action(
        &self,
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
        reasoning_chars: usize,
    ) -> bool {
        if !has_no_tool_calls || use_last_message {
            return false;
        }

        if !self.missing_required_task_tools().is_empty() {
            return true;
        }

        // Strip any residual think blocks from content to measure real output.
        let effective_content = strip_think_blocks(content);
        let effective_len = effective_content.len();

        // SWE/coding tasks that require mutation must not burn dozens of turns
        // on long prose. If no mutating tool has succeeded yet, require a
        // concrete action unless the response contains extractable code that
        // the execution layer can auto-write immediately after this check.
        if self.current_task_requires_mutation()
            && self.mutating_tool_call_count() == 0
            && !super::execution::contains_unwritten_code(&effective_content)
        {
            return true;
        }

        // If the model produced substantial non-think content, treat as real output.
        // Use a relative threshold: if think blocks dominate (>80% of total output),
        // the "real" content is likely just leaked intent.
        let total_output = effective_len + reasoning_chars;
        if total_output > 0 && effective_len > 500 {
            let think_ratio = reasoning_chars as f64 / total_output as f64;
            if think_ratio < 0.8 {
                return false; // Genuine long response
            }
            // High think ratio with short content — likely confused, keep checking
        } else if effective_len >= 1000 {
            return false; // Long content with no think blocks — genuine
        }

        // DETECTION: Only flag short content that contains clear intent phrases
        // Content over 300 chars is likely a genuine response
        if effective_len >= 300 {
            return false;
        }

        // For shorter content, check for intent patterns
        let lower = effective_content.to_lowercase();
        let intent_phrases = ["let me", "i'll ", "i will", "let's ", "going to"];

        intent_phrases.iter().any(|p| lower.contains(p))
    }

    /// Detect malformed tool call attempts and push a correction message.
    /// Returns `true` if malformed markers were found and a correction was injected.
    pub(super) fn detect_and_correct_malformed_tools(
        &mut self,
        content: &str,
        tool_calls: &[super::execution::CollectedToolCall],
    ) -> bool {
        if !tool_calls.is_empty() {
            return false;
        }

        if !looks_like_malformed_tool_xml(content) {
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

    /// Track tool calls and detect repetition loops.
    /// Returns `Some(message)` if the same call has been made too many times
    /// recently against the same workspace revision.
    ///
    /// A loop is the same call on the same state. Observing and verifying
    /// calls (reads, searches, type-checks, tests) are fingerprinted together
    /// with the revision they observe (`repetition_guard_revision`), so
    /// re-running the same check after an edit is new work, not a repeat.
    /// Without that, the required post-edit re-verification was blocked here
    /// and the completion gate then refused the stale pass (val083 ts run,
    /// turns 6/8/11/14/15). Mutating calls keep the plain fingerprint: an edit
    /// re-applied with the same arguments advances the revision each time but
    /// is still a loop.
    pub(super) fn detect_repetition(
        &mut self,
        tool_calls: &[super::execution::CollectedToolCall],
    ) -> Option<String> {
        const MAX_REPEATS: usize = 3;
        const WINDOW_SIZE: usize = 10;

        let revision = self.repetition_guard_revision;
        let batch_signatures: Vec<_> = tool_calls
            .iter()
            .map(|(name, args_str, _)| repetition_signature(name, args_str, revision))
            .collect();

        for sig in &batch_signatures {
            self.recent_tool_calls.push_back(sig.clone());
            if self.recent_tool_calls.len() > WINDOW_SIZE {
                self.recent_tool_calls.pop_front();
            }
        }
        self.recent_tool_batches.push_back(batch_signatures.clone());
        if self.recent_tool_batches.len() > WINDOW_SIZE {
            self.recent_tool_batches.pop_front();
        }

        for sig in &batch_signatures {
            let name = &sig.0;
            let repeat_count = self.recent_tool_calls.iter().filter(|s| *s == sig).count();

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
                self.recent_tool_batches.clear();

                // Escalate more aggressively when both tool repetition AND visual
                // stuck loop are active — the screen hasn't changed either.
                let visual_escalation = if self.visual_stuck_loop_active {
                    "\n\nCRITICAL: The screen has ALSO not changed after your recent actions. \
                     Both your tool calls AND the visual state are stuck. \
                     You MUST abandon your current strategy entirely and try something fundamentally different."
                } else {
                    ""
                };

                return Some(format!(
                    "STUCK LOOP DETECTED: You have called `{}` {} times with the exact same arguments. \
                     This is not making progress. STOP and try a DIFFERENT approach:\n\
                     - If file_edit fails with 'old_str not found', re-read the file first to see current content\n\
                     - If file_write keeps writing the same content, your output is wrong — re-read the test expectations\n\
                     - If file_read keeps reading the same file, you already have the content — make your edit now\n\
                     - Consider using a completely different tool or strategy{}",
                    name, repeat_count, visual_escalation
                ));
            }
        }

        if let Some((first, second)) = detect_oscillating_batch_pair(&self.recent_tool_batches) {
            warn!(
                "Oscillation loop detected between '{}' and '{}'",
                first.0, second.0
            );
            self.cognitive_state.episodic_memory.what_failed(
                "oscillation_loop",
                &format!(
                    "Stuck oscillating between {} and {} with identical recent signatures",
                    first.0, second.0
                ),
            );
            self.recent_tool_calls.clear();
            self.recent_tool_batches.clear();
            return Some(format!(
                "OSCILLATION LOOP DETECTED: you are alternating between `{}` and `{}` with the same recent inputs (A -> B -> A -> B). This is not making progress. Stop repeating the pair and choose a different approach: reread only if new evidence is needed, edit the file using the content already in context, or switch to a different tool/strategy.",
                first.0, second.0
            ));
        }
        None
    }

    /// Parse the model's intent text to pick a useful fallback tool instead of
    /// always running `directory_tree .`. Extracts file paths, search queries,
    /// or commands the model mentioned but didn't execute.
    pub(super) fn pick_smart_fallback(&self, content: &str) -> (String, String) {
        let stripped = strip_think_blocks(content);

        if let Some(required_fallback) = self.pick_required_tool_fallback() {
            return required_fallback;
        }

        // Try to extract a file path the model mentioned wanting to read.
        if let Some(path) = extract_mentioned_path(&stripped) {
            let p = std::path::Path::new(&path);
            if self.context_map.level_of(p) != Some(crate::evolve::ContextMode::Full) {
                return (
                    "file_read".to_string(),
                    serde_json::json!({"path": path}).to_string(),
                );
            }
        }

        // Pick the next unread source file (prioritize .rs, then other source).
        // This is more useful than keyword matching which returns random non-source files.
        let source_extensions = [".rs", ".toml", ".py", ".ts", ".js", ".go"];
        for ext in &source_extensions {
            let unread: Vec<_> = self
                .context_map
                .files_at_level(crate::evolve::ContextMode::Map)
                .into_iter()
                .chain(
                    self.context_map
                        .files_at_level(crate::evolve::ContextMode::Lite),
                )
                .filter(|p| p.to_string_lossy().ends_with(ext))
                .filter(|p| p.to_string_lossy().starts_with("src/"))
                .collect();

            if let Some(path) = unread.first() {
                let path_str = path.to_string_lossy().to_string();
                return (
                    "file_read".to_string(),
                    serde_json::json!({"path": path_str}).to_string(),
                );
            }
        }

        // Default: list the project structure (only useful once, but safe).
        (
            super::FALLBACK_TOOL_NAME.to_string(),
            super::FALLBACK_TOOL_ARGS.to_string(),
        )
    }

    fn pick_required_tool_fallback(&self) -> Option<(String, String)> {
        let task_context = self
            .current_checkpoint
            .as_ref()
            .map(|cp| cp.task_description.as_str())
            .unwrap_or_else(|| self.learning_context());
        let missing_required_tools = self.missing_required_task_tools();

        for tool_name in missing_required_tools {
            match tool_name.as_str() {
                "file_read" => {
                    if let Some(path) = extract_mentioned_path(task_context) {
                        let args = serde_json::json!({ "path": path }).to_string();
                        return Some((tool_name, args));
                    }
                }
                "vision_analyze" => {
                    if let Some(args) = build_vision_analyze_fallback_args(task_context) {
                        return Some((tool_name, args));
                    }
                }
                "vision_compare" => {
                    if let Some(args) = build_vision_compare_fallback_args(task_context) {
                        return Some((tool_name, args));
                    }
                }
                _ => {}
            }
        }

        None
    }
}

/// Extract a file path mentioned in model output (e.g., "src/main.rs", "./Cargo.toml").
fn extract_mentioned_path(content: &str) -> Option<String> {
    use std::sync::LazyLock;
    static PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r#"(?:^|[\s`"'(])((?:\./|/)?[a-zA-Z_][\w\-./]*\.(?:rs|toml|json|yaml|yml|md|txt|py|ts|js|go))"#,
        )
        .expect("mentioned path regex must compile")
    });

    for cap in PATH_RE.captures_iter(content) {
        let full = cap.get(1)?.as_str().trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
        });
        if full.contains('/') || full.ends_with(".rs") || full.ends_with(".toml") {
            return Some(full.to_string());
        }
    }
    None
}

fn extract_mentioned_image_paths(content: &str) -> Vec<String> {
    use std::sync::LazyLock;
    static IMAGE_PATH_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?i)(?:^|[\s`"'(])((?:\./|/)?[\w./-]+\.(?:png|jpe?g|webp|gif|bmp))"#)
            .expect("image path regex must compile")
    });

    let mut paths = Vec::new();
    for cap in IMAGE_PATH_RE.captures_iter(content) {
        let Some(path) = cap.get(1).map(|m| m.as_str().to_string()) else {
            continue;
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
    }

    paths
}

fn derive_vision_prompt(task_context: &str, anchor: &str, default_prompt: &str) -> String {
    let prompt = task_context
        .split_once(anchor)
        .map(|(_, after)| after.trim())
        .unwrap_or(task_context)
        .trim_start_matches(|c: char| c.is_ascii_whitespace() || matches!(c, ',' | ';' | ':'))
        .trim_start_matches("and ")
        .trim_start_matches("then ")
        .trim_start_matches("please ")
        .trim();

    if prompt.is_empty() {
        default_prompt.to_string()
    } else {
        prompt.to_string()
    }
}

fn build_vision_analyze_fallback_args(task_context: &str) -> Option<String> {
    let image_path = extract_mentioned_image_paths(task_context)
        .into_iter()
        .next()?;
    let prompt = derive_vision_prompt(
        task_context,
        &image_path,
        "Describe the main subject in the image.",
    );
    Some(
        serde_json::json!({
            "image_path": image_path,
            "prompt": prompt,
        })
        .to_string(),
    )
}

fn build_vision_compare_fallback_args(task_context: &str) -> Option<String> {
    let image_paths = extract_mentioned_image_paths(task_context);
    if image_paths.len() < 2 {
        return None;
    }

    let prompt = derive_vision_prompt(
        task_context,
        image_paths.get(1)?,
        "Compare these two images and summarize the main differences.",
    );
    Some(
        serde_json::json!({
            "image_a": image_paths[0],
            "image_b": image_paths[1],
            "threshold": 90.0,
            "prompt": prompt,
        })
        .to_string(),
    )
}

/// Extract a quoted string from content (single or double quotes, or backticks).
fn _extract_quoted_string(content: &str) -> Option<String> {
    for delim in ['"', '\'', '`'] {
        if let Some(start) = content.find(delim) {
            if let Some(end) = content[start + 1..].find(delim) {
                let inner = &content[start + 1..start + 1 + end];
                if !inner.is_empty() && inner.len() < 200 {
                    return Some(inner.to_string());
                }
            }
        }
    }
    None
}

/// The repetition guard's fingerprint for one call: tool name plus argument
/// hash, with the observed revision folded in for calls that only observe
/// state.
///
/// A call that observes or verifies answers a question about the tree at
/// `revision`; the same call at a later revision asks a new question. Every
/// other call keeps the plain `(name, args)` fingerprint: an edit advances
/// the revision on every run, so folding the revision in would hide a true
/// re-apply loop. A verification the shell classifier also counts as
/// mutating (`tsc`) is still an observer here: it does not advance
/// `revision` (see `repetition_guard_revision`), so re-running it with no
/// edit in between keeps the same fingerprint.
pub(super) fn repetition_signature(name: &str, args_str: &str, revision: usize) -> (String, u64) {
    use std::hash::{Hash, Hasher};
    let args_hash = super::tool_dispatch::hash_tool_args(args_str);
    let observes = super::tool_dispatch::tool_call_is_observational(name, args_str)
        || super::tool_dispatch::tool_call_is_verification(name, args_str);
    if !observes {
        return (name.to_string(), args_hash);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    args_hash.hash(&mut hasher);
    revision.hash(&mut hasher);
    (name.to_string(), hasher.finish())
}

#[cfg(test)]
#[path = "../../tests/unit/agent/recovery/recovery_test.rs"]
mod tests;
