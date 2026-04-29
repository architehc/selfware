//! LSP client implementation.
//!
//! Manages connections to language servers (one per language), communicating
//! via JSON-RPC 2.0 over stdio with `Content-Length` header framing.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
fn server_candidates(lang: Language) -> Vec<(&'static str, Vec<&'static str>)> {
    match lang {
        Language::Rust => vec![("rust-analyzer", vec![])],
        Language::Python => vec![("pyright-langserver", vec!["--stdio"]), ("pylsp", vec![])],
        Language::TypeScript | Language::JavaScript => {
            vec![("typescript-language-server", vec!["--stdio"])]
        }
        Language::Go => vec![("gopls", vec!["serve"])],
    }
}

/// Check if a binary is available on PATH.
async fn binary_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// LSP transport (Content-Length framed JSON-RPC 2.0)
// ---------------------------------------------------------------------------

/// A single connection to a language server process.
struct LspServerConnection {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    /// Published diagnostics keyed by file URI.
    diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
    next_id: AtomicU64,
    child: Arc<Mutex<Child>>,
    reader_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    language: Language,
    root_uri: String,
}

impl LspServerConnection {
    /// Spawn the language server and start the background reader.
    async fn spawn(command: &str, args: &[&str], root: &Path, language: Language) -> Result<Self> {
        info!(
            "Spawning LSP server: {} {:?} (lang={:?})",
            command, args, language
        );

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(root);

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn LSP server: {} {:?}", command, args))?;

        let stdin = child.stdin.take().context("Failed to capture LSP stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture LSP stdout")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let diagnostics: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = Arc::clone(&pending);
        let diag_clone = Arc::clone(&diagnostics);

        // Background task: read Content-Length framed messages from stdout.
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_lsp_message(&mut reader).await {
                    Ok(Some(msg)) => {
                        Self::dispatch_message(msg, &pending_clone, &diag_clone).await;
                    }
                    Ok(None) => {
                        debug!("LSP stdout closed");
                        break;
                    }
                    Err(e) => {
                        debug!("LSP read error: {}", e);
                        break;
                    }
                }
            }
        });

        let root_uri = format!("file://{}", root.display());

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            diagnostics,
            next_id: AtomicU64::new(1),
            child: Arc::new(Mutex::new(child)),
            reader_handle: Mutex::new(Some(reader_handle)),
            language,
            root_uri,
        })
    }

    /// Route an incoming JSON message to the right handler.
    async fn dispatch_message(
        msg: Value,
        pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
        diagnostics: &Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
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
            }
            // Other notifications are silently ignored.
        }
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
            pending.insert(id, tx);
        }

        self.send_message(&msg).await?;
        debug!("Sent LSP request: {} (id={})", method, id);

        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| anyhow::anyhow!("LSP request '{}' timed out after 30s", method))?
            .map_err(|_| anyhow::anyhow!("LSP response channel closed for '{}'", method))?;

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

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send_message(&msg).await?;
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

    /// Gracefully shut down the server.
    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down LSP server for {:?}", self.language);

        // Send shutdown request (server should respond)
        let _ = self.request("shutdown", Value::Null).await;

        // Send exit notification
        let _ = self.notify("exit", Value::Null).await;

        // Give the server a moment to exit gracefully, then kill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let mut child = self.child.lock().await;
        let _ = child.kill().await;

        let mut handle = self.reader_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }

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
/// language, and keeps it alive for the session. If a server crashes, it
/// is restarted on the next request.
pub struct LspClient {
    connections: Arc<Mutex<HashMap<Language, Arc<LspServerConnection>>>>,
    project_root: PathBuf,
}

impl LspClient {
    /// Create a new LspClient rooted at the given project directory.
    pub fn new(project_root: &Path) -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            project_root: project_root.to_path_buf(),
        }
    }

    /// Initialize (or lazily start) the language server for the given file.
    pub async fn initialize(&self, project_root: &Path) -> Result<()> {
        // Just stores the root; actual servers start lazily.
        info!("LspClient initialized for {}", project_root.display());
        Ok(())
    }

    /// Get or start the connection for a language.
    async fn connection_for(&self, lang: Language) -> Result<Arc<LspServerConnection>> {
        {
            let conns = self.connections.lock().await;
            if let Some(conn) = conns.get(&lang) {
                // Check if the process is still alive.
                let mut child = conn.child.lock().await;
                match child.try_wait() {
                    Ok(Some(_status)) => {
                        // Process exited; fall through to restart.
                        drop(child);
                        drop(conns);
                    }
                    Ok(None) => {
                        // Still running.
                        return Ok(Arc::clone(conn));
                    }
                    Err(_) => {
                        // Can't check; assume alive.
                        return Ok(Arc::clone(conn));
                    }
                }
            }
        }

        // Need to start a new server.
        self.start_server(lang).await
    }

    /// Start a language server, initialize it, and store the connection.
    async fn start_server(&self, lang: Language) -> Result<Arc<LspServerConnection>> {
        let candidates = server_candidates(lang);

        for (cmd, args) in &candidates {
            if !binary_exists(cmd).await {
                debug!("LSP server binary not found: {}", cmd);
                continue;
            }

            let str_args: Vec<&str> = args.to_vec();
            match LspServerConnection::spawn(cmd, &str_args, &self.project_root, lang).await {
                Ok(conn) => {
                    // Run the initialize handshake.
                    if let Err(e) = conn.initialize().await {
                        warn!("LSP initialize failed for {}: {}", cmd, e);
                        let _ = conn.shutdown().await;
                        continue;
                    }

                    let conn = Arc::new(conn);
                    let mut conns = self.connections.lock().await;
                    conns.insert(lang, Arc::clone(&conn));
                    return Ok(conn);
                }
                Err(e) => {
                    debug!("Failed to spawn {}: {}", cmd, e);
                    continue;
                }
            }
        }

        bail!(
            "No LSP server available for {:?}. Install one of: {:?}",
            lang,
            candidates.iter().map(|(c, _)| *c).collect::<Vec<_>>()
        )
    }

    /// Build a `TextDocumentIdentifier` from a file path.
    fn file_uri(file: &str) -> String {
        if file.starts_with("file://") {
            file.to_string()
        } else {
            let abs = if Path::new(file).is_absolute() {
                file.to_string()
            } else {
                // Best-effort: treat as relative to cwd.
                std::env::current_dir()
                    .map(|cwd| cwd.join(file).display().to_string())
                    .unwrap_or_else(|_| file.to_string())
            };
            format!("file://{}", abs)
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

        let conn = self.connection_for(lang).await?;
        conn.notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": Self::file_uri(file),
                    "languageId": lang.id(),
                    "version": 1,
                    "text": content,
                }
            }),
        )
        .await
    }

    /// Notify the server that a file has changed (full content sync).
    pub async fn did_change(&self, file: &str, content: &str) -> Result<()> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;

        let conn = self.connection_for(lang).await?;
        conn.notify(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": {
                    "uri": Self::file_uri(file),
                    "version": 2,
                },
                "contentChanges": [{
                    "text": content,
                }]
            }),
        )
        .await
    }

    /// Go to the definition of the symbol at the given position.
    pub async fn goto_definition(&self, file: &str, line: u32, col: u32) -> Result<Vec<Location>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let result = conn
            .request(
                "textDocument/definition",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col }
                }),
            )
            .await?;

        Self::parse_locations(&result)
    }

    /// Find all references to the symbol at the given position.
    pub async fn find_references(&self, file: &str, line: u32, col: u32) -> Result<Vec<Location>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let result = conn
            .request(
                "textDocument/references",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col },
                    "context": { "includeDeclaration": true }
                }),
            )
            .await?;

        Self::parse_locations(&result)
    }

    /// List all symbols in a document.
    pub async fn document_symbols(&self, file: &str) -> Result<Vec<SymbolInfo>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let result = conn
            .request(
                "textDocument/documentSymbol",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) }
                }),
            )
            .await?;

        Self::parse_symbols(&result)
    }

    /// Get hover information for a symbol.
    pub async fn hover(&self, file: &str, line: u32, col: u32) -> Result<Option<String>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let result = conn
            .request(
                "textDocument/hover",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col }
                }),
            )
            .await?;

        if result.is_null() {
            return Ok(None);
        }

        // Hover result can have "contents" as string, MarkupContent, or array.
        let contents = result.get("contents");
        match contents {
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(Value::Object(obj)) => {
                // MarkupContent: { kind: "markdown"|"plaintext", value: "..." }
                Ok(obj.get("value").and_then(|v| v.as_str()).map(String::from))
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
                    Ok(None)
                } else {
                    Ok(Some(parts.join("\n\n")))
                }
            }
            _ => Ok(None),
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

    /// Find workspace symbols matching a query string.
    pub async fn workspace_symbol(&self, query: &str) -> Result<Vec<SymbolInfo>> {
        // Try existing connections first.
        let langs: Vec<Language> = {
            let conns = self.connections.lock().await;
            conns.keys().cloned().collect()
        };

        for lang in langs {
            if let Ok(conn) = self.connection_for(lang).await {
                let result = conn
                    .request("workspace/symbol", serde_json::json!({ "query": query }))
                    .await?;
                let symbols = Self::parse_symbols(&result)?;
                if !symbols.is_empty() {
                    return Ok(symbols);
                }
            }
        }

        // No existing connections — try to start a server for the dominant language.
        if let Some(lang) = detect_dominant_language(&self.project_root).await {
            let conn = self.connection_for(lang).await?;
            let result = conn
                .request("workspace/symbol", serde_json::json!({ "query": query }))
                .await?;
            return Self::parse_symbols(&result);
        }

        Ok(vec![])
    }

    /// Go to the implementation of a symbol at a given position.
    pub async fn goto_implementation(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<Vec<Location>> {
        let lang = Language::from_path(file)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect language for: {}", file))?;
        let conn = self.connection_for(lang).await?;

        let content = tokio::fs::read_to_string(&file).await.unwrap_or_default();
        self.did_open(file, &content).await?;

        let result = conn
            .request(
                "textDocument/implementation",
                serde_json::json!({
                    "textDocument": { "uri": Self::file_uri(file) },
                    "position": { "line": line, "character": col }
                }),
            )
            .await?;

        Self::parse_locations(&result)
    }

    /// Gracefully shut down all connected language servers.
    pub async fn shutdown(&self) -> Result<()> {
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
mod tests {
    use super::*;

    #[test]
    fn test_language_from_path() {
        assert_eq!(Language::from_path("src/main.rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("app.py"), Some(Language::Python));
        assert_eq!(Language::from_path("index.ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("index.tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("app.js"), Some(Language::JavaScript));
        assert_eq!(Language::from_path("main.go"), Some(Language::Go));
        assert_eq!(Language::from_path("README.md"), None);
        assert_eq!(Language::from_path("Makefile"), None);
    }

    #[test]
    fn test_language_id() {
        assert_eq!(Language::Rust.id(), "rust");
        assert_eq!(Language::Python.id(), "python");
        assert_eq!(Language::TypeScript.id(), "typescript");
        assert_eq!(Language::JavaScript.id(), "javascript");
        assert_eq!(Language::Go.id(), "go");
    }

    #[test]
    fn test_file_uri() {
        // Already a URI should pass through on all platforms
        let uri = LspClient::file_uri("file:///already/a/uri.rs");
        assert_eq!(uri, "file:///already/a/uri.rs");

        // Unix-style absolute paths only valid on non-Windows
        #[cfg(not(target_os = "windows"))]
        {
            let uri = LspClient::file_uri("/home/user/project/src/main.rs");
            assert_eq!(uri, "file:///home/user/project/src/main.rs");
        }
    }

    #[test]
    fn test_uri_to_path() {
        assert_eq!(LspClient::uri_to_path("/plain/path"), "/plain/path");

        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            LspClient::uri_to_path("file:///home/user/main.rs"),
            "/home/user/main.rs"
        );
    }

    #[test]
    fn test_parse_locations_null() {
        let locs = LspClient::parse_locations(&Value::Null).unwrap();
        assert!(locs.is_empty());
    }

    #[test]
    fn test_parse_locations_single() {
        let val = serde_json::json!({
            "uri": "file:///src/main.rs",
            "range": {
                "start": { "line": 10, "character": 5 },
                "end": { "line": 10, "character": 15 }
            }
        });
        let locs = LspClient::parse_locations(&val).unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].file, "/src/main.rs");
        assert_eq!(locs[0].line, 10);
        assert_eq!(locs[0].column, 5);
    }

    #[test]
    fn test_parse_locations_array() {
        let val = serde_json::json!([
            {
                "uri": "file:///a.rs",
                "range": { "start": { "line": 1, "character": 2 }, "end": { "line": 1, "character": 10 } }
            },
            {
                "uri": "file:///b.rs",
                "range": { "start": { "line": 5, "character": 0 }, "end": { "line": 5, "character": 8 } }
            }
        ]);
        let locs = LspClient::parse_locations(&val).unwrap();
        assert_eq!(locs.len(), 2);
        assert_eq!(locs[0].file, "/a.rs");
        assert_eq!(locs[1].file, "/b.rs");
    }

    #[test]
    fn test_parse_locations_location_link() {
        let val = serde_json::json!([{
            "targetUri": "file:///target.rs",
            "targetSelectionRange": {
                "start": { "line": 20, "character": 4 },
                "end": { "line": 20, "character": 12 }
            },
            "targetRange": {
                "start": { "line": 18, "character": 0 },
                "end": { "line": 25, "character": 1 }
            }
        }]);
        let locs = LspClient::parse_locations(&val).unwrap();
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].file, "/target.rs");
        assert_eq!(locs[0].line, 20);
        assert_eq!(locs[0].column, 4);
    }

    #[test]
    fn test_parse_symbols_flat() {
        let val = serde_json::json!([
            {
                "name": "main",
                "kind": 12,
                "location": {
                    "uri": "file:///main.rs",
                    "range": { "start": { "line": 0, "character": 3 }, "end": { "line": 5, "character": 1 } }
                }
            },
            {
                "name": "Config",
                "kind": 23,
                "location": {
                    "uri": "file:///main.rs",
                    "range": { "start": { "line": 7, "character": 4 }, "end": { "line": 10, "character": 1 } }
                }
            }
        ]);
        let symbols = LspClient::parse_symbols(&val).unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, "function");
        assert_eq!(symbols[1].name, "Config");
        assert_eq!(symbols[1].kind, "struct");
    }

    #[test]
    fn test_parse_symbols_hierarchical() {
        let val = serde_json::json!([{
            "name": "MyStruct",
            "kind": 23,
            "selectionRange": { "start": { "line": 0, "character": 4 }, "end": { "line": 0, "character": 12 } },
            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 10, "character": 1 } },
            "children": [
                {
                    "name": "field_a",
                    "kind": 8,
                    "selectionRange": { "start": { "line": 1, "character": 4 }, "end": { "line": 1, "character": 11 } },
                    "range": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 20 } }
                },
                {
                    "name": "do_thing",
                    "kind": 6,
                    "selectionRange": { "start": { "line": 5, "character": 7 }, "end": { "line": 5, "character": 15 } },
                    "range": { "start": { "line": 5, "character": 0 }, "end": { "line": 9, "character": 1 } }
                }
            ]
        }]);
        let symbols = LspClient::parse_symbols(&val).unwrap();
        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "MyStruct");
        assert_eq!(symbols[0].kind, "struct");
        assert_eq!(symbols[1].name, "field_a");
        assert_eq!(symbols[1].kind, "field");
        assert_eq!(symbols[2].name, "do_thing");
        assert_eq!(symbols[2].kind, "method");
    }

    #[test]
    fn test_symbol_kind_name() {
        assert_eq!(symbol_kind_name(12), "function");
        assert_eq!(symbol_kind_name(5), "class");
        assert_eq!(symbol_kind_name(23), "struct");
        assert_eq!(symbol_kind_name(6), "method");
        assert_eq!(symbol_kind_name(999), "unknown");
    }

    #[test]
    fn test_diagnostics_parsing() {
        let params = serde_json::json!({
            "uri": "file:///test.rs",
            "diagnostics": [
                {
                    "range": { "start": { "line": 5, "character": 10 }, "end": { "line": 5, "character": 20 } },
                    "severity": 1,
                    "message": "expected `;`"
                },
                {
                    "range": { "start": { "line": 12, "character": 0 }, "end": { "line": 12, "character": 15 } },
                    "severity": 2,
                    "message": "unused variable"
                }
            ]
        });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let store: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>> =
                Arc::new(Mutex::new(HashMap::new()));

            LspServerConnection::handle_diagnostics(&params, &store).await;

            let s = store.lock().await;
            let diags = s.get("file:///test.rs").unwrap();
            assert_eq!(diags.len(), 2);
            assert_eq!(diags[0].severity, "error");
            assert_eq!(diags[0].message, "expected `;`");
            assert_eq!(diags[0].line, 5);
            assert_eq!(diags[1].severity, "warning");
            assert_eq!(diags[1].message, "unused variable");
        });
    }

    #[test]
    fn test_server_candidates() {
        let rust_candidates = server_candidates(Language::Rust);
        assert!(!rust_candidates.is_empty());
        assert_eq!(rust_candidates[0].0, "rust-analyzer");

        let py_candidates = server_candidates(Language::Python);
        assert!(py_candidates.len() >= 2);

        let go_candidates = server_candidates(Language::Go);
        assert_eq!(go_candidates[0].0, "gopls");
    }

    #[test]
    fn test_location_serialization() {
        let loc = Location {
            file: "/src/main.rs".to_string(),
            line: 42,
            column: 7,
        };
        let json = serde_json::to_value(&loc).unwrap();
        assert_eq!(json["file"], "/src/main.rs");
        assert_eq!(json["line"], 42);
        assert_eq!(json["column"], 7);
    }

    #[test]
    fn test_symbol_info_serialization() {
        let sym = SymbolInfo {
            name: "my_func".to_string(),
            kind: "function".to_string(),
            line: 10,
            column: 0,
        };
        let json = serde_json::to_value(&sym).unwrap();
        assert_eq!(json["name"], "my_func");
        assert_eq!(json["kind"], "function");
    }

    #[test]
    fn test_diagnostic_serialization() {
        let diag = Diagnostic {
            message: "type mismatch".to_string(),
            severity: "error".to_string(),
            line: 15,
            column: 8,
        };
        let json = serde_json::to_value(&diag).unwrap();
        assert_eq!(json["severity"], "error");
        assert_eq!(json["message"], "type mismatch");
    }
}
