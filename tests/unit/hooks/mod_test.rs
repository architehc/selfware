use super::*;

#[test]
fn test_hook_event_display() {
    assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
    assert_eq!(HookEvent::PostToolUse.to_string(), "PostToolUse");
    assert_eq!(HookEvent::Stop.to_string(), "Stop");
}

#[test]
fn test_extract_path_from_args() {
    let args = r#"{"path": "./src/main.rs", "content": "test"}"#;
    assert_eq!(
        extract_path_from_args(args),
        Some("./src/main.rs".to_string())
    );

    let args = r#"{"command": "cargo test"}"#;
    assert_eq!(extract_path_from_args(args), None);
}

#[test]
fn test_hook_context_constructors() {
    let ctx = HookContext::pre_tool("file_write", r#"{"path": "test.rs"}"#);
    assert_eq!(ctx.event, HookEvent::PreToolUse);
    assert_eq!(ctx.tool_name.as_deref(), Some("file_write"));
    assert_eq!(ctx.affected_path.as_deref(), Some("test.rs"));

    let ctx = HookContext::post_tool("file_edit", r#"{"path": "x.rs"}"#, true, "ok");
    assert_eq!(ctx.event, HookEvent::PostToolUse);
    assert_eq!(ctx.tool_success, Some(true));

    let ctx = HookContext::stop();
    assert_eq!(ctx.event, HookEvent::Stop);
    assert!(ctx.tool_name.is_none());
}

#[test]
fn test_hook_registry_empty() {
    let registry = HookRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(!registry.has_hooks_for(&HookEvent::Stop));
}

#[test]
fn test_hook_registry_from_config() {
    let hooks = vec![HookConfig {
        event: HookEvent::PostToolUse,
        command: "cargo fmt".to_string(),
        match_tools: vec!["file_write".to_string()],
        timeout_secs: 30,
    }];
    let registry = HookRegistry::from_config(&hooks);
    assert_eq!(registry.len(), 1);
    assert!(registry.has_hooks_for(&HookEvent::PostToolUse));
    assert!(!registry.has_hooks_for(&HookEvent::PreToolUse));
}

#[tokio::test]
async fn test_fire_no_matching_hooks() {
    let registry = HookRegistry::new();
    let ctx = HookContext::stop();
    let action = registry.fire(&ctx).await;
    assert!(matches!(action, HookAction::Continue));
}

fn hook(event: HookEvent, command: &str, timeout_secs: u64) -> HookConfig {
    HookConfig {
        event,
        command: command.to_string(),
        match_tools: vec![],
        timeout_secs,
    }
}

#[tokio::test]
async fn fire_pre_tool_hook_timeout_fails_closed_with_honest_message() {
    let registry = HookRegistry::from_config(&[hook(HookEvent::PreToolUse, "sleep 10", 1)]);
    let ctx = HookContext::pre_tool("file_write", r#"{"path":"a.rs"}"#);
    let (reason, kind) = match registry.fire(&ctx).await {
        HookAction::Skip { reason, kind } => (reason, kind),
        other => panic!("pre-tool hook timeout must not let the tool run, got: {other:?}"),
    };
    assert_eq!(kind, SkipKind::HookFailure);

    let (msg, audit, failure_kind) = pre_tool_skip_message("file_write", &reason, kind);
    assert!(
        msg.starts_with(
            "Tool 'file_write' was not run: its PreToolUse policy hook could not complete (timed out after 1s)"
        ),
        "msg: {msg}"
    );
    assert!(msg.contains("infrastructure failure of the hook, not a policy decision"));
    assert!(
        !msg.contains("POLICY BLOCK"),
        "must not claim a policy decision: {msg}"
    );
    assert!(!msg.contains("MUST NOT"), "msg: {msg}");
    assert!(audit.contains("could not complete"));
    assert_eq!(failure_kind, "hook_failure");
}

#[tokio::test]
async fn fire_pre_tool_hook_nonzero_exit_is_policy_block() {
    let registry = HookRegistry::from_config(&[hook(HookEvent::PreToolUse, "exit 1", 5)]);
    let ctx = HookContext::pre_tool("file_write", r#"{"path":"prod.toml"}"#);
    let (reason, kind) = match registry.fire(&ctx).await {
        HookAction::Skip { reason, kind } => (reason, kind),
        other => panic!("non-zero pre-tool hook must block, got: {other:?}"),
    };
    assert_eq!(kind, SkipKind::Policy);
    let (msg, _audit, failure_kind) = pre_tool_skip_message("file_write", &reason, kind);
    assert!(
        msg.starts_with("POLICY BLOCK: Tool 'file_write' was blocked by PreToolUse hook policy")
    );
    assert!(msg.contains("MUST NOT attempt to bypass"));
    assert_eq!(failure_kind, "hook_policy");
}

#[tokio::test]
async fn fire_post_tool_hook_timeout_continues() {
    let registry = HookRegistry::from_config(&[hook(HookEvent::PostToolUse, "sleep 10", 1)]);
    let ctx = HookContext::post_tool("file_write", r#"{"path":"a.rs"}"#, true, "ok");
    let action = registry.fire(&ctx).await;
    assert!(
        matches!(action, HookAction::Continue),
        "post-tool hook errors stay non-fatal, got: {action:?}"
    );
}

#[tokio::test]
async fn fire_stop_hook_failure_continues() {
    let registry = HookRegistry::from_config(&[hook(HookEvent::Stop, "exit 2", 5)]);
    let action = registry.fire(&HookContext::stop()).await;
    assert!(matches!(action, HookAction::Continue), "got: {action:?}");
}
