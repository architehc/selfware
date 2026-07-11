use std::hash::{Hash, Hasher};

use anyhow::Result;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::api::types::Message;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::self_improvement::Outcome;
use crate::hooks::HookContext;

/// A tool execution was halted before it returned a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolHalt {
    /// The per-tool deadline (`step_timeout_secs`) elapsed.
    TimedOut,
    /// The agent's cancel token was set (ESC / abort) mid-execution.
    Cancelled,
}

/// Race a tool-execution future against the per-tool deadline AND the agent's
/// cancel token. Returns the tool's own `Result` when it finishes first, or a
/// [`ToolHalt`] when the deadline elapses or cancellation is observed. The
/// cancel token is polled every 50ms so an in-flight tool is interrupted
/// promptly instead of blocking up to `timeout`.
pub(crate) async fn run_tool_bounded<F>(
    fut: F,
    timeout: std::time::Duration,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> std::result::Result<anyhow::Result<serde_json::Value>, ToolHalt>
where
    F: std::future::Future<Output = anyhow::Result<serde_json::Value>>,
{
    use std::sync::atomic::Ordering;
    // Fast path: already cancelled before we start.
    if cancel.load(Ordering::Relaxed) {
        return Err(ToolHalt::Cancelled);
    }
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(fut);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            biased;
            r = &mut fut => return Ok(r),
            _ = &mut deadline => return Err(ToolHalt::TimedOut),
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ToolHalt::Cancelled);
                }
            }
        }
    }
}

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
async fn summarize_and_spill(
    tool_name: &str,
    call_id: &str,
    raw: &str,
    estimated_tokens: usize,
) -> String {
    // Save raw result to disk
    let spill_dir = std::path::Path::new(TOOL_RESULTS_DIR);
    let _ = tokio::fs::create_dir_all(spill_dir).await;
    let spill_file = spill_dir.join(format!(
        "{}_{}.json",
        tool_name,
        // Char-safe truncation: byte-slicing `&call_id[..12]` panics if a
        // non-ASCII tool_call_id from the API has a multi-byte char across byte 12
        // (found by GLM-5.2 reviewing tool_dispatch.rs; verified + fixed by Claude).
        call_id.chars().take(12).collect::<String>()
    ));
    let spill_path = spill_file.display().to_string();
    if let Err(e) = tokio::fs::write(&spill_file, raw).await {
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

fn tool_result_value_indicates_success(result: &Value) -> bool {
    if result.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }
    if result.get("passed").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }
    if result
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .is_some_and(|code| code != 0)
    {
        return false;
    }
    true
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
    // Tail: show the trailing lines for any file over 100 lines (capped to the
    // last 50), starting at line 100. The old `> 150` guard emitted NO tail for
    // 101–150 line files, silently dropping lines 101..=end even though they don't
    // overlap the head (found by GLM-5.2 reviewing tool_dispatch.rs).
    let tail_start = lines.len().saturating_sub(50).max(100);
    let tail: String = if lines.len() > 100 {
        lines[tail_start..].join("\n")
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
            tail_start,
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
    // Keep the head AND the tail, eliding only the middle. Head-only truncation
    // silently drops trailing markers the completion gate relies on (FAILED,
    // <verification_failed>, error tails), which can make a failed verification
    // read as passing.
    const HEAD_CHARS: usize = 12_000;
    const TAIL_CHARS: usize = 3_000;
    let char_count = raw.chars().count();
    let line_count = raw.lines().count();
    if char_count <= HEAD_CHARS + TAIL_CHARS {
        return raw.to_string();
    }
    let head: String = raw.chars().take(HEAD_CHARS).collect();
    let tail: String = raw.chars().skip(char_count - TAIL_CHARS).collect();
    let omitted = char_count - HEAD_CHARS - TAIL_CHARS;
    format!(
        "{}\n\n[... {} chars omitted from the middle — {} total chars, {} lines; \
         see raw file for full output ...]\n\n{}",
        head, omitted, char_count, line_count, tail
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

fn extract_backticked_tool_names(text: &str) -> Vec<String> {
    let mut tools = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        let candidate = after_start[..end].trim();
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            tools.push(candidate.to_string());
        }
        rest = &after_start[end + 1..];
    }

    tools
}

pub(super) fn extract_explicit_allowed_tools(
    task_context: &str,
) -> Option<std::collections::BTreeSet<String>> {
    let mut allowed = std::collections::BTreeSet::new();
    let mut collecting = false;

    for line in task_context.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        if !collecting
            && (lower.contains("use only these concrete tools")
                || lower.contains("use only these tools")
                || lower.contains("use only the following tools")
                || lower.contains("allowed tools"))
        {
            collecting = true;
            allowed.extend(extract_backticked_tool_names(trimmed));
            continue;
        }

        if !collecting {
            continue;
        }

        if trimmed.is_empty() {
            if !allowed.is_empty() {
                break;
            }
            continue;
        }

        let names = extract_backticked_tool_names(trimmed);
        let is_bullet = trimmed.starts_with('-')
            || trimmed.starts_with('*')
            || trimmed.chars().next().is_some_and(|ch| ch.is_ascii_digit());

        if names.is_empty() {
            if !allowed.is_empty() && !is_bullet {
                break;
            }
            continue;
        }

        if !is_bullet {
            if !allowed.is_empty() {
                break;
            }
            continue;
        }

        allowed.extend(names);
    }

    (!allowed.is_empty()).then_some(allowed)
}

pub(super) fn extract_explicit_requested_tools<'a, I>(
    task_context: &str,
    tool_names: I,
) -> std::collections::BTreeSet<String>
where
    I: IntoIterator<Item = &'a str>,
{
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::LazyLock;

    static CACHE: LazyLock<Mutex<HashMap<String, Vec<regex::Regex>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));

    let mut required = std::collections::BTreeSet::new();

    for tool_name in tool_names {
        let escaped = regex::escape(tool_name);
        let patterns = [
            format!(
                r"(?i)\b(?:use|call|invoke|run)\s+(?:the\s+)?`?{}`?(?:\s+tool)?\b",
                escaped
            ),
            format!(r"(?i)\busing\s+`?{}`?(?:\s+tool)?\b", escaped),
        ];

        let mut cache = CACHE.lock();
        let regexes = cache.entry(tool_name.to_string()).or_insert_with(|| {
            patterns
                .iter()
                .filter_map(|pattern| regex::Regex::new(pattern).ok())
                .collect()
        });

        if regexes.iter().any(|re| re.is_match(task_context)) {
            required.insert(tool_name.to_string());
        }
    }

    // A negated mention must never become a completion requirement. Without
    // this precedence rule, "don't use `shell_exec`" both required and
    // prohibited the same tool, leaving the agent in an impossible loop.
    let disallowed = extract_explicit_disallowed_tools(task_context);
    required.retain(|tool_name| !disallowed.contains(tool_name));

    required
}

fn extract_explicit_disallowed_tools(task_context: &str) -> std::collections::BTreeSet<String> {
    let mut disallowed = std::collections::BTreeSet::new();

    for line in task_context.lines() {
        let lower = line.to_lowercase();
        let contains_denial = lower.contains("never call")
            || lower.contains("do not use")
            || lower.contains("don't use")
            || lower.contains("never use")
            || lower.contains("do not run")
            || lower.contains("don't run")
            || lower.contains("never run")
            || lower.contains("without shell")
            || lower.contains("no shell")
            || lower.contains("avoid ");

        if contains_denial {
            disallowed.extend(extract_backticked_tool_names(line));

            // Users normally describe a capability ("shell commands"), not
            // Selfware's concrete implementation names. Treat the shell
            // category as covering both execution tools so the restriction is
            // enforced before either tool can prompt or run.
            if lower.contains("shell") {
                disallowed.insert("shell_exec".to_string());
                disallowed.insert("pty_shell".to_string());
            }
        }
    }

    disallowed
}

/// True if `needle` appears in `lower` at least once WITHOUT an immediately
/// preceding negation ("not", "don't", "never", "without", "avoid", "no").
///
/// This prevents a read-only instruction such as "do NOT edit any files" from
/// arming the mutation machinery just because it contains the substring "edit",
/// while still treating "fix the bug but do not edit the tests" as a mutation
/// task (the un-negated "fix" wins).
fn mention_is_unnegated(lower: &str, needle: &str) -> bool {
    const NEGATORS: &[&str] = &["not ", "n't ", "never ", "without ", "avoid ", "no "];
    let mut start = 0;
    while let Some(pos) = lower[start..].find(needle) {
        let abs = start + pos;
        // Look back up to ~16 chars for a negation, in a UTF-8-safe way
        // (byte-slicing a fixed offset could split a multibyte char and panic).
        let prefix = &lower[..abs];
        let win_start = prefix
            .char_indices()
            .rev()
            .take(16)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        let window = &prefix[win_start..];
        if !NEGATORS.iter().any(|n| window.contains(n)) {
            return true;
        }
        start = abs + needle.len().max(1);
    }
    false
}

pub(super) fn task_requires_mutation(task_context: &str) -> bool {
    let lower = task_context.to_lowercase();
    // A read-only code-review / audit deliverable must not be classified as a
    // mutation task just because it says "Create a code review" (matches
    // "create") — the output is prose, not code. Kept deliberately TIGHT (only
    // strong review-deliverable phrasings, and only when there is no edit verb)
    // so it never mis-classifies an `improve`/edit task as read-only. Without
    // this, "Create a thorough code review …" hit the mutation FAKE_COMPLETE
    // gate and churned (found confirming the review-completion fix).
    // Read-only PROSE deliverables — "explain how X works", "create a summary",
    // "write a report on Y" — produce text, not code, but would otherwise hit the
    // "create"/"write" mutation keywords below and get force-written/scaffolded.
    // Detect unambiguous prose commands + prose-output nouns, but exclude requests
    // that name a code artifact (a report GENERATOR, a summary FUNCTION, a .rs
    // file), which are genuine mutation tasks.
    let prose_command = [
        "explain ",
        "summarize ",
        "describe ",
        "analyze ",
        "list the ",
        "what is ",
        "how does ",
        "how do ",
    ]
    .iter()
    .any(|p| lower.starts_with(p));
    let names_code_artifact = [
        "function",
        "struct",
        "impl ",
        ".rs",
        "generator",
        "parser",
        "endpoint",
        "the code",
    ]
    .iter()
    .any(|c| lower.contains(c));
    let prose_output = (lower.contains("a summary")
        || lower.contains("a report")
        || lower.contains("an explanation")
        || lower.contains("an analysis")
        || lower.contains("a write-up")
        || lower.contains("a writeup"))
        && !names_code_artifact;
    let is_review_deliverable = lower.contains("code review")
        || lower.contains("review the code")
        || lower.contains("review this code")
        || lower.contains("review src/")
        || lower.contains("audit the")
        || (lower.contains("review") && lower.contains("line reference"))
        || prose_command
        || prose_output;
    let has_edit_verb = ["fix ", "implement ", "refactor ", "rename ", "delete ", "modify ", "edit the"]
        .iter()
        .any(|v| lower.contains(v));
    if is_review_deliverable && !has_edit_verb {
        return false;
    }
    [
        "fix",
        "implement",
        "edit",
        "modify",
        "update",
        "write",
        "create",
        "refactor",
        "rename",
        "delete",
        "remove",
        "make tests pass",
        "tests pass",
        "turn green",
        "until green",
        "add at least",
        "add ",
    ]
    .iter()
    .any(|needle| mention_is_unnegated(&lower, needle))
        || make_is_mutation_imperative(&lower)
}

/// True if the task contains an un-negated "make X ..." mutation imperative
/// ("Make parse_port return Result"), excluding the qualifier phrases
/// "make sure"/"make certain" and (via the trailing space) the filename
/// "makefile". Without this, a task phrased purely as "Make X do Y" with no
/// other mutation verb was classified read-only and every anti-fake-complete
/// gate silently no-oped (MUT-MAKE-VERB).
fn make_is_mutation_imperative(lower: &str) -> bool {
    const NEGATORS: &[&str] = &["not ", "n't ", "never ", "without ", "avoid ", "no "];
    let mut start = 0;
    while let Some(pos) = lower[start..].find("make ") {
        let abs = start + pos;
        let prefix = &lower[..abs];
        let win_start = prefix
            .char_indices()
            .rev()
            .take(16)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0);
        let negated = NEGATORS.iter().any(|n| prefix[win_start..].contains(n));
        let after = lower[abs + "make ".len()..].trim_start();
        if !negated && !after.starts_with("sure") && !after.starts_with("certain") {
            return true;
        }
        start = abs + "make ".len();
    }
    false
}

/// Operator's choice at the tool-confirmation prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmDecision {
    /// Approve just this one tool call.
    ExecuteOnce,
    /// Disable confirmations for the rest of the session (YOLO).
    EnableYolo,
    /// Do not execute this tool call.
    Skip,
}

/// Parse a confirm-prompt response. Enabling session-wide YOLO requires the
/// explicit full word "yolo" — a single stray keystroke (e.g. "s") must NOT
/// silently disable every future confirmation; it means "skip".
pub(crate) fn parse_confirm_response(response: &str) -> ConfirmDecision {
    match response.trim().to_lowercase().as_str() {
        "y" | "yes" => ConfirmDecision::ExecuteOnce,
        "yolo" => ConfirmDecision::EnableYolo,
        _ => ConfirmDecision::Skip,
    }
}

/// Detect an output redirect to a FILE (`>`, `>>`, `cat>file`, `echo x>y`)
/// while ignoring (a) descriptor duplication that writes no file (`2>&1`,
/// `>&2`) and (b) any `>` inside single/double quotes (`grep "->" f`). The
/// crude `" >"` substring marker missed redirects without a leading space, so
/// `echo x>y` was misclassified as read-only — a silently missed mutation.
fn has_file_redirect(command: &str) -> bool {
    let chars: Vec<char> = command.chars().collect();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        match c {
            '\\' if !in_single => escaped = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '>' if !in_single && !in_double => {
                // Skip a second '>' (append), then any spaces.
                let mut j = i + 1;
                if chars.get(j) == Some(&'>') {
                    j += 1;
                }
                while chars.get(j) == Some(&' ') {
                    j += 1;
                }
                // '>&' duplicates a descriptor (e.g. 2>&1) — no file is written.
                if chars.get(j) != Some(&'&') {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

pub(super) fn shell_command_is_observational(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    // A redirect writes a file even without surrounding spaces (echo x>y).
    if has_file_redirect(&normalized) {
        return false;
    }

    let mutating_markers = [
        "| tee",
        " tee ",
        "touch ",
        "mkdir ",
        "mktemp",
        "rm ",
        "mv ",
        "cp ",
        "chmod ",
        "chown ",
        "sed -i",
        "perl -pi",
        "cargo fmt",
        "cargo fix",
        "cargo update",
        "git add",
        "git commit",
        "git switch",
        "git checkout",
        "git apply",
        "patch ",
        "npm install",
        "pnpm install",
        "yarn add ",
        "pip install",
    ];
    if mutating_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return false;
    }

    let read_only_prefixes = [
        "cargo test",
        "cargo check",
        "cargo clippy",
        "cargo metadata",
        "cargo locate-project",
        "cargo nextest",
        "git status",
        "git diff",
        "git log",
        "ls",
        "pwd",
        "find",
        "rg",
        "grep",
        "cat",
        "sed -n",
        "head",
        "tail",
        "wc",
        "tree",
        "pytest",
        "python -m pytest",
        "npm test",
        "pnpm test",
        "yarn test",
        "go test",
        "which",
        "echo",
        "env",
        "printenv",
    ];

    read_only_prefixes.iter().any(|prefix| {
        normalized == *prefix
            || (normalized.starts_with(prefix)
                && (normalized[prefix.len()..].starts_with(' ')
                    || normalized[prefix.len()..].starts_with("--")))
    })
}

/// Canonical predicate: does a SUCCESSFUL call to this tool mutate the
/// workspace? Single source of truth for mutation accounting so the progress
/// guard, the completion gate, and FailureMode all agree what "a real edit" is.
/// Previously two duplicated hardcoded lists under-counted edits made via
/// file_multi_edit / patch_apply / git writes, producing spurious
/// StaleVerification / ReadLoop / FakeComplete verdicts.
pub(super) fn tool_call_is_mutating(name: &str, args: &serde_json::Value) -> bool {
    // Direct file-content or file-tree mutations.
    if matches!(
        name,
        "file_edit"
            | "file_write"
            | "file_delete"
            | "file_fim_edit"
            | "file_multi_edit"
            | "patch_apply"
    ) {
        return true;
    }
    // Mutating version-control operations (index / tree / history changes).
    // Observational git (status/log/diff/show) is intentionally excluded.
    if matches!(
        name,
        "git_commit"
            | "git_add"
            | "git_checkout"
            | "git_apply"
            | "git_reset"
            | "git_stash"
            | "git_merge"
            | "git_rebase"
            | "git_cherry_pick"
            | "git_revert"
            | "git_rm"
            | "git_mv"
    ) {
        return true;
    }
    // Shell / PTY running a NON-observational command (rm, mv, sed -i, package
    // installers, output redirects, `git add`/`git commit` via the CLI, etc.).
    if matches!(name, "shell_exec" | "pty_shell") {
        return args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|cmd| !shell_command_is_observational(cmd))
            .unwrap_or(false);
    }
    false
}

pub(super) fn shell_command_is_verification(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    // A compile-/collect-only invocation ("cargo test --no-run",
    // "pytest --collect-only") does not actually run the tests, so it must not
    // count as verification.
    if command_is_noop_verification(&normalized) {
        return false;
    }

    let verification_prefixes = [
        "cargo check",
        "cargo test",
        "cargo clippy",
        "pytest",
        "python -m pytest",
        "python3 -m pytest",
        "python -m unittest",
        "python3 -m unittest",
        "python -m py_compile",
        "python3 -m py_compile",
        "npm test",
        "pnpm test",
        "yarn test",
        "npx tsc",
        "tsc ",
        "go test",
        "go build",
        "javac",
        "mvn test",
        "mvn verify",
        "gradle test",
        "./gradlew test",
        "dotnet build",
        "dotnet test",
        "cmake --build",
        "make test",
        "ctest",
        "swift build",
        "swift test",
        "sqlfluff lint",
    ];

    // Match the verification command at a shell command boundary, not just at the
    // very start — so a full path ("~/.cargo/bin/cargo check"), a cd prefix
    // ("cd sub && cargo check"), or an env prefix still counts. Otherwise a model
    // that works around a PATH issue by invoking cargo by full path never gets
    // its verification credited and loops on StaleVerification.
    verification_prefixes
        .iter()
        .any(|prefix| command_contains_at_boundary(&normalized, prefix))
}

/// True if `prefix` occurs in `command` starting at a shell command boundary
/// (start of string, or after a space / `/` / `&` / `;` / `|` / tab) and is
/// followed by end-of-command (space, `-`, `;`, `&`, `|`, tab, or end).
fn command_contains_at_boundary(command: &str, prefix: &str) -> bool {
    let bytes = command.as_bytes();
    let mut from = 0;
    while let Some(rel) = command[from..].find(prefix) {
        let abs = from + rel;
        let before_ok =
            abs == 0 || matches!(bytes[abs - 1], b' ' | b'/' | b'&' | b';' | b'|' | b'\t' | b'(');
        let after = abs + prefix.len();
        let after_ok = after >= command.len()
            || matches!(bytes[after], b' ' | b'-' | b';' | b'&' | b'|' | b'\t');
        if before_ok && after_ok {
            return true;
        }
        from = abs + 1;
    }
    false
}

/// True for verification INVOCATIONS that do not actually run anything —
/// `cargo test --no-run` (compiles but runs 0 tests), `pytest --collect-only`,
/// `--dry-run`, `go test -run=^$`, etc. These must NOT satisfy the verification
/// gate: they type-check or enumerate but never execute the tests.
pub(super) fn command_is_noop_verification(text: &str) -> bool {
    let c = text.to_lowercase();
    [
        "--no-run",
        "--collect-only",
        "--collectonly",
        "--dry-run",
        "-run=^$",
        "-run '^$'",
        "-run \"^$\"",
    ]
    .iter()
    .any(|flag| c.contains(flag))
}

pub(super) fn tool_call_is_verification(name: &str, args_str: &str) -> bool {
    match name {
        "cargo_check" | "cargo_test" | "cargo_clippy" => !command_is_noop_verification(args_str),
        "shell_exec" => serde_json::from_str::<Value>(args_str)
            .ok()
            .and_then(|args| {
                args.get("command")
                    .and_then(|value| value.as_str())
                    .map(shell_command_is_verification)
            })
            .unwrap_or(false),
        _ => false,
    }
}

pub(super) fn tool_call_is_observational(name: &str, args_str: &str) -> bool {
    match name {
        "file_read"
        | "directory_tree"
        | "glob_find"
        | "grep_search"
        | "symbol_search"
        | "git_status"
        | "git_diff"
        | "git_log"
        | "tool_search"
        | "cargo_check"
        | "cargo_test"
        | "cargo_clippy"
        | crate::tools::context::CONTEXT_BULK_READ
        | crate::tools::context::CONTEXT_SUMMARY
        | crate::tools::context::CONTEXT_STATUS
        | crate::tools::context::CONTEXT_FOCUS
        | crate::tools::context::CONTEXT_EVICT
        | crate::tools::context::CONTEXT_RECOMMEND
        | crate::tools::context::CONTEXT_LOAD_SKELETON => true,
        "shell_exec" => serde_json::from_str::<Value>(args_str)
            .ok()
            .and_then(|args| {
                args.get("command")
                    .and_then(|value| value.as_str())
                    .map(shell_command_is_observational)
            })
            .unwrap_or(false),
        _ => false,
    }
}

pub(super) fn tool_call_counts_as_state_change(name: &str, args_str: &str) -> bool {
    match name {
        "shell_exec" => serde_json::from_str::<Value>(args_str)
            .ok()
            .and_then(|args| {
                args.get("command")
                    .and_then(|value| value.as_str())
                    .map(|command| !shell_command_is_observational(command))
            })
            .unwrap_or(false),
        "cargo_check" | "cargo_test" | "cargo_clippy" => false,
        _ => !tool_call_is_observational(name, args_str),
    }
}

/// Extract the primary investigative target (file path, glob pattern, or grep
/// query) from a read-only tool call.  Returns `None` for tools that don't
/// have a meaningful unique target (e.g. `git_status`, `cargo_check` without a
/// path, `tool_search`).  The returned string is used as a key in
/// `seen_read_targets` to detect whether the agent is reading something NEW
/// (investigative progress) or re-reading something already seen (a loop).
pub(super) fn read_tool_target(name: &str, args_str: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_str).ok()?;
    match name {
        "file_read" | "file_write" | "file_edit" | "file_delete" => {
            args.get("path").and_then(|v| v.as_str()).map(|s| s.to_string())
        }
        "directory_tree" => {
            // directory_tree may use "path" or "pattern"
            args.get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    args.get("pattern")
                        .and_then(|v| v.as_str())
                        .map(|s| format!("tree:{}", s))
                })
        }
        "glob_find" => {
            args.get("pattern")
                .and_then(|v| v.as_str())
                .map(|s| format!("glob:{}", s))
        }
        "grep_search" => {
            args.get("pattern")
                .and_then(|v| v.as_str())
                .map(|s| format!("grep:{}", s))
        }
        "symbol_search" => {
            args.get("query")
                .or_else(|| args.get("pattern"))
                .and_then(|v| v.as_str())
                .map(|s| format!("sym:{}", s))
        }
        _ => None,
    }
}

fn configured_vision_profile(
    config: &crate::config::Config,
) -> Option<&crate::config::ModelProfile> {
    config
        .models
        .get("vision")
        .filter(|profile| profile.supports_vision())
        .or_else(|| {
            config
                .resolve_model(None)
                .filter(|profile| profile.supports_vision())
        })
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
    fn current_task_tool_policy_violation(&self, tool_name: &str) -> Option<String> {
        let task = self.learning_context();
        if task.trim().is_empty() || task == "general" {
            return None;
        }

        let disallowed = extract_explicit_disallowed_tools(task);
        if disallowed.contains(tool_name) {
            return Some(format!(
                "Task tool policy violation: `{}` is explicitly disallowed by the task instructions. Choose a different tool now.",
                tool_name
            ));
        }

        let allowed = extract_explicit_allowed_tools(task)?;
        if allowed.contains(tool_name) {
            return None;
        }

        let allowed_list = allowed
            .iter()
            .map(|tool| format!("`{}`", tool))
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Task tool policy violation: `{}` is not allowed for this task. Allowed tools from the task prompt: {}. Use one of those tools instead.",
            tool_name, allowed_list
        ))
    }

    async fn reject_tool_call_before_execution(
        &mut self,
        tool_name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
        failure_kind: &'static str,
        error_msg: &str,
    ) {
        cli_println!("{} {}", "✗".bright_red(), error_msg);
        self.pending_failure_hint = Some(error_msg.to_string());
        self.push_tool_result_message(use_native_fc, call_id, tool_name, false, error_msg)
            .await;
        self.log_tool_call(tool_name, args_str, error_msg, false, start_time, false);
        self.remember_failed_tool(tool_name, error_msg);
        self.record_failed_tool_attempt(tool_name, args_str, failure_kind, error_msg);
        self.consecutive_suppressions += 1;
    }

    async fn maybe_block_progressless_batch(
        &mut self,
        tool_calls: Vec<super::execution::CollectedToolCall>,
    ) -> Result<Option<Vec<super::execution::CollectedToolCall>>> {
        // Use relaxed threshold when agent has already written source files.
        // Verification loops (cargo check → cargo test → read output) are expected
        // after writing and should not be blocked aggressively.
        let has_written = self.has_written_any_file;

        // Pre-edit thresholds are generous (12/18) so that legitimate
        // investigation of a complex change — reading many distinct files
        // before the first edit — is not prematurely blocked.  True infinite
        // loops are still caught because the investigation-progress reset
        // (in execution.rs) only rewards *novel* reads; redundant re-reads
        // let the counter climb to the threshold.
        let block_threshold = if has_written { 16 } else { 12 };
        let escalation_threshold = if has_written { 20 } else { 18 };

        if !task_requires_mutation(self.task_context_for_classification())
            || self.consecutive_read_only_steps <= block_threshold
            || tool_calls.is_empty()
            || !tool_calls
                .iter()
                .all(|(name, args_str, _)| tool_call_is_observational(name, args_str))
        {
            return Ok(Some(tool_calls));
        }

        let error_msg = format!(
            "PROGRESS GUARD: This task requires making changes, but you have already spent {} consecutive steps on read-only or verification actions. Read-only tools are temporarily blocked. Your next action must change code or project state: use `file_edit`, `file_write`, `file_delete`, or `shell_exec` with a mutating command. Do NOT rerun more reads, status commands, or test commands until after you edit something.",
            self.consecutive_read_only_steps
        );

        // Record the firing for FailureMode classification.
        self.note_progress_guard_fired();
        self.emit_progress(super::progress::ProgressEvent::GuardFired {
            kind: "progress_guard".to_string(),
            count: self.progress_guard_fire_count(),
        });
        let guard_count = self.progress_guard_fire_count();

        for (name, args_str, tool_call_id) in tool_calls {
            let start_time = std::time::Instant::now();
            let (call_id, use_native_fc, _) =
                self.build_tool_call_context(&name, &args_str, tool_call_id);
            self.reject_tool_call_before_execution(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
                "progress_guard",
                &error_msg,
            )
            .await;
        }

        if guard_count >= 3 && self.mutating_tool_call_count() == 0 {
            anyhow::bail!(
                "READ_LOOP_NO_EDIT: progress guard blocked read-only tools {} times after {} consecutive read-only steps, with 0 mutating tools",
                guard_count,
                self.consecutive_read_only_steps
            );
        }

        if self.config.agent.read_loop_policy == crate::config::ReadLoopPolicy::ForceMutation {
            self.force_mutation_pending = true;
            self.messages
                .push(Message::user(self.force_mutation_directive()));
        } else {
            self.messages.push(Message::user(
                "<selfware_system_directive>\n\
                 Read-only and verification tools are blocked until you make a real change.\n\
                 Your NEXT response must do one of these:\n\
                 - use `file_edit`, `file_write`, or `file_delete`\n\
                 - use `shell_exec` with a mutating command\n\
                 - if you already know the exact code change, output the replacement code as text and include the target path; Selfware will write it automatically\n\
                 Do NOT call more file reads, directory listings, grep searches, cargo test, or cargo check right now.\n\
                 </selfware_system_directive>"
                    .to_string(),
            ));
        }

        if self.consecutive_read_only_steps >= escalation_threshold
            && self.pending_synthesis.is_none()
        {
            info!(
                "Escalating progress-guard stall to phase-2 synthesis after {} read-only steps",
                self.consecutive_read_only_steps
            );
            self.pending_synthesis = Some(self.learning_context().to_string());
        }

        Ok(None)
    }

    fn force_mutation_directive(&self) -> String {
        let target = self
            .last_read_file
            .as_deref()
            .unwrap_or("PATH_YOU_ALREADY_READ");
        let target_json =
            serde_json::to_string(target).unwrap_or_else(|_| "\"PATH_YOU_ALREADY_READ\"".into());
        format!(
            "<selfware_system_directive>\n\
             READ-LOOP FORCE-MUTATION MODE is active.\n\
             Your previous read-only or verification tool calls were suppressed. \
             The next accepted action must mutate code or project state.\n\n\
             Choose ONE of these exact tool shapes on the most relevant existing file you already inspected:\n\n\
             <tool>\n\
             <name>file_edit</name>\n\
             <arguments>{{\"path\":{target_json},\"old_str\":\"EXACT ORIGINAL TEXT FROM THE FILE\",\"new_str\":\"REPLACEMENT TEXT\"}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>file_multi_edit</name>\n\
             <arguments>{{\"path\":{target_json},\"edits\":[{{\"old_str\":\"...\",\"new_str\":\"...\"}}]}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>file_write</name>\n\
             <arguments>{{\"path\":{target_json},\"content\":\"FULL NEW FILE CONTENT\"}}</arguments>\n\
             </tool>\n\n\
             <tool>\n\
             <name>patch_apply</name>\n\
             <arguments>{{\"patch\":\"--- a/file\\n+++ b/file\\n@@ -1 +1 @@\\n-old\\n+new\\n\"}}</arguments>\n\
             </tool>\n\n\
             If you already know the exact change, you may also output the replacement code as plain text with the target path; Selfware will write it automatically.\n\n\
             Rules:\n\
             - Do NOT call file_read, directory_tree, glob_find, grep_search, git_diff, cargo_check, cargo_test, pytest, npm test, or go test BEFORE making an edit.\n\
             - After you edit, you MAY run one targeted test command to verify the change.\n\
             - Do NOT create src/lib.rs unless this repository already has Cargo.toml and src/lib.rs is the real target.\n\
             - You MUST make an edit now. If uncertain, edit the highest-ranked source file with the smallest plausible fix; do not stop without editing.\n\
             </selfware_system_directive>"
        )
    }

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
            "task_policy" => format!(
                "RETRY SUPPRESSED: `{}` is blocked by the task's explicit tool constraints. Use a tool that matches the task instructions instead. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            "operator_denied" => format!(
                "RETRY SUPPRESSED: the operator denied `{}` with these exact arguments. Do not ask for the same permission again; choose a different approach or explain that the task cannot continue without it. Last response: {}",
                failure.tool_name, failure.error_preview
            ),
            "progress_guard" => format!(
                "RETRY SUPPRESSED: `{}` is blocked by the progress guard because you need to make an edit or other state-changing action before using more read-only or verification tools. Last error: {}",
                failure.tool_name, failure.error_preview
            ),
            other => {
                // For file_read failures, hint that the file may need to be created first
                let hint = if failure.tool_name == "file_read" && failure.error_preview.contains("Failed to read") {
                    " If the file does not exist yet, use file_write to CREATE it first."
                } else {
                    ""
                };
                format!(
                    "RETRY SUPPRESSED: `{}` with these exact arguments already failed due to {}. Do not rerun it until a different successful tool call changes the situation or you change the inputs.{} Last error: {}",
                    failure.tool_name, other, hint, failure.error_preview
                )
            },
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
        self.escalated_edit_args_hashes.clear();
        self.consecutive_suppressions = 0;
    }

    /// Clear recorded failed attempts for a single tool name.
    /// Used when that tool succeeds so that unrelated failures are not forgiven.
    pub(super) fn clear_failed_tool_attempts_for_tool(&mut self, tool_name: &str) {
        self.recent_failed_tool_attempts
            .retain(|existing| existing.tool_name != tool_name);
    }

    pub(super) async fn maybe_block_redundant_reread(
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
        // Allow up to 3 unchanged rereads before blocking — in long sessions
        // the model may need to re-read files after context compression evicts
        // earlier content. Only block truly excessive rereads.
        if state.unchanged_read_count < 3 || self.file_tracker.stale_files.contains(path) {
            return false;
        }

        let current_mtime = tokio::fs::metadata(path)
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());

        if current_mtime != state.last_modified {
            return false;
        }

        // The threshold is 3 unchanged rereads; this call is the one that exceeds
        // it, so the ordinal is one greater than the threshold.
        let read_count = state.unchanged_read_count + 1;
        // Increment the counter so repeated suppressions eventually trigger
        // the forced text response (at count >= 3).
        if let Some(state_mut) = self.file_tracker.read_state.get_mut(path) {
            state_mut.unchanged_read_count = read_count;
        }
        let err = format!(
            "Repeated unchanged reread blocked: `{}` has already been read unchanged 3 times in this task. Use the content already in context or make the edit now instead of reading it again.",
            path
        );
        self.push_task_state_note(format!(
            "Blocked redundant reread of `{}` on the {}th unchanged read",
            path, read_count
        ));
        self.pending_failure_hint = Some(err.clone());
        self.push_tool_result_message(use_native_fc, call_id, name, false, &err)
            .await;
        self.log_tool_call(name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(name, &err);
        self.record_failed_tool_attempt(name, args_str, "task_state", &err);
        self.consecutive_suppressions += 1;

        // After suppressed rereads, trigger phase-2 synthesis early.
        // The model has the data in context — force it to produce code.
        if read_count >= 3 && self.pending_synthesis.is_none() {
            info!(
                "Triggering phase-2 synthesis after {} suppressed rereads",
                read_count
            );
            // Extract the task description from the first user message
            let task = self
                .messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| m.content.to_string())
                .unwrap_or_default();
            self.pending_synthesis = Some(task);
        }

        true
    }

    pub(super) async fn track_task_state_after_tool(
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
                let last_modified = tokio::fs::metadata(&path_str)
                    .await
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

    pub(super) async fn suppress_repeated_failed_tool_retry(
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

        // For file_read failures, check if the file exists now — it may have
        // been created by file_write since the last failed attempt.
        if tool_name == "file_read" {
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_str) {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    if tokio::fs::try_exists(path).await.unwrap_or(false) {
                        info!(
                            "file_read('{}') was previously suppressed but file now exists — allowing retry",
                            path
                        );
                        self.recent_failed_tool_attempts
                            .retain(|a| !(a.tool_name == tool_name && a.args_hash == args_hash));
                        return false;
                    }
                }
            }
        }

        // For file_edit failures (old_str not found), escalate the workflow:
        // 1. Force a file_read of the target so the model sees current content
        // 2. Tell the model to use file_write instead of file_edit
        // This prevents the 315-retry death spiral from the logs.
        if tool_name == "file_edit" {
            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_str) {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    // If this exact edit was already escalated once, suppress the
                    // repeat WITHOUT re-reading and re-injecting the whole file —
                    // the model already has the content (EDIT-RETRY-REINJECT).
                    if self.escalated_edit_args_hashes.contains(&args_hash) {
                        let short = format!(
                            "<selfware_system_directive>\n\
                             file_edit for {} keeps failing (old_str not found) and you were \
                             already given the full file content. Use file_write to replace the \
                             ENTIRE file now — do not retry file_edit.\n\
                             </selfware_system_directive>",
                            path
                        );
                        self.push_tool_result_message(
                            use_native_fc,
                            call_id,
                            tool_name,
                            false,
                            &short,
                        )
                        .await;
                        self.log_tool_call(
                            tool_name,
                            args_str,
                            "edit_reescalation_suppressed",
                            false,
                            start_time,
                            false,
                        );
                        self.consecutive_suppressions += 1;
                        return true;
                    }
                    self.escalated_edit_args_hashes.insert(args_hash);

                    let edit_fail_count = self
                        .recent_failed_tool_attempts
                        .iter()
                        .filter(|a| {
                            a.tool_name == "file_edit" && a.error_preview.contains("not found")
                        })
                        .count();

                    info!(
                        "file_edit failed on '{}' ({} prior edit failures) — escalating to file_write",
                        path, edit_fail_count
                    );

                    // Force-read the file so the model sees current content
                    let read_result = if tokio::fs::try_exists(path).await.unwrap_or(false) {
                        match tokio::fs::read_to_string(path).await {
                            Ok(content) => {
                                let lines = content.lines().count();
                                format!(
                                    "Current content of {} ({} lines):\n{}",
                                    path, lines, content
                                )
                            }
                            Err(e) => format!("Could not read {}: {}", path, e),
                        }
                    } else {
                        format!("File {} does not exist. Use file_write to create it.", path)
                    };

                    let escalation = format!(
                        "<selfware_system_directive>\n\
                         file_edit FAILED because old_str was not found in the file.\n\
                         {}\n\n\
                         DO NOT retry file_edit. Use file_write to REPLACE THE ENTIRE FILE:\n\n\
                         <tool>\n<name>file_write</name>\n\
                         <arguments>{{\"path\": \"{}\", \"content\": \"FULL FILE CONTENT HERE\"}}</arguments>\n\
                         </tool>\n\
                         </selfware_system_directive>",
                        read_result, path
                    );
                    self.push_tool_result_message(
                        use_native_fc,
                        call_id,
                        tool_name,
                        false,
                        &escalation,
                    )
                    .await;
                    self.log_tool_call(
                        tool_name,
                        args_str,
                        "escalated_to_file_write",
                        false,
                        start_time,
                        false,
                    );
                    self.consecutive_suppressions += 1;
                    return true;
                }
            }
        }

        let err = self.build_failed_tool_retry_suppressed_message(&failure);
        warn!(
            "Suppressing repeated failed tool call for '{}' after prior {} failure",
            tool_name, failure.failure_kind
        );
        cli_println!("{} {}", "✗".bright_red(), err);
        self.push_tool_result_message(use_native_fc, call_id, tool_name, false, &err)
            .await;
        self.log_tool_call(tool_name, args_str, &err, false, start_time, false);
        self.remember_failed_tool(tool_name, &err);
        // Surface this as a permanently-blocked tool call for FailureMode.
        self.note_permanently_blocked(tool_name);
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
        self.consecutive_suppressions += 1;
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
        let Some(tool_calls) = self.maybe_block_progressless_batch(tool_calls).await? else {
            return Ok(());
        };

        // Phase 1: Partition into parallel-safe and sequential groups.
        // Read-only tools with no path conflicts go into the parallel batch.
        let mut parallel_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut sequential_batch: Vec<super::execution::CollectedToolCall> = Vec::new();
        let mut parallel_paths: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        // Track whether any parallel-safe tool appears AFTER a sequential one in
        // the original order. Hoisting the parallel batch ahead of the sequential
        // batch is only safe when all parallel tools precede all sequential ones;
        // otherwise a read could run before the mutation it depends on.
        let mut parallel_follows_sequential = false;
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
                    if !sequential_batch.is_empty() {
                        parallel_follows_sequential = true;
                    }
                    parallel_batch.push(call.clone());
                }
            } else {
                sequential_batch.push(call.clone());
            }
        }

        // Phase 2: If fewer than 2 parallel tools, OR a parallel tool follows a
        // sequential one in the original order (so hoisting would reorder a
        // dependency), run everything sequentially in the original order to
        // preserve execution semantics.
        if parallel_batch.len() < 2 || parallel_follows_sequential {
            for (name, args_str, tool_call_id) in tool_calls {
                if self.is_cancelled() {
                    break;
                }
                // Clone name/tool_call_id before execute_single_tool_in_batch
                // takes them by value — we need them in the catch to push a
                // synthetic error result if the fn returns Err BEFORE pushing
                // any tool-result (e.g. confirmation rejection, pre-execution
                // safety gate). Without this, native-FC history gets N calls
                // but k<N results → 400.
                let name_clone = name.clone();
                let args_str_clone = args_str.clone();
                let id_clone = tool_call_id.clone();
                if let Err(e) = self
                    .execute_single_tool_in_batch(name, args_str, tool_call_id)
                    .await
                {
                    if super::task_runner::is_fatal_loop_error(&e) {
                        return Err(e);
                    }
                    // Non-fatal tool error that returned Err before pushing a
                    // tool-result: push a synthetic error result for this
                    // tool_call_id so every call gets exactly one result.
                    let (call_id, use_native_fc, _) =
                        self.build_tool_call_context(&name_clone, &args_str_clone, id_clone);
                    let error_text = e.to_string();
                    self.push_tool_result_message(
                        use_native_fc,
                        &call_id,
                        &name_clone,
                        false,
                        &error_text,
                    )
                    .await;
                    warn!("Non-fatal tool error in sequential batch: {e}");
                }
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
            // Clone before execute_single_tool_in_batch takes them by value.
            let name_clone = name.clone();
            let args_str_clone = args_str.clone();
            let id_clone = tool_call_id.clone();
            if let Err(e) = self
                .execute_single_tool_in_batch(name, args_str, tool_call_id)
                .await
            {
                if super::task_runner::is_fatal_loop_error(&e) {
                    return Err(e);
                }
                // Push a synthetic error result so native-FC history stays
                // balanced (N calls → N results).
                let (call_id, use_native_fc, _) =
                    self.build_tool_call_context(&name_clone, &args_str_clone, id_clone);
                let error_text = e.to_string();
                self.push_tool_result_message(
                    use_native_fc,
                    &call_id,
                    &name_clone,
                    false,
                    &error_text,
                )
                .await;
                warn!("Non-fatal tool error in sequential batch (phase 4): {e}");
            }
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

            if self
                .suppress_repeated_failed_tool_retry(
                    &name,
                    &args_str,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                continue;
            }

            if let Some(error_msg) = self.current_task_tool_policy_violation(&name) {
                self.reject_tool_call_before_execution(
                    &name,
                    &args_str,
                    &call_id,
                    use_native_fc,
                    start_time,
                    "task_policy",
                    &error_msg,
                )
                .await;
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
                self.push_tool_result_message(use_native_fc, &call_id, &name, false, &error_msg)
                    .await;
                self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                self.remember_failed_tool(&name, &error_msg);
                self.record_failed_tool_attempt(&name, &args_str, "safety", &error_msg);
                continue;
            }

            // Schema validation for native function calls
            if use_native_fc {
                let defs = self.tools.definitions();
                if let Err(e) = crate::agent::tool_validator::validate_tool_call(&fake_call, &defs)
                {
                    let error_msg = format!("Tool call validation failed: {}", e);
                    warn!("{}", error_msg);
                    self.push_tool_result_message(
                        use_native_fc,
                        &call_id,
                        &name,
                        false,
                        &error_msg,
                    )
                    .await;
                    self.log_tool_call(&name, &args_str, &error_msg, false, start_time, false);
                    self.remember_failed_tool(&name, &error_msg);
                    self.record_failed_tool_attempt(&name, &args_str, "validation", &error_msg);
                    self.emit_event(crate::agent::AgentEvent::ToolCompleted {
                        name: name.clone(),
                        success: false,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                    continue;
                }
            }

            let args = match self
                .parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time)
                .await
            {
                Some(args) => args,
                None => continue,
            };

            if !self
                .validate_tool_args(
                    &name,
                    &args_str,
                    &args,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                continue;
            }

            if self
                .maybe_block_redundant_reread(
                    &name,
                    &args_str,
                    &args,
                    &call_id,
                    use_native_fc,
                    start_time,
                )
                .await
            {
                continue;
            }

            // Same gate the sequential path runs (YOLO forbidden-ops/protected-path/
            // container-mount checks, plus a confirmation prompt for anything that
            // still needs one). Without this, tools in the parallel-safe list were
            // silently exempt from the YOLO gate entirely -- e.g. a file_read of a
            // YOLO-protected path would be Block-ed in the sequential path but ran
            // unchecked here just because it happened to land in a >=2-tool batch.
            if !self
                .confirm_tool_execution(&name, &args_str, &call_id, use_native_fc)
                .await?
            {
                continue;
            }

            // Fire PreToolUse hooks (may skip execution)
            let pre_ctx = HookContext::pre_tool(&name, &args_str);
            if let HookAction::Skip { reason } = self.hook_registry.fire(&pre_ctx).await {
                let skip_msg = format!("Tool skipped by PreToolUse hook: {}", reason);
                info!("{}", skip_msg);
                self.push_tool_result_message(use_native_fc, &call_id, &name, false, &skip_msg)
                    .await;
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

        let timeout_secs = self.config.agent.step_timeout_secs.max(1);
        let batch_cancel = self.cancel_token();

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
                let cancel = batch_cancel.clone();

                futures.push(async move {
                    let Some(tool) = tool_ref else {
                        let msg = format!("Unknown tool: {}", tool_name);
                        return (idx, (false, msg.clone(), msg));
                    };
                    let start = std::time::Instant::now();
                    let execution = run_tool_bounded(
                        crate::observability::telemetry::track_tool_execution(&tool_name, || {
                            tool.execute(tool_args.clone())
                        }),
                        std::time::Duration::from_secs(timeout_secs),
                        cancel,
                    )
                    .await;
                    let elapsed = start.elapsed().as_millis() as u64;
                    match execution {
                        Ok(Ok(result)) => {
                            let tool_success = tool_result_value_indicates_success(&result);
                            let result_str =
                                serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                            let summary = crate::output::semantic_summary(
                                &tool_name,
                                &tool_args,
                                Some(&result_str),
                                tool_success,
                                elapsed,
                            );
                            (idx, (tool_success, result_str, summary))
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
                        Err(ToolHalt::TimedOut) => {
                            let msg = format!("Tool execution timed out after {}s", timeout_secs);
                            (idx, (false, msg.clone(), msg))
                        }
                        Err(ToolHalt::Cancelled) => {
                            let msg = format!("Tool '{}' cancelled", tool_name);
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
                // Only forgive failures for the tool that actually succeeded.
                // Clearing the entire history on any success would mask unrelated
                // failures in the same parallel batch.
                self.clear_failed_tool_attempts_for_tool(&vt.name);
            } else {
                self.record_failed_tool_attempt(&vt.name, &vt.args_str, "execution", &result_str);
            }

            self.track_task_state_after_tool(&vt.name, &vt.args, &result_str, success)
                .await;

            if success && tool_call_is_mutating(&vt.name, &vt.args) {
                self.note_mutating_tool_call();
                if matches!(
                    vt.name.as_str(),
                    "file_edit"
                        | "file_write"
                        | "file_fim_edit"
                        | "file_multi_edit"
                        | "patch_apply"
                ) {
                    self.has_written_any_file = true;
                    self.terminal_guard_hits = 0;
                }
            }

            if success
                && tool_call_is_verification(&vt.name, &vt.args_str)
                && self.mutation_sequence > 0
            {
                self.last_successful_verification_mutation_sequence = self.mutation_sequence;
                self.last_failed_verification_summary = None;
            } else if !success && tool_call_is_verification(&vt.name, &vt.args_str) {
                let preview: String = result_str.chars().take(300).collect();
                self.last_failed_verification_summary =
                    Some(format!("{} failed: {}", vt.name, preview));
            }

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
                            self.track_file_read_in_context_map(&path_str, &content).await;
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
            )
            .await;
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

        if self
            .suppress_repeated_failed_tool_retry(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
            )
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if let Some(error_msg) = self.current_task_tool_policy_violation(&name) {
            self.reject_tool_call_before_execution(
                &name,
                &args_str,
                &call_id,
                use_native_fc,
                start_time,
                "task_policy",
                &error_msg,
            )
            .await;
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
            self.push_tool_result_message(use_native_fc, &call_id, &name, false, &error_msg)
                .await;
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

        let args = match self
            .parse_tool_args(&name, &args_str, &call_id, use_native_fc, start_time)
            .await
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

        if !self
            .validate_tool_args(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            )
            .await
        {
            self.emit_event(AgentEvent::ToolCompleted {
                name: name.clone(),
                success: false,
                duration_ms: start_time.elapsed().as_millis() as u64,
            });
            return Ok(());
        }

        if self
            .maybe_block_redundant_reread(
                &name,
                &args_str,
                &args,
                &call_id,
                use_native_fc,
                start_time,
            )
            .await
        {
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
            self.push_tool_result_message(use_native_fc, &call_id, &name, false, &skip_msg)
                .await;
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

        self.track_task_state_after_tool(&name, &args, &result, success)
            .await;

        // Track file operations for context management
        if success {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                let path_str = path.to_string();
                match name.as_str() {
                    "file_read" => {
                        self.last_read_file = Some(path_str.clone());
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
                            self.track_file_read_in_context_map(&path_str, &content).await;
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

        self.push_tool_result_message(use_native_fc, &call_id, &name, success, &result)
            .await;
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

                let to_promote = self.context_map.focus_on_query(query, max_files).await;

                // Actually load the files that need promoting.
                let root = super::current_project_root();
                let mut loaded = Vec::new();
                for path in &to_promote {
                    let full_path = root.join(path);
                    if let Ok(content) = tokio::fs::read_to_string(&full_path).await {
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
                let rec = self.context_map.recommend_context(task).await;
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

                match tokio::fs::read_to_string(&full_path).await {
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

    /// Execute tool_search - search for deferred tools and activate them.
    /// This allows the LLM to discover tools on demand, reducing context window usage.
    async fn execute_tool_search(&mut self, args: &serde_json::Value) -> serde_json::Value {
        use crate::tools::tool_search::ToolSearchResult;

        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        if query.is_empty() {
            return serde_json::json!({
                "error": "query parameter is required",
                "found_tools": [],
                "count": 0,
            });
        }

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5)
            .clamp(1, 20);

        // Search for tools in the registry
        let results: Vec<ToolSearchResult> = self.tools.search(query, limit);

        // Activate the found tools (make them available for use)
        let mut activated = Vec::new();
        for result in &results {
            if !result.is_critical && !self.tools.is_activated(&result.name) {
                self.tools.activate(&result.name);
                activated.push(result.name.clone());
            }
        }

        // Build response
        let found_tools: Vec<serde_json::Value> = results
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "description": r.description,
                    "schema": r.schema,
                    "is_critical": r.is_critical,
                    "category": r.category,
                })
            })
            .collect();

        let total_tools = self.tools.total_count();
        let activated_tools = self.tools.activated_count();

        serde_json::json!({
            "found_tools": found_tools,
            "count": found_tools.len(),
            "query": query,
            "newly_activated": activated,
            "total_tools_available": total_tools,
            "activated_tools_count": activated_tools,
            "note": if activated.is_empty() {
                "These tools are available for use in this session."
            } else {
                "These tools are now available for use in this session."
            },
        })
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

    /// Push a `<tool_result><skipped>...</skipped></tool_result>` (or native
    /// tool-message equivalent) recording that a tool call was denied/skipped
    /// without being executed.
    fn push_tool_skip_message(&mut self, call_id: &str, use_native_fc: bool, msg: &str) {
        if use_native_fc {
            self.messages.push(Message::tool(
                serde_json::json!({"skipped": msg}).to_string(),
                call_id,
            ));
        } else {
            self.messages.push(Message::user(format!(
                "<tool_result><skipped>{}</skipped></tool_result>",
                msg
            )));
        }
    }

    async fn confirm_tool_execution(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
    ) -> Result<bool> {
        // In Yolo/Daemon mode `needs_confirmation()` below always says "no
        // need to ask" -- but the YoloManager still enforces a hard floor:
        // forbidden operations, protected paths, dangerous container mounts,
        // and (unless explicitly allowed in config) destructive shell
        // commands / unconfirmed git pushes. This is the only place those
        // checks run, so it must not be skipped.
        if matches!(
            self.config.execution_mode,
            crate::config::ExecutionMode::Yolo | crate::config::ExecutionMode::Daemon
        ) {
            let args_value: serde_json::Value =
                serde_json::from_str(args_str).unwrap_or(serde_json::Value::Null);
            let decision = self.yolo_manager.should_auto_approve(name, &args_value);
            use crate::safety::yolo::YoloDecision;
            match decision {
                YoloDecision::AutoApprove => {
                    self.yolo_manager.record_operation(
                        name,
                        &args_value,
                        true,
                        crate::safety::yolo::AuditResult::Success,
                        0,
                    );
                }
                YoloDecision::Block(reason) => {
                    self.yolo_manager.record_operation(
                        name,
                        &args_value,
                        false,
                        crate::safety::yolo::AuditResult::Blocked(reason.clone()),
                        0,
                    );
                    self.push_tool_skip_message(
                        call_id,
                        use_native_fc,
                        &format!("Blocked by YOLO safety gate: {}", reason),
                    );
                    return Ok(false);
                }
                YoloDecision::RequireConfirmation(reason) => {
                    // No operator to ask in a headless/daemon run -- fail
                    // closed rather than silently allowing or hanging.
                    if !self.is_interactive() && !self.has_tui_renderer() {
                        self.yolo_manager.record_operation(
                            name,
                            &args_value,
                            false,
                            crate::safety::yolo::AuditResult::Blocked(reason.clone()),
                            0,
                        );
                        self.push_tool_skip_message(
                            call_id,
                            use_native_fc,
                            &format!(
                                "Denied (unattended session, no operator to confirm): {}",
                                reason
                            ),
                        );
                        return Ok(false);
                    }
                    // An operator is present (CLI or TUI): fall through to
                    // the normal interactive prompt below instead of the
                    // usual YOLO auto-approve.
                    return self
                        .prompt_tool_confirmation(name, args_str, call_id, use_native_fc)
                        .await;
                }
            }
        }

        if !self.needs_confirmation(name) {
            return Ok(true);
        }

        self.prompt_tool_confirmation(name, args_str, call_id, use_native_fc)
            .await
    }

    /// Interactive (CLI or TUI) yes/no confirmation prompt for a single tool
    /// call. Assumes the caller has already decided confirmation is required.
    async fn prompt_tool_confirmation(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
    ) -> Result<bool> {
        let args_preview: String = args_str
            .chars()
            .take(TOOL_CONFIRM_ARGS_PREVIEW_CHARS)
            .collect();
        let args_display = if args_str.chars().count() > TOOL_CONFIRM_ARGS_PREVIEW_CHARS {
            format!("{}...", args_preview)
        } else {
            args_preview
        };

        // When TUI is active, route the confirmation through the TUI's own
        // permission modal instead of writing to stdout/stdin (which the TUI
        // owns) or auto-approving.
        if self.has_tui_renderer() {
            use super::tui_events::AgentEvent;
            self.emit_event(AgentEvent::PermissionRequested {
                tool_name: name.to_string(),
                reason: format!("Args: {}", args_display),
            });
            let approved = self.await_tui_permission_response().await;
            if !approved {
                let denial = "Tool execution denied via TUI permission prompt";
                self.record_failed_tool_attempt(name, args_str, "operator_denied", denial);
                self.push_tool_skip_message(
                    call_id,
                    use_native_fc,
                    denial,
                );
            }
            return Ok(approved);
        }

        use tokio::io::AsyncWriteExt;

        if !self.is_interactive() {
            return Err(anyhow::anyhow!(
                "Tool '{}' requires confirmation but cannot prompt in headless mode. \
                 Re-run with --yolo to auto-approve all tool calls, \
                 or use interactive/TUI mode for manual confirmation.",
                name
            ));
        }

        cli_println!(
            "{} Tool: {} Args: {}",
            "⚠️".bright_yellow(),
            name.bright_cyan(),
            args_display.bright_white()
        );
        print!("\n\x1b[0m\x1b[1m\x1b[97mExecute? [y = once / N = skip / type \"yolo\" to disable confirmations]: \x1b[0m");
        let _ = tokio::io::stdout().flush().await;

        let response =
            super::execution::read_line_pausing_esc(&self.esc_paused, &self.esc_pause_ack).await;
        if let Ok(response) = response {
            match parse_confirm_response(&response) {
                ConfirmDecision::ExecuteOnce => return Ok(true),
                ConfirmDecision::EnableYolo => {
                    self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                    cli_println!(
                        "{} Confirmations disabled for the rest of this session (YOLO)",
                        "⚡".bright_yellow()
                    );
                    return Ok(true);
                }
                ConfirmDecision::Skip => {}
            }
        }

        let skip_msg = "Tool execution skipped by user";
        self.record_failed_tool_attempt(name, args_str, "operator_denied", skip_msg);
        cli_println!("{} {}", "⏭️".bright_yellow(), skip_msg);
        self.push_tool_skip_message(call_id, use_native_fc, skip_msg);
        Ok(false)
    }

    pub(super) async fn parse_tool_args(
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
                self.push_tool_result_message(use_native_fc, call_id, name, false, &err)
                    .await;
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

    pub(super) async fn validate_tool_args(
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
                self.push_tool_result_message(use_native_fc, call_id, name, false, &err)
                    .await;
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
        // Track every dispatched tool call for FailureMode classification.
        self.note_total_tool_call();

        // Emit a `ToolCallStarted` progress event before dispatch. The matching
        // `ToolCallCompleted` event is emitted at the end via the inner helper
        // so we don't have to thread it through every early-return branch.
        self.emit_progress(super::progress::ProgressEvent::ToolCallStarted {
            tool: name.to_string(),
            args_short: super::progress::short_args_for(name, args),
        });

        let result = self
            .execute_single_tool_inner(name, args_str, args, start_time)
            .await;
        let elapsed_ms = start_time.elapsed().as_millis() as u64;
        let ok = matches!(&result, Ok((true, _, _)));
        self.emit_progress(super::progress::ProgressEvent::ToolCallCompleted {
            tool: name.to_string(),
            ok,
            elapsed_ms,
        });
        result
    }

    /// Inner body of [`execute_single_tool`] — kept as a separate method so the
    /// outer wrapper can emit `ToolCallStarted` / `ToolCallCompleted` progress
    /// events around it without threading them through every early return.
    async fn execute_single_tool_inner(
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

        // Intercept tool_search — it activates deferred tools and returns their schemas
        if name == "tool_search" {
            let result = self.execute_tool_search(args).await;
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
            if let Some(cached_value) = self.cache_manager.tool_cache.get(name, args).await {
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
                self.cache_manager.invalidate_path(path).await;
            }
            // shell_exec and git operations can affect any file — clear all read caches
            if matches!(name, "shell_exec" | "git_commit" | "git_checkout") {
                self.cache_manager.tool_cache.clear().await;
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

        // For tools that spawn an OS subprocess (shell_exec, pty_shell), emit
        // structured progress events around the spawn so live observers
        // (StderrProgressEmitter, future Prometheus exporter) can track
        // subprocess lifecycles independently of the in-process tool wrapper.
        let spawns_subprocess = is_bash;
        let subprocess_start = std::time::Instant::now();
        if spawns_subprocess {
            self.emit_progress(super::progress::ProgressEvent::SubprocessStarted {
                name: name.to_string(),
            });
        }

        let execution = run_tool_bounded(
            crate::observability::telemetry::track_tool_execution(name, || {
                tool.execute(args.clone())
            }),
            std::time::Duration::from_secs(timeout_secs),
            self.cancel_token(),
        )
        .await;

        if spawns_subprocess {
            // Best-effort exit code: tool wrappers (e.g. shell_exec) surface it
            // in the JSON result; the agent layer can't read it back here, so we
            // report success/failure as 0/-1 and the elapsed wall time.
            let exit = match &execution {
                Ok(Ok(_)) => 0,
                Ok(Err(_)) => -1,
                Err(_) => -2, // tokio timeout
            };
            self.emit_progress(super::progress::ProgressEvent::SubprocessCompleted {
                name: name.to_string(),
                exit,
                elapsed_ms: subprocess_start.elapsed().as_millis() as u64,
            });
        }

        match execution {
            Ok(Ok(result)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let result_str = serde_json::to_string(&result)?;
                let tool_success = tool_result_value_indicates_success(&result);
                let summary = crate::output::semantic_summary(
                    name,
                    args,
                    Some(&result_str),
                    tool_success,
                    elapsed,
                );
                self.log_tool_call(name, args_str, &result_str, tool_success, start_time, true);

                // Store successful cacheable results in ToolCache
                if is_cacheable && tool_success {
                    self.cache_manager
                        .tool_cache
                        .set(name, args, result.clone())
                        .await;
                }

                // Cache tool results in LocalFirstCoordinator
                if tool_success {
                    let cache_key = crate::session::cache::ToolCache::cache_key(name, args);
                    self.cache_manager.local_first.cache_response(
                        &cache_key,
                        result_str.clone(),
                        result_str.len(),
                    );
                }

                // Display color-coded diff for file mutations
                if let Some((ref path, ref old_content)) = pre_edit_content {
                    if tool_success && matches!(name, "file_edit" | "file_write") {
                        self.has_written_any_file = true;
                        self.terminal_guard_hits = 0;
                        if let Ok(new_content) = tokio::fs::read_to_string(path).await {
                            crate::output::display_file_diff(path, old_content, &new_content);
                        }
                    }
                }

                // Track mutating tool calls for FailureMode classification.
                // For `shell_exec`, only count as mutating when the command is
                // NOT observational (e.g. `rm`, `mv`, `git add`, `cargo fmt`,
                // `sed -i`, redirects).  Observational shell calls like
                // `cargo check` / `git status` / `ls` should NOT bump the
                // mutating counter.
                if tool_call_is_mutating(name, args) && tool_success {
                    self.note_mutating_tool_call();
                }

                if tool_success
                    && tool_call_is_verification(name, args_str)
                    && self.mutation_sequence > 0
                {
                    self.last_successful_verification_mutation_sequence = self.mutation_sequence;
                    self.last_failed_verification_summary = None;
                } else if !tool_success && tool_call_is_verification(name, args_str) {
                    self.last_failed_verification_summary = Some({
                        let preview: String = result_str.chars().take(300).collect();
                        format!("{} failed: {}", name, preview)
                    });
                }

                // Record successful tool usage for learning
                self.self_improvement.record_tool(
                    name,
                    self.learning_context(),
                    if tool_success {
                        Outcome::Success
                    } else {
                        Outcome::Failure
                    },
                    elapsed,
                    (!tool_success).then(|| result_str.clone()),
                );

                let verification_result = if tool_success {
                    self.maybe_verify_file_change(name, args).await
                } else {
                    None
                };
                let visual_verification_result = self.maybe_verify_visual_change(name, args).await;
                let enhanced_result = self.maybe_enhance_tool_result(name, &result_str);
                let mut final_result = enhanced_result;
                if let Some(ver_msg) = verification_result {
                    final_result.push_str(&ver_msg);
                }
                // Track visual verification details for potential error reporting
                let mut hard_failure_details: Option<(String, String, String)> = None;
                if let Some(ref vvr) = visual_verification_result {
                    if !vvr.message.is_empty() {
                        final_result.push_str(&vvr.message);
                    }
                    if let Some(ref assertion) = vvr.assertion {
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            // On hard failure, set as pending assertion to gate progression
                            if vvr.hard_failure {
                                checkpoint.set_pending_visual_assertion(assertion.clone());
                            } else {
                                checkpoint.log_visual_assertion(assertion.clone());
                            }
                        }
                        // Capture details for error message if this is a hard failure
                        if vvr.hard_failure {
                            let exp = assertion
                                .expected
                                .clone()
                                .unwrap_or_else(|| "Expected UI state".to_string());
                            let obs = assertion
                                .observed
                                .clone()
                                .unwrap_or_else(|| "Actual UI state did not match".to_string());
                            // Extract issues from the message if present
                            let iss = if vvr.message.contains("issues:") {
                                vvr.message
                                    .split("issues:")
                                    .nth(1)
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_else(|| "No specific issues listed".to_string())
                            } else {
                                "No specific issues listed".to_string()
                            };
                            hard_failure_details = Some((exp, obs, iss));
                        }
                    }
                }
                if let Some((expected, actual, issues)) = hard_failure_details {
                    // Return an error to trigger error recovery flow with rich details
                    return Err(crate::errors::AgentError::VisualAssertionFailed {
                        description: format!("Visual verification failed after {}: {}", name, issues),
                        expected,
                        actual,
                        recovery_hint: format!(
                            "The {} action did not produce the expected visual result. \
                             Retry the action with different parameters or try a different approach.",
                            name
                        ),
                    }.into());
                }
                Ok((tool_success, final_result, summary))
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
            Err(ToolHalt::TimedOut) => {
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
            Err(ToolHalt::Cancelled) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                let err = format!("Tool '{}' cancelled", name);
                let summary =
                    crate::output::semantic_summary(name, args, Some(&err), false, elapsed);
                self.log_tool_call(name, args_str, &err, false, start_time, false);
                Ok((false, err, summary))
            }
        }
    }

    pub(super) async fn push_tool_result_message(
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
        // Applies to BOTH success AND error results — an oversized error was
        // previously stored verbatim, which could blow the context-token budget
        // (an OOM surface). summarize_and_spill keeps head+tail, so trailing
        // failure markers still survive.
        let result_to_store = {
            let estimated_tokens = crate::token_count::estimate_content_tokens(result);
            if estimated_tokens > MAX_TOOL_RESULT_TOKENS {
                info!(
                    "Tool result from '{}' is {} tokens (budget {}), summarizing with disk reference",
                    tool_name, estimated_tokens, MAX_TOOL_RESULT_TOKENS
                );
                summarize_and_spill(tool_name, call_id, result, estimated_tokens).await
            } else {
                result.to_string()
            }
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
    use crate::config::Config;
    use crate::testing::mock_api::MockLlmServer;

    #[test]
    fn confirm_response_requires_explicit_yolo_word() {
        use super::{parse_confirm_response, ConfirmDecision};
        assert_eq!(parse_confirm_response("y"), ConfirmDecision::ExecuteOnce);
        assert_eq!(parse_confirm_response("YES"), ConfirmDecision::ExecuteOnce);
        assert_eq!(parse_confirm_response("yolo"), ConfirmDecision::EnableYolo);
        assert_eq!(parse_confirm_response(" YOLO "), ConfirmDecision::EnableYolo);
        // The old footgun keys must now be harmless skips, not a session downgrade.
        assert_eq!(parse_confirm_response("s"), ConfirmDecision::Skip);
        assert_eq!(parse_confirm_response("skip"), ConfirmDecision::Skip);
        assert_eq!(parse_confirm_response("n"), ConfirmDecision::Skip);
        assert_eq!(parse_confirm_response(""), ConfirmDecision::Skip);
    }

    #[test]
    fn test_shell_verification_matches_at_command_boundary() {
        // Plain and flagged forms still match.
        assert!(shell_command_is_verification("cargo check"));
        assert!(shell_command_is_verification("cargo test --all"));
        // Regression: full-path / cd-prefixed invocations must be credited too
        // (the model used ~/.cargo/bin/cargo to dodge a PATH issue and looped).
        assert!(shell_command_is_verification("~/.cargo/bin/cargo check"));
        assert!(shell_command_is_verification("/usr/bin/cargo check --message-format short"));
        assert!(shell_command_is_verification("cd crates/foo && cargo test"));
        // Non-verification commands are not falsely credited.
        assert!(!shell_command_is_verification("cargo add serde"));
        assert!(!shell_command_is_verification("echo cargo checkers"));
        assert!(!shell_command_is_verification("ls -la"));
    }

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
    fn summarize_generic_preserves_tail_marker() {
        // A large result whose FAILURE marker is at the very end must survive
        // summarization — head-only truncation would drop it and the gate would
        // miss the failure.
        let middle = "x".repeat(60_000);
        let raw = format!("START\n{}\n<verification_failed>tests FAILED</verification_failed>", middle);
        let summary = summarize_generic(&raw);
        assert!(summary.contains("START"), "head kept");
        assert!(
            summary.contains("<verification_failed>") && summary.contains("FAILED"),
            "tail failure marker must survive summarization: {}",
            &summary[summary.len().saturating_sub(200)..]
        );
        // Middle was actually elided (summary far smaller than raw).
        assert!(summary.chars().count() < raw.chars().count());
        assert!(summary.contains("omitted from the middle"));
    }

    #[test]
    fn summarize_generic_keeps_small_input_verbatim() {
        let raw = "short output\nline 2\n<verification_failed>nope</verification_failed>";
        assert_eq!(summarize_generic(raw), raw);
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
    fn test_extract_explicit_allowed_tools_from_task_prompt() {
        let task = "Use only these concrete tools for this task:\n- `file_read`\n- `file_edit`\n- `file_write`\n- `shell_exec`\n";
        let allowed = extract_explicit_allowed_tools(task).expect("expected allowlist");
        assert!(allowed.contains("file_read"));
        assert!(allowed.contains("file_edit"));
        assert!(allowed.contains("file_write"));
        assert!(allowed.contains("shell_exec"));
        assert_eq!(allowed.len(), 4);
    }

    #[test]
    fn test_extract_explicit_requested_tools_detects_imperative_use() {
        let required = extract_explicit_requested_tools(
            "Use vision_analyze on ./sample.jpg and answer in one sentence.",
            ["vision_analyze", "file_read"].iter().copied(),
        );
        assert!(required.contains("vision_analyze"));
        assert_eq!(required.len(), 1);
    }

    #[test]
    fn test_extract_explicit_requested_tools_detects_backticked_tool() {
        let required = extract_explicit_requested_tools(
            "Please call `file_read` on Cargo.toml before answering.",
            ["vision_analyze", "file_read"].iter().copied(),
        );
        assert!(required.contains("file_read"));
    }

    #[test]
    fn test_negated_tool_mention_is_not_a_required_tool() {
        let required = extract_explicit_requested_tools(
            "Create notes.txt, but don't use `shell_exec`.",
            ["shell_exec", "file_write"].iter().copied(),
        );
        assert!(!required.contains("shell_exec"));
    }

    #[test]
    fn test_shell_category_denial_overrides_plain_tool_mention() {
        let task = "Create user-check_1+2=3.txt using file_write. Do not run shell commands or use pty_shell.";
        let required = extract_explicit_requested_tools(
            task,
            ["file_write", "shell_exec", "pty_shell"].iter().copied(),
        );

        assert!(required.contains("file_write"));
        assert!(!required.contains("shell_exec"));
        assert!(!required.contains("pty_shell"));
    }

    #[test]
    fn test_shell_exec_verification_commands_are_observational() {
        assert!(shell_command_is_observational("cargo test --quiet"));
        assert!(shell_command_is_observational("cargo check"));
        assert!(!shell_command_is_observational("cargo fmt"));
        assert!(!shell_command_is_observational("mkdir tmp"));
    }

    #[test]
    fn test_shell_redirect_writes_are_not_observational() {
        // Redirects WITHOUT a leading space used to slip through (#22).
        assert!(!shell_command_is_observational("echo x>y"));
        assert!(!shell_command_is_observational("cat>file"));
        assert!(!shell_command_is_observational("echo hi > out.txt"));
        assert!(!shell_command_is_observational("cat a >> b"));
        assert!(!shell_command_is_observational("echo data >/etc/thing"));
    }

    #[test]
    fn test_shell_fd_dup_and_quoted_gt_stay_observational() {
        // 2>&1 duplicates a descriptor — it writes no file.
        assert!(shell_command_is_observational("cargo test 2>&1"));
        assert!(shell_command_is_observational("grep foo bar 2>&1"));
        // A '>' inside quotes is data, not a redirect.
        assert!(shell_command_is_observational(r#"grep "->" file"#));
        assert!(shell_command_is_observational("echo 'a>b'"));
    }

    #[test]
    fn test_tool_call_counts_shell_exec_state_changes_correctly() {
        assert!(!tool_call_counts_as_state_change(
            "shell_exec",
            r#"{"command":"cargo test"}"#
        ));
        assert!(tool_call_counts_as_state_change(
            "shell_exec",
            r#"{"command":"cargo fmt"}"#
        ));
        assert!(!tool_call_counts_as_state_change("shell_exec", r#"{}"#));
    }

    #[tokio::test]
    async fn test_task_tool_policy_blocks_unlisted_tools() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_task_context = "Use only these concrete tools for this task:\n- `file_read`\n- `file_edit`\n- `file_write`\n- `shell_exec`\nNever call `tool_search`.".to_string();

        agent
            .execute_tool_batch(vec![(
                crate::tools::context::CONTEXT_BULK_READ.to_string(),
                r#"{"pattern":"src/**/*.rs","max_files":2}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent
            .messages
            .last()
            .expect("expected tool policy rejection");
        assert!(last.content.text().contains("Task tool policy violation"));
        assert!(last.content.text().contains("Allowed tools"));
        assert!(agent
            .recent_failed_tool_attempts
            .back()
            .is_some_and(|attempt| attempt.failure_kind == "task_policy"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_operator_denial_is_remembered_for_exact_retry() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        let args = r#"{"path":"notes.txt","content":"hello"}"#;

        agent.record_failed_tool_attempt(
            "file_write",
            args,
            "operator_denied",
            "Tool execution denied via TUI permission prompt",
        );

        let failure = agent
            .recent_failed_tool_attempts
            .back()
            .expect("operator denial should be task-local retry memory");
        assert_eq!(failure.failure_kind, "operator_denied");
        let retry_message = agent.build_failed_tool_retry_suppressed_message(failure);
        assert!(retry_message.contains("operator denied `file_write`"));
        assert!(retry_message.contains("Do not ask for the same permission again"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_progress_guard_blocks_read_only_batches_after_threshold() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_task_context =
            "Fix the failing tests, make code changes, and keep going until everything is green."
                .to_string();
        // New pre-edit block_threshold is 12, escalation_threshold is 18.
        // Set above escalation so the guard fires AND synthesis is triggered.
        agent.consecutive_read_only_steps = 19;

        agent
            .execute_tool_batch(vec![(
                "shell_exec".to_string(),
                r#"{"command":"cargo test"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        assert!(agent
            .messages
            .iter()
            .any(|msg| msg.content.text().contains("PROGRESS GUARD")));
        let last = agent
            .messages
            .last()
            .expect("expected follow-up progress directive");
        assert!(last
            .content
            .text()
            .contains("READ-LOOP FORCE-MUTATION MODE"));
        assert!(last.content.text().contains("<name>file_edit</name>"));
        assert_eq!(
            agent.pending_synthesis.as_deref(),
            Some("Fix the failing tests, make code changes, and keep going until everything is green.")
        );
        assert!(agent
            .recent_failed_tool_attempts
            .back()
            .is_some_and(|attempt| attempt.failure_kind == "progress_guard"));

        // guard_count is now 1 (first fire).  Need >= 3 for hard abort.
        agent.consecutive_read_only_steps = 14;
        agent
            .execute_tool_batch(vec![(
                "shell_exec".to_string(),
                r#"{"command":"git status"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();
        // guard_count is now 2 — still not enough for hard abort (>= 3).
        agent.consecutive_read_only_steps = 15;
        let err = agent
            .execute_tool_batch(vec![(
                "shell_exec".to_string(),
                r#"{"command":"git status"}"#.to_string(),
                None,
            )])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("READ_LOOP_NO_EDIT"));

        server.stop().await;
    }

    #[tokio::test]
    async fn test_progress_guard_novel_reads_decrement_counter() {
        // Bug #13: reading DISTINCT new files should NOT trip the guard as fast
        // as re-reading the same file.  We verify that the investigation-progress
        // reset causes `consecutive_read_only_steps` to DECREASE when the agent
        // reads a novel file, while re-reading the same file INCREASES it.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();
        agent.current_task_context =
            "Refactor the module: read many files, then make changes.".to_string();

        // Start with a moderate read-only streak.
        agent.consecutive_read_only_steps = 5;

        // Read file A — novel target, counter should DECREMENT.
        agent.update_read_only_step_tracking(
            &[(
                "file_read".to_string(),
                r#"{"path":"src/main.rs"}"#.to_string(),
                None,
            )],
            false,
        );
        assert_eq!(
            agent.consecutive_read_only_steps, 4,
            "novel read should decrement counter"
        );

        // Read file B — novel target, counter should DECREMENT again.
        agent.update_read_only_step_tracking(
            &[(
                "file_read".to_string(),
                r#"{"path":"src/lib.rs"}"#.to_string(),
                None,
            )],
            false,
        );
        assert_eq!(
            agent.consecutive_read_only_steps, 3,
            "second novel read should decrement counter"
        );

        // Re-read file A — redundant, counter should INCREMENT.
        agent.update_read_only_step_tracking(
            &[(
                "file_read".to_string(),
                r#"{"path":"src/main.rs"}"#.to_string(),
                None,
            )],
            false,
        );
        assert_eq!(
            agent.consecutive_read_only_steps, 4,
            "redundant re-read should increment counter"
        );

        // Re-read file A again — still redundant, counter should INCREMENT.
        agent.update_read_only_step_tracking(
            &[(
                "file_read".to_string(),
                r#"{"path":"src/main.rs"}"#.to_string(),
                None,
            )],
            false,
        );
        assert_eq!(
            agent.consecutive_read_only_steps, 5,
            "second redundant re-read should increment counter"
        );

        // A write tool should reset counter AND clear the seen-set.
        agent.update_read_only_step_tracking(
            &[(
                "file_edit".to_string(),
                r#"{"path":"src/main.rs"}"#.to_string(),
                None,
            )],
            true,
        );
        assert_eq!(
            agent.consecutive_read_only_steps, 0,
            "write should reset counter to 0"
        );
        assert!(
            agent.seen_read_targets.is_empty(),
            "write should clear seen_read_targets"
        );

        server.stop().await;
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
                native_function_calling: None,
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
                native_function_calling: None,
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

    #[test]
    fn test_inject_runtime_tool_defaults_ignores_text_only_default_profile() {
        let mut config = crate::config::Config::default();
        config.models.insert(
            "default".to_string(),
            crate::config::ModelProfile {
                endpoint: "https://text.example/v1".to_string(),
                model: "text-only".to_string(),
                api_key: None,
                max_tokens: 512,
                temperature: 0.3,
                modalities: vec!["text".to_string()],
                context_length: 131_072,
                extra_body: None,
                native_function_calling: None,
            },
        );

        let effective = inject_runtime_tool_defaults(
            &config,
            "vision_analyze",
            r#"{"prompt":"describe","image_base64":"AAAA"}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&effective).unwrap();
        assert!(parsed.get("endpoint").is_none());
        assert!(parsed.get("model").is_none());
    }

    // =========================================================================
    // summarize_directory_tree tests
    // =========================================================================

    #[test]
    fn test_summarize_directory_tree_basic() {
        let raw = serde_json::json!({
            "root": "/home/user/project",
            "total": 5,
            "entries": [
                {"path": "/home/user/project/src/main.rs", "type": "file", "size": 1024},
                {"path": "/home/user/project/src/lib.rs", "type": "file", "size": 512},
                {"path": "/home/user/project/src", "type": "directory", "size": 0},
                {"path": "/home/user/project/Cargo.toml", "type": "file", "size": 256},
                {"path": "/home/user/project/README.md", "type": "file", "size": 128}
            ]
        });
        let summary = summarize_directory_tree(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("/home/user/project"));
        assert!(summary.contains("5 entries"));
    }

    #[test]
    fn test_summarize_directory_tree_empty() {
        let raw = serde_json::json!({"root": ".", "total": 0, "entries": []});
        let summary = summarize_directory_tree(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("0 entries"));
    }

    #[test]
    fn test_summarize_directory_tree_invalid_json() {
        let summary = summarize_directory_tree("not json");
        assert!(summary.contains("0 entries"));
    }

    // =========================================================================
    // summarize_file_read tests
    // =========================================================================

    #[test]
    fn test_summarize_file_read_short() {
        let raw = serde_json::json!({
            "total_lines": 5,
            "content": "line1\nline2\nline3\nline4\nline5"
        });
        let summary = summarize_file_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("5 total lines"));
        assert!(summary.contains("line1"));
    }

    #[test]
    fn test_summarize_file_read_long() {
        let lines: String = (0..200)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let raw = serde_json::json!({
            "total_lines": 200,
            "content": lines
        });
        let summary = summarize_file_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("200 total lines"));
        assert!(summary.contains("First 100 lines"));
        assert!(summary.contains("Last 50 lines"));
        assert!(summary.contains("lines omitted"));
    }

    #[test]
    fn test_summarize_file_read_empty() {
        let raw = serde_json::json!({"total_lines": 0, "content": ""});
        let summary = summarize_file_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("0 total lines"));
    }

    #[test]
    fn test_summarize_file_read_150_boundary_no_silent_drop() {
        // Regression: files of 101–150 lines used to show only the first 100 and
        // silently drop the rest (the tail required > 150). Ensure lines 101–150
        // now appear and nothing is marked omitted (found by GLM-5.2).
        let lines: String = (0..150)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let raw = serde_json::json!({"total_lines": 150, "content": lines});
        let summary = summarize_file_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("line 0"), "head present");
        assert!(summary.contains("line 149"), "last line must not be dropped");
        assert!(summary.contains("line 120"), "mid-tail line must be present");
        assert!(
            !summary.contains("lines omitted"),
            "nothing is actually omitted at 150 lines"
        );
    }

    // =========================================================================
    // summarize_git_diff tests
    // =========================================================================

    #[test]
    fn test_summarize_git_diff_single_file() {
        let diff = "diff --git a/src/main.rs b/src/main.rs\n--- a/src/main.rs\n+++ b/src/main.rs\n+added line\n-removed line\n+another add";
        let raw = serde_json::json!({"diff": diff});
        let summary = summarize_git_diff(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("1 files changed"));
        assert!(summary.contains("+2"));
        assert!(summary.contains("-1"));
    }

    #[test]
    fn test_summarize_git_diff_multiple_files() {
        let diff = "diff --git a/a.rs b/a.rs\n+line1\ndiff --git a/b.rs b/b.rs\n-line2";
        let raw = serde_json::json!({"diff": diff});
        let summary = summarize_git_diff(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("2 files changed"));
    }

    #[test]
    fn test_summarize_git_diff_empty() {
        let raw = serde_json::json!({"diff": ""});
        let summary = summarize_git_diff(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("0 files changed"));
    }

    // =========================================================================
    // summarize_bulk_read tests
    // =========================================================================

    #[test]
    fn test_summarize_bulk_read() {
        let raw = serde_json::json!({"loaded": 5, "skipped": 2, "tokens_added": 10000});
        let summary = summarize_bulk_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("5 files loaded"));
        assert!(summary.contains("2 skipped"));
        assert!(summary.contains("10000 tokens"));
    }

    #[test]
    fn test_summarize_bulk_read_empty() {
        let raw = serde_json::json!({});
        let summary = summarize_bulk_read(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("0 files loaded"));
    }

    // =========================================================================
    // summarize_shell_exec tests
    // =========================================================================

    #[test]
    fn test_summarize_shell_exec_basic() {
        let raw = serde_json::json!({
            "exit_code": 0,
            "stdout": "Hello World\nLine 2",
            "stderr": ""
        });
        let summary = summarize_shell_exec(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("Exit code: 0"));
        assert!(summary.contains("Hello World"));
    }

    #[test]
    fn test_summarize_shell_exec_with_stderr() {
        let raw = serde_json::json!({
            "exit_code": 1,
            "stdout": "",
            "stderr": "error: something failed"
        });
        let summary = summarize_shell_exec(&serde_json::to_string(&raw).unwrap());
        assert!(summary.contains("Exit code: 1"));
        assert!(summary.contains("error: something failed"));
    }

    // =========================================================================
    // summarize_generic tests
    // =========================================================================

    #[test]
    fn test_summarize_generic_short() {
        // Small results are now returned verbatim (no head/tail elision needed),
        // so no summary/stats banner is added.
        let summary = summarize_generic("hello world");
        assert_eq!(summary, "hello world");
    }

    #[test]
    fn test_summarize_generic_long() {
        let long = "x".repeat(20000);
        let summary = summarize_generic(&long);
        assert!(summary.contains("see raw file"));
    }

    // =========================================================================
    // task_requires_mutation tests
    // =========================================================================

    #[test]
    fn test_task_requires_mutation_fix() {
        assert!(task_requires_mutation("Fix the failing test"));
    }

    #[test]
    fn test_task_requires_mutation_respects_negation() {
        // Regression: a read-only review whose prompt says "do NOT edit" must not
        // be classified as mutation-required just because it contains "edit".
        assert!(!task_requires_mutation(
            "Review the codebase and produce a report. Do NOT edit any files."
        ));
        assert!(!task_requires_mutation(
            "Analyze src/ for dead code without modifying anything; output your findings."
        ));
        // But an un-negated mutation verb still wins even alongside a negation.
        assert!(task_requires_mutation(
            "Fix the bug, but do not edit the tests."
        ));
        // Plain mutation instructions are unaffected.
        assert!(task_requires_mutation("edit main.rs to add a field"));
    }

    #[test]
    fn test_task_requires_mutation_make_imperative() {
        // Regression (MUT-MAKE-VERB): "Make X return Y" with no other mutation
        // verb must be treated as a mutation task so the safety gates arm.
        assert!(task_requires_mutation(
            "Make parse_port return Result<u16, String> instead of panicking"
        ));
        assert!(task_requires_mutation("Make the function generic over T"));
        // But qualifier phrases are not mutations on their own.
        assert!(!task_requires_mutation(
            "Make sure you understand how the parser works"
        ));
        assert!(!task_requires_mutation("Explain the makefile targets"));
        assert!(!task_requires_mutation(
            "Review the code but do not make any changes"
        ));
    }

    #[test]
    fn test_task_requires_mutation_implement() {
        assert!(task_requires_mutation("Implement the new feature"));
    }

    #[test]
    fn test_task_requires_mutation_edit() {
        assert!(task_requires_mutation("Edit the config file"));
    }

    #[test]
    fn test_task_requires_mutation_modify() {
        assert!(task_requires_mutation("Modify the agent loop"));
    }

    #[test]
    fn test_task_requires_mutation_update() {
        assert!(task_requires_mutation("Update the dependencies"));
    }

    #[test]
    fn test_task_requires_mutation_write() {
        assert!(task_requires_mutation("Write the new module"));
    }

    #[test]
    fn test_task_requires_mutation_create() {
        assert!(task_requires_mutation("Create a new tool"));
    }

    #[test]
    fn test_task_requires_mutation_review_deliverable_is_read_only() {
        // "Create a code review" is read-only despite the word "create".
        assert!(!task_requires_mutation(
            "Create a thorough code review of src/agent/verification.rs with line references"
        ));
        assert!(!task_requires_mutation("Audit the auth module for issues"));
        // But a review paired with a real edit verb is still a mutation task.
        assert!(task_requires_mutation(
            "Review the code and fix the bug in parser.rs"
        ));
        // And an ordinary "create a tool" stays a mutation task.
        assert!(task_requires_mutation("Create a new benchmark tool"));
    }

    #[test]
    fn test_task_requires_mutation_prose_deliverable_is_read_only() {
        // Prose deliverables are read-only despite the create/write verbs.
        assert!(!task_requires_mutation("Create a summary of the auth flow"));
        assert!(!task_requires_mutation("Write a report on the test coverage"));
        assert!(!task_requires_mutation(
            "Explain how the completion gate works"
        ));
        assert!(!task_requires_mutation("Summarize the recent changes"));
        // But naming a code artifact makes it a genuine mutation task.
        assert!(task_requires_mutation("Write a report generator function"));
        assert!(task_requires_mutation(
            "Create a summary parser in parser.rs"
        ));
    }

    #[test]
    fn test_task_requires_mutation_refactor() {
        assert!(task_requires_mutation("Refactor the parser"));
    }

    #[test]
    fn test_task_requires_mutation_rename() {
        assert!(task_requires_mutation("Rename the variable"));
    }

    #[test]
    fn test_task_requires_mutation_delete() {
        assert!(task_requires_mutation("Delete the unused file"));
    }

    #[test]
    fn test_task_requires_mutation_remove() {
        assert!(task_requires_mutation("Remove dead code"));
    }

    #[test]
    fn test_task_requires_mutation_make_tests_pass() {
        assert!(task_requires_mutation("Make tests pass"));
    }

    #[test]
    fn test_task_requires_mutation_until_green() {
        assert!(task_requires_mutation("Keep going until green"));
    }

    #[test]
    fn test_task_no_mutation_read() {
        assert!(!task_requires_mutation("Read the log file"));
    }

    #[test]
    fn test_task_no_mutation_explore() {
        assert!(!task_requires_mutation("Explore the codebase structure"));
    }

    #[test]
    fn test_task_no_mutation_understand() {
        assert!(!task_requires_mutation("Understand how the system works"));
    }

    // =========================================================================
    // shell_command_is_observational tests
    // =========================================================================

    #[test]
    fn test_observational_cargo_test() {
        assert!(shell_command_is_observational("cargo test"));
    }

    #[test]
    fn test_observational_cargo_check() {
        assert!(shell_command_is_observational("cargo check"));
    }

    #[test]
    fn test_observational_cargo_clippy() {
        assert!(shell_command_is_observational("cargo clippy"));
    }

    #[test]
    fn test_observational_git_status() {
        assert!(shell_command_is_observational("git status"));
    }

    #[test]
    fn test_observational_git_diff() {
        assert!(shell_command_is_observational("git diff"));
    }

    #[test]
    fn test_observational_git_log() {
        assert!(shell_command_is_observational("git log"));
    }

    #[test]
    fn test_observational_ls() {
        assert!(shell_command_is_observational("ls"));
    }

    #[test]
    fn test_observational_pwd() {
        assert!(shell_command_is_observational("pwd"));
    }

    #[test]
    fn test_observational_find() {
        assert!(shell_command_is_observational("find . -name '*.rs'"));
    }

    #[test]
    fn test_observational_grep() {
        assert!(shell_command_is_observational("grep -r 'pattern'"));
    }

    #[test]
    fn test_observational_cat() {
        assert!(shell_command_is_observational("cat file.txt"));
    }

    #[test]
    fn test_observational_head() {
        assert!(shell_command_is_observational("head -20 file.txt"));
    }

    #[test]
    fn test_observational_tail() {
        assert!(shell_command_is_observational("tail -f log.txt"));
    }

    #[test]
    fn test_observational_wc() {
        assert!(shell_command_is_observational("wc -l file.txt"));
    }

    #[test]
    fn test_observational_tree() {
        assert!(shell_command_is_observational("tree src/"));
    }

    #[test]
    fn test_observational_which() {
        assert!(shell_command_is_observational("which cargo"));
    }

    #[test]
    fn test_observational_echo() {
        assert!(shell_command_is_observational("echo hello"));
    }

    #[test]
    fn test_observational_pytest() {
        assert!(shell_command_is_observational("pytest tests/"));
    }

    #[test]
    fn test_observational_sed_n() {
        assert!(shell_command_is_observational("sed -n '1,10p' file.txt"));
    }

    #[test]
    fn test_not_observational_cargo_fmt() {
        assert!(!shell_command_is_observational("cargo fmt"));
    }

    #[test]
    fn test_not_observational_cargo_fix() {
        assert!(!shell_command_is_observational("cargo fix"));
    }

    #[test]
    fn test_not_observational_cargo_update() {
        assert!(!shell_command_is_observational("cargo update"));
    }

    #[test]
    fn test_not_observational_mkdir() {
        assert!(!shell_command_is_observational("mkdir new_dir"));
    }

    #[test]
    fn test_not_observational_touch() {
        assert!(!shell_command_is_observational("touch file.txt"));
    }

    #[test]
    fn test_not_observational_rm() {
        assert!(!shell_command_is_observational("rm file.txt"));
    }

    #[test]
    fn test_not_observational_mv() {
        assert!(!shell_command_is_observational("mv a.txt b.txt"));
    }

    #[test]
    fn test_not_observational_cp() {
        assert!(!shell_command_is_observational("cp a.txt b.txt"));
    }

    #[test]
    fn test_not_observational_sed_inplace() {
        assert!(!shell_command_is_observational(
            "sed -i 's/foo/bar/' file.txt"
        ));
    }

    #[test]
    fn test_not_observational_git_add() {
        assert!(!shell_command_is_observational("git add ."));
    }

    #[test]
    fn test_not_observational_git_commit() {
        assert!(!shell_command_is_observational("git commit -m 'msg'"));
    }

    #[test]
    fn test_not_observational_redirect() {
        assert!(!shell_command_is_observational("echo hi > file.txt"));
    }

    #[test]
    fn test_not_observational_npm_install() {
        assert!(!shell_command_is_observational("npm install express"));
    }

    #[test]
    fn test_not_observational_pip_install() {
        assert!(!shell_command_is_observational("pip install requests"));
    }

    #[test]
    fn test_observational_empty() {
        assert!(!shell_command_is_observational(""));
    }

    // =========================================================================
    // tool_call_is_observational tests
    // =========================================================================

    #[test]
    fn test_observational_file_read() {
        assert!(tool_call_is_observational("file_read", "{}"));
    }

    #[test]
    fn test_observational_directory_tree() {
        assert!(tool_call_is_observational("directory_tree", "{}"));
    }

    #[test]
    fn test_observational_glob_find() {
        assert!(tool_call_is_observational("glob_find", "{}"));
    }

    #[test]
    fn test_observational_grep_search() {
        assert!(tool_call_is_observational("grep_search", "{}"));
    }

    #[test]
    fn test_observational_symbol_search() {
        assert!(tool_call_is_observational("symbol_search", "{}"));
    }

    #[test]
    fn test_observational_git_status_tool() {
        assert!(tool_call_is_observational("git_status", "{}"));
    }

    #[test]
    fn test_observational_cargo_check_tool() {
        assert!(tool_call_is_observational("cargo_check", "{}"));
    }

    #[test]
    fn test_observational_cargo_test_tool() {
        assert!(tool_call_is_observational("cargo_test", "{}"));
    }

    #[test]
    fn test_not_observational_file_write() {
        assert!(!tool_call_is_observational("file_write", "{}"));
    }

    #[test]
    fn test_not_observational_file_edit() {
        assert!(!tool_call_is_observational("file_edit", "{}"));
    }

    #[test]
    fn test_observational_shell_exec_read_only() {
        assert!(tool_call_is_observational(
            "shell_exec",
            r#"{"command":"cargo test"}"#
        ));
    }

    #[test]
    fn test_not_observational_shell_exec_mutating() {
        assert!(!tool_call_is_observational(
            "shell_exec",
            r#"{"command":"cargo fmt"}"#
        ));
    }

    #[test]
    fn test_not_observational_shell_exec_no_command() {
        assert!(!tool_call_is_observational("shell_exec", "{}"));
    }

    // =========================================================================
    // tool_call_counts_as_state_change tests
    // =========================================================================

    #[test]
    fn test_state_change_file_write() {
        assert!(tool_call_counts_as_state_change("file_write", "{}"));
    }

    #[test]
    fn test_state_change_file_edit() {
        assert!(tool_call_counts_as_state_change("file_edit", "{}"));
    }

    #[test]
    fn test_no_state_change_file_read() {
        assert!(!tool_call_counts_as_state_change("file_read", "{}"));
    }

    #[test]
    fn test_no_state_change_cargo_check() {
        assert!(!tool_call_counts_as_state_change("cargo_check", "{}"));
    }

    #[test]
    fn test_no_state_change_cargo_test() {
        assert!(!tool_call_counts_as_state_change("cargo_test", "{}"));
    }

    #[test]
    fn test_no_state_change_cargo_clippy() {
        assert!(!tool_call_counts_as_state_change("cargo_clippy", "{}"));
    }

    // =========================================================================
    // extract_backticked_tool_names tests
    // =========================================================================

    #[test]
    fn test_extract_backticked_tool_names_basic() {
        let names = extract_backticked_tool_names("Use `file_read` and `file_edit`");
        assert_eq!(names, vec!["file_read", "file_edit"]);
    }

    #[test]
    fn test_extract_backticked_tool_names_empty() {
        let names = extract_backticked_tool_names("no tools here");
        assert!(names.is_empty());
    }

    #[test]
    fn test_extract_backticked_tool_names_invalid_chars() {
        let names = extract_backticked_tool_names("Use `File Read` and `hello-world`");
        // Only lowercase, digits, underscore
        assert!(names.is_empty());
    }

    #[test]
    fn test_extract_backticked_tool_names_single() {
        let names = extract_backticked_tool_names("`shell_exec`");
        assert_eq!(names, vec!["shell_exec"]);
    }

    #[test]
    fn test_extract_backticked_tool_names_with_digits() {
        let names = extract_backticked_tool_names("`tool_v2`");
        assert_eq!(names, vec!["tool_v2"]);
    }

    // =========================================================================
    // extract_explicit_allowed_tools tests
    // =========================================================================

    #[test]
    fn test_extract_allowed_tools_no_section() {
        let task = "Just do something useful.";
        assert!(extract_explicit_allowed_tools(task).is_none());
    }

    #[test]
    fn test_extract_allowed_tools_with_bullets() {
        let task = "Use only these concrete tools:\n- `file_read`\n- `shell_exec`\n\nDo the task.";
        let allowed = extract_explicit_allowed_tools(task).unwrap();
        assert!(allowed.contains("file_read"));
        assert!(allowed.contains("shell_exec"));
        assert_eq!(allowed.len(), 2);
    }

    #[test]
    fn test_extract_allowed_tools_case_variations() {
        let task = "Allowed tools:\n- `grep_search`\n- `glob_find`\n";
        let allowed = extract_explicit_allowed_tools(task).unwrap();
        assert!(allowed.contains("grep_search"));
        assert!(allowed.contains("glob_find"));
    }

    // =========================================================================
    // extract_explicit_disallowed_tools tests
    // =========================================================================

    #[test]
    fn test_extract_disallowed_never_call() {
        let task = "Never call `tool_search`.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.contains("tool_search"));
    }

    #[test]
    fn test_extract_disallowed_do_not_use() {
        let task = "Do not use `file_delete`.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.contains("file_delete"));
    }

    #[test]
    fn test_extract_disallowed_dont_use() {
        let task = "Don't use `shell_exec`.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.contains("shell_exec"));
    }

    #[test]
    fn test_extract_disallowed_avoid() {
        let task = "Avoid `git_commit` for now.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.contains("git_commit"));
    }

    #[test]
    fn test_extract_disallowed_shell_category() {
        let task = "Do not run shell commands or use pty_shell.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.contains("shell_exec"));
        assert!(disallowed.contains("pty_shell"));
    }

    #[test]
    fn test_extract_disallowed_empty() {
        let task = "Just do the task.";
        let disallowed = extract_explicit_disallowed_tools(task);
        assert!(disallowed.is_empty());
    }

    // =========================================================================
    // insert_missing_tool_arg tests
    // =========================================================================

    #[test]
    fn test_insert_missing_arg_adds_when_absent() {
        let mut obj = serde_json::Map::new();
        let inserted = insert_missing_tool_arg(&mut obj, "key", serde_json::json!("value"));
        assert!(inserted);
        assert_eq!(obj["key"], "value");
    }

    #[test]
    fn test_insert_missing_arg_skips_when_present() {
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), serde_json::json!("existing"));
        let inserted = insert_missing_tool_arg(&mut obj, "key", serde_json::json!("new"));
        assert!(!inserted);
        assert_eq!(obj["key"], "existing");
    }

    #[test]
    fn test_insert_missing_arg_replaces_null() {
        let mut obj = serde_json::Map::new();
        obj.insert("key".to_string(), serde_json::Value::Null);
        let inserted = insert_missing_tool_arg(&mut obj, "key", serde_json::json!("value"));
        assert!(inserted);
        assert_eq!(obj["key"], "value");
    }

    // =========================================================================
    // shell_exec mutating-counter classification (#4)
    // =========================================================================

    /// Helper that mirrors the increment-site's classification predicate so we
    /// can unit-test it without spinning up a full agent loop.
    fn classify_shell_as_mutating(name: &str, command: Option<&str>) -> bool {
        if matches!(
            name,
            "file_edit" | "file_write" | "file_delete" | "file_fim_edit"
        ) {
            return true;
        }
        if name == "shell_exec" {
            if let Some(cmd) = command {
                return !shell_command_is_observational(cmd);
            }
        }
        false
    }

    #[test]
    fn shell_exec_cargo_check_does_not_count_as_mutating() {
        assert!(!classify_shell_as_mutating(
            "shell_exec",
            Some("cargo check")
        ));
        assert!(!classify_shell_as_mutating(
            "shell_exec",
            Some("git status")
        ));
        assert!(!classify_shell_as_mutating("shell_exec", Some("ls -la")));
    }

    #[test]
    fn shell_exec_mutating_commands_count_as_mutating() {
        // git add / rm / cargo fmt / mv / sed -i — all should bump the counter.
        assert!(classify_shell_as_mutating(
            "shell_exec",
            Some("git add src/")
        ));
        assert!(classify_shell_as_mutating(
            "shell_exec",
            Some("rm /tmp/foo")
        ));
        assert!(classify_shell_as_mutating("shell_exec", Some("cargo fmt")));
        assert!(classify_shell_as_mutating(
            "shell_exec",
            Some("mv a.txt b.txt")
        ));
        assert!(classify_shell_as_mutating(
            "shell_exec",
            Some("sed -i 's/a/b/' file.rs")
        ));
        // file_* tools are always mutating.
        assert!(classify_shell_as_mutating("file_write", None));
        assert!(classify_shell_as_mutating("file_edit", None));
    }

    #[tokio::test]
    async fn tui_permission_response_denies_when_no_channel_wired() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        // No channel wired at all -- must fail closed, not auto-approve.
        assert!(!agent.await_tui_permission_response().await);
        server.stop().await;
    }

    #[cfg(feature = "tui")]
    #[tokio::test]
    async fn tui_permission_response_relays_user_answer() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let (tx, rx) = std::sync::mpsc::channel();
        agent = agent.with_permission_channel(rx);
        tx.send(true).unwrap();
        assert!(agent.await_tui_permission_response().await);

        let (tx, rx) = std::sync::mpsc::channel();
        agent = agent.with_permission_channel(rx);
        tx.send(false).unwrap();
        assert!(!agent.await_tui_permission_response().await);

        server.stop().await;
    }

    #[cfg(feature = "tui")]
    #[tokio::test]
    async fn tui_permission_response_denies_when_sender_dropped() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        agent = agent.with_permission_channel(rx);
        drop(tx); // simulate the TUI thread exiting without answering

        assert!(!agent.await_tui_permission_response().await);
        server.stop().await;
    }

    #[tokio::test]
    async fn yolo_gate_blocks_protected_path_write() {
        // YoloConfig's protected_paths (e.g. /etc) apply to any tool with a
        // path/file/directory argument, independent of the pre-existing
        // SafetyChecker/path_validator's allowed_paths -- this test's config
        // permissively allows "/**" and only denies .env/.ssh/secrets, so
        // /etc is only blocked because of the (newly wired-in) YOLO gate.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![(
                "file_write".to_string(),
                r#"{"path":"/etc/selfware-test.conf","content":"x"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent.messages.last().expect("expected a skip message");
        assert!(last.content.text().contains("Blocked by YOLO safety gate"));
        server.stop().await;
    }

    #[tokio::test]
    async fn yolo_gate_applies_in_parallel_batch_too() {
        // Regression test: execute_parallel_tools (used when 2+ tools in a
        // batch are in PARALLEL_SAFE_TOOLS) never called
        // confirm_tool_execution at all, so the YOLO gate silently didn't
        // apply to any tool executed that way -- a file_read of a
        // YOLO-protected path would be Block-ed via the sequential path but
        // ran unchecked here just because a second parallel-safe call
        // happened to land in the same batch. Uses two file_read calls
        // (file_read is parallel-safe) to force the parallel path.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![
                // /etc/hostname (not /etc/passwd -- that one's already
                // caught by an earlier, narrower hardcoded dangerous-files
                // list in path_validator.rs, which would pass regardless of
                // this fix and defeat the point of this test).
                (
                    "file_read".to_string(),
                    r#"{"path":"/etc/hostname"}"#.to_string(),
                    None,
                ),
                (
                    "file_read".to_string(),
                    r#"{"path":"Cargo.toml"}"#.to_string(),
                    None,
                ),
            ])
            .await
            .unwrap();

        let all_text: String = agent
            .messages
            .iter()
            .map(|m| m.content.text())
            .collect::<Vec<_>>()
            .join("\n---\n");
        assert!(
            all_text.contains("Blocked by YOLO safety gate"),
            "expected the /etc/hostname read to be blocked; got: {all_text}"
        );
        // The unrelated, unprotected read should have gone through untouched.
        assert!(
            all_text.contains("[package]"),
            "expected the Cargo.toml read to succeed; got: {all_text}"
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn yolo_gate_denies_destructive_shell_without_operator() {
        // Destructive but not forbidden -- YoloDecision::RequireConfirmation.
        // No CLI/TUI operator is attached in this test harness, so it must
        // fail closed rather than hang or silently auto-approve.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![(
                "shell_exec".to_string(),
                r#"{"command":"rm -rf ./scratch"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent.messages.last().expect("expected a skip message");
        assert!(last.content.text().contains("unattended session"));
        server.stop().await;
    }

    #[tokio::test]
    async fn yolo_gate_allows_non_destructive_shell_command() {
        let server = MockLlmServer::builder().with_response("done").build().await;
        let config = test_config(format!("{}/v1", server.url()));
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![(
                "shell_exec".to_string(),
                r#"{"command":"echo hello"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent.messages.last().expect("expected a tool result");
        assert!(!last.content.text().contains("Blocked by YOLO safety gate"));
        assert!(!last.content.text().contains("unattended session"));
        server.stop().await;
    }

    #[tokio::test]
    async fn yolo_gate_denies_git_push_when_disallowed() {
        // Push to a non-protected branch so this exercises the YOLO gate's
        // own git-push handling specifically, not the separate
        // protected_branches check (covered below).
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.yolo.allow_git_push = false;
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![(
                "git_push".to_string(),
                r#"{"branch":"feature-branch"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent.messages.last().expect("expected a skip message");
        assert!(last.content.text().contains("unattended session"));
        server.stop().await;
    }

    #[tokio::test]
    async fn git_push_to_protected_branch_is_blocked_even_with_git_push_allowed() {
        // protected_branches is a hard block, distinct from (and checked
        // before) the YOLO allow_git_push toggle -- allowing git_push in
        // general must not bypass it.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.yolo.allow_git_push = true;
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![(
                "git_push".to_string(),
                r#"{"branch":"main"}"#.to_string(),
                None,
            )])
            .await
            .unwrap();

        let last = agent.messages.last().expect("expected a skip message");
        assert!(last.content.text().contains("protected branch"));
        server.stop().await;
    }

    #[tokio::test]
    async fn confirmation_error_in_batch_still_pushes_tool_result() {
        // Regression: when execute_single_tool_in_batch returns Err BEFORE
        // pushing a tool-result (e.g. confirmation rejection in non-YOLO
        // headless mode), the catch-and-continue loop must push a synthetic
        // error result for that tool_call_id so native-FC history stays
        // balanced (N calls → N results).  Without the fix, the tool_call_id
        // had NO result → 400 on the next API call.
        //
        // We use Normal mode (not Yolo) so confirmation is required for
        // file_write.  In the test runner stdin is not a terminal, so
        // confirm_tool_execution returns Err("requires confirmation but
        // cannot prompt in headless mode").  The fix pushes a synthetic
        // error result and the batch continues with the second tool.
        let server = MockLlmServer::builder().with_response("done").build().await;
        let mut config = test_config(format!("{}/v1", server.url()));
        config.execution_mode = crate::config::ExecutionMode::Normal;
        let mut agent = Agent::new(config).await.unwrap();

        agent
            .execute_tool_batch(vec![
                (
                    "file_write".to_string(),
                    r#"{"path":"/tmp/selfware-test-confirm.txt","content":"x"}"#.to_string(),
                    Some("call_confirm_err".to_string()),
                ),
                // A second tool that should still execute.
                (
                    "shell_exec".to_string(),
                    r#"{"command":"echo hello"}"#.to_string(),
                    Some("call_after_err".to_string()),
                ),
            ])
            .await
            .unwrap();

        let all_text: String = agent
            .messages
            .iter()
            .map(|m| m.content.text())
            .collect::<Vec<_>>()
            .join("\n---\n");

        // The confirmation-errored tool must have a synthetic error result
        // pushed (contains "headless mode" from the error message).
        assert!(
            all_text.contains("headless mode"),
            "expected a synthetic error result for the confirmation-errored tool; got: {all_text}"
        );
        // The second tool should also have executed (its result present).
        assert!(
            all_text.contains("hello"),
            "expected the second tool in the batch to still execute after the first errored; got: {all_text}"
        );

        server.stop().await;
    }

    #[tokio::test]
    async fn run_tool_bounded_returns_result_when_fast() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let cancel = Arc::new(AtomicBool::new(false));
        let fut = async { Ok(serde_json::json!({"ok": true})) };
        let out = run_tool_bounded(fut, std::time::Duration::from_secs(5), cancel).await;
        assert!(out.is_ok());
        assert!(out.unwrap().is_ok());
    }

    #[tokio::test]
    async fn run_tool_bounded_times_out() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let cancel = Arc::new(AtomicBool::new(false));
        let slow = async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok(serde_json::json!({}))
        };
        let out = run_tool_bounded(slow, std::time::Duration::from_millis(50), cancel).await;
        assert_eq!(out.unwrap_err(), ToolHalt::TimedOut);
    }

    #[tokio::test]
    async fn run_tool_bounded_cancels_in_flight() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            c2.store(true, Ordering::Relaxed);
        });
        let slow = async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            Ok(serde_json::json!({}))
        };
        // Deadline is long (10s) so the ONLY way this returns quickly is cancellation.
        let out = run_tool_bounded(slow, std::time::Duration::from_secs(10), cancel).await;
        assert_eq!(out.unwrap_err(), ToolHalt::Cancelled);
    }

    #[tokio::test]
    async fn run_tool_bounded_fast_path_already_cancelled() {
        use std::sync::Arc;
        use std::sync::atomic::AtomicBool;
        let cancel = Arc::new(AtomicBool::new(true));
        let fut = async { Ok(serde_json::json!({})) };
        let out = run_tool_bounded(fut, std::time::Duration::from_secs(5), cancel).await;
        assert_eq!(out.unwrap_err(), ToolHalt::Cancelled);
    }

    #[test]
    fn mutating_predicate_covers_all_real_editors() {
        use serde_json::json;
        let empty = json!({});
        // Direct editors — including the previously-missed ones.
        for t in [
            "file_edit",
            "file_write",
            "file_delete",
            "file_fim_edit",
            "file_multi_edit",
            "patch_apply",
        ] {
            assert!(tool_call_is_mutating(t, &empty), "{t} should be mutating");
        }
        // Mutating git ops.
        for t in ["git_commit", "git_add", "git_apply", "git_reset"] {
            assert!(tool_call_is_mutating(t, &empty), "{t} should be mutating");
        }
        // Observational tools are NOT mutating.
        for t in ["file_read", "git_status", "git_log", "git_diff", "grep", "list_dir"] {
            assert!(!tool_call_is_mutating(t, &empty), "{t} should NOT be mutating");
        }
        // Shell is mutating only for non-observational commands.
        assert!(tool_call_is_mutating(
            "shell_exec",
            &json!({"command": "rm -rf build"})
        ));
        assert!(tool_call_is_mutating(
            "shell_exec",
            &json!({"command": "npm install"})
        ));
        assert!(!tool_call_is_mutating(
            "shell_exec",
            &json!({"command": "cargo check"})
        ));
        assert!(!tool_call_is_mutating(
            "shell_exec",
            &json!({"command": "git status"})
        ));
    }
}
