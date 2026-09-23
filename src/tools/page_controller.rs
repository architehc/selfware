#![allow(dead_code, unused_imports, unused_variables)]
//! Page Controller — full Playwright-based browser automation tool.
//!
//! Spawns a companion Node.js process (`scripts/playwright-bridge.js`) that
//! communicates over stdin/stdout using newline-delimited JSON (NDJSON).
//! This avoids needing native Rust Playwright bindings while providing the
//! full Playwright API surface for browser automation.
//!
//! Falls back to the existing `browser.rs` fetch tools if Playwright is
//! unavailable.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info, warn};

use super::net_policy;
use super::Tool;

/// The Playwright bridge script, embedded in the binary at compile time so
/// PageControl works even when scripts/ is not shipped (cargo install --git /
/// release archives). Extracted to ~/.selfware/bridge/ on demand.
const EMBEDDED_BRIDGE_JS: &str = include_str!("../../scripts/playwright-bridge.js");

// ============================================================================
// Constants
// ============================================================================

/// Default per-action timeout in milliseconds.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// All valid actions for the page_control tool.
const VALID_ACTIONS: &[&str] = &[
    // Navigation
    "goto",
    "back",
    "forward",
    "reload",
    "wait_for",
    // Interaction
    "click",
    "type",
    "fill",
    "select",
    "check",
    "uncheck",
    "hover",
    "press",
    // Content extraction
    "text",
    "html",
    "attribute",
    "value",
    "count",
    "visible",
    // Page info
    "title",
    "url",
    "screenshot",
    "pdf",
    // JavaScript
    "evaluate",
    "evaluate_handle",
    // Multi-tab
    "new_tab",
    "switch_tab",
    "close_tab",
    "list_tabs",
    // Lifecycle
    "shutdown",
];

// ============================================================================
// Bridge Process Communication
// ============================================================================

/// A bridge response from the Node.js playwright-bridge process.
#[derive(Debug, serde::Deserialize)]
struct BridgeResponse {
    id: Option<u64>,
    success: bool,
    result: Option<Value>,
    error: Option<String>,
}

/// Maximum number of stderr lines from the bridge retained before the drainer
/// discards the rest. Chromium's logging can be extremely verbose; the pipe is
/// still drained (so the bridge never blocks on a full stderr pipe), only the
/// captured copy is bounded.
const MAX_BRIDGE_STDERR_LINES: usize = 400;

/// Typed, infrastructure-level failure of the Playwright bridge process (same
/// shape as `mcp::transport::McpTransportError`): the bridge died, its pipes
/// broke, or it never answered — as opposed to a Playwright error the bridge
/// reported on purpose (`success: false`). Cloneable so one death fails every
/// pending command with the same cause.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BridgeTransportError {
    /// The bridge's stdout reached EOF: Node exited or closed its output.
    #[error("Playwright bridge exited / closed its output{detail}")]
    Exited { detail: String },
    /// Reading the bridge's stdout failed.
    #[error("Playwright bridge output could not be read: {message}")]
    ReadFailed { message: String },
    /// Writing to the bridge's stdin hit a broken pipe (the bridge exited or
    /// closed its stdin). SIGPIPE is ignored, so this surfaces as EPIPE.
    #[error("Playwright bridge input is closed (broken pipe){detail}")]
    BrokenPipe { detail: String },
    /// Writing to the bridge's stdin failed for another reason.
    #[error("Playwright bridge write failed: {message}")]
    WriteFailed { message: String },
    /// The bridge did not answer in time (the bridge is NOT marked dead).
    #[error("Playwright-bridge command timed out after {timeout_ms}ms")]
    TimedOut { timeout_ms: u64 },
}

type BridgePending = HashMap<u64, oneshot::Sender<BridgeResponse>>;

/// `None` while the bridge is usable, otherwise the first fatal cause. Checked
/// under the pending-map lock so no command registers after the drain.
type BridgeDeadState = Arc<std::sync::Mutex<Option<BridgeTransportError>>>;

/// Last few bridge stderr lines, used to explain an exit
/// ("Cannot find module 'playwright'", a Node stack trace, ...).
const BRIDGE_STDERR_TAIL_LINES: usize = 5;
const BRIDGE_STDERR_TAIL_LINE_MAX_CHARS: usize = 300;

type BridgeStderrTail = Arc<std::sync::Mutex<std::collections::VecDeque<String>>>;

/// Record the bridge as dead (first cause wins) and fail every pending
/// command immediately by dropping its response channel.
async fn mark_bridge_dead(
    pending: &Mutex<BridgePending>,
    dead: &BridgeDeadState,
    cause: BridgeTransportError,
) {
    let mut pending = pending.lock().await;
    {
        let mut slot = dead.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(cause);
        }
    }
    pending.clear();
}

fn bridge_dead_cause(dead: &BridgeDeadState) -> Option<BridgeTransportError> {
    dead.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Describe why the bridge's pipes closed: exit status (if Node exited within
/// a short grace period) and the tail of its stderr. Waits (<=300ms) for both
/// the exit status and the stderr drain.
async fn describe_bridge_exit(
    child: &Mutex<Child>,
    stderr_tail: &BridgeStderrTail,
    stderr_done: &std::sync::atomic::AtomicBool,
) -> String {
    let mut status = None;
    for _ in 0..30 {
        if status.is_none() {
            if let Ok(mut c) = child.try_lock() {
                if let Ok(Some(s)) = c.try_wait() {
                    status = Some(s);
                }
            }
        }
        if status.is_some() && stderr_done.load(Ordering::Acquire) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let mut detail = String::new();
    if let Some(s) = status {
        detail.push_str(&format!(" ({s})"));
    }
    let tail: Vec<String> = stderr_tail
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .cloned()
        .collect();
    if !tail.is_empty() {
        detail.push_str(&format!("; stderr: {}", tail.join(" | ")));
    }
    detail
}

/// Manages the lifecycle of the playwright-bridge.js child process.
struct PlaywrightBridge {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<BridgePending>>,
    /// First fatal cause once the bridge died (EOF, read/write failure);
    /// later commands fail fast with it instead of waiting for a timeout.
    dead: BridgeDeadState,
    stderr_tail: BridgeStderrTail,
    stderr_done: Arc<std::sync::atomic::AtomicBool>,
    next_id: AtomicU64,
    child: Arc<Mutex<Child>>,
    reader_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Task that drains the bridge's stderr concurrently, so a >64KB burst of
    /// Chromium logging cannot fill the pipe and stall Node (2026-09-21
    /// review: the pipe was never read — the parent stalled, the child
    /// stalled, the agent deadlocked until the command timed out).
    stderr_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Process-group id of the bridge, captured at spawn. On unix the bridge
    /// is spawned with `process_group(0)`, making it the leader of its own
    /// process group, so the pgid equals the bridge pid and the group covers
    /// the Chromium children the bridge spawns. Kept separately from the
    /// [`Child`] handle because `start_kill()` / `kill()` only signal the
    /// direct Node process (2026-09-21 review: killing Node reparented the
    /// still-running Chromium to PID 1); the group kill reaches the whole tree.
    #[cfg(unix)]
    pgid: Option<u32>,
}

impl PlaywrightBridge {
    /// Build a sanitized `npm`/`npx` command for the Playwright bridge
    /// install: the installer runs against project-controlled package
    /// metadata and downloads dependencies, so it must not inherit host
    /// credentials (see `safety::process_env`). No package install here is
    /// credential-mediated, so nothing is preserved.
    fn bridge_installer_command(program: &str, dir: &std::path::Path) -> std::process::Command {
        let mut cmd = std::process::Command::new(program);
        crate::safety::process_env::sanitize_std_command_env_preserve(&mut cmd, &[]);
        cmd.current_dir(dir);
        cmd
    }

    /// Spawn the playwright-bridge.js process.
    async fn spawn() -> Result<Self> {
        let bridge_script = Self::find_bridge_script()?;
        Self::ensure_bridge_dependencies(&bridge_script)?;
        Self::spawn_with_script(&bridge_script, &[]).await
    }

    /// Testable core: spawn the bridge with an explicit script and optional
    /// extra argv. Skips script discovery and the `npm install` dependency
    /// bootstrap — tests point this at a stub script so no real browser or
    /// package install is ever touched.
    async fn spawn_with_script(
        bridge_script: &std::path::Path,
        extra_args: &[&str],
    ) -> Result<Self> {
        let mut args: Vec<&str> = Vec::with_capacity(extra_args.len() + 1);
        let script = bridge_script.to_string_lossy();
        args.push(&script);
        args.extend_from_slice(extra_args);
        Self::spawn_program("node", &args).await
    }

    /// Spawn `program args...` as the bridge process. Production always runs
    /// `node <bridge script>`; tests substitute `sh -c ...` stubs to exercise
    /// the transport (exit, broken pipe) without Node or a browser.
    async fn spawn_program(program: &str, args: &[&str]) -> Result<Self> {
        info!("Spawning playwright-bridge: {} {:?}", program, args);

        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Clear the inherited environment first; the bridge loads and executes
        // page content, so it must not carry the agent's secrets. The specific
        // SELFWARE_* vars the bridge needs are re-added explicitly below.
        crate::safety::process_env::sanitize_command_env(&mut cmd);

        // Run the bridge in its own process group so teardown can kill the
        // ENTIRE tree (Node + the Chromium children it spawns), not just the
        // direct Node process — `child.start_kill()`/`kill()` only signal
        // Node, and an orphaned Chromium keeps running after the bridge is
        // gone (2026-09-21 review: killing Node left Chromium reparented to
        // PID 1, running forever).
        #[cfg(unix)]
        cmd.process_group(0);

        // Forward the private-network env var
        if let Ok(val) = std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK") {
            cmd.env("SELFWARE_ALLOW_PRIVATE_NETWORK", val);
        }
        if let Ok(val) = std::env::var("SELFWARE_PLAYWRIGHT_NODE_PATH") {
            let merged = match std::env::var("NODE_PATH") {
                Ok(existing) if !existing.is_empty() => format!("{}:{}", val, existing),
                _ => val,
            };
            cmd.env("NODE_PATH", merged);
        }
        for key in [
            "SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH",
            "SELFWARE_CHROME_EXECUTABLE_PATH",
        ] {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }
        // The agent's workspace root (not the process cwd: entering a git
        // worktree moves only the agent's root).
        cmd.env(
            "SELFWARE_WORKSPACE_ROOT",
            crate::tools::workspace_root::current_path(),
        );

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn playwright-bridge: {program} {args:?}"))?;

        // Capture the process-group id at spawn. The bridge child runs in its
        // own process group (see `process_group(0)` above), so the pgid equals
        // the child's pid. It must be captured here rather than re-derived
        // later from the Child handle: `start_kill`/`kill` only signal the
        // direct Node process, and once Node is reaped `Child::id()` returns
        // None — the surviving Chromium group members would be unreachable.
        #[cfg(unix)]
        let pgid = child.id();

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture playwright-bridge stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture playwright-bridge stdout")?;
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture playwright-bridge stderr")?;

        let pending: Arc<Mutex<BridgePending>> = Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);
        let dead: BridgeDeadState = Arc::new(std::sync::Mutex::new(None));
        let stderr_tail: BridgeStderrTail = Arc::new(std::sync::Mutex::new(Default::default()));
        let stderr_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let child = Arc::new(Mutex::new(child));

        let reader_dead = Arc::clone(&dead);
        let reader_child = Arc::clone(&child);
        let reader_tail = Arc::clone(&stderr_tail);
        let reader_stderr_done = Arc::clone(&stderr_done);

        // Background reader task — reads NDJSON responses from stdout. On EOF
        // or a read error the bridge is marked dead and every pending command
        // fails immediately with the cause (exit status + stderr tail),
        // instead of each one waiting out its timeout.
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            loop {
                let line = match lines.next_line().await {
                    Ok(Some(line)) => line,
                    Ok(None) => {
                        let detail =
                            describe_bridge_exit(&reader_child, &reader_tail, &reader_stderr_done)
                                .await;
                        warn!("Playwright bridge closed its output{}", detail);
                        mark_bridge_dead(
                            &pending_clone,
                            &reader_dead,
                            BridgeTransportError::Exited { detail },
                        )
                        .await;
                        break;
                    }
                    Err(e) => {
                        warn!("Playwright bridge stdout read error: {}", e);
                        mark_bridge_dead(
                            &pending_clone,
                            &reader_dead,
                            BridgeTransportError::ReadFailed {
                                message: e.to_string(),
                            },
                        )
                        .await;
                        break;
                    }
                };
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<BridgeResponse>(&line) {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            let mut pending = pending_clone.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let _ = tx.send(response);
                            } else {
                                debug!(
                                    "Received bridge response for unknown ID {}: {:?}",
                                    id, response
                                );
                            }
                        } else {
                            debug!("Bridge notification (no id): {:?}", response);
                        }
                    }
                    Err(e) => {
                        debug!("Non-JSON line from playwright-bridge: {}", line);
                    }
                }
            }

            debug!("Playwright-bridge stdout reader exited");
        });

        // Background stderr drain task. The bridge's stderr pipe was never
        // read, so a >64KB burst of Chromium logging filled the pipe and
        // blocked Node's write(2) — the parent stalled waiting for NDJSON
        // that never arrived (2026-09-21 review). The pipe is drained
        // concurrently now; only the first [`MAX_BRIDGE_STDERR_LINES`] lines
        // are retained (for diagnostics), everything beyond the cap is still
        // consumed, never buffered.
        let drain_tail = Arc::clone(&stderr_tail);
        let drain_done = Arc::clone(&stderr_done);
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut retained = 0usize;
            while let Ok(Some(line)) = lines.next_line().await {
                if retained < MAX_BRIDGE_STDERR_LINES {
                    retained += 1;
                    warn!("playwright-bridge stderr: {}", line);
                }
                // Beyond the cap: keep draining so the pipe never fills up.
                // A short rolling tail explains an exit in error messages.
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    let mut tail = drain_tail.lock().unwrap_or_else(|p| p.into_inner());
                    if tail.len() == BRIDGE_STDERR_TAIL_LINES {
                        tail.pop_front();
                    }
                    tail.push_back(
                        trimmed
                            .chars()
                            .take(BRIDGE_STDERR_TAIL_LINE_MAX_CHARS)
                            .collect(),
                    );
                }
            }
            drain_done.store(true, Ordering::Release);
            debug!("Playwright-bridge stderr reader exited");
        });

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            dead,
            stderr_tail,
            stderr_done,
            next_id: AtomicU64::new(1),
            child,
            reader_handle: Mutex::new(Some(reader_handle)),
            stderr_handle: Mutex::new(Some(stderr_handle)),
            #[cfg(unix)]
            pgid,
        })
    }

    /// Send a command to the bridge and wait for a response.
    async fn send(&self, mut command: Value, timeout_ms: u64) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        command
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("Command must be a JSON object"))?
            .insert("id".to_string(), json!(id));

        let mut bytes = serde_json::to_vec(&command)?;
        bytes.push(b'\n');

        // Register pending response channel before sending. The dead check
        // shares the pending lock with the reader's drain, so a command can
        // never register after the bridge died and then wait out its timeout.
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if let Some(cause) = self.dead_cause() {
                return Err(anyhow::Error::new(cause));
            }
            pending.insert(id, tx);
        }

        // Send command; on any write failure, drop the now-orphaned pending
        // entry so the map doesn't leak one slot per failed request, and mark
        // the bridge dead (a broken pipe means it is gone).
        {
            let mut stdin = self.stdin.lock().await;
            let written = match stdin.write_all(&bytes).await {
                Ok(()) => stdin.flush().await,
                Err(e) => Err(e),
            };
            drop(stdin);
            if let Err(e) = written {
                self.pending.lock().await.remove(&id);
                return Err(anyhow::Error::new(self.write_failed(e).await));
            }
        }

        debug!("Sent bridge command id={}: {}", id, command);

        // Wait for response with timeout
        let timeout_dur = std::time::Duration::from_millis(timeout_ms + 5000);
        let response = match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                // Sender dropped without replying: the reader marked the
                // bridge dead. Report the recorded cause.
                self.pending.lock().await.remove(&id);
                let cause = self
                    .dead_cause()
                    .unwrap_or_else(|| BridgeTransportError::Exited {
                        detail: String::new(),
                    });
                return Err(anyhow::Error::new(cause));
            }
            Err(_) => {
                // Timed out: drop the pending entry so it doesn't leak a slot.
                self.pending.lock().await.remove(&id);
                return Err(anyhow::Error::new(BridgeTransportError::TimedOut {
                    timeout_ms,
                }));
            }
        };

        if !response.success {
            let error_msg = response
                .error
                .unwrap_or_else(|| "Unknown bridge error".to_string());
            bail!("Playwright-bridge error: {}", error_msg);
        }

        Ok(response.result.unwrap_or(json!(null)))
    }

    /// The recorded fatal cause, if the bridge is dead.
    fn dead_cause(&self) -> Option<BridgeTransportError> {
        bridge_dead_cause(&self.dead)
    }

    /// Classify a stdin write failure, mark the bridge dead, and return the
    /// recorded cause (the reader's `Exited` may win the race — equally fatal,
    /// and it carries the exit status).
    async fn write_failed(&self, err: std::io::Error) -> BridgeTransportError {
        let cause = if err.kind() == std::io::ErrorKind::BrokenPipe {
            BridgeTransportError::BrokenPipe {
                detail: describe_bridge_exit(&self.child, &self.stderr_tail, &self.stderr_done)
                    .await,
            }
        } else {
            BridgeTransportError::WriteFailed {
                message: err.to_string(),
            }
        };
        mark_bridge_dead(&self.pending, &self.dead, cause.clone()).await;
        self.dead_cause().unwrap_or(cause)
    }

    /// SIGKILL every member of the bridge's process group.
    ///
    /// Uses the pgid captured at spawn rather than the live child pid: it
    /// reaches the Node process AND the Chromium children it spawned, and it
    /// still works after the direct Node child has been reaped (`Child::id()`
    /// returns `None` then, but surviving group members — orphaned Chromium —
    /// would otherwise be unreachable).
    fn kill_bridge_group(&self) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(pgid as i32), Signal::SIGKILL);
        }
    }

    /// Shut down the bridge process gracefully.
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down playwright-bridge");

        // Send shutdown command (a dead bridge cannot answer — skip it).
        if self.dead_cause().is_none() {
            let _ = self.send(json!({"action": "shutdown"}), 5000).await;

            // Give it a moment, then force kill the whole process group (Node +
            // Chromium children — `child.kill()` alone would orphan the latter).
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        self.kill_bridge_group();
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        let _ = child.wait().await;

        // Cancel reader tasks
        let mut handle = self.reader_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }
        let mut handle = self.stderr_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }

        Ok(())
    }

    /// Resolve the Playwright bridge script to run with Node.
    ///
    /// SECURITY: the bridge compiled into the binary (`EMBEDDED_BRIDGE_JS`,
    /// extracted to the user-owned `~/.selfware/bridge/`) is the ONLY production
    /// source. Previously this scanned `./scripts/playwright-bridge.js` (the
    /// current directory) and exe-relative dirs first, so an untrusted checkout
    /// could drop a malicious `scripts/playwright-bridge.js` and have it executed
    /// with Node on an ordinary — or Auto/YOLO auto-approved — browser action:
    /// arbitrary code execution from repository contents.
    ///
    /// A developer may point at a local bridge ONLY via the explicit
    /// `SELFWARE_PLAYWRIGHT_BRIDGE` environment variable (an operator decision,
    /// never repository-controlled). No directory is auto-discovered.
    fn find_bridge_script() -> Result<PathBuf> {
        if let Ok(override_path) = std::env::var("SELFWARE_PLAYWRIGHT_BRIDGE") {
            let p = PathBuf::from(override_path);
            if p.is_file() {
                return Ok(p);
            }
            bail!(
                "SELFWARE_PLAYWRIGHT_BRIDGE is set but is not a readable file: {}",
                p.display()
            );
        }
        // Default and only production path: the embedded bridge.
        Self::extract_embedded_bridge()
    }

    /// Extract the embedded bridge to `~/.selfware/bridge/` and return its path.
    fn extract_embedded_bridge() -> Result<PathBuf> {
        let dir = dirs::home_dir()
            .context("cannot resolve home directory for the Playwright bridge")?
            .join(".selfware")
            .join("bridge");
        Self::extract_embedded_bridge_to(&dir)
    }

    /// Testable core: write the embedded bridge (and a package.json declaring the
    /// playwright dependency) into `dir`, returning the script path. Rewrites the
    /// script only when missing or stale so upgrades re-extract.
    fn extract_embedded_bridge_to(dir: &std::path::Path) -> Result<PathBuf> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("creating bridge dir {}", dir.display()))?;
        let script = dir.join("playwright-bridge.js");
        let needs_write = match std::fs::read_to_string(&script) {
            Ok(existing) => existing != EMBEDDED_BRIDGE_JS,
            Err(_) => true,
        };
        if needs_write {
            std::fs::write(&script, EMBEDDED_BRIDGE_JS)
                .with_context(|| format!("writing bridge to {}", script.display()))?;
        }
        // Drop a package.json so `npm install` in this dir pulls Playwright.
        let pkg = dir.join("package.json");
        if !pkg.exists() {
            let _ = std::fs::write(
                &pkg,
                "{\n  \"name\": \"selfware-playwright-bridge\",\n  \"private\": true,\n  \"dependencies\": { \"playwright\": \"*\" }\n}\n",
            );
        }
        Ok(script)
    }

    /// Ensure the extracted bridge's Node dependencies (Playwright) are present.
    /// On the first run the bridge dir has a package.json but no node_modules, so
    /// `require('playwright')` fails; run `npm install` (and fetch the Chromium
    /// browser) once. Skipped when the operator configured an existing install
    /// via SELFWARE_PLAYWRIGHT_NODE_PATH, or when playwright is already present.
    fn ensure_bridge_dependencies(bridge_script: &std::path::Path) -> Result<()> {
        // Operator pointed us at an existing Playwright install — trust it.
        if std::env::var_os("SELFWARE_PLAYWRIGHT_NODE_PATH").is_some() {
            return Ok(());
        }
        let dir = bridge_script
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        if dir.join("node_modules").join("playwright").exists() {
            return Ok(());
        }
        eprintln!(
            "Installing Playwright bridge dependencies in {} (first run — may take a minute)...",
            dir.display()
        );
        let status = Self::bridge_installer_command("npm", dir)
            .arg("install")
            .status()
            .with_context(|| {
                "running `npm install` for the Playwright bridge (is Node.js + npm installed?)"
                    .to_string()
            })?;
        if !status.success() {
            bail!(
                "`npm install` failed in {} — install Node.js + npm, or set \
                 SELFWARE_PLAYWRIGHT_NODE_PATH to an existing Playwright install",
                dir.display()
            );
        }
        // Fetch the Chromium browser binary too (best-effort; the agent gets a
        // clear runtime error from the bridge if it is still missing).
        let _ = Self::bridge_installer_command("npx", dir)
            .args(["playwright", "install", "chromium"])
            .status();
        Ok(())
    }
}

impl Drop for PlaywrightBridge {
    fn drop(&mut self) {
        // Owned teardown: if the bridge is dropped without an explicit async
        // shutdown() (e.g. an error path), still reap the browser child process
        // and its reader tasks so they cannot leak. Best-effort, synchronous —
        // no await, so use try_lock + start_kill (SIGKILL). The process GROUP
        // is killed first so the Chromium children spawn-killed alongside the
        // Node process instead of being orphaned to PID 1 (the 2026-09-21
        // finding that `start_kill()` alone only signals Node).
        self.kill_bridge_group();
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        }
        if let Ok(mut handle) = self.reader_handle.try_lock() {
            if let Some(h) = handle.take() {
                h.abort();
            }
        }
        if let Ok(mut handle) = self.stderr_handle.try_lock() {
            if let Some(h) = handle.take() {
                h.abort();
            }
        }
    }
}

// ============================================================================
// URL Safety Validation (Rust-side pre-check)
// ============================================================================

/// Validate a URL for safety before sending it to the bridge.
/// Allows workspace-local file:// URLs and localhost, while still blocking
/// arbitrary private-network targets unless explicitly overridden.
fn validate_url(url: &str) -> Result<()> {
    validate_url_with_allow_private(
        url,
        std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1",
    )
}

fn validate_url_with_allow_private(url: &str, allow_private: bool) -> Result<()> {
    let parsed = url::Url::parse(url).context("Invalid URL")?;

    if parsed.scheme() == "file" {
        return validate_file_url(&parsed);
    }

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        bail!(
            "Only http://, https://, and workspace file:// URLs are allowed, got {}://",
            parsed.scheme()
        );
    }

    // Delegate http/https validation to the shared net_policy module.
    // The shared function handles localhost detection, private-IP blocking,
    // and the allow_private override.
    net_policy::validate_url_target(&parsed, allow_private)?;

    // Additional DNS-rebinding check: resolve the hostname and reject if any
    // resolved address is private. (The HTTP tool relies on PinnedDnsResolver
    // for this at connection time; here we do it eagerly since the Playwright
    // bridge doesn't go through our resolver.)
    if !allow_private {
        if let Some(host) = parsed.host_str() {
            if host.parse::<IpAddr>().is_err() && !net_policy::is_private_network_host(host) {
                let port = parsed.port_or_known_default().unwrap_or(80);
                if let Ok(addrs) = (host, port).to_socket_addrs() {
                    for addr in addrs {
                        if net_policy::is_private_or_internal_ip(&addr.ip()) {
                            bail!(
                                "DNS rebinding blocked: {} resolves to private address {}",
                                host,
                                addr.ip()
                            );
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn validate_file_url(parsed: &url::Url) -> Result<()> {
    let path = parsed
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("file:// URL must point to a local absolute path"))?;

    let workspace_root = crate::tools::workspace_root::current_path()
        .canonicalize()
        .context("Failed to canonicalize current workspace directory")?;

    let target = canonicalize_existing_path(&path)?;

    if !target.starts_with(&workspace_root) {
        bail!(
            "Blocked file:// URL outside workspace: {}",
            target.display()
        );
    }

    Ok(())
}

fn canonicalize_existing_path(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        bail!("file:// target does not exist: {}", path.display());
    }

    path.canonicalize()
        .with_context(|| format!("Failed to canonicalize {}", path.display()))
}

// Private-IP checking is now delegated to `net_policy::is_private_or_internal_ip`.

// ============================================================================
// PageController Struct
// ============================================================================

/// A session-based Playwright browser controller.
///
/// Manages a headless Chromium instance via the playwright-bridge.js child
/// process. Supports multiple pages/tabs, navigation, interaction, content
/// extraction, screenshots, PDFs, and JavaScript evaluation.
pub struct PageController {
    bridge: Arc<Mutex<Option<PlaywrightBridge>>>,
}

impl PageController {
    /// Create a new PageController. The browser is lazily spawned on first use.
    pub fn new() -> Self {
        Self {
            bridge: Arc::new(Mutex::new(None)),
        }
    }

    /// Ensure the bridge is running, spawning it if necessary.
    async fn ensure_bridge(&self) -> Result<()> {
        let mut bridge = self.bridge.lock().await;
        if let Some(cause) = bridge.as_ref().and_then(|b| b.dead_cause()) {
            // The previous bridge died (its pending command already failed
            // with the cause); tear it down and start a fresh one.
            warn!("Restarting Playwright bridge after: {cause}");
            if let Some(old) = bridge.take() {
                let _ = old.shutdown().await;
            }
        }
        if bridge.is_none() {
            *bridge = Some(PlaywrightBridge::spawn().await?);
        }
        Ok(())
    }

    /// Send a command to the bridge.
    async fn send_command(&self, command: Value, timeout_ms: u64) -> Result<Value> {
        self.ensure_bridge().await?;
        let bridge = self.bridge.lock().await;
        let bridge = bridge
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Bridge not initialized"))?;
        bridge.send(command, timeout_ms).await
    }

    /// Shut down the bridge and browser.
    pub async fn shutdown(&self) -> Result<()> {
        let mut bridge = self.bridge.lock().await;
        if let Some(b) = bridge.take() {
            b.shutdown().await?;
        }
        Ok(())
    }
}

impl Default for PageController {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// page_control Tool Implementation
// ============================================================================

/// Comprehensive browser automation tool using Playwright.
///
/// Supports navigation, interaction, content extraction, screenshots,
/// JavaScript evaluation, and multi-tab management via a single tool
/// with an `action` parameter.
pub struct PageControlTool {
    controller: PageController,
}

impl PageControlTool {
    pub fn new() -> Self {
        Self {
            controller: PageController::new(),
        }
    }

    /// Shut down the underlying browser and bridge process.
    pub async fn shutdown(&self) -> Result<()> {
        self.controller.shutdown().await
    }
}

impl Default for PageControlTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PageControlTool {
    fn name(&self) -> &str {
        "page_control"
    }

    fn description(&self) -> &str {
        "Full browser automation via Playwright. Supports navigation (goto, back, forward, \
         reload, wait_for), interaction (click, type, fill, select, check, uncheck, hover, \
         press), content extraction (text, html, attribute, value, count, visible), page info \
         (title, url, screenshot, pdf), JavaScript (evaluate, evaluate_handle), and multi-tab \
         management (new_tab, switch_tab, close_tab, list_tabs). Requires Node.js and Playwright."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": VALID_ACTIONS,
                    "description": "The browser action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL for goto, wait_for, or new_tab actions"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector for interaction/extraction actions"
                },
                "text": {
                    "type": "string",
                    "description": "Text for type/fill actions"
                },
                "value": {
                    "type": "string",
                    "description": "Value for select action"
                },
                "values": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Multiple values for select action"
                },
                "key": {
                    "type": "string",
                    "description": "Key for press action (e.g. Enter, Tab, Escape)"
                },
                "name": {
                    "type": "string",
                    "description": "Attribute name for attribute action"
                },
                "expression": {
                    "type": "string",
                    "description": "JavaScript expression for evaluate/evaluate_handle"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 30000)"
                },
                "tab_index": {
                    "type": "integer",
                    "description": "Tab index for switch_tab action"
                },
                "path": {
                    "type": "string",
                    "description": "Output path for screenshot/pdf"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Full page screenshot (default: false)"
                },
                "all": {
                    "type": "boolean",
                    "description": "Get all matching elements for text action"
                },
                "outer": {
                    "type": "boolean",
                    "description": "Get outerHTML instead of innerHTML"
                },
                "wait_until": {
                    "type": "string",
                    "enum": ["load", "domcontentloaded", "networkidle", "commit"],
                    "description": "Wait until state for goto (default: load)"
                },
                "load_state": {
                    "type": "string",
                    "enum": ["load", "domcontentloaded", "networkidle"],
                    "description": "Load state for wait_for action"
                },
                "state": {
                    "type": "string",
                    "enum": ["visible", "hidden", "attached", "detached"],
                    "description": "Element state for wait_for action"
                },
                "button": {
                    "type": "string",
                    "enum": ["left", "right", "middle"],
                    "description": "Mouse button for click (default: left)"
                },
                "click_count": {
                    "type": "integer",
                    "description": "Number of clicks (1=single, 2=double, 3=triple)"
                },
                "delay": {
                    "type": "integer",
                    "description": "Delay between keystrokes in ms for type action"
                },
                "format": {
                    "type": "string",
                    "description": "Paper format for pdf (e.g. A4, Letter)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("action is required"))?;

        // Validate action
        if !VALID_ACTIONS.contains(&action) {
            bail!(
                "Unknown action '{}'. Valid actions: {}",
                action,
                VALID_ACTIONS.join(", ")
            );
        }

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);

        // Validate URL if present (Rust-side pre-check before sending to bridge)
        if let Some(url) = args.get("url").and_then(|v| v.as_str()) {
            if action == "goto" || action == "new_tab" {
                validate_url(url)?;
            }
        }

        if matches!(action, "screenshot" | "pdf") {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                validate_page_output_path(path, self.name())?;
                ensure_page_output_parent(std::path::Path::new(path)).await?;
            }
        }

        // Build the command object to send to the bridge.
        // We forward the entire args object; the bridge picks the fields it needs.
        let mut command = args.clone();
        // Ensure action is present (it always is, but be safe)
        if let Some(obj) = command.as_object_mut() {
            obj.insert("action".to_string(), json!(action));
        }

        // Send to bridge and get result
        let result = self.controller.send_command(command, timeout_ms).await;

        match result {
            Ok(value) => Ok(json!({
                "success": true,
                "action": action,
                "result": value
            })),
            Err(e) => {
                // Return error as structured JSON rather than propagating
                // so the agent can see what went wrong and retry.
                Ok(json!({
                    "success": false,
                    "action": action,
                    "error": e.to_string()
                }))
            }
        }
    }
}

fn validate_page_output_path(output_path: &str, tool_name: &str) -> Result<()> {
    let safety = crate::tools::file::resolve_safety_config(None);
    crate::tools::file::validate_tool_path(output_path, &safety)
        .with_context(|| format!("{tool_name} output path validation failed"))
}

async fn ensure_page_output_parent(output_path: &std::path::Path) -> Result<()> {
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create page output dir {}", parent.display()))?;
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/tools/page_controller/page_controller_test.rs"]
mod tests;
