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
/// Upper bound on the lines of a full diff (`v` at the prompt) — a pager
/// can page far more than a prompt shows, but not without limit.
pub const FULL_VIEW_MAX_LINES: usize = 20_000;
/// Characters per line in a full diff (lines are still sanitized).
pub const FULL_VIEW_LINE_CHARS: usize = 2_000;

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
    line_chars: usize,
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
                    format!("{}{}", sign, sanitize_line(raw, line_chars)),
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

/// How much of a call a rendering shows: the prompt body, or the full diff.
#[derive(Debug, Clone, Copy)]
struct Limits {
    body_lines: usize,
    line_chars: usize,
    new_file_lines: usize,
}

const PROMPT_LIMITS: Limits = Limits {
    body_lines: MAX_BODY_LINES,
    line_chars: MAX_LINE_CHARS,
    new_file_lines: NEW_FILE_PREVIEW_LINES,
};

const FULL_LIMITS: Limits = Limits {
    body_lines: FULL_VIEW_MAX_LINES,
    line_chars: FULL_VIEW_LINE_CHARS,
    new_file_lines: FULL_VIEW_MAX_LINES,
};

/// Render one or more `(path, old, new)` edits as a unified diff bounded by
/// `limits`.
fn render_edits_within(edits: &[(String, String, String)], limits: Limits) -> Vec<ConfirmLine> {
    let mut out = Vec::new();
    let mut budget = limits.body_lines;
    let mut hidden_total = 0usize;
    for (path, old, new) in edits {
        let header_idx = out.len();
        out.push(ConfirmLine::new(LineKind::Header, String::new()));
        let (added, removed, hidden) =
            push_text_diff(old, new, &mut out, &mut budget, limits.line_chars);
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
    render_file_write_within(path, content, existing, PROMPT_LIMITS)
}

fn render_file_write_within(
    path: &str,
    content: &str,
    existing: Option<&str>,
    limits: Limits,
) -> Vec<ConfirmLine> {
    if let Some(old) = existing {
        let mut lines = render_edits_within(
            &[(path.to_string(), old.to_string(), content.to_string())],
            limits,
        );
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
    for line in content.lines().take(limits.new_file_lines) {
        out.push(ConfirmLine::new(
            LineKind::Added,
            format!("+{}", sanitize_line(line, limits.line_chars)),
        ));
    }
    if total > limits.new_file_lines {
        out.push(more_lines_note(total - limits.new_file_lines));
    }
    out
}

/// Render a unified diff (`patch_apply`) with per-file counts and the cap.
pub fn render_patch(diff: &str) -> Vec<ConfirmLine> {
    render_patch_within(diff, PROMPT_LIMITS)
}

fn render_patch_within(diff: &str, limits: Limits) -> Vec<ConfirmLine> {
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
    for line in diff.lines().take(limits.body_lines) {
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
        out.push(ConfirmLine::new(
            kind,
            sanitize_line(line, limits.line_chars),
        ));
    }
    if total > limits.body_lines {
        out.push(more_lines_note(total - limits.body_lines));
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
    if let Some(lines) = render_diff_within(tool_name, args_str, existing_file, PROMPT_LIMITS) {
        return lines;
    }
    let args: Value = match serde_json::from_str(args_str) {
        Ok(v @ Value::Object(_)) => v,
        _ => {
            return vec![
                ConfirmLine::new(LineKind::Note, "(arguments are not a JSON object)"),
                ConfirmLine::new(LineKind::Field, sanitize_line(args_str, MAX_LINE_CHARS)),
            ]
        }
    };
    render_fields(tool_name, &args)
}

/// The diff view of a file-changing call (`file_edit`, `file_multi_edit`,
/// `file_write`, `patch_apply`) within `limits`; `None` for other tools or
/// arguments that do not describe a diff.
fn render_diff_within(
    tool_name: &str,
    args_str: &str,
    existing_file: Option<&str>,
    limits: Limits,
) -> Option<Vec<ConfirmLine>> {
    let args: Value = match serde_json::from_str(args_str) {
        Ok(v @ Value::Object(_)) => v,
        _ => return None,
    };
    match tool_name {
        "file_edit" => {
            if let (Some(path), Some(old), Some(new)) = (
                path_arg(&args),
                str_arg(&args, &["old_str", "old_string"]),
                str_arg(&args, &["new_str", "new_string"]),
            ) {
                return Some(render_edits_within(
                    &[(path.to_string(), old.to_string(), new.to_string())],
                    limits,
                ));
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
                    return Some(render_edits_within(&edits, limits));
                }
            }
        }
        "file_write" => {
            if let (Some(path), Some(content)) = (path_arg(&args), str_arg(&args, &["content"])) {
                return Some(render_file_write_within(
                    path,
                    content,
                    existing_file,
                    limits,
                ));
            }
        }
        "patch_apply" => {
            if let Some(diff) = str_arg(&args, &["diff"]) {
                return Some(render_patch_within(diff, limits));
            }
        }
        _ => {}
    }
    None
}

/// The full diff of a file-changing call for the prompt's `v` (view full
/// diff) answer — bounded by [`FULL_VIEW_MAX_LINES`] /
/// [`FULL_VIEW_LINE_CHARS`] and sanitized like the prompt body. `None` when
/// the call is not a diff, or when `shown` (the prompt body) already shows
/// all of it: `v` is offered only when there is more to see.
pub fn full_diff_view(
    tool_name: &str,
    args_str: &str,
    existing_file: Option<&str>,
    shown: &[ConfirmLine],
) -> Option<Vec<ConfirmLine>> {
    render_diff_within(tool_name, args_str, existing_file, FULL_LIMITS).filter(|full| full != shown)
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

// ---------------------------------------------------------------------------
// Front-end neutral prompt / answer
// ---------------------------------------------------------------------------

/// A tool-permission request, as an interactive front end (the TUI modal)
/// renders it. Built by the dispatcher from the same pieces the CLI prompt
/// prints, so both front ends show the same content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionPrompt {
    /// The tool awaiting approval.
    pub tool_name: String,
    /// Short risk tag (`[installs packages]`, …).
    pub risk: RiskTag,
    /// Why the call needs confirmation.
    pub reason: Option<String>,
    /// Readable, bounded arguments / diff ([`render_tool_call`]).
    pub body: Vec<ConfirmLine>,
    /// Whether "always allow this tool (session)" is offered.
    pub allow_always: bool,
    /// The offered session shell rule, described (e.g. "commands starting
    /// with `cargo test`"), if any.
    pub shell_rule: Option<String>,
    /// The full diff behind a truncated `body` (`v` = view full diff), when
    /// there is more to see. Viewing it never answers the prompt.
    pub full_view: Option<FullDiffView>,
}

/// The full diff a prompt can show on `v`, and whether it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullDiffView {
    /// The full diff ([`full_diff_view`]).
    pub lines: Vec<ConfirmLine>,
    /// Whether the full diff replaces the bounded body right now.
    pub open: bool,
    /// First shown line of `lines` while open.
    pub scroll: usize,
}

impl FullDiffView {
    /// A closed view of `lines`.
    pub fn new(lines: Vec<ConfirmLine>) -> Self {
        Self {
            lines,
            open: false,
            scroll: 0,
        }
    }

    /// Scroll by `delta` lines, clamped to the diff.
    pub fn scroll_by(&mut self, delta: isize) {
        let max = self.lines.len().saturating_sub(1);
        self.scroll = self.scroll.saturating_add_signed(delta).min(max);
    }
}

impl PermissionPrompt {
    /// One-line summary for logs / status lines.
    pub fn summary(&self) -> String {
        let head = self
            .body
            .first()
            .map(|line| format!(" — {}", sanitize_line(&line.text, 80)))
            .unwrap_or_default();
        format!("{} {}{}", self.tool_name, self.risk.label(), head)
    }
}

/// An interactive front end's answer to a [`PermissionPrompt`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionAnswer {
    /// Do not run the call.
    Deny,
    /// Run this call once.
    Once,
    /// Run it and allow this tool for the rest of the session.
    AlwaysTool,
    /// Run it and add the offered shell rule for the session.
    ShellRule,
}

// ---------------------------------------------------------------------------
// Risk tags
// ---------------------------------------------------------------------------

/// A short, prompt-facing label for what a call can do.
///
/// `pip3 install -r dev.requirements.txt` and `grep` used to look identical
/// at the prompt. The tag is a *display* hint derived from the tool plus
/// command heuristics; it never grants anything — approval policy lives in
/// `tool_metadata` / the permission store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskTag {
    /// Only reads the workspace / state.
    Reads,
    /// Runs a command or program whose effects are not classified further.
    RunsCommand,
    /// Writes files in the workspace.
    WritesWorkspace,
    /// Talks to the network.
    Network,
    /// Installs packages / dependencies.
    InstallsPackages,
    /// Rewrites git history or publishes it (commit, reset, push, …).
    GitHistory,
    /// Deletes files.
    DeletesFiles,
    /// A tool with no explicit safety classification (e.g. `mcp_*`).
    Unclassified,
}

impl RiskTag {
    /// The bracketed label shown in the prompt, e.g. `[installs packages]`.
    pub fn label(self) -> &'static str {
        match self {
            RiskTag::Reads => "[reads]",
            RiskTag::RunsCommand => "[runs command]",
            RiskTag::WritesWorkspace => "[writes workspace]",
            RiskTag::Network => "[network]",
            RiskTag::InstallsPackages => "[installs packages]",
            RiskTag::GitHistory => "[git history]",
            RiskTag::DeletesFiles => "[deletes files]",
            RiskTag::Unclassified => "[unclassified tool]",
        }
    }

    /// Whether the tag marks a call that only reads.
    pub fn is_read_only(self) -> bool {
        self == RiskTag::Reads
    }

    /// Severity used to pick one tag for a compound shell command (higher
    /// wins).
    fn severity(self) -> u8 {
        match self {
            RiskTag::Reads => 0,
            RiskTag::RunsCommand => 1,
            RiskTag::WritesWorkspace => 2,
            RiskTag::Network => 3,
            RiskTag::InstallsPackages => 4,
            RiskTag::GitHistory => 5,
            RiskTag::DeletesFiles => 6,
            RiskTag::Unclassified => 7,
        }
    }
}

/// Classify a tool call for the prompt's risk tag.
pub fn classify_risk(tool_name: &str, args: &Value) -> RiskTag {
    match tool_name {
        "shell_exec" | "pty_shell" => {
            return match str_arg(args, &["command", "cmd"]) {
                Some(cmd) if !cmd.trim().is_empty() => classify_shell_risk(cmd),
                _ => RiskTag::RunsCommand,
            };
        }
        "file_delete" => return RiskTag::DeletesFiles,
        "git_commit" | "git_push" | "git_checkpoint" => return RiskTag::GitHistory,
        "npm_install" | "pip_install" | "yarn_install" => return RiskTag::InstallsPackages,
        "cargo_fmt" => return RiskTag::WritesWorkspace,
        "cargo_clippy" => {
            return if args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false) {
                RiskTag::WritesWorkspace
            } else {
                RiskTag::RunsCommand
            };
        }
        "cargo_test" | "cargo_check" => return RiskTag::RunsCommand,
        _ => {}
    }
    if crate::tools::context::is_context_tool(tool_name) {
        return RiskTag::Reads;
    }
    match crate::safety::classify_tool_metadata(tool_name) {
        None => RiskTag::Unclassified,
        Some(meta) if meta.destructive && !meta.shell_execution => RiskTag::DeletesFiles,
        Some(meta) if meta.network_access => RiskTag::Network,
        Some(meta) if meta.shell_execution => RiskTag::RunsCommand,
        Some(meta) if meta.read_only => RiskTag::Reads,
        Some(_) => RiskTag::WritesWorkspace,
    }
}

/// Heuristic risk tag for a shell command line. Compound commands take the
/// most severe tag of their segments; anything not positively recognised
/// as a read is at least `[runs command]`.
pub fn classify_shell_risk(command: &str) -> RiskTag {
    let lower = command.to_lowercase();
    let mut worst = RiskTag::Reads;
    // Newlines separate commands too; the dispatcher's segment splitter is
    // quote-aware and splits on `;`, `&&`, `||`, `|` and a backgrounding `&`.
    for segment in lower
        .lines()
        .flat_map(crate::agent::tool_dispatch::helpers::split_shell_segments)
    {
        let tag = classify_shell_segment(&segment);
        if tag.severity() > worst.severity() {
            worst = tag;
        }
    }
    if worst == RiskTag::Reads
        && !crate::agent::tool_dispatch::helpers::shell_command_is_observational(command)
    {
        // Our own segment table saw nothing risky, but the dispatcher's
        // stricter read-only classifier does not vouch for it: never label
        // an unrecognised command as a read.
        worst = RiskTag::RunsCommand;
    }
    worst
}

fn classify_shell_segment(segment: &str) -> RiskTag {
    let words: Vec<&str> = segment
        .split_whitespace()
        .skip_while(|w| {
            matches!(*w, "sudo" | "env" | "command" | "exec" | "nohup" | "time")
                || (w.contains('=') && !w.starts_with('-'))
        })
        .collect();
    let Some(first) = words.first() else {
        return RiskTag::Reads;
    };
    let prog = first.rsplit('/').next().unwrap_or(first);
    let sub = words.get(1).copied().unwrap_or("");
    let has = |w: &str| words.contains(&w);

    // Deletion.
    if matches!(prog, "rm" | "rmdir" | "shred" | "unlink")
        || (prog == "find" && (has("-delete") || has("-exec") || has("-execdir")))
        || (prog == "git" && sub == "clean")
    {
        return RiskTag::DeletesFiles;
    }
    // Git history / publication.
    if prog == "git" {
        return match sub {
            "commit" | "push" | "reset" | "rebase" | "merge" | "cherry-pick" | "revert" | "tag"
            | "am" | "stash" | "pull" | "branch" | "filter-branch" | "update-ref" => {
                RiskTag::GitHistory
            }
            "clone" | "fetch" | "ls-remote" | "submodule" => RiskTag::Network,
            "checkout" | "restore" | "switch" | "apply" | "mv" | "rm" | "add" | "init"
            | "config" => RiskTag::WritesWorkspace,
            _ => RiskTag::Reads,
        };
    }
    // Package installation.
    let installs = match prog {
        "pip" | "pip3" | "uv" | "pipx" | "poetry" | "pdm" | "conda" | "mamba" => {
            matches!(sub, "install" | "add" | "sync" | "pip") && !has("list") && !has("freeze")
        }
        "python" | "python3" | "py" => {
            sub == "-m" && words.get(2).is_some_and(|m| *m == "pip") && has("install")
        }
        "npm" | "pnpm" | "yarn" | "bun" => {
            matches!(sub, "install" | "i" | "ci" | "add" | "update" | "upgrade")
        }
        "cargo" => matches!(sub, "install" | "add" | "update" | "binstall"),
        "brew" | "apt" | "apt-get" | "dnf" | "yum" | "apk" | "port" | "snap" | "gem"
        | "composer" => matches!(sub, "install" | "add" | "require" | "upgrade" | "update"),
        "pacman" => sub.starts_with("-s") || sub.starts_with("-u"),
        "go" => matches!(sub, "install" | "get"),
        _ => false,
    };
    if installs {
        return RiskTag::InstallsPackages;
    }
    // Network.
    if matches!(
        prog,
        "curl"
            | "wget"
            | "ssh"
            | "scp"
            | "sftp"
            | "rsync"
            | "nc"
            | "ncat"
            | "telnet"
            | "ftp"
            | "http"
            | "https"
            | "xh"
    ) || (prog == "docker" && matches!(sub, "pull" | "push" | "login"))
    {
        return RiskTag::Network;
    }
    // Workspace writes.
    if crate::agent::tool_dispatch::helpers::has_file_redirect(segment)
        || matches!(
            prog,
            "mv" | "cp"
                | "mkdir"
                | "touch"
                | "tee"
                | "chmod"
                | "chown"
                | "ln"
                | "truncate"
                | "patch"
                | "dd"
        )
        || (prog == "sed" && words.iter().any(|w| w.starts_with("-i")))
        || (prog == "cargo" && sub == "fmt" && !has("--check"))
    {
        return RiskTag::WritesWorkspace;
    }
    // Test runners, builds and interpreters execute project code. The
    // dispatcher counts them as observational (for its loop guards), but a
    // person approving the call must not see "[reads]" for
    // `python3 -m unittest` (UX field test, 0.9.2).
    let runs_project_code = matches!(
        prog,
        "python"
            | "python3"
            | "py"
            | "node"
            | "deno"
            | "ruby"
            | "perl"
            | "php"
            | "pytest"
            | "tox"
            | "nox"
            | "jest"
            | "vitest"
            | "mocha"
            | "make"
            | "just"
            | "gradle"
            | "mvn"
            | "dotnet"
            | "bash"
            | "sh"
            | "zsh"
    ) || (prog == "cargo"
        && matches!(
            sub,
            "test" | "run" | "bench" | "build" | "check" | "clippy" | "nextest"
        ))
        || (matches!(prog, "npm" | "pnpm" | "yarn" | "bun" | "npx")
            && matches!(sub, "test" | "run" | "exec" | "start" | "x" | "dlx"))
        || (prog == "go" && matches!(sub, "test" | "run" | "build" | "generate"));
    if runs_project_code {
        return RiskTag::RunsCommand;
    }
    if crate::agent::tool_dispatch::helpers::shell_command_is_observational(segment) {
        RiskTag::Reads
    } else {
        RiskTag::RunsCommand
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/confirm_view/confirm_view_test.rs"]
mod tests;
