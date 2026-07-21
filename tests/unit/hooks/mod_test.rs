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
