//! PTY-based interactive shell sessions.
//!
//! Provides persistent shell sessions that survive across multiple tool
//! invocations, unlike the one-shot [`ShellExec`](super::shell_exec::ShellExec).
//! Each session maintains a child shell process with piped stdin/stdout/stderr
//! and uses a completion marker to detect when a command has finished.

use super::Tool;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use uuid::Uuid;

/// Marker injected after every command to detect completion.
/// The shell will echo this string followed by the exit code.
const CMD_DONE_MARKER: &str = "__SELFWARE_CMD_DONE_";

/// Maximum output size returned per command (bytes).
const MAX_OUTPUT_BYTES: usize = 10_240;

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
static SESSIONS: Lazy<Arc<RwLock<HashMap<String, PtySession>>>> =
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// A persistent interactive shell session backed by a child process with
/// piped stdin, stdout, and stderr.
pub struct PtySession {
    /// The child shell process.
    child: Child,
    /// Writer to the child's stdin.
    stdin: tokio::process::ChildStdin,
    /// Buffered reader for the child's stdout.
    stdout: BufReader<tokio::process::ChildStdout>,
    /// Buffered reader for the child's stderr.
    stderr: BufReader<tokio::process::ChildStderr>,
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
    /// `shell` defaults to `$SHELL` or `/bin/bash` on Unix, `cmd` on Windows.
    pub async fn new(shell: Option<&str>) -> Result<Self> {
        let shell_path = shell
            .map(|s| s.to_string())
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| {
                if cfg!(target_os = "windows") {
                    "cmd".to_string()
                } else {
                    "/bin/bash".to_string()
                }
            });

        let mut cmd = Command::new(&shell_path);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        // Clear the inherited environment (SELFWARE_API_KEY, AWS_*, …) before
        // adding our own vars, matching shell_exec — an interactive shell must
        // not hand the agent's secrets to whatever the operator types.
        crate::safety::process_env::sanitize_command_env(&mut cmd);

        // Disable history and prompts to keep output clean.
        cmd.env("HISTFILE", "/dev/null")
            .env("PS1", "")
            .env("PS2", "")
            .env("TERM", "dumb");

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn shell: {}", shell_path))?;

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

        let mut session = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            stderr: BufReader::new(stderr),
            last_command_at: now - MIN_COMMAND_INTERVAL, // allow immediate first command
            created_at: now,
            last_activity: now,
            cols: 80,
            rows: 24,
        };

        // Drain any initial shell startup output (e.g., motd, bashrc messages).
        // We send a no-op command with our marker to synchronize.
        session.drain_startup().await?;

        Ok(session)
    }

    /// Send a synchronization marker after startup to consume any banner output.
    async fn drain_startup(&mut self) -> Result<()> {
        let marker_cmd = format!("echo {}{}_STARTUP__\n", CMD_DONE_MARKER, "0");
        self.stdin
            .write_all(marker_cmd.as_bytes())
            .await
            .context("Failed to write startup marker")?;
        self.stdin.flush().await?;

        let startup_marker = format!("{}0_STARTUP__", CMD_DONE_MARKER);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut line = String::new();

        loop {
            if Instant::now() > deadline {
                // Timed out waiting for startup marker; proceed anyway.
                break;
            }

            line.clear();
            let read_future = self.stdout.read_line(&mut line);
            match tokio::time::timeout(Duration::from_millis(500), read_future).await {
                Ok(Ok(0)) => break, // EOF
                Ok(Ok(_)) => {
                    if line.contains(&startup_marker) {
                        break;
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => {
                    // Timeout on this read; try a few more times.
                    continue;
                }
            }
        }

        Ok(())
    }

    /// Send a command and wait for its output.
    ///
    /// Injects a completion marker after the command and reads stdout until
    /// that marker appears (or a timeout is hit).
    pub async fn send_command(&mut self, cmd: &str, timeout_secs: u64) -> Result<CommandOutput> {
        // Rate limiting
        let elapsed = self.last_command_at.elapsed();
        if elapsed < MIN_COMMAND_INTERVAL {
            tokio::time::sleep(MIN_COMMAND_INTERVAL - elapsed).await;
        }

        self.last_command_at = Instant::now();
        self.last_activity = Instant::now();

        // Build the command sequence:
        //   <user command>
        //   __selfware_ec=$?
        //   echo __SELFWARE_CMD_DONE_${__selfware_ec}__
        let full_cmd = format!(
            "{}\n__selfware_ec=$?\necho {}${{__selfware_ec}}__\n",
            cmd, CMD_DONE_MARKER
        );

        self.stdin
            .write_all(full_cmd.as_bytes())
            .await
            .context("Failed to write command to shell stdin")?;
        self.stdin.flush().await?;

        let timeout = Duration::from_secs(timeout_secs.min(3600));
        let deadline = Instant::now() + timeout;
        let mut output_lines: Vec<String> = Vec::new();
        let mut exit_code: Option<i32> = None;
        let mut total_bytes = 0usize;
        let mut line = String::new();

        // Read until we see the completion marker.
        loop {
            if Instant::now() > deadline {
                return Ok(CommandOutput {
                    stdout: Self::collect_output(&output_lines),
                    stderr: String::new(),
                    exit_code: -1,
                    timed_out: true,
                });
            }

            line.clear();
            let remaining = deadline.saturating_duration_since(Instant::now());
            let read_future = self.stdout.read_line(&mut line);
            match tokio::time::timeout(remaining, read_future).await {
                Ok(Ok(0)) => {
                    // EOF — process terminated.
                    break;
                }
                Ok(Ok(_)) => {
                    let trimmed = line.trim_end();
                    // Check for our completion marker.
                    if let Some(ec) = Self::parse_marker(trimmed) {
                        exit_code = Some(ec);
                        break;
                    }
                    total_bytes += line.len();
                    if total_bytes <= MAX_OUTPUT_BYTES {
                        output_lines.push(trimmed.to_string());
                    }
                }
                Ok(Err(e)) => {
                    bail!("Error reading shell output: {}", e);
                }
                Err(_) => {
                    // Timeout
                    return Ok(CommandOutput {
                        stdout: Self::collect_output(&output_lines),
                        stderr: String::new(),
                        exit_code: -1,
                        timed_out: true,
                    });
                }
            }
        }

        // Drain any stderr that's accumulated.
        let stderr = self.drain_stderr().await;

        Ok(CommandOutput {
            stdout: Self::collect_output(&output_lines),
            stderr,
            exit_code: exit_code.unwrap_or(-1),
            timed_out: false,
        })
    }

    /// Try to parse the completion marker line, returning the exit code if matched.
    fn parse_marker(line: &str) -> Option<i32> {
        // Marker format: __SELFWARE_CMD_DONE_<exit_code>__
        let stripped = line.trim();
        if stripped.starts_with(CMD_DONE_MARKER) && stripped.ends_with("__") {
            let inner = &stripped[CMD_DONE_MARKER.len()..stripped.len() - 2];
            inner.parse::<i32>().ok()
        } else {
            None
        }
    }

    /// Collect output lines into a single string, stripping ANSI codes and
    /// truncating to the maximum size.
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
        let mut buf = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match tokio::time::timeout(Duration::from_millis(50), self.stderr.read_line(&mut line))
                .await
            {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(_)) => {
                    buf.push(line.trim_end().to_string());
                    if buf.len() > 200 {
                        break;
                    }
                }
            }
        }
        strip_ansi(&buf.join("\n"))
    }

    /// Read any pending output without sending a command.
    pub async fn read_output(&mut self) -> Result<String> {
        self.last_activity = Instant::now();
        let mut lines = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            match tokio::time::timeout(Duration::from_millis(100), self.stdout.read_line(&mut line))
                .await
            {
                Ok(Ok(0)) | Ok(Err(_)) | Err(_) => break,
                Ok(Ok(_)) => {
                    lines.push(line.trim_end().to_string());
                    if lines.len() > 500 {
                        break;
                    }
                }
            }
        }

        let stderr = self.drain_stderr().await;
        let mut output = strip_ansi(&lines.join("\n"));
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
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Terminate the session, killing the child process.
    pub async fn close(&mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
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

/// Validate a command against the shared dangerous-pattern blocklist.
fn check_dangerous_patterns(command: &str) -> Result<()> {
    if let Some(pattern) = super::find_dangerous_shell_pattern(command) {
        bail!("Blocked potentially dangerous shell pattern: {}", pattern);
    }
    Ok(())
}

/// Remove sessions that have been idle longer than [`IDLE_TIMEOUT`].
async fn cleanup_idle_sessions(sessions: &RwLock<HashMap<String, PtySession>>) {
    let mut map = sessions.write().await;
    let stale_ids: Vec<String> = map
        .iter()
        .filter(|(_, s)| s.last_activity.elapsed() > IDLE_TIMEOUT)
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale_ids {
        if let Some(mut session) = map.remove(&id) {
            session.close().await;
        }
    }
}

// ---------------------------------------------------------------------------
// Tool implementation
// ---------------------------------------------------------------------------

/// Interactive PTY shell tool that maintains persistent shell sessions.
pub struct PtyShellTool;

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
        "Interactive shell sessions that persist across invocations. \
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
                    "description": "Shell to use (for 'start' action; defaults to $SHELL or /bin/bash)"
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
        let mut sessions = SESSIONS.write().await;

        if sessions.len() >= MAX_SESSIONS {
            bail!(
                "Maximum number of concurrent sessions ({}) reached. \
                 Close an existing session first.",
                MAX_SESSIONS
            );
        }

        let session_id = Uuid::new_v4().to_string();
        let shell = args.shell.as_deref();
        let session = PtySession::new(shell).await?;

        sessions.insert(session_id.clone(), session);

        Ok(serde_json::json!({
            "status": "started",
            "session_id": session_id,
            "active_sessions": sessions.len()
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

        let mut sessions = SESSIONS.write().await;
        let session = sessions
            .get_mut(session_id)
            .context(format!("No session found with id: {}", session_id))?;

        if !session.is_alive() {
            sessions.remove(session_id);
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

        let mut sessions = SESSIONS.write().await;
        let session = sessions
            .get_mut(session_id)
            .context(format!("No session found with id: {}", session_id))?;

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

        let mut sessions = SESSIONS.write().await;
        let session = sessions
            .get_mut(session_id)
            .context(format!("No session found with id: {}", session_id))?;

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

        let mut sessions = SESSIONS.write().await;
        let session = sessions
            .get_mut(session_id)
            .context(format!("No session found with id: {}", session_id))?;

        let alive = session.is_alive();
        let idle_secs = session.last_activity.elapsed().as_secs();
        let age_secs = session.created_at.elapsed().as_secs();

        Ok(serde_json::json!({
            "session_id": session_id,
            "alive": alive,
            "idle_secs": idle_secs,
            "age_secs": age_secs,
            "cols": session.cols,
            "rows": session.rows
        }))
    }

    async fn handle_close(&self, args: PtyArgs) -> Result<Value> {
        let session_id = args
            .session_id
            .as_deref()
            .context("session_id is required for 'close' action")?;

        let mut sessions = SESSIONS.write().await;
        let mut session = sessions
            .remove(session_id)
            .context(format!("No session found with id: {}", session_id))?;

        session.close().await;

        Ok(serde_json::json!({
            "status": "closed",
            "session_id": session_id,
            "remaining_sessions": sessions.len()
        }))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/pty_shell/pty_shell_test.rs"]
mod tests;
