use super::*;

#[test]
fn test_risk_level_ordering() {
    assert!(RiskLevel::High > RiskLevel::Medium);
    assert!(RiskLevel::Medium > RiskLevel::Low);
}

#[test]
fn test_execution_mode_display() {
    assert_eq!(ExecutionMode::Normal.to_string(), "normal");
    assert_eq!(ExecutionMode::Plan.to_string(), "plan");
    assert_eq!(ExecutionMode::Auto.to_string(), "auto");
    assert_eq!(ExecutionMode::Yolo.to_string(), "yolo");
}

#[test]
fn test_permission_checker_plan_mode() {
    let checker = PermissionChecker::new(ExecutionMode::Plan);
    let read_meta = ToolMetadata::read_only();
    let write_meta = ToolMetadata::file_write();

    assert_eq!(
        checker.check("file_read", &read_meta, &Value::Null),
        PermissionResult::Allow
    );
    assert!(matches!(
        checker.check("file_write", &write_meta, &Value::Null),
        PermissionResult::Deny { .. }
    ));
}

#[test]
fn test_permission_checker_normal_mode() {
    let checker = PermissionChecker::new(ExecutionMode::Normal);
    let read_meta = ToolMetadata::read_only();
    let write_meta = ToolMetadata::file_write();
    let destructive_meta = ToolMetadata::file_destructive();

    assert!(matches!(
        checker.check("file_read", &read_meta, &Value::Null),
        PermissionResult::Allow
    ));
    assert!(matches!(
        checker.check("file_write", &write_meta, &Value::Null),
        PermissionResult::Prompt { .. }
    ));
    assert!(matches!(
        checker.check("file_delete", &destructive_meta, &Value::Null),
        PermissionResult::Prompt { .. }
    ));
}

#[test]
fn test_permission_checker_auto_mode() {
    let checker = PermissionChecker::new(ExecutionMode::Auto);
    let read_meta = ToolMetadata::read_only();
    let write_meta = ToolMetadata::file_write();

    assert!(matches!(
        checker.check("file_read", &read_meta, &Value::Null),
        PermissionResult::Allow
    ));
    assert!(matches!(
        checker.check("file_write", &write_meta, &Value::Null),
        PermissionResult::Allow
    ));
}

#[test]
fn test_permission_checker_yolo_mode() {
    let checker = PermissionChecker::new(ExecutionMode::Yolo);
    let read_meta = ToolMetadata::read_only();

    assert!(matches!(
        checker.check("file_read", &read_meta, &Value::Null),
        PermissionResult::Allow
    ));
}

#[test]
fn test_shell_meta_prompts_in_auto_mode() {
    // A shell tool executes commands and must require confirmation in Auto mode.
    let checker = PermissionChecker::new(ExecutionMode::Auto);
    let shell_meta = ToolMetadata::shell();
    let result = checker.check("shell_exec", &shell_meta, &Value::Null);
    assert!(
        matches!(result, PermissionResult::Prompt { .. }),
        "a shell tool should prompt in Auto mode, got {:?}",
        result
    );
}

#[test]
fn test_medium_risk_auto_approved_in_auto_mode() {
    // A Medium-risk, non-destructive tool should be auto-approved in Auto mode.
    let checker = PermissionChecker::new(ExecutionMode::Auto);
    let meta = ToolMetadata::custom(false, false, RiskLevel::Medium, true, false);
    let result = checker.check("http_request", &meta, &Value::Null);
    assert_eq!(result, PermissionResult::Allow);
}

#[test]
fn test_default_tool_metadata() {
    assert!(default_tool_metadata("file_read").read_only);
    assert!(!default_tool_metadata("file_write").read_only);
    assert!(default_tool_metadata("file_delete").destructive);
    assert_eq!(
        default_tool_metadata("shell_exec").risk_level,
        RiskLevel::High
    );
    assert!(default_tool_metadata("tool_search").read_only);
    assert_eq!(
        default_tool_metadata("tool_search").risk_level,
        RiskLevel::Low
    );
}

// ===== normal_mode_needs_confirmation tests (P1-5) =====

fn no_grants() -> crate::safety::permissions::PermissionStore {
    crate::safety::permissions::PermissionStore::new()
}

#[test]
fn test_normal_mode_read_only_tools_skip_confirmation() {
    // The old hardcoded safe list prompted for these harmless read-only
    // tools; metadata classification must now let them through.
    for tool in [
        // previously safe-listed
        "file_read",
        "directory_tree",
        "glob_find",
        "grep_search",
        "symbol_search",
        "tool_search",
        "git_status",
        "git_diff",
        // the P1-5 report's examples
        "lsp_diagnostics",
        "process_list",
        "ask_user",
        // other metadata-classified read-only tools
        "port_check",
        "lsp_hover",
        "code_metrics",
        "list_worktrees",
        "knowledge_query",
    ] {
        assert!(
            !normal_mode_needs_confirmation(tool, &[], &no_grants()),
            "read-only tool '{}' must not prompt in Normal mode",
            tool
        );
    }
}

#[test]
fn test_normal_mode_egress_tools_prompt() {
    // vision/screen tools upload file bytes to the model endpoint — network
    // egress, not read-only. They must confirm in Normal mode (review round 9:
    // the mislabel let them exfiltrate without prompting).
    for tool in ["vision_analyze", "vision_compare", "screen_capture"] {
        assert!(
            normal_mode_needs_confirmation(tool, &[], &no_grants()),
            "egress tool '{}' must prompt in Normal mode",
            tool
        );
    }
}

#[test]
fn test_normal_mode_mutating_tools_still_prompt() {
    for tool in [
        "file_write",
        "file_edit",
        "file_multi_edit",
        "patch_apply",
        "file_delete",
        "shell_exec",
        "pty_shell",
        "git_commit",
        "git_push",
        "cargo_test",
    ] {
        assert!(
            normal_mode_needs_confirmation(tool, &[], &no_grants()),
            "mutating tool '{}' must still prompt in Normal mode",
            tool
        );
    }
}

#[test]
fn test_normal_mode_network_tools_still_prompt() {
    // Network tools are read_only=true but Medium risk — they must keep
    // prompting (only Low-risk read-only tools are exempt).
    for tool in ["http_request", "browser_fetch", "page_control"] {
        assert!(
            normal_mode_needs_confirmation(tool, &[], &no_grants()),
            "network tool '{}' must still prompt in Normal mode",
            tool
        );
    }
}

#[test]
fn test_normal_mode_unclassified_tool_prompts() {
    // Dynamic MCP tool names have no explicit classification — keep the
    // old prompt-by-default behavior.
    assert!(normal_mode_needs_confirmation(
        "mcp_server_thing",
        &[],
        &no_grants()
    ));
}

#[test]
fn test_normal_mode_require_confirmation_overrides_metadata() {
    // An operator-listed read-only tool must still prompt when present
    // in safety.require_confirmation.
    let require = vec!["file_read".to_string()];
    assert!(normal_mode_needs_confirmation(
        "file_read",
        &require,
        &no_grants()
    ));
}

#[test]
fn test_normal_mode_session_grant_skips_confirmation() {
    // The "always allow" prompt option records a session grant; a grant
    // must short-circuit even tools that would otherwise prompt.
    let mut store = crate::safety::permissions::PermissionStore::new();
    store.add(crate::safety::permissions::PermissionGrant::session(
        "shell_exec",
    ));
    assert!(!normal_mode_needs_confirmation("shell_exec", &[], &store));
    // The grant is tool-scoped: other tools are unaffected.
    assert!(normal_mode_needs_confirmation("file_write", &[], &store));
}

#[test]
fn test_tool_metadata_builder() {
    let meta = ToolMetadata::custom(true, false, RiskLevel::Low, true, false);
    assert!(meta.read_only);
    assert!(!meta.destructive);
    assert_eq!(meta.risk_level, RiskLevel::Low);
    assert!(meta.network_access);
    assert!(!meta.shell_execution);
}
