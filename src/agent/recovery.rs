use std::hash::{Hash, Hasher};

use tracing::{debug, info, warn};

use super::*;
use crate::api::types::Message;

/// Maximum consecutive no-action prompts before aborting.
pub(super) const MAX_NO_ACTION_PROMPTS: usize = 5;
/// After this many text-only reprompts, force a deterministic fallback tool call
/// instead of sending another text correction the model will ignore.
/// Set to 2: one text correction, then immediate force-fallback.
/// Weak models (qwen3.5-27b) rarely respond to text corrections.
pub(super) const FORCE_FALLBACK_AFTER: usize = 2;
/// Absolute lifetime cap on total no-action prompts across the entire task.
/// Prevents infinite cycling when the consecutive counter gets reset by
/// intervening responses that pass `should_prompt_for_action` (e.g. long
/// responses, or responses without intent phrases after seeing forced tool output).
/// Increased to 50 because smart fallbacks make real progress (reading files)
/// even though the model itself isn't producing tool calls.
pub(super) const MAX_TOTAL_NO_ACTION_PROMPTS: usize = 50;
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

/// Strip `<think>...</think>` blocks and Qwen3.5 thinking from content.
pub(super) fn strip_think_blocks(content: &str) -> String {
    // Handle Qwen3.5 format: extensive thinking followed by </think> marker
    // The model outputs thinking as regular text, then </think>, then the answer
    if let Some(end_think) = content.find("</think>") {
        // Return everything after </think>
        let after_think = &content[end_think + 8..]; // 8 = len("</think>")
        return after_think.trim().to_string();
    }

    // Handle explicit <think>...</think> tags (for other models)
    let mut result = String::with_capacity(content.len());
    let mut rest = content;
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

impl Agent {
    pub(super) fn reset_no_action_prompt_state(&mut self) {
        self.consecutive_no_action_prompts = 0;
        self.last_no_action_prompt_hash = None;
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

        crate::output::intent_without_action();

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

        // Strip any residual think blocks from content to measure real output.
        let effective_content = strip_think_blocks(content);
        let effective_len = effective_content.len();

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

        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let lower = effective_content.to_lowercase();
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

        // Try to extract a file path the model mentioned wanting to read.
        if let Some(path) = extract_mentioned_path(&stripped) {
            let p = std::path::Path::new(&path);
            if self.context_map.level_of(p)
                != Some(super::context_map::ContextLevel::Full)
            {
                return (
                    "file_read".to_string(),
                    format!(r#"{{"path":"{}"}}"#, path),
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
                    format!(r#"{{"path":"{}"}}"#, path_str),
                );
            }
        }

        // Default: list the project structure (only useful once, but safe).
        (
            super::FALLBACK_TOOL_NAME.to_string(),
            super::FALLBACK_TOOL_ARGS.to_string(),
        )
    }
}

/// Extract a file path mentioned in model output (e.g., "src/main.rs", "./Cargo.toml").
fn extract_mentioned_path(content: &str) -> Option<String> {
    let path_re = regex::Regex::new(
        r#"(?:^|[\s`"'(])(\./)?([a-zA-Z_][\w\-./]*\.(?:rs|toml|json|yaml|yml|md|txt|py|ts|js|go))"#,
    )
    .ok()?;

    for cap in path_re.captures_iter(content) {
        let full = cap.get(0)?.as_str().trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
        });
        if full.contains('/') || full.ends_with(".rs") || full.ends_with(".toml") {
            return Some(full.to_string());
        }
    }
    None
}

/// Extract a quoted string from content (single or double quotes, or backticks).
#[allow(dead_code)]
fn extract_quoted_string(content: &str) -> Option<String> {
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
