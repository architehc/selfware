#![allow(dead_code, unused_imports, unused_variables)]
//! Shell-based hook execution.
//!
//! Runs hook commands via `sh -c` with placeholder substitution and timeout.

use anyhow::Result;
use std::time::Duration;
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

    let timeout_duration = Duration::from_secs(hook.timeout_secs.max(1));
    let result = run_shell_command(&command, timeout_duration).await;

    match result {
        Ok(Some(output)) => {
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
        Ok(None) => {
            let msg = format!("Hook '{}' timed out after {}s", command, hook.timeout_secs);
            warn!("{}", msg);
            if ctx.event == super::HookEvent::PreToolUse {
                HookAction::Skip { reason: msg }
            } else {
                HookAction::Error { message: msg }
            }
        }
        Err(e) => {
            let msg = format!("Hook '{}' failed to run: {}", command, e);
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
#[derive(Debug)]
struct ShellOutput {
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

/// Read a child pipe to EOF, keeping at most `MAX_OUTPUT_BYTES` in memory while
/// still consuming the rest so the process isn't blocked on a full pipe.
async fn drain_capped<R: tokio::io::AsyncRead + Unpin>(mut reader: R) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_OUTPUT_BYTES {
                    let take = n.min(MAX_OUTPUT_BYTES - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
                // Beyond the cap: keep reading to drain the pipe, discard excess.
            }
        }
    }
    buf
}

/// Run a command via `sh -c` and capture output.
///
/// Returns `Ok(Some(output))` on normal completion, `Ok(None)` if the command
/// timed out (the whole process group has been killed in that case).
async fn run_shell_command(command: &str, timeout: Duration) -> Result<Option<ShellOutput>> {
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

    // Kill the child if the future is dropped (defense in depth; the timeout
    // path also explicitly reaps the process group).
    cmd.kill_on_drop(true);
    // Run the child in its own process group so a timeout can reap the ENTIRE
    // process tree (grandchildren included, e.g. a `sleep` or build), not just
    // the direct shell — `kill_on_drop`/`child.kill()` only signal the
    // immediate child, so descendants would otherwise orphan.
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let child_pid = child.id();

    // Drain stdout/stderr concurrently (bounded) so a chatty hook can't
    // deadlock on a full pipe or OOM the agent with unbounded output.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_task = tokio::spawn(async move {
        match stdout_pipe {
            Some(s) => drain_capped(s).await,
            None => Vec::new(),
        }
    });
    let stderr_task = tokio::spawn(async move {
        match stderr_pipe {
            Some(s) => drain_capped(s).await,
            None => Vec::new(),
        }
    });

    let wait_result = tokio::time::timeout(timeout, child.wait()).await;

    match wait_result {
        Ok(Ok(status)) => {
            let stdout_bytes = stdout_task.await.unwrap_or_default();
            let stderr_bytes = stderr_task.await.unwrap_or_default();
            let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
            let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
            Ok(Some(ShellOutput {
                success: status.success(),
                exit_code: status.code().unwrap_or(-1),
                stdout,
                stderr,
            }))
        }
        Ok(Err(e)) => {
            // Make sure the drain tasks don't leak.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            Err(e.into())
        }
        Err(_) => {
            // Timed out: kill the whole process group, then reap the child.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            // Always await the drain tasks so they don't leak; the pipes close
            // once the process (and its group) exit.
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            Ok(None)
        }
    }
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
#[path = "../../tests/unit/hooks/shell_handler/shell_handler_test.rs"]
mod tests;
