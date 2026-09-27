//! Permission grants for tool execution.
//!
//! Allows users to pre-authorize specific tools/patterns with optional
//! time-based expiry, reducing confirmation prompts for trusted operations.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::info;

/// A pre-authorized permission grant for tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrant {
    /// Tool name pattern (exact match or glob, e.g., "file_*").
    pub tool_pattern: String,
    /// Resource pattern the grant applies to (e.g., "./src/**").
    #[serde(default)]
    pub resource_pattern: Option<String>,
    /// When this grant expires. None means it persists until removed.
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Human-readable reason for the grant.
    #[serde(default)]
    pub reason: Option<String>,
}

impl PermissionGrant {
    /// Create a permanent grant for a tool pattern.
    pub fn permanent(tool_pattern: &str) -> Self {
        Self {
            tool_pattern: tool_pattern.to_string(),
            resource_pattern: None,
            expires_at: None,
            reason: None,
        }
    }

    /// Create a grant that expires after `duration` from now.
    pub fn temporary(tool_pattern: &str, duration: Duration) -> Self {
        Self {
            tool_pattern: tool_pattern.to_string(),
            resource_pattern: None,
            expires_at: Some(Utc::now() + duration),
            reason: None,
        }
    }

    /// Create a session-scoped grant (expires in 24 hours).
    pub fn session(tool_pattern: &str) -> Self {
        Self::temporary(tool_pattern, Duration::hours(24))
    }

    /// Add a resource pattern constraint.
    pub fn with_resource(mut self, pattern: &str) -> Self {
        self.resource_pattern = Some(pattern.to_string());
        self
    }

    /// Check if this grant has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Check if this grant matches a tool name.
    pub fn matches_tool(&self, tool_name: &str) -> bool {
        if self.is_expired() {
            return false;
        }
        pattern_matches(&self.tool_pattern, tool_name)
    }

    /// Check if this grant matches a tool name and resource path.
    pub fn matches(&self, tool_name: &str, resource_path: Option<&str>) -> bool {
        if !self.matches_tool(tool_name) {
            return false;
        }

        // If the grant has a resource pattern, the path must match
        if let Some(ref res_pattern) = self.resource_pattern {
            if let Some(path) = resource_path {
                return pattern_matches(res_pattern, path);
            }
            return false; // Grant requires a resource but none provided
        }

        true // No resource constraint
    }
}

/// Simple glob-like pattern matching (supports `*` wildcard).
fn pattern_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }

    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }

    pattern == value
}

/// Characters that make a shell command more than "program + arguments":
/// separators, pipes, redirections, substitutions, subshells, escapes and
/// line breaks. A command containing any of them is never matched by a
/// PREFIX rule — only by an exact rule created from that identical command.
/// Scanned on the raw text, quotes included (conservative: a `;` inside
/// quotes also disqualifies).
pub const SHELL_RULE_METACHARS: &[char] = &[
    ';', '&', '|', '$', '`', '<', '>', '(', ')', '\\', '\n', '\r', '\0',
];

/// Programs for which no prefix rule is ever offered — a prefix would grant
/// "anything": wrappers that run another command, shells, deletion /
/// permission / network tools. The operator can still allow the EXACT
/// command.
pub const SHELL_RULE_EXACT_ONLY_PROGRAMS: &[&str] = &[
    "sudo",
    "su",
    "doas",
    "env",
    "eval",
    "exec",
    "xargs",
    "nohup",
    "time",
    "timeout",
    "nice",
    "ionice",
    "command",
    "builtin",
    "source",
    ".",
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "csh",
    "tcsh",
    "rm",
    "rmdir",
    "dd",
    "mkfs",
    "shred",
    "chmod",
    "chown",
    "chgrp",
    "mv",
    // File writers and script-capable editors (`awk` runs `system()`): a
    // prefix grant would allow every later rewrite (review, 0.9.2).
    "cp",
    "ln",
    "touch",
    "truncate",
    "sed",
    "awk",
    "tee",
    "patch",
    "curl",
    "wget",
    "ssh",
    "scp",
    "sftp",
    "rsync",
    "nc",
    "ncat",
    "osascript",
    "open",
    "kill",
    "killall",
    "pkill",
    "crontab",
    "launchctl",
    "systemctl",
    "find",
];

/// Interpreters: a prefix is offered only for the `-m <module>` form
/// (`python3 -m unittest`); `python3 script.py` / `node -e …` are exact-only.
pub const SHELL_RULE_INTERPRETERS: &[&str] = &[
    "python",
    "python2",
    "python3",
    "py",
    "node",
    "deno",
    "bun",
    "ruby",
    "perl",
    "php",
    "lua",
    "rscript",
    "pwsh",
    "powershell",
];

/// Multi-purpose tools whose bare name must never be the whole prefix — it
/// would allow every subcommand (`git push`, `npm publish`, `docker run`…).
pub const SHELL_RULE_NEEDS_SUBCOMMAND: &[&str] = &[
    "git",
    "cargo",
    "npm",
    "npx",
    "pnpm",
    "pnpx",
    "yarn",
    "bun",
    "bunx",
    "pip",
    "pip3",
    "pipx",
    "uv",
    "uvx",
    "poetry",
    "pdm",
    "conda",
    "docker",
    "podman",
    "kubectl",
    "helm",
    "gh",
    "brew",
    "apt",
    "apt-get",
    "dnf",
    "yum",
    "go",
    "make",
    "gradle",
    "mvn",
    "dotnet",
    "terraform",
    "aws",
    "gcloud",
    "az",
    "heroku",
    "fly",
    "vercel",
];

/// Maximum tokens in a derived command prefix.
pub const SHELL_RULE_MAX_PREFIX_TOKENS: usize = 3;

/// A session-scoped allowance for `shell_exec` commands, created by the `p`
/// answer at the confirmation prompt.
///
/// - [`ShellAllowRule::Prefix`] — the first 1–3 plain tokens of a command
///   (e.g. `python3 -m unittest`). It matches a later command only when that
///   command contains NO shell metacharacter ([`SHELL_RULE_METACHARS`]) and
///   its leading whitespace-separated tokens equal the prefix exactly.
/// - [`ShellAllowRule::Exact`] — one full command, matched only by the
///   identical (trimmed) command. Offered when no safe prefix exists: the
///   command has metacharacters, starts with an env assignment, or its
///   program is a wrapper/shell/destructive tool
///   ([`SHELL_RULE_EXACT_ONLY_PROGRAMS`]), an interpreter run of a script,
///   or a multi-purpose tool with no plain subcommand
///   ([`SHELL_RULE_NEEDS_SUBCOMMAND`]).
///
/// Neither kind matches a call that sets `env` — environment variables such
/// as `LD_PRELOAD` / `PYTHONPATH` change what the same command line runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellAllowRule {
    /// Commands whose leading tokens are exactly these.
    Prefix(Vec<String>),
    /// Exactly this command.
    Exact(String),
}

fn plain_prefix_token(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '@' | '+' | ','))
}

/// Whether a command's risk tag allows a prefix rule: it only reads, or runs
/// project code (tests, builds, scripts), and does nothing the classifier
/// tags as writing, installing, networking, rewriting git history or
/// deleting.
fn prefix_eligible_risk(command: &str) -> bool {
    use crate::safety::confirm_view::{classify_shell_risk, RiskTag};
    matches!(
        classify_shell_risk(command),
        RiskTag::Reads | RiskTag::RunsCommand
    )
}

impl ShellAllowRule {
    /// The rule the prompt offers for `command`: a safe prefix when one
    /// exists, otherwise the exact command.
    pub fn for_command(command: &str) -> Self {
        let trimmed = command.trim();
        let exact = || ShellAllowRule::Exact(trimmed.to_string());
        if trimmed.contains(SHELL_RULE_METACHARS) {
            return exact();
        }
        // Only commands that merely read or run project code may seed a
        // prefix rule. A write (`sed -i`), install (`pip install`), network,
        // git-history or delete command is exact-only: a prefix grant would
        // allow every later rewrite or install unasked (review, 0.9.2).
        if !prefix_eligible_risk(trimmed) {
            return exact();
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(first) = tokens.first() else {
            return exact();
        };
        // `NAME=value cmd` changes the environment; paths to programs
        // (`./x`, `/usr/bin/x`) and odd names are exact-only too.
        if !plain_prefix_token(first) {
            return exact();
        }
        let program = first.to_ascii_lowercase();
        if SHELL_RULE_EXACT_ONLY_PROGRAMS.contains(&program.as_str()) {
            return exact();
        }
        if SHELL_RULE_INTERPRETERS.contains(&program.as_str()) {
            return match (tokens.get(1), tokens.get(2)) {
                (Some(&"-m"), Some(module))
                    if plain_prefix_token(module)
                        && !matches!(*module, "pip" | "pip3" | "pipx" | "ensurepip" | "venv") =>
                {
                    ShellAllowRule::Prefix(tokens[..3].iter().map(|t| t.to_string()).collect())
                }
                _ => exact(),
            };
        }
        // Options never belong to a prefix: `sed -i` as a prefix would be an
        // in-place-edit grant.
        let prefix: Vec<String> = tokens
            .iter()
            .take(SHELL_RULE_MAX_PREFIX_TOKENS)
            .take_while(|t| plain_prefix_token(t) && !t.starts_with('-'))
            .map(|t| t.to_string())
            .collect();
        if prefix.is_empty() {
            return exact();
        }
        // A bare multi-purpose tool (`git`, `cargo`, `npm`, …) as the whole
        // prefix would allow every subcommand (push, publish, …).
        if prefix.len() == 1 && SHELL_RULE_NEEDS_SUBCOMMAND.contains(&program.as_str()) {
            return exact();
        }
        ShellAllowRule::Prefix(prefix)
    }

    /// Whether this rule allows `command`.
    pub fn matches(&self, command: &str) -> bool {
        let trimmed = command.trim();
        match self {
            ShellAllowRule::Exact(rule) => trimmed == rule,
            ShellAllowRule::Prefix(prefix) => {
                if prefix.is_empty() || trimmed.contains(SHELL_RULE_METACHARS) {
                    return false;
                }
                let tokens: Vec<&str> = trimmed.split_whitespace().collect();
                tokens.len() >= prefix.len()
                    && tokens
                        .iter()
                        .zip(prefix.iter())
                        .all(|(t, p)| *t == p.as_str())
                    // Nothing after the prefix may be an option: a granted
                    // `cargo test` must not carry `--config=…runner=…`, and a
                    // granted `python3 -m unittest` must not become `-c …`.
                    && tokens[prefix.len()..].iter().all(|t| !t.starts_with('-'))
                    // …and the concrete command must still only read or run
                    // project code.
                    && prefix_eligible_risk(trimmed)
            }
        }
    }

    /// Prompt / log wording, e.g. "commands starting with `cargo test`".
    pub fn describe(&self) -> String {
        match self {
            ShellAllowRule::Prefix(prefix) => {
                format!("commands starting with `{}`", prefix.join(" "))
            }
            ShellAllowRule::Exact(_) => "this exact command".to_string(),
        }
    }
}

/// The command of a `shell_exec` call when the call is eligible for a shell
/// rule: `command` (or its `cmd` alias) present and non-empty, and no `env`
/// overrides.
pub fn shell_rule_command(tool_name: &str, args: &serde_json::Value) -> Option<String> {
    if tool_name != "shell_exec" {
        return None;
    }
    match args.get("env") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::Object(map)) if map.is_empty() => {}
        Some(_) => return None,
    }
    let command = args
        .get("command")
        .or_else(|| args.get("cmd"))
        .and_then(|v| v.as_str())?;
    if command.trim().is_empty() {
        return None;
    }
    Some(command.to_string())
}

/// Store for managing permission grants.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionStore {
    grants: Vec<PermissionGrant>,
    /// Session-only `shell_exec` rules (`p` at the prompt). Never loaded
    /// from or written to configuration.
    #[serde(skip)]
    shell_rules: Vec<ShellAllowRule>,
}

impl PermissionStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a store from configuration grants.
    pub fn from_config(grants: &[PermissionGrant]) -> Self {
        Self {
            grants: grants.to_vec(),
            shell_rules: Vec::new(),
        }
    }

    /// Add a session-scoped `shell_exec` rule.
    pub fn add_shell_rule(&mut self, rule: ShellAllowRule) {
        info!("Shell rule granted for this session: {:?}", rule);
        if !self.shell_rules.contains(&rule) {
            self.shell_rules.push(rule);
        }
    }

    /// Whether a session shell rule allows this call (see
    /// [`shell_rule_command`] for eligibility).
    pub fn shell_rule_allows(&self, tool_name: &str, args: &serde_json::Value) -> bool {
        let Some(command) = shell_rule_command(tool_name, args) else {
            return false;
        };
        self.shell_rules.iter().any(|rule| rule.matches(&command))
    }

    /// Add a new grant.
    pub fn add(&mut self, grant: PermissionGrant) {
        info!(
            "Permission granted: {} (expires: {:?})",
            grant.tool_pattern, grant.expires_at
        );
        self.grants.push(grant);
    }

    /// Check if the given tool+resource is authorized by any grant.
    pub fn is_authorized(&self, tool_name: &str, resource_path: Option<&str>) -> bool {
        self.grants
            .iter()
            .any(|g| g.matches(tool_name, resource_path))
    }

    /// Number of active (non-expired) grants.
    pub fn active_count(&self) -> usize {
        self.grants.iter().filter(|g| !g.is_expired()).count()
    }

    /// Clear all grants (and session shell rules).
    pub fn clear(&mut self) {
        self.grants.clear();
        self.shell_rules.clear();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/permissions/permissions_test.rs"]
mod tests;
