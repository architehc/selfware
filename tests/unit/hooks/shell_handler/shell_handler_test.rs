use super::*;
use crate::hooks::HookContext;

#[test]
fn test_expand_placeholders() {
    let ctx = HookContext::post_tool("file_write", r#"{"path": "src/main.rs"}"#, true, "ok");

    let cmd = expand_placeholders("cargo fmt -- {path}", &ctx);
    assert_eq!(cmd, "cargo fmt -- 'src/main.rs'");

    let cmd = expand_placeholders("echo {tool} modified {path}", &ctx);
    assert_eq!(cmd, "echo 'file_write' modified 'src/main.rs'");
}

#[test]
fn test_expand_placeholders_no_path() {
    let ctx = HookContext::stop();
    let cmd = expand_placeholders("cargo test", &ctx);
    assert_eq!(cmd, "cargo test");
}

#[test]
fn test_expand_placeholders_shell_quotes_injection_chars() {
    let ctx = HookContext::post_tool(
        "file_write",
        r#"{"path": "src/main.rs; touch /tmp/pwned"}"#,
        true,
        "ok",
    );

    let cmd = expand_placeholders("cargo fmt -- {path}", &ctx);
    assert_eq!(cmd, "cargo fmt -- 'src/main.rs; touch /tmp/pwned'");
}

#[tokio::test]
async fn test_run_shell_command_success() {
    let output = run_shell_command("echo hello", Duration::from_secs(5))
        .await
        .unwrap()
        .expect("should not time out");
    assert!(output.success);
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout.contains("hello"));
}

#[tokio::test]
async fn test_run_shell_command_failure() {
    let output = run_shell_command("false", Duration::from_secs(5))
        .await
        .unwrap()
        .expect("should not time out");
    assert!(!output.success);
    assert_ne!(output.exit_code, 0);
}

#[tokio::test]
async fn run_shell_command_does_not_leak_secret_env() {
    // A secret in the agent's environment must NOT reach a hook command.
    std::env::set_var("SELFWARE_HOOK_SECRET_TEST", "topsecret");
    let output = run_shell_command(
        "printf %s \"${SELFWARE_HOOK_SECRET_TEST:-CLEARED}\"",
        Duration::from_secs(5),
    )
    .await
    .unwrap()
    .expect("should not time out");
    std::env::remove_var("SELFWARE_HOOK_SECRET_TEST");
    assert_eq!(
        output.stdout.trim(),
        "CLEARED",
        "hook must not inherit secret env vars, got: {:?}",
        output.stdout
    );

    // PATH stays allowlisted so hook commands still resolve binaries.
    let path_out = run_shell_command("printf %s \"${PATH:+HASPATH}\"", Duration::from_secs(5))
        .await
        .unwrap()
        .expect("should not time out");
    assert_eq!(
        path_out.stdout.trim(),
        "HASPATH",
        "PATH must remain available to hooks"
    );
}

#[tokio::test]
async fn run_shell_command_timeout_returns_none() {
    // A command that sleeps longer than the timeout must return Ok(None).
    let result = run_shell_command("sleep 10", Duration::from_secs(1))
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "expected None (timed out), got: {:?}",
        result
    );
}

#[tokio::test]
#[cfg(unix)]
async fn hook_timeout_kills_child_process_tree() {
    // A hook whose command is `sleep 3; touch <marker>` with a 1-second
    // timeout must kill the whole process group on timeout, so the marker
    // file is NEVER created — proving the child was reaped before `touch`
    // could run.
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("marker");

    let hook = HookConfig {
        command: format!("sleep 3; touch {}", marker.display()),
        event: crate::hooks::HookEvent::PostToolUse,
        timeout_secs: 1,
        match_tools: vec![],
    };
    let ctx = HookContext::stop();

    let action = execute_hook(&hook, &ctx).await;

    // The action must be an Error (PostToolUse + timeout → Error).
    match action {
        HookAction::Error { message } => {
            assert!(message.contains("timed out"), "message was: {message}");
        }
        other => panic!("expected Error, got: {:?}", other),
    }

    // Wait long enough for the `sleep 3` to have finished IF the child had
    // been left running. 3.5s total > 3s sleep.
    tokio::time::sleep(Duration::from_millis(3500)).await;

    assert!(
        !marker.exists(),
        "marker file was created — the child process tree was NOT killed on timeout!"
    );
}

fn pre_hook(command: &str, timeout_secs: u64) -> HookConfig {
    HookConfig {
        command: command.to_string(),
        event: crate::hooks::HookEvent::PreToolUse,
        timeout_secs,
        match_tools: vec![],
    }
}

#[tokio::test]
async fn pre_tool_hook_timeout_fails_closed_as_hook_failure() {
    // A PreToolUse hook that never completes must NOT let the tool run, and
    // must not be reported as a policy decision.
    let hook = pre_hook("sleep 10", 1);
    let ctx = HookContext::pre_tool("file_write", r#"{"path":"a.rs"}"#);
    match execute_hook(&hook, &ctx).await {
        HookAction::Skip { reason, kind } => {
            assert_eq!(kind, crate::hooks::SkipKind::HookFailure);
            assert!(reason.contains("timed out after 1s"), "reason: {reason}");
        }
        other => panic!("pre-tool hook timeout must fail closed, got: {other:?}"),
    }
}

#[tokio::test]
async fn pre_tool_hook_nonzero_exit_is_policy_skip() {
    let hook = pre_hook("echo denied >&2; exit 3", 5);
    let ctx = HookContext::pre_tool("file_write", r#"{"path":"a.rs"}"#);
    match execute_hook(&hook, &ctx).await {
        HookAction::Skip { reason, kind } => {
            assert_eq!(kind, crate::hooks::SkipKind::Policy);
            assert!(reason.contains("exited with code 3"), "reason: {reason}");
            assert!(reason.contains("denied"), "reason: {reason}");
        }
        other => panic!("non-zero pre-tool hook must be a policy skip, got: {other:?}"),
    }
}

#[test]
fn failure_action_spawn_failure_fails_closed_only_for_pre_tool() {
    match failure_action(true, "hook-cmd", "failed to start: boom".to_string()) {
        HookAction::Skip { reason, kind } => {
            assert_eq!(kind, crate::hooks::SkipKind::HookFailure);
            assert_eq!(reason, "failed to start: boom");
        }
        other => panic!("expected fail-closed Skip, got: {other:?}"),
    }
    match failure_action(false, "hook-cmd", "failed to start: boom".to_string()) {
        HookAction::Error { message } => {
            assert!(message.contains("hook-cmd") && message.contains("failed to start: boom"));
        }
        other => panic!("post/stop hook failure must stay non-fatal, got: {other:?}"),
    }
}

#[tokio::test]
#[cfg(unix)]
async fn hook_exiting_with_background_descendant_holding_pipes_does_not_hang() {
    // The hook exits 0 immediately but a backgrounded `sleep 30` inherits its
    // stdout/stderr. Before the fix, the unbounded drain awaited EOF for 30s.
    let started = std::time::Instant::now();
    let output = run_shell_command("sleep 30 & exit 0", Duration::from_secs(2))
        .await
        .unwrap()
        .expect("hook exited on its own; must not be reported as a timeout");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(3),
        "hook with a lingering descendant must return within ~3s, took {elapsed:?}"
    );
    assert!(output.success, "the hook's own exit status was 0");
    assert!(output.killed_descendants);

    // Through execute_hook, the hook's decision (exit 0) is honoured.
    let started = std::time::Instant::now();
    let action = execute_hook(
        &pre_hook("sleep 30 & exit 0", 2),
        &HookContext::pre_tool("file_write", "{}"),
    )
    .await;
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(action, HookAction::Continue), "got: {action:?}");
}
