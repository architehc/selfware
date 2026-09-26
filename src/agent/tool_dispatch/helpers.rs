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

fn is_question_without_edit_imperative(lower: &str) -> bool {
    let trimmed = lower.trim();
    let is_q = trimmed.ends_with('?')
        || [
            "how to ",
            "how do ",
            "how does ",
            "how can ",
            "where is ",
            "where are ",
            "can you ",
            "could you ",
            "what is ",
            "what are ",
            "why is ",
            "why does ",
            "is there ",
            "are there ",
            "which ",
        ]
        .iter()
        .any(|p| trimmed.starts_with(p));

    if !is_q {
        return false;
    }

    let edit_imperatives = [
        "fix ",
        "implement ",
        "refactor ",
        "rename ",
        "modify ",
        "delete ",
        "remove ",
        "edit ",
        "write ",
        "create ",
        "update ",
        "add ",
    ];

    if edit_imperatives
        .iter()
        .any(|verb| trimmed.starts_with(verb))
    {
        return false;
    }

    let mut after_polite = trimmed;
    if let Some(rest) = after_polite.strip_prefix("can you ") {
        after_polite = rest.trim_start();
    } else if let Some(rest) = after_polite.strip_prefix("could you ") {
        after_polite = rest.trim_start();
    }
    if let Some(rest) = after_polite.strip_prefix("please ") {
        after_polite = rest.trim_start();
    }

    if edit_imperatives
        .iter()
        .any(|verb| after_polite.starts_with(verb))
    {
        return false;
    }

    true
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
    if is_question_without_edit_imperative(&lower) {
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
                if chars.get(j) == Some(&'&') {
                    i = j;
                    continue;
                }
                // Check if redirecting to /dev/null (e.g. 2>/dev/null, >/dev/null)
                let rem: String = chars[j..].iter().collect();
                if let Some(after) = rem.strip_prefix("/dev/null") {
                    if after.is_empty()
                        || after.starts_with(' ')
                        || after.starts_with(';')
                        || after.starts_with('&')
                        || after.starts_with('|')
                    {
                        i = j + "/dev/null".len();
                        continue;
                    }
                }
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Split a command on shell operators, ignoring operators inside quotes.
///
/// Quote-awareness is not a nicety: `jq '.nodes | length' file` contains a pipe
/// that is part of an argument, and treating it as a separator turns one
/// read-only command into two unrecognisable fragments.
///
/// Shared with Phi's observer so the two cannot disagree about where one
/// command ends and the next begins — they did, and the observer's naive
/// `split('|')` reintroduced the very bug this function was written to fix.
pub(crate) fn split_shell_segments(command: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if escaped {
            current.push(c);
            escaped = false;
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                current.push(c);
                escaped = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                current.push(c);
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
            }
            // `&` is three different things, and an earlier revision treated
            // only the first as a separator:
            //   `&&`   — connector, separates
            //   `2>&1` — descriptor duplication, does NOT separate
            //   `cmd &`— background job, DOES separate
            // Refusing to split on the third let a read-only prefix vouch for
            // whatever followed it: `ls & python3 fix.py` stayed one segment,
            // matched the `ls` prefix, and an arbitrary script ran without
            // advancing the mutation sequence.
            '&' if !in_single && !in_double && chars.get(i + 1) == Some(&'&') => {
                i += 1;
                segments.push(std::mem::take(&mut current));
            }
            '&' if !in_single
                && !in_double
                && !current.trim_end().ends_with('>')
                && !current.trim_end().ends_with('<')
                && chars.get(i + 1) != Some(&'>') =>
            {
                // Backgrounding. The job still runs; it just runs detached.
                segments.push(std::mem::take(&mut current));
            }
            '|' if !in_single && !in_double => {
                if chars.get(i + 1) == Some(&'|') {
                    i += 1;
                }
                segments.push(std::mem::take(&mut current));
            }
            ';' if !in_single && !in_double => {
                segments.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
        i += 1;
    }
    segments.push(current);
    segments
        .into_iter()
        .map(|segment| segment.trim().to_string())
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(crate) fn shell_command_is_observational(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }

    if has_file_redirect(&normalized) {
        return false;
    }

    // A compound command is observational only if EVERY segment is.
    //
    // `shell_command_runs_test_script` scans all tokens, so a trailing
    // `python3 -m unittest` vouched for a leading `python3 fix.py` and the
    // whole thing read as read-only — an arbitrary script could run without
    // advancing the mutation sequence, and a later edit would not invalidate
    // the verification that followed it. One segment cannot speak for another.
    let segments = split_shell_segments(&normalized);
    if segments.len() > 1 {
        return segments
            .iter()
            .all(|segment| shell_command_is_observational(segment));
    }

    // Classification keys off the program actually run: leading `VAR=value`
    // assignments, `sudo`/`env` wrappers, and any path prefix on the runner
    // are noise. Without this, `CARGO_TERM_COLOR=never cargo test 2>&1 |
    // grep …` and `cd crate && cargo test | tail` read as unknown commands
    // and counted as MUTATIONS — the 2026-09-22 long-task e2e burned
    // iterations re-verifying state its own test runs had "dirtied".
    // Mutating-marker scanning below still sees the raw text, so a wrapper
    // cannot launder a write.
    let run_command = strip_leading_wrappers(&normalized);

    // A plain formatter check does not write. Limit this special case to
    // known flags so shell operators, substitutions, and comments cannot hide
    // a mutating suffix or turn a commented-out --check into approval.
    let words: Vec<_> = run_command.split_whitespace().collect();
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
        // `find` is a read-only prefix below, but these options make it write.
        // Phi's observer already knew this; the dispatcher did not, so the two
        // disagreed about the same command.
        " -delete",
        " -exec",
        " -execdir",
        " -ok",
        " -fprint",
        " -fls",
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
        "wmctrl -r",
        "wmctrl -c",
        "wmctrl -s",
        "wmctrl -k",
        "wmctrl -a",
        "wmctrl -e",
    ];
    if mutating_markers
        .iter()
        .any(|marker| normalized.contains(marker))
    {
        return false;
    }

    // Autofix / write-in-place flags turn a read-only checker into a writer:
    // `cargo clippy --fix` matched the `cargo clippy` read-only prefix below
    // and never advanced the mutation sequence (same for `eslint --fix`,
    // `ruff check --fix`, `prettier --write`). Word-exact so
    // `grep --fixed-strings` stays read-only.
    if words.iter().any(|word| {
        matches!(*word, "--fix" | "--fix-only" | "--write" | "--in-place")
            || word.starts_with("--fix=")
    }) {
        return false;
    }

    // `tsc --noEmit` type-checks without writing. Plain `tsc` emits JS, so
    // only the no-emit form is read-only, and never together with the flags
    // that still write build info (`--build`/`-b`, `--incremental`,
    // `--tsBuildInfoFile`, `--generateTrace`). Counting it as a mutation
    // bumped the mutation sequence on every type-check (2026-09-25 audit:
    // `tsc --noEmit` at steps 6, 8 and 17 of one run).
    let tsc_args = match words.as_slice() {
        ["tsc", rest @ ..] | ["npx", "tsc", rest @ ..] => Some(rest),
        _ => None,
    };
    if let Some(rest) = tsc_args {
        let writes_build_info = rest.iter().any(|word| {
            matches!(*word, "-b" | "--build" | "--incremental")
                || word.starts_with("--tsbuildinfofile")
                || word.starts_with("--generatetrace")
        });
        if rest.contains(&"--noemit") && !writes_build_info {
            return true;
        }
    }

    if words.first().copied() == Some("wmctrl") {
        let has_mutating = words[1..].iter().any(|word| {
            word.starts_with("-r")
                || word.starts_with("-c")
                || word.starts_with("-s")
                || word.starts_with("-k")
                || word.starts_with("-a")
                || word.starts_with("-e")
                || word.starts_with("-b")
                || word.starts_with("-o")
                || word.starts_with("-n")
                || word.starts_with("-R")
        });
        let has_query = words[1..]
            .iter()
            .any(|word| word.starts_with("-l") || *word == "-d" || *word == "-m");
        return has_query && !has_mutating;
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
        // Phi's observer listed these as read-only and the dispatcher did not.
        // They are: none of them write to the working tree.
        "git show",
        "git branch",
        "git ls-files",
        "git rev-parse",
        "less",
        "more",
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
        "python3 -m pytest",
        "python -m unittest",
        "python3 -m unittest",
        "python -m test",
        "python3 -m test",
        "python -m py_compile",
        "python3 -m py_compile",
        "node --test",
        "npm test",
        "pnpm test",
        "yarn test",
        "bun test",
        "deno test",
        "npx jest",
        "npx mocha",
        "npx vitest",
        "npx ava",
        "go test",
        "mvn test",
        "mvn verify",
        "gradle test",
        "./gradlew test",
        "dotnet test",
        "ctest",
        "make test",
        "swift test",
        "lake test",
        "which",
        "echo",
        "env",
        "printenv",
        // `cd dir && cargo test 2>&1 | tail` is the standard monorepo
        // verification shape; `cd` itself writes nothing. Any mutating
        // segment after it still disqualifies the whole chain.
        "cd",
        "xdotool getactivewindow",
        "xdotool search",
        "xdotool getwindow",
        "ps",
        "top",
        "uptime",
    ];

    read_only_prefixes.iter().any(|prefix| {
        run_command == *prefix
            || (run_command.starts_with(prefix)
                && (run_command[prefix.len()..].starts_with(' ')
                    || run_command[prefix.len()..].starts_with("--")))
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

/// `cargo_clippy` with `fix: true` runs `cargo clippy --fix`, which rewrites
/// source files it does not name.
pub(crate) fn cargo_clippy_applies_fixes(name: &str, args: &serde_json::Value) -> bool {
    name == "cargo_clippy" && args.get("fix").and_then(Value::as_bool).unwrap_or(false)
}

/// Registered tools that rewrite workspace files WITHOUT naming them:
/// `cargo_fmt` (unless `check`), `cargo_clippy` with `fix`, and the package
/// tools, which rewrite lockfiles/manifests and dependency trees
/// (`npm_install`, `yarn_install`, `pip_install`) or run arbitrary
/// package.json scripts (`npm_run` — `lint --fix`, `format`, `build`; the
/// `test` script is treated as read-only, like shell `npm test`).
/// Their written paths are unknown, so every consumer must treat them as
/// "anything may have changed" (full cache invalidation, no path list).
pub(crate) fn tool_call_is_opaque_mutation(name: &str, args: &serde_json::Value) -> bool {
    match name {
        "cargo_fmt" => !args.get("check").and_then(Value::as_bool).unwrap_or(false),
        "cargo_clippy" => cargo_clippy_applies_fixes(name, args),
        "npm_install" | "yarn_install" | "pip_install" => true,
        // Parity with the shell classifier, where `npm test` is read-only
        // and `npm run <anything else>` is not.
        "npm_run" => args.get("script").and_then(Value::as_str) != Some("test"),
        _ => false,
    }
}

pub(crate) fn tool_call_is_mutating(name: &str, args: &serde_json::Value) -> bool {
    if matches!(
        name,
        "cargo_fmt" | "cargo_clippy" | "npm_install" | "yarn_install" | "pip_install" | "npm_run"
    ) {
        return tool_call_is_opaque_mutation(name, args);
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

/// What kind of verification a command performs.
///
/// `shell_command_is_verification` is deliberately broad — it answers "is this
/// a checking command", which includes compilers and linters. Consumers that
/// care whether TESTS RAN must not reuse that boolean: `cargo check`,
/// `npx tsc`, `go build` and `sqlfluff lint` all return true and execute no
/// tests. The evidence ledger discharged test obligations on them until this
/// split existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationKind {
    /// Executes tests, or runs a deliverable script WITH an explicit expected
    /// result (exit-code assertion and/or expected-output check) — assertions
    /// demonstrably ran.
    TestExecution,
    /// Compiles, type-checks or lints. Proves the code builds, not that it works.
    CompileOrLint,
    /// A deliverable script ran and exited 0 with NO explicit expected result
    /// checked — no `$?`/`rc=` exit-code assertion, no grep/diff/`test` on its
    /// output (P1 review finding: a successful launch got TestExecution
    /// credit without anything being asserted). Lesser credit than
    /// TestExecution: the process started cleanly, nothing about its output
    /// was verified.
    SmokeRun,
}

/// Commands that check without executing tests. Subset of the prefixes below,
/// kept as one list so the two cannot drift.
const COMPILE_OR_LINT_PREFIXES: &[&str] = &[
    "cargo check",
    "cargo clippy",
    "python -m py_compile",
    "python3 -m py_compile",
    "node --check",
    "bash -n",
    "sh -n",
    "ruff check",
    "ruff lint",
    "mypy",
    "npx tsc",
    "tsc ",
    "go build",
    "javac",
    "dotnet build",
    "cmake --build",
    "swift build",
    "sqlfluff lint",
    "lake build",
];

/// Every verification-runner prefix (superset of [`COMPILE_OR_LINT_PREFIXES`];
/// the two lists are documented to drift only via that comment). Hoisted so
/// both the recognition gate and the per-segment kind classifier consult the
/// SAME runner set.
const VERIFICATION_PREFIXES: &[&str] = &[
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
    "python -m test",
    "python3 -m test",
    "node --test",
    "npm test",
    "pnpm test",
    "yarn test",
    "bun test",
    "deno test",
    "npx jest",
    "npx mocha",
    "npx vitest",
    "npx ava",
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
    // Static syntax/type checks for script languages, credited the same
    // way `python -m py_compile` already was (review finding: bare
    // `node --check` / `bash -n` runs on deliverable scripts should count).
    "node --check",
    "bash -n",
    "sh -n",
    "ruff check",
    "ruff lint",
    "mypy",
];

/// The command with leading wrapper prefixes removed, so classification keys
/// off the program actually run. Strips leading `NAME=value` environment
/// assignments, leading `sudo` / `env` words, and any path prefix on the
/// first remaining word: `CARGO_TERM_COLOR=never cargo check`, `sudo cargo
/// check` and `/usr/bin/cargo check` all normalize to `cargo check`.
///
/// The kind classifier used to match compile prefixes against the raw
/// segment with `starts_with`; nothing survived a wrapper, so an
/// env-prefixed `cargo check` fell through to "a verification prefix in
/// command position" and earned TestExecution credit — compilation
/// discharging test obligations (review finding #1).
fn strip_leading_wrappers(segment: &str) -> &str {
    let mut rest = segment.trim_start();
    // `sudo` / `env` words and `NAME=value` assignments, in any order:
    // `env FOO=1 cargo check` and `FOO=1 env cargo check` are the same
    // command. An assignment's NAME must be a bare identifier (`find = x`
    // and `echo foo=bar` are commands, not assignments).
    loop {
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let word = &rest[..word_end];
        let basename = word.rsplit('/').next().unwrap_or(word);
        let is_assignment = word
            .split_once('=')
            .map(|(name, _)| {
                !name.is_empty()
                    && !name.contains(char::is_whitespace)
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !name.starts_with(|c: char| c.is_ascii_digit())
            })
            .unwrap_or(false);
        if matches!(basename, "sudo" | "env") || is_assignment {
            rest = rest[word_end..].trim_start();
            continue;
        }
        break;
    }
    // A path prefix on the first word: `/usr/bin/cargo check` and
    // `~/.cargo/bin/cargo check` key off `cargo`, not the full path.
    let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let word = &rest[..word_end];
    if let Some((_, prog)) = word.rsplit_once('/') {
        if !prog.is_empty() {
            rest = &rest[word_end - prog.len()..];
        }
    }
    rest
}

/// Classify a verification command by kind. `None` when it is not verification
/// at all.
pub(crate) fn shell_command_verification_kind(command: &str) -> Option<VerificationKind> {
    if !shell_command_is_verification(command) {
        return None;
    }
    let normalized = command.trim().to_lowercase();
    // A compound command is judged by its LAST runner-prefix verification
    // segment: in `cargo check && cargo test` the tests did run. Segments
    // that only qualify through the script-interpreter / deliverable-script
    // FALLBACKS are not classified per segment — their tier is decided as a
    // whole command below (a `bash deploy.sh` smoke segment must never flip
    // a compound command to TestExecution on its own).
    let mut kind = VerificationKind::CompileOrLint;
    let mut saw_any = false;
    let segments: Vec<&str> = normalized
        .split("&&")
        .flat_map(|s| s.split("||"))
        .flat_map(|s| s.split(';'))
        .flat_map(|s| s.split('|'))
        .collect();
    for segment in segments {
        let segment = segment.trim();
        if segment.is_empty() || !shell_command_is_verification(segment) {
            continue;
        }
        // Match against the program actually run, wrappers stripped:
        // `CARGO_TERM_COLOR=never cargo check` and `/usr/bin/cargo check`
        // classify exactly like `cargo check`. starts_with on the raw
        // segment let env-wrapped compiles fall through to the runner branch
        // below and earn TestExecution credit (review finding #1).
        let run_command = strip_leading_wrappers(segment);
        if COMPILE_OR_LINT_PREFIXES
            .iter()
            .any(|prefix| run_command.starts_with(prefix))
        {
            saw_any = true;
            kind = VerificationKind::CompileOrLint;
        } else if segment_verification_prefix(segment, VERIFICATION_PREFIXES).is_some() {
            // A genuine test-runner prefix in command position.
            return Some(VerificationKind::TestExecution);
        }
        // Fallback-only segments (test scripts, inline asserts, deliverable
        // smoke runs) are handled by the whole-command classifier below.
    }
    if saw_any {
        Some(kind)
    } else if COMPILE_OR_LINT_PREFIXES
        .iter()
        .any(|prefix| strip_leading_wrappers(&normalized).starts_with(prefix))
    {
        Some(VerificationKind::CompileOrLint)
    } else {
        // No runner prefix matched; the command qualified through the
        // script-interpreter / deliverable-script fallbacks. Tests and
        // explicit-result deliverable checks are TestExecution; a bare
        // exit-0 deliverable run is only a SmokeRun (P1 review finding).
        let op_segments = shell_segments_with_operators(&normalized);
        if script_runner_status_is_authoritative(&op_segments)
            || deliverable_check_status_is_authoritative(&op_segments)
        {
            Some(VerificationKind::TestExecution)
        } else {
            Some(VerificationKind::SmokeRun)
        }
    }
}

pub(crate) fn shell_command_is_verification(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if command_is_noop_verification(&normalized) {
        return false;
    }

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
        match segment_verification_prefix(segment, VERIFICATION_PREFIXES) {
            Some(prefix) => {
                !segment_is_info_only_invocation(segment, prefix)
                    && runner_status_is_authoritative(&segments, i)
            }
            None => false,
        }
    }) || script_runner_status_is_authoritative(&segments)
        || deliverable_check_status_is_authoritative(&segments)
        || deliverable_smoke_status_is_authoritative(&segments)
}

/// A recognized verification runner IS invoked in command position, but the
/// command's final exit status is not the runner's: a pipeline or a `;`/`||`
/// chain masks it (`cargo test 2>&1 | grep 'test result'`, `cargo test |
/// true`, `cargo test; true`). Such a run earns no exit-status credit — but
/// its captured output may still carry unambiguous evidence, which
/// [`runner_output_proves_success`] / [`runner_output_proves_failure`] read.
///
/// Returns false for commands whose runner status IS authoritative (those
/// take the ordinary credit path), for info-only invocations (`cargo test
/// --help`), and for no-op runs (`--no-run`, `--collect-only`, …).
pub(crate) fn shell_command_is_masked_verification(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() || command_is_noop_verification(&normalized) {
        return false;
    }
    if shell_command_is_verification(&normalized) {
        return false;
    }
    let segments = shell_segments_with_operators(&normalized);
    segments.iter().any(|(_op, segment)| {
        match segment_verification_prefix(segment, VERIFICATION_PREFIXES) {
            Some(prefix) => !segment_is_info_only_invocation(segment, prefix),
            None => false,
        }
    })
}

/// Is this segment a (non-info-only) verification runner whose OUTPUT may be
/// read as evidence — a runner prefix in command position, or a test script?
fn segment_is_output_runner(segment: &str) -> bool {
    match segment_verification_prefix(segment, VERIFICATION_PREFIXES) {
        Some(prefix) => !segment_is_info_only_invocation(segment, prefix),
        None => segment_runs_test_script(segment),
    }
}

/// Does this segment divert stdout or stderr away from the tool result?
/// Every unquoted `>` counts (`> o`, `>> o`, `1> o`, `2> o`, `&> o`, `>&-`,
/// `2>/dev/null`) EXCEPT a pure descriptor duplication onto the other
/// captured stream (`2>&1`, `1>&2`, `>&2`), which keeps every line in the
/// captured output. Fail-closed: `2>/dev/null` hides compile errors and
/// panics, so it diverts too.
fn segment_diverts_output(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\\' if !in_single => {
                i += 2;
                continue;
            }
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'>' if !in_single && !in_double => {
                let dup_to_captured = bytes.get(i + 1) == Some(&b'&')
                    && matches!(bytes.get(i + 2), Some(b'1') | Some(b'2'))
                    && !bytes.get(i + 3).is_some_and(|c| c.is_ascii_digit());
                if !dup_to_captured {
                    return true;
                }
                i += 3;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// A pipeline stage downstream of the runner that passes every line through
/// to the tool result unmodified: `tee [-a|-i|-p] FILE…` (copies to files,
/// stdout still flows) or bare `cat` / `cat -`. `cat FILE` would print the
/// file instead of the runner's stream, so it is not transparent. Any output
/// diversion on the stage itself (`tee o > /dev/null`) disqualifies it.
fn segment_is_transparent_stage(segment: &str) -> bool {
    if segment_diverts_output(segment) {
        return false;
    }
    let words: Vec<&str> = segment.split_whitespace().collect();
    match words.first().copied() {
        Some("tee") => words[1..]
            .iter()
            .all(|w| !w.starts_with('-') || matches!(*w, "-a" | "--append" | "-i" | "-p")),
        Some("cat") => words[1..].iter().all(|w| matches!(*w, "-" | "-u")),
        _ => false,
    }
}

/// A non-runner segment that cannot put runner-shaped text into the tool
/// result: directory changes, environment builtins, `true`/`false`/`:`,
/// `exit`, `sleep`, and a literal `echo` carrying no outcome vocabulary and
/// no expansion/escape (`echo done`). Everything else — including `printf`,
/// `echo "test result: ok"`, a second runner, `grep … o` on a log file —
/// could add or fabricate success-shaped lines.
fn segment_is_output_neutral(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return true;
    }
    let word = first_shell_word(trimmed).unwrap_or("");
    match word {
        "cd" | "pushd" | "popd" | "true" | "false" | ":" | "exit" | "export" | "set" | "unset"
        | "sleep" => true,
        "echo" => {
            let lower = trimmed.to_lowercase();
            !lower.contains('$')
                && !lower.contains('`')
                && !lower.contains('\\')
                && !["ok", "pass", "fail", "result", "error", "success"]
                    .iter()
                    .any(|w| lower.contains(w))
        }
        _ => false,
    }
}

/// Does the verification runner's output reach the tool result UNFILTERED —
/// the only condition under which captured output may substitute for a
/// masked exit status as SUCCESS evidence?
///
/// Requirements (fail-closed, AGENTS.md rule 3):
/// - exactly one runner segment (two runners' outputs mix: one suite's
///   `test result: ok` can sit beside another's unlabelled error);
/// - the runner segment diverts neither stream (`cargo test > o`,
///   `cargo test > o 2>&1`, `pytest 2>/dev/null` all fail);
/// - every downstream pipeline stage is transparent (`tee FILE`, bare
///   `cat`) — `grep` of any kind (`grep 'test result'`, `grep -m1`,
///   `grep -E 'test result: o.'`), `head`, `tail`, `sort`, `awk`, … can drop
///   the failing lines while keeping a passing one;
/// - every other segment is output-neutral, so a later `grep 'test result:
///   ok' o` / `cat o` / `echo '3 passed'` cannot contribute success-shaped
///   text that the runner never printed to the result.
pub(crate) fn runner_output_reaches_result_unfiltered(command: &str) -> bool {
    let normalized = command.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let segments = shell_segments_with_operators(&normalized);
    let runners: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter(|(_, (_, segment))| segment_is_output_runner(segment))
        .map(|(i, _)| i)
        .collect();
    let [runner_idx] = runners.as_slice() else {
        return false;
    };
    let runner_idx = *runner_idx;
    if segment_diverts_output(&segments[runner_idx].1) {
        return false;
    }
    let mut j = runner_idx + 1;
    while let Some((op, stage)) = segments.get(j) {
        if op.trim() != "|" {
            break;
        }
        if !segment_is_transparent_stage(stage.trim()) {
            return false;
        }
        j += 1;
    }
    let runner_pipeline = runner_idx..j;
    // Every other segment — including an upstream stage piped INTO the
    // runner, whose stderr still reaches the result — must be neutral.
    segments
        .iter()
        .enumerate()
        .all(|(i, (_, segment))| runner_pipeline.contains(&i) || segment_is_output_neutral(segment))
}

/// Did the tool deliver the command's COMPLETE output? A paginated page with
/// more to come (`has_more`), a page starting past offset 0, a pty
/// `[output truncated …]` marker, or a result that is not a well-formed JSON
/// object (e.g. the checkpoint log's 1000-char head truncation) all mean the
/// failing tail of a multi-suite run may be missing — so an early
/// `test result: ok` in the visible head proves nothing.
pub(crate) fn tool_result_output_is_complete(result: &str) -> bool {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(result)
    else {
        return false;
    };
    map.iter().all(|(key, value)| {
        if key.ends_with("pagination") {
            let has_more = value
                .get("has_more")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let offset = value
                .get("offset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            return !has_more && offset == 0;
        }
        match value.as_str() {
            Some(text) => !text.contains("[output truncated"),
            None => true,
        }
    })
}

/// The single success-credit rule for a MASKED verification run (exit status
/// not authoritative): the runner's output reached the tool result
/// unfiltered and complete, and it carries an unambiguous success marker with
/// no failure marker. Used by the dispatcher's credit path and by both
/// checkpoint-scan paths so they cannot drift apart (AGENTS.md rule 5).
pub(crate) fn masked_run_output_proves_success(command: &str, result: &str) -> bool {
    runner_output_reaches_result_unfiltered(command)
        && tool_result_output_is_complete(result)
        && runner_output_proves_success(result)
}

/// The `N failed` count directly preceding a "failed" summary word, when the
/// word is part of a `<digits> failed` tally. `"10 failed"` parses as 10
/// (the digit walk is unbounded), so the `"0 failed"` substring trap cannot
/// read a real failure as a zero-failure summary. Used for every
/// `<digits> <word>` tally (`passed`, `passing`, `failing`, `error`).
fn failed_count_before(lower: &str, abs: usize) -> Option<u64> {
    let bytes = lower.as_bytes();
    if abs == 0 || bytes[abs - 1] != b' ' {
        return None;
    }
    let mut start = abs - 1;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start == abs - 1 {
        return None;
    }
    lower[start..abs - 1].parse().ok()
}

/// Every `<digits> <word>` tally in `lower` (e.g. all `N passed` counts).
fn tallies_before(lower: &str, word: &str) -> Vec<u64> {
    let mut counts = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find(word) {
        let abs = from + rel;
        if let Some(n) = failed_count_before(lower, abs) {
            counts.push(n);
        }
        from = abs + 1;
    }
    counts
}

/// The runner's captured output inside a tool result: the `stdout`,
/// `stderr` and `output` strings of a JSON-object result, joined by
/// newlines. Line-anchored markers (unittest's lone `OK`, go's `ok  \tpkg`)
/// need real lines — a JSON-encoded result carries them as `\n` escapes —
/// and unittest prints its summary on STDERR, so both streams are read. A
/// result that is not such an object is read as raw text.
fn captured_runner_output(result: &str) -> String {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(result) {
        let streams: Vec<&str> = ["stdout", "stderr", "output"]
            .iter()
            .filter_map(|key| map.get(*key).and_then(Value::as_str))
            .collect();
        if !streams.is_empty() {
            return streams.join("\n");
        }
    }
    result.to_string()
}

/// The `N` in a Python unittest `Ran N test(s) in …` summary line.
fn unittest_ran_count(line: &str) -> Option<u64> {
    let rest = line.strip_prefix("Ran ")?;
    let (count, tail) = rest.split_once(' ')?;
    if !(tail.starts_with("test in ") || tail.starts_with("tests in ")) {
        return None;
    }
    count.parse().ok()
}

/// A `go test` package-pass line: `ok  \t<pkg>\t<duration>` (or
/// `(cached)`). The literal `ok` must be followed by whitespace and the line
/// must carry go's tab-separated package column, so a bare `ok` or prose
/// (`ok fine`) never matches.
fn is_go_test_ok_line(line: &str) -> bool {
    line.strip_prefix("ok")
        .is_some_and(|rest| rest.starts_with([' ', '\t']) && rest.contains('\t'))
        && line
            .split('\t')
            .nth(1)
            .is_some_and(|pkg| !pkg.trim().is_empty())
}

/// A `node --test` summary count: TAP `# pass 5` / `# fail 0`, or the spec
/// reporter's `ℹ pass 5` / `ℹ fail 0`.
fn tap_count(line: &str, word: &str) -> Option<u64> {
    line.strip_prefix("# ")
        .or_else(|| line.strip_prefix("\u{2139} "))?
        .strip_prefix(word)?
        .strip_prefix(' ')?
        .trim()
        .parse()
        .ok()
}

/// Line-anchored runner failure markers, matched on the runner's ORIGINAL
/// casing (the runners print them in upper case; prose rarely does):
///
/// - unittest: `FAILED (failures=…)` / `FAILED (errors=…)` summary and the
///   per-test `FAIL: test_x` / `ERROR: test_x` headers;
/// - go test: `--- FAIL: TestX`, `FAIL\t<pkg>`, a lone `FAIL`;
/// - jest/vitest: `FAIL src/x.test.js` file headers;
/// - node TAP: a nonzero `# fail N`.
fn line_proves_runner_failure(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("FAILED (")
        || trimmed.starts_with("FAIL: ")
        || trimmed.starts_with("ERROR: test")
        || trimmed.starts_with("--- FAIL")
        || trimmed == "FAIL"
        || trimmed.starts_with("FAIL\t")
        || trimmed.starts_with("FAIL ")
        || tap_count(trimmed, "fail").is_some_and(|n| n > 0)
}

/// Unambiguous runner-FAILURE evidence in captured output: a failing libtest
/// summary, a rustc diagnostic, a cargo compile failure, a panic, a nonzero
/// `N failed` / `N failing` tally (libtest, pytest, jest, vitest, mocha), or a line-anchored unittest / go test / jest / TAP
/// failure marker ([`line_proves_runner_failure`]). Each marker is specific
/// enough that a passing run never prints it, so recording the failure is
/// safe.
pub(crate) fn runner_output_proves_failure(output: &str) -> bool {
    let text = captured_runner_output(output);
    let lower = text.to_lowercase();
    if lower.contains("test result: failed")
        || lower.contains("failures:")
        || lower.contains("error[e")
        || lower.contains("error: aborting")
        || lower.contains("error: could not compile")
        || lower.contains("panicked at")
    {
        return true;
    }
    if ["failed", "failing"]
        .iter()
        .any(|word| tallies_before(&lower, word).iter().any(|&n| n > 0))
    {
        return true;
    }
    text.lines().any(line_proves_runner_failure)
}

/// Unambiguous runner-SUCCESS evidence in captured output, for runs whose
/// exit status was masked by a pipeline or connector (see
/// [`shell_command_is_masked_verification`]). Fail-closed (AGENTS.md rule 3):
/// only these success markers count —
///
/// - `N passed`, N > 0       (libtest / pytest / jest / vitest summaries;
///   `test result: ok` and `0 failed` alone are not credit — a libtest
///   binary that ran nothing prints both),
/// - `N passing`, N > 0      (mocha),
/// - `Ran N tests in …` (N > 0) PLUS a line that is exactly `OK` or starts
///   `OK (` (`OK (skipped=1)`) — Python unittest; `Ran` alone is not a
///   verdict and a lone `OK` is not a unittest summary,
/// - an `ok  \t<pkg>` line   (go test),
/// - `# pass N` (N > 0) PLUS `# fail 0` (node --test TAP summary),
///
/// and ANY failure-shaped content vetoes the credit — every
/// [`runner_output_proves_failure`] marker, a bare `failed` word (grep
/// matching `test result` prints the FAILED line too), a nonzero error
/// tally, and a Python traceback — so output bearing both a success marker
/// and a failure marker earns nothing.
pub(crate) fn runner_output_proves_success(output: &str) -> bool {
    // A run that executed zero tests (`test result: ok. 0 passed; 0 failed`,
    // pytest `no tests ran`, go `[no tests to run]`, jest `No tests found`)
    // carries success-shaped text but proves nothing (AGENTS.md rule 3).
    if runner_output_proves_no_tests_ran(output) {
        return false;
    }
    let text = captured_runner_output(output);
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("failed") {
        let abs = from + rel;
        // A nonzero count or a bare "failed" vetoes the credit; `0 failed`
        // is neutral.
        if failed_count_before(&lower, abs) != Some(0) {
            return false;
        }
        from = abs + 1;
    }
    let positive_tally = |word: &str| tallies_before(&lower, word).iter().any(|&n| n > 0);
    // pytest reports collection/fixture errors as `N error(s)` beside
    // `M passed` with no "failed" word (`2 passed, 1 error in 0.1s`): a
    // nonzero error tally vetoes the credit.
    if positive_tally("error")
        || runner_output_proves_failure(output)
        || lower.contains("traceback (most recent call last)")
    {
        return false;
    }
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let unittest_ok = lines
        .iter()
        .any(|line| unittest_ran_count(line).is_some_and(|n| n > 0))
        && lines
            .iter()
            .any(|line| *line == "OK" || line.starts_with("OK ("));
    let go_ok = text
        .lines()
        .any(|line| is_go_test_ok_line(line) && !go_ok_line_ran_nothing(line));
    let tap_ok = lines
        .iter()
        .any(|line| tap_count(line, "pass").is_some_and(|n| n > 0))
        && lines.iter().any(|line| tap_count(line, "fail") == Some(0));
    // `test result: ok` and a `0 failed` tally are NOT success markers on
    // their own: libtest prints both for a binary that ran nothing. The
    // libtest/pytest/jest/vitest credit is the executed-test tally `N passed`
    // (N > 0), which every passing libtest summary carries.
    positive_tally("passed") || positive_tally("passing") || unittest_ok || go_ok || tap_ok
}

/// A go `ok  \t<pkg>\t<dur> [no tests to run]` line: the package passed
/// because a `-run` filter matched no test in it.
fn go_ok_line_ran_nothing(line: &str) -> bool {
    line.contains("[no tests to run]")
}

/// Unambiguous evidence that a test runner finished WITHOUT EXECUTING ANY
/// TEST — the classic case being a mistyped filter (`cargo test typo`
/// exits 0 with `running 0 tests` in every binary). Such a run is neither a
/// pass nor a failure of the code: it must earn no verification credit, and
/// recording it as a failure would park a check that never ran (the same
/// reasoning as the 126/127 "could not execute" skip).
///
/// Recognised zero-execution markers:
/// - a structured `cargo_test` result with `no_tests_ran: true`;
/// - libtest `test result:` summaries whose `N passed` tallies sum to zero;
/// - pytest `no tests ran` / an `N deselected` with nothing passed;
/// - unittest `Ran 0 tests` / `NO TESTS RAN`;
/// - go `[no test files]` / `[no tests to run]` / `testing: warning: no
///   tests to run` with no package that actually ran tests;
/// - jest `No tests found`, vitest `No test files found`;
/// - mocha `0 passing` / `No test files found`, node TAP `# tests 0`;
/// - ava `Couldn't find any files to test`.
///
/// Any failure marker or any evidence of an executed test (a positive
/// `passed`/`passing` tally, `Ran N` with N > 0, a go `ok` line that ran
/// tests, a positive TAP `# pass`) vetoes the verdict, so a multi-binary run
/// where only the doc-tests were empty still counts as having run tests.
pub(crate) fn runner_output_proves_no_tests_ran(output: &str) -> bool {
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(output) {
        if map.get("no_tests_ran").and_then(Value::as_bool) == Some(true) {
            return true;
        }
    }
    if runner_output_proves_failure(output) {
        return false;
    }
    let text = captured_runner_output(output);
    let lower = text.to_lowercase();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let positive_tally = |word: &str| tallies_before(&lower, word).iter().any(|&n| n > 0);
    let zero_tally = |word: &str| tallies_before(&lower, word).contains(&0);

    let executed = positive_tally("passed")
        || positive_tally("passing")
        || lines
            .iter()
            .any(|line| unittest_ran_count(line).is_some_and(|n| n > 0))
        || text
            .lines()
            .any(|line| is_go_test_ok_line(line) && !go_ok_line_ran_nothing(line))
        || lines
            .iter()
            .any(|line| tap_count(line, "pass").is_some_and(|n| n > 0));
    if executed {
        return false;
    }

    // libtest: a `test result:` summary that carries its tallies, every
    // `N passed` being zero (checked above). A bare `test result: ok` with no
    // tally is ambiguous, not proof that nothing ran.
    (lines
        .iter()
        .any(|line| line.to_lowercase().starts_with("test result:"))
        && !tallies_before(&lower, "passed").is_empty())
        || lower.contains("no tests ran")
        || positive_tally("deselected")
        || lines.iter().any(|line| unittest_ran_count(line) == Some(0))
        || lower.contains("[no test files]")
        || lower.contains("[no tests to run]")
        || lower.contains("no tests to run")
        || lower.contains("no tests found")
        || lower.contains("no test files found")
        || lower.contains("couldn't find any files to test")
        || zero_tally("passing")
        || lines.iter().any(|line| tap_count(line, "tests") == Some(0))
}

/// Did this verification call run a TEST RUNNER that executed no test?
/// Only test-executing calls qualify (`cargo_test`, a shell command whose
/// [`VerificationKind`] is `TestExecution`): compile checks legitimately run
/// no tests and keep their compile credit, so a project with no tests at all
/// is still verifiable through `cargo check` & co.
pub(crate) fn verification_call_ran_no_tests(name: &str, args: &Value, result: &str) -> bool {
    let runs_tests = match name {
        "cargo_test" => true,
        "shell_exec" | "pty_shell" => {
            args.get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| {
                    shell_command_verification_kind(command)
                        == Some(VerificationKind::TestExecution)
                })
        }
        _ => false,
    };
    runs_tests && runner_output_proves_no_tests_ran(result)
}

/// Mark a test-runner result that executed zero tests as NOT successful,
/// with an honest message, before the dispatcher derives `tool_success`
/// from it. A mistyped `cargo test <filter>` via shell exits 0; left alone,
/// every checkpoint-scan consumer of the logged success flag would credit
/// it as a green verification. Returns true when the result was annotated.
pub(crate) fn annotate_zero_test_verification(
    name: &str,
    args: &Value,
    result: &mut Value,
) -> bool {
    let args_str = serde_json::to_string(args).unwrap_or_default();
    if !tool_call_is_verification(name, &args_str) {
        return false;
    }
    let result_str = serde_json::to_string(&*result).unwrap_or_default();
    if !verification_call_ran_no_tests(name, args, &result_str) {
        return false;
    }
    if let Value::Object(map) = result {
        map.insert("success".to_string(), Value::Bool(false));
        map.insert("no_tests_ran".to_string(), Value::Bool(true));
        map.entry("message".to_string()).or_insert_with(|| {
            Value::String(
                "No tests ran: the test runner exited without executing a single test \
                 (a filter that matched nothing, or no test files). This is not a passing \
                 verification — fix the filter/path, or use a compile check if the project \
                 has no tests."
                    .to_string(),
            )
        });
        return true;
    }
    false
}

/// Why a masked verification run earned no success credit, phrased to
/// follow "the pipeline/connector masked the runner's exit status, and" in a
/// gate message. `None` when the run DID earn output credit.
///
/// Distinguishes the two gaps because their fixes differ: a filtered stream
/// (`| tail -5`, `| grep …`, `> log; cat log`) can never earn credit — a
/// filter can drop the failing lines — so the run must be repeated without
/// the filter; an unfiltered-but-masked run whose output simply carried no
/// recognised verdict should be rerun with an authoritative exit status.
pub(crate) fn masked_run_uncredited_reason(command: &str, result: &str) -> Option<&'static str> {
    if masked_run_output_proves_success(command, result) {
        return None;
    }
    Some(if !runner_output_reaches_result_unfiltered(command) {
        "its output was piped through a filter or redirected (`| tail`, `| grep`, `> file`), \
         which can drop the failing lines — filtered output never earns credit"
    } else if !tool_result_output_is_complete(result) {
        "its captured output was truncated or paginated, so a failing tail may be missing"
    } else {
        "its complete, unfiltered output carried no unambiguous success summary \
         (e.g. `test result: ok`, `N passed`, unittest `OK`)"
    })
}

/// The test-runner segment of `command`, in its ORIGINAL spelling, stripped
/// of pipes, `;`/`||` chains and output redirections — the form whose exit
/// status is authoritative. Leading `cd DIR &&` segments are kept so the
/// runner still runs where it did. `None` when no segment is a
/// [`VerificationKind::TestExecution`] runner (compile-only checks such as
/// `py_compile` / `cargo check` never qualify).
///
/// `python3 -m unittest discover -s tests 2>&1 | tail -5` →
/// `python3 -m unittest discover -s tests 2>&1`.
pub(crate) fn unmasked_test_runner_command(command: &str) -> Option<String> {
    let segments = shell_segments_with_operators(command);
    let runner_idx = segments.iter().rposition(|(_, segment)| {
        let lower = segment.trim().to_lowercase();
        match segment_verification_prefix(&lower, VERIFICATION_PREFIXES) {
            Some(prefix) => {
                !segment_is_info_only_invocation(&lower, prefix)
                    && shell_command_verification_kind(&lower)
                        == Some(VerificationKind::TestExecution)
            }
            None => false,
        }
    })?;
    let runner = strip_output_redirections(segments[runner_idx].1.trim());
    if runner.is_empty() {
        return None;
    }
    // Keep a contiguous `cd DIR && … &&` prefix: the runner's own `&&`
    // connector and every earlier one must be `&&` and each earlier segment
    // a `cd`.
    let mut start = runner_idx;
    while start > 0
        && segments[start].0.trim() == "&&"
        && first_shell_word(segments[start - 1].1.trim()) == Some("cd")
    {
        start -= 1;
    }
    let mut parts: Vec<String> = segments[start..runner_idx]
        .iter()
        .map(|(_, segment)| segment.trim().to_string())
        .collect();
    parts.push(runner);
    Some(parts.join(" && "))
}

/// Drop output-diverting redirections from a single command segment (`>
/// o`, `>>o`, `2>/dev/null`, `&> log`), keeping descriptor duplication onto
/// the captured stream (`2>&1`). Word-level: good enough for advice text.
fn strip_output_redirections(segment: &str) -> String {
    const BARE_OPERATORS: &[&str] = &[">", ">>", "1>", "1>>", "2>", "2>>", "&>", "&>>"];
    let mut kept: Vec<&str> = Vec::new();
    let mut words = segment.split_whitespace();
    while let Some(word) = words.next() {
        if BARE_OPERATORS.contains(&word) {
            words.next(); // the redirection target
            continue;
        }
        if word.contains('>') && !matches!(word, "2>&1" | "1>&2" | ">&2") {
            continue;
        }
        kept.push(word);
    }
    kept.join(" ")
}

/// A verification segment that does NOT execute a deliverable script — the
/// SmokeRun arm's mutational signal, split off from the credit gate.
///
/// The credit gate grew a SmokeRun arm: a bare deliverable-script run
/// (`bash deploy.sh`) is now "verification" in credit terms — and those
/// exact runs are the most likely to write files. The mutational question
/// (Phi's `command_may_mutate`) must not inherit that widening, or every
/// deploy script reads as read-only and the new arm's mutational signal
/// never reaches OpaqueRun (`python3 fix.py && pytest -q` lost its
/// may_have_mutated flag). Runner prefixes, test scripts and syntax checks
/// keep their pre-existing verdict; deliverable-script executions — smoke
/// tier or checked tier — run arbitrary project code and may write.
pub(crate) fn shell_segment_is_verification_without_deliverable(segment: &str) -> bool {
    shell_command_is_verification(segment) && !segment_runs_deliverable_script(segment)
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

/// Fallbacks for DELIVERABLE scripts — the standalone-script-verification
/// deadlock fix (review finding): a project with no test framework verifies
/// by running its own deliverable script (`bash deploy.sh`, `python3
/// solve.py`, `./generate.py`), and a clean run of it was refused because the
/// script's path is not test*/spec*. Only consulted when NO test-runner
/// prefix matched, so existing test-framework verdicts are untouched.
///
/// Deliberately SEPARATE from `shell_command_runs_test_script`: that one is
/// also consulted by the observational classifier, where crediting an
/// arbitrary script run as read-only would hide a mutating deliverable
/// (`bash deploy.sh` usually writes files) from mutation accounting. The
/// verification classifier alone credits deliverable runs.
///
/// Two credit tiers (P1 + HIGH follow-up review findings): a bare exit-0
/// deliverable run proves the process launched cleanly but asserts NOTHING
/// about its output — a SMOKE run, lesser credit than a test execution and
/// never labelled TestExecution. TestExecution requires a REAL check
/// construct, structurally coupled to the command's outcome: a predicate in
/// COMMAND position (grep/diff/cmp/comm/test/`[`/`[[`) whose failure gates
/// the chain's success (`python3 app.py && grep -q x out.txt`,
/// `python3 app.py | grep -q x`, `python3 app.py; [ $? -eq 0 ]`). Trigger
/// words inside QUOTED arguments or filler commands are not signals:
/// `python3 app.py && printf 'grep\n'` and a bare `rc=$?` capture check
/// nothing and stay smoke.
///
/// All-`&&` deliverable chain WITH a real check: every segment's failure
/// fails the chain, so a predicate segment's non-zero exit decides the
/// outcome → tests tier.
fn deliverable_check_status_is_authoritative(segments: &[(String, String)]) -> bool {
    if segments.iter().any(|(op, _)| op.contains("||")) {
        // `||` can skip the script or the check entirely — never evidence.
        return false;
    }
    if !segments
        .iter()
        .any(|(_, segment)| segment_runs_deliverable_script(segment))
    {
        return false;
    }
    let all_authoritative = segments
        .iter()
        .all(|(op, _)| op.is_empty() || op.trim() == "&&");
    if all_authoritative {
        return segments
            .iter()
            .any(|(_, segment)| segment_is_expected_result_checker(segment));
    }
    // `|`/`;` chain: the command's final status is the LAST segment's exit,
    // so a checker only decides the outcome when it IS last, or when every
    // segment after it is reached through `&&` — the one connector under
    // which overall success implies the checker ran and passed
    // (`cmd | grep -q x && echo pass`: the echo runs only when the grep
    // passed). A `;`/`|` segment after the checker hands the final status to
    // a command that always exits 0 (`python3 app.py; test 1 = 2; true`) —
    // the check's failure is masked and must not earn verification credit
    // (review finding #2). Unlike `cargo test | true`, this is about the
    // checker being decoupled from the outcome, not about masking a runner.
    segments.iter().enumerate().any(|(i, (_, segment))| {
        segment_is_expected_result_checker(segment)
            && segments[i + 1..].iter().all(|(op, _)| op.trim() == "&&")
    })
}

/// All-`&&` deliverable chain WITHOUT any real check: the run exited 0 but
/// nothing about its output was asserted → smoke tier.
fn deliverable_smoke_status_is_authoritative(segments: &[(String, String)]) -> bool {
    let all_authoritative = segments
        .iter()
        .all(|(op, _)| op.is_empty() || op.trim() == "&&");
    all_authoritative
        && segments
            .iter()
            .any(|(_, segment)| segment_runs_deliverable_script(segment))
        && !segments
            .iter()
            .any(|(_, segment)| segment_is_expected_result_checker(segment))
}

/// Is this pipeline segment a real check construct — a predicate over the
/// command's exit status or output (grep/diff/cmp/comm/test/`[`/`[[`) in
/// COMMAND position? Quoted keywords and filler carry the words but never the
/// check: `printf 'grep\n'` and `echo "see diff docs"` are not checks.
fn segment_is_expected_result_checker(segment: &str) -> bool {
    let Some(word) = first_shell_word(segment) else {
        return false;
    };
    let basename = word.rsplit('/').next().unwrap_or(word);
    matches!(
        basename,
        "grep" | "egrep" | "fgrep" | "diff" | "cmp" | "comm" | "test" | "[" | "[["
    )
}

/// Does this single segment execute a deliverable script — an interpreter
/// with a script-file argument, or a direct `./script` — whose path is not
/// test*/spec* (those are the test-script predicate's job)?
fn segment_runs_deliverable_script(segment: &str) -> bool {
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
        return shell_command_runs_deliverable_script(segment);
    }
    (word.starts_with("./") || word.starts_with('/')) && shell_path_is_deliverable_script(word)
}

/// A path names a deliverable script when it carries a known script extension
/// and is NOT a test* / spec* file. Test-named paths are the test-script
/// predicate's business and would otherwise be counted by both.
fn shell_path_is_deliverable_script(path: &str) -> bool {
    const SCRIPT_EXTENSIONS: &[&str] = &[
        "py", "pyw", "js", "mjs", "cjs", "ts", "rb", "pl", "pm", "php", "sh",
    ];
    let has_script_ext = path
        .rsplit_once('.')
        .map(|(_, ext)| SCRIPT_EXTENSIONS.contains(&ext))
        .unwrap_or(false);
    has_script_ext && !Agent::gate_path_is_test(path)
}

/// Interpreter-in-command-position scan for a DELIVERABLE script: the first
/// non-flag argument must be a script FILE. Inline `-c`/`-e`/`--eval` code
/// and `-m` module invocations are not script files (and are already handled
/// by the assert/`-m unittest`/`--test` test-script and prefix paths), so
/// they never earn deliverable credit.
fn shell_command_runs_deliverable_script(command: &str) -> bool {
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
        if !is_interpreter {
            continue;
        }
        for arg in &tokens[index + 1..] {
            if matches!(*arg, "-c" | "-e" | "--eval" | "-m") {
                break;
            }
            if arg.starts_with('-') {
                continue;
            }
            if shell_path_is_deliverable_script(arg) {
                return true;
            }
            break;
        }
    }
    false
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
            let mut iter = tokens[index + 1..].iter().peekable();
            while let Some(arg) = iter.next() {
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
                if *arg == "-m" {
                    if let Some(mod_name) = iter.next() {
                        if mod_name.starts_with("unittest")
                            || mod_name.starts_with("pytest")
                            || *mod_name == "test"
                        {
                            return true;
                        }
                    }
                    continue;
                }
                if matches!(*arg, "--test" | "test") {
                    return true;
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
        // `cargo clippy --fix` rewrites source; only a plain lint run observes.
        "cargo_clippy" => !serde_json::from_str::<Value>(args_str)
            .ok()
            .is_some_and(|args| cargo_clippy_applies_fixes(name, &args)),
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
        "cargo_check" | "cargo_test" => false,
        "cargo_clippy" => serde_json::from_str::<Value>(args_str)
            .ok()
            .is_some_and(|args| cargo_clippy_applies_fixes(name, &args)),
        _ => !tool_call_is_observational(name, args_str),
    }
}

pub(crate) fn read_tool_target(name: &str, args_str: &str) -> Option<String> {
    let args: Value = serde_json::from_str(args_str).ok()?;
    match name {
        // A file_read target includes its line range: paging through a large
        // file (`line_range` 1-200, then 200-400) is new ground, not a
        // re-read. On a small window an oversized read arrives as its first
        // chunk with an instruction to continue by range; keyed by path
        // alone, every next page counted as redundant and the read-loop
        // guard aborted the run with READ_LOOP_NO_EDIT (c24, 24k window, all
        // four validation rounds). An identical re-read (same path, same
        // range) is still redundant, so real thrashing still trips it.
        "file_read" => {
            let path = args.get("path").and_then(|v| v.as_str())?;
            match args.get("line_range").and_then(|r| r.as_array()) {
                Some(range) if range.len() == 2 => Some(format!(
                    "{path}#{}-{}",
                    range[0].as_i64().unwrap_or(0),
                    range[1].as_i64().unwrap_or(0)
                )),
                _ => Some(path.to_string()),
            }
        }
        "file_write" | "file_edit" | "file_delete" => args
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
