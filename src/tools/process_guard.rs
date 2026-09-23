//! Process group and execution lifecycle guards for external tools.
//!
//! Provides RAII guards to ensure all descendant processes in a process group
//! are reaped on future drop, timeout, or cancellation, preventing orphaned
//! background processes from lingering under PID 1.
//! Also provides bounded output drains to prevent unbounded memory consumption
//! when commands produce high volumes of stdout/stderr.

use std::time::Duration;

/// An RAII guard that manages the process group of a spawned child process.
///
/// When spawned in its own process group (`cmd.process_group(0)` on Unix), the child's
/// PID is the process group ID (PGID). If the outer future is dropped (cancellation,
/// agent-level step timeout, panic), this guard drops while still armed and sends
/// `SIGKILL` to the entire process group.
///
/// On normal completion, the caller must call [`ProcessGroupGuard::disarm`].
pub struct ProcessGroupGuard {
    pgid: Option<u32>,
    armed: bool,
}

impl ProcessGroupGuard {
    pub fn new(pgid: Option<u32>) -> Self {
        Self { pgid, armed: true }
    }

    /// Disarm the guard when the process and its drains have completed cleanly.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// Explicitly send SIGKILL to the entire process group now and disarm.
    pub fn kill(&mut self) {
        if !self.armed {
            return;
        }
        self.kill_pg();
        self.armed = false;
    }

    fn kill_pg(&self) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            // Guard against pgid 0, which would signal the caller's own process group.
            if pgid > 0 {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(pgid as i32), Signal::SIGKILL);
            }
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.armed {
            self.kill_pg();
        }
    }
}

/// Read an async reader (stdout/stderr pipe) up to `max_bytes`, continuing to consume
/// and discard remaining bytes until EOF so the child process never deadlocks on a full pipe.
pub async fn drain_capped<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    max_bytes: usize,
) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < max_bytes {
                    let take = n.min(max_bytes - buf.len());
                    buf.extend_from_slice(&chunk[..take]);
                }
                // Discard excess output to keep pipe drained without unbounded RAM
            }
        }
    }
    buf
}

/// Output captured from a bounded command run.
#[derive(Debug, Clone)]
pub struct BoundedCommandOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub status: std::process::ExitStatus,
    /// Whether lingering descendant processes had to be killed after the parent process exited.
    pub killed_descendants: bool,
}

impl BoundedCommandOutput {
    /// Return true if the process exited with code 0 AND no descendant processes had to be killed.
    pub fn success(&self) -> bool {
        !self.killed_descendants && self.status.success()
    }

    /// Return the exit code. If descendants were forcibly killed, return Some(-1).
    pub fn exit_code(&self) -> Option<i32> {
        if self.killed_descendants {
            Some(-1)
        } else {
            self.status.code()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CommandRunError {
    #[error("Failed to spawn command: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Command timed out after {0:?}")]
    Timeout(Duration),
    #[error("IO error waiting for command: {0}")]
    Io(#[source] std::io::Error),
}

/// Grace period for the output drains after the direct child has exited.
pub const DRAIN_GRACE: Duration = Duration::from_secs(1);
/// Final wait for the output drains after the process group was killed.
pub const DRAIN_AFTER_KILL: Duration = Duration::from_millis(500);

type DrainSlot<'a> = (
    &'a mut tokio::task::JoinHandle<Vec<u8>>,
    &'a mut Option<Vec<u8>>,
);

/// Poll both drain tasks concurrently until `deadline`, storing each finished
/// task's output in its slot. A handle whose slot is already filled is never
/// polled again; a handle that is still pending at the deadline is left
/// unpolled-to-completion and is safe to poll again later.
async fn collect_drains_until(
    deadline: tokio::time::Instant,
    stdout: DrainSlot<'_>,
    stderr: DrainSlot<'_>,
) {
    async fn one(deadline: tokio::time::Instant, (task, slot): DrainSlot<'_>) {
        if slot.is_some() {
            return;
        }
        if let Ok(res) = tokio::time::timeout_at(deadline, &mut *task).await {
            // A JoinError (drain panicked/cancelled) still counts as finished:
            // the handle is complete and must not be polled again.
            *slot = Some(res.unwrap_or_default());
        }
    }
    tokio::join!(one(deadline, stdout), one(deadline, stderr));
}

/// Execute a command with bounded output memory, process-group isolation,
/// and timeout enforcement.
///
/// Guarantees:
/// 1. The command runs in its own process group (Unix).
/// 2. If the returned future is dropped or cancelled, the entire process group
///    (including grandchildren) is killed with SIGKILL via [`ProcessGroupGuard`].
/// 3. Stdout and stderr are drained concurrently into buffers capped at `max_output_bytes`.
///    Remaining output is drained and discarded to prevent pipe-buffer deadlocks without OOM.
/// 4. If the timeout expires, the whole process group is reaped and `CommandRunError::Timeout` is returned.
pub async fn run_command_bounded(
    mut cmd: tokio::process::Command,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<BoundedCommandOutput, CommandRunError> {
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(CommandRunError::Spawn)?;
    let child_pid = child.id();
    let mut pg_guard = ProcessGroupGuard::new(child_pid);

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let mut stdout_task = tokio::spawn(async move {
        match stdout_pipe {
            Some(s) => drain_capped(s, max_output_bytes).await,
            None => Vec::new(),
        }
    });
    let mut stderr_task = tokio::spawn(async move {
        match stderr_pipe {
            Some(s) => drain_capped(s, max_output_bytes).await,
            None => Vec::new(),
        }
    });

    let wait_res = tokio::time::timeout(timeout, child.wait()).await;
    let status = match wait_res {
        Ok(Ok(st)) => st,
        Ok(Err(e)) => {
            pg_guard.kill();
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(CommandRunError::Io(e));
        }
        Err(_) => {
            pg_guard.kill();
            let _ = child.kill().await;
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(CommandRunError::Timeout(timeout));
        }
    };

    // Await the drains with a short grace period so a backgrounded
    // descendant holding the pipes open cannot block the caller indefinitely.
    //
    // Each drain has its own result slot and a handle is only polled while
    // its slot is empty: a JoinHandle that completed during the grace period
    // must never be polled again (Tokio panics "JoinHandle polled after
    // completion"), and its output must survive into the result.
    let mut stdout_slot: Option<Vec<u8>> = None;
    let mut stderr_slot: Option<Vec<u8>> = None;
    collect_drains_until(
        tokio::time::Instant::now() + DRAIN_GRACE,
        (&mut stdout_task, &mut stdout_slot),
        (&mut stderr_task, &mut stderr_slot),
    )
    .await;

    let killed_descendants = stdout_slot.is_none() || stderr_slot.is_none();
    if killed_descendants {
        // Descendants are still running and holding a pipe open. Kill the
        // process group; with it gone, the pipe write ends close and the
        // remaining drain(s) reach EOF.
        pg_guard.kill();
        collect_drains_until(
            tokio::time::Instant::now() + DRAIN_AFTER_KILL,
            (&mut stdout_task, &mut stdout_slot),
            (&mut stderr_task, &mut stderr_slot),
        )
        .await;
        // A descendant that escaped the process group (e.g. setsid) can still
        // hold a pipe: abort only the drains that never finished.
        if stdout_slot.is_none() {
            stdout_task.abort();
        }
        if stderr_slot.is_none() {
            stderr_task.abort();
        }
    }
    let stdout_bytes = stdout_slot.unwrap_or_default();
    let mut stderr_bytes = stderr_slot.unwrap_or_default();

    pg_guard.disarm();

    if killed_descendants {
        if !stderr_bytes.is_empty() && !stderr_bytes.ends_with(b"\n") {
            stderr_bytes.push(b'\n');
        }
        stderr_bytes.extend_from_slice(
            format!(
                "[process_guard] Process group had lingering descendant processes that were killed after {:?} grace period.\n",
                DRAIN_GRACE
            )
            .as_bytes(),
        );
    }

    Ok(BoundedCommandOutput {
        stdout: stdout_bytes,
        stderr: stderr_bytes,
        status,
        killed_descendants,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_run_command_bounded_success() {
        let mut cmd = tokio::process::Command::new("echo");
        cmd.arg("hello world");

        let output = run_command_bounded(cmd, Duration::from_secs(5), 10_000)
            .await
            .expect("echo should succeed");

        assert!(output.success());
        assert_eq!(output.exit_code(), Some(0));
        assert!(!output.killed_descendants);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "hello world"
        );
    }

    #[tokio::test]
    async fn test_run_command_bounded_kills_lingering_descendants_and_reports_failure() {
        // Parent prints output and exits immediately, but backgrounds a sleep 30 descendant
        // that holds stdout/stderr pipes open.
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg("(sleep 30 &); echo 'parent output'");

        let output = run_command_bounded(cmd, Duration::from_secs(5), 10_000)
            .await
            .expect("command should run and terminate descendants");

        // The 1s grace period should elapse, descendants killed, output collected.
        assert!(
            output.killed_descendants,
            "lingering sleep 30 must be marked as killed descendants"
        );
        assert!(
            !output.success(),
            "killed descendants must not report success: true"
        );
        assert_eq!(
            output.exit_code(),
            Some(-1),
            "killed descendants must report exit code -1"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("parent output"),
            "stdout output produced by parent before exit must be preserved"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("[process_guard] Process group had lingering descendant"),
            "stderr must contain diagnostic message"
        );
    }

    /// Regression: the first drain wait used to time out after ONE pipe had
    /// already hit EOF (its JoinHandle completed inside `join!`), and the retry
    /// `join!` re-polled that completed JoinHandle -> Tokio panic
    /// "JoinHandle polled after completion", losing the captured output.
    #[tokio::test]
    #[cfg(unix)]
    async fn test_run_command_bounded_one_pipe_closed_other_held_does_not_panic() {
        // stdout closes as soon as the shell exits (nothing else holds it), but
        // the backgrounded `sleep` inherits stderr and keeps it open.
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg("echo stdout-before-close; exec 1>&-; sleep 5 & echo stderr-line >&2");

        let started = std::time::Instant::now();
        let output = run_command_bounded(cmd, Duration::from_secs(10), 10_000)
            .await
            .expect("command should run and terminate descendants");

        assert!(
            started.elapsed() < Duration::from_secs(4),
            "bounded drain must not wait for the 5s descendant, took {:?}",
            started.elapsed()
        );
        assert!(
            output.killed_descendants,
            "lingering sleep holding stderr must be reported as killed descendants"
        );
        assert!(!output.success());
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("stdout-before-close"),
            "stdout captured before the first grace period expired must be preserved, got {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("stderr-line"),
            "stderr drained after the process-group kill must be preserved, got {stderr:?}"
        );
        assert!(stderr.contains("[process_guard] Process group had lingering descendant"));
    }
}
