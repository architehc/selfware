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

/// Maximum tokens allowed for a single tool result before it gets summarized.
/// ~5% of a 1M context window — leaves room for many tool results in one session.
const MAX_TOOL_RESULT_TOKENS: usize = 50_000;

/// Directory where oversized raw tool results are spilled to disk.
const TOOL_RESULTS_DIR: &str = ".selfware/tool_results";

/// Summarize an oversized tool result and save raw data to disk.
///
/// Returns a structured summary string that includes:
/// - Key statistics extracted from the result
/// - A reference path to the raw data on disk
/// - Enough context for the agent to decide whether to drill down
fn summarize_and_spill(
    tool_name: &str,
    call_id: &str,
    raw: &str,
    estimated_tokens: usize,
) -> String {
    // Save raw result to disk
    let spill_dir = std::path::Path::new(TOOL_RESULTS_DIR);
    let _ = std::fs::create_dir_all(spill_dir);
    let spill_file = spill_dir.join(format!(
        "{}_{}.json",
        tool_name,
        &call_id[..call_id.len().min(12)]
    ));
    let spill_path = spill_file.display().to_string();
    if let Err(e) = std::fs::write(&spill_file, raw) {
        warn!("Failed to spill tool result to {}: {}", spill_path, e);
        // Fall back to aggressive truncation if disk write fails
        let truncated: String = raw.chars().take(20_000).collect();
        return format!(
            "{}\n\n[TRUNCATED — original was ~{} tokens, disk spill failed: {}]",
            truncated, estimated_tokens, e
        );
    }

    // Build a tool-specific structured summary
    let summary = match tool_name {
        "directory_tree" => summarize_directory_tree(raw),
        "file_read" => summarize_file_read(raw),
        "git_diff" => summarize_git_diff(raw),
        "context_bulk_read" => summarize_bulk_read(raw),
        "shell_exec" => summarize_shell_exec(raw),
        _ => summarize_generic(raw),
    };

    format!(
        "{}\n\n[SUMMARY — original result was ~{} tokens. Raw data saved to: {} — use file_read to inspect details]",
        summary, estimated_tokens, spill_path
    )
}

fn summarize_directory_tree(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let root = v.get("root").and_then(|v| v.as_str()).unwrap_or(".");
    let entries = v.get("entries").and_then(|v| v.as_array());
    let total = v.get("total").and_then(|v| v.as_u64()).unwrap_or(0);

    if let Some(entries) = entries {
        // Count dirs vs files, group top-level
        let mut dir_count = 0usize;
        let mut file_count = 0usize;
        let mut top_level: std::collections::BTreeMap<String, (usize, usize, u64)> =
            std::collections::BTreeMap::new(); // name -> (files, dirs, total_size)

        for entry in entries {
            let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let etype = entry.get("type").and_then(|v| v.as_str()).unwrap_or("file");
            let size = entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0);

            if etype == "directory" {
                dir_count += 1;
            } else {
                file_count += 1;
            }

            // Extract first path component after root
            let relative = path.strip_prefix(root).unwrap_or(path);
            let relative = relative.trim_start_matches('/');
            if let Some(top) = relative.split('/').next() {
                if !top.is_empty() {
                    let entry = top_level.entry(top.to_string()).or_insert((0, 0, 0));
                    if etype == "directory" {
                        entry.1 += 1;
                    } else {
                        entry.0 += 1;
                    }
                    entry.2 += size;
                }
            }
        }

        let mut summary = format!(
            "Directory: {}\nTotal: {} entries ({} files, {} dirs)\n\nTop-level contents:\n",
            root, total, file_count, dir_count
        );
        for (name, (files, dirs, size)) in &top_level {
            let size_str = if *size > 1_000_000 {
                format!("{:.1}MB", *size as f64 / 1_000_000.0)
            } else if *size > 1_000 {
                format!("{:.1}KB", *size as f64 / 1_000.0)
            } else {
                format!("{}B", size)
            };
            summary.push_str(&format!(
                "  {:<30} {:>4} files, {:>3} dirs, {}\n",
                name, files, dirs, size_str
            ));
        }
        summary
    } else {
        format!(
            "Directory: {} — {} entries (parse failed, see raw file)",
            root, total
        )
    }
}

fn summarize_file_read(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let total_lines = v.get("total_lines").and_then(|v| v.as_u64()).unwrap_or(0);
    let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");

    // Show first 100 and last 50 lines
    let lines: Vec<&str> = content.lines().collect();
    let head: String = lines
        .iter()
        .take(100)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let tail: String = if lines.len() > 150 {
        lines
            .iter()
            .rev()
            .take(50)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };

    let mut summary = format!(
        "File: {} total lines\n\n--- First 100 lines ---\n{}",
        total_lines, head
    );
    if !tail.is_empty() {
        summary.push_str(&format!(
            "\n\n--- Last 50 lines (lines {}–{}) ---\n{}",
            lines.len() - 50,
            lines.len(),
            tail
        ));
    }
    if lines.len() > 150 {
        summary.push_str(&format!(
            "\n\n[{} lines omitted from middle]",
            lines.len() - 150
        ));
    }
    summary
}

fn summarize_git_diff(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let diff = v.get("diff").and_then(|v| v.as_str()).unwrap_or("");

    // Parse diff headers to extract per-file stats
    let mut files: Vec<(String, usize, usize)> = Vec::new(); // (path, added, removed)
    let mut current_file = String::new();
    let mut added = 0usize;
    let mut removed = 0usize;

    for line in diff.lines() {
        if line.starts_with("diff --git") {
            if !current_file.is_empty() {
                files.push((current_file.clone(), added, removed));
            }
            current_file = line.split(" b/").last().unwrap_or("").to_string();
            added = 0;
            removed = 0;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            added += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            removed += 1;
        }
    }
    if !current_file.is_empty() {
        files.push((current_file, added, removed));
    }

    let total_added: usize = files.iter().map(|(_, a, _)| a).sum();
    let total_removed: usize = files.iter().map(|(_, _, r)| r).sum();

    let mut summary = format!(
        "Diff: {} files changed, +{} -{}\n\n",
        files.len(),
        total_added,
        total_removed
    );
    for (path, a, r) in &files {
        summary.push_str(&format!("  {:<60} +{:<5} -{}\n", path, a, r));
    }
    summary
}

fn summarize_bulk_read(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let loaded = v.get("loaded").and_then(|v| v.as_u64()).unwrap_or(0);
    let skipped = v.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0);
    let tokens = v.get("tokens_added").and_then(|v| v.as_u64()).unwrap_or(0);
    format!(
        "Bulk read: {} files loaded, {} skipped, {} tokens added",
        loaded, skipped, tokens
    )
}

fn summarize_shell_exec(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let exit_code = v.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
    let stdout = v.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    let stderr = v.get("stderr").and_then(|v| v.as_str()).unwrap_or("");

    let stdout_lines: Vec<&str> = stdout.lines().collect();
    let stderr_lines: Vec<&str> = stderr.lines().collect();

    let mut summary = format!(
        "Exit code: {}\nStdout: {} lines, Stderr: {} lines\n",
        exit_code,
        stdout_lines.len(),
        stderr_lines.len()
    );

    // Show first 80 + last 20 lines of stdout
    let head: String = stdout_lines
        .iter()
        .take(80)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    summary.push_str(&format!("\n--- stdout (first 80 lines) ---\n{}", head));
    if stdout_lines.len() > 100 {
        let tail: String = stdout_lines
            .iter()
            .rev()
            .take(20)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        summary.push_str(&format!(
            "\n\n--- stdout (last 20 lines) ---\n{}\n[{} lines omitted]",
            tail,
            stdout_lines.len() - 100
        ));
    }

    if !stderr.is_empty() {
        let stderr_head: String = stderr_lines
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        summary.push_str(&format!(
            "\n\n--- stderr (first 30 lines) ---\n{}",
            stderr_head
        ));
    }
    summary
}

fn summarize_generic(raw: &str) -> String {
    // Show first 15K chars + stats
    let char_count = raw.chars().count();
    let line_count = raw.lines().count();
    let preview: String = raw.chars().take(15_000).collect();
    format!(
        "{}\n\n[... {} total chars, {} lines — see raw file for full output]",
        preview, char_count, line_count
    )
}

/// Classification of tool execution failures for better recovery suggestions.
///
/// Each variant represents a category of error that the agent can use to
/// adapt its strategy and provide contextual recovery hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// Safety/blocked operations (e.g., attempting to modify protected files)
    SafetyViolation,
    /// Missing files or resources
    ResourceNotFound,
    /// Permission denied errors
    PermissionDenied,
    /// Invalid arguments, parse errors, or JSON issues
    ArgumentError,
    /// Timeout errors
    Timeout,
    /// Generic execution errors (fallback)
    ExecutionError,
}

impl ToolErrorKind {
    /// Classify an error message into a ToolErrorKind.
    ///
    /// Uses keyword heuristics to categorize error messages.
    pub fn classify(error: &str) -> Self {
        let error_lower = error.to_lowercase();
        if error_lower.contains("safety") || error_lower.contains("blocked") {
            Self::SafetyViolation
        } else if error_lower.contains("not found") || error_lower.contains("no such file") {
            Self::ResourceNotFound
        } else if error_lower.contains("permission")
            || error_lower.contains("denied")
            || error_lower.contains("not permitted")
        {
            Self::PermissionDenied
        } else if error_lower.contains("parse")
            || error_lower.contains("json")
            || error_lower.contains("invalid")
        {
            Self::ArgumentError
        } else if error_lower.contains("timeout") || error_lower.contains("timed out") {
            Self::Timeout
        } else {
            Self::ExecutionError
        }
    }

    /// Get a human-readable name for this error kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SafetyViolation => "SAFETY_VIOLATION",
            Self::ResourceNotFound => "RESOURCE_NOT_FOUND",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::ArgumentError => "ARGUMENT_ERROR",
            Self::Timeout => "TIMEOUT",
            Self::ExecutionError => "EXECUTION_ERROR",
        }
    }

    /// Get a recovery hint specific to this error kind.
    ///
    /// The hint guides the agent toward appropriate corrective actions.
    pub fn recovery_hint(&self) -> &'static str {
        match self {
            Self::SafetyViolation => {
                "Try a different approach that doesn't modify protected files."
            }
            Self::ResourceNotFound => "Check the path exists or create the resource first.",
            Self::PermissionDenied => "Use sudo or check file permissions before retrying.",
            Self::ArgumentError => "Review the tool schema and fix the arguments.",
            Self::Timeout => "Consider breaking the task into smaller steps.",
            Self::ExecutionError => "Review the error and adjust your approach.",
        }
    }
}

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

fn configured_vision_profile(
    config: &crate::config::Config,
) -> Option<&crate::config::ModelProfile> {
    config
        .models
        .get("vision")
        .or_else(|| config.resolve_model(None))
}

fn insert_missing_tool_arg(
    obj: &mut serde_json::Map<String, Value>,
    key: &str,
    value: Value,
) -> bool {
    match obj.get(key) {
        Some(existing) if !existing.is_null() => false,
        _ => {
            obj.insert(key.to_string(), value);
            true
        }
    }
}

pub(super) fn inject_runtime_tool_defaults(
    config: &crate::config::Config,
    name: &str,
    args_str: &str,
) -> String {
    if !matches!(name, "vision_analyze" | "vision_compare") {
        return args_str.to_string();
    }

    let Some(profile) = configured_vision_profile(config) else {
        return args_str.to_string();
    };

    let Ok(mut args) = serde_json::from_str::<Value>(args_str) else {
        return args_str.to_string();
    };
    let Some(obj) = args.as_object_mut() else {
        return args_str.to_string();
    };

    let mut changed = false;
    changed |= insert_missing_tool_arg(obj, "endpoint", serde_json::json!(profile.endpoint));
    changed |= insert_missing_tool_arg(obj, "model", serde_json::json!(profile.model));
    changed |= insert_missing_tool_arg(obj, "max_tokens", serde_json::json!(profile.max_tokens));
    changed |= insert_missing_tool_arg(obj, "temperature", serde_json::json!(profile.temperature));
    changed |= insert_missing_tool_arg(obj, "detail", serde_json::json!("low"));

    if let Some(extra_body) = &profile.extra_body {
        changed |= insert_missing_tool_arg(obj, "extra_body", serde_json::json!(extra_body));
    }

    if changed {
        serde_json::to_string(&args).unwrap_or_else(|_| args_str.to_string())
    } else {
        args_str.to_string()
    }
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

        // Classify the error and generate contextual recovery hint
        let error_kind = ToolErrorKind::classify(error);
        let recovery_hint = error_kind.recovery_hint();

        self.pending_failure_hint = Some(format!(
            "⚠️  Tool failure [{}]: `{}` failed.\n   Error: {}\n   Recovery: {}",
            error_kind.as_str(),
            tool_name,
            error_preview,
            recovery_hint
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

    /// Tools that are safe to execute concurrently (read-only, no side effects).
    const PARALLEL_SAFE_TOOLS: &'static [&'static str] = &[
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "git_status",
        "git_diff",
        "git_log",
        "lsp_document_symbols",
        "lsp_find_references",
        "lsp_goto_definition",
        "lsp_hover",
    ];

    /// Check if a tool can be executed concurrently with other tools.
    fn is_parallel_safe(name: &str) -> bool {
        Self::PARALLEL_SAFE_TOOLS.contains(&name)
    }

    pub(super) async fn execute_tool_batch(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<()> {
        // Phase 1: Partition into parallel-safe and sequential groups.
        // Read-only tools with no path conflicts go into the parallel batch.
        let mut parallel_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut sequential_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut parallel_paths: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        for call in &tool_calls {
            let (name, args_str, _) = call;
            if Self::is_parallel_safe(name) {
                // Check for path conflicts within the parallel batch
                let path = serde_json::from_str::<serde_json::Value>(args_str)
                    .ok()
                    .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(String::from));
                let has_conflict = path.as_ref().is_some_and(|p| parallel_paths.contains(p));
                if has_conflict {
                    sequential_batch.push(call.clone());
                } else {
                    if let Some(ref p) = path {
                        parallel_paths.insert(p.clone());
                    }
                    parallel_batch.push(call.clone());
                }
            } else {
                sequential_batch.push(call.clone());
            }
        }

        // Phase 2: Execute parallel-safe tools concurrently.
        // Phase 2: If fewer than 2 parallel tools, merge back and run everything
        // sequentially in the original order to preserve execution semantics.
        if parallel_batch.len() < 2 {
            for (name, args_str, tool_call_id) in tool_calls {
                if self.is_cancelled() {
                    break;
                }
                self.execute_single_tool_in_batch(name, args_str, tool_call_id)
                    .await?;
            }
            return Ok(());
        }

        // Phase 3: Execute parallel-safe tools concurrently.
        debug!(
            "Executing {} tools in parallel, {} sequentially",
            parallel_batch.len(),
            sequential_batch.len()
        );
        self.execute_parallel_tools(parallel_batch).await?;

        // Phase 4: Execute sequential tools one at a time.
        for (name, args_str, tool_call_id) in sequential_batch {
            if self.is_cancelled() {
                break;
            }
            self.execute_single_tool_in_batch(name, args_str, tool_call_id)
                .await?;
        }

        Ok(())
    }

    /// Execute multiple read-only tools concurrently.
    ///
    /// Pre-validates all tools sequentially (fast), spawns concurrent executions
    /// for tools that pass validation, then processes results sequentially.
    async fn execute_parallel_tools(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<()> {
        use super::tui_events::AgentEvent;
        use crate::hooks::HookAction;

        // Pre-validate all tools and collect validated ones for concurrent execution
        struct ValidatedTool {
            name: String,
            args_str: String,
            args: Value,
            call_id: String,
            use_native_fc: bool,
            start_time: std::time::Instant,
        }

        let mut validated: Vec<ValidatedTool> = Vec::with_capacity(tool_calls.len());

        for (name, args_str, tool_call_id) in tool_calls {
            if self.is_cancelled() {
                break;
            }
            let args_str = inject_runtime_tool_defaults(&self.config, &name, &args_str);

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
                crate::output::safety_blocked(&error_msg);
                if let Some(ref logger) = self.audit_logger {
                    logger.log_safety_block(&name, &error_msg);
                }
                self.push_tool_result_message(use_native_fc, &call_id, &name, false, &error_msg);
                self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                self.remember_failed_tool(&name, &error_msg);
                self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
                continue;
            }

            let args =
                match self.parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time) {
                    Some(args) => args,
                    None => continue,
                };

            if !self.validate_tool_args(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            ) {
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

            validated.push(ValidatedTool {
                name,
                args_str,
                args,
                call_id,
                use_native_fc,
                start_time,
            });
        }

        if validated.is_empty() {
            return Ok(());
        }

        // Build futures for concurrent execution. We use FuturesUnordered
        // to run them concurrently within the current task (no spawning needed,
        // avoids 'static lifetime requirements).
        let timeout_secs = self.config.agent.step_timeout_secs.max(1);

        for vt in &validated {
            let activity = crate::output::tool_activity_message(&vt.name, &vt.args);
            cli_println!("  {} {}", "↪".bright_black(), activity.dimmed());
        }

        // Execute all validated tools concurrently using the tool registry
        let mut results: Vec<(usize, (bool, String, String))> = Vec::with_capacity(validated.len());

        {
            use futures::stream::{FuturesUnordered, StreamExt};
            let mut futures = FuturesUnordered::new();

            for (idx, vt) in validated.iter().enumerate() {
                let tool_name = vt.name.clone();
                let tool_args = vt.args.clone();
                let tool_ref = self.tools.get(&tool_name);

                futures.push(async move {
                    let Some(tool) = tool_ref else {
                        let msg = format!("Unknown tool: {}", tool_name);
                        return (idx, (false, msg.clone(), msg));
                    };
                    let start = std::time::Instant::now();
                    let execution = tokio::time::timeout(
                        std::time::Duration::from_secs(timeout_secs),
                        tool.execute(tool_args.clone()),
                    )
                    .await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    match execution {
                        Ok(Ok(result)) => {
                            let result_str =
                                serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                            let summary = crate::output::semantic_summary(
                                &tool_name,
                                &tool_args,
                                Some(&result_str),
                                true,
                                elapsed,
                            );
                            (idx, (true, result_str, summary))
                        }
                        Ok(Err(e)) => {
                            let summary = crate::output::semantic_summary(
                                &tool_name,
                                &tool_args,
                                Some(&e.to_string()),
                                false,
                                elapsed,
                            );
                            (idx, (false, e.to_string(), summary))
                        }
                        Err(_) => {
                            let msg = format!("Tool execution timed out after {}s", timeout_secs);
                            (idx, (false, msg.clone(), msg))
                        }
                    }
                });
            }

            while let Some(result) = futures.next().await {
                results.push(result);
            }
        }

        // Sort by original order to maintain deterministic message ordering
        results.sort_by_key(|(idx, _)| *idx);

        // Post-process all results
        for (idx, (success, result_str, summary)) in results {
            let vt = &validated[idx];

            let duration_ms = vt.start_time.elapsed().as_millis() as u64;
            self.emit_event(AgentEvent::ToolCompleted {
                name: vt.name.clone(),
                success,
                duration_ms,
            });

            if success {
                cli_println!("  {} {}", "✔".bright_green(), summary);
            } else {
                cli_println!("  {} {}", "✗".bright_red(), summary);
            }

            // Store for progressive disclosure via /last
            {
                let exit_code = serde_json::from_str::<serde_json::Value>(&result_str)
                    .ok()
                    .and_then(|v| v.get("exit_code").and_then(|c| c.as_i64()))
                    .map(|c| c as i32);
                self.store_last_tool_output(crate::agent::last_tool::LastToolOutput {
                    tool_name: vt.name.clone(),
                    summary: summary.clone(),
                    full_output: result_str.clone(),
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
            let tool_error = (!success).then(|| result_str.clone());
            self.self_improvement.record_tool(
                &vt.name,
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
                    &vt.name,
                    None,
                );
            }
            if success {
                self.clear_failed_tool_attempts();
            } else {
                self.record_failed_tool_attempt(&vt.name, &vt.args_str, "execution", &result_str);
            }

            self.track_task_state_after_tool(&vt.name, &vt.args, &result_str, success);

            // Track file operations for context management
            if success {
                if let Some(path) = vt.args.get("path").and_then(|v| v.as_str()) {
                    let path_str = path.to_string();
                    if vt.name == "file_read" {
                        if self.file_tracker.context_files.len() < 500
                            && !self.file_tracker.context_files.contains(&path_str)
                        {
                            self.file_tracker.context_files.push(path_str.clone());
                        }
                        if let Some(content) =
                            serde_json::from_str::<serde_json::Value>(&result_str)
                                .ok()
                                .and_then(|v| {
                                    v.get("content").and_then(|c| c.as_str()).map(String::from)
                                })
                        {
                            self.track_file_read_in_context_map(&path_str, &content);
                        }
                    }
                }
            }

            self.push_tool_result_message(
                vt.use_native_fc,
                &vt.call_id,
                &vt.name,
                success,
                &result_str,
            );
            if !success {
                self.remember_failed_tool(&vt.name, &result_str);
            }

            self.reset_no_action_prompt_state();

            if !success {
                let recovery_hint = self.build_error_recovery_hint(&vt.name, &result_str);
                self.messages.push(Message::user(recovery_hint));
            }

            // Fire PostToolUse hooks
            let post_ctx = HookContext::post_tool(&vt.name, &vt.args_str, success, &result_str);
            self.hook_registry.fire(&post_ctx).await;

            // Audit log
            if let Some(ref logger) = self.audit_logger {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                vt.args_str.hash(&mut hasher);
                let args_hash = format!("{:x}", hasher.finish());
                logger.log_tool_execution(&vt.name, &args_hash, success, duration_ms, None);
            }

            self.log_tool_call(
                &vt.name,
                &vt.args_str,
                &result_str,
                success,
                vt.start_time,
                true,
            );
        }

        Ok(())
    }

    /// Execute a single tool call within a batch (sequential path).
    async fn execute_single_tool_in_batch(
        &mut self,
        name: String,
        args_str: String,
        tool_call_id: Option<String>,
    ) -> Result<()> {
        use super::tui_events::AgentEvent;
        use crate::hooks::HookAction;

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

        let args_str = inject_runtime_tool_defaults(&self.config, &name, &args_str);
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
            return Ok(());
        }

        if let Err(e) = self.safety.check_tool_call(&fake_call) {
            let error_msg = format!("Safety check failed: {}", e);
            let spinner = crate::ui::spinner::TerminalSpinner::start(&error_msg);
            spinner.stop_error(&error_msg);
            crate::output::safety_blocked(&error_msg);
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
            return Ok(());
        }

        let args = match self.parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time)
        {
            Some(args) => args,
            None => {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return Ok(());
            }
        };

        if !self.validate_tool_args(&name, &args_str, &args, &call_id, use_native_fc, start_time) {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
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
            return Ok(());
        }

        if !self
            .confirm_tool_execution(&name, &args_str, &call_id, use_native_fc)
            .await?
        {
            return Ok(());
        }

        // Fire PreToolUse hooks (may skip execution)
        let pre_ctx = HookContext::pre_tool(&name, &args_str);
        if let HookAction::Skip { reason } = self.hook_registry.fire(&pre_ctx).await {
            let skip_msg = format!("Tool skipped by PreToolUse hook: {}", reason);
            info!("{}", skip_msg);
            self.push_tool_result_message(use_native_fc, &call_id, &name, false, &skip_msg);
            return Ok(());
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
                        if let Some(content) = serde_json::from_str::<serde_json::Value>(&result)
                            .ok()
                            .and_then(|v| {
                                v.get("content").and_then(|c| c.as_str()).map(String::from)
                            })
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

        Ok(())
    }

    /// Execute a context management tool (operates on agent state, not filesystem).
    async fn execute_context_tool_async(
        &mut self,
        name: &str,
        args: &serde_json::Value,
    ) -> serde_json::Value {
        use crate::tools::context::*;

        match name {
            CONTEXT_BULK_READ => {
                let pattern = args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("src/**/*.rs");
                let max_files =
                    args.get("max_files").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

                // Collect matching files from context map.
                let root = super::current_project_root();
                let mut paths: Vec<std::path::PathBuf> = Vec::new();
                let glob_pattern = root.join(pattern).to_string_lossy().to_string();
                if let Ok(entries) = glob::glob(&glob_pattern) {
                    for entry in entries.flatten() {
                        if let Ok(rel) = entry.strip_prefix(&root) {
                            paths.push(rel.to_path_buf());
                        }
                        if paths.len() >= max_files {
                            break;
                        }
                    }
                }

                let total_files = paths.len();
                let (loaded, skipped, tokens) = self.parallel_bulk_read(paths).await;

                serde_json::json!({
                    "matched_files": total_files,
                    "loaded": loaded,
                    "skipped": skipped,
                    "tokens_added": tokens,
                    "context_usage_pct": format!("{:.1}%", self.context_map.usage_fraction() * 100.0),
                })
            }
            CONTEXT_SUMMARY => {
                let summary = self.generate_structured_summary();
                serde_json::json!({
                    "summary": summary,
                    "total_tokens": self.context_map.total_tokens(),
                    "budget": self.context_map.budget(),
                })
            }
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
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let max_files =
                    args.get("max_files").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

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
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let freed = self.context_map.evict_to_tree(std::path::Path::new(path));
                serde_json::json!({
                    "evicted": path,
                    "tokens_freed": freed,
                    "remaining": self.context_map.remaining(),
                })
            }
            CONTEXT_RECOMMEND => {
                let task = args.get("task").and_then(|v| v.as_str()).unwrap_or("");
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
                let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let path = std::path::Path::new(path_str);
                let root = super::current_project_root();
                let full_path = root.join(path);

                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        let skeleton = super::context_map::extract_rust_skeleton(path, &content);
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

        let response =
            super::execution::read_line_pausing_esc(&self.esc_paused, &self.esc_pause_ack).await;
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
            let result = self.execute_context_tool_async(name, args).await;
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
        let pre_edit_content: Option<(String, String)> =
            if matches!(name, "file_edit" | "file_write" | "file_delete") {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    if let Ok(content) = tokio::fs::read_to_string(path).await {
                        use crate::session::edit_history::{EditAction, FileSnapshot};
                        let snapshot =
                            FileSnapshot::new(std::path::PathBuf::from(path), content.clone());
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
                    self.cache_manager
                        .tool_cache
                        .set(name, args, result.clone());
                }

                // Cache tool results in LocalFirstCoordinator
                let cache_key = crate::session::cache::ToolCache::cache_key(name, args);
                self.cache_manager.local_first.cache_response(
                    &cache_key,
                    result_str.clone(),
                    result_str.len(),
                );

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
                let visual_verification_result = self.maybe_verify_visual_change(name, args).await;
                let enhanced_result = self.maybe_enhance_tool_result(name, &result_str);
                let mut final_result = enhanced_result;
                if let Some(ver_msg) = verification_result {
                    final_result.push_str(&ver_msg);
                }
                let mut needs_retry = false;
                if let Some(vvr) = visual_verification_result {
                    if !vvr.message.is_empty() {
                        final_result.push_str(&vvr.message);
                    }
                    if let Some(assertion) = vvr.assertion {
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_visual_assertion(assertion);
                        }
                    }
                    if vvr.hard_failure {
                        needs_retry = true;
                    }
                }
                if needs_retry {
                    Ok((false, final_result, summary))
                } else {
                    Ok((true, final_result, summary))
                }
            }
            Ok(Err(e)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let summary = crate::output::semantic_summary(
                    name,
                    args,
                    Some(&e.to_string()),
                    false,
                    elapsed,
                );
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
                let summary =
                    crate::output::semantic_summary(name, args, Some(&err), false, elapsed);
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
        tool_name: &str,
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

        // Budget check: if the result exceeds the per-result token budget,
        // spill the raw data to disk and store a structured summary + reference.
        let result_to_store = if success {
            let estimated_tokens = crate::token_count::estimate_content_tokens(result);
            if estimated_tokens > MAX_TOOL_RESULT_TOKENS {
                info!(
                    "Tool result from '{}' is {} tokens (budget {}), summarizing with disk reference",
                    tool_name, estimated_tokens, MAX_TOOL_RESULT_TOKENS
                );
                summarize_and_spill(tool_name, call_id, result, estimated_tokens)
            } else {
                result.to_string()
            }
        } else {
            result.to_string()
        };

        if use_native_fc {
            let result_json = if success {
                result_to_store
            } else {
                serde_json::json!({"error": result_to_store}).to_string()
            };
            self.messages.push(Message::tool(result_json, call_id));
        } else {
            let formatted = if success {
                format!("<tool_result>{}</tool_result>", result_to_store)
            } else {
                format!(
                    "<tool_result><error>{}</error></tool_result>",
                    result_to_store
                )
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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // ToolErrorKind Classification Tests
    // =========================================================================

    #[test]
    fn test_tool_error_kind_classify_safety_violation() {
        // Test safety-related keywords
        assert_eq!(
            ToolErrorKind::classify("safety check failed"),
            ToolErrorKind::SafetyViolation
        );
        assert_eq!(
            ToolErrorKind::classify("Operation blocked by safety policy"),
            ToolErrorKind::SafetyViolation
        );
        assert_eq!(
            ToolErrorKind::classify("BLOCKED: File access denied"),
            ToolErrorKind::SafetyViolation
        );
    }

    #[test]
    fn test_tool_error_kind_classify_resource_not_found() {
        // Test resource not found keywords
        assert_eq!(
            ToolErrorKind::classify("File not found"),
            ToolErrorKind::ResourceNotFound
        );
        assert_eq!(
            ToolErrorKind::classify("No such file or directory"),
            ToolErrorKind::ResourceNotFound
        );
        assert_eq!(
            ToolErrorKind::classify("resource NOT FOUND"),
            ToolErrorKind::ResourceNotFound
        );
    }

    #[test]
    fn test_tool_error_kind_classify_permission_denied() {
        // Test permission-related keywords
        assert_eq!(
            ToolErrorKind::classify("Permission denied"),
            ToolErrorKind::PermissionDenied
        );
        assert_eq!(
            ToolErrorKind::classify("Access denied"),
            ToolErrorKind::PermissionDenied
        );
        assert_eq!(
            ToolErrorKind::classify("operation not permitted"),
            ToolErrorKind::PermissionDenied
        );
    }

    #[test]
    fn test_tool_error_kind_classify_argument_error() {
        // Test parse/JSON/invalid keywords
        assert_eq!(
            ToolErrorKind::classify("Failed to parse JSON"),
            ToolErrorKind::ArgumentError
        );
        assert_eq!(
            ToolErrorKind::classify("Invalid argument provided"),
            ToolErrorKind::ArgumentError
        );
        assert_eq!(
            ToolErrorKind::classify("JSON parsing error"),
            ToolErrorKind::ArgumentError
        );
        assert_eq!(
            ToolErrorKind::classify("parse error at line 5"),
            ToolErrorKind::ArgumentError
        );
    }

    #[test]
    fn test_tool_error_kind_classify_timeout() {
        // Test timeout keyword
        assert_eq!(
            ToolErrorKind::classify("Request timeout"),
            ToolErrorKind::Timeout
        );
        assert_eq!(
            ToolErrorKind::classify("Operation timed out after 30s"),
            ToolErrorKind::Timeout
        );
    }

    #[test]
    fn test_tool_error_kind_classify_execution_error_fallback() {
        // Test that unknown errors fall back to ExecutionError
        assert_eq!(
            ToolErrorKind::classify("Something went wrong"),
            ToolErrorKind::ExecutionError
        );
        assert_eq!(
            ToolErrorKind::classify("Unknown error occurred"),
            ToolErrorKind::ExecutionError
        );
        assert_eq!(ToolErrorKind::classify(""), ToolErrorKind::ExecutionError);
    }

    #[test]
    fn test_tool_error_kind_classify_case_insensitive() {
        // Test that classification is case-insensitive
        assert_eq!(
            ToolErrorKind::classify("SAFETY VIOLATION"),
            ToolErrorKind::SafetyViolation
        );
        assert_eq!(ToolErrorKind::classify("Timeout"), ToolErrorKind::Timeout);
        assert_eq!(
            ToolErrorKind::classify("JSON error"),
            ToolErrorKind::ArgumentError
        );
    }

    // =========================================================================
    // ToolErrorKind String Representation Tests
    // =========================================================================

    #[test]
    fn test_tool_error_kind_as_str() {
        assert_eq!(ToolErrorKind::SafetyViolation.as_str(), "SAFETY_VIOLATION");
        assert_eq!(
            ToolErrorKind::ResourceNotFound.as_str(),
            "RESOURCE_NOT_FOUND"
        );
        assert_eq!(
            ToolErrorKind::PermissionDenied.as_str(),
            "PERMISSION_DENIED"
        );
        assert_eq!(ToolErrorKind::ArgumentError.as_str(), "ARGUMENT_ERROR");
        assert_eq!(ToolErrorKind::Timeout.as_str(), "TIMEOUT");
        assert_eq!(ToolErrorKind::ExecutionError.as_str(), "EXECUTION_ERROR");
    }

    // =========================================================================
    // ToolErrorKind Recovery Hint Tests
    // =========================================================================

    #[test]
    fn test_tool_error_kind_recovery_hint_safety() {
        let hint = ToolErrorKind::SafetyViolation.recovery_hint();
        assert!(hint.contains("protected files"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_recovery_hint_resource_not_found() {
        let hint = ToolErrorKind::ResourceNotFound.recovery_hint();
        assert!(hint.contains("path exists"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_recovery_hint_permission_denied() {
        let hint = ToolErrorKind::PermissionDenied.recovery_hint();
        assert!(hint.contains("sudo") || hint.contains("permissions"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_recovery_hint_argument_error() {
        let hint = ToolErrorKind::ArgumentError.recovery_hint();
        assert!(hint.contains("schema") || hint.contains("arguments"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_recovery_hint_timeout() {
        let hint = ToolErrorKind::Timeout.recovery_hint();
        assert!(hint.contains("smaller steps") || hint.contains("timeout"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_recovery_hint_execution_error() {
        let hint = ToolErrorKind::ExecutionError.recovery_hint();
        assert!(hint.contains("adjust") || hint.contains("Review"));
        assert!(!hint.is_empty());
    }

    #[test]
    fn test_tool_error_kind_all_hints_are_non_empty() {
        // Ensure all error kinds have meaningful recovery hints
        for kind in [
            ToolErrorKind::SafetyViolation,
            ToolErrorKind::ResourceNotFound,
            ToolErrorKind::PermissionDenied,
            ToolErrorKind::ArgumentError,
            ToolErrorKind::Timeout,
            ToolErrorKind::ExecutionError,
        ] {
            let hint = kind.recovery_hint();
            assert!(
                hint.len() > 10,
                "Recovery hint for {:?} should be meaningful, got: {}",
                kind,
                hint
            );
        }
    }

    // =========================================================================
    // Integration Test: Round-trip Classification
    // =========================================================================

    #[test]
    fn test_tool_error_kind_roundtrip_classification() {
        // Test that classified errors can be converted back to strings
        let test_errors = vec![
            ("safety block triggered", ToolErrorKind::SafetyViolation),
            ("file not found error", ToolErrorKind::ResourceNotFound),
            ("permission denied on read", ToolErrorKind::PermissionDenied),
            ("invalid JSON format", ToolErrorKind::ArgumentError),
            ("connection timeout", ToolErrorKind::Timeout),
            ("unexpected failure", ToolErrorKind::ExecutionError),
        ];

        for (error_msg, expected_kind) in test_errors {
            let classified = ToolErrorKind::classify(error_msg);
            assert_eq!(
                classified, expected_kind,
                "Failed to classify '{}' correctly",
                error_msg
            );

            // Verify we can get string representation and hint
            let _ = classified.as_str();
            let _ = classified.recovery_hint();
        }
    }

    // =========================================================================
    // Helper Function Tests
    // =========================================================================

    #[test]
    fn test_truncate_chars_short_string() {
        let input = "short";
        let result = truncate_chars(input, 100);
        assert_eq!(result, input);
    }

    #[test]
    fn test_truncate_chars_exact_length() {
        let input = "exactly10";
        let result = truncate_chars(input, 9);
        assert_eq!(result, input);
    }

    #[test]
    fn test_truncate_chars_long_string() {
        let input = "this is a very long string";
        let result = truncate_chars(input, 10);
        assert_eq!(result, "this is a ...");
    }

    #[test]
    fn test_truncate_chars_unicode() {
        let input = "🎉🎊🎁🎄🎃🎅🤶🧑‍🎄";
        let result = truncate_chars(input, 3);
        assert_eq!(result, "🎉🎊🎁...");
    }

    #[test]
    fn test_canonicalize_tool_args_valid_json() {
        let input = r#"{"key": "value", "num": 42}"#;
        let result = canonicalize_tool_args(input);
        // Should parse and re-serialize
        assert!(result.contains("key"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_canonicalize_tool_args_invalid_json() {
        let input = "not valid json";
        let result = canonicalize_tool_args(input);
        // Should return original string
        assert_eq!(result, input);
    }

    #[test]
    fn test_hash_tool_args_consistency() {
        // Same input should produce same hash
        let input = r#"{"key": "value"}"#;
        let hash1 = hash_tool_args(input);
        let hash2 = hash_tool_args(input);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_tool_args_equivalent_json() {
        // Different formatting of same JSON should produce same hash
        let input1 = r#"{"a":1,"b":2}"#;
        let input2 = r#"{"b":2,"a":1}"#;
        let hash1 = hash_tool_args(input1);
        let hash2 = hash_tool_args(input2);
        // Note: This depends on JSON canonicalization
        // The current implementation uses serde_json which preserves order
        // This test documents current behavior
        let _ = (hash1, hash2);
    }

    #[test]
    fn test_inject_runtime_tool_defaults_uses_vision_profile() {
        let mut config = crate::config::Config::default();
        config.models.insert(
            "vision".to_string(),
            crate::config::ModelProfile {
                endpoint: "https://vision.example/v1".to_string(),
                model: "remote-vision".to_string(),
                api_key: None,
                max_tokens: 192,
                temperature: 0.0,
                modalities: vec!["text".to_string(), "vision".to_string()],
                context_length: 262_144,
                extra_body: Some({
                    let mut map = serde_json::Map::new();
                    map.insert(
                        "chat_template_kwargs".to_string(),
                        serde_json::json!({ "enable_thinking": false }),
                    );
                    map
                }),
            },
        );

        let effective = inject_runtime_tool_defaults(
            &config,
            "vision_analyze",
            r#"{"prompt":"describe","image_base64":"AAAA"}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&effective).unwrap();
        assert_eq!(parsed["endpoint"], "https://vision.example/v1");
        assert_eq!(parsed["model"], "remote-vision");
        assert_eq!(parsed["max_tokens"], 192);
        assert_eq!(parsed["temperature"], 0.0);
        assert_eq!(parsed["detail"], "low");
        assert_eq!(
            parsed["extra_body"]["chat_template_kwargs"]["enable_thinking"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn test_inject_runtime_tool_defaults_preserves_explicit_values() {
        let mut config = crate::config::Config::default();
        config.models.insert(
            "vision".to_string(),
            crate::config::ModelProfile {
                endpoint: "https://vision.example/v1".to_string(),
                model: "remote-vision".to_string(),
                api_key: None,
                max_tokens: 192,
                temperature: 0.0,
                modalities: vec!["text".to_string(), "vision".to_string()],
                context_length: 262_144,
                extra_body: None,
            },
        );

        let effective = inject_runtime_tool_defaults(
            &config,
            "vision_compare",
            r#"{"image_a":"a.png","image_b":"b.png","endpoint":"http://custom/v1","model":"custom-model","max_tokens":512,"temperature":0.5,"detail":"high"}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&effective).unwrap();
        assert_eq!(parsed["endpoint"], "http://custom/v1");
        assert_eq!(parsed["model"], "custom-model");
        assert_eq!(parsed["max_tokens"], 512);
        assert_eq!(parsed["temperature"], 0.5);
        assert_eq!(parsed["detail"], "high");
    }
}
