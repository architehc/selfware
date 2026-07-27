use std::hash::{Hash, Hasher};

use serde_json::Value;

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

pub(crate) fn tool_call_is_mutating(name: &str, args: &serde_json::Value) -> bool {
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
    ];

    verification_prefixes
        .iter()
        .any(|prefix| command_contains_at_boundary(&normalized, prefix))
        || shell_command_runs_test_script(&normalized)
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
