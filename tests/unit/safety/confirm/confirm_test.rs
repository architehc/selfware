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
