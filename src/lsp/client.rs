//! LSP client implementation.
//!
//! Manages connections to language servers (one per language), communicating
//! via JSON-RPC 2.0 over stdio with `Content-Length` header framing.

use crate::safety::process_env::SanitizedEnvExt;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A source code location (file, line, column).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

/// A symbol within a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    /// Human-readable kind: "function", "struct", "method", "class", etc.
    pub kind: String,
    pub line: u32,
    pub column: u32,
}

/// A diagnostic message from the language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    /// "error", "warning", "info", or "hint".
    pub severity: String,
    pub line: u32,
    pub column: u32,
}

// ---------------------------------------------------------------------------
// Language detection
// ---------------------------------------------------------------------------

/// Language identifier for LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
}

impl Language {
    /// LSP `languageId` string.
    pub fn id(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Go => "go",
        }
    }

    /// Detect language from file extension.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension()?.to_str()?;
        match ext {
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            "ts" | "tsx" => Some(Language::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "go" => Some(Language::Go),
            _ => None,
        }
    }
}

/// Server binary candidates for each language, tried in order.
fn server_candidates(lang: Language) -> Vec<(String, Vec<String>)> {
    let list: Vec<(&str, Vec<&str>)> = match lang {
        Language::Rust => vec![("rust-analyzer", vec![])],
        Language::Python => vec![("pyright-langserver", vec!["--stdio"]), ("pylsp", vec![])],
        Language::TypeScript | Language::JavaScript => {
            vec![("typescript-language-server", vec!["--stdio"])]
        }
        Language::Go => vec![("gopls", vec!["serve"])],
    };
    list.into_iter()
        .map(|(c, a)| (c.to_string(), a.into_iter().map(String::from).collect()))
        .collect()
}

/// Check if a binary is available on PATH.
async fn binary_exists(name: &str) -> bool {
    Command::new("which")
        .sanitized_env()
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Typed transport failures
// ---------------------------------------------------------------------------

/// Default time a `textDocument/*` / `workspace/*` request waits.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
/// Time the `initialize` handshake waits.
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(5);
/// Time the polite `shutdown` request waits before the server is killed.
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
/// Default bound on waiting for indexing to finish before retrying an empty
/// query once (see `query_settling_indexing`).
pub const DEFAULT_INDEXING_WAIT: Duration = Duration::from_secs(10);

/// Typed, infrastructure-level failure of a language-server connection (same
/// shape as `mcp::transport::McpTransportError`).
///
/// These are *transport* failures — the server process died, its pipes broke,
/// or it never answered — as opposed to JSON-RPC errors the server returned on
/// purpose. Cloneable so one death can fail every pending request with the
/// same cause. Callers can `downcast_ref::<LspTransportError>()`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LspTransportError {
    /// The server's stdout reached EOF: the process exited or closed its output.
    #[error("LSP server '{server}' exited / closed its output{detail}")]
    ServerExited { server: String, detail: String },
    /// Reading the server's stdout failed (I/O or framing error).
    #[error("LSP server '{server}' output could not be read: {message}")]
    ReadFailed { server: String, message: String },
    /// Writing to the server's stdin hit a broken pipe (the server exited or
    /// closed its stdin). SIGPIPE is ignored, so this surfaces as EPIPE.
    #[error("LSP server '{server}' input is closed (broken pipe){detail}")]
    BrokenPipe { server: String, detail: String },
    /// Writing to the server's stdin failed for another reason.
    #[error("LSP server '{server}' write failed: {message}")]
    WriteFailed { server: String, message: String },
    /// The server did not answer the request in time (the connection is NOT
    /// marked dead — a slow server may still answer later requests).
    #[error("LSP request '{method}' to server '{server}' timed out after {secs}s")]
    TimedOut {
        server: String,
        method: String,
        secs: u64,
    },
}

fn is_broken_pipe(err: &anyhow::Error) -> bool {
    err.chain()
        .filter_map(|c| c.downcast_ref::<std::io::Error>())
        .any(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
}

type PendingMap = HashMap<u64, oneshot::Sender<Value>>;

/// `None` while the connection is usable, otherwise the first fatal cause.
/// Checked under the pending-map lock (see [`mark_dead`]) so no request can
/// register after the drain and then wait out its full timeout.
type DeadState = Arc<std::sync::Mutex<Option<LspTransportError>>>;

/// Record the connection as dead (first cause wins) and fail every pending
/// request immediately by dropping its response channel.
async fn mark_dead(pending: &Mutex<PendingMap>, dead: &DeadState, cause: LspTransportError) {
    let mut pending = pending.lock().await;
    {
        let mut slot = dead.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(cause);
        }
    }
    // Dropping the senders wakes every waiter with `RecvError`; `request`
    // then reports the recorded cause.
    pending.clear();
}

fn dead_cause(dead: &DeadState) -> Option<LspTransportError> {
    dead.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Keep the last few stderr lines of the server so an exit can be explained
/// (rustup's "Unknown binary 'rust-analyzer'", a stack trace, ...).
const STDERR_TAIL_LINES: usize = 5;
const STDERR_TAIL_LINE_MAX_CHARS: usize = 300;

type StderrTail = Arc<std::sync::Mutex<std::collections::VecDeque<String>>>;

/// Actionable remediation for well-known "server not really installed"
/// failures. rustup installs a `rust-analyzer` proxy on PATH even when the
/// component is missing; the proxy then exits immediately with
/// `error: Unknown binary 'rust-analyzer' in official toolchain ...` (older
/// rustup) or `error: 'rust-analyzer' is not installed for the toolchain ...`
/// (newer rustup).
fn remediation_hint(server: &str, stderr: &[String]) -> Option<String> {
    let joined = stderr.join("\n");
    let rustup_missing_component = joined.contains("Unknown binary")
        || joined.contains("is not installed for the toolchain")
        || joined.contains("rustup component add");
    if !rustup_missing_component {
        return None;
    }
    let component = if joined.contains("rust-analyzer") || server.ends_with("rust-analyzer") {
        "rust-analyzer".to_string()
    } else {
        Path::new(server)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| server.to_string())
    };
    Some(format!(
        "{component} is not installed for this toolchain; run `rustup component add {component}`"
    ))
}

/// Describe why the server's pipes closed: exit status (if the process has
/// exited within a short grace period), a remediation hint for known causes,
/// and the tail of its stderr. Waits (<=300ms) for both the exit status and
/// the stderr drain, since stdout EOF usually precedes both by a hair.
async fn describe_exit(
    server: &str,
    child: &Mutex<Child>,
    stderr_tail: &StderrTail,
    stderr_done: &AtomicBool,
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
        tokio::time::sleep(Duration::from_millis(10)).await;
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
    if let Some(hint) = remediation_hint(server, &tail) {
        detail.push_str(&format!(": {hint}"));
    }
    if !tail.is_empty() {
        detail.push_str(&format!("; stderr: {}", tail.join(" | ")));
    }
    detail
}

// ---------------------------------------------------------------------------
// Indexing state
// ---------------------------------------------------------------------------

/// Tracks whether the server is still indexing.
///
/// Servers report work through `$/progress` streams, several of which may be
/// active at once (rust-analyzer: "Fetching", "Roots Scanned", "Indexing",
/// ...). A single boolean was cleared by ANY stream's `end` while others were
/// still running, so an empty result during indexing was reported as a plain
/// "ok, 0 references". Active tokens are tracked as a set instead.
#[derive(Debug, Default)]
struct IndexingState {
    /// Progress tokens that have sent `begin` but not yet `end`.
    active_tokens: HashSet<String>,
    /// `experimental/serverStatus` said the server is not quiescent.
    server_busy: bool,
}

impl IndexingState {
    fn is_indexing(&self) -> bool {
        !self.active_tokens.is_empty() || self.server_busy
    }

    /// Apply a `$/progress` notification's params.
    fn apply_progress(&mut self, params: &Value) {
        let token = match params.get("token") {
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => String::new(),
        };
        match params
            .get("value")
            .and_then(|v| v.get("kind"))
            .and_then(|k| k.as_str())
        {
            Some("begin") => {
                self.active_tokens.insert(token);
            }
            Some("end") => {
                self.active_tokens.remove(&token);
            }
            _ => {}
        }
    }
}

/// Result of an LSP query plus whether the server was still indexing when it
/// was produced. An EMPTY value with `still_indexing` is NOT a confirmed
/// "nothing found" (Rule 3): the index may simply not cover it yet.
#[derive(Debug, Clone, PartialEq)]
pub struct LspQueryOutcome<T> {
    pub value: T,
    pub still_indexing: bool,
}

/// Run `query`; if it comes back empty while the server is indexing, wait (at
/// most `max_wait`) for indexing to finish, then retry once. The outcome says
/// whether the server was still indexing when the returned value was produced.
pub(crate) async fn query_settling_indexing<T, Q, Fut>(
    is_indexing: impl Fn() -> bool,
    is_empty: impl Fn(&T) -> bool,
    max_wait: Duration,
    mut query: Q,
) -> Result<LspQueryOutcome<T>>
where
    Q: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let first = query().await?;
    if !is_empty(&first) || !is_indexing() {
        return Ok(LspQueryOutcome {
            still_indexing: is_indexing(),
            value: first,
        });
    }
    debug!("LSP query empty while the server is indexing; waiting up to {max_wait:?}");
    let deadline = tokio::time::Instant::now() + max_wait;
    while is_indexing() {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            break;
        }
        tokio::time::sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
    let second = query().await?;
    Ok(LspQueryOutcome {
        still_indexing: is_indexing(),
        value: second,
    })
}

// ---------------------------------------------------------------------------
// LSP transport (Content-Length framed JSON-RPC 2.0)
// ---------------------------------------------------------------------------

/// A single connection to a language server process.
struct LspServerConnection {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<PendingMap>>,
    /// First fatal cause once the connection died (EOF, read/write failure);
    /// later requests fail fast with it instead of waiting for a timeout.
    dead: DeadState,
    /// Server command, used in error messages.
    server_name: String,
    stderr_tail: StderrTail,
    stderr_done: Arc<AtomicBool>,
    /// Published diagnostics keyed by file URI.
    diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
    indexing: Arc<std::sync::Mutex<IndexingState>>,
    next_id: AtomicU64,
    child: Arc<Mutex<Child>>,
    reader_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    language: Language,
    root_uri: String,
}

impl LspServerConnection {
    /// Spawn the language server and start the background reader.
    async fn spawn(
        command: &str,
        args: &[String],
        root: &Path,
        language: Language,
    ) -> Result<Self> {
        info!(
            "Spawning LSP server: {} {:?} (lang={:?})",
            command, args, language
        );

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .current_dir(root);

        // A language server is third-party code that should not inherit the
        // agent's secrets (SELFWARE_API_KEY, AWS_*, GITHUB_TOKEN, …). Clear the
        // environment down to the minimal base, then re-add only non-secret
        // toolchain-discovery vars language servers legitimately need — so
        // rust-analyzer/pyright/tsserver still resolve their toolchains.
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        for key in [
            "CARGO_HOME",
            "RUSTUP_HOME",
            "NODE_PATH",
            "NVM_DIR",
            "npm_config_prefix",
            "PYTHONPATH",
            "VIRTUAL_ENV",
            "GOPATH",
            "GOROOT",
            "JAVA_HOME",
        ] {
            if let Ok(val) = std::env::var(key) {
                cmd.env(key, val);
            }
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn LSP server: {} {:?}", command, args))?;

        let stdin = child.stdin.take().context("Failed to capture LSP stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture LSP stdout")?;
        // stderr was piped but never read: a chatty server could fill the OS
        // pipe buffer and block forever, and an exit's real cause (e.g.
        // rustup's "Unknown binary 'rust-analyzer'") was invisible. Drain it
        // and keep a short tail for error messages.
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture LSP stderr")?;

        let pending: Arc<Mutex<PendingMap>> = Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let indexing = Arc::new(std::sync::Mutex::new(IndexingState::default()));
        let dead: DeadState = Arc::new(std::sync::Mutex::new(None));
        let stderr_tail: StderrTail = Arc::new(std::sync::Mutex::new(Default::default()));
        let stderr_done = Arc::new(AtomicBool::new(false));
        let child = Arc::new(Mutex::new(child));

        // Background task: drain stderr, keeping the last few lines.
        {
            let stderr_tail = Arc::clone(&stderr_tail);
            let stderr_done = Arc::clone(&stderr_done);
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                debug!("LSP server stderr: {}", trimmed);
                                let mut tail =
                                    stderr_tail.lock().unwrap_or_else(|p| p.into_inner());
                                if tail.len() == STDERR_TAIL_LINES {
                                    tail.pop_front();
                                }
                                tail.push_back(
                                    trimmed.chars().take(STDERR_TAIL_LINE_MAX_CHARS).collect(),
                                );
                            }
                        }
                    }
                }
                stderr_done.store(true, Ordering::Release);
            });
        }

        // Background task: read Content-Length framed messages from stdout.
        // On EOF / read error the connection is marked dead and every pending
        // request fails immediately with the cause (exit status + stderr
        // tail) instead of waiting out its 5s/30s timeout.
        let reader_handle = {
            let pending = Arc::clone(&pending);
            let diagnostics = Arc::clone(&diagnostics);
            let indexing = Arc::clone(&indexing);
            let dead = Arc::clone(&dead);
            let stderr_tail = Arc::clone(&stderr_tail);
            let stderr_done = Arc::clone(&stderr_done);
            let child = Arc::clone(&child);
            let server = command.to_string();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                loop {
                    match read_lsp_message(&mut reader).await {
                        Ok(Some(msg)) => {
                            Self::dispatch_message(msg, &pending, &diagnostics, &indexing).await;
                        }
                        Ok(None) => {
                            let detail =
                                describe_exit(&server, &child, &stderr_tail, &stderr_done).await;
                            warn!("LSP server '{}' closed its output{}", server, detail);
                            mark_dead(
                                &pending,
                                &dead,
                                LspTransportError::ServerExited {
                                    server: server.clone(),
                                    detail,
                                },
                            )
                            .await;
                            break;
                        }
                        Err(e) => {
                            warn!("LSP server '{}' stdout read/framing error: {:#}", server, e);
                            mark_dead(
                                &pending,
                                &dead,
                                LspTransportError::ReadFailed {
                                    server: server.clone(),
                                    message: format!("{e:#}"),
                                },
                            )
                            .await;
                            break;
                        }
                    }
                }
            })
        };

        let root_uri = format!("file://{}", root.display());

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            dead,
            server_name: command.to_string(),
            stderr_tail,
            stderr_done,
            diagnostics,
            indexing,
            next_id: AtomicU64::new(1),
            child,
            reader_handle: Mutex::new(Some(reader_handle)),
            language,
            root_uri,
        })
    }

    /// Route an incoming JSON message to the right handler.
    async fn dispatch_message(
        msg: Value,
        pending: &Arc<Mutex<PendingMap>>,
        diagnostics: &Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
        indexing: &Arc<std::sync::Mutex<IndexingState>>,
    ) {
        // Is it a response (has "id" + either "result" or "error")?
        if let Some(id) = msg.get("id").and_then(|v| v.as_u64()) {
            if msg.get("method").is_none() {
                // It's a response, not a request from server.
                let mut pending = pending.lock().await;
                if let Some(tx) = pending.remove(&id) {
                    let _ = tx.send(msg);
                }
                return;
            }
        }

        // Is it a notification?
        if let Some(method) = msg.get("method").and_then(|v| v.as_str()) {
            if method == "textDocument/publishDiagnostics" {
                if let Some(params) = msg.get("params") {
                    Self::handle_diagnostics(params, diagnostics).await;
                }
            } else if method == "$/progress" {
                if let Some(params) = msg.get("params") {
                    indexing
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .apply_progress(params);
                }
            } else if method == "experimental/serverStatus" {
                if let Some(params) = msg.get("params") {
                    if let Some(quiescent) = params.get("quiescent").and_then(|q| q.as_bool()) {
                        indexing
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .server_busy = !quiescent;
                    }
                }
            }
        }
    }

    /// Check if this server is currently indexing (any progress stream still
    /// active, or the server reported itself non-quiescent).
    pub fn is_indexing(&self) -> bool {
        self.indexing
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_indexing()
    }

    /// The recorded fatal cause, if the connection is dead.
    fn dead_cause(&self) -> Option<LspTransportError> {
        dead_cause(&self.dead)
    }

    /// Parse and store diagnostics from `textDocument/publishDiagnostics`.
    async fn handle_diagnostics(
        params: &Value,
        store: &Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
    ) {
        let uri = match params.get("uri").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => return,
        };

        let diags: Vec<Diagnostic> = params
            .get("diagnostics")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        let message = d.get("message")?.as_str()?.to_string();
                        let severity = match d.get("severity").and_then(|s| s.as_u64()) {
                            Some(1) => "error",
                            Some(2) => "warning",
                            Some(3) => "info",
                            Some(4) => "hint",
                            _ => "warning",
                        }
                        .to_string();
                        let range = d.get("range")?;
                        let start = range.get("start")?;
                        let line = start.get("line")?.as_u64()? as u32;
                        let column = start.get("character")?.as_u64()? as u32;
                        Some(Diagnostic {
                            message,
                            severity,
                            line,
                            column,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut store = store.lock().await;
        store.insert(uri, diags);
    }

    /// Send a JSON-RPC request and wait for the response (with timeout).
    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let timeout = if method == "initialize" {
            INITIALIZE_TIMEOUT
        } else {
            REQUEST_TIMEOUT
        };
        self.request_with_timeout(method, params, timeout).await
    }

    /// Send a JSON-RPC request and wait at most `timeout` for the response.
    ///
    /// Fails fast with a typed [`LspTransportError`] when the server is (or
    /// becomes) dead: registration checks the dead state under the pending
    /// lock, the reader drains pending requests on EOF, and write errors mark
    /// the connection dead.
    async fn request_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if let Some(cause) = self.dead_cause() {
                return Err(anyhow::Error::new(cause));
            }
            pending.insert(id, tx);
        }

        if let Err(e) = self.send_message(&msg).await {
            self.pending.lock().await.remove(&id);
            return Err(anyhow::Error::new(self.write_failed(&e).await));
        }
        debug!("Sent LSP request: {} (id={})", method, id);

        let response = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                // Sender dropped: the reader marked the connection dead.
                let cause = self
                    .dead_cause()
                    .unwrap_or_else(|| LspTransportError::ServerExited {
                        server: self.server_name.clone(),
                        detail: String::new(),
                    });
                return Err(anyhow::Error::new(cause));
            }
            Err(_) => {
                self.pending.lock().await.remove(&id);
                return Err(anyhow::Error::new(LspTransportError::TimedOut {
                    server: self.server_name.clone(),
                    method: method.to_string(),
                    secs: timeout.as_secs(),
                }));
            }
        };

        if let Some(error) = response.get("error") {
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            bail!("LSP error for '{}': [{}] {}", method, code, message);
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Classify a stdin write failure, mark the connection dead, and return
    /// the recorded cause (the reader's `ServerExited` may win the race — it
    /// is equally fatal and carries the exit status).
    async fn write_failed(&self, err: &anyhow::Error) -> LspTransportError {
        let cause = if is_broken_pipe(err) {
            LspTransportError::BrokenPipe {
                server: self.server_name.clone(),
                detail: describe_exit(
                    &self.server_name,
                    &self.child,
                    &self.stderr_tail,
                    &self.stderr_done,
                )
                .await,
            }
        } else {
            LspTransportError::WriteFailed {
                server: self.server_name.clone(),
                message: format!("{err:#}"),
            }
        };
        mark_dead(&self.pending, &self.dead, cause.clone()).await;
        self.dead_cause().unwrap_or(cause)
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        if let Some(cause) = self.dead_cause() {
            return Err(anyhow::Error::new(cause));
        }
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        if let Err(e) = self.send_message(&msg).await {
            return Err(anyhow::Error::new(self.write_failed(&e).await));
        }
        debug!("Sent LSP notification: {}", method);
        Ok(())
    }

    /// Write a message with `Content-Length` framing.
    async fn send_message(&self, msg: &Value) -> Result<()> {
        let body = serde_json::to_string(msg)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(header.as_bytes()).await?;
        stdin.write_all(body.as_bytes()).await?;
        stdin.flush().await?;
        Ok(())
    }

    /// Send `initialize` + `initialized` handshake.
    async fn initialize(&self) -> Result<Value> {
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": self.root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": { "dynamicRegistration": false },
                    "references": { "dynamicRegistration": false },
                    "documentSymbol": {
                        "dynamicRegistration": false,
                        "symbolKind": {
                            "valueSet": [1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26]
                        }
                    },
                    "hover": { "dynamicRegistration": false, "contentFormat": ["plaintext", "markdown"] },
                    "publishDiagnostics": { "relatedInformation": true },
                    "synchronization": {
                        "dynamicRegistration": false,
                        "didSave": true,
                        "willSave": false,
                        "willSaveWaitUntil": false
                    }
                },
                "workspace": {
                    "workspaceFolders": true
                }
            },
            "workspaceFolders": [{
                "uri": self.root_uri,
                "name": "workspace"
            }]
        });

        let result = self.request("initialize", params).await?;

        // Send initialized notification
        self.notify("initialized", serde_json::json!({})).await?;

        info!("LSP server initialized for {:?}", self.language);
        Ok(result)
    }

    /// Kill the server immediately (no polite shutdown) — used when it never
    /// finished starting, so a hung server is not waited on again.
    async fn kill_now(&self) {
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        drop(child);
        if let Some(h) = self.reader_handle.lock().await.take() {
            h.abort();
        }
    }

    /// Gracefully shut down the server.
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down LSP server for {:?}", self.language);

        // A dead server cannot answer; skip the polite handshake entirely.
        if self.dead_cause().is_none() {
            // Send shutdown request (server should respond)
            let _ = self
                .request_with_timeout("shutdown", Value::Null, SHUTDOWN_TIMEOUT)
                .await;

            // Send exit notification
            let _ = self.notify("exit", Value::Null).await;

            // Give the server a moment to exit gracefully, then kill
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        self.kill_now().await;
        Ok(())
    }
}

/// Read a single Content-Length framed LSP message from a reader.
async fn read_lsp_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Value>> {
    // Read headers until we find Content-Length.
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    loop {
        header_line.clear();
        let n = reader.read_line(&mut header_line).await?;
        if n == 0 {
            return Ok(None); // EOF
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            // End of headers
            break;
        }

        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                len_str
                    .trim()
                    .parse::<usize>()
                    .context("Invalid Content-Length value")?,
            );
        }
        // Ignore other headers (e.g. Content-Type)
    }

    let length = content_length.context("Missing Content-Length header in LSP message")?;

    // Sanity cap: 64 MiB
    if length > 64 * 1024 * 1024 {
        bail!("LSP message too large: {} bytes", length);
    }

    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).await?;

    let msg: Value = serde_json::from_slice(&body)?;
    Ok(Some(msg))
}

// ---------------------------------------------------------------------------
// Public LspClient — manages multiple language server connections
// ---------------------------------------------------------------------------

/// Client that manages language server connections for multiple languages.
///
/// Lazily starts the appropriate server on the first request for a given
/// language, and keeps it alive for the session. If a running server crashes,
/// it is restarted on the next request. A server that FAILED TO START is
/// remembered for the session: later calls fail immediately with the recorded
/// cause instead of re-spawning it and waiting again.
pub struct LspClient {
    connections: Arc<Mutex<HashMap<Language, Arc<LspServerConnection>>>>,
    project_root: PathBuf,
    /// Per-document version counters for `textDocument/didChange`.
    document_versions: Arc<Mutex<HashMap<String, u32>>>,
    /// Start failures recorded this session, keyed by language.
    failed_starts: Arc<Mutex<HashMap<Language, String>>>,
    /// Serializes check-and-start so concurrent calls never race to spawn two
    /// servers for the same language.
    start_lock: Mutex<()>,
    /// Replacement server candidates per language (tests, custom setups).
    candidate_overrides: HashMap<Language, Vec<(String, Vec<String>)>>,
    /// Upper bound on waiting for indexing before retrying an empty query.
    indexing_wait: Duration,
}

impl LspClient {
    /// Create a new LspClient rooted at the given project directory.
    pub fn new(project_root: &Path) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            project_root: project_root.to_path_buf(),
            document_versions: Arc::new(Mutex::new(HashMap::new())),
            failed_starts: Arc::new(Mutex::new(HashMap::new())),
            start_lock: Mutex::new(()),
            candidate_overrides: HashMap::new(),
            indexing_wait: DEFAULT_INDEXING_WAIT,
        }
    }

    /// Use `candidates` (command, args) instead of the built-in server list
    /// for `lang`.
    pub fn with_server_candidates(
        mut self,
        lang: Language,
        candidates: Vec<(String, Vec<String>)>,
    ) -> Self {
        self.candidate_overrides.insert(lang, candidates);
        self
    }

    /// Override how long an empty query waits for indexing to finish before
    /// its single retry (default [`DEFAULT_INDEXING_WAIT`]).
    pub fn with_indexing_wait(mut self, wait: Duration) -> Self {
        self.indexing_wait = wait;
        self
    }

    /// Initialize (or lazily start) the language server for the given file.
    pub async fn initialize(&self, project_root: &Path) -> Result<()> {
        // Just stores the root; actual servers start lazily.
        info!("LspClient initialized for {}", project_root.display());
        Ok(())
    }

    /// Check if the language server for `lang` is currently indexing.
    pub async fn is_indexing(&self, lang: Language) -> bool {
        let conns = self.connections.lock().await;
        conns.get(&lang).map(|c| c.is_indexing()).unwrap_or(false)
    }

    /// Get or start the connection for a language.
    async fn connection_for(&self, lang: Language) -> Result<Arc<LspServerConnection>> {
        let _start = self.start_lock.lock().await;
        {
            let mut conns = self.connections.lock().await;
            if let Some(conn) = conns.get(&lang) {
                let alive = if conn.dead_cause().is_some() {
                    false
                } else {
                    // Check if the process is still alive (can't check =>
                    // assume alive).
                    let mut child = conn.child.lock().await;
                    !matches!(child.try_wait(), Ok(Some(_)))
                };
                if alive {
                    return Ok(Arc::clone(conn));
                }
                // Died mid-session: drop it and fall through to restart.
                if let Some(old) = conns.remove(&lang) {
                    old.kill_now().await;
                }
            }
        }

        if let Some(cause) = self.failed_starts.lock().await.get(&lang) {
            bail!(
                "LSP server for {:?} is unavailable this session (it failed to start earlier \
                 and is not retried; restart selfware after fixing it): {}",
                lang,
                cause
            );
        }

        match self.start_server(lang).await {
            Ok(conn) => Ok(conn),
            Err(e) => {
                let cause = format!("{e:#}");
                self.failed_starts.lock().await.insert(lang, cause.clone());
                Err(e)
            }
        }
    }

    /// Start a language server, initialize it, and store the connection.
    async fn start_server(&self, lang: Language) -> Result<Arc<LspServerConnection>> {
        let candidates = self
            .candidate_overrides
            .get(&lang)
            .cloned()
            .unwrap_or_else(|| server_candidates(lang));
        let mut failures: Vec<String> = Vec::new();

        for (cmd, args) in &candidates {
            if !binary_exists(cmd).await {
                debug!("LSP server binary not found: {}", cmd);
                continue;
            }

            match LspServerConnection::spawn(cmd, args, &self.project_root, lang).await {
                Ok(conn) => {
                    // Run the initialize handshake. A dead server fails this
                    // fast with its exit cause (stderr tail, remediation).
                    if let Err(e) = conn.initialize().await {
                        warn!("LSP initialize failed for {}: {:#}", cmd, e);
                        failures.push(format!("{e:#}"));
                        conn.kill_now().await;
                        continue;
                    }

                    let conn = Arc::new(conn);
                    let mut conns = self.connections.lock().await;
                    conns.insert(lang, Arc::clone(&conn));
                    return Ok(conn);
                }
                Err(e) => {
                    debug!("Failed to spawn {}: {:#}", cmd, e);
                    failures.push(format!("{e:#}"));
                    continue;
                }
            }
        }

        if failures.is_empty() {
            bail!(
                "No LSP server available for {:?}. Install one of: {:?}",
                lang,
                candidates
                    .iter()
                    .map(|(c, _)| c.as_str())
                    .collect::<Vec<_>>()
            )
        }
        bail!(
            "LSP server for {:?} failed to start: {}",
            lang,
            failures.join("; ")
        )
    }

    /// Build a `TextDocumentIdentifier` from a file path.
    ///
    /// Normalizes the path to an absolute, canonical `file://` URI so that
    /// LSP servers receive the same identifier regardless of whether the
    /// caller passed a relative or absolute path.
    fn file_uri(file: &str) -> String {
        if file.starts_with("file://") {
            return file.to_string();
        }

        // Resolve to an absolute, canonical path when possible.
        let abs = if Path::new(file).is_absolute() {
            // Canonicalize to resolve symlinks and `.`/`..` components.
            std::fs::canonicalize(file)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| file.to_string())
        } else {
            std::env::current_dir()
                .and_then(|cwd| {
                    let joined = cwd.join(file);
                    std::fs::canonicalize(&joined)
                        .map(|p| p.display().to_string())
                        .or(Ok(joined.display().to_string()))
                })
                .unwrap_or_else(|_| file.to_string())
        };

        // On Windows, absolute paths look like `C:\...`; on Unix, `/...`.
        // The `file://` scheme expects `file:///path` (three slashes for
        // Unix) or `file:///C:/...` on Windows (with forward slashes).
        let normalized = abs.replace('\\', "/");

        // Percent-encode characters that are not allowed unencoded in a
        // file URI path component.  We encode the path segment-by-segment
        // (splitting on `/`) so that the path separators themselves are
        // preserved.
        let encoded: String = normalized
            .split('/')
            .map(|segment| {
                segment
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric()
                            || ch == '-'
                            || ch == '.'
                            || ch == '_'
                            || ch == '~'
                        {
                            ch.to_string()
                        } else {
                            format!("%{:02X}", ch as u8)
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("/");

        if encoded.starts_with('/') {
            format!("file://{}", encoded)
        } else {
            // Windows drive letter: `C:/...` → `file:///C:/...`
            format!("file:///{}", encoded)
        }
    }

    /// Strip `file://` prefix from a URI, returning a plain path.
    fn uri_to_path(uri: &str) -> String {
        uri.strip_prefix("file://").unwrap_or(uri).to_string()
    }

    // -----------------------------------------------------------------------
    // Public LSP operations
    // -----------------------------------------------------------------------

    /// Notify the server that a file has been opened.
    pub async fn did_open(&self, file: &str, content: &str) -> Result<()> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;

        let uri = Self::file_uri(file);
        {
            let mut versions = self.document_versions.lock().await;
            versions.insert(uri.clone(), 1);
        }

        let conn = self.connection_for(lang).await?;
        conn.notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": lang.id(),
                    "version": 1,
                    "text": content,
                }
            }),
        )
        .await
    }

    /// Notify the server that a file has been closed.
    ///
    /// Sends `textDocument/didClose` so the LSP server can release resources
    /// associated with the document.  Call this after a `did_open` when the
    /// caller no longer needs diagnostics / navigation for that file.
    pub async fn did_close(&self, file: &str) -> Result<()> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;

        let uri = Self::file_uri(file);
        {
            let mut versions = self.document_versions.lock().await;
            versions.remove(&uri);
        }

        let conn = self.connection_for(lang).await?;
        conn.notify(
            "textDocument/didClose",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                }
            }),
        )
        .await
    }

    /// Run `query` against `conn`; an empty result while the server is
    /// indexing waits (bounded) for indexing to finish and is retried once.
    async fn settled<T, Q, Fut>(
        &self,
        conn: &Arc<LspServerConnection>,
        is_empty: impl Fn(&T) -> bool,
        query: Q,
    ) -> Result<LspQueryOutcome<T>>
    where
        Q: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let probe = Arc::clone(conn);
        query_settling_indexing(
            move || probe.is_indexing(),
            is_empty,
            self.indexing_wait,
            query,
        )
        .await
    }

    /// Issue a position-based location request (`definition`, `references`,
    /// `implementation`) with indexing settling.
    async fn location_query(
        &self,
        conn: &Arc<LspServerConnection>,
        method: &'static str,
        params: Value,
    ) -> Result<LspQueryOutcome<Vec<Location>>> {
        self.settled(conn, Vec::is_empty, || {
            let conn = Arc::clone(conn);
            let params = params.clone();
            async move {
                let result = conn.request(method, params).await?;
                Self::parse_locations(&result)
            }
        })
        .await
    }

    /// Go to the definition of the symbol at the given position.
    ///
    /// `still_indexing` on an empty outcome means "not found YET", not a
    /// confirmed absence.
    pub async fn goto_definition(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<LspQueryOutcome<Vec<Location>>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let content = tokio::fs::read_to_string(&file).await.unwrap_or_default();
        self.did_open(file, &content).await?;

        let outcome = self
            .location_query(
                &conn,
                "textDocument/definition",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col }
                }),
            )
            .await;

        // Close the document so the server doesn't leak it.
        let _ = self.did_close(file).await;
        outcome
    }

    /// Find all references to the symbol at the given position.
    ///
    /// `still_indexing` on an empty outcome means "0 references found SO
    /// FAR", not a confirmed zero.
    pub async fn find_references(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<LspQueryOutcome<Vec<Location>>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        self.location_query(
            &conn,
            "textDocument/references",
            serde_json::json!({
                "textDocument": { "uri": Self::file_uri(file) },
                "position": { "line": line, "character": col },
                "context": { "includeDeclaration": true }
            }),
        )
        .await
    }

    /// List all symbols in a document.
    pub async fn document_symbols(&self, file: &str) -> Result<LspQueryOutcome<Vec<SymbolInfo>>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;
        let params = serde_json::json!({
            "textDocument": { "uri": Self::file_uri(file) }
        });

        self.settled(&conn, Vec::is_empty, || {
            let conn = Arc::clone(&conn);
            let params = params.clone();
            async move {
                let result = conn.request("textDocument/documentSymbol", params).await?;
                Self::parse_symbols(&result)
            }
        })
        .await
    }

    /// Get hover information for a symbol.
    pub async fn hover(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<LspQueryOutcome<Option<String>>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;
        let params = serde_json::json!({
            "textDocument": { "uri": Self::file_uri(file) },
            "position": { "line": line, "character": col }
        });

        self.settled(&conn, Option::is_none, || {
            let conn = Arc::clone(&conn);
            let params = params.clone();
            async move {
                let result = conn.request("textDocument/hover", params).await?;
                Ok(Self::parse_hover(&result))
            }
        })
        .await
    }

    /// Extract hover text: `contents` can be a string, MarkupContent, or array.
    fn parse_hover(result: &Value) -> Option<String> {
        if result.is_null() {
            return None;
        }
        match result.get("contents") {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Object(obj)) => {
                // MarkupContent: { kind: "markdown"|"plaintext", value: "..." }
                obj.get("value").and_then(|v| v.as_str()).map(String::from)
            }
            Some(Value::Array(arr)) => {
                let parts: Vec<String> = arr
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(s) => Some(s.clone()),
                        Value::Object(obj) => {
                            obj.get("value").and_then(|v| v.as_str()).map(String::from)
                        }
                        _ => None,
                    })
                    .collect();
                if parts.is_empty() {
                    None
                } else {
                    Some(parts.join("\n\n"))
                }
            }
            _ => None,
        }
    }

    /// Get current diagnostics for a file (from last `publishDiagnostics` notification).
    pub async fn diagnostics(&self, file: &str) -> Result<Vec<Diagnostic>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let diag_store = conn.diagnostics.lock().await;
        let uri = Self::file_uri(file);
        Ok(diag_store.get(&uri).cloned().unwrap_or_default())
    }

    /// `workspace/symbol` against one connection, with indexing settling.
    async fn workspace_symbol_on(
        &self,
        conn: &Arc<LspServerConnection>,
        query: &str,
    ) -> Result<LspQueryOutcome<Vec<SymbolInfo>>> {
        let params = serde_json::json!({ "query": query });
        self.settled(conn, Vec::is_empty, || {
            let conn = Arc::clone(conn);
            let params = params.clone();
            async move {
                let result = conn.request("workspace/symbol", params).await?;
                Self::parse_symbols(&result)
            }
        })
        .await
    }

    /// Find workspace symbols matching a query string.
    pub async fn workspace_symbol(&self, query: &str) -> Result<LspQueryOutcome<Vec<SymbolInfo>>> {
        // Try existing connections first.
        let langs: Vec<Language> = {
            let conns = self.connections.lock().await;
            conns.keys().cloned().collect()
        };

        let mut any_indexing = false;
        let mut queried: HashSet<Language> = HashSet::new();
        for lang in langs {
            if let Ok(conn) = self.connection_for(lang).await {
                queried.insert(lang);
                let outcome = self.workspace_symbol_on(&conn, query).await?;
                if !outcome.value.is_empty() {
                    return Ok(outcome);
                }
                any_indexing |= outcome.still_indexing;
            }
        }

        // Nothing yet — try (starting) a server for the dominant language.
        if let Some(lang) = detect_dominant_language(&self.project_root).await {
            if !queried.contains(&lang) {
                let conn = self.connection_for(lang).await?;
                let mut outcome = self.workspace_symbol_on(&conn, query).await?;
                if outcome.value.is_empty() {
                    outcome.still_indexing |= any_indexing;
                }
                return Ok(outcome);
            }
        }

        Ok(LspQueryOutcome {
            value: vec![],
            still_indexing: any_indexing,
        })
    }

    /// Go to the implementation of a symbol at a given position.
    pub async fn goto_implementation(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<LspQueryOutcome<Vec<Location>>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let content = tokio::fs::read_to_string(&file).await.unwrap_or_default();
        self.did_open(file, &content).await?;

        let outcome = self
            .location_query(
                &conn,
                "textDocument/implementation",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col }
                }),
            )
            .await;

        // Close the document so the server doesn't leak it.
        let _ = self.did_close(file).await;
        outcome
    }

    /// Gracefully shut down all connected language servers.
    pub async fn shutdown(&self) -> Result<()> {
        // Close all open documents before shutting down servers.
        let open_uris: Vec<String> = {
            let versions = self.document_versions.lock().await;
            versions.keys().cloned().collect()
        };
        for uri in &open_uris {
            let path = Self::uri_to_path(uri);
            let _ = self.did_close(&path).await;
        }

        let mut conns = self.connections.lock().await;
        for (lang, conn) in conns.drain() {
            if let Err(e) = conn.shutdown().await {
                warn!("Error shutting down LSP server for {:?}: {}", lang, e);
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Response parsing helpers
    // -----------------------------------------------------------------------

    /// Parse a definition/references response into `Vec<Location>`.
    ///
    /// The LSP spec allows either a single Location, an array of Locations,
    /// or an array of LocationLinks.
    fn parse_locations(value: &Value) -> Result<Vec<Location>> {
        if value.is_null() {
            return Ok(vec![]);
        }

        if let Some(arr) = value.as_array() {
            let mut locs = Vec::new();
            for item in arr {
                if let Some(loc) = Self::parse_single_location(item) {
                    locs.push(loc);
                }
            }
            Ok(locs)
        } else if let Some(loc) = Self::parse_single_location(value) {
            Ok(vec![loc])
        } else {
            Ok(vec![])
        }
    }

    fn parse_single_location(value: &Value) -> Option<Location> {
        // Standard Location: { uri, range: { start: { line, character } } }
        let uri = value.get("uri").or_else(|| value.get("targetUri"))?;
        let uri_str = uri.as_str()?;

        let range = value
            .get("range")
            .or_else(|| value.get("targetSelectionRange"))?;
        let start = range.get("start")?;
        let line = start.get("line")?.as_u64()? as u32;
        let column = start.get("character")?.as_u64()? as u32;

        Some(Location {
            file: Self::uri_to_path(uri_str),
            line,
            column,
        })
    }

    /// Parse document symbols response.
    ///
    /// Can be `DocumentSymbol[]` (hierarchical) or `SymbolInformation[]` (flat).
    fn parse_symbols(value: &Value) -> Result<Vec<SymbolInfo>> {
        if value.is_null() {
            return Ok(vec![]);
        }

        let empty = vec![];
        let arr = value.as_array().unwrap_or(&empty);
        let mut symbols = Vec::new();

        for item in arr {
            Self::collect_symbols(item, &mut symbols);
        }

        Ok(symbols)
    }

    /// Recursively collect symbols (handles hierarchical DocumentSymbol).
    fn collect_symbols(value: &Value, out: &mut Vec<SymbolInfo>) {
        let name = match value.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => return,
        };

        let kind_num = value.get("kind").and_then(|k| k.as_u64()).unwrap_or(0);
        let kind = symbol_kind_name(kind_num);

        // DocumentSymbol has "selectionRange", SymbolInformation has "location".
        let (line, column) = if let Some(sel_range) = value.get("selectionRange") {
            let start = sel_range.get("start").unwrap_or(&Value::Null);
            (
                start.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32,
                start.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
            )
        } else if let Some(location) = value.get("location") {
            let range = location.get("range").unwrap_or(&Value::Null);
            let start = range.get("start").unwrap_or(&Value::Null);
            (
                start.get("line").and_then(|l| l.as_u64()).unwrap_or(0) as u32,
                start.get("character").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
            )
        } else {
            (0, 0)
        };

        out.push(SymbolInfo {
            name,
            kind,
            line,
            column,
        });

        // Recurse into children (DocumentSymbol).
        if let Some(children) = value.get("children").and_then(|c| c.as_array()) {
            for child in children {
                Self::collect_symbols(child, out);
            }
        }
    }
}

/// Map LSP SymbolKind numeric value to a human-readable string.
/// Detect the dominant language in a project by counting source files.
async fn detect_dominant_language(root: &Path) -> Option<Language> {
    let mut counts: HashMap<Language, usize> = HashMap::new();

    for entry in walkdir::WalkDir::new(root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        if let Some(lang) = Language::from_path(entry.path().to_string_lossy().as_ref()) {
            *counts.entry(lang).or_insert(0) += 1;
        }
    }

    counts.into_iter().max_by_key(|(_, c)| *c).map(|(l, _)| l)
}

fn symbol_kind_name(kind: u64) -> String {
    match kind {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "unknown",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../../tests/unit/lsp/client/client_test.rs"]
mod tests;
