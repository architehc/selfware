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

    // Await the drains with a short grace period (1s) so a backgrounded
    // descendant holding the pipes open cannot block the caller indefinitely.
    let drain_res = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::join!(&mut stdout_task, &mut stderr_task)
    })
    .await;

    let (stdout_bytes, stderr_bytes) = match drain_res {
        Ok((out, err)) => (out.unwrap_or_default(), err.unwrap_or_default()),
        Err(_) => {
            pg_guard.kill();
            stdout_task.abort();
            stderr_task.abort();
            (Vec::new(), Vec::new())
        }
    };

    pg_guard.disarm();

    Ok(BoundedCommandOutput {
        stdout: stdout_bytes,
        stderr: stderr_bytes,
        status,
    })
}
