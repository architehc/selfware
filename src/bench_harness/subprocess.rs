//! Shared subprocess launcher for the benchmark harnesses.
//!
//! Both the SWE-bench-Pro harness and the long-running system-test harness
//! spawn the `selfware` binary (or another agent command) as a child process
//! and must (a) run it with a *scrubbed* environment so the operator's
//! unrelated secrets aren't handed to arbitrary agent-invoked tools, and
//! (b) kill the *whole process group* on a wall-clock timeout so a hung agent
//! doesn't leave orphaned `git`/`cargo`/tool/server processes behind.
//!
//! This module centralizes both concerns so the two harnesses can't drift.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use wait_timeout::ChildExt;

/// Whether an environment variable is safe to forward to a spawned agent.
///
/// The agent needs its toolchain (PATH/HOME/CARGO_HOME/…) and its model
/// credentials, but not the operator's every other secret. `SELFWARE_*` and
/// `LC_*` pass through; everything else must be on the explicit allowlist.
pub fn env_allowed(key: &str) -> bool {
    if key.starts_with("SELFWARE_") || key.starts_with("LC_") {
        return true;
    }
    matches!(
        key,
        "PATH"
            | "HOME"
            | "USER"
            | "LOGNAME"
            | "SHELL"
            | "LANG"
            | "TERM"
            | "TMPDIR"
            | "TZ"
            | "CARGO_HOME"
            | "RUSTUP_HOME"
            | "RUST_BACKTRACE"
            | "GOPATH"
            | "GOROOT"
            | "PYENV_ROOT"
            | "NODE_PATH"
            | "JAVA_HOME"
            | "OPENROUTER_API_KEY"
            | "LLM_API_KEY"
            | "LLAMA_SERVER_BIN"
            | "SWEBENCH_MODELS_DIR"
    )
}

/// Scrub `cmd`'s environment down to the [`env_allowed`] allowlist: clear the
/// inherited environment, then re-add only the permitted vars from the current
/// process. Callers may still layer bench-specific `.env()` calls afterwards.
pub fn apply_env_allowlist(cmd: &mut Command) {
    cmd.env_clear();
    for (k, v) in std::env::vars() {
        if env_allowed(&k) {
            cmd.env(k, v);
        }
    }
}

/// Outcome of a group-timeout subprocess run.
#[derive(Debug, Clone)]
pub struct ProcOutcome {
    /// Captured stdout of the child.
    pub stdout: String,
    /// Exit code (or -1 if killed / no code).
    pub exit_code: i32,
    /// Whether the wall-clock timeout fired and the group was killed.
    pub timed_out: bool,
    /// Wall-clock duration in seconds.
    pub wall_secs: f64,
}

/// Spawn `cmd` in its own process group, capture stdout, and enforce a
/// wall-clock `timeout`. On timeout, the entire process group is killed
/// (SIGTERM, 500 ms grace, SIGKILL) so children the agent spawned — `git`,
/// `cargo`, tool servers — die too rather than orphaning.
///
/// The caller owns argument, cwd, env, and stderr/stdin wiring on `cmd`;
/// this helper forces `stdout` to a pipe (it captures it) and sets
/// `process_group(0)` on Unix. It does **not** parse stdout — callers
/// interpret [`ProcOutcome::stdout`] however they need.
pub fn run_with_group_timeout(mut cmd: Command, timeout: Duration) -> Result<ProcOutcome> {
    cmd.stdout(Stdio::piped());

    // New process group so a timeout can kill the agent AND everything it
    // spawned, not just the direct child. The child becomes the group leader,
    // so its pgid == its pid.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let started = Instant::now();
    let mut child = cmd.spawn().context("spawning subprocess")?;

    // Drain stdout on a helper thread so a chatty child can't deadlock on a
    // full pipe while we poll for exit.
    let stdout = child.stdout.take().expect("stdout piped above");
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::BufReader::new(stdout), &mut buf);
        buf
    });

    let deadline = started + timeout;
    let poll_interval = Duration::from_millis(500);
    loop {
        match child.wait_timeout(poll_interval)? {
            Some(status) => {
                let stdout = stdout_handle.join().unwrap_or_default();
                return Ok(ProcOutcome {
                    stdout,
                    exit_code: status.code().unwrap_or(-1),
                    timed_out: false,
                    wall_secs: started.elapsed().as_secs_f64(),
                });
            }
            None => {
                if Instant::now() >= deadline {
                    kill_group(&mut child);
                    let _ = child.wait();
                    let stdout = stdout_handle.join().unwrap_or_default();
                    return Ok(ProcOutcome {
                        stdout,
                        exit_code: -1,
                        timed_out: true,
                        wall_secs: started.elapsed().as_secs_f64(),
                    });
                }
            }
        }
    }
}

/// Kill the child's whole process group (Unix) or just the child (elsewhere).
fn kill_group(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        let pgid = Pid::from_raw(child.id() as i32);
        let _ = killpg(pgid, Signal::SIGTERM);
        std::thread::sleep(Duration::from_millis(500));
        let _ = killpg(pgid, Signal::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_allowed_passes_selfware_and_toolchain_but_not_random_secrets() {
        assert!(env_allowed("SELFWARE_ENDPOINT"));
        assert!(env_allowed("LC_ALL"));
        assert!(env_allowed("PATH"));
        assert!(env_allowed("CARGO_HOME"));
        assert!(env_allowed("OPENROUTER_API_KEY"));
        assert!(!env_allowed("AWS_SECRET_ACCESS_KEY"));
        assert!(!env_allowed("GITHUB_TOKEN"));
        assert!(!env_allowed("HOME_ADDRESS"));
    }

    #[cfg(unix)]
    #[test]
    fn run_captures_stdout_and_exit_code() {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("printf hello; exit 3");
        let out = run_with_group_timeout(cmd, Duration::from_secs(10)).unwrap();
        assert_eq!(out.stdout, "hello");
        assert_eq!(out.exit_code, 3);
        assert!(!out.timed_out);
    }

    #[cfg(unix)]
    #[test]
    fn run_times_out_and_kills_the_group() {
        // A child that spawns a grandchild and both sleep; on timeout the whole
        // group must die. We assert timed_out and that the call returns promptly
        // (≈ timeout + 500ms grace), not after the 30s sleep.
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg("sleep 30 & sleep 30");
        let started = Instant::now();
        let out = run_with_group_timeout(cmd, Duration::from_millis(300)).unwrap();
        assert!(out.timed_out, "must report timeout");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must return shortly after timeout, not wait for the 30s sleep (took {:?})",
            started.elapsed()
        );
    }
}
