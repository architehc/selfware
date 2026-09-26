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

/// browser_eval executes arbitrary JS inside the page and can mutate DOM,
/// storage and remote state, so it is NOT read-only — the old network()
/// classification labelled it read-only and let it execute over MCP without
/// the write opt-in (2026-09-21 review finding).
#[test]
fn test_browser_eval_classified_as_write_capable() {
    let meta = default_tool_metadata("browser_eval");
    assert!(!meta.read_only, "browser_eval must be write-capable");
    assert!(meta.network_access, "browser_eval talks to the browser");
    assert!(
        !meta.destructive,
        "browser_eval is not in the destructive class — only the write class covers it"
    );
}

/// pip_freeze and knowledge_export can WRITE (output_file / output_path) and
/// must be write-capable, not read-only: the MCP write gate keys on read_only
/// (2026-09-21 follow-up review finding — freeze-to-file rode through without
/// the opt-in).
#[test]
fn test_pip_freeze_and_knowledge_export_classified_as_write_capable() {
    let freeze = default_tool_metadata("pip_freeze");
    assert!(!freeze.read_only, "pip_freeze must be write-capable");
    assert_eq!(freeze.risk_level, RiskLevel::Medium);

    let export = default_tool_metadata("knowledge_export");
    assert!(!export.read_only, "knowledge_export must be write-capable");
    assert_eq!(export.risk_level, RiskLevel::Medium);

    // Sibling reads keep their read-only classification.
    assert!(default_tool_metadata("pip_list").read_only);
    assert!(default_tool_metadata("knowledge_query").read_only);
}

/// browser_screenshot / browser_pdf ALWAYS write a destination file
/// (output_path defaults even when omitted) — they are write-capable, not
/// read-only; the old network() label let them ride through the MCP write
/// gate without the opt-in (2026-09-21 review finding).
#[test]
fn test_browser_screenshot_and_pdf_classified_as_write_capable() {
    let shot = default_tool_metadata("browser_screenshot");
    assert!(!shot.read_only, "browser_screenshot must be write-capable");
    assert!(
        shot.network_access,
        "browser_screenshot egresses to the browser"
    );
    assert_eq!(shot.risk_level, RiskLevel::Medium);

    let pdf = default_tool_metadata("browser_pdf");
    assert!(!pdf.read_only, "browser_pdf must be write-capable");
    assert!(pdf.network_access, "browser_pdf egresses to the browser");
    assert_eq!(pdf.risk_level, RiskLevel::Medium);

    // Sibling browser reads keep their read-only classification.
    assert!(default_tool_metadata("browser_fetch").read_only);
    assert!(default_tool_metadata("browser_links").read_only);
}

// ===== normal_mode_call_needs_confirmation (0.9.1 field report) =====

fn call_needs(tool: &str, args: serde_json::Value) -> bool {
    normal_mode_call_needs_confirmation(tool, &args, &[], &no_grants())
}

#[test]
fn test_normal_mode_context_tools_are_read_only_and_run_without_asking() {
    // 0.9.1 live: `context_bulk_read` asked "Execute?" every time because the
    // context tools had no explicit classification (unclassified ⇒ prompt).
    for tool in crate::tools::context::CONTEXT_TOOL_NAMES {
        let meta = classify_tool_metadata(tool).expect("context tools are classified");
        assert!(
            meta.read_only && meta.risk_level == RiskLevel::Low,
            "{tool}"
        );
        assert!(
            !call_needs(tool, serde_json::json!({"pattern": "src/*.rs"})),
            "{tool} must not prompt in Normal mode"
        );
    }
}

#[test]
fn test_normal_mode_plain_verification_calls_run_without_asking() {
    assert!(!call_needs("cargo_check", serde_json::json!({})));
    assert!(!call_needs(
        "cargo_check",
        serde_json::json!({"all_targets": true, "all_features": false, "release": false})
    ));
    assert!(!call_needs("cargo_test", serde_json::json!({})));
    assert!(!call_needs(
        "cargo_test",
        serde_json::json!({"package": "selfware", "test_name": "safety::tests", "no_fail_fast": true})
    ));
    assert!(!call_needs("cargo_clippy", serde_json::json!({})));
    assert!(!call_needs(
        "cargo_clippy",
        serde_json::json!({"fix": false})
    ));
}

#[test]
fn test_normal_mode_writing_or_flag_injecting_verification_calls_prompt() {
    // clippy --fix rewrites sources.
    assert!(call_needs("cargo_clippy", serde_json::json!({"fix": true})));
    assert!(call_needs(
        "cargo_clippy",
        serde_json::json!({"fix": "yes"})
    ));
    // A string argument is passed positionally to cargo: a leading `-` would
    // turn it into a cargo flag (`--config` runner = arbitrary command).
    assert!(call_needs(
        "cargo_test",
        serde_json::json!({"test_name": "--config=target.x.runner='sh -c id'"})
    ));
    assert!(call_needs(
        "cargo_test",
        serde_json::json!({"package": "--manifest-path=/tmp/evil/Cargo.toml"})
    ));
    assert!(call_needs(
        "cargo_test",
        serde_json::json!({"test_name": " -q"})
    ));
    // Nested / unexpected structures are not the plain form.
    assert!(call_needs(
        "cargo_check",
        serde_json::json!({"extra": ["--x"]})
    ));
    assert!(call_needs("cargo_check", serde_json::Value::Null));
    // cargo_fmt rewrites sources — never auto-allowed.
    assert!(call_needs("cargo_fmt", serde_json::json!({})));
}

#[test]
fn test_normal_mode_call_policy_keeps_mutations_and_shell_prompting() {
    for (tool, args) in [
        ("shell_exec", serde_json::json!({"command": "ls"})),
        ("pty_shell", serde_json::json!({"action": "start"})),
        (
            "file_write",
            serde_json::json!({"path": "a", "content": "b"}),
        ),
        (
            "file_edit",
            serde_json::json!({"path": "a", "old_str": "b", "new_str": "c"}),
        ),
        ("git_commit", serde_json::json!({"message": "m"})),
        ("git_push", serde_json::json!({})),
        ("pip_install", serde_json::json!({"package": "x"})),
        ("npm_install", serde_json::json!({})),
        ("http_request", serde_json::json!({"url": "http://x"})),
        ("mcp_server_thing", serde_json::json!({})),
    ] {
        assert!(call_needs(tool, args), "{tool} must still prompt");
    }
}

#[test]
fn test_normal_mode_call_policy_respects_require_confirmation_and_grants() {
    let require = vec!["cargo_test".to_string(), "context_bulk_read".to_string()];
    assert!(normal_mode_call_needs_confirmation(
        "cargo_test",
        &serde_json::json!({}),
        &require,
        &no_grants()
    ));
    assert!(normal_mode_call_needs_confirmation(
        "context_bulk_read",
        &serde_json::json!({}),
        &require,
        &no_grants()
    ));
    let mut store = crate::safety::permissions::PermissionStore::new();
    store.add(crate::safety::permissions::PermissionGrant::session(
        "file_edit",
    ));
    assert!(!normal_mode_call_needs_confirmation(
        "file_edit",
        &serde_json::json!({}),
        &[],
        &store
    ));
}

#[test]
fn test_normal_mode_shell_rule_allows_only_matching_shell_calls() {
    let mut store = crate::safety::permissions::PermissionStore::new();
    store.add_shell_rule(crate::safety::permissions::ShellAllowRule::for_command(
        "python3 -m unittest tests.test_slug",
    ));
    let needs = |cmd: &str| {
        normal_mode_call_needs_confirmation(
            "shell_exec",
            &serde_json::json!({ "command": cmd }),
            &[],
            &store,
        )
    };
    assert!(!needs("python3 -m unittest"));
    assert!(!needs("python3 -m unittest tests.test_other"));
    assert!(needs("python3 -m unittest; rm -rf ~"));
    assert!(needs("python3 -m unittest && git push"));
    assert!(needs("pip3 install -r dev.requirements.txt"));
    // The rule never leaks to other tools.
    assert!(normal_mode_call_needs_confirmation(
        "pty_shell",
        &serde_json::json!({"action": "send", "command": "python3 -m unittest"}),
        &[],
        &store
    ));
}

#[test]
fn test_cargo_call_injects_flags() {
    use serde_json::json;
    assert!(!cargo_call_injects_flags("cargo_test", &json!({})));
    assert!(!cargo_call_injects_flags(
        "cargo_test",
        &json!({"package": "selfware", "test_name": "a::b", "release": false})
    ));
    assert!(cargo_call_injects_flags(
        "cargo_test",
        &json!({"test_name": "--config=target.x.runner='sh -c id'"})
    ));
    assert!(cargo_call_injects_flags("cargo_fmt", &json!({"x": " -v"})));
    assert!(cargo_call_injects_flags(
        "cargo_check",
        &json!({"x": ["a"]})
    ));
    assert!(cargo_call_injects_flags("cargo_clippy", &json!(null)));
    // Not a cargo tool: not this guard's business.
    assert!(!cargo_call_injects_flags(
        "shell_exec",
        &json!({"command": "-x"})
    ));
}
