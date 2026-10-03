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

/// Run `cmd` to completion like [`tokio::process::Command::output`] (stdin
/// null, stdout/stderr captured), but spawned in its own process group with a
/// [`ProcessGroupGuard`] held across the wait.
///
/// `kill_on_drop` alone signals only the direct child: a dropped future (a
/// tool timeout, Ctrl-C cancel via `run_tool_bounded`, an outer
/// `tokio::time::timeout`) killed `git` but left `git-remote-https`/`ssh`
/// pushing, or `cargo` but left `rustc` compiling. Here, if the returned future
/// is dropped before the child finished, or the wait itself fails, the whole
/// group is SIGKILLed.
///
/// Wrap it in `tokio::time::timeout` for a deadline: the timeout drops this
/// future, which kills the group.
///
/// Windows: no process groups here — `kill_on_drop` kills the direct child
/// only (no job object is set up); descendants can outlive a cancel.
///
/// The child is taken out of the terminal's foreground process group, so a
/// terminal Ctrl-C reaches selfware (which cancels and kills the group), not the
/// child directly. Do not use it for interactive children the user drives from
/// the terminal (pagers, `!cmd` passthrough).
pub async fn output_in_process_group(
    cmd: &mut tokio::process::Command,
) -> std::io::Result<std::process::Output> {
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.kill_on_drop(true);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn()?;
    let mut guard = ProcessGroupGuard::new(child.id());
    // Keep the leader unreaped until inherited output pipes have reached EOF.
    // Otherwise a descendant can hold a pipe open after the leader was reaped,
    // and an outer timeout can drop the still-armed guard after that numeric
    // pgid has already been reused by an unrelated process group.
    let output = wait_with_output_without_reaping(&mut child).await?;
    guard.disarm();
    Ok(output)
}

/// Method form of [`output_in_process_group`] for builder chains:
/// `Command::new("git").args([..]).output_grouped().await`.
pub trait GroupedOutputExt {
    /// See [`output_in_process_group`].
    fn output_grouped(
        &mut self,
    ) -> impl std::future::Future<Output = std::io::Result<std::process::Output>> + Send + '_;
}

impl GroupedOutputExt for tokio::process::Command {
    fn output_grouped(
        &mut self,
    ) -> impl std::future::Future<Output = std::io::Result<std::process::Output>> + Send + '_ {
        output_in_process_group(self)
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

pub(crate) type DrainSlot<'a> = (
    &'a mut tokio::task::JoinHandle<Vec<u8>>,
    &'a mut Option<Vec<u8>>,
);

/// Poll both drain tasks concurrently until `deadline`, storing each finished
/// task's output in its slot. A handle whose slot is already filled is never
/// polled again; a handle that is still pending at the deadline is left
/// unpolled-to-completion and is safe to poll again later.
pub(crate) async fn collect_drains_until(
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

/// Wait until a child has exited while keeping its process id reserved on
/// Unix. The returned status is `None` there because the child deliberately
/// remains an unreaped zombie; callers must reap it with [`tokio::process::Child::wait`]
/// after they have finished any process-group cleanup. On other platforms the
/// child is reaped here and its status is returned.
///
/// Keeping the leader unreaped matters when a descendant inherits a captured
/// stdout/stderr pipe. If the leader were reaped before a bounded drain wait,
/// its pid could be reused and a later `killpg(stored_pid)` could signal an
/// unrelated process group.
pub(crate) async fn wait_for_exit_without_reaping(
    child: &mut tokio::process::Child,
) -> std::io::Result<Option<std::process::ExitStatus>> {
    #[cfg(unix)]
    {
        let pid = child.id().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "cannot observe an already-reaped child",
            )
        })?;
        loop {
            if process_has_exited_without_reaping(pid)? {
                return Ok(None);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    #[cfg(not(unix))]
    {
        child.wait().await.map(Some)
    }
}

/// Observe a mutex-owned long-lived child exiting without holding its mutex
/// for the whole lifetime of the process. On Unix the normal path uses
/// `WNOWAIT`, preserving the leader until its owner can tear down descendants;
/// if that probe fails, `try_wait` either proves the child is still live or
/// reaps it before this function reports completion.
pub(crate) async fn wait_for_locked_child_exit(child: &tokio::sync::Mutex<tokio::process::Child>) {
    loop {
        let exited = {
            let mut child = child.lock().await;
            #[cfg(unix)]
            {
                match child.id() {
                    None => true,
                    Some(pid) => match process_has_exited_without_reaping(pid) {
                        Ok(exited) => exited,
                        Err(_) => matches!(child.try_wait(), Ok(Some(_))),
                    },
                }
            }
            #[cfg(not(unix))]
            {
                matches!(child.try_wait(), Ok(Some(_)))
            }
        };
        if exited {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Collect a child's piped stdout/stderr while keeping its process-group
/// leader unreaped until both pipes reach EOF.
///
/// This has the same successful result as `Child::wait_with_output`, but is
/// safe to use while a [`ProcessGroupGuard`] retains the child's pid. Tokio's
/// helper may reap the direct child before a background descendant closes an
/// inherited pipe; cancellation during that interval would otherwise let the
/// guard signal a recycled process-group id.
pub(crate) async fn wait_with_output_without_reaping(
    child: &mut tokio::process::Child,
) -> std::io::Result<std::process::Output> {
    async fn read_all<R: tokio::io::AsyncRead + Unpin>(
        pipe: Option<R>,
    ) -> std::io::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut bytes).await?;
        }
        Ok(bytes)
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (status_before_reap, stdout, stderr) = tokio::try_join!(
        wait_for_exit_without_reaping(child),
        read_all(stdout),
        read_all(stderr),
    )?;
    let status = match status_before_reap {
        Some(status) => status,
        None => child.wait().await?,
    };
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// One nonblocking `waitid(WNOWAIT)` probe. Keeping `siginfo_t` inside this
/// synchronous helper also ensures the async waiter remains `Send`: several
/// libc targets represent siginfo with raw pointers.
#[cfg(unix)]
pub(crate) fn process_has_exited_without_reaping(pid: u32) -> std::io::Result<bool> {
    loop {
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                pid as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == 0 {
            return Ok(unsafe { info.si_pid() } != 0);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
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

    let wait_res = tokio::time::timeout(timeout, wait_for_exit_without_reaping(&mut child)).await;
    let status_before_drain = match wait_res {
        Ok(Ok(status)) => status,
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

    // On Unix the leader stayed unreaped until every possible `killpg` above
    // was complete, so its process-group identity could not be recycled. Other
    // platforms already returned the reaped status from the wait helper.
    let status = match status_before_drain {
        Some(status) => status,
        None => child.wait().await.map_err(CommandRunError::Io)?,
    };

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
pub(crate) mod tests {
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

    /// Test support: a `sh` stub that records its own pid, forks a
    /// `sleep 60` grandchild, records that pid too, and waits. Returns the
    /// stub path and the two pid files.
    #[cfg(unix)]
    pub(crate) fn forking_stub(
        dir: &std::path::Path,
        name: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let pidfile = dir.join(format!("{name}.pid"));
        let sleep_pidfile = dir.join(format!("{name}.sleep.pid"));
        let stub = dir.join(name);
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nsleep 60 &\necho $! > '{}'\nwait\n",
                pidfile.display(),
                sleep_pidfile.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        (stub, pidfile, sleep_pidfile)
    }

    /// Test support: poll a pid file until it holds a pid or `deadline`.
    #[cfg(unix)]
    pub(crate) async fn wait_for_pidfile(
        path: &std::path::Path,
        deadline: std::time::Instant,
    ) -> Option<i32> {
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
            {
                return Some(pid);
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Test support: true while `pid` exists and is not a zombie.
    #[cfg(unix)]
    pub(crate) fn pid_is_running(pid: i32) -> bool {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        if kill(Pid::from_raw(pid), None).is_err() {
            return false;
        }
        // A killed grandchild is reparented to init and reaped promptly; a
        // zombie still answers kill(0). `ps -o stat=` works on macOS and Linux.
        let stat = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default();
        !(stat.is_empty() || stat.starts_with('Z'))
    }

    /// Test support: wait up to ~15s for every pid to be gone.
    #[cfg(unix)]
    pub(crate) async fn all_gone(pids: &[i32]) -> bool {
        for _ in 0..150 {
            if pids.iter().all(|p| !pid_is_running(*p)) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        false
    }

    /// Drive `fut` until the stub recorded both pids, then return them with
    /// the still-pending future (panics if it finished first).
    #[cfg(unix)]
    pub(crate) async fn pids_while_running<F: std::future::Future + Unpin>(
        fut: &mut F,
        pidfile: &std::path::Path,
        sleep_pidfile: &std::path::Path,
    ) -> (i32, i32) {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let pids = async {
            (
                wait_for_pidfile(pidfile, deadline).await,
                wait_for_pidfile(sleep_pidfile, deadline).await,
            )
        };
        let (a, b) = tokio::select! {
            pids = pids => pids,
            _ = fut => panic!("command finished before it could be cancelled"),
        };
        (
            a.expect("stub wrote its pid"),
            b.expect("stub wrote the grandchild pid"),
        )
    }

    /// Cancel (future dropped mid-wait, as `run_tool_bounded` does on
    /// Ctrl-C) must kill the grandchild, not only the direct child.
    #[tokio::test]
    #[cfg(unix)]
    async fn output_in_process_group_drop_kills_grandchild() {
        let dir = tempfile::tempdir().unwrap();
        let (stub, pidfile, sleep_pidfile) = forking_stub(dir.path(), "stub");
        let mut cmd = tokio::process::Command::new(&stub);
        let mut fut = Box::pin(output_in_process_group(&mut cmd));
        let (child, grandchild) = pids_while_running(&mut fut, &pidfile, &sleep_pidfile).await;
        drop(fut);
        assert!(
            all_gone(&[child, grandchild]).await,
            "dropped future must kill child {child} and grandchild {grandchild}"
        );
    }

    /// A `tokio::time::timeout` around the helper drops it at the deadline:
    /// the whole group dies.
    #[tokio::test]
    #[cfg(unix)]
    async fn output_in_process_group_timeout_kills_grandchild() {
        let dir = tempfile::tempdir().unwrap();
        let (stub, pidfile, sleep_pidfile) = forking_stub(dir.path(), "stub");
        let mut cmd = tokio::process::Command::new(&stub);
        let res =
            tokio::time::timeout(Duration::from_secs(3), output_in_process_group(&mut cmd)).await;
        assert!(res.is_err(), "hung stub must time out");
        let deadline = std::time::Instant::now() + Duration::from_secs(1);
        let child = wait_for_pidfile(&pidfile, deadline).await.expect("pid");
        let grandchild = wait_for_pidfile(&sleep_pidfile, deadline)
            .await
            .expect("pid");
        assert!(all_gone(&[child, grandchild]).await);
    }

    /// Regression: `Child::wait_with_output` can reap the group leader first
    /// and then wait on a pipe inherited by a background child. If an outer
    /// timeout fires in that interval, an armed guard must still have a live
    /// (zombie) leader pinning the pgid it signals.
    #[tokio::test]
    #[cfg(unix)]
    async fn output_timeout_after_parent_exit_kills_pipe_holding_descendant() {
        let dir = tempfile::tempdir().unwrap();
        let sleep_pidfile = dir.path().join("sleep.pid");
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(format!(
            "sleep 60 & echo $! > '{}'; exit 0",
            sleep_pidfile.display()
        ));

        let result = tokio::time::timeout(
            Duration::from_millis(500),
            output_in_process_group(&mut cmd),
        )
        .await;
        assert!(
            result.is_err(),
            "inherited stdout/stderr should keep output collection pending"
        );
        let descendant = wait_for_pidfile(
            &sleep_pidfile,
            std::time::Instant::now() + Duration::from_secs(1),
        )
        .await
        .expect("shell recorded background pid");
        assert!(
            all_gone(&[descendant]).await,
            "timeout must kill pipe-holding descendant {descendant}"
        );
    }

    /// Same guarantee for `run_command_bounded` (cargo_*, npm/pip/yarn, hook
    /// commands): a dropped future kills the grandchild.
    #[tokio::test]
    #[cfg(unix)]
    async fn run_command_bounded_drop_kills_grandchild() {
        let dir = tempfile::tempdir().unwrap();
        let (stub, pidfile, sleep_pidfile) = forking_stub(dir.path(), "stub");
        let cmd = tokio::process::Command::new(&stub);
        let mut fut = Box::pin(run_command_bounded(cmd, Duration::from_secs(60), 10_000));
        let (child, grandchild) = pids_while_running(&mut fut, &pidfile, &sleep_pidfile).await;
        drop(fut);
        assert!(all_gone(&[child, grandchild]).await);
    }

    #[tokio::test]
    async fn output_in_process_group_collects_output() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 3"]);
        let out = output_in_process_group(&mut cmd).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "out");
        assert_eq!(String::from_utf8_lossy(&out.stderr).trim(), "err");
        assert_eq!(out.status.code(), Some(3));
    }
}
