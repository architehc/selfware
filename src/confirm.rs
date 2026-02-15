//! Confirmation prompts for destructive operations

#![allow(dead_code)]

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

/// Prompt the user for confirmation
pub fn prompt_confirmation(operation: &DestructiveOperation) -> io::Result<ConfirmResult> {
    let risk = operation.risk_level();
    let reset = "\x1b[0m";

    eprintln!();
    eprintln!(
        "{}⚠️  CONFIRMATION REQUIRED [{}]{}",
        risk.color_code(),
        risk.as_str(),
        reset
    );
    eprintln!("Operation: {}", operation.description());
    eprintln!();
    eprint!("Do you want to proceed? [y/N/s(kip)]: ");
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let input = input.trim().to_lowercase();

    Ok(match input.as_str() {
        "y" | "yes" => ConfirmResult::Approved,
        "s" | "skip" => ConfirmResult::Skipped,
        _ => ConfirmResult::Rejected,
    })
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

/// Truncate a string for display
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destructive_operation_description() {
        let op = DestructiveOperation::FileDelete {
            path: "/tmp/test.txt".to_string(),
        };
        assert!(op.description().contains("/tmp/test.txt"));
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
    }

    #[test]
    fn test_requires_confirmation_disabled() {
        let config = ConfirmConfig {
            enabled: false,
            ..Default::default()
        };
        assert!(!requires_confirmation("git_push", None, &config));
    }

    #[test]
    fn test_requires_confirmation_always_list() {
        let config = ConfirmConfig::default();
        assert!(requires_confirmation("git_push", None, &config));
    }

    #[test]
    fn test_requires_confirmation_never_list() {
        let config = ConfirmConfig::default();
        assert!(!requires_confirmation("file_read", None, &config));
    }

    #[test]
    fn test_detect_destructive_shell_rm() {
        let result = detect_destructive_shell_command("rm -rf /tmp/test");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_safe() {
        let result = detect_destructive_shell_command("ls -la");
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_destructive_git_force_push() {
        let args = serde_json::json!({"force": true, "branch": "main"});
        let result = detect_destructive_git_operation("git_push", &args);
        assert!(matches!(
            result,
            Some(DestructiveOperation::GitForcePush { .. })
        ));
    }

    #[test]
    fn test_detect_destructive_git_normal_push() {
        let args = serde_json::json!({"branch": "main"});
        let result = detect_destructive_git_operation("git_push", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long() {
        assert_eq!(truncate("hello world", 5), "hello...");
    }

    #[test]
    fn test_auto_confirm_non_interactive() {
        let op = DestructiveOperation::FileDelete {
            path: "test.txt".to_string(),
        };

        let config = ConfirmConfig {
            auto_approve_non_interactive: true,
            ..Default::default()
        };
        assert_eq!(auto_confirm(&op, &config), ConfirmResult::Approved);

        let config = ConfirmConfig {
            auto_approve_non_interactive: false,
            ..Default::default()
        };
        assert_eq!(auto_confirm(&op, &config), ConfirmResult::Rejected);
    }

    #[test]
    fn test_risk_level_color() {
        assert!(RiskLevel::High.color_code().contains("31")); // Red
        assert!(RiskLevel::Medium.color_code().contains("35")); // Magenta
        assert!(RiskLevel::Low.color_code().contains("33")); // Yellow
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(RiskLevel::High.as_str(), "HIGH");
        assert_eq!(RiskLevel::Medium.as_str(), "MEDIUM");
        assert_eq!(RiskLevel::Low.as_str(), "LOW");
    }

    #[test]
    fn test_destructive_operation_git_reset() {
        let op = DestructiveOperation::GitResetHard;
        assert!(op.description().contains("Reset"));
        assert_eq!(op.risk_level(), RiskLevel::High);
    }

    #[test]
    fn test_destructive_operation_git_clean() {
        let op = DestructiveOperation::GitClean;
        assert!(op.description().contains("Clean"));
        assert_eq!(op.risk_level(), RiskLevel::High);
    }

    #[test]
    fn test_destructive_operation_file_overwrite() {
        let op = DestructiveOperation::FileOverwrite {
            path: "config.json".to_string(),
        };
        assert!(op.description().contains("config.json"));
        assert_eq!(op.risk_level(), RiskLevel::Low);
    }

    #[test]
    fn test_destructive_operation_database_modify() {
        let op = DestructiveOperation::DatabaseModify {
            query: "DELETE FROM users WHERE id = 1".to_string(),
        };
        assert!(op.description().contains("DELETE"));
        assert_eq!(op.risk_level(), RiskLevel::Medium);
    }

    #[test]
    fn test_destructive_operation_shell_exec() {
        let op = DestructiveOperation::ShellExec {
            command: "rm -rf /very/long/path/to/delete/some/files".to_string(),
        };
        let desc = op.description();
        assert!(desc.contains("rm -rf"));
        assert_eq!(op.risk_level(), RiskLevel::Low);
    }

    #[test]
    fn test_detect_destructive_shell_rmdir() {
        let result = detect_destructive_shell_command("rmdir /tmp/empty");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_git_force() {
        let result = detect_destructive_shell_command("git push --force origin main");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_git_reset() {
        let result = detect_destructive_shell_command("git reset --hard HEAD~1");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_git_clean() {
        let result = detect_destructive_shell_command("git clean -fd");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_drop_table() {
        let result = detect_destructive_shell_command("psql -c 'DROP TABLE users'");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_truncate() {
        let result = detect_destructive_shell_command("mysql -e 'TRUNCATE logs'");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_dd() {
        let result = detect_destructive_shell_command("dd if=/dev/zero of=/dev/sda");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_shell_dev_redirect() {
        let result = detect_destructive_shell_command("echo test > /dev/sda");
        assert!(result.is_some());
    }

    #[test]
    fn test_detect_destructive_git_reset_hard() {
        let args = serde_json::json!({"hard": true});
        let result = detect_destructive_git_operation("git_reset", &args);
        assert!(matches!(result, Some(DestructiveOperation::GitResetHard)));
    }

    #[test]
    fn test_detect_destructive_git_reset_soft() {
        let args = serde_json::json!({"hard": false});
        let result = detect_destructive_git_operation("git_reset", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_destructive_git_clean_operation() {
        let args = serde_json::json!({});
        let result = detect_destructive_git_operation("git_clean", &args);
        assert!(matches!(result, Some(DestructiveOperation::GitClean)));
    }

    #[test]
    fn test_detect_destructive_git_unknown() {
        let args = serde_json::json!({});
        let result = detect_destructive_git_operation("git_status", &args);
        assert!(result.is_none());
    }

    #[test]
    fn test_requires_confirmation_by_risk_level() {
        let config = ConfirmConfig {
            min_risk_level: RiskLevel::High,
            ..Default::default()
        };

        let medium_op = DestructiveOperation::FileDelete {
            path: "test.txt".to_string(),
        };
        // Medium risk should not require confirmation when min is High
        assert!(!requires_confirmation(
            "some_tool",
            Some(&medium_op),
            &config
        ));
    }

    #[test]
    fn test_requires_confirmation_high_risk() {
        let config = ConfirmConfig::default();

        let high_op = DestructiveOperation::GitResetHard;
        assert!(requires_confirmation("some_tool", Some(&high_op), &config));
    }

    #[test]
    fn test_confirm_result_equality() {
        assert_eq!(ConfirmResult::Approved, ConfirmResult::Approved);
        assert_eq!(ConfirmResult::Rejected, ConfirmResult::Rejected);
        assert_eq!(ConfirmResult::Skipped, ConfirmResult::Skipped);
        assert_eq!(ConfirmResult::NotRequired, ConfirmResult::NotRequired);
        assert_ne!(ConfirmResult::Approved, ConfirmResult::Rejected);
    }

    #[test]
    fn test_confirm_config_default_lists() {
        let config = ConfirmConfig::default();

        assert!(config.always_confirm.contains(&"git_push".to_string()));
        assert!(config.always_confirm.contains(&"file_delete".to_string()));
        assert!(config.never_confirm.contains(&"file_read".to_string()));
        assert!(config.never_confirm.contains(&"git_status".to_string()));
    }

    #[test]
    fn test_destructive_operation_git_force_push_description() {
        let op = DestructiveOperation::GitForcePush {
            branch: "main".to_string(),
        };
        let desc = op.description();
        assert!(desc.contains("Force push"));
        assert!(desc.contains("main"));
    }
}
