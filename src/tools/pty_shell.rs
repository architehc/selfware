//! PTY-based interactive shell sessions.
//!
//! Provides persistent shell sessions that survive across multiple tool
//! invocations, unlike the one-shot [`ShellExec`](super::shell_exec::ShellExec).
//! Each session maintains a child shell process with piped stdin/stdout/stderr
//! and uses a private control channel to detect when a command has finished.

use super::Tool;
use crate::config::SafetyConfig;
use crate::tools::file::{resolve_safety_config, validate_tool_path};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(unix)]
use tokio::io::AsyncBufReadExt;
#[cfg(not(unix))]
use tokio::io::AsyncRead;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

/// Descriptor inherited only by the trusted session shell. User commands
/// explicitly close it, so their stdout cannot forge completion.
#[cfg(unix)]
const CONTROL_FD: std::os::fd::RawFd = 3;

/// Maximum output size returned per command (bytes).
const MAX_OUTPUT_BYTES: usize = 10_240;

/// Maximum number of stderr lines retained per command. Output beyond this
/// is still drained (so the child never blocks on a full stderr pipe) but
/// discarded, so a verbose child can't grow memory without limit.
const MAX_STDERR_LINES: usize = 200;

/// Maximum number of concurrent sessions.
const MAX_SESSIONS: usize = 5;

/// Idle timeout after which a session is automatically cleaned up.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Minimum interval between commands on a single session (rate limit).
const MIN_COMMAND_INTERVAL: Duration = Duration::from_secs(1);

/// Maximum command length in characters.
const MAX_COMMAND_LENGTH: usize = 10_000;

/// Regex for stripping ANSI escape codes from output.
static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\].*?\x07|\x1b\[.*?[@-~]")
        .expect("ANSI regex pattern is valid")
});

/// Global session store.
type SharedPtySession = Arc<Mutex<PtySession>>;
static SESSIONS: Lazy<Arc<RwLock<HashMap<String, SharedPtySession>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// A persistent interactive shell session backed by a child process with
/// piped stdin, stdout, and stderr.
pub struct PtySession {
    /// The child shell process.
    child: Child,
    /// Process-group id of the session, captured at spawn.
    ///
    /// On unix the child is spawned with `process_group(0)`, making it the
    /// leader of its own group, so the pgid equals the child's pid. We keep it
    /// separately from the [`Child`] handle because `Child::id()` returns
    /// `None` once the direct shell has been reaped — at which point surviving
    /// background descendants of the group would become unreachable for
    /// cleanup (see [`PtySession::kill_process_group`]).
    #[cfg(unix)]
    pgid: Option<u32>,
    /// Writer to the child's stdin.
    stdin: tokio::process::ChildStdin,
    /// Buffered reader for the child's stdout.
    stdout: BufReader<tokio::process::ChildStdout>,
    /// Buffered reader for the child's stderr.
    stderr: BufReader<tokio::process::ChildStderr>,
    /// Private completion channel. Only the parent session shell owns the
    /// write end; every user-command child receives it closed.
    #[cfg(unix)]
    control: BufReader<tokio::net::UnixStream>,
    /// Shell executable reused for isolated per-command child shells.
    shell_path: String,
    /// Trusted, validated working directory carried between commands.
    cwd: std::path::PathBuf,
    /// Path policy used for persistent `cd` state transitions.
    safety_config: SafetyConfig,
    /// Last time a command was sent.
    last_command_at: Instant,
    /// Session creation time (for diagnostics).
    created_at: Instant,
    /// Last time any activity occurred (for idle timeout).
    last_activity: Instant,
    /// Terminal dimensions (informational only; no real PTY resize).
    cols: u16,
    rows: u16,
}

impl PtySession {
    /// Spawn a new shell session.
    ///
    /// `shell` defaults to a validated `$SHELL` or `/bin/bash` on Unix, `cmd`
    /// on Windows. An unsafe `$SHELL` value is ignored rather than spawned.
    pub async fn new(shell: Option<&str>, safety_config: SafetyConfig) -> Result<Self> {
        let env_shell = std::env::var("SHELL").ok();
        let shell_path = select_shell_argument(
            shell,
            env_shell.as_deref(),
            &safety_config,
            cfg!(target_os = "windows"),
        )?;

        // Unix uses a fixed POSIX supervisor for the private control protocol;
        // `shell_path` still selects the isolated child that interprets each
        // submitted command. This keeps completion compatible with fish/csh
        // and other accepted command shells.
        #[cfg(unix)]
        let mut cmd = Command::new("/bin/sh");
        #[cfg(not(unix))]
        let mut cmd = Command::new(&shell_path);
        // Clear inherited credentials immediately after construction so both
        // cfg-specific spawn paths use the shared child-environment policy.
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Run the child in its own process group so a timeout can reap the
        // ENTIRE process tree (grandchildren included, e.g. an interactive
        // python/node/cat stuck reading stdin), not just the direct shell —
        // `kill_on_drop`/`child.kill()` only signal the immediate child, so an
        // orphaned grandchild would otherwise keep the session poisoned.
        #[cfg(unix)]
        cmd.process_group(0);

        #[cfg(unix)]
        let (control_reader, control_writer) = {
            use std::os::fd::AsRawFd;
            use std::os::unix::process::CommandExt;

            let (reader, writer) = std::os::unix::net::UnixStream::pair()
                .context("Failed to create PTY completion channel")?;
            reader
                .set_nonblocking(true)
                .context("Failed to configure PTY completion channel")?;
            let writer_fd = writer.as_raw_fd();
            // SAFETY: the closure calls only async-signal-safe libc functions
            // between fork and exec. `writer` remains alive through spawn.
            unsafe {
                cmd.as_std_mut().pre_exec(move || {
                    if libc::dup2(writer_fd, CONTROL_FD) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            (reader, writer)
        };

        // Start the session in the calling agent's workspace root.
        crate::tools::workspace_root::CommandRootExt::in_workspace_root(&mut cmd);

        // Disable history and prompts to keep output clean.
        cmd.env("HISTFILE", "/dev/null")
            .env("PS1", "")
            .env("PS2", "")
            .env("TERM", "dumb");
        // git in this session must not run what an untrusted repository
        // configured (see `git_exec::shell_git_env`).
        crate::safety::git_exec::apply_shell_git_env(
            &mut cmd,
            &crate::tools::workspace_root::current_path(),
        );

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn shell: {}", shell_path))?;
        #[cfg(unix)]
        drop(control_writer);

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture child stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture child stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture child stderr")?;

        let now = Instant::now();
        let cwd = crate::tools::workspace_root::current_path()
            .canonicalize()
            .context("Failed to resolve PTY workspace root")?;

        // Capture the process-group id at spawn. The session child runs in its
        // own process group (see `process_group(0)` above), so the pgid equals
        // the child's pid. It must be captured here rather than re-derived
        // later from the Child handle: once the direct shell has been reaped,
        // `Child::id()` returns None and any surviving background descendants
        // of the group would become unreachable.
        #[cfg(unix)]
        let pgid = child.id();

        let mut session = Self {
            child,
            #[cfg(unix)]
            pgid,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
            #[cfg(unix)]
            control: BufReader::new(
                tokio::net::UnixStream::from_std(control_reader)
                    .context("Failed to register PTY completion channel")?,
            ),
            shell_path,
            cwd,
            safety_config,
            last_command_at: now - MIN_COMMAND_INTERVAL, // allow immediate first command
            created_at: now,
            last_activity: now,
            cols: 80,
            rows: 24,
        };

        // Synchronize the trusted Unix supervisor before accepting commands.
        session.drain_startup().await?;

        Ok(session)
    }

    /// Send a synchronization marker after startup to consume any banner output.
    async fn drain_startup(&mut self) -> Result<()> {
        #[cfg(unix)]
        {
            self.stdin
                .write_all(b"printf '0\\n' >&3\n")
                .await
                .context("Failed to write startup synchronization")?;
            self.stdin.flush().await?;
            let mut line = String::new();
            let read =
                tokio::time::timeout(Duration::from_secs(5), self.control.read_line(&mut line))
                    .await
                    .context("Timed out synchronizing PTY shell startup")??;
            if read == 0 || Self::parse_control_code(&line) != Some(0) {
                bail!("PTY shell startup completion channel closed unexpectedly");
            }
            Ok(())
        }

        #[cfg(not(unix))]
        {
            // Commands on this platform are direct children whose OS exit
            // status is trusted, so the idle session shell needs no stdout
            // synchronization protocol.
            Ok(())
        }
    }

    /// Send a command and wait for its output.
    ///
    /// On Unix each command runs in a fresh child shell with stdin and the
    /// private completion descriptor closed. The persistent parent therefore
    /// retains process/cwd identity without retaining attacker-controlled
    /// variables, functions, aliases, traps or PATH changes across calls.
    pub async fn send_command(&mut self, cmd: &str, timeout_secs: u64) -> Result<CommandOutput> {
        #[cfg(unix)]
        {
            return self.send_command_unix(cmd, timeout_secs).await;
        }
        #[cfg(not(unix))]
        {
            self.send_command_nonunix(cmd, timeout_secs).await
        }
    }

    #[cfg(unix)]
    async fn send_command_unix(&mut self, cmd: &str, timeout_secs: u64) -> Result<CommandOutput> {
        // Rate limiting
        let elapsed = self.last_command_at.elapsed();
        if elapsed < MIN_COMMAND_INTERVAL {
            tokio::time::sleep(MIN_COMMAND_INTERVAL - elapsed).await;
        }

        self.last_command_at = Instant::now();
        self.last_activity = Instant::now();

        if let Some(new_cwd) = self.persistent_cd_target(cmd)? {
            self.cwd = new_cwd;
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
            });
        }
        self.cwd = self.canonicalize_safe_cwd(&self.cwd)?;

        // Quote both operands instead of interpolating the user program into
        // the trusted parent shell's syntax. Descriptor 3 and stdin are closed
        // in the child, so it cannot consume or forge the control protocol.
        let shell_name = std::path::Path::new(&self.shell_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let interpreter = if shell_name == "busybox" {
            format!(
                "{} sh -c {}",
                shell_quote(&self.shell_path),
                shell_quote(cmd)
            )
        } else {
            format!("{} -c {}", shell_quote(&self.shell_path), shell_quote(cmd))
        };
        // Bind the completion message to this command. Even on systems where
        // a same-UID child can reopen another process's descriptors through
        // procfs, it cannot guess a nonce that exists only in the trusted
        // supervisor's pending script.
        let control_nonce = Uuid::new_v4().simple().to_string();
        let full_cmd = format!(
            "cd -- {} && {} </dev/null 3>&-\n__selfware_ec=$?\nprintf '%s:%s\\n' {} \"$__selfware_ec\" >&3\n",
            shell_quote(&self.cwd.to_string_lossy()),
            interpreter,
            shell_quote(&control_nonce),
        );

        self.stdin
            .write_all(full_cmd.as_bytes())
            .await
            .context("Failed to write command to shell stdin")?;
        self.stdin.flush().await?;

        let timeout = Duration::from_secs(timeout_secs.min(3600));
        let deadline = Instant::now() + timeout;
        let mut stdout_bytes = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
        let mut stdout_truncated = false;
        let mut stderr_bytes = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
        let mut stderr_truncated = false;
        let exit_code: i32;
        let mut control_line = String::new();

        // Read stdout, stderr and the private control channel concurrently
        // until the trusted parent reports completion. The stderr drain keeps a
        // chatty child (compiler warnings, verbose test suites) from blocking
        // on a full stderr pipe while the parent reads only stdout — the
        // pipe-stall deadlock from the 2026-09-21 review: a child emitting
        // more than one pipe buffer (~64KB) of stderr stalled its write(2)
        // on the full pipe while the parent blocked on read(2) for the
        // completion marker, and only the timeout force-kill broke it. The
        // bounded stderr capture is kept intact; everything beyond the cap is
        // still drained, never buffered. Fixed-size chunk reads are required:
        // `read_line` itself can allocate without bound before a newline.
        // Disabled once the child's stderr hits EOF so the select never
        // busy-polls an immediately-ready stream.
        let mut stderr_open = true;
        let mut stdout_chunk = [0_u8; 8192];
        let mut stderr_chunk = [0_u8; 8192];

        loop {
            if Instant::now() > deadline {
                return Ok(self
                    .report_timeout(
                        stdout_bytes,
                        stdout_truncated,
                        stderr_bytes,
                        stderr_truncated,
                    )
                    .await);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(self
                    .report_timeout(
                        stdout_bytes,
                        stdout_truncated,
                        stderr_bytes,
                        stderr_truncated,
                    )
                    .await);
            }

            control_line.clear();
            // Unbiased select: when BOTH stdout and stderr are continuously
            // ready, each ready arm is serviced fairly, so a sustained stdout
            // flood can never starve the stderr drain (the pipe-stall would
            // just move to stderr). The deadline is enforced by the
            // top-of-loop checks above plus this sleep arm, which is ready as
            // soon as `remaining` elapses and is then picked within a couple
            // of iterations.
            tokio::select! {
                _ = tokio::time::sleep(remaining) => {
                    // Deadline: terminate the stuck child before reporting, so
                    // the hung process cannot keep running behind the session.
                    return Ok(self.report_timeout(
                        stdout_bytes,
                        stdout_truncated,
                        stderr_bytes,
                        stderr_truncated,
                    ).await);
                }
                res = self.control.read_line(&mut control_line) => {
                    match res {
                        Ok(0) => {
                            self.terminate_stuck_child().await;
                            bail!("PTY completion channel closed unexpectedly");
                        }
                        Ok(_) => {
                            let Some(code) = Self::parse_command_control_code(
                                &control_line,
                                &control_nonce,
                            ) else {
                                self.terminate_stuck_child().await;
                                bail!("PTY completion channel sent an invalid exit status");
                            };
                            exit_code = code;
                            break;
                        }
                        Err(error) => {
                            self.terminate_stuck_child().await;
                            bail!("Error reading PTY completion channel: {error}");
                        }
                    }
                }
                res = self.stdout.read(&mut stdout_chunk) => {
                    match res {
                        Ok(0) => {
                            self.terminate_stuck_child().await;
                            bail!("PTY shell output closed before command completion");
                        }
                        Ok(read) => {
                            append_bounded(
                                &mut stdout_bytes,
                                &stdout_chunk[..read],
                                MAX_OUTPUT_BYTES,
                                &mut stdout_truncated,
                            );
                        }
                        Err(e) => {
                            self.terminate_stuck_child().await;
                            bail!("Error reading shell output: {}", e);
                        }
                    }
                }
                res = self.stderr.read(&mut stderr_chunk), if stderr_open => {
                    match res {
                        Ok(0) | Err(_) => {
                            // stderr EOF / error: stop polling this stream,
                            // keep reading stdout.
                            stderr_open = false;
                        }
                        Ok(read) => {
                            append_bounded(
                                &mut stderr_bytes,
                                &stderr_chunk[..read],
                                MAX_OUTPUT_BYTES,
                                &mut stderr_truncated,
                            );
                        }
                    }
                }
            }
        }

        // The control socket and stdout are separate pipes, so the exit status
        // may be observed just before the final output bytes. Briefly drain
        // both output streams after completion.
        self.drain_stdout_into(&mut stdout_bytes, &mut stdout_truncated)
            .await;

        // Drain any stderr that's accumulated since completion (e.g. a
        // background writer still holding the pipe).
        self.drain_stderr_into(&mut stderr_bytes, &mut stderr_truncated)
            .await;

        Ok(CommandOutput {
            stdout: format_captured_output(&stdout_bytes, stdout_truncated, None),
            stderr: format_captured_output(&stderr_bytes, stderr_truncated, Some(MAX_STDERR_LINES)),
            exit_code,
            timed_out: false,
        })
    }

    fn persistent_cd_target(&self, command: &str) -> Result<Option<std::path::PathBuf>> {
        #[cfg(unix)]
        let operand = {
            let Some(words) = shlex::split(command) else {
                return Ok(None);
            };
            if words.first().map(String::as_str) != Some("cd") || words.len() > 2 {
                return Ok(None);
            }
            words.get(1).cloned()
        };
        #[cfg(not(unix))]
        let Some(operand) = parse_nonunix_standalone_cd(command) else {
            return Ok(None);
        };

        let candidate = match operand.as_deref() {
            None => crate::tools::workspace_root::current_path(),
            Some("-") | Some("~") => return Ok(None),
            Some(path) => {
                let path = std::path::Path::new(path);
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.cwd.join(path)
                }
            }
        };
        Ok(Some(self.canonicalize_safe_cwd(&candidate)?))
    }

    fn canonicalize_safe_cwd(&self, candidate: &std::path::Path) -> Result<std::path::PathBuf> {
        let display = candidate.to_string_lossy();
        validate_tool_path(&display, &self.safety_config)
            .with_context(|| format!("PTY cwd target rejected: {display}"))?;
        let canonical = candidate
            .canonicalize()
            .with_context(|| format!("PTY cwd target does not exist: {display}"))?;
        if !canonical.is_dir() {
            bail!("PTY cwd target is not a directory: {}", canonical.display());
        }
        let canonical_display = canonical.to_string_lossy();
        validate_tool_path(&canonical_display, &self.safety_config).with_context(|| {
            format!("PTY cwd target rejected after resolving links: {canonical_display}")
        })?;
        Ok(canonical)
    }

    #[cfg(not(unix))]
    async fn send_command_nonunix(
        &mut self,
        cmd: &str,
        timeout_secs: u64,
    ) -> Result<CommandOutput> {
        let elapsed = self.last_command_at.elapsed();
        if elapsed < MIN_COMMAND_INTERVAL {
            tokio::time::sleep(MIN_COMMAND_INTERVAL - elapsed).await;
        }
        self.last_command_at = Instant::now();
        self.last_activity = Instant::now();

        if let Some(new_cwd) = self.persistent_cd_target(cmd)? {
            self.cwd = new_cwd;
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
            });
        }
        self.cwd = self.canonicalize_safe_cwd(&self.cwd)?;

        // Platforms without Unix descriptor passing execute each command as a
        // direct child and trust the operating-system exit status. Keeping the
        // completion protocol out of stdout prevents command output from
        // impersonating a marker, while the isolated process also prevents
        // environment or shell-function state from poisoning later calls.
        let mut command = Command::new(&self.shell_path);
        crate::safety::process_env::sanitize_command_env(&mut command);
        let shell_name = std::path::Path::new(&self.shell_path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match shell_name.as_str() {
            "cmd" => {
                command.args(["/D", "/S", "/C", cmd]);
            }
            "powershell" | "pwsh" => {
                command.args(["-NoProfile", "-NonInteractive", "-Command", cmd]);
            }
            "busybox" => {
                command.args(["sh", "-c", cmd]);
            }
            _ => {
                command.args(["-c", cmd]);
            }
        }
        command
            .current_dir(&self.cwd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        crate::safety::git_exec::apply_shell_git_env(&mut command, &self.cwd);

        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to spawn command shell: {}", self.shell_path))?;
        let mut process_guard = crate::tools::process_guard::ProcessGroupGuard::new(child.id());
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture command stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture command stderr")?;
        let mut stdout_task = tokio::spawn(drain_bounded(stdout, MAX_OUTPUT_BYTES + 1));
        let mut stderr_task = tokio::spawn(drain_bounded(stderr, MAX_OUTPUT_BYTES + 1));
        let timeout = Duration::from_secs(timeout_secs.min(3600));
        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(result) => result.context("Failed waiting for command shell")?,
            Err(_) => {
                process_guard.kill();
                let _ = child.kill().await;
                let _ = child.wait().await;
                stdout_task.abort();
                stderr_task.abort();
                return Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    exit_code: -1,
                    timed_out: true,
                });
            }
        };
        process_guard.disarm();

        // A background descendant can inherit a pipe after the direct shell
        // exits. Give final output a short drain window, then stop the readers
        // rather than letting an inherited handle hang the tool forever.
        let stdout = match tokio::time::timeout(Duration::from_millis(100), &mut stdout_task).await
        {
            Ok(Ok(Ok(bytes))) => bytes,
            Ok(Ok(Err(error))) => return Err(error).context("Failed reading command stdout"),
            Ok(Err(error)) => return Err(error).context("Command stdout reader failed"),
            Err(_) => {
                stdout_task.abort();
                Vec::new()
            }
        };
        let stderr = match tokio::time::timeout(Duration::from_millis(100), &mut stderr_task).await
        {
            Ok(Ok(Ok(bytes))) => bytes,
            Ok(Ok(Err(error))) => return Err(error).context("Failed reading command stderr"),
            Ok(Err(error)) => return Err(error).context("Command stderr reader failed"),
            Err(_) => {
                stderr_task.abort();
                Vec::new()
            }
        };

        Ok(CommandOutput {
            stdout: format_bounded_output(&stdout),
            stderr: format_bounded_output(&stderr),
            exit_code: status.code().unwrap_or(-1),
            timed_out: false,
        })
    }

    /// Report a timeout after terminating the stuck child process tree, so a
    /// hung interactive command cannot keep running behind the session.
    #[cfg(unix)]
    async fn report_timeout(
        &mut self,
        mut stdout: Vec<u8>,
        mut stdout_truncated: bool,
        mut stderr: Vec<u8>,
        mut stderr_truncated: bool,
    ) -> CommandOutput {
        self.terminate_stuck_child().await;
        self.drain_stdout_into(&mut stdout, &mut stdout_truncated)
            .await;
        self.drain_stderr_into(&mut stderr, &mut stderr_truncated)
            .await;
        CommandOutput {
            stdout: format_captured_output(&stdout, stdout_truncated, None),
            stderr: format_captured_output(&stderr, stderr_truncated, Some(MAX_STDERR_LINES)),
            exit_code: -1,
            timed_out: true,
        }
    }

    /// Terminate a stuck child process tree after a command times out.
    ///
    /// Escalation order:
    /// 1. Cooperative interrupt — an ETX (`\x03`, Ctrl+C) is written to the
    ///    child's stdin so an interactive program gets a chance to shut down
    ///    cleanly.
    /// 2. A short grace period (~500 ms) for the process to exit on its own.
    /// 3. If the process tree is still alive, the ENTIRE process group is
    ///    SIGKILLed and the shell reaped. The session child runs in its own
    ///    process group (see [`PtySession::new`]), so the group kill reaches
    ///    grandchildren — an interactive `python`/`node`/`cat` still reading
    ///    stdin — instead of orphaning them behind a dead direct child.
    ///
    /// The group kill is attempted even when the shell itself has already
    /// exited: background descendants may have outlived it (e.g. a background
    /// job holding the pipes open, which is exactly when this path runs).
    #[cfg(unix)]
    async fn terminate_stuck_child(&mut self) {
        // 1. Cooperative interrupt: ETX (Ctrl+C) on the child's stdin.
        let _ = self.stdin.write_all(b"\x03").await;
        let _ = self.stdin.flush().await;

        // 2. Short grace period for the shell to exit on its own. Note that an
        //    exited shell does NOT end the cleanup: descendants may belong to
        //    the process group, so we always fall through to the group kill.
        let grace = Instant::now() + Duration::from_millis(500);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break, // shell exited; descendants may remain.
                Ok(None) => {}
                Err(_) => break, // cannot determine state; still try the kill.
            }
            if Instant::now() >= grace {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // 3. Still alive (or its group is): kill the whole process tree and
        //    reap the shell.
        self.kill_process_group();
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }

    /// SIGKILL every remaining member of the session's process group.
    ///
    /// Uses the pgid captured at spawn rather than the live child pid: once
    /// the shell has been reaped, `Child::id()` returns `None` and the group
    /// (which may still contain backgrounded descendants) would be
    /// unreachable.
    fn kill_process_group(&mut self) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid.take() {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(pgid as i32), Signal::SIGKILL);
        }
    }

    /// Parse the trusted control channel's one-line exit status.
    #[cfg(unix)]
    fn parse_control_code(line: &str) -> Option<i32> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.bytes().any(|b| !b.is_ascii_digit() && b != b'-') {
            return None;
        }
        trimmed.parse::<i32>().ok()
    }

    #[cfg(unix)]
    fn parse_command_control_code(line: &str, expected_nonce: &str) -> Option<i32> {
        let (nonce, code) = line.trim().split_once(':')?;
        if nonce != expected_nonce {
            return None;
        }
        Self::parse_control_code(code)
    }

    /// Collect output lines into a single string, stripping ANSI codes and
    /// truncating to the maximum size.
    #[cfg(test)]
    fn collect_output(lines: &[String]) -> String {
        let raw = lines.join("\n");
        let cleaned = strip_ansi(&raw);
        if cleaned.len() > MAX_OUTPUT_BYTES {
            let truncated: String = cleaned.chars().take(MAX_OUTPUT_BYTES).collect();
            format!(
                "{}\n... [output truncated at {} bytes]",
                truncated, MAX_OUTPUT_BYTES
            )
        } else {
            cleaned
        }
    }

    /// Drain any available stderr without blocking.
    async fn drain_stderr(&mut self) -> String {
        let mut bytes = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
        let mut truncated = false;
        self.drain_stderr_into(&mut bytes, &mut truncated).await;
        format_captured_output(&bytes, truncated, Some(MAX_STDERR_LINES))
    }

    async fn drain_stderr_into(&mut self, bytes: &mut Vec<u8>, truncated: &mut bool) {
        drain_available(
            &mut self.stderr,
            bytes,
            truncated,
            Duration::from_millis(50),
        )
        .await;
    }

    async fn drain_stdout_into(&mut self, bytes: &mut Vec<u8>, truncated: &mut bool) {
        drain_available(
            &mut self.stdout,
            bytes,
            truncated,
            Duration::from_millis(50),
        )
        .await;
    }

    /// Read any pending output without sending a command.
    pub async fn read_output(&mut self) -> Result<String> {
        self.last_activity = Instant::now();
        let mut stdout_bytes = Vec::with_capacity(MAX_OUTPUT_BYTES.min(8192));
        let mut stdout_truncated = false;
        drain_available(
            &mut self.stdout,
            &mut stdout_bytes,
            &mut stdout_truncated,
            Duration::from_millis(100),
        )
        .await;

        let stderr = self.drain_stderr().await;
        let mut output = format_captured_output(&stdout_bytes, stdout_truncated, None);
        if !stderr.is_empty() {
            output.push_str("\n[stderr] ");
            output.push_str(&stderr);
        }
        Ok(output)
    }

    /// Update the stored terminal dimensions.
    ///
    /// Note: without a real PTY this is informational only. The stored values
    /// are returned in session info but do not affect the child process.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
    }

    /// Check whether the child process is still running.
    pub fn is_alive(&mut self) -> bool {
        #[cfg(unix)]
        {
            // Observe exit without reaping first. The zombie leader pins its
            // PID/PGID, so we can safely kill any surviving descendants
            // before `try_wait` releases that identity for OS reuse.
            let Some(pid) = self.child.id() else {
                self.pgid = None;
                return false;
            };
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let observed = unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            };
            if observed == 0 {
                if unsafe { info.si_pid() } == 0 {
                    return true;
                }
                self.kill_process_group();
                let _ = self.child.try_wait();
                return false;
            }

            // If the non-reaping probe is unavailable, retain the PGID while
            // the child is live. On a confirmed exit, consume it before the
            // fallback reap so a later drop can never signal a reused group.
            match self.child.try_wait() {
                Ok(None) => true,
                Ok(Some(_)) | Err(_) => {
                    self.pgid = None;
                    false
                }
            }
        }

        #[cfg(not(unix))]
        {
            matches!(self.child.try_wait(), Ok(None))
        }
    }

    /// Pid of the session shell (also its process-group id on unix), while
    /// it has not been reaped.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Signal the session's process tree without waiting: SIGTERM, or
    /// SIGKILL with `force`. Only while the shell is unreaped, so the group
    /// id cannot have been reused.
    fn signal_tree(&mut self, force: bool) {
        if !self.is_alive() {
            return;
        }
        #[cfg(unix)]
        if let Some(pgid) = self.pgid.and_then(|p| i32::try_from(p).ok()) {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let signal = if force {
                Signal::SIGKILL
            } else {
                Signal::SIGTERM
            };
            if killpg(Pid::from_raw(pgid), signal).is_ok() {
                return;
            }
        }
        let _ = self.child.start_kill();
    }

    /// Terminate the session, killing the child process tree.
    pub async fn close(&mut self) {
        // Kill the whole process group (shell plus anything it spawned) so no
        // interactive grandchild survives the session, then reap the shell.
        self.kill_process_group();
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for PtySession {
    /// Last-resort teardown: if the session is dropped while its child (or a
    /// grandchild from a backgrounded job) is still running, SIGKILL the whole
    /// process group. `kill_on_drop` on the child would only signal the direct
    /// shell, leaving grandchildren stranded.
    fn drop(&mut self) {
        self.kill_process_group();
    }
}

/// Output from a single command execution.
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub timed_out: bool,
}

/// Strip ANSI escape sequences from text.
fn strip_ansi(s: &str) -> String {
    ANSI_RE.replace_all(s, "").into_owned()
}

fn append_bounded(kept: &mut Vec<u8>, chunk: &[u8], limit: usize, truncated: &mut bool) {
    let remaining = limit.saturating_sub(kept.len());
    kept.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    *truncated |= chunk.len() > remaining;
}

async fn drain_available<R>(
    reader: &mut R,
    kept: &mut Vec<u8>,
    truncated: &mut bool,
    window: Duration,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let deadline = Instant::now() + window;
    let mut chunk = [0_u8; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, reader.read(&mut chunk)).await {
            Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
            Ok(Ok(read)) => append_bounded(kept, &chunk[..read], MAX_OUTPUT_BYTES, truncated),
        }
    }
}

fn format_captured_output(bytes: &[u8], byte_truncated: bool, max_lines: Option<usize>) -> String {
    let cleaned = strip_ansi(&String::from_utf8_lossy(bytes));
    let cleaned = cleaned.trim_end_matches(['\r', '\n']);
    let mut line_truncated = false;
    let output = if let Some(max_lines) = max_lines {
        let mut lines = cleaned.lines();
        let output = lines
            .by_ref()
            .take(max_lines)
            .collect::<Vec<_>>()
            .join("\n");
        line_truncated = lines.next().is_some();
        output
    } else {
        cleaned.to_string()
    };
    if byte_truncated {
        format!(
            "{}\n... [output truncated at {} bytes]",
            output, MAX_OUTPUT_BYTES
        )
    } else if line_truncated {
        format!(
            "{}\n... [stderr truncated at {} lines]",
            output,
            max_lines.unwrap_or(MAX_STDERR_LINES)
        )
    } else {
        output
    }
}

/// Parse the deliberately narrow persistent-cwd syntax used on non-Unix
/// shells without applying POSIX escaping rules to Windows backslashes.
/// Returns `None` when the command is not a standalone safe `cd`; the inner
/// option is `None` for bare `cd` and `Some(path)` for an explicit operand.
#[cfg(any(not(unix), test))]
fn parse_nonunix_standalone_cd(command: &str) -> Option<Option<String>> {
    let command = command.trim();
    let prefix = command.get(..2)?;
    if !prefix.eq_ignore_ascii_case("cd") {
        return None;
    }
    let rest = &command[2..];
    if !rest.is_empty() && !rest.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let operand = rest.trim();
    if operand.is_empty() {
        return Some(None);
    }
    if operand
        .chars()
        .any(|c| matches!(c, '&' | '|' | ';' | '<' | '>' | '\r' | '\n' | '\0'))
    {
        return None;
    }
    let path = match (operand.chars().next(), operand.chars().last()) {
        (Some(first @ ('\'' | '"')), Some(last)) if first == last && operand.len() >= 2 => {
            let inner = &operand[1..operand.len() - 1];
            if inner.contains(first) {
                return None;
            }
            inner
        }
        (Some('\'' | '"'), _) | (_, Some('\'' | '"')) => return None,
        _ if operand.chars().any(char::is_whitespace) => return None,
        _ => operand,
    };
    Some(Some(path.to_string()))
}

#[cfg(not(unix))]
async fn drain_bounded<R>(mut reader: R, limit: usize) -> std::io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut kept = Vec::with_capacity(limit.min(8192));
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(kept)
}

#[cfg(not(unix))]
fn format_bounded_output(bytes: &[u8]) -> String {
    format_captured_output(
        &bytes[..bytes.len().min(MAX_OUTPUT_BYTES)],
        bytes.len() > MAX_OUTPUT_BYTES,
        None,
    )
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Validate a command against the shared dangerous-pattern blocklist.
fn check_dangerous_patterns(command: &str) -> Result<()> {
    if let Some(pattern) = super::find_dangerous_shell_pattern(command) {
        bail!("Blocked potentially dangerous shell pattern: {}", pattern);
    }
    Ok(())
}

/// Well-known shell executable names. A bare `shell` argument resolving
/// through PATH is only accepted when it is one of these.
fn is_known_shell_name(name: &str) -> bool {
    matches!(
        name,
        "sh" | "bash"
            | "zsh"
            | "ksh"
            | "dash"
            | "ash"
            | "csh"
            | "tcsh"
            | "fish"
            | "elvish"
            | "xonsh"
            | "nu"
            | "nushell"
            | "pwsh"
            | "powershell"
            | "cmd"
            | "busybox"
    )
}

/// System directories that may legitimately host a shell binary. Shells in
/// these locations are trusted by (name, directory); everything else must
/// pass the full workspace path policy below.
const TRUSTED_SHELL_DIRS: &[&str] = &[
    "/bin",
    "/usr/bin",
    "/sbin",
    "/usr/sbin",
    "/opt/homebrew/bin",
    "/opt/homebrew/sbin",
    "/usr/local/bin",
    "/opt/local/bin",
    "/run/current-system/sw/bin",
];

/// Validate the `shell` argument of `pty_shell start`.
///
/// The operand is SPAWNED as a process, so an arbitrary path here is a
/// code-execution primitive: `pty_shell {action: "start", shell:
/// "/tmp/evil"}` would execute whatever the attacker parked there. The
/// default shells (the omitted case resolves to `$SHELL` or `/bin/bash`)
/// are trusted by construction and never validated here. An explicit
/// `shell` must be (in order):
///
/// 1. Bare NAME resolved by the OS through PATH — allowed only when it is
///    a known shell name (`bash`, `zsh`, `cmd`, …).
/// 2. A known shell binary inside a trusted system shell directory
///    (`/bin/bash`, `/usr/bin/fish`, `/opt/homebrew/bin/zsh`, …).
/// 3. A path that passes the full workspace path policy
///    ([`validate_tool_path`]) — a project-local shell under the workspace
///    or in an explicitly allowed directory.
///
/// Anything else is refused. The system-shell carve-out (rules 1–2) is the
/// deliberate exception the 2026-09-21 review's sweep allows for this
/// class: the strict file-tool policy alone would refuse `/bin/bash`, which
/// is the tool's documented default and would make legitimate shell
/// sessions unusable.
fn validate_shell_argument(shell: &str, safety: &SafetyConfig) -> Result<()> {
    if shell.is_empty() {
        bail!("shell must not be empty");
    }
    if shell.contains('\0') {
        bail!("shell path contains null bytes");
    }
    // Bare names resolve through PATH; allow only known shell names.
    if !shell.contains('/') && !shell.contains('\\') {
        if is_known_shell_name(shell) {
            return Ok(());
        }
        bail!("refusing unknown shell executable from PATH: {shell}");
    }
    let path = std::path::Path::new(shell);
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if is_known_shell_name(name) {
        if let Some(dir) = path.parent().and_then(|d| d.to_str()) {
            if TRUSTED_SHELL_DIRS.contains(&dir) {
                return Ok(());
            }
        }
    }
    // Fallback: the path must obey the workspace path policy (denied
    // patterns, workspace containment or allowed list, symlink escapes).
    validate_tool_path(shell, safety)
}

fn select_shell_argument(
    explicit: Option<&str>,
    env_shell: Option<&str>,
    safety: &SafetyConfig,
    windows: bool,
) -> Result<String> {
    let fallback = if windows { "cmd" } else { "/bin/bash" };
    if let Some(shell) = explicit {
        validate_shell_argument(shell, safety)?;
        return Ok(shell.to_string());
    }
    if let Some(candidate) = env_shell {
        if validate_shell_argument(candidate, safety).is_ok() {
            return Ok(candidate.to_string());
        }
        tracing::warn!(
            shell = %candidate,
            fallback,
            "ignoring unsafe SHELL value for pty session"
        );
    }
    Ok(fallback.to_string())
}

/// Remove sessions that have been idle longer than [`IDLE_TIMEOUT`].
async fn cleanup_idle_sessions(sessions: &RwLock<HashMap<String, SharedPtySession>>) {
    let candidates: Vec<_> = sessions
        .read()
        .await
        .iter()
        .map(|(id, session)| (id.clone(), Arc::clone(session)))
        .collect();
    for (id, shared) in candidates {
        // An active command owns this session lock and refreshed
        // `last_activity` before starting; skip it without delaying unrelated
        // sessions or actions.
        let Ok(mut session) = shared.try_lock() else {
            continue;
        };
        if session.last_activity.elapsed() <= IDLE_TIMEOUT {
            continue;
        }
        let removed = {
            let mut map = sessions.write().await;
            if map
                .get(&id)
                .is_some_and(|current| Arc::ptr_eq(current, &shared))
            {
                map.remove(&id);
                true
            } else {
                false
            }
        };
        if removed {
            session.close().await;
            release_session_resource(&id);
        }
    }
}

/// Mark the registry entry of a closed (reaped) session released.
fn release_session_resource(session_id: &str) {
    use crate::resources::{ResourceHandle, ResourceRegistry};
    let session = crate::resources::session_id();
    ResourceRegistry::global().release_where("pty session closed and reaped", |r| {
        r.session == session
            && matches!(&r.handle, ResourceHandle::Pty { session_id: s, .. } if s == session_id)
    });
}

async fn shared_session(id: &str) -> Option<SharedPtySession> {
    SESSIONS.read().await.get(id).cloned()
}

/// Whether session `id` of this process has exited (reaping the shell).
/// `None` if the session is not in this process's map.
pub(crate) async fn session_exited(id: &str) -> Option<bool> {
    let shared = shared_session(id).await?;
    let mut session = tokio::time::timeout(Duration::from_secs(1), shared.lock())
        .await
        .ok()?;
    Some(!session.is_alive())
}

/// Signal session `id`'s process tree (teardown). `false` if the session is
/// not in this process's map.
pub(crate) async fn signal_session(id: &str, force: bool) -> bool {
    let Some(shared) = shared_session(id).await else {
        return false;
    };
    let Ok(mut session) = tokio::time::timeout(Duration::from_secs(1), shared.lock()).await else {
        return false;
    };
    session.signal_tree(force);
    true
}

/// Drop session `id` from the map after teardown confirmed it exited.
pub(crate) async fn forget_session(id: &str) {
    let shared = SESSIONS.write().await.remove(id);
    if let Some(shared) = shared {
        let mut session = shared.lock().await;
        session.close().await;
    }
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

/// Interactive PTY shell tool that maintains persistent shell sessions.
#[derive(Default)]
pub struct PtyShellTool {
    /// Per-instance safety config for path-policy enforcement; falls back to
    /// the process-global config when `None`.
    pub safety_config: Option<SafetyConfig>,
}

impl PtyShellTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[derive(Deserialize)]
struct PtyArgs {
    action: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    shell: Option<String>,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}

fn default_timeout() -> u64 {
    60
}

#[async_trait]
impl Tool for PtyShellTool {
    fn name(&self) -> &str {
        "pty_shell"
    }

    fn description(&self) -> &str {
        "Interactive shell processes that persist across invocations within a task \
         (closed when the task ends). Standalone 'cd <path>' updates a validated persistent \
         working directory; each other command runs in an isolated child shell, so environment, \
         aliases, functions and traps do not carry into later calls. \
         Supports multiple concurrent sessions with automatic idle cleanup. \
         Actions: start, send, read, resize, status, close."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "send", "read", "resize", "status", "close"],
                    "description": "Action to perform on the session"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID (required for all actions except 'start')"
                },
                "command": {
                    "type": "string",
                    "description": "Command to send (required for 'send' action)"
                },
                "shell": {
                    "type": "string",
                    "description": "Shell to use (for 'start' action; defaults to a validated $SHELL or /bin/bash)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "default": 60,
                    "description": "Timeout for command completion in seconds (max 3600)"
                },
                "cols": {
                    "type": "integer",
                    "description": "Terminal columns (for 'resize' action)"
                },
                "rows": {
                    "type": "integer",
                    "description": "Terminal rows (for 'resize' action)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let args: PtyArgs = serde_json::from_value(args)?;

        // Run idle cleanup before every action.
        cleanup_idle_sessions(&SESSIONS).await;

        match args.action.as_str() {
            "start" => self.handle_start(args).await,
            "send" => self.handle_send(args).await,
            "read" => self.handle_read(args).await,
            "resize" => self.handle_resize(args).await,
            "status" => self.handle_status(args).await,
            "close" => self.handle_close(args).await,
            other => bail!("Unknown pty_shell action: {}", other),
        }
    }
}

impl PtyShellTool {
    async fn handle_start(&self, args: PtyArgs) -> Result<Value> {
        if SESSIONS.read().await.len() >= MAX_SESSIONS {
            bail!(
                "Maximum number of concurrent sessions ({}) reached. \
                 Close an existing session first.",
                MAX_SESSIONS
            );
        }

        let session_id = Uuid::new_v4().to_string();
        let shell = args.shell.as_deref();
        let safety = resolve_safety_config(self.safety_config.as_ref());
        let mut session = PtySession::new(shell, safety).await?;
        let cwd = session.cwd.display().to_string();
        let pid = session.pid();

        let active_sessions = {
            let mut sessions = SESSIONS.write().await;
            // Recheck after the async spawn so concurrent starts cannot race
            // past the global limit.
            if sessions.len() >= MAX_SESSIONS {
                drop(sessions);
                session.close().await;
                bail!(
                    "Maximum number of concurrent sessions ({}) reached. \
                     Close an existing session first.",
                    MAX_SESSIONS
                );
            }
            sessions.insert(session_id.clone(), Arc::new(Mutex::new(session)));
            sessions.len()
        };

        if let Some(pid) = pid {
            use crate::resources::{NewResource, ResourceHandle, ResourceKind, ResourceRegistry};
            ResourceRegistry::global().register(NewResource::new(
                ResourceKind::Pty,
                ResourceHandle::Pty {
                    session_id: session_id.clone(),
                    pid,
                    pgid: cfg!(unix).then_some(pid),
                    start_time: crate::resources::driver::process_start_time(pid),
                },
                format!("pty_shell {}", shell.unwrap_or("default shell")),
            ));
        }
        Ok(serde_json::json!({
            "status": "started",
            "session_id": session_id,
            "cwd": cwd,
            "active_sessions": active_sessions
        }))
    }

    async fn handle_send(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'send' action")?;
        let command = args
            .command
            .as_deref()
            .context("command is required for 'send' action")?;

        // Validate command length.
        if command.len() > MAX_COMMAND_LENGTH {
            bail!(
                "Command exceeds maximum length of {} characters",
                MAX_COMMAND_LENGTH
            );
        }

        // Check dangerous patterns.
        check_dangerous_patterns(command)?;

        let shared = SESSIONS
            .read()
            .await
            .get(session_id)
            .cloned()
            .context(format!("No session found with id: {}", session_id))?;
        let mut session = shared.lock().await;

        if !session.is_alive() {
            drop(session);
            let mut sessions = SESSIONS.write().await;
            if sessions
                .get(session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &shared))
            {
                sessions.remove(session_id);
            }
            release_session_resource(session_id);
            bail!("Session {} has terminated", session_id);
        }

        let result = session.send_command(command, args.timeout_secs).await?;

        Ok(serde_json::json!({
            "session_id": session_id,
            "stdout": result.stdout,
            "stderr": result.stderr,
            "exit_code": result.exit_code,
            "timed_out": result.timed_out
        }))
    }

    async fn handle_read(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'read' action")?;

        let shared = SESSIONS
            .read()
            .await
            .get(session_id)
            .cloned()
            .context(format!("No session found with id: {}", session_id))?;
        let mut session = shared.lock().await;

        let output = session.read_output().await?;

        Ok(serde_json::json!({
            "session_id": session_id,
            "output": output
        }))
    }

    async fn handle_resize(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'resize' action")?;

        let cols = args.cols.unwrap_or(80);
        let rows = args.rows.unwrap_or(24);

        let shared = SESSIONS
            .read()
            .await
            .get(session_id)
            .cloned()
            .context(format!("No session found with id: {}", session_id))?;
        let mut session = shared.lock().await;

        session.resize(cols, rows);

        Ok(serde_json::json!({
            "session_id": session_id,
            "cols": cols,
            "rows": rows,
            "status": "resized"
        }))
    }

    async fn handle_status(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'status' action")?;

        let shared = SESSIONS
            .read()
            .await
            .get(session_id)
            .cloned()
            .context(format!("No session found with id: {}", session_id))?;
        let mut session = shared.lock().await;

        let alive = session.is_alive();
        let idle_secs = session.last_activity.elapsed().as_secs();
        let age_secs = session.created_at.elapsed().as_secs();

        Ok(serde_json::json!({
            "session_id": session_id,
            "alive": alive,
            "idle_secs": idle_secs,
            "age_secs": age_secs,
            "cwd": session.cwd.display().to_string(),
            "cols": session.cols,
            "rows": session.rows
        }))
    }

    async fn handle_close(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'close' action")?;

        let (shared, remaining_sessions) = {
            let mut sessions = SESSIONS.write().await;
            let shared = sessions
                .remove(session_id)
                .context(format!("No session found with id: {}", session_id))?;
            (shared, sessions.len())
        };
        let mut session = shared.lock().await;

        session.close().await;
        release_session_resource(session_id);

        Ok(serde_json::json!({
            "status": "closed",
            "session_id": session_id,
            "remaining_sessions": remaining_sessions
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/pty_shell/pty_shell_test.rs"]
mod tests;
