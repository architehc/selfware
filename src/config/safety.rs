//! Safety guardrails configuration.

use serde::{Deserialize, Serialize};

use super::types::default_true;

/// Safety guardrails: allowed/denied paths, protected branches, and confirmation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<String>,
    /// Denylist semantics: an explicit `denied_paths` key in a config file can
    /// only ADD restrictions — `Config::load` unions it with
    /// [`default_denied_paths`] (see `union_with_default_denied_paths`), so the
    /// built-in credential/.git protections cannot be removed by config.
    #[serde(default = "default_denied_paths")]
    pub denied_paths: Vec<String>,
    #[serde(default = "default_protected_branches")]
    pub protected_branches: Vec<String>,
    #[serde(default = "default_require_confirmation")]
    pub require_confirmation: Vec<String>,
    /// When true, config files with overly permissive permissions (group- or
    /// world-readable, i.e. mode & 0o077 != 0) cause a hard error instead of a
    /// warning.  Can also be activated via `SELFWARE_STRICT_PERMISSIONS=1`.
    /// Default: false (backward compatible -- warn only).
    #[serde(default)]
    pub strict_permissions: bool,
    /// Pre-authorized permission grants for tool execution.
    /// Reduces confirmation prompts for trusted operations.
    #[serde(default)]
    pub permissions: Vec<crate::safety::permissions::PermissionGrant>,
    /// Trust-gate tool results: scan tool output for prompt-injection
    /// patterns before it enters the model's context and neutralize
    /// high-severity findings in place (the loop cannot refuse a result, so
    /// offending lines are replaced and flagged instead of dropped).
    /// Default: true.
    #[serde(default = "default_true")]
    pub trust_gate_tool_results: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            allowed_paths: default_allowed_paths(),
            denied_paths: default_denied_paths(),
            protected_branches: default_protected_branches(),
            require_confirmation: default_require_confirmation(),
            strict_permissions: false,
            permissions: Vec::new(),
            trust_gate_tool_results: true,
        }
    }
}

pub fn default_allowed_paths() -> Vec<String> {
    vec!["./**".to_string()]
}
pub fn default_denied_paths() -> Vec<String> {
    vec![
        "**/.env".to_string(),
        "**/.env.local".to_string(),
        // `.env.production`, `.env.staging`, … — the same credential file in
        // a costume (red-team wave-3 finding).
        "**/.env.*".to_string(),
        // Suffix form too: `production.env` / `db.env` — the same credential
        // file named the other way around (wave-25).
        "**/*.env".to_string(),
        // The .ssh DIRECTORY itself, not just its contents: `**/.ssh/**`
        // needs a component below .ssh, so file_list/file_read on the bare
        // dir (enumerate key filenames — recon) slipped past (red-team
        // wave-11 finding).
        "**/.ssh".to_string(),
        "**/.ssh/**".to_string(),
        // Same bare-dir shape for secrets dirs (file_list on the dir itself).
        "**/secrets".to_string(),
        "**/secrets/**".to_string(),
        // Block writing into git executable-config vectors: an agent could
        // plant a hook script or rewrite git config to later execute in the
        // user's shell.  Legitimate git operations go through the git tool, not
        // through file_write/file_edit into .git/.
        "**/.git/hooks/**".to_string(),
        "**/.git/config".to_string(),
    ]
}

/// Union an explicit (config-file) `denied_paths` list with the built-in
/// defaults: denylist semantics — explicit entries can only ADD restrictions,
/// never remove the safety defaults. Without this, an explicit key REPLACES
/// the default list (serde field semantics), so a stale generated config with
/// the old 3-entry list (`**/.env`, `**/secrets/**`, `**/.ssh/**`) silently
/// re-exposed `.env.production`, bare `secrets/`, `db.env` and `.git/config`
/// (red-team review finding). Defaults come first so their relative order —
/// and the doc comments grouping them — stay stable; explicit entries keep
/// their own order after, de-duplicated.
pub fn union_with_default_denied_paths(explicit: Vec<String>) -> Vec<String> {
    let mut merged = default_denied_paths();
    for pattern in explicit {
        if !merged.contains(&pattern) {
            merged.push(pattern);
        }
    }
    merged
}

/// The default `denied_paths` rendered as a TOML array literal, for generated
/// configs (unpack / auto-config). Emitting the full current list keeps the
/// file on disk in sync with the effective policy instead of freezing a stale
/// subset into every generated config.
pub fn default_denied_paths_toml() -> String {
    let entries: Vec<String> = default_denied_paths()
        .iter()
        .map(|p| format!("\"{}\"", p))
        .collect();
    format!("[{}]", entries.join(", "))
}

pub fn default_protected_branches() -> Vec<String> {
    vec!["main".to_string(), "master".to_string()]
}

pub fn default_require_confirmation() -> Vec<String> {
    vec![
        "git_push".to_string(),
        "file_delete".to_string(),
        "shell_exec".to_string(),
    ]
}

#[cfg(test)]
#[path = "../../tests/unit/config/safety/safety_test.rs"]
mod tests;
