//! Safety guardrails configuration.

use serde::{Deserialize, Serialize};

use super::types::default_true;

/// Safety guardrails: allowed/denied paths, protected branches, and confirmation rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default = "default_allowed_paths")]
    pub allowed_paths: Vec<String>,
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
        "**/.ssh/**".to_string(),
        "**/secrets/**".to_string(),
        // Block writing into git executable-config vectors: an agent could
        // plant a hook script or rewrite git config to later execute in the
        // user's shell.  Legitimate git operations go through the git tool, not
        // through file_write/file_edit into .git/.
        "**/.git/hooks/**".to_string(),
        "**/.git/config".to_string(),
    ]
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
