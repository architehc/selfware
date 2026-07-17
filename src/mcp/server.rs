//! MCP server implementation.
//!
//! Exposes Selfware's tools and project resources to external AI clients
//! via the Model Context Protocol (JSON-RPC 2.0 over stdio).
//!
//! The wire framing of each incoming message is auto-detected:
//! newline-delimited JSON-RPC (the MCP stdio spec, protocol 2024-11-05) or
//! LSP-style `Content-Length` headers (legacy clients). Every response is
//! written in the same framing as the request that produced it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, RwLock, Semaphore};
use tracing::{debug, info};

use crate::safety::SafetyChecker;
use crate::tools::ToolRegistry;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 message types
// ---------------------------------------------------------------------------

/// Incoming JSON-RPC 2.0 request (or notification when `id` is `None`).
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// Outgoing JSON-RPC 2.0 response.
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

// Standard JSON-RPC error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;

/// MCP protocol version we implement.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "selfware";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------------------------------------------------------------------------
// Wire framing (newline-delimited JSON-RPC and LSP-style Content-Length)
// ---------------------------------------------------------------------------

/// Wire framing detected for an incoming message.
///
/// The MCP stdio spec (protocol 2024-11-05) frames messages as
/// newline-delimited JSON-RPC, but this server historically spoke (and some
/// LSP-derived clients still speak) `Content-Length` header framing. The
/// framing is auto-detected per message, and each response is written in the
/// same framing as the request that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    /// LSP-style `Content-Length: <n>\r\n\r\n<body>`.
    ContentLength,
    /// One JSON-RPC message per line (MCP stdio spec, 2024-11-05).
    NewlineDelimited,
}

/// Peek at the buffered bytes to detect which framing the peer is using.
///
/// A header-framed message always starts with `Content-Length:`, while a
/// newline-delimited JSON-RPC message is a single JSON object on one line —
/// and a JSON document can never start with `C`. The first byte therefore
/// decides unambiguously, even if the peer's write arrived in several chunks.
/// Returns `Ok(None)` on EOF.
async fn detect_framing<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Framing>> {
    let buf = reader.fill_buf().await?;
    if buf.is_empty() {
        // EOF
        return Ok(None);
    }
    Ok(Some(if buf[0] == b'C' {
        Framing::ContentLength
    } else {
        Framing::NewlineDelimited
    }))
}

/// Read the next message from `reader`, auto-detecting its framing.
///
/// Returns `Ok(None)` on clean EOF and `Ok(Some((body, framing)))` on a
/// successful read. The returned framing is the one the response to this
/// message must use so the peer sees the same framing it sent.
///
/// Test-only helper: the serve loop calls [`detect_framing`] and the two
/// framing-specific readers directly so it can answer malformed frames in
/// the framing they arrived in.
#[cfg(test)]
async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<(String, Framing)>> {
    let framing = match detect_framing(reader).await? {
        Some(framing) => framing,
        None => return Ok(None),
    };
    let body = match framing {
        Framing::ContentLength => read_content_length_message(reader).await?,
        Framing::NewlineDelimited => read_newline_message(reader).await?,
    };
    Ok(body.map(|body| (body, framing)))
}

/// Read a single Content-Length framed message from `reader`.
///
/// The format is:
/// ```text
/// Content-Length: <n>\r\n
/// \r\n
/// <n bytes of JSON>
/// ```
///
/// Returns `Ok(None)` on EOF, `Ok(Some(body))` on a successful read, and
/// `Err` when headers are malformed or the body is truncated.
async fn read_content_length_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    // Read headers until we hit the empty line.
    loop {
        let mut header_line = String::new();
        let bytes_read = reader.read_line(&mut header_line).await?;
        if bytes_read == 0 {
            // EOF
            return Ok(None);
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            // End of headers.
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("Invalid Content-Length value")?,
            );
        }
        // Ignore other headers (e.g. Content-Type).
    }

    let length = content_length.context("Missing Content-Length header")?;

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await?;

    String::from_utf8(buf)
        .context("Message body is not valid UTF-8")
        .map(Some)
}

/// Read a single newline-delimited JSON-RPC message (one line).
///
/// Per the MCP stdio spec (2024-11-05), messages are UTF-8 JSON-RPC with no
/// embedded newlines, delimited by `\n`. Returns `Ok(None)` on EOF.
async fn read_newline_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        // EOF
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

/// Write a Content-Length framed message to `writer`.
async fn write_message<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, body: &str) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

/// Write a message to `writer` using the requested framing.
///
/// Responses must use the framing of the request they answer. The compact
/// `serde_json` serialization escapes control characters inside strings, so
/// a newline-delimited body never contains a raw `\n` of its own.
async fn write_framed_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
    framing: Framing,
) -> Result<()> {
    match framing {
        Framing::ContentLength => write_message(writer, body).await,
        Framing::NewlineDelimited => {
            writer.write_all(body.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// MCP server that exposes Selfware tools and project resources.
pub struct McpServer {
    /// The tool registry containing all available tools.
    ///
    /// Wrapped in `Arc<RwLock<…>>` so that multiple concurrent handler tasks
    /// can share read access to the registry for `list`/`get`/`execute`,
    /// while `activate` (which needs `&mut`) takes a brief write lock that
    /// is **never** held across the tool-execution `.await`.
    registry: Arc<RwLock<ToolRegistry>>,
    /// Root directory for project resource exposure.
    project_root: PathBuf,
    /// Whether the server has been initialized.
    ///
    /// `Arc<AtomicBool>` so that the flag set by the `initialize` handler
    /// is visible to all concurrently-spawned handler tasks without needing
    /// `&mut self` on `McpServer`.
    initialized: Arc<AtomicBool>,
    /// Gates every tools/call the same way the CLI/TUI agent loop does --
    /// without this, an MCP client (any external AI tool speaking the
    /// protocol) could invoke any registered tool, including shell_exec/
    /// file_delete/git_push, with none of the checks in src/safety/ applied.
    safety: SafetyChecker,
    /// Optional explicit config path override (from `--config` CLI flag).
    config_path: Option<String>,
    /// Kept so `with_project_root` doesn't need to recompute the env cwd.
    _project_root_env: PathBuf,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Load the safety config for the MCP server. Falls back to defaults (which
/// are already conservative -- allowed_paths=["./**"], the standard
/// denied_paths/require_confirmation lists) if no config file is found or it
/// fails to parse, rather than refusing to start the server entirely.
fn load_mcp_safety_config(config_path: Option<&str>) -> crate::config::SafetyConfig {
    match crate::config::Config::load(config_path) {
        Ok(config) => config.safety,
        Err(e) => {
            tracing::warn!(
                "MCP server: failed to load selfware config ({}), using default safety settings",
                e
            );
            crate::config::SafetyConfig::default()
        }
    }
}

impl McpServer {
    /// Create a new MCP server with the default tool registry.
    pub fn new() -> Self {
        Self::with_config(None)
    }

    /// Create a new MCP server with a custom project root.
    pub fn with_project_root(project_root: PathBuf) -> Self {
        let project_root_env = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            registry: Arc::new(RwLock::new(ToolRegistry::new())),
            project_root,
            initialized: Arc::new(AtomicBool::new(false)),
            safety: SafetyChecker::new(&load_mcp_safety_config(None)),
            config_path: None,
            _project_root_env: project_root_env,
        }
    }

    /// Create a new MCP server with an explicit config path override.
    /// When `config_path` is `Some`, the config is loaded from that path
    /// instead of the default search.  This threads the `--config` CLI
    /// flag into the MCP server so it uses the same config as the rest
    /// of the application.
    pub fn with_config(config_path: Option<String>) -> Self {
        let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            registry: Arc::new(RwLock::new(ToolRegistry::new())),
            project_root,
            initialized: Arc::new(AtomicBool::new(false)),
            safety: SafetyChecker::new(&load_mcp_safety_config(config_path.as_deref())),
            config_path,
            _project_root_env: PathBuf::from("."),
        }
    }

    /// Handle a parsed JSON-RPC request and return a response (or `None` for notifications).
    pub async fn handle_request(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = match &request.id {
            Some(id) => id.clone(),
            None => {
                // This is a notification — no response needed.
                self.handle_notification(request).await;
                return None;
            }
        };

        // Gate: methods other than `initialize` and `ping` require the
        // `initialize` handshake to have been completed first (MCP spec).
        // Without this, a client could call `tools/call` before initializing.
        const PRE_INIT_METHODS: &[&str] = &["initialize", "ping", "shutdown"];
        if !self.initialized.load(Ordering::SeqCst)
            && !PRE_INIT_METHODS.contains(&request.method.as_str())
        {
            return Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: INVALID_REQUEST,
                    message: format!(
                        "Method '{}' requires an 'initialize' request first",
                        request.method
                    ),
                    data: None,
                }),
            });
        }

        let (result, error) = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.params),
            "tools/list" => self.handle_tools_list(&request.params).await,
            "tools/call" => self.handle_tools_call(&request.params).await,
            "resources/list" => self.handle_resources_list(&request.params),
            "resources/read" => self.handle_resources_read(&request.params).await,
            "ping" => (Some(serde_json::json!({})), None),
            "shutdown" => {
                info!("MCP server received shutdown request");
                (Some(serde_json::json!({})), None)
            }
            _ => (
                None,
                Some(JsonRpcError {
                    code: METHOD_NOT_FOUND,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            ),
        };

        Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result,
            error,
        })
    }

    /// Handle notifications (no response expected).
    async fn handle_notification(&self, request: &JsonRpcRequest) {
        match request.method.as_str() {
            "notifications/initialized" => {
                info!("MCP client confirmed initialization");
            }
            "notifications/cancelled" => {
                if let Some(params) = &request.params {
                    let request_id = params.get("requestId");
                    debug!("Client cancelled request: {:?}", request_id);
                }
            }
            "notifications/exit" => {
                info!("MCP client sent exit notification");
            }
            _ => {
                debug!("Unhandled notification: {}", request.method);
            }
        }
    }

    // -- Method handlers -----------------------------------------------------

    /// Handle `initialize` request.
    fn handle_initialize(&self, params: &Option<Value>) -> (Option<Value>, Option<JsonRpcError>) {
        // Check the client's requested protocolVersion.  The MCP spec says
        // the server SHOULD negotiate a version.  We support
        // "2024-11-05"; if the client requests a different version we
        // still respond with our version (the client can decide whether
        // to proceed), but we log a warning so the operator is aware.
        if let Some(p) = params {
            if let Some(client_version) = p.get("protocolVersion").and_then(|v| v.as_str()) {
                if client_version != MCP_PROTOCOL_VERSION {
                    tracing::warn!(
                        "MCP client requested protocol version '{}', we support '{}'",
                        client_version,
                        MCP_PROTOCOL_VERSION
                    );
                }
            }
        }

        self.initialized.store(true, Ordering::SeqCst);

        let result = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {},
                "resources": {
                    "subscribe": false,
                    "listChanged": false
                }
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            }
        });

        info!("MCP server initialized (protocol {})", MCP_PROTOCOL_VERSION);
        (Some(result), None)
    }

    /// Handle `tools/list` request.
    async fn handle_tools_list(
        &self,
        _params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let registry = self.registry.read().await;
        let tools: Vec<Value> = registry
            .list()
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "inputSchema": tool.schema()
                })
            })
            .collect();

        debug!("tools/list returning {} tools", tools.len());
        (Some(serde_json::json!({ "tools": tools })), None)
    }

    /// Handle `tools/call` request.
    ///
    /// If the requested tool exists but has not been activated yet (it's
    /// deferred), it is activated on demand — the same way the agent's tool
    /// discovery activates tools — so that every tool advertised by
    /// `tools/list` can actually be called.
    ///
    /// **Concurrency note:** The registry is behind an `Arc<RwLock<…>>`.
    /// Read locks are used for schema validation and tool lookup.  A *brief*
    /// write lock is taken only for the `activate` call, and it is dropped
    /// before the tool-execution `.await` so that concurrent calls are not
    /// serialized by the lock.
    async fn handle_tools_call(
        &self,
        params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let params = match params {
            Some(p) => p,
            None => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: "Missing params for tools/call".to_string(),
                        data: None,
                    }),
                );
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(name) => name,
            None => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: "Missing 'name' parameter in tools/call".to_string(),
                        data: None,
                    }),
                );
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        debug!("tools/call: {} with args: {}", tool_name, arguments);

        // Validate the provided arguments against the tool's declared input
        // schema *before* executing.  Without this, a client could call a
        // tool with missing required fields or wrong argument types and the
        // tool would receive malformed input, potentially causing confusing
        // errors or undefined behaviour.  On mismatch we return a proper
        // JSON-RPC invalid-params error.
        //
        // If the tool is not registered, skip validation (the execute call
        // below will produce a tool-not-found error response).  If the tool
        // is registered but exposes no usable schema (e.g. schema is not an
        // object), skip validation for that tool.
        {
            let registry = self.registry.read().await;
            if let Some(tool) = registry.get(tool_name) {
                let schema = tool.schema();
                if schema.is_object() {
                    if let Err(e) =
                        crate::tools::validate_tool_arguments_schema(tool_name, &schema, &arguments)
                    {
                        return (
                            None,
                            Some(JsonRpcError {
                                code: INVALID_PARAMS,
                                message: e.to_string(),
                                data: Some(serde_json::json!({
                                    "tool": tool_name,
                                    "arguments": arguments,
                                })),
                            }),
                        );
                    }
                }
            }
        }

        let fake_call = crate::api::types::ToolCall {
            id: "mcp".to_string(),
            call_type: "function".to_string(),
            function: crate::api::types::ToolFunction {
                name: tool_name.to_string(),
                arguments: arguments.to_string(),
            },
        };
        if let Err(e) = self.safety.check_tool_call(&fake_call) {
            let response = serde_json::json!({
                "content": [
                    {
                        "type": "text",
                        "text": format!("Blocked by safety checker: {}", e)
                    }
                ],
                "isError": true
            });
            return (Some(response), None);
        }

        // If the tool exists but hasn't been activated yet, activate it on
        // demand so that every tool advertised in tools/list is actually
        // callable.  This mirrors how the agent's tool discovery activates
        // deferred tools before first use.
        //
        // We take a brief write lock for `activate` and drop it immediately
        // — never holding it across the `execute` await below, to avoid
        // serializing concurrent tool calls.
        {
            let needs_activate = {
                let registry = self.registry.read().await;
                registry.get(tool_name).is_some() && registry.get_activated(tool_name).is_none()
            };
            if needs_activate {
                debug!(
                    "tools/call: activating deferred tool '{}' on demand",
                    tool_name
                );
                let mut registry = self.registry.write().await;
                registry.activate(tool_name);
            }
        }

        let result = {
            let registry = self.registry.read().await;
            registry.execute(tool_name, arguments).await
        };

        match result {
            Ok(result) => {
                // Convert tool result to MCP content format.
                let text = match result.as_str() {
                    Some(s) => s.to_string(),
                    None => serde_json::to_string_pretty(&result).unwrap_or_default(),
                };

                let response = serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": text
                        }
                    ],
                    "isError": false
                });
                (Some(response), None)
            }
            Err(err) => {
                let response = serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Error: {}", err)
                        }
                    ],
                    "isError": true
                });
                (Some(response), None)
            }
        }
    }

    /// Handle `resources/list` request.
    fn handle_resources_list(
        &self,
        _params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let resources = vec![
            serde_json::json!({
                "uri": "selfware://project/files",
                "name": "Project Files",
                "description": "List all project files in the working directory",
                "mimeType": "application/json"
            }),
            serde_json::json!({
                "uri": "selfware://project/structure",
                "name": "Project Structure",
                "description": "Directory tree of the project",
                "mimeType": "text/plain"
            }),
            serde_json::json!({
                "uri": "selfware://config",
                "name": "Selfware Configuration",
                "description": "Current selfware configuration",
                "mimeType": "application/json"
            }),
        ];

        (Some(serde_json::json!({ "resources": resources })), None)
    }

    /// Handle `resources/read` request.
    async fn handle_resources_read(
        &self,
        params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let params = match params {
            Some(p) => p,
            None => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: "Missing params for resources/read".to_string(),
                        data: None,
                    }),
                );
            }
        };

        let uri = match params.get("uri").and_then(|u| u.as_str()) {
            Some(uri) => uri,
            None => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: "Missing 'uri' parameter in resources/read".to_string(),
                        data: None,
                    }),
                );
            }
        };

        match uri {
            "selfware://project/files" => {
                let files = self.list_project_files();
                let content = serde_json::to_string_pretty(&files).unwrap_or_default();
                (
                    Some(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": content
                        }]
                    })),
                    None,
                )
            }
            "selfware://project/structure" => {
                let tree = self.build_directory_tree(&self.project_root, 0, 4);
                (
                    Some(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "text/plain",
                            "text": tree
                        }]
                    })),
                    None,
                )
            }
            "selfware://config" => {
                let config = match crate::config::Config::load(self.config_path.as_deref()) {
                    Ok(cfg) => serde_json::to_string_pretty(&cfg).unwrap_or_default(),
                    Err(e) => format!("{{\"error\": \"{}\"}}", e),
                };
                (
                    Some(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": "application/json",
                            "text": config
                        }]
                    })),
                    None,
                )
            }
            _ => {
                if let Some(file_path) = uri.strip_prefix("selfware://project/file/") {
                    self.read_project_file(uri, file_path)
                } else {
                    (
                        None,
                        Some(JsonRpcError {
                            code: INVALID_PARAMS,
                            message: format!("Unknown resource URI: {}", uri),
                            data: None,
                        }),
                    )
                }
            }
        }
    }

    // -- Resource helpers ----------------------------------------------------

    /// List project files (up to a bounded limit).
    fn list_project_files(&self) -> Vec<String> {
        const MAX_FILES: usize = 10_000;
        let mut files = Vec::new();

        let walker = walkdir::WalkDir::new(&self.project_root)
            .max_depth(8)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Skip hidden dirs, target, node_modules, etc.
                !name.starts_with('.')
                    && name != "target"
                    && name != "node_modules"
                    && name != "__pycache__"
                    && name != ".git"
            });

        for entry in walker {
            if files.len() >= MAX_FILES {
                break;
            }
            if let Ok(entry) = entry {
                if entry.file_type().is_file() {
                    if let Ok(relative) = entry.path().strip_prefix(&self.project_root) {
                        files.push(relative.to_string_lossy().to_string());
                    }
                }
            }
        }

        files
    }

    /// Build a simple directory tree string.
    #[allow(clippy::only_used_in_recursion)]
    fn build_directory_tree(&self, path: &Path, depth: usize, max_depth: usize) -> String {
        if depth >= max_depth {
            return String::new();
        }

        let mut result = String::new();
        let indent = "  ".repeat(depth);

        let dir_name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());
        result.push_str(&format!("{}{}/\n", indent, dir_name));

        if let Ok(entries) = std::fs::read_dir(path) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());

            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden, target, node_modules
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }

                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    result.push_str(&self.build_directory_tree(
                        &entry.path(),
                        depth + 1,
                        max_depth,
                    ));
                } else {
                    result.push_str(&format!("{}  {}\n", indent, name));
                }
            }
        }

        result
    }

    /// Read a project file by relative path.
    fn read_project_file(
        &self,
        uri: &str,
        relative_path: &str,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let full_path = self.project_root.join(relative_path);

        // Safety: ensure the resolved path is within the project root.
        match full_path.canonicalize() {
            Ok(canonical) => {
                if let Ok(root_canonical) = self.project_root.canonicalize() {
                    if !canonical.starts_with(&root_canonical) {
                        return (
                            None,
                            Some(JsonRpcError {
                                code: INVALID_PARAMS,
                                message: "Path escapes project root".to_string(),
                                data: None,
                            }),
                        );
                    }
                }
            }
            Err(e) => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INVALID_PARAMS,
                        message: format!("Cannot resolve path '{}': {}", relative_path, e),
                        data: None,
                    }),
                );
            }
        }

        match std::fs::read_to_string(&full_path) {
            Ok(content) => {
                let mime = if relative_path.ends_with(".json") {
                    "application/json"
                } else if relative_path.ends_with(".toml") {
                    "application/toml"
                } else if relative_path.ends_with(".yaml") || relative_path.ends_with(".yml") {
                    "application/yaml"
                } else {
                    "text/plain"
                };

                (
                    Some(serde_json::json!({
                        "contents": [{
                            "uri": uri,
                            "mimeType": mime,
                            "text": content
                        }]
                    })),
                    None,
                )
            }
            Err(e) => (
                None,
                Some(JsonRpcError {
                    code: INTERNAL_ERROR,
                    message: format!("Failed to read file '{}': {}", relative_path, e),
                    data: None,
                }),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the MCP server, reading from stdin and writing to stdout.
///
/// This function blocks until stdin is closed or a shutdown request is received.
///
/// `config_path` is the resolved path to the config file (from `--config` or
/// default search).  When `Some`, it is threaded into the server so that
/// internal `Config::load` calls (e.g. for the `selfware://config` resource)
/// use the same file rather than falling back to the default search.
pub async fn run_mcp_server(
    _config: &crate::config::Config,
    config_path: Option<&str>,
) -> Result<()> {
    // Redirect tracing output to stderr so it doesn't interfere with the JSON-RPC
    // protocol on stdout.
    eprintln!("selfware MCP server v{} starting...", SERVER_VERSION);
    info!("MCP server starting on stdio transport");

    // Wrap the server in an Arc so it can be cheaply cloned into each
    // per-request handler task.  All interior mutability is handled via
    // Arc<AtomicBool> (initialized flag) and Arc<RwLock<ToolRegistry>>.
    let server = Arc::new(McpServer::with_config(config_path.map(|s| s.to_string())));

    serve_io(server, tokio::io::stdin(), tokio::io::stdout()).await?;

    eprintln!("selfware MCP server stopped.");
    Ok(())
}

/// Serve MCP requests read from `input`, writing responses to `output`.
///
/// The framing of each request is auto-detected (see [`detect_framing`]) and
/// every response is written in the same framing as its request. Runs until
/// EOF on `input` or a `shutdown` request / `notifications/exit` notification.
///
/// Split out from [`run_mcp_server`] so the full wire protocol can be tested
/// over in-memory duplex streams instead of real stdin/stdout.
async fn serve_io<I, O>(server: Arc<McpServer>, input: I, output: O) -> Result<()>
where
    I: tokio::io::AsyncRead + Unpin,
    O: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let mut reader = BufReader::new(input);

    // Bounded channel (256) for responses — provides backpressure so a fast
    // client cannot create unlimited in-flight response messages.  The single
    // writer task drains this channel and serializes all output writes.
    // Each item carries the framing the response must be written in (always
    // the framing of the request that produced it).
    let (tx, mut rx) = mpsc::channel::<(String, Framing)>(256);

    // Semaphore (32 permits) to cap concurrent in-flight request handler
    // tasks.  Each request acquires a permit before spawning; the permit is
    // dropped when the handler completes, preventing unbounded task growth.
    let request_semaphore = Arc::new(Semaphore::new(32));

    // Spawn ONE writer task that owns the output stream — this serializes all
    // writes so concurrent handler tasks never interleave their frames.
    let writer_task = tokio::spawn(async move {
        let mut out = output;
        while let Some((body, framing)) = rx.recv().await {
            // Ignore write errors — there's nothing we can do if the output is broken.
            if let Err(e) = write_framed_message(&mut out, &body, framing).await {
                eprintln!("MCP server: write error: {}", e);
            }
        }
    });

    loop {
        // Peek at the next bytes to learn the framing before consuming the
        // message; the response must use the same framing.
        let framing = match detect_framing(&mut reader).await {
            Ok(Some(framing)) => framing,
            Ok(None) => {
                info!("MCP server: input closed, shutting down");
                break;
            }
            Err(e) => {
                // I/O error on the input stream itself — nothing sensible we
                // can answer, so shut down rather than spin.
                debug!("MCP server: input read error: {}", e);
                break;
            }
        };

        let message = match framing {
            Framing::ContentLength => read_content_length_message(&mut reader).await,
            Framing::NewlineDelimited => read_newline_message(&mut reader).await,
        };
        let message = match message {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                info!("MCP server: input closed, shutting down");
                break;
            }
            Err(e) => {
                // Malformed frame — send a parse error response in the same
                // framing the broken bytes arrived in.
                let error_response = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: PARSE_ERROR,
                        message: format!("Failed to read message: {}", e),
                        data: None,
                    }),
                };
                if let Ok(body) = serde_json::to_string(&error_response) {
                    let _ = tx.send((body, framing)).await;
                }
                continue;
            }
        };

        // Parse the JSON-RPC request.
        let request: JsonRpcRequest = match serde_json::from_str(&message) {
            Ok(req) => req,
            Err(e) => {
                let error_response = JsonRpcResponse {
                    jsonrpc: "2.0",
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: PARSE_ERROR,
                        message: format!("Invalid JSON: {}", e),
                        data: None,
                    }),
                };
                if let Ok(body) = serde_json::to_string(&error_response) {
                    let _ = tx.send((body, framing)).await;
                }
                continue;
            }
        };

        debug!(
            "MCP server received: {} (id={:?})",
            request.method, request.id
        );

        // Check if this is a shutdown request or exit notification.
        let is_shutdown = request.method == "shutdown" || request.method == "notifications/exit";

        // Spawn a task to handle each request concurrently.  The task
        // clones the Arc<McpServer> handle and the channel sender, runs
        // `handle_request`, and sends the serialized response (if any) to
        // the writer task.  Notifications (None response) send nothing.
        //
        // A semaphore permit is acquired before spawning to cap in-flight
        // tasks at 32; the permit is held for the lifetime of the handler
        // task and released on completion.
        {
            let server = Arc::clone(&server);
            let tx = tx.clone();
            let permit = request_semaphore.clone().acquire_owned().await;
            // If the semaphore is closed (shouldn't happen in normal operation),
            // skip spawning — but this is non-fatal.
            if permit.is_err() {
                continue;
            }
            let permit = permit.unwrap();
            tokio::spawn(async move {
                // _permit is held until the handler completes, bounding
                // the number of concurrent in-flight request tasks.
                let _permit = permit;
                if let Some(resp) = server.handle_request(&request).await {
                    if let Ok(body) = serde_json::to_string(&resp) {
                        let _ = tx.send((body, framing)).await;
                    }
                }
            });
        }

        if is_shutdown {
            info!("MCP server shutting down after shutdown request");
            break;
        }
    }

    // Drop the original sender so the writer task's `rx.recv()` will return
    // None once all in-flight handler tasks have finished sending.
    drop(tx);

    // Wait for the writer task to drain remaining responses and exit.
    let _ = writer_task.await;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: initialize a server so post-init methods can be tested.
    async fn initialize_server(server: &McpServer) {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(0)),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };
        server.handle_request(&req).await;
    }

    #[test]
    fn test_json_rpc_request_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.id, Some(Value::from(1)));
        assert_eq!(request.method, "initialize");
        assert!(request.params.is_some());
    }

    #[test]
    fn test_json_rpc_request_notification_parsing() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.method, "notifications/initialized");
        assert!(request.id.is_none());
        assert!(request.params.is_none());
    }

    #[test]
    fn test_json_rpc_response_serialization() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::from(1),
            result: Some(serde_json::json!({"ok": true})),
            error: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
        assert!(json.contains("\"ok\":true"));
        // error field should be skipped
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn test_json_rpc_error_response_serialization() {
        let response = JsonRpcResponse {
            jsonrpc: "2.0",
            id: Value::from(2),
            result: None,
            error: Some(JsonRpcError {
                code: METHOD_NOT_FOUND,
                message: "Method not found".to_string(),
                data: None,
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"code\":-32601"));
        assert!(json.contains("\"Method not found\""));
        // result field should be skipped
        assert!(!json.contains("\"result\""));
    }

    #[test]
    fn test_json_rpc_error_display() {
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found".to_string(),
            data: None,
        };
        assert_eq!(
            format!("{}", err),
            "JSON-RPC error -32601: Method not found"
        );
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(1)),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "test-client",
                    "version": "1.0.0"
                }
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        assert_eq!(
            result.get("protocolVersion").and_then(|v| v.as_str()),
            Some(MCP_PROTOCOL_VERSION)
        );
        assert_eq!(
            result
                .get("serverInfo")
                .and_then(|i| i.get("name"))
                .and_then(|n| n.as_str()),
            Some("selfware")
        );
        assert!(result.get("capabilities").is_some());
        assert!(server.initialized.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        let tools = result.get("tools").and_then(|t| t.as_array()).unwrap();
        // ToolRegistry::new() registers many tools; check that we got a reasonable number
        assert!(tools.len() > 10, "Expected many tools, got {}", tools.len());

        // Verify tool structure
        let first_tool = &tools[0];
        assert!(first_tool.get("name").is_some());
        assert!(first_tool.get("description").is_some());
        assert!(first_tool.get("inputSchema").is_some());
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_params() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(3)),
            method: "tools/call".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_tools_call_missing_name() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(4)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({"arguments": {}})),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_tools_call_unknown_tool() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(5)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "nonexistent_tool_xyz",
                "arguments": {}
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
        // Tool errors are returned as isError content, not JSON-RPC errors
        let result = response.result.unwrap();
        assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn test_handle_tools_call_blocks_dangerous_shell_command() {
        // Regression test: an MCP client (any external tool speaking the
        // protocol) used to be able to invoke any registered tool, including
        // shell_exec, with none of the src/safety/ checks applied at all.
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(7)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "shell_exec",
                "arguments": {"command": "rm -rf /"}
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(true));
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            text.contains("Blocked by safety checker"),
            "expected the dangerous command to be blocked, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_handle_tools_call_allows_safe_shell_command() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(8)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "shell_exec",
                "arguments": {"command": "echo hello"}
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        let result = response.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap_or("");
        assert!(
            !text.contains("Blocked by safety checker"),
            "expected the safe command to run, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_handle_resources_list() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(6)),
            method: "resources/list".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        let resources = result.get("resources").and_then(|r| r.as_array()).unwrap();
        assert_eq!(resources.len(), 3);

        // Check URIs
        let uris: Vec<&str> = resources
            .iter()
            .filter_map(|r| r.get("uri").and_then(|u| u.as_str()))
            .collect();
        assert!(uris.contains(&"selfware://project/files"));
        assert!(uris.contains(&"selfware://project/structure"));
        assert!(uris.contains(&"selfware://config"));
    }

    #[tokio::test]
    async fn test_handle_resources_read_missing_params() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(7)),
            method: "resources/read".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_resources_read_unknown_uri() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(8)),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({"uri": "selfware://unknown"})),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_resources_read_project_files() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(9)),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({"uri": "selfware://project/files"})),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        assert!(result.get("contents").is_some());
    }

    #[tokio::test]
    async fn test_handle_resources_read_project_structure() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(10)),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({"uri": "selfware://project/structure"})),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());

        let result = response.result.unwrap();
        let contents = result.get("contents").and_then(|c| c.as_array()).unwrap();
        assert!(!contents.is_empty());
    }

    #[tokio::test]
    async fn test_handle_method_not_found() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(11)),
            method: "unknown/method".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_handle_ping() {
        let server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(12)),
            method: "ping".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[tokio::test]
    async fn test_handle_notification_no_response() {
        let server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None, // Notification
            method: "notifications/initialized".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await;
        assert!(
            response.is_none(),
            "Notifications should not produce a response"
        );
    }

    #[tokio::test]
    async fn test_content_length_framing_roundtrip() {
        let message_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

        // Write
        let mut buffer: Vec<u8> = Vec::new();
        write_message(&mut buffer, message_body).await.unwrap();

        let expected = format!(
            "Content-Length: {}\r\n\r\n{}",
            message_body.len(),
            message_body
        );
        assert_eq!(String::from_utf8(buffer.clone()).unwrap(), expected);

        // Read back — auto-detection must pick Content-Length framing.
        let mut reader = BufReader::new(buffer.as_slice());
        let (read_back, framing) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(read_back, message_body);
        assert_eq!(framing, Framing::ContentLength);
    }

    #[tokio::test]
    async fn test_read_message_eof() {
        let empty: &[u8] = b"";
        let mut reader = BufReader::new(empty);
        let result = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Wire framing: auto-detection and framing-matched responses (P0-8)
    //
    // The MCP stdio spec (2024-11-05) frames messages as newline-delimited
    // JSON-RPC, but this server originally only spoke LSP-style
    // Content-Length framing — spec-standard clients got zero bytes back and
    // the server looked dead. The server now auto-detects the framing per
    // message and answers in the same framing.
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_newline_framing_roundtrip() {
        let message_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_framed_message(&mut buffer, message_body, Framing::NewlineDelimited)
            .await
            .unwrap();

        // On the wire: exactly the body plus a trailing newline, no headers.
        let on_wire = String::from_utf8(buffer.clone()).unwrap();
        assert_eq!(on_wire, format!("{}\n", message_body));

        let mut reader = BufReader::new(buffer.as_slice());
        let (read_back, framing) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(read_back, message_body);
        assert_eq!(framing, Framing::NewlineDelimited);

        // After the message, the next read must hit clean EOF.
        let eof = read_message(&mut reader).await.unwrap();
        assert!(eof.is_none());
    }

    #[tokio::test]
    async fn test_detect_framing_per_message() {
        // A Content-Length message followed by a newline-delimited one on the
        // same stream must each be detected correctly.
        let cl_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let nl_body = r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_message(&mut buffer, cl_body).await.unwrap();
        write_framed_message(&mut buffer, nl_body, Framing::NewlineDelimited)
            .await
            .unwrap();

        let mut reader = BufReader::new(buffer.as_slice());
        let (body1, framing1) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(
            (body1.as_str(), framing1),
            (cl_body, Framing::ContentLength)
        );
        let (body2, framing2) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(
            (body2.as_str(), framing2),
            (nl_body, Framing::NewlineDelimited)
        );
        assert!(read_message(&mut reader).await.unwrap().is_none());
    }

    /// A newline-delimited message that arrives byte-by-byte must still be
    /// read as one message (BufReader awaits the rest of the line).
    #[tokio::test]
    async fn test_newline_framing_partial_writes() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        let (client, mut server_side) = tokio::io::duplex(4096);
        let writer = tokio::spawn(async move {
            for chunk in body.as_bytes().chunks(7) {
                server_side.write_all(chunk).await.unwrap();
                server_side.flush().await.unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            server_side.write_all(b"\n").await.unwrap();
            server_side.flush().await.unwrap();
        });

        let mut reader = BufReader::new(client);
        let (recovered, framing) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(recovered, body);
        assert_eq!(framing, Framing::NewlineDelimited);
        writer.await.unwrap();
    }

    // -- Full serve_io sessions over in-memory duplex streams ---------------

    /// Spin up `serve_io` on one end of a duplex pair; return the client end.
    fn spawn_test_server() -> (tokio::io::DuplexStream, tokio::task::JoinHandle<Result<()>>) {
        let server = Arc::new(McpServer::new());
        let (client, server_side) = tokio::io::duplex(1024 * 1024);
        let (server_in, server_out) = tokio::io::split(server_side);
        let handle = tokio::spawn(serve_io(server, server_in, server_out));
        (client, handle)
    }

    /// Send one newline-delimited request and read the newline-delimited
    /// response, asserting the wire shape (single line, no headers).
    async fn newline_roundtrip(
        writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
        reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        request: &str,
    ) -> Value {
        writer.write_all(request.as_bytes()).await.unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();

        let mut line = String::new();
        let n = reader.read_line(&mut line).await.unwrap();
        assert!(n > 0, "server must answer a newline-delimited request");
        assert!(
            !line.starts_with("Content-Length:"),
            "newline-delimited request must not get a header-framed response: {:?}",
            line
        );
        serde_json::from_str(&line)
            .unwrap_or_else(|e| panic!("response is not JSON: {e}: {line:?}"))
    }

    /// Send one Content-Length framed request and read the response,
    /// asserting the exact LSP-style wire shape.
    async fn content_length_roundtrip(
        writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
        reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        request: &str,
    ) -> Value {
        write_message(writer, request).await.unwrap();

        let mut header = String::new();
        let n = reader.read_line(&mut header).await.unwrap();
        assert!(n > 0, "server must answer a Content-Length request");
        let length: usize = header
            .strip_prefix("Content-Length: ")
            .unwrap_or_else(|| panic!("expected Content-Length header, got: {header:?}"))
            .trim()
            .parse()
            .expect("Content-Length must be a number");
        let mut blank = String::new();
        reader.read_line(&mut blank).await.unwrap();
        assert_eq!(blank, "\r\n", "expected CRLF after headers");

        let mut buf = vec![0u8; length];
        reader.read_exact(&mut buf).await.unwrap();
        serde_json::from_slice(&buf).unwrap_or_else(|e| panic!("response body is not JSON: {e}"))
    }

    /// Spec-standard session: newline-delimited initialize → tools/list →
    /// tools/call → shutdown, all answered newline-delimited. This is the
    /// exact scenario that previously got zero bytes back (P0-8).
    #[tokio::test]
    async fn test_serve_io_newline_delimited_session() {
        let (client, server_task) = spawn_test_server();
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = BufReader::new(client_read);

        // initialize
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"spec-client","version":"1.0"}}}"#,
        )
        .await;
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(resp["result"]["serverInfo"]["name"], "selfware");

        // notifications/initialized — no response expected.
        client_write
            .write_all(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
            .await
            .unwrap();
        client_write.write_all(b"\n").await.unwrap();
        client_write.flush().await.unwrap();

        // tools/list
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .await;
        assert_eq!(resp["id"], 2);
        let tools = resp["result"]["tools"].as_array().unwrap();
        assert!(tools.len() > 10, "tools/list should return the catalog");
        assert!(tools[0]["name"].is_string());
        assert!(tools[0]["inputSchema"].is_object());

        // tools/call
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shell_exec","arguments":{"command":"echo hello"}}}"#,
        )
        .await;
        assert_eq!(resp["id"], 3);
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("hello"), "expected echo output, got: {text}");

        // shutdown — server answers, then the serve loop exits cleanly.
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
        )
        .await;
        assert_eq!(resp["id"], 4);

        tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
            .await
            .expect("server must exit after shutdown")
            .unwrap()
            .unwrap();
    }

    /// Back-compat session: a legacy client speaking Content-Length framing
    /// must keep working, with Content-Length framed responses.
    #[tokio::test]
    async fn test_serve_io_content_length_session() {
        let (client, server_task) = spawn_test_server();
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = BufReader::new(client_read);

        // initialize
        let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"legacy-client","version":"1.0"}}}"#,
        )
        .await;
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);

        // tools/list
        let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .await;
        assert_eq!(resp["id"], 2);
        assert!(resp["result"]["tools"].as_array().unwrap().len() > 10);

        // tools/call
        let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shell_exec","arguments":{"command":"echo hello"}}}"#,
        )
        .await;
        assert_eq!(resp["id"], 3);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("hello"), "expected echo output, got: {text}");

        // shutdown
        let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
        )
        .await;
        assert_eq!(resp["id"], 4);

        tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
            .await
            .expect("server must exit after shutdown")
            .unwrap()
            .unwrap();
    }

    /// Framing is detected per message, not per connection: a client may mix
    /// framings on one stream and each response matches its request.
    #[tokio::test]
    async fn test_serve_io_mixed_framing_session() {
        let (client, server_task) = spawn_test_server();
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = BufReader::new(client_read);

        // newline-delimited initialize → newline-delimited response
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .await;
        assert_eq!(resp["id"], 1);

        // Content-Length ping → Content-Length response
        let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        )
        .await;
        assert_eq!(resp["id"], 2);
        assert!(resp["result"].is_object());

        // newline-delimited shutdown → newline-delimited response
        let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#,
        )
        .await;
        assert_eq!(resp["id"], 3);

        tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
            .await
            .expect("server must exit after shutdown")
            .unwrap()
            .unwrap();
    }

    /// Invalid JSON on a newline-delimited stream gets a newline-delimited
    /// JSON-RPC parse error, not silence.
    #[tokio::test]
    async fn test_serve_io_parse_error_matches_newline_framing() {
        let (client, server_task) = spawn_test_server();
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = BufReader::new(client_read);

        let resp = newline_roundtrip(&mut client_write, &mut reader, "this is not json").await;
        assert_eq!(resp["error"]["code"], PARSE_ERROR);
        assert!(resp["id"].is_null());

        // EOF on the client side shuts the server down cleanly. Both halves
        // of the duplex stream must go: tokio's split shares one underlying
        // stream, which only signals EOF once it is fully dropped.
        drop(client_write);
        drop(reader);
        tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
            .await
            .expect("server must exit on EOF")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_handle_resources_read_file_path_escape() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(13)),
            method: "resources/read".to_string(),
            params: Some(serde_json::json!({
                "uri": "selfware://project/file/../../../etc/passwd"
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        // Should either fail with error or be blocked by path validation
        // The exact behavior depends on canonicalize, but it should not
        // return /etc/passwd content.
        if let Some(result) = &response.result {
            // If it somehow returned content, verify it's not /etc/passwd
            if let Some(contents) = result.get("contents").and_then(|c| c.as_array()) {
                for content in contents {
                    if let Some(text) = content.get("text").and_then(|t| t.as_str()) {
                        assert!(!text.contains("root:"), "Path traversal should be blocked");
                    }
                }
            }
        }
        // Having an error response is also acceptable
    }

    #[tokio::test]
    async fn test_tool_list_has_correct_schema_format() {
        let server = McpServer::new();
        initialize_server(&server).await;

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(14)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        let result = response.result.unwrap();
        let tools = result.get("tools").and_then(|t| t.as_array()).unwrap();

        for tool in tools {
            let name = tool.get("name").and_then(|n| n.as_str()).unwrap();
            let description = tool.get("description").and_then(|d| d.as_str()).unwrap();
            let schema = tool.get("inputSchema").unwrap();

            assert!(!name.is_empty(), "Tool name should not be empty");
            assert!(
                !description.is_empty(),
                "Tool '{}' description should not be empty",
                name
            );
            // MCP requires inputSchema to be a JSON Schema object
            assert!(
                schema.is_object(),
                "Tool '{}' inputSchema should be an object",
                name
            );
        }
    }

    #[tokio::test]
    async fn test_handle_shutdown() {
        let server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(15)),
            method: "shutdown".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
    }

    #[tokio::test]
    async fn test_tools_call_rejected_before_initialize() {
        // A client must not be able to call tools/call before the
        // `initialize` handshake has been completed.
        let server = McpServer::new();
        assert!(!server.initialized.load(Ordering::SeqCst));

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(20)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "shell_exec",
                "arguments": {"command": "echo hi"}
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_REQUEST);
    }

    #[tokio::test]
    async fn test_tools_list_rejected_before_initialize() {
        let server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(21)),
            method: "tools/list".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, INVALID_REQUEST);
    }

    #[tokio::test]
    async fn test_ping_allowed_before_initialize() {
        let server = McpServer::new();
        assert!(!server.initialized.load(Ordering::SeqCst));

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(22)),
            method: "ping".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
        assert!(response.result.is_some());
    }

    #[tokio::test]
    async fn test_tools_call_allowed_after_initialize() {
        let server = McpServer::new();

        // Initialize first
        let init_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(23)),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };
        server.handle_request(&init_req).await;
        assert!(server.initialized.load(Ordering::SeqCst));

        // Now tools/call should not be rejected with INVALID_REQUEST
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(24)),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": "nonexistent_tool_xyz",
                "arguments": {}
            })),
        };

        let response = server.handle_request(&request).await.unwrap();
        // Should not get INVALID_REQUEST — the error (if any) should be
        // a tool-not-found content response, not a protocol error.
        if let Some(err) = &response.error {
            assert_ne!(err.code, INVALID_REQUEST);
        }
    }

    #[test]
    fn test_list_project_files_bounded() {
        let server = McpServer::new();
        let files = server.list_project_files();
        // Should return some files but be bounded
        assert!(files.len() <= 10_000);
    }

    #[test]
    fn test_build_directory_tree() {
        let server = McpServer::new();
        let tree = server.build_directory_tree(&server.project_root, 0, 2);
        // Should produce some output for the project root
        assert!(!tree.is_empty());
        // Should contain directory markers
        assert!(tree.contains('/'));
    }

    /// Verify that a cloned `McpServer` (as used by per-request tasks)
    /// shares the `initialized` flag — i.e. that `initialize` on one clone
    /// is visible to all others.  This is the core concurrency invariant.
    #[tokio::test]
    async fn test_concurrent_handle_request_via_shared_handle() {
        let server = std::sync::Arc::new(McpServer::new());

        // Initialize via the shared handle.
        let init_req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(100)),
            method: "initialize".to_string(),
            params: Some(serde_json::json!({})),
        };
        let _ = server.handle_request(&init_req).await.unwrap();
        assert!(server.initialized.load(std::sync::atomic::Ordering::SeqCst));

        // Spawn multiple concurrent tools/list requests through cloned handles.
        // If the shared state works, all should succeed (no INVALID_REQUEST).
        let mut handles = Vec::new();
        for i in 0..5u64 {
            let server = std::sync::Arc::clone(&server);
            handles.push(tokio::spawn(async move {
                let req = JsonRpcRequest {
                    jsonrpc: "2.0".to_string(),
                    id: Some(Value::from(200 + i)),
                    method: "tools/list".to_string(),
                    params: None,
                };
                server.handle_request(&req).await.unwrap()
            }));
        }

        for handle in handles {
            let response = handle.await.unwrap();
            assert!(
                response.error.is_none(),
                "concurrent tools/list should succeed"
            );
            assert!(response.result.is_some());
        }
    }
}
