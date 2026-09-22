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
    assert!(config.is_protected_path(".selfware/skills/foo.md"));
    assert!(config.is_protected_path(".admitted_ledger.json"));
    assert!(!config.is_protected_path("/home/user/project"));
}

#[test]
fn test_shell_exec_targeting_protected_path_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    let args = serde_json::json!({"command": "rm -f .admitted_ledger.json"});
    let decision = manager.should_auto_approve("shell_exec", &args);
    assert!(matches!(decision, YoloDecision::Block(_)));

    let args_nested = serde_json::json!({"command": "sh -c 'echo evil > .admitted_ledger.json'"});
    let decision_nested = manager.should_auto_approve("shell_exec", &args_nested);
    assert!(matches!(decision_nested, YoloDecision::Block(_)));
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

#[test]
fn test_yolo_cp_operands_and_substitutions_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    // cp operands (both source and destination)
    for cmd in [
        "cp .admitted_ledger.json /tmp/exfil",
        "cp /tmp/evil .admitted_ledger.json",
        "cp -r .selfware/skills /tmp/skills_copy",
        "cp /tmp/evil .selfware/active_policy.json",
        "cp /tmp/evil .selfware/attempts/run_1.jsonl",
    ] {
        let args = serde_json::json!({ "command": cmd });
        let decision = manager.should_auto_approve("shell_exec", &args);
        assert!(
            matches!(decision, YoloDecision::Block(_)),
            "cp command should be blocked: {cmd}"
        );
    }

    // Command substitutions in YOLO
    for cmd in [
        "rm -f $(echo .admitted_ledger.json)",
        "rm -f `echo .admitted_ledger.json`",
        "touch $(echo .selfware/active_policy.json)",
    ] {
        let args = serde_json::json!({ "command": cmd });
        let decision = manager.should_auto_approve("shell_exec", &args);
        assert!(
            matches!(decision, YoloDecision::Block(_)),
            "subshell command should be blocked: {cmd}"
        );
    }
}

#[test]
fn test_yolo_attached_flags_and_subshells_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    // Protected path mutations with attached flags must be BLOCKED
    for cmd in [
        "sh -c'rm -f .selfware/active_policy.json'",
        "sh -c\"rm -f .selfware/active_policy.json\"",
        "sh -xc'rm -f .selfware/active_policy.json'",
        "sh -lc'rm -f .selfware/active_policy.json'",
        "sh -ec'rm -f .selfware/active_policy.json'",
        "sh -c=rm -f .selfware/active_policy.json",
        "bash -c=rm -f .selfware/active_policy.json",
        "bash --command='rm -f .selfware/active_policy.json'",
        "sh -c'echo evil > .admitted_ledger.json'",
        "sh -c'cp /tmp/evil .admitted_ledger.json'",
    ] {
        let args = serde_json::json!({ "command": cmd });
        let decision = manager.should_auto_approve("shell_exec", &args);
        assert!(
            matches!(decision, YoloDecision::Block(_)),
            "attached flag mutation command should be blocked by YOLO: {cmd}"
        );
    }

    // Sensitive path reads with attached flags must require confirmation
    for cmd in [
        "sh -c'cat .env'",
        "sh -c\"cat .env\"",
        "sh -xc'cat .env'",
        "sh -lc'cat .env'",
        "sh -c=cat .env",
        "bash -c=cat .env",
        "bash --command='cat .env'",
    ] {
        let args = serde_json::json!({ "command": cmd });
        let decision = manager.should_auto_approve("shell_exec", &args);
        assert!(
            matches!(decision, YoloDecision::RequireConfirmation(_)),
            "attached flag read command should require confirmation in YOLO: {cmd}"
        );
    }
}

#[test]
fn test_yolo_mcp_arguments_blocked() {
    let config = YoloConfig::fully_autonomous();
    let manager = YoloManager::new(config);

    // MCP plural path array targeting protected paths must be BLOCKED
    let mcp_paths = serde_json::json!({ "paths": ["src/main.rs", ".admitted_ledger.json"] });
    let decision = manager.should_auto_approve("mcp_fs_tool", &mcp_paths);
    assert!(
        matches!(decision, YoloDecision::Block(_)),
        "MCP paths array containing protected path must be blocked by YOLO"
    );

    // MCP target_paths targeting active policy must be BLOCKED
    let mcp_target_paths = serde_json::json!({ "target_paths": [".selfware/active_policy.json"] });
    let decision = manager.should_auto_approve("mcp_fs_tool", &mcp_target_paths);
    assert!(
        matches!(decision, YoloDecision::Block(_)),
        "MCP target_paths containing protected path must be blocked by YOLO"
    );

    // MCP args array command targeting protected path must be BLOCKED
    let mcp_cmd_args =
        serde_json::json!({ "args": ["bash", "-c", "rm -f .selfware/active_policy.json"] });
    let decision = manager.should_auto_approve("mcp_server_exec", &mcp_cmd_args);
    assert!(
        matches!(decision, YoloDecision::Block(_)),
        "MCP command args targeting protected path must be blocked by YOLO"
    );

    // Benign non-path MCP args (MIME types, URLs, CLI options) must NOT be blocked by YOLO
    let mcp_benign_args = serde_json::json!({
        "args": ["application/json", "https://api.example.com/v1", "--verbose", "feature/branch"]
    });
    let decision_benign = manager.should_auto_approve("mcp_server_exec", &mcp_benign_args);
    assert!(
        matches!(decision_benign, YoloDecision::AutoApprove),
        "Benign MCP arguments must be approved by YOLO"
    );

    // Generic MCP array keys containing relative protected paths must be BLOCKED by YOLO
    for nested in [
        "nested/.selfware/active_policy.json",
        "sub/secrets/key.txt",
        "--file=nested/.env",
        "image/.admitted_ledger.json",
    ] {
        let mcp_nested_items = serde_json::json!({ "items": [nested] });
        let decision = manager.should_auto_approve("mcp_custom_tool", &mcp_nested_items);
        assert!(
            matches!(decision, YoloDecision::Block(_)),
            "MCP generic items array with nested relative path '{nested}' must be blocked by YOLO"
        );
    }

    // Single-token argv destructive command in shell_exec must require confirmation in YOLO
    let single_token_cmd = serde_json::json!({ "command": ["rm -rf ./test"] });
    let decision = manager.should_auto_approve("shell_exec", &single_token_cmd);
    assert!(
        matches!(decision, YoloDecision::RequireConfirmation(_)),
        "Single-token argv destructive shell command must require confirmation in YOLO"
    );

    // Single-token argv forbidden command in shell_exec must be blocked in YOLO
    let forbidden_cmd = serde_json::json!({ "command": ["rm -rf /"] });
    let decision_forbidden = manager.should_auto_approve("shell_exec", &forbidden_cmd);
    assert!(
        matches!(decision_forbidden, YoloDecision::Block(_)),
        "Single-token argv forbidden shell command must be blocked in YOLO"
    );

    // Denied wildcard paths (e.g. **/*.csv) must be blocked by YOLO even under MIME prefixes
    let mut config_wildcard = YoloConfig::fully_autonomous();
    config_wildcard.denied_paths.push("**/*.csv".to_string());
    let manager_wildcard = YoloManager::new(config_wildcard);

    let mcp_denied_csv = serde_json::json!({ "items": ["image/customer.csv"] });
    let decision_csv = manager_wildcard.should_auto_approve("mcp_custom_tool", &mcp_denied_csv);
    assert!(
        matches!(decision_csv, YoloDecision::Block(_)),
        "Wildcard **/*.csv under image/ prefix must be blocked by YOLO"
    );

    // Benign non-denied MIME item is auto-approved
    let mcp_benign_png = serde_json::json!({ "items": ["image/valid.png"] });
    let decision_png = manager_wildcard.should_auto_approve("mcp_custom_tool", &mcp_benign_png);
    assert!(
        matches!(decision_png, YoloDecision::AutoApprove),
        "Benign image/valid.png must be auto-approved by YOLO"
    );
}

// =========================================================================
// Headless AutoEdit read-only widening: the yolo guard-heuristic oracle
// (`headless_auto_edit_shell_guard_pass`). These pin the fail-closed vetoes
// the agent's widening consults; the allow-list itself owns workspace paths
// (the checker), this owns destructive verbs / credential-shaped reads.
// =========================================================================

#[test]
fn test_headless_auto_edit_guard_pass_approves_observational_reads() {
    let denied: Vec<String> = vec![];
    let protected = YoloConfig::default().protected_paths;

    for cmd in [
        "git status --short",
        "cat src/main.rs",
        "python3 stats.py",
        "grep -rn TODO src",
        "cargo check",
    ] {
        assert!(
            super::headless_auto_edit_shell_guard_pass(cmd, &denied, &protected),
            "'{cmd}' must pass the guard heuristics (not destructive, no \
             sensitive/denied/protected-path reads)"
        );
    }
}

#[test]
fn test_headless_auto_edit_guard_pass_vetoes_destructive_and_sensitive_commands() {
    let denied: Vec<String> = vec![];
    let protected = YoloConfig::default().protected_paths;

    // Destructive verbs — never candidates regardless of how the rest looks.
    for cmd in [
        "rm -rf docs/out",
        "git push --force origin main",
        "git reset --hard HEAD",
        "rmdir empty_dir",
    ] {
        assert!(
            !super::headless_auto_edit_shell_guard_pass(cmd, &denied, &protected),
            "'{cmd}' is destructive and must FAIL the guard heuristics"
        );
    }

    // Credential-shaped reads — vetoed even as reads.
    for cmd in ["cat ~/.ssh/id_rsa", "grep -r private_key src", "cat .env"] {
        assert!(
            !super::headless_auto_edit_shell_guard_pass(cmd, &denied, &protected),
            "'{cmd}' reads a sensitive path and must FAIL the guard heuristics"
        );
    }

    // Protected-path MUTATIONS — vetoed; protected-path plain reads are the
    // allow-list's job (checker), so `/etc/passwd` under `cat` stays a pass.
    let rm_etc = "rm /etc/passwd";
    assert!(
        !super::headless_auto_edit_shell_guard_pass(rm_etc, &denied, &protected),
        "'{rm_etc}' mutates a protected path and must FAIL the guard heuristics"
    );
    assert!(
        super::headless_auto_edit_shell_guard_pass("cat /etc/passwd", &denied, &protected),
        "'cat /etc/passwd' is a READ — the workspace allow-list (checker) \
         owns it; the guard heuristics must not double-flag it"
    );
}

#[test]
fn test_headless_auto_edit_guard_pass_vetoes_deny_glob_reads() {
    let denied = vec!["**/.env".to_string(), "**/secrets/**".to_string()];
    let protected = YoloConfig::default().protected_paths;

    for cmd in ["cat docs/.env", "cat .env", "tail secrets/prod.txt"] {
        assert!(
            !super::headless_auto_edit_shell_guard_pass(cmd, &denied, &protected),
            "'{cmd}' reads a deny-glob path and must FAIL the guard heuristics"
        );
    }
}
