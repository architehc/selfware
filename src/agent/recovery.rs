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

pub(super) fn looks_like_malformed_tool_xml(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }

    let has_tool_tag = trimmed.contains("<tool");
    let has_tool_close = trimmed.contains("</tool>");
    let has_valid_tool_shape = has_tool_tag
        && has_tool_close
        && trimmed.contains("<name>")
        && trimmed.contains("</name>")
        && trimmed.contains("<arguments>")
        && trimmed.contains("</arguments>");

    (has_tool_tag && !has_valid_tool_shape)
        || trimmed.contains("<function=")
        || trimmed.contains("<name=")
        || trimmed.contains("<parameter=")
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

/// Strip `<think>...</think>` blocks, Qwen3.5 thinking, and gemma
/// `<|channel>thought...<channel|>` blocks from content.
pub(super) fn strip_think_blocks(content: &str) -> String {
    // Handle gemma-4 format: <|channel>thought...<channel|> blocks
    let content = strip_gemma_thinking(content);

    // Handle Qwen3.5 format: extensive thinking followed by </think> marker
    // The model outputs thinking as regular text, then </think>, then the answer.
    // Only strip if there is a matching <think> start tag; otherwise preserve
    // the content so an unclosed think tag does not erase the actual answer.
    if let Some(start_think) = content.find("<think>") {
        if let Some(end_think) = content[start_think..].find("</think>") {
            let after_think = &content[start_think + end_think + 8..];
            return after_think.trim().to_string();
        }
    }

    // Handle explicit <think>...</think> tags (for other models)
    let mut result = String::with_capacity(content.len());
    let mut rest = content.as_str();
    while let Some(start) = rest.find("<think>") {
        result.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + 8..],
            None => {
                rest = "";
                break;
            }
        }
    }
    result.push_str(rest);
    result
}

/// Strip gemma `<|channel>thought...<channel|>` blocks from content.
fn strip_gemma_thinking(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("<|channel>") {
        result.push_str(&rest[..start]);
        match rest[start..].find("<channel|>") {
            Some(end) => rest = &rest[start + end + 10..], // 10 = len("<channel|>")
            None => {
                // Unclosed thinking block — strip everything after the marker
                rest = "";
                break;
            }
        }
    }
    result.push_str(rest);
    result
}

impl Agent {
    pub(super) fn reset_no_action_prompt_state(&mut self) {
        self.consecutive_no_action_prompts = 0;
        self.last_no_action_prompt_hash = None;
    }

    pub(super) fn missing_required_task_tools(&self) -> Vec<String> {
        let successful_tools: std::collections::BTreeSet<String> = self
            .current_checkpoint
            .as_ref()
            .map(|cp| {
                cp.tool_calls
                    .iter()
                    .filter(|tc| tc.success)
                    .map(|tc| tc.tool_name.clone())
                    .collect()
            })
            .unwrap_or_default();

        self.required_task_tools
            .iter()
            .filter(|tool| !successful_tools.contains(*tool))
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

    /// Build a hint message to guide the model after a tool failure.
    /// This helps the model recover by suggesting alternative approaches.
    pub(super) fn build_error_recovery_hint(&self, tool_name: &str, error: &str) -> String {
        let error_lower = error.to_lowercase();

        // Endpoint / connection errors — self-healing
        if error_lower.contains("connection refused")
            || error_lower.contains("connection reset")
            || error_lower.contains("timed out")
            || error_lower.contains("502 bad gateway")
            || error_lower.contains("503 service unavailable")
            || error_lower.contains("endpoint")
            || error_lower.contains("network unreachable")
        {
            warn!(
                "Endpoint error detected for tool '{}': {}",
                tool_name,
                &error[..error.len().min(200)]
            );
            return format!(
                "ERROR RECOVERY: Endpoint/connection issue detected for '{}'. \
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
                &error[..error.len().min(200)]
            );
            return "ERROR RECOVERY: Rate limit or quota exceeded. \
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
            return "ERROR RECOVERY: Tests are failing after your change. This may be a regression. \
                 Steps to fix:\
                 1. Read the test error output carefully\
                 2. Use git_diff to review your changes\
                 3. If your edit introduced the failure, use file_edit to fix it\
                 4. Run the tests again to verify\
                 Do NOT add more tests — fix the source code that caused the regression.".to_string();
        }

        // File not found errors - suggest alternatives
        if error_lower.contains("file not found") || error_lower.contains("no such file") {
            return format!(
                "ERROR RECOVERY: The tool '{}' failed because the file was not found. \
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
                "ERROR RECOVERY: The tool '{}' failed due to permission issues. \
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
                "ERROR RECOVERY: The tool '{}' failed because the path is outside allowed directories. \
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
                "ERROR RECOVERY: The tool '{}' failed due to invalid arguments. \
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
            "ERROR RECOVERY: The tool '{}' failed. \
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
    /// Returns `Some(message)` if the same tool+args has been called too many times recently.
    pub(super) fn detect_repetition(
        &mut self,
        tool_calls: &[super::execution::CollectedToolCall],
    ) -> Option<String> {
        const MAX_REPEATS: usize = 3;
        const WINDOW_SIZE: usize = 10;

        let batch_signatures: Vec<_> = tool_calls
            .iter()
            .map(|(name, args_str, _)| {
                (name.clone(), super::tool_dispatch::hash_tool_args(args_str))
            })
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
            if self.context_map.level_of(p) != Some(super::context_map::ContextLevel::Full) {
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
                .files_at_level(super::context_map::ContextLevel::Tree)
                .into_iter()
                .chain(
                    self.context_map
                        .files_at_level(super::context_map::ContextLevel::Skeleton),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mentioned_image_paths_finds_absolute_image() {
        let paths = extract_mentioned_image_paths(
            "Use vision_analyze on /home/ivo/radarcam/samples/sample_3_00000693.jpg and answer.",
        );
        assert_eq!(
            paths,
            vec!["/home/ivo/radarcam/samples/sample_3_00000693.jpg".to_string()]
        );
    }

    #[test]
    fn build_vision_analyze_fallback_args_uses_task_prompt_suffix() {
        let args = build_vision_analyze_fallback_args(
            "Use vision_analyze on /tmp/frame.png and answer in one short sentence describing the main subject.",
        )
        .expect("expected fallback args");
        let parsed: serde_json::Value = serde_json::from_str(&args).expect("valid json");
        assert_eq!(parsed["image_path"], "/tmp/frame.png");
        assert_eq!(
            parsed["prompt"],
            "answer in one short sentence describing the main subject."
        );
    }

    #[test]
    fn extract_mentioned_path_finds_absolute_markdown_file() {
        let path = extract_mentioned_path(
            "Use file_read on /home/ivo/radarcam/AGENTS.md and answer in one line.",
        )
        .expect("expected path");
        assert_eq!(path, "/home/ivo/radarcam/AGENTS.md");
    }

    #[test]
    fn strip_think_blocks_removes_paired_tags() {
        let content = "<think>inner thought</think>answer";
        assert_eq!(strip_think_blocks(content), "answer");
    }

    #[test]
    fn strip_think_blocks_preserves_unclosed_tag_content() {
        // An unmatched </think> should not erase the preceding answer.
        let content = "final answer text </think>";
        assert_eq!(strip_think_blocks(content), "final answer text </think>");
    }

    #[test]
    fn strip_think_blocks_extracts_after_paired_end_tag() {
        let content = "<think>thinking</think>  the answer  ";
        assert_eq!(strip_think_blocks(content), "the answer");
    }
}
