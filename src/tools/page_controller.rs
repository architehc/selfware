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

/// Manages the lifecycle of the playwright-bridge.js child process.
struct PlaywrightBridge {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BridgeResponse>>>>,
    next_id: AtomicU64,
    child: Arc<Mutex<Child>>,
    reader_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl PlaywrightBridge {
    /// Spawn the playwright-bridge.js process.
    async fn spawn() -> Result<Self> {
        let bridge_script = Self::find_bridge_script()?;
        Self::ensure_bridge_dependencies(&bridge_script)?;

        info!("Spawning playwright-bridge: {}", bridge_script.display());

        let mut cmd = Command::new("node");
        cmd.arg(&bridge_script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Clear the inherited environment first; the bridge loads and executes
        // page content, so it must not carry the agent's secrets. The specific
        // SELFWARE_* vars the bridge needs are re-added explicitly below.
        crate::safety::process_env::sanitize_command_env(&mut cmd);

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
        if let Ok(workspace_root) = std::env::current_dir() {
            cmd.env("SELFWARE_WORKSPACE_ROOT", workspace_root);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn playwright-bridge: {:?}", bridge_script))?;

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture playwright-bridge stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture playwright-bridge stdout")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BridgeResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);

        // Background reader task — reads NDJSON responses from stdout
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
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

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            child: Arc::new(Mutex::new(child)),
            reader_handle: Mutex::new(Some(reader_handle)),
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

        // Register pending response channel before sending
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Send command; on any write failure, drop the now-orphaned pending
        // entry so the map doesn't leak one slot per failed request.
        {
            let mut stdin = self.stdin.lock().await;
            if let Err(e) = stdin.write_all(&bytes).await {
                self.pending.lock().await.remove(&id);
                return Err(e).context("Failed to write to playwright-bridge stdin");
            }
            if let Err(e) = stdin.flush().await {
                self.pending.lock().await.remove(&id);
                return Err(e).context("Failed to flush playwright-bridge stdin");
            }
        }

        debug!("Sent bridge command id={}: {}", id, command);

        // Wait for response with timeout
        let timeout_dur = std::time::Duration::from_millis(timeout_ms + 5000);
        let response = match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                // Sender dropped without replying: drop the pending entry.
                self.pending.lock().await.remove(&id);
                bail!("Playwright-bridge response channel closed");
            }
            Err(_) => {
                // Timed out: drop the pending entry so it doesn't leak a slot.
                self.pending.lock().await.remove(&id);
                bail!("Playwright-bridge command timed out after {}ms", timeout_ms);
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

    /// Shut down the bridge process gracefully.
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down playwright-bridge");

        // Send shutdown command
        let _ = self.send(json!({"action": "shutdown"}), 5000).await;

        // Give it a moment, then force kill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let mut child = self.child.lock().await;
        let _ = child.kill().await;

        // Cancel reader task
        let mut handle = self.reader_handle.lock().await;
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
        let status = std::process::Command::new("npm")
            .arg("install")
            .current_dir(dir)
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
        let _ = std::process::Command::new("npx")
            .args(["playwright", "install", "chromium"])
            .current_dir(dir)
            .status();
        Ok(())
    }
}

impl Drop for PlaywrightBridge {
    fn drop(&mut self) {
        // Owned teardown: if the bridge is dropped without an explicit async
        // shutdown() (e.g. an error path), still reap the browser child process
        // and its reader task so they cannot leak. Best-effort, synchronous —
        // no await, so use try_lock + Child::start_kill (SIGKILL). Mirrors the
        // MCP StdioTransport Drop guard.
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        }
        if let Ok(mut handle) = self.reader_handle.try_lock() {
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

    let workspace_root = std::env::current_dir()
        .context("Failed to determine current workspace directory")?
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
