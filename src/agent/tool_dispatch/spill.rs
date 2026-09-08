use serde_json::Value;
use tracing::warn;

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

pub(crate) const TOOL_CONFIRM_ARGS_PREVIEW_CHARS: usize = 240;
/// Char budget for the last-attempt error quoted in the retry-suppression
/// message. Truncation is TAIL-preserving (see `truncate_chars_tail`): the
/// actionable part of an error — missing field, line/column, blocked
/// pattern — sits at the end, so the head is what gets cut.
pub(crate) const RETRY_SUPPRESSION_ERROR_PREVIEW_CHARS: usize = 300;
/// Char budget for the retry-suppression message BODY (everything after the
/// policy-envelope marker line). If the assembled body exceeds this, the
/// quoted error is shrunk first — the failure category and suggested_fix
/// lines are the actionable part and are never cut.
pub(crate) const RETRY_SUPPRESSED_BODY_BUDGET_CHARS: usize = 500;
pub(crate) const FAILED_TOOL_ATTEMPT_WINDOW_SIZE: usize = 16;

/// Maximum tokens allowed for a single tool result before it gets summarized.
/// ~5% of a 1M context window — leaves room for many tool results in one session.
pub(crate) const MAX_TOOL_RESULT_TOKENS: usize = 50_000;

/// Directory where oversized raw tool results are spilled to disk.
pub(crate) const TOOL_RESULTS_DIR: &str = ".selfware/tool_results";

/// Summarize an oversized tool result and save raw data to disk.
///
/// Returns a structured summary string that includes:
/// - Key statistics extracted from the result
/// - A reference path to the raw data on disk
/// - Enough context for the agent to decide whether to drill down
pub(crate) async fn summarize_and_spill(
    tool_name: &str,
    call_id: &str,
    raw: &str,
    estimated_tokens: usize,
) -> String {
    // Redact secrets BEFORE the result is written to disk or summarized into
    // context. An oversized tool result — shell output, a file_read of a .env,
    // a git_diff touching a key — can carry credentials, and the spill file is
    // plaintext on disk (under .selfware/, gitignored but still on the box).
    // Redacting here covers both the spill and every summary built from `raw`.
    let redacted = crate::safety::redact::redact_secrets(raw);
    let raw: &str = redacted.as_ref();

    // Save the redacted result to disk
    let spill_dir = std::path::Path::new(TOOL_RESULTS_DIR);
    let _ = tokio::fs::create_dir_all(spill_dir).await;
    // Keep the agent's scratch out of the user's repo: drop a .gitignore into
    // the project-local .selfware/ (the spill dir's parent).
    if let Some(selfware_dir) = spill_dir.parent() {
        crate::agent::turn_artifacts::ensure_selfware_gitignore(selfware_dir);
    }
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

pub(crate) fn tool_result_value_indicates_success(result: &Value) -> bool {
    if result.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }
    if result.get("passed").and_then(|v| v.as_bool()) == Some(false) {
        return false;
    }
    // A truthy top-level `error` key means the tool reported a failure even
    // if it returned a structured payload (e.g. CONTEXT_LOAD_SKELETON read
    // errors). Honest status over optimistic success.
    if result.get("error").is_some_and(is_truthy_json) {
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

/// JSON truthiness: `null`/`false`/`""`/`0` are falsy, everything else truthy.
/// Mirrors how a payload like `{"error": null}` or `{"error": false}` carries
/// no failure signal, while `{"error": "..."}` does.
fn is_truthy_json(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

pub(crate) fn summarize_directory_tree(raw: &str) -> String {
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

pub(crate) fn summarize_file_read(raw: &str) -> String {
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

pub(crate) fn summarize_git_diff(raw: &str) -> String {
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

pub(crate) fn summarize_bulk_read(raw: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(raw).unwrap_or_default();
    let loaded = v.get("loaded").and_then(|v| v.as_u64()).unwrap_or(0);
    let skipped = v.get("skipped").and_then(|v| v.as_u64()).unwrap_or(0);
    let tokens = v.get("tokens_added").and_then(|v| v.as_u64()).unwrap_or(0);
    format!(
        "Bulk read: {} files loaded, {} skipped, {} tokens added",
        loaded, skipped, tokens
    )
}

pub(crate) fn summarize_shell_exec(raw: &str) -> String {
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

pub(crate) fn summarize_generic(raw: &str) -> String {
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
