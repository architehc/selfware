#![allow(dead_code, unused_imports, unused_variables)]
//! Shell-based hook execution.
//!
//! Runs hook commands via `sh -c` with placeholder substitution and timeout.

use anyhow::Result;
use tracing::{debug, info, warn};

use super::{HookAction, HookConfig, HookContext};

/// Maximum hook command output to capture (prevent unbounded memory).
const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 64 KB

/// Execute a shell-based hook command.
///
/// Substitutes placeholders in the command string:
/// - `{path}` → affected file path (if any)
/// - `{tool}` → tool name (if any)
///
/// The hook runs with a timeout. Non-zero exit codes are treated as errors
/// but do not block execution (hooks are advisory by default).
///
/// For `PreToolUse` hooks, a non-zero exit code returns `HookAction::Skip`.
pub async fn execute_hook(hook: &HookConfig, ctx: &HookContext) -> HookAction {
    let command = expand_placeholders(&hook.command, ctx);

    debug!(
        "Executing hook: {} (event: {}, timeout: {}s)",
        command, ctx.event, hook.timeout_secs
    );

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(hook.timeout_secs.max(1)),
        run_shell_command(&command),
    )
    .await;

    match result {
        Ok(Ok(output)) => {
            if output.success {
                debug!("Hook succeeded: {}", command);
                if !output.stdout.is_empty() {
                    debug!("Hook stdout: {}", output.stdout.trim());
                }
                HookAction::Continue
            } else {
                let msg = format!(
                    "Hook '{}' exited with code {} — {}",
                    command,
                    output.exit_code,
                    output.stderr.trim()
                );
                warn!("{}", msg);

                // PreToolUse hook failure means "skip this tool"
                if ctx.event == super::HookEvent::PreToolUse {
                    HookAction::Skip { reason: msg }
                } else {
                    HookAction::Error { message: msg }
                }
            }
        }
        Ok(Err(e)) => {
            let msg = format!("Hook '{}' failed to run: {}", command, e);
            warn!("{}", msg);
            if ctx.event == super::HookEvent::PreToolUse {
                HookAction::Skip { reason: msg }
            } else {
                HookAction::Error { message: msg }
            }
        }
        Err(_) => {
            let msg = format!("Hook '{}' timed out after {}s", command, hook.timeout_secs);
            warn!("{}", msg);
            if ctx.event == super::HookEvent::PreToolUse {
                HookAction::Skip { reason: msg }
            } else {
                HookAction::Error { message: msg }
            }
        }
    }
}

/// Output from a shell command execution.
struct ShellOutput {
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// Run a command via `sh -c` and capture output.
async fn run_shell_command(command: &str) -> Result<ShellOutput> {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    // Hooks execute arbitrary repo-defined commands — do NOT hand them the
    // agent's full environment, which can carry API keys and other secrets.
    // Start from an empty environment and re-add only a minimal safe allowlist
    // (matches shell_exec / ProcessManager sanitization).
    cmd.env_clear();
    for key in ["PATH", "HOME", "LANG"] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    let output = cmd.output().await?;

    let stdout =
        String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(MAX_OUTPUT_BYTES)])
            .to_string();
    let stderr =
        String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(MAX_OUTPUT_BYTES)])
            .to_string();

    Ok(ShellOutput {
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
    })
}

/// Replace `{path}`, `{tool}` placeholders in a hook command string.
fn expand_placeholders(command: &str, ctx: &HookContext) -> String {
    let mut result = command.to_string();

    if let Some(ref path) = ctx.affected_path {
        result = result.replace("{path}", &shell_quote(path));
    } else {
        result = result.replace("{path}", "''");
    }

    if let Some(ref tool) = ctx.tool_name {
        result = result.replace("{tool}", &shell_quote(tool));
    } else {
        result = result.replace("{tool}", "''");
    }

    result
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

#[cfg(test)]
mod tests {
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
        let output = run_shell_command("echo hello").await.unwrap();
        assert!(output.success);
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_run_shell_command_failure() {
        let output = run_shell_command("false").await.unwrap();
        assert!(!output.success);
        assert_ne!(output.exit_code, 0);
    }

    #[tokio::test]
    async fn run_shell_command_does_not_leak_secret_env() {
        // A secret in the agent's environment must NOT reach a hook command.
        std::env::set_var("SELFWARE_HOOK_SECRET_TEST", "topsecret");
        let output = run_shell_command("printf %s \"${SELFWARE_HOOK_SECRET_TEST:-CLEARED}\"")
            .await
            .unwrap();
        std::env::remove_var("SELFWARE_HOOK_SECRET_TEST");
        assert_eq!(
            output.stdout.trim(),
            "CLEARED",
            "hook must not inherit secret env vars, got: {:?}",
            output.stdout
        );

        // PATH stays allowlisted so hook commands still resolve binaries.
        let path_out = run_shell_command("printf %s \"${PATH:+HASPATH}\"")
            .await
            .unwrap();
        assert_eq!(
            path_out.stdout.trim(),
            "HASPATH",
            "PATH must remain available to hooks"
        );
    }
}
