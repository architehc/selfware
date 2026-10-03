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
///
/// IMPORTANT: this helper drains **only stdout**. The caller MUST NOT set
/// `stderr` to an unread pipe (`Stdio::piped()`) — nothing drains it, so a
/// chatty child fills the pipe buffer and blocks, then gets killed and falsely
/// reported as timed out. Send stderr to a file, `Stdio::null()`, or inherit.
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
    // full pipe while we poll for exit. Send chunks back as they arrive: a
    // background descendant can inherit the pipe after the leader exits, and
    // a plain thread `join` would otherwise wait past the wall-clock timeout.
    let stdout = child.stdout.take().expect("stdout piped above");
    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
    let stdout_handle = std::thread::spawn(move || {
        use std::io::Read;
        let mut reader = std::io::BufReader::new(stdout);
        let mut chunk = [0_u8; 8192];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    let _ = stdout_tx.send(None);
                    break;
                }
                Ok(n) => {
                    if stdout_tx.send(Some(chunk[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let deadline = started + timeout;
    let poll_interval = Duration::from_millis(50);
    let mut stdout_bytes = Vec::new();
    let mut stdout_done = false;
    #[cfg(unix)]
    let status_before_reap: Option<std::process::ExitStatus> = None;
    #[cfg(not(unix))]
    let mut status_before_reap = None;
    let mut child_exited = false;
    loop {
        while !stdout_done {
            match stdout_rx.try_recv() {
                Ok(Some(chunk)) => stdout_bytes.extend_from_slice(&chunk),
                Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    stdout_done = true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
            }
        }

        if !child_exited {
            #[cfg(unix)]
            {
                child_exited = match crate::tools::process_guard::process_has_exited_without_reaping(
                    child.id(),
                ) {
                    Ok(exited) => exited,
                    Err(error) => {
                        // If the leader is still ours, its id safely pins the
                        // process group for cleanup. If it was already reaped
                        // elsewhere, never signal the retained numeric pgid.
                        match child.try_wait() {
                            Ok(None) => {
                                kill_group(&mut child);
                                let _ = child.wait();
                            }
                            Ok(Some(_)) | Err(_) => {
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                        }
                        return Err(error).context("observing subprocess exit");
                    }
                };
            }
            #[cfg(not(unix))]
            if let Some(status) = child.try_wait()? {
                status_before_reap = Some(status);
                child_exited = true;
            }
        }

        if child_exited && stdout_done {
            // On Unix the leader is still an unreaped zombie here, so no
            // stored pgid survives this final wait. Any descendant that closed
            // stdout is also stopped before the group identity is released.
            force_kill_group_while_unreaped(&child);
            let status = match status_before_reap {
                Some(status) => status,
                None => child.wait()?,
            };
            let _ = stdout_handle.join();
            return Ok(ProcOutcome {
                stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                exit_code: status.code().unwrap_or(-1),
                timed_out: false,
                wall_secs: started.elapsed().as_secs_f64(),
            });
        }

        let now = Instant::now();
        if now >= deadline {
            // Unix has deliberately not reaped the leader, so its pid still
            // pins the pgid through both signals below.
            kill_group(&mut child);
            if status_before_reap.is_none() {
                let _ = child.wait();
            }

            // Group termination normally closes stdout immediately. Bound the
            // final drain too: a setsid descendant can escape the group while
            // retaining the pipe and must not hang the benchmark harness.
            let drain_deadline = Instant::now() + Duration::from_secs(1);
            while !stdout_done && Instant::now() < drain_deadline {
                let remaining = drain_deadline.saturating_duration_since(Instant::now());
                match stdout_rx.recv_timeout(remaining.min(poll_interval)) {
                    Ok(Some(chunk)) => stdout_bytes.extend_from_slice(&chunk),
                    Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        stdout_done = true;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            if stdout_done {
                let _ = stdout_handle.join();
            }
            return Ok(ProcOutcome {
                stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                exit_code: -1,
                timed_out: true,
                wall_secs: started.elapsed().as_secs_f64(),
            });
        }

        if !stdout_done {
            let wait = deadline.saturating_duration_since(now).min(poll_interval);
            match stdout_rx.recv_timeout(wait) {
                Ok(Some(chunk)) => stdout_bytes.extend_from_slice(&chunk),
                Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    stdout_done = true;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }
        } else {
            std::thread::sleep(deadline.saturating_duration_since(now).min(poll_interval));
        }
    }
}

/// Kill the group only while its leader is known to be unreaped. This is used
/// on normal completion to stop descendants that detached their stdio before
/// the leader exited.
fn force_kill_group_while_unreaped(child: &std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        if let Ok(pid) = i32::try_from(child.id()) {
            if pid > 1 {
                let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = child;
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
#[path = "../../tests/unit/bench_harness/subprocess/subprocess_test.rs"]
mod tests;
