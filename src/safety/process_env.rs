//! Environment sanitization for spawned child processes.
//!
//! The agent process holds secrets in its environment — `SELFWARE_API_KEY`,
//! and whatever the operator exported (`AWS_*`, `GITHUB_TOKEN`, …). Any child
//! process spawned with the inherited environment can read all of it, so a
//! single compromised subprocess (a hostile MCP server, an injected shell
//! command, the Playwright bridge loading attacker-controlled content) can
//! exfiltrate every credential on the box.
//!
//! `shell_exec` already clears the environment and re-adds a minimal base.
//! This helper centralizes that policy so every spawn site applies the same
//! allowlist and none silently drifts back to inheriting the full env.

/// Clear a command's inherited environment and re-populate only a minimal,
/// non-sensitive base (`PATH`, `HOME`, `LANG`).
///
/// Call this immediately after constructing the `Command` and BEFORE adding
/// any task-specific variables, so those additions survive the clear.
pub fn sanitize_command_env(cmd: &mut tokio::process::Command) {
    cmd.env_clear();
    for key in ["PATH", "HOME", "LANG"] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/safety/process_env/process_env_test.rs"]
mod tests;
