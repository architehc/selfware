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
    sanitize_command_env_preserve(cmd, &[]);
}

/// Like [`sanitize_command_env`], but additionally re-adds the named
/// variables from the parent environment when present.
///
/// Use this for spawn sites whose child tools legitimately need session
/// state to function — e.g. computer-control tools (xdotool, wmctrl, …)
/// need `DISPLAY`/`WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`/`XAUTHORITY`/
/// `SSH_AUTH_SOCK` to reach the user's display server. Never pass
/// credential-bearing names (`SELFWARE_API_KEY`, `AWS_*`, tokens) here.
pub fn sanitize_command_env_preserve(cmd: &mut tokio::process::Command, preserve: &[&str]) {
    cmd.env_clear();
    for (key, value) in kept_env(preserve) {
        cmd.env(key, value);
    }
}

/// [`sanitize_command_env_preserve`] for synchronous spawn sites built on
/// `std::process::Command` (e.g. backend probes that cannot `.await`).
pub fn sanitize_std_command_env_preserve(cmd: &mut std::process::Command, preserve: &[&str]) {
    cmd.env_clear();
    for (key, value) in kept_env(preserve) {
        cmd.env(key, value);
    }
}

/// The keep-list applied after `env_clear`: the minimal base plus any
/// caller-preserved names, resolved from the parent environment.
fn kept_env<'a>(preserve: &'a [&'a str]) -> Vec<(&'a str, std::ffi::OsString)> {
    ["PATH", "HOME", "LANG"]
        .into_iter()
        .chain(preserve.iter().copied())
        .filter_map(|key| std::env::var_os(key).map(|value| (key, value)))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/safety/process_env/process_env_test.rs"]
mod tests;
