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
