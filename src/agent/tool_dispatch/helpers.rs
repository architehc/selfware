use std::hash::{Hash, Hasher};

use serde_json::Value;
use tracing::warn;

use crate::agent::Agent;

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

pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> String {
    let collected: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{}...", collected)
    } else {
        collected
    }
}

/// Like [`truncate_chars`] but keeps the TAIL: error messages put the
/// actionable part (missing field, line/column, blocked pattern) at the end,
/// so when the retry-suppression preview must be shortened the head is cut,
/// not the tail.
pub(crate) fn truncate_chars_tail(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let tail: String = s.chars().skip(total - max_chars).collect();
    format!("...{}", tail)
}

/// Human-readable failure category for the retry-suppression message. The
/// 4-model harness study found "change X before retrying" without a named
/// failure class left models guessing WHAT to change.
pub(crate) fn failure_category(failure_kind: &str) -> &'static str {
    match failure_kind {
        "validation" => "schema validation",
        "parsing" => "argument parse",
        "safety" => "safety check",
        "task_policy" | "operator_denied" => "policy refusal",
        "progress_guard" => "progress guard",
        _ => "execution error",
    }
}

/// Field names a schema-validation error reports as missing, in either
/// serde's "missing field `x`" form or this crate's validator form
/// "missing required field(s): a, b" (src/tools/mod.rs).
pub(crate) fn missing_fields_in_error(error: &str) -> Vec<String> {
    const SERDE_MARKER: &str = "missing field `";
    const VALIDATOR_MARKER: &str = "missing required field(s): ";
    let mut fields = Vec::new();
    let mut rest = error;
    while let Some(pos) = rest.find(SERDE_MARKER) {
        let after = &rest[pos + SERDE_MARKER.len()..];
        if let Some(end) = after.find('`') {
            fields.push(format!("`{}`", &after[..end]));
        }
        rest = after;
    }
    if let Some(pos) = error.find(VALIDATOR_MARKER) {
        let after = &error[pos + VALIDATOR_MARKER.len()..];
        let list: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | ',' | ' '))
            .collect();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            fields.push(format!("`{}`", name));
        }
    }
    fields
}

/// serde_json's "at line N column M" tail, when present — the position where
/// the argument parser stopped, which is where the model should look first.
pub(crate) fn parse_error_position(error: &str) -> Option<String> {
    let pos = error.rfind(" at line ")?;
    let tail = error[pos + 1..].trim_end_matches(['.', '\n', '\r', ' ']);
    tail.starts_with("line ").then(|| tail.to_string())
}

pub(crate) fn canonicalize_tool_args(args_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_str)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| args_str.to_string())
}

pub(crate) fn hash_tool_args(args_str: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonicalize_tool_args(args_str).hash(&mut hasher);
    hasher.finish()
}

/// Char budget for file content injected into the escalation directive when
/// file_edit keeps failing. The whole file used to be embedded verbatim,
/// bloating the message history without bound on large targets.
pub(crate) const ESCALATION_CONTENT_CHAR_BUDGET: usize = 24_000;

/// Only a confirmed-absent file keeps a failed file_read retry suppressed.
/// Stat errors (permissions, transient I/O) allow the retry so the real
/// error surfaces instead of masquerading as "file does not exist".
pub(crate) fn file_read_retry_stays_suppressed(exists: &std::io::Result<bool>) -> bool {
    match exists {
        Ok(exists) => !exists,
        Err(e) => {
            warn!("file_read retry-suppression: stat failed ({e}); allowing retry to surface the real error");
            false
        }
    }
}

/// Consecutive failed install attempts before the dependency firewall blocks
/// further installs and forces a strategy pivot.
pub(crate) const DEPENDENCY_SPIRAL_LIMIT: usize = 3;

/// Cheap workspace fingerprint for the stagnation detector: fold
/// (path, mtime-secs, size) over a bounded walk. Stat-only — no content
/// reads. Returns None on walk errors (fail-open: caller treats as changed).
pub(crate) fn workspace_fingerprint(root: &std::path::Path) -> Option<u64> {
    const SKIP: &[&str] = &[
        ".git",
        "target",
        "node_modules",
        "dist",
        "build",
        ".venv",
        "__pycache__",
    ];
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut seen = 0usize;
    for entry in walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .filter_entry(|e| {
            !(e.file_type().is_dir()
                && e.depth() > 0
                && SKIP.contains(&e.file_name().to_string_lossy().as_ref()))
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if seen >= 2000 {
            break;
        }
        seen += 1;
        entry.path().to_string_lossy().hash(&mut hasher);
        let meta = entry.metadata().ok()?;
        meta.len().hash(&mut hasher);
        if let Ok(t) = meta.modified() {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .hash(&mut hasher);
        }
    }
    Some(hasher.finish())
}

/// Identical normalized shell commands allowed before the repeated-probe
/// pivot blocks the next one (loop 12). The (LIMIT+1)th — 6th — identical
/// probe is blocked once with a change-strategy directive.
pub(crate) const REPEATED_PROBE_LIMIT: usize = 5;

/// Bound on distinct normalized probe commands tracked per task (loop 12).
/// Past the cap, new commands are simply not tracked (fail-open).
pub(crate) const TRACKED_PROBE_COMMAND_LIMIT: usize = 64;

/// Normalize a shell command for repeated-probe detection (loop 12):
/// lowercase, collapse whitespace runs to a single space, and collapse each
/// digit run to a single `#`. Heredoc probe variants that differ only in
/// embedded numbers or indentation (`python3 - <<'PYEOF' ... print(1)` vs
/// `print(2)`) hash to the same command — the established normalization
/// approach of `normalize_no_action_content`, plus digit collapsing.
pub(crate) fn normalize_probe_command(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut last_was_space = true; // trims leading whitespace
    let mut last_was_digit = false;
    for ch in command.chars().flat_map(|c| c.to_lowercase()) {
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
            last_was_digit = false;
        } else if ch.is_ascii_digit() {
            if !last_was_digit {
                out.push('#');
            }
            last_was_digit = true;
            last_was_space = false;
        } else {
            out.push(ch);
            last_was_space = false;
            last_was_digit = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// True for dependency-installation shell commands (`pip install`, `apt-get
/// install`, `npm install`, `cargo add`, `go get`, …). Token-exact: `pip list`
/// and `apt list --installed` are diagnostics, not installs. The dependency
/// firewall counts consecutive failures of these commands.
pub(crate) fn is_dependency_install_command(command: &str) -> bool {
    let lower = command.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .filter(|t| !t.is_empty())
        .collect();
    let has_installer = tokens.iter().any(|t| {
        matches!(
            *t,
            "pip"
                | "pip3"
                | "pipx"
                | "uv"
                | "npm"
                | "pnpm"
                | "yarn"
                | "apt"
                | "apt-get"
                | "conda"
                | "gem"
                | "cargo"
                | "go"
        )
    });
    let has_verb = tokens
        .iter()
        .any(|t| matches!(*t, "install" | "add" | "get"));
    has_installer && has_verb
}

pub(crate) fn extract_backticked_tool_names(text: &str) -> Vec<String> {
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

pub(crate) fn extract_explicit_allowed_tools(
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

pub(crate) fn extract_explicit_requested_tools<'a, I>(
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

    let disallowed = extract_explicit_disallowed_tools(task_context);
    required.retain(|tool_name| !disallowed.contains(tool_name));

    required
}

pub(crate) fn extract_explicit_disallowed_tools(
    task_context: &str,
) -> std::collections::BTreeSet<String> {
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

            if lower.contains("shell") {
                disallowed.insert("shell_exec".to_string());
                disallowed.insert("pty_shell".to_string());
            }
        }
    }

    disallowed
}

pub(crate) fn mention_is_unnegated(lower: &str, needle: &str) -> bool {
    const NEGATORS: &[&str] = &["not ", "n't ", "never ", "without ", "avoid ", "no "];
    let mut start = 0;
    while let Some(pos) = lower[start..].find(needle) {
        let abs = start + pos;
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

pub fn task_requires_mutation(task_context: &str) -> bool {
    let lower = task_context.to_lowercase();
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
    let has_edit_verb = [
        "fix ",
        "implement ",
        "refactor ",
        "rename ",
        "delete ",
        "modify ",
        "edit the",
    ]
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

pub(crate) fn make_is_mutation_imperative(lower: &str) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmDecision {
    ExecuteOnce,
    AlwaysAllow,
    EnableYolo,
    Skip,
}

pub(crate) fn parse_confirm_response(response: &str) -> ConfirmDecision {
    match response.trim().to_lowercase().as_str() {
        "y" | "yes" => ConfirmDecision::ExecuteOnce,
        "a" | "always" => ConfirmDecision::AlwaysAllow,
        "yolo" => ConfirmDecision::EnableYolo,
        _ => ConfirmDecision::Skip,
    }
}

pub(crate) fn has_file_redirect(command: &str) -> bool {
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
                let mut j = i + 1;
                if chars.get(j) == Some(&'>') {
                    j += 1;
                }
                while chars.get(j) == Some(&' ') {
                    j += 1;
                }
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

pub(crate) fn shell_command_is_observational(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    if has_file_redirect(&normalized) {
        return false;
    }

    // A plain formatter check does not write. Limit this special case to
    // known flags so shell operators, substitutions, and comments cannot hide
    // a mutating suffix or turn a commented-out --check into approval.
    let words: Vec<_> = normalized.split_whitespace().collect();
    if words.starts_with(&["cargo", "fmt"])
        && words[2..].contains(&"--check")
        && words[2..].iter().all(|word| {
            matches!(
                *word,
                "--" | "--check" | "--all" | "--verbose" | "-v" | "--quiet" | "-q"
            )
        })
    {
        return true;
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
        // Never-write inspection/filter utilities (2026-08-29: glm's
        // `diff -q a b` was keyword-classified as mutating and the run was
        // mislabeled REAL_EDIT). Redirection is rejected upstream by
        // has_file_redirect, and none of these have a write mode of their
        // own. Deliberately excluded: `sort` (-o writes), `awk` (program
        // text may redirect internally), `python3 -c` (arbitrary code).
        "diff",
        "comm",
        "jq",
        "cut",
        "uniq",
        "column",
        "file",
        "stat",
        "du",
        "df",
        "date",
        "basename",
        "dirname",
        "readlink",
        "realpath",
        "md5sum",
        "sha256sum",
        "strings",
        "uname",
        "nproc",
        "whoami",
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
    }) || shell_command_runs_test_script(&normalized)
}

pub(crate) fn patch_target_paths(diff: &str) -> Vec<std::path::PathBuf> {
    let mut targets = Vec::new();
    let mut old_path: Option<String> = None;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            // Remember the old-file path; a following `+++ /dev/null` means
            // this file is being deleted.
            let p = rest.split('\t').next().unwrap_or("").trim();
            let p = p.strip_prefix("a/").unwrap_or(p);
            old_path = if p.is_empty() || p == "/dev/null" {
                None
            } else {
                Some(p.to_string())
            };
        } else if line.starts_with("+++ ") {
            if line.starts_with("+++ /dev/null") {
                // File deletion: the old path is the operation's target, so it
                // must be snapshotted for undo/checkpoint accounting just
                // like a written file.
                if let Some(old) = old_path.take() {
                    targets.push(std::path::PathBuf::from(old));
                }
            } else {
                let path = line
                    .strip_prefix("+++ b/")
                    .or_else(|| line.strip_prefix("+++ "))
                    .unwrap_or("");
                let path = path.split('\t').next().unwrap_or("").trim();
                if !path.is_empty() {
                    targets.push(std::path::PathBuf::from(path));
                }
            }
            old_path = None;
        }
    }
    targets
}

/// File paths a mutating file tool touches, mirroring the file-tool branch of
/// `tool_call_is_mutating`. Used by the best-snapshot ledger
/// (`Agent::written_paths`) so snapshot capture and restore cover every file
/// the run wrote — `file_multi_edit` carries a LIST of per-edit paths and
/// `patch_apply` embeds its targets inside the unified diff, so both need
/// structured extraction rather than a single `path` lookup.
pub(crate) fn written_paths_for_tool_call(
    name: &str,
    args: &serde_json::Value,
) -> Vec<std::path::PathBuf> {
    match name {
        "file_edit" | "file_write" | "file_delete" | "file_fim_edit" => args
            .get("path")
            .and_then(|p| p.as_str())
            .map(std::path::PathBuf::from)
            .into_iter()
            .collect(),
        "file_multi_edit" => args
            .get("edits")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
            .map(std::path::PathBuf::from)
            .collect(),
        "patch_apply" => args
            .get("diff")
            .and_then(|v| v.as_str())
            .map(patch_target_paths)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// True when a successful call to `name` writes file CONTENT (create or
/// edit). This is the "has the agent written any file" evidence set for the
/// completion gates: every file-writing tool counts — file_edit, file_write,
/// file_fim_edit, file_multi_edit and patch_apply — while file_delete and
/// shell/git mutations do not (they mutate state but are not "writing a
/// file"). Mirrors the file-tool branch of `tool_call_is_mutating` minus
/// `file_delete`; keep the two in sync.
pub(crate) fn tool_call_writes_file(name: &str) -> bool {
    matches!(
        name,
        "file_edit" | "file_write" | "file_fim_edit" | "file_multi_edit" | "patch_apply"
    )
}

pub(crate) fn tool_call_is_mutating(name: &str, args: &serde_json::Value) -> bool {
    if name == "cargo_fmt" {
        return !args.get("check").and_then(Value::as_bool).unwrap_or(false);
    }
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
    if matches!(name, "shell_exec" | "pty_shell") {
        return args
            .get("command")
            .and_then(|v| v.as_str())
            .map(|cmd| !shell_command_is_observational(cmd))
            .unwrap_or(false);
    }
    false
}

pub(crate) fn shell_command_is_verification(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
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
        // Lean 4 (vero/proof repos): the build IS the proof check.
        "lake build",
        "lake exe",
        "lake test",
        // Coq (TB4 coq-block-bound used `coqc -Q . Top Main.v`).
        "coqc",
    ];

    // A verification prefix only counts when it appears in a pipeline segment
    // whose FIRST shell word (after optional `sudo` / `env` / `VAR=value`
    // prefixes) is the runner for that prefix. Substring matching alone
    // credited `echo cargo test` as a real verification run (AGENTS.md rule
    // 3: honest status over optimistic success).
    //
    // Verification CREDIT requires the runner's exit status to be
    // authoritative for the command's final status (review finding:
    // `cargo test; true`, `cargo test || echo done`, and `true || cargo test`
    // earned credit while failing or never executing). The command may still
    // EXECUTE — this gate only decides what the ledger may trust.
    let segments = shell_segments_with_operators(&normalized);
    segments.iter().enumerate().any(|(i, (_op, segment))| {
        match segment_verification_prefix(segment, &verification_prefixes) {
            Some(prefix) => {
                !segment_is_info_only_invocation(segment, prefix)
                    && runner_status_is_authoritative(&segments, i)
            }
            None => false,
        }
    }) || script_runner_status_is_authoritative(&segments)
}

/// Does the runner segment's exit status determine the command's final
/// status? A preceding `||` can skip the runner outright (`true || cargo
/// test` exits 0 without running the test), and every operator after the
/// runner must be `&&` — the only connector under which overall success
/// implies the runner succeeded. `;` hands the final status to whatever runs
/// last (`cargo test; true`), `|` to the last pipeline stage (`cargo test |
/// true`), and `||` to the recovery command (`cargo test || echo done`).
fn runner_status_is_authoritative(segments: &[(String, String)], runner_idx: usize) -> bool {
    // The connector reaching the runner (stored on the runner's own entry)
    // plus any earlier connector: a `||` anywhere before the runner can skip
    // it entirely (`true || cargo test` exits 0 without running anything).
    if segments[..=runner_idx]
        .iter()
        .any(|(op, _)| op.contains("||"))
    {
        return false;
    }
    segments[runner_idx + 1..]
        .iter()
        .all(|(op, _)| op.trim() == "&&")
}

/// Script-interpreter fallback (`python3 -c 'assert …'`, `node test_x.js`,
/// `./test_x.py`) with the same authority requirement: every connector must
/// be `&&`, so overall success implies every segment — including the test
/// script — actually ran and passed. A masked or skipped assertion is not
/// evidence (review finding: the fallback sat outside the masking check).
/// Detection is scoped per segment with the interpreter in COMMAND position:
/// scanning the whole command credited prose like `echo python3 -c 'assert
/// …'` — the echo prints the text, nothing executes (external review
/// finding).
fn script_runner_status_is_authoritative(segments: &[(String, String)]) -> bool {
    let all_authoritative = segments
        .iter()
        .all(|(op, _)| op.is_empty() || op.trim() == "&&");
    all_authoritative
        && segments
            .iter()
            .any(|(_, segment)| segment_runs_test_script(segment))
}

/// Does this single segment execute a test script, with the interpreter (or
/// a direct `./test_x.py`-style path) as its first shell word?
fn segment_runs_test_script(segment: &str) -> bool {
    let Some(word) = first_shell_word(segment) else {
        return false;
    };
    let basename = word.rsplit('/').next().unwrap_or(word);
    let is_interpreter = basename.starts_with("python")
        || basename.starts_with("pypy")
        || matches!(
            basename,
            "node" | "nodejs" | "deno" | "bun" | "ruby" | "perl" | "php" | "bash" | "sh"
        );
    if is_interpreter {
        return shell_command_runs_test_script(segment);
    }
    (word.starts_with("./") || word.starts_with('/')) && Agent::gate_path_is_test(word)
}

/// Split a shell command into segments at top-level connectors (`&&`, `||`,
/// `;`, `|`, background `&`, newlines), tracking single/double quotes so
/// connectors inside quoted code (`python3 -c "assert add(2, 2) == 4"; true`)
/// don't shred the analysis. An `&` that is part of a redirection (`2>&1`,
/// `>&2`, `&>file`, `&>>file`) is NOT a connector — it stays in the segment
/// text. Returns (operator_before, segment) pairs; the
/// first segment's operator is empty. Parens and subshells are left in the
/// segment text — only the status-connecting operators matter here.
fn shell_segments_with_operators(command: &str) -> Vec<(String, String)> {
    let mut segments: Vec<(String, String)> = Vec::new();
    let mut cur = String::new();
    let mut pending_op = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                in_single = !in_single;
                cur.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                cur.push(c);
            }
            '\\' if in_double || !in_single => {
                cur.push(c);
                if let Some(next) = chars.next() {
                    cur.push(next);
                }
            }
            '&' if !in_single && !in_double => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    segments.push((std::mem::take(&mut pending_op), std::mem::take(&mut cur)));
                    pending_op = "&&".to_string();
                } else if (cur.ends_with('>') && !cur.ends_with("\\>"))
                    || chars.peek() == Some(&'>')
                {
                    // Redirection, not background: `2>&1` / `>&2` duplicate a
                    // descriptor and `&>file` / `&>>file` redirect both
                    // streams — the runner's exit status still governs the
                    // command's final status. Splitting on this `&` made
                    // `cargo test 2>&1` non-authoritative, so a correct test
                    // run never earned verification credit and the completion
                    // gate refused every final answer (review finding #3).
                    // `cmd & > log` (space before `>`) stays background.
                    cur.push(c);
                } else {
                    // Background: the runner detaches; final status belongs to
                    // whatever follows — same authority stance as `;`.
                    segments.push((std::mem::take(&mut pending_op), std::mem::take(&mut cur)));
                    pending_op = ";".to_string();
                }
            }
            '|' if !in_single && !in_double => {
                let op = if chars.peek() == Some(&'|') {
                    chars.next();
                    "||"
                } else {
                    "|"
                };
                segments.push((std::mem::take(&mut pending_op), std::mem::take(&mut cur)));
                pending_op = op.to_string();
            }
            ';' | '\n' if !in_single && !in_double => {
                segments.push((std::mem::take(&mut pending_op), std::mem::take(&mut cur)));
                pending_op = ";".to_string();
            }
            _ => cur.push(c),
        }
    }
    segments.push((pending_op, cur));
    segments
}

/// First shell word of a pipeline segment, skipping leading `sudo`, `env`,
/// and `VAR=value` environment assignments.
pub(crate) fn first_shell_word(segment: &str) -> Option<&str> {
    let mut words = segment.split_whitespace();
    let mut word = words.next()?;
    loop {
        let basename = word.rsplit('/').next().unwrap_or(word);
        if matches!(basename, "sudo" | "env") {
            word = words.next()?;
            continue;
        }
        if let Some((name, _)) = word.split_once('=') {
            if !name.is_empty()
                && !name.starts_with('-')
                && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                word = words.next()?;
                continue;
            }
        }
        return Some(word);
    }
}

/// Does this pipeline segment invoke a recognized verification runner as its
/// first shell word? Returns the matched prefix. The runner set is derived
/// from `verification_prefixes` (first token of each prefix); the existing
/// boundary matching then decides whether the full prefix (e.g. `cargo
/// test`, not `cargo add`) is present. Segments that merely PRINT a runner
/// command (`echo`, `printf`, `true`, `exit`) never count.
fn segment_verification_prefix<'p>(segment: &str, prefixes: &[&'p str]) -> Option<&'p str> {
    let word = first_shell_word(segment)?;
    let basename = word.rsplit('/').next().unwrap_or(word);
    if matches!(basename, "echo" | "printf" | "true" | "exit") {
        return None;
    }
    prefixes.iter().copied().find(|prefix| {
        let runner = prefix.split_whitespace().next().unwrap_or(prefix);
        let runner_basename = runner.rsplit('/').next().unwrap_or(runner);
        basename == runner_basename && command_contains_at_boundary(segment, prefix)
    })
}

/// Info-only runner invocations run no tests: `pytest --version`,
/// `go test -h`, `cargo test --help` (external review finding —
/// `pytest --version` earned verification credit). Everything after the
/// matched runner prefix must be info flags for this to apply; a lone `--`
/// separator is ignored (`npm test -- --version`).
fn segment_is_info_only_invocation(segment: &str, prefix: &str) -> bool {
    const INFO_FLAGS: &[&str] = &[
        "--version",
        "-V",
        "--help",
        "-h",
        "version",
        "help",
        "--collect-only",
        "--list",
        "--markers",
        "--fixtures",
    ];
    let Some(at) = segment.find(prefix) else {
        return false;
    };
    let rest = &segment[at + prefix.len()..];
    let mut saw_info_flag = false;
    for tok in rest.split_whitespace() {
        if tok == "--" {
            continue;
        }
        if INFO_FLAGS.contains(&tok) {
            saw_info_flag = true;
        } else {
            return false;
        }
    }
    saw_info_flag
}

/// True when the command's FIRST shell word (after optional `sudo` / `env` /
/// `VAR=value` prefixes) is a file-content reader: cat/head/tail/grep/less/
/// more/nl/tac/rg/diff/awk, or `sed -n`. Containing a reader token anywhere
/// used to credit `rm notes.txt`-style commands as a readback of the file
/// they destroy — the readback gate must see an actual reader in command
/// position (AGENTS.md rule 3: honest status over optimistic success).
pub(crate) fn shell_command_is_reader(command: &str) -> bool {
    let Some(word) = first_shell_word(command) else {
        return false;
    };
    let basename = word.rsplit('/').next().unwrap_or(word);
    if matches!(
        basename,
        "cat" | "head" | "tail" | "grep" | "less" | "more" | "nl" | "tac" | "rg" | "diff" | "awk"
    ) {
        return true;
    }
    // `sed` only reads-and-prints in its `-n` (quiet, explicit print) form.
    basename == "sed" && command.contains(" -n")
}

pub(crate) fn shell_command_runs_test_script(command: &str) -> bool {
    const SCRIPT_EXTENSIONS: &[&str] = &[
        "py", "pyw", "js", "mjs", "cjs", "ts", "rb", "pl", "pm", "php", "sh",
    ];
    let tokens: Vec<&str> = command
        .split(|c: char| c.is_whitespace() || matches!(c, '&' | ';' | '|' | '(' | ')'))
        .filter(|token| !token.is_empty())
        .collect();

    for (index, token) in tokens.iter().enumerate() {
        let basename = token.rsplit('/').next().unwrap_or(token);
        let is_interpreter = basename.starts_with("python")
            || basename.starts_with("pypy")
            || matches!(
                basename,
                "node" | "nodejs" | "deno" | "bun" | "ruby" | "perl" | "php" | "bash" | "sh"
            );

        if is_interpreter {
            for arg in &tokens[index + 1..] {
                if matches!(*arg, "-c" | "-e" | "--eval") {
                    if command
                        .find(*arg)
                        .map(|at| command[at + arg.len()..].contains("assert"))
                        .unwrap_or(false)
                    {
                        return true;
                    }
                    break;
                }
                if arg.starts_with('-') {
                    continue;
                }
                let looks_like_path = arg.contains('/')
                    || arg
                        .rsplit_once('.')
                        .map(|(_, ext)| SCRIPT_EXTENSIONS.contains(&ext))
                        .unwrap_or(false);
                if looks_like_path && Agent::gate_path_is_test(arg) {
                    return true;
                }
                break;
            }
        } else if (token.starts_with("./") || token.starts_with('/'))
            && Agent::gate_path_is_test(token)
        {
            return true;
        }
    }
    false
}

pub(crate) fn command_contains_at_boundary(command: &str, prefix: &str) -> bool {
    let bytes = command.as_bytes();
    let mut from = 0;
    while let Some(rel) = command[from..].find(prefix) {
        let abs = from + rel;
        let before_ok = abs == 0
            || matches!(
                bytes[abs - 1],
                b' ' | b'/' | b'&' | b';' | b'|' | b'\t' | b'('
            );
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

pub(crate) fn command_is_noop_verification(text: &str) -> bool {
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

pub(crate) fn tool_call_is_verification(name: &str, args_str: &str) -> bool {
    match name {
        "cargo_check" | "cargo_test" | "cargo_clippy" => !command_is_noop_verification(args_str),
        "shell_exec" | "pty_shell" => serde_json::from_str::<Value>(args_str)
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

pub(crate) fn tool_call_is_observational(name: &str, args_str: &str) -> bool {
    match name {
        "cargo_fmt" => serde_json::from_str::<Value>(args_str)
            .ok()
            .and_then(|args| args.get("check").and_then(Value::as_bool))
            .unwrap_or(false),
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

pub(crate) fn tool_call_counts_as_state_change(name: &str, args_str: &str) -> bool {
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

pub(crate) fn read_tool_target(name: &str, args_str: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_str).ok()?;
    match name {
        "file_read" | "file_write" | "file_edit" | "file_delete" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        "directory_tree" => args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                args.get("pattern")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("tree:{}", s))
            }),
        "glob_find" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| format!("glob:{}", s)),
        "grep_search" => args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| format!("grep:{}", s)),
        "symbol_search" => args
            .get("query")
            .or_else(|| args.get("pattern"))
            .and_then(|v| v.as_str())
            .map(|s| format!("sym:{}", s)),
        _ => None,
    }
}

pub(crate) fn configured_vision_profile(
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

pub(crate) fn insert_missing_tool_arg(
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

pub(crate) fn inject_runtime_tool_defaults(
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
