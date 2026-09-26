//! Human-readable rendering of a tool call awaiting confirmation.
//!
//! The confirmation prompt used to print the raw, escaped JSON arguments cut
//! at ~240 characters (`{"new_str":"    *,\n    replacement_stage: ...`), so
//! an operator approving a `file_edit` saw neither the `old_str` nor a diff
//! nor how many lines changed. This module turns a tool call into a bounded
//! list of typed [`ConfirmLine`]s that both front ends render:
//!
//! - `file_edit` / `file_multi_edit` / `file_write` (over an existing file):
//!   a unified diff with a path header and `+N −M lines`,
//! - `file_write` of a new file: path, line count and the first lines,
//! - `patch_apply`: the patch itself, coloured, with the same counts,
//! - every other tool: `key: value` lines with strings unescaped.
//!
//! Everything here is pure (the caller supplies any existing file content),
//! bounded (at most [`MAX_BODY_LINES`] body lines, each at most
//! [`MAX_LINE_CHARS`] characters) and sanitized: control characters —
//! including ESC, which would let model-authored arguments repaint the
//! terminal and spoof the prompt — are shown escaped, never emitted.

use serde_json::Value;

/// Maximum number of body lines (diff or argument lines) in one prompt.
pub const MAX_BODY_LINES: usize = 40;
/// Maximum characters shown per rendered line.
pub const MAX_LINE_CHARS: usize = 200;
/// Maximum lines shown for one multi-line string argument (non-diff tools).
const MAX_LINES_PER_VALUE: usize = 8;
/// Maximum characters of a shell command shown (commands are shown in full up
/// to this bound — the operator must see what runs).
const MAX_COMMAND_CHARS: usize = 2000;
/// First lines of a NEW file shown by a `file_write` prompt.
const NEW_FILE_PREVIEW_LINES: usize = 20;
/// Unified-diff context lines around each change.
const DIFF_CONTEXT_LINES: usize = 2;

/// What a rendered confirmation line represents (drives colouring).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A path / section header (`src/lib.rs  +3 −1 lines`).
    Header,
    /// A diff hunk marker (`@@ -1,3 +1,4 @@`).
    Hunk,
    /// An added line (`+…`).
    Added,
    /// A removed line (`-…`).
    Removed,
    /// Unchanged diff context.
    Context,
    /// A `key: value` argument line.
    Field,
    /// Secondary information (truncation notes, "new file", …).
    Note,
}

/// One line of the rendered confirmation body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmLine {
    /// How the line should be styled.
    pub kind: LineKind,
    /// The already sanitized, already truncated text.
    pub text: String,
}

impl ConfirmLine {
    fn new(kind: LineKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// Replace control characters with visible escapes and cap the length.
///
/// Tabs become two spaces; every other control character (ESC, CR, BEL,
/// C1 controls, …) is shown as `\u{..}` so a model-authored argument can
/// never emit a terminal escape sequence into the prompt.
pub fn sanitize_line(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut count = 0usize;
    let mut truncated = false;
    for ch in input.chars() {
        let piece: String = if ch == '\t' {
            "  ".to_string()
        } else if ch.is_control() {
            format!("\\u{{{:x}}}", ch as u32)
        } else {
            ch.to_string()
        };
        let len = piece.chars().count();
        if count + len > max_chars {
            truncated = true;
            break;
        }
        count += len;
        out.push_str(&piece);
    }
    if truncated {
        out.push('…');
    }
    out
}

/// Accept the argument-name aliases the file tools accept.
fn str_arg<'a>(args: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()))
}

fn path_arg(args: &Value) -> Option<&str> {
    str_arg(args, &["path", "file_path", "file", "filepath"])
}

/// A diff of `old` → `new` under a header, appended to `out` within `budget`
/// body lines. Returns the (added, removed, hidden-by-the-cap) line counts.
fn push_text_diff(
    old: &str,
    new: &str,
    out: &mut Vec<ConfirmLine>,
    budget: &mut usize,
) -> (usize, usize, usize) {
    use similar::{ChangeTag, TextDiff};
    let diff = TextDiff::from_lines(old, new);
    let mut added = 0usize;
    let mut removed = 0usize;
    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Insert => added += 1,
            ChangeTag::Delete => removed += 1,
            ChangeTag::Equal => {}
        }
    }
    let mut hidden = 0usize;
    for group in diff.grouped_ops(DIFF_CONTEXT_LINES) {
        for op in group {
            for change in diff.iter_changes(&op) {
                if *budget == 0 {
                    hidden += 1;
                    continue;
                }
                let raw = change.value().trim_end_matches(['\n', '\r']);
                let (kind, sign) = match change.tag() {
                    ChangeTag::Insert => (LineKind::Added, '+'),
                    ChangeTag::Delete => (LineKind::Removed, '-'),
                    ChangeTag::Equal => (LineKind::Context, ' '),
                };
                out.push(ConfirmLine::new(
                    kind,
                    format!("{}{}", sign, sanitize_line(raw, MAX_LINE_CHARS)),
                ));
                *budget -= 1;
            }
        }
    }
    (added, removed, hidden)
}

fn counts_label(added: usize, removed: usize) -> String {
    format!("+{} −{} lines", added, removed)
}

fn more_lines_note(hidden: usize) -> ConfirmLine {
    ConfirmLine::new(
        LineKind::Note,
        format!(
            "… {} more line{}",
            hidden,
            if hidden == 1 { "" } else { "s" }
        ),
    )
}

/// Render one or more `(path, old, new)` edits as a bounded unified diff.
fn render_edits(edits: &[(String, String, String)]) -> Vec<ConfirmLine> {
    let mut out = Vec::new();
    let mut budget = MAX_BODY_LINES;
    let mut hidden_total = 0usize;
    for (path, old, new) in edits {
        let header_idx = out.len();
        out.push(ConfirmLine::new(LineKind::Header, String::new()));
        let (added, removed, hidden) = push_text_diff(old, new, &mut out, &mut budget);
        hidden_total += hidden;
        out[header_idx].text = format!(
            "{}  {}",
            sanitize_line(path, MAX_LINE_CHARS),
            counts_label(added, removed)
        );
    }
    if hidden_total > 0 {
        out.push(more_lines_note(hidden_total));
    }
    out
}

/// Render a `file_write`: a diff against `existing` when the file exists,
/// otherwise the new file's line count and first lines.
pub fn render_file_write(path: &str, content: &str, existing: Option<&str>) -> Vec<ConfirmLine> {
    if let Some(old) = existing {
        let mut lines = render_edits(&[(path.to_string(), old.to_string(), content.to_string())]);
        if let Some(first) = lines.first_mut() {
            first.text.push_str("  (overwrite)");
        }
        return lines;
    }
    let total = content.lines().count();
    let mut out = vec![ConfirmLine::new(
        LineKind::Header,
        format!(
            "{}  new file, {} line{}",
            sanitize_line(path, MAX_LINE_CHARS),
            total,
            if total == 1 { "" } else { "s" }
        ),
    )];
    for line in content.lines().take(NEW_FILE_PREVIEW_LINES) {
        out.push(ConfirmLine::new(
            LineKind::Added,
            format!("+{}", sanitize_line(line, MAX_LINE_CHARS)),
        ));
    }
    if total > NEW_FILE_PREVIEW_LINES {
        out.push(more_lines_note(total - NEW_FILE_PREVIEW_LINES));
    }
    out
}

/// Render a unified diff (`patch_apply`) with per-file counts and the cap.
pub fn render_patch(diff: &str) -> Vec<ConfirmLine> {
    let mut added = 0usize;
    let mut removed = 0usize;
    let mut files = 0usize;
    for line in diff.lines() {
        if line.starts_with("+++") {
            files += 1;
        } else if line.starts_with("---") {
        } else if line.starts_with('+') {
            added += 1;
        } else if line.starts_with('-') {
            removed += 1;
        }
    }
    let mut out = vec![ConfirmLine::new(
        LineKind::Header,
        format!(
            "patch: {} file{}  {}",
            files,
            if files == 1 { "" } else { "s" },
            counts_label(added, removed)
        ),
    )];
    let total = diff.lines().count();
    for line in diff.lines().take(MAX_BODY_LINES) {
        let kind =
            if line.starts_with("+++") || line.starts_with("---") || line.starts_with("diff ") {
                LineKind::Header
            } else if line.starts_with("@@") {
                LineKind::Hunk
            } else if line.starts_with('+') {
                LineKind::Added
            } else if line.starts_with('-') {
                LineKind::Removed
            } else {
                LineKind::Context
            };
        out.push(ConfirmLine::new(kind, sanitize_line(line, MAX_LINE_CHARS)));
    }
    if total > MAX_BODY_LINES {
        out.push(more_lines_note(total - MAX_BODY_LINES));
    }
    out
}

/// Render arbitrary arguments as `key: value` lines (strings unescaped).
pub fn render_fields(tool_name: &str, args: &Value) -> Vec<ConfirmLine> {
    let Some(map) = args.as_object() else {
        return vec![ConfirmLine::new(
            LineKind::Field,
            sanitize_line(&args.to_string(), MAX_LINE_CHARS),
        )];
    };
    if map.is_empty() {
        return vec![ConfirmLine::new(LineKind::Note, "(no arguments)")];
    }
    let is_command_tool = matches!(tool_name, "shell_exec" | "pty_shell");
    let mut out = Vec::new();
    let mut hidden = 0usize;
    for (key, value) in map {
        let key = sanitize_line(key, 40);
        match value {
            Value::String(s) if is_command_tool && (key == "command" || key == "cmd") => {
                // The command is what runs: show all of it (bounded), wrapped
                // over several lines rather than cut at the line width.
                let shown = sanitize_line(s, MAX_COMMAND_CHARS);
                let chars: Vec<char> = shown.chars().collect();
                for (i, chunk) in chars.chunks(MAX_LINE_CHARS).enumerate() {
                    if out.len() >= MAX_BODY_LINES {
                        hidden += 1;
                        continue;
                    }
                    let text: String = chunk.iter().collect();
                    out.push(ConfirmLine::new(
                        LineKind::Field,
                        if i == 0 {
                            format!("{}: {}", key, text)
                        } else {
                            format!("  {}", text)
                        },
                    ));
                }
            }
            Value::String(s) if s.contains('\n') => {
                let total = s.lines().count();
                if out.len() >= MAX_BODY_LINES {
                    hidden += total + 1;
                    continue;
                }
                out.push(ConfirmLine::new(
                    LineKind::Field,
                    format!("{}: ({} lines)", key, total),
                ));
                for line in s.lines().take(MAX_LINES_PER_VALUE) {
                    if out.len() >= MAX_BODY_LINES {
                        hidden += 1;
                        continue;
                    }
                    out.push(ConfirmLine::new(
                        LineKind::Context,
                        format!("  {}", sanitize_line(line, MAX_LINE_CHARS)),
                    ));
                }
                if total > MAX_LINES_PER_VALUE {
                    hidden += total - MAX_LINES_PER_VALUE;
                }
            }
            other => {
                if out.len() >= MAX_BODY_LINES {
                    hidden += 1;
                    continue;
                }
                let text = match other {
                    Value::String(s) => s.clone(),
                    v => v.to_string(),
                };
                out.push(ConfirmLine::new(
                    LineKind::Field,
                    format!("{}: {}", key, sanitize_line(&text, MAX_LINE_CHARS)),
                ));
            }
        }
    }
    if hidden > 0 {
        out.push(more_lines_note(hidden));
    }
    out
}

/// Render the confirmation body for any tool call.
///
/// `args_str` is the raw argument string (shown, sanitized and bounded, only
/// when it is not a JSON object). `existing_file` is the current content of a
/// `file_write` target when it exists — the caller reads it; this stays pure.
pub fn render_tool_call(
    tool_name: &str,
    args_str: &str,
    existing_file: Option<&str>,
) -> Vec<ConfirmLine> {
    let args: Value = match serde_json::from_str(args_str) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            return vec![
                ConfirmLine::new(LineKind::Note, "(arguments are not a JSON object)"),
                ConfirmLine::new(LineKind::Field, sanitize_line(args_str, MAX_LINE_CHARS)),
            ]
        }
    };
    match tool_name {
        "file_edit" => {
            if let (Some(path), Some(old), Some(new)) = (
                path_arg(&args),
                str_arg(&args, &["old_str", "old_string"]),
                str_arg(&args, &["new_str", "new_string"]),
            ) {
                return render_edits(&[(path.to_string(), old.to_string(), new.to_string())]);
            }
        }
        "file_multi_edit" => {
            if let Some(items) = args.get("edits").and_then(|e| e.as_array()) {
                let edits: Option<Vec<(String, String, String)>> = items
                    .iter()
                    .map(|item| {
                        Some((
                            path_arg(item)?.to_string(),
                            str_arg(item, &["old_str", "old_string"])?.to_string(),
                            str_arg(item, &["new_str", "new_string"])?.to_string(),
                        ))
                    })
                    .collect();
                if let Some(edits) = edits.filter(|e| !e.is_empty()) {
                    return render_edits(&edits);
                }
            }
        }
        "file_write" => {
            if let (Some(path), Some(content)) = (path_arg(&args), str_arg(&args, &["content"])) {
                return render_file_write(path, content, existing_file);
            }
        }
        "patch_apply" => {
            if let Some(diff) = str_arg(&args, &["diff"]) {
                return render_patch(diff);
            }
        }
        _ => {}
    }
    render_fields(tool_name, &args)
}

/// The `file_write` target whose current content the prompt should diff
/// against, if the call is a `file_write`.
pub fn file_write_target(tool_name: &str, args_str: &str) -> Option<String> {
    if tool_name != "file_write" {
        return None;
    }
    let args: Value = serde_json::from_str(args_str).ok()?;
    path_arg(&args).map(str::to_string)
}

#[cfg(test)]
#[path = "../../tests/unit/safety/confirm_view/confirm_view_test.rs"]
mod tests;
