use super::*;

#[test]
fn test_yolo_config_default() {
    let config = YoloConfig::default();
    assert!(!config.enabled);
    assert!(config.allow_git_push);
}

#[test]
fn test_yolo_config_for_coding() {
    let config = YoloConfig::for_coding();
    assert!(config.enabled);
    assert!(!config.allow_git_push); // Safer default
}

#[test]
fn test_is_forbidden() {
    let config = YoloConfig::default();
    assert!(config.is_forbidden("rm -rf /"));
    assert!(config.is_forbidden("sudo rm -rf /"));
    assert!(!config.is_forbidden("rm file.txt"));
}

#[test]
fn test_is_protected_path() {
    let config = YoloConfig::default();
    assert!(config.is_protected_path("/etc/passwd"));
    assert!(config.is_protected_path("/usr/bin/bash"));
    assert!(!config.is_protected_path("/home/user/project"));
}

#[test]
fn test_yolo_manager_inactive_by_default() {
    let config = YoloConfig::default();
    let manager = YoloManager::new(config);
    assert!(!manager.is_active());
}

#[test]
fn test_yolo_manager_enable_disable() {
    let config = YoloConfig {
        enabled: true,
        ..Default::default()
    };
    let manager = YoloManager::new(config);

    assert!(manager.is_active());
    manager.disable();
    assert!(!manager.is_active());
    manager.enable();
    assert!(manager.is_active());
}

#[test]
fn test_auto_approve_when_active() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"path": "/home/user/test.txt"});
    let decision = manager.should_auto_approve("file_read", &args);

    assert_eq!(decision, YoloDecision::AutoApprove);
}

#[test]
fn test_block_forbidden_operation() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"command": "rm -rf /"});
    let decision = manager.should_auto_approve("shell_exec", &args);

    assert!(matches!(decision, YoloDecision::Block(_)));
}

#[test]
fn test_block_protected_path() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"path": "/etc/passwd"});
    let decision = manager.should_auto_approve("file_write", &args);

    assert!(matches!(decision, YoloDecision::Block(_)));
}

#[test]
fn test_require_confirmation_git_push() {
    let config = YoloConfig::for_coding(); // git push disabled
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"branch": "main"});
    let decision = manager.should_auto_approve("git_push", &args);

    assert!(matches!(decision, YoloDecision::RequireConfirmation(_)));
}

#[test]
fn test_operation_counting() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    assert_eq!(manager.operation_count(), 0);

    manager.record_operation(
        "file_read",
        &serde_json::json!({"path": "test.txt"}),
        true,
        AuditResult::Success,
        100,
    );

    assert_eq!(manager.operation_count(), 1);
}

#[test]
fn test_max_operations_limit() {
    let mut config = YoloConfig::fully_autonomous();
    config.max_operations = 2;
    let manager = YoloManager::new(config);

    assert!(manager.is_active());

    manager.record_operation("t1", &serde_json::json!({}), true, AuditResult::Success, 0);
    assert!(manager.is_active());

    manager.record_operation("t2", &serde_json::json!({}), true, AuditResult::Success, 0);
    assert!(!manager.is_active()); // Limit reached
}

#[test]
fn test_audit_summary() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    manager.record_operation(
        "file_read",
        &serde_json::json!({}),
        true,
        AuditResult::Success,
        50,
    );
    manager.record_operation(
        "file_write",
        &serde_json::json!({}),
        true,
        AuditResult::Success,
        100,
    );
    manager.record_operation(
        "shell_exec",
        &serde_json::json!({}),
        true,
        AuditResult::Failed("error".to_string()),
        200,
    );

    let summary = manager.audit_summary();

    assert_eq!(summary.total_operations, 3);
    assert_eq!(summary.success, 2);
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.total_duration_ms, 350);
}

#[test]
fn test_is_destructive_command() {
    assert!(is_destructive_command("rm -rf /tmp/test"));
    assert!(is_destructive_command("git push --force"));
    assert!(is_destructive_command("DROP TABLE users"));
    assert!(!is_destructive_command("ls -la"));
    assert!(!is_destructive_command("cargo test"));
}

#[test]
fn test_summarize_args_truncates() {
    let long_content = "x".repeat(200);
    let args = serde_json::json!({"content": long_content});
    let summary = summarize_args(&args);

    assert!(summary.len() < 250);
    assert!(summary.contains("200 chars"));
}

#[test]
fn test_expand_home() {
    // This test depends on HOME being set
    if std::env::var("HOME").is_ok() {
        let expanded = expand_home("~/test");
        assert!(!expanded.starts_with("~"));
        assert!(expanded.ends_with("/test"));
    }
}

#[test]
fn test_yolo_config_default_values() {
    let config = YoloConfig::default();
    assert!(!config.enabled);
    assert_eq!(config.max_operations, 0);
    assert!((config.max_hours - 0.0).abs() < f64::EPSILON);
    assert!(config.allow_git_push);
    assert!(!config.allow_destructive_shell);
    assert!(config.audit_log_path.is_none());
    assert_eq!(config.status_interval, 100);
}

#[test]
fn test_yolo_config_for_coding_values() {
    let config = YoloConfig::for_coding();
    assert!(config.enabled);
    assert!(!config.allow_git_push);
    assert!(!config.allow_destructive_shell);
    assert_eq!(config.status_interval, 50);
}

#[test]
fn test_yolo_config_fully_autonomous() {
    let config = YoloConfig::fully_autonomous();
    assert!(config.enabled);
    assert!(config.allow_git_push);
    assert!(!config.allow_destructive_shell);
}

#[test]
fn test_yolo_config_with_destructive_shell() {
    let config = YoloConfig::for_coding().with_destructive_shell(true);
    assert!(config.allow_destructive_shell);

    let config2 = YoloConfig::for_coding().with_destructive_shell(false);
    assert!(!config2.allow_destructive_shell);
}

#[test]
fn test_yolo_config_with_git_push() {
    let config = YoloConfig::for_coding().with_git_push(true);
    assert!(config.allow_git_push);

    let config2 = YoloConfig::fully_autonomous().with_git_push(false);
    assert!(!config2.allow_git_push);
}

#[test]
fn test_is_forbidden_case_insensitive() {
    let config = YoloConfig::default();
    assert!(config.is_forbidden("RM -RF /"));
    assert!(config.is_forbidden("DD IF=/DEV/ZERO"));
    assert!(!config.is_forbidden("ls -la"));
}

#[test]
fn test_yolo_decision_eq() {
    assert_eq!(YoloDecision::AutoApprove, YoloDecision::AutoApprove);
    assert_ne!(
        YoloDecision::AutoApprove,
        YoloDecision::Block("x".to_string())
    );
}

#[test]
fn test_yolo_decision_debug() {
    let decision = YoloDecision::RequireConfirmation("test".to_string());
    let debug_str = format!("{:?}", decision);
    assert!(debug_str.contains("RequireConfirmation"));
}

#[test]
fn test_audit_result_variants() {
    let success = AuditResult::Success;
    let failed = AuditResult::Failed("error".to_string());
    let blocked = AuditResult::Blocked("protected".to_string());

    let _ = format!("{:?}", success);
    let _ = format!("{:?}", failed);
    let _ = format!("{:?}", blocked);
}

#[test]
fn test_audit_entry_clone() {
    let entry = AuditEntry {
        timestamp: Utc::now(),
        operation_id: 1,
        tool_name: "test".to_string(),
        arguments_summary: "args".to_string(),
        auto_approved: true,
        result: AuditResult::Success,
        duration_ms: 100,
    };

    let cloned = entry.clone();
    assert_eq!(entry.operation_id, cloned.operation_id);
    assert_eq!(entry.tool_name, cloned.tool_name);
}

#[test]
fn test_audit_entry_serde() {
    let entry = AuditEntry {
        timestamp: Utc::now(),
        operation_id: 1,
        tool_name: "file_read".to_string(),
        arguments_summary: "path: test.txt".to_string(),
        auto_approved: true,
        result: AuditResult::Success,
        duration_ms: 50,
    };

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("file_read"));
    assert!(json.contains("operation_id"));

    let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.tool_name, entry.tool_name);
}

#[test]
fn test_yolo_config_clone() {
    let config = YoloConfig::fully_autonomous();
    let cloned = config.clone();
    assert_eq!(config.enabled, cloned.enabled);
    assert_eq!(config.allow_git_push, cloned.allow_git_push);
}

#[test]
fn test_yolo_config_serde() {
    let config = YoloConfig::for_coding();
    let json = serde_json::to_string(&config).unwrap();
    assert!(json.contains("enabled"));

    let parsed: YoloConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.enabled, config.enabled);
}

#[test]
fn test_audit_summary_fields() {
    let summary = AuditSummary {
        total_operations: 10,
        success: 8,
        failed: 1,
        blocked: 1,
        tools_used: std::collections::HashMap::new(),
        total_duration_ms: 5000,
        elapsed_hours: 1.5,
    };

    let debug_str = format!("{:?}", summary);
    assert!(debug_str.contains("total_operations"));
}

#[test]
fn test_require_confirmation_destructive_shell() {
    let config = YoloConfig::fully_autonomous().with_destructive_shell(false);
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"command": "rm -rf ./test"});
    let decision = manager.should_auto_approve("shell_exec", &args);

    assert!(matches!(decision, YoloDecision::RequireConfirmation(_)));
}

#[test]
fn test_allow_destructive_shell_when_enabled() {
    let config = YoloConfig::fully_autonomous().with_destructive_shell(true);
    let manager = YoloManager::new(config);

    // Safe destructive command (not in forbidden list)
    let args = serde_json::json!({"command": "rm -rf ./test_dir"});
    let decision = manager.should_auto_approve("shell_exec", &args);

    // Should auto-approve since destructive shell is enabled
    // and it's not in the forbidden list
    assert_eq!(decision, YoloDecision::AutoApprove);
}

#[test]
fn test_harmless_shell_command_auto_approved_even_with_destructive_shell_disallowed() {
    // Regression test: shell_exec is broadly classified `destructive: true`
    // in tool_metadata regardless of the actual command. The blanket
    // "any destructive tool requires confirmation" fallback must not
    // apply to shell_exec/pty_shell/git_push -- they have their own
    // more specific per-argument checks above it -- otherwise every
    // harmless shell command would require confirmation whenever
    // allow_destructive_shell is false (the default in every production
    // config in this repo).
    let config = YoloConfig::fully_autonomous(); // allow_destructive_shell: false
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"command": "echo hello"});
    let decision = manager.should_auto_approve("shell_exec", &args);

    assert_eq!(decision, YoloDecision::AutoApprove);
}

#[test]
fn test_yolo_manager_with_audit_log_path() {
    let config = YoloConfig {
        enabled: true,
        audit_log_path: Some(PathBuf::from("/tmp/test_audit.log")),
        ..Default::default()
    };
    let manager = YoloManager::new(config);
    assert!(manager.is_active());
}

#[test]
fn test_protected_paths_include_ssh() {
    let config = YoloConfig::default();
    // SSH directory should be protected
    if std::env::var("HOME").is_ok() {
        let expanded = expand_home("~/.ssh/id_rsa");
        assert!(config.is_protected_path(&expanded) || config.is_protected_path("~/.ssh/id_rsa"));
    }
}

#[test]
fn test_expand_home_no_tilde() {
    let path = "/absolute/path";
    let expanded = expand_home(path);
    assert_eq!(expanded, path);
}

#[test]
fn test_container_run_volume_mount_etc_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({
        "image": "ubuntu",
        "volumes": ["/etc:/etc"]
    });
    let decision = manager.should_auto_approve("container_run", &args);

    assert!(matches!(decision, YoloDecision::Block(_)));
    if let YoloDecision::Block(msg) = decision {
        assert!(msg.contains("/etc:/etc"));
    }
}

#[test]
fn test_container_run_volume_mount_ssh_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({
        "image": "ubuntu",
        "volumes": ["~/.ssh:/root/.ssh"]
    });
    let decision = manager.should_auto_approve("container_run", &args);

    assert!(matches!(decision, YoloDecision::Block(_)));
    if let YoloDecision::Block(msg) = decision {
        assert!(msg.contains("~/.ssh:/root/.ssh"));
    }
}

#[test]
fn reads_sensitive_path_flags_secret_reads() {
    assert!(reads_sensitive_path("cat ~/.ssh/id_rsa").is_some());
    assert!(reads_sensitive_path("base64 .env").is_some());
    assert!(reads_sensitive_path("head -n1 ~/.aws/credentials").is_some());
    assert!(reads_sensitive_path("grep TOKEN .env").is_some());
    // Not a secret read:
    assert!(reads_sensitive_path("ls -la").is_none());
    assert!(reads_sensitive_path("cargo test").is_none());
    // Mentions a secret path but does not read contents (listing only):
    assert!(reads_sensitive_path("ls ~/.ssh/").is_none());
}

#[test]
fn reads_denied_path_honors_config_globs() {
    let denied = vec![
        "**/.env".to_string(),
        "**/secrets/**".to_string(),
        "**/.ssh/**".to_string(),
        "vault_token".to_string(),
    ];

    // Filename-only-final-segment globs match a bare token by basename.
    assert_eq!(
        reads_denied_path("cat .env", &denied).as_deref(),
        Some("**/.env")
    );
    assert_eq!(
        reads_denied_path("grep TOKEN ./config/.env", &denied).as_deref(),
        Some("**/.env")
    );
    // Directory globs match a full path token.
    assert_eq!(
        reads_denied_path("base64 ~/.ssh/id_rsa", &denied).as_deref(),
        Some("**/.ssh/**")
    );
    assert_eq!(
        reads_denied_path("cat project/secrets/db.key", &denied).as_deref(),
        Some("**/secrets/**")
    );
    // A custom non-secret glob (not in the hardcoded SENSITIVE list) is caught.
    assert_eq!(
        reads_denied_path("head -c9 vault_token", &denied).as_deref(),
        Some("vault_token")
    );

    // Listing only (no reader command) is not flagged.
    assert!(reads_denied_path("ls ~/.ssh/", &denied).is_none());
    // A benign command matching no denied glob.
    assert!(reads_denied_path("cat src/main.rs", &denied).is_none());
    // Empty deny-list short-circuits.
    assert!(reads_denied_path("cat .env", &[]).is_none());
}

#[test]
fn shell_exec_denied_glob_requires_confirmation() {
    // A YOLO config with a custom deny-glob that is NOT in the hardcoded
    // sensitive list; a shell read of it must still require confirmation.
    let mut config = YoloConfig::fully_autonomous();
    config.denied_paths = vec!["**/vault/**".to_string()];
    let manager = YoloManager::new(config);

    let args = serde_json::json!({ "command": "cat app/vault/master.key" });
    assert!(matches!(
        manager.should_auto_approve("shell_exec", &args),
        YoloDecision::RequireConfirmation(_)
    ));

    // A path outside the deny-glob is still auto-approved.
    let ok = serde_json::json!({ "command": "cat app/README.md" });
    assert_eq!(
        manager.should_auto_approve("shell_exec", &ok),
        YoloDecision::AutoApprove
    );
}

#[test]
fn shell_exec_block_reason_does_not_coach_bypass() {
    // P1-8: these reasons are pushed into the model-visible conversation
    // in unattended sessions — they must not describe how to bypass the
    // denied-path guards.
    let mut config = YoloConfig::fully_autonomous();
    config.denied_paths = vec!["**/vault/**".to_string()];
    let manager = YoloManager::new(config);

    for args in [
        serde_json::json!({ "command": "cat app/vault/master.key" }),
        serde_json::json!({ "command": "cat ~/.ssh/id_rsa" }),
    ] {
        match manager.should_auto_approve("shell_exec", &args) {
            YoloDecision::RequireConfirmation(reason) => {
                assert!(
                    reason.contains("requires confirmation"),
                    "reason should say what happened: {reason}"
                );
                assert!(
                    !reason.contains("bypass"),
                    "reason must not coach a bypass: {reason}"
                );
                assert!(
                    !reason.contains("do not cover"),
                    "reason must not describe the coverage gap: {reason}"
                );
            }
            other => panic!("expected RequireConfirmation, got {other:?}"),
        }
    }
}

#[test]
fn test_container_run_volume_mount_tmp_allowed() {
    // Enable destructive shell so the test isolates the volume-mount validator.
    let config = YoloConfig::fully_autonomous().with_destructive_shell(true);
    let manager = YoloManager::new(config);

    let args = serde_json::json!({
        "image": "ubuntu",
        "volumes": ["/tmp/data:/data"]
    });
    let decision = manager.should_auto_approve("container_run", &args);

    assert_eq!(decision, YoloDecision::AutoApprove);
}
