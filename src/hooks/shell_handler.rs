#![allow(dead_code, unused_imports, unused_variables)]
//! Shell-based hook execution.
//!
//! Runs hook commands via `sh -c` with placeholder substitution and timeout.

use anyhow::Result;
use std::time::Duration;
use tracing::{debug, info, warn};

use super::{HookAction, HookConfig, HookContext, SkipKind};

/// Maximum hook command output to capture (prevent unbounded memory).
const MAX_OUTPUT_BYTES: usize = 64 * 1024; // 64 KB

/// Execute a shell-based hook command.
///
/// Substitutes placeholders in the command string:
/// - `{path}` → affected file path (if any)
/// - `{tool}` → tool name (if any)
///
/// Outcomes:
/// - exit 0 → [`HookAction::Continue`]
/// - non-zero exit on `PreToolUse` → [`HookAction::Skip`] with
///   [`SkipKind::Policy`] (the hook decided to block the tool)
/// - timeout / failure to start on `PreToolUse` → [`HookAction::Skip`] with
///   [`SkipKind::HookFailure`]: the policy check never completed, so the tool
///   must not run (fail closed), but this is not a policy decision
/// - any failure on `PostToolUse` / `Stop` → [`HookAction::Error`] (non-fatal)
pub async fn execute_hook(hook: &HookConfig, ctx: &HookContext) -> HookAction {
    execute_hook_in_root(hook, ctx, &crate::tools::workspace_root::current()).await
}

/// [`execute_hook`] with an explicit workspace root: the hook command runs in
/// `root` (the agent's workspace — an entered worktree, not the process cwd),
/// exactly like the agent's tools do, so a formatter / test / auto-commit
/// hook acts on the checkout the agent is actually editing.
pub async fn execute_hook_in_root(
    hook: &HookConfig,
    ctx: &HookContext,
    root: &crate::tools::workspace_root::WorkspaceRoot,
) -> HookAction {
    let command = expand_placeholders(&hook.command, ctx);
    let is_pre_tool = ctx.event == super::HookEvent::PreToolUse;

    debug!(
        "Executing hook: {} (event: {}, timeout: {}s)",
        command, ctx.event, hook.timeout_secs
    );

    let timeout_secs = hook.timeout_secs.max(1);
    let timeout_duration = Duration::from_secs(timeout_secs);
    let result = run_shell_command_in(&command, timeout_duration, root).await;

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

                // PreToolUse hook non-zero exit is a policy decision: skip the tool.
                if is_pre_tool {
                    HookAction::Skip {
                        reason: msg,
                        kind: SkipKind::Policy,
                    }
                } else {
                    HookAction::Error { message: msg }
                }
            }
        }
        Ok(None) => {
            let detail = format!("timed out after {}s", timeout_secs);
            warn!("Hook '{}' {}", command, detail);
            failure_action(is_pre_tool, &command, detail)
        }
        Err(e) => {
            let detail = format!("failed to start: {}", e);
            warn!("Hook '{}' {}", command, detail);
            failure_action(is_pre_tool, &command, detail)
        }
    }
}

/// Map a hook that could not complete (timeout, spawn/wait failure) to an
/// action. PreToolUse fails closed with an infrastructure-failure kind;
/// other events stay non-fatal.
fn failure_action(is_pre_tool: bool, command: &str, detail: String) -> HookAction {
    if is_pre_tool {
        HookAction::Skip {
            reason: detail,
            kind: SkipKind::HookFailure,
        }
    } else {
        HookAction::Error {
            message: format!("Hook '{}' {}", command, detail),
        }
    }
}

/// Output from a shell command execution.
#[derive(Debug)]
struct ShellOutput {
    /// The hook command's own exit status was 0. This is the hook's decision;
    /// lingering descendants are reported separately in `killed_descendants`.
    success: bool,
    exit_code: i32,
    stdout: String,
    stderr: String,
    /// Descendants still held the output pipes after the hook exited and were
    /// killed (see [`crate::tools::process_guard::run_command_bounded`]).
    killed_descendants: bool,
}

/// Run a command via `sh -c` and capture output.
///
/// Uses the shared [`crate::tools::process_guard::run_command_bounded`] runner
/// (process-group isolation, capped output, bounded drains) so a hook that
/// exits while a background descendant keeps stdout/stderr open cannot hang
/// the agent past its timeout plus the drain grace period.
///
/// Returns `Ok(Some(output))` on normal completion, `Ok(None)` if the command
/// timed out (the whole process group has been killed in that case).
async fn run_shell_command(command: &str, timeout: Duration) -> Result<Option<ShellOutput>> {
    run_shell_command_in(command, timeout, &crate::tools::workspace_root::current()).await
}

/// [`run_shell_command`] started in `root` (see [`execute_hook_in_root`]).
async fn run_shell_command_in(
    command: &str,
    timeout: Duration,
    root: &crate::tools::workspace_root::WorkspaceRoot,
) -> Result<Option<ShellOutput>> {
    use crate::tools::process_guard::{run_command_bounded, CommandRunError};
    use crate::tools::workspace_root::CommandRootExt;

    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);
    // Hooks execute arbitrary repo-defined commands — do NOT hand them the
    // agent's full environment, which can carry API keys and other secrets.
    // Start from an empty environment and re-add the shared non-sensitive
    // allowlist (matches shell_exec / ProcessManager sanitization).
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.stdin(std::process::Stdio::null());
    // Run in the agent's workspace root (an entered worktree), not the
    // process cwd. `PWD` is not on the env allowlist, so `sh` derives it from
    // the real directory — no stale original-checkout PWD leaks through.
    cmd.in_root(root);

    match run_command_bounded(cmd, timeout, MAX_OUTPUT_BYTES).await {
        Ok(out) => {
            if out.killed_descendants {
                warn!(
                    "Hook '{}' exited but left descendant processes holding its output pipes; they were killed",
                    command
                );
            }
            Ok(Some(ShellOutput {
                success: out.status.success(),
                exit_code: out.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&out.stdout).to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).to_string(),
                killed_descendants: out.killed_descendants,
            }))
        }
        Err(CommandRunError::Timeout(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Replace `{path}`, `{tool}` placeholders in a hook command string.
fn expand_placeholders(command: &str, ctx: &HookContext) -> String {
    let mut result = command.to_string();

    // Every affected path, each quoted separately (a multi-edit or patch
    // touches several files; `rustfmt {path}` must see all of them).
    if !ctx.affected_paths.is_empty() {
        let quoted: Vec<String> = ctx.affected_paths.iter().map(|p| shell_quote(p)).collect();
        result = result.replace("{path}", &quoted.join(" "));
    } else if let Some(ref path) = ctx.affected_path {
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
