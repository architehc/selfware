//! Confirmation prompts for destructive operations

// Feature-gated module

use std::io::{self, Write};

/// Operations that require confirmation
#[derive(Debug, Clone, PartialEq)]
pub enum DestructiveOperation {
    /// Deleting files
    FileDelete {
        path: String,
    },
    /// Force git operations
    GitForcePush {
        branch: String,
    },
    GitResetHard,
    GitClean,
    /// Shell commands that modify system
    ShellExec {
        command: String,
    },
    /// Overwriting existing files
    FileOverwrite {
        path: String,
    },
    /// Database modifications
    DatabaseModify {
        query: String,
    },
}

impl DestructiveOperation {
    /// Get a human-readable description of the operation
    pub fn description(&self) -> String {
        match self {
            Self::FileDelete { path } => format!("Delete file: {}", path),
            Self::GitForcePush { branch } => format!("Force push to branch: {}", branch),
            Self::GitResetHard => "Reset git repository (discard all changes)".to_string(),
            Self::GitClean => "Clean untracked files from repository".to_string(),
            Self::ShellExec { command } => {
                format!("Execute shell command: {}", truncate(command, 50))
            }
            Self::FileOverwrite { path } => format!("Overwrite existing file: {}", path),
            Self::DatabaseModify { query } => format!("Modify database: {}", truncate(query, 50)),
        }
    }

    /// Get the risk level of this operation
    pub fn risk_level(&self) -> RiskLevel {
        match self {
            Self::GitForcePush { .. } | Self::GitResetHard | Self::GitClean => RiskLevel::High,
            Self::FileDelete { .. } | Self::DatabaseModify { .. } => RiskLevel::Medium,
            Self::ShellExec { .. } | Self::FileOverwrite { .. } => RiskLevel::Low,
        }
    }
}

/// Risk level for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
        }
    }

    pub fn color_code(&self) -> &'static str {
        match self {
            Self::Low => "\x1b[33m",    // Yellow
            Self::Medium => "\x1b[35m", // Magenta
            Self::High => "\x1b[31m",   // Red
        }
    }
}

/// Configuration for confirmation behavior
#[derive(Debug, Clone)]
pub struct ConfirmConfig {
    /// Whether to require confirmation at all
    pub enabled: bool,
    /// Minimum risk level that requires confirmation
    pub min_risk_level: RiskLevel,
    /// Whether to auto-approve in non-interactive mode
    pub auto_approve_non_interactive: bool,
    /// Tool names that always require confirmation
    pub always_confirm: Vec<String>,
    /// Tool names that never require confirmation
    pub never_confirm: Vec<String>,
}

impl Default for ConfirmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_risk_level: RiskLevel::Medium,
            auto_approve_non_interactive: false,
            always_confirm: vec!["git_push".to_string(), "file_delete".to_string()],
            never_confirm: vec![
                "file_read".to_string(),
                "directory_tree".to_string(),
                "git_status".to_string(),
                "git_diff".to_string(),
            ],
        }
    }
}

/// Result of a confirmation prompt
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfirmResult {
    /// User approved the operation
    Approved,
    /// User rejected the operation
    Rejected,
    /// User requested to skip this operation
    Skipped,
    /// Confirmation was not required
    NotRequired,
}

/// Check if an operation requires confirmation
pub fn requires_confirmation(
    tool_name: &str,
    operation: Option<&DestructiveOperation>,
    config: &ConfirmConfig,
) -> bool {
    if !config.enabled {
        return false;
    }

    // Check never_confirm list
    if config.never_confirm.iter().any(|t| t == tool_name) {
        return false;
    }

    // Check always_confirm list
    if config.always_confirm.iter().any(|t| t == tool_name) {
        return true;
    }

    // Check by risk level
    if let Some(op) = operation {
        op.risk_level() >= config.min_risk_level
    } else {
        false
    }
}

/// Non-interactive confirmation (for testing or automation)
pub fn auto_confirm(operation: &DestructiveOperation, config: &ConfirmConfig) -> ConfirmResult {
    if config.auto_approve_non_interactive {
        tracing::warn!(
            "Auto-approving operation in non-interactive mode: {}",
            operation.description()
        );
        ConfirmResult::Approved
    } else {
        tracing::error!(
            "Operation rejected in non-interactive mode: {}",
            operation.description()
        );
        ConfirmResult::Rejected
    }
}

/// Detect if a shell command is potentially destructive
pub fn detect_destructive_shell_command(command: &str) -> Option<DestructiveOperation> {
    let dangerous_patterns = [
        ("rm -rf", true),
        ("rm -r", true),
        ("rmdir", true),
        ("git push -f", true),
        ("git push --force", true),
        ("git reset --hard", true),
        ("git clean", true),
        ("DROP TABLE", true),
        ("DROP DATABASE", true),
        ("DELETE FROM", true),
        ("TRUNCATE", true),
        ("> /dev/", true),
        ("dd if=", true),
        ("mkfs", true),
    ];

    for (pattern, _) in &dangerous_patterns {
        if command.to_lowercase().contains(&pattern.to_lowercase()) {
            return Some(DestructiveOperation::ShellExec {
                command: command.to_string(),
            });
        }
    }

    None
}

/// Detect if a git operation is destructive
pub fn detect_destructive_git_operation(
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<DestructiveOperation> {
    match tool_name {
        "git_push" => {
            if args.get("force").and_then(|v| v.as_bool()).unwrap_or(false) {
                let branch = args
                    .get("branch")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Some(DestructiveOperation::GitForcePush { branch })
            } else {
                None
            }
        }
        "git_reset" => {
            if args.get("hard").and_then(|v| v.as_bool()).unwrap_or(false) {
                Some(DestructiveOperation::GitResetHard)
            } else {
                None
            }
        }
        "git_clean" => Some(DestructiveOperation::GitClean),
        _ => None,
    }
}

/// Truncate a string for display (UTF-8 safe)
fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/confirm/confirm_test.rs"]
mod tests;
