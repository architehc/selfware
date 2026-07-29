//! MCP server implementation.
//!
//! Exposes Selfware's tools and project resources to external AI clients
//! via the Model Context Protocol (JSON-RPC 2.0 over stdio).
//!
//! The wire framing of each incoming message is auto-detected:
//! newline-delimited JSON-RPC (the MCP stdio spec, protocol 2024-11-05) or
//! LSP-style `Content-Length` headers (legacy clients). Every response is
//! written in the same framing as the request that produced it.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::BufReader;
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
//
// The framing primitives live in `crate::mcp::transport` — the canonical
// home shared by the MCP client transport, this server, and the LSP server.
// The framing of each incoming message is auto-detected per message, and
// each response is written in the same framing as the request that produced
// it.
// ---------------------------------------------------------------------------

#[cfg(test)]
use crate::mcp::transport::write_message;
use crate::mcp::transport::{
    detect_framing, read_content_length_message, read_newline_message, write_framed_message,
    Framing,
};

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

/// Whether destructive tools may execute over MCP. Off by default: the MCP
/// channel has no confirmation prompt, so tools classified destructive
/// (`Tool::is_destructive`) are refused unless the operator opts in by
/// starting the server with `SELFWARE_MCP_ALLOW_DESTRUCTIVE=1` (also
/// accepts "true"). Read per call so a long-running server picks up the
/// value its spawning client injected without a code change.
fn mcp_destructive_tools_allowed() -> bool {
    std::env::var("SELFWARE_MCP_ALLOW_DESTRUCTIVE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
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

    /// Create a server with an explicit safety config, bypassing on-disk config
    /// loading. Hermetic: unlike [`Self::new`], the safety checker does not read
    /// the developer's `~/.config/selfware/config.toml`, so tests behave the
    /// same on every machine regardless of local `denied_paths`.
    pub fn with_explicit_safety_config(safety: crate::config::SafetyConfig) -> Self {
        Self {
            registry: Arc::new(RwLock::new(ToolRegistry::new())),
            project_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            initialized: Arc::new(AtomicBool::new(false)),
            safety: SafetyChecker::new(&safety),
            config_path: None,
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

        // MCP has no interactive confirmation channel: a connected client
        // can be ANY external AI tool, and the agent loop's destructive-op
        // confirmation prompt (src/safety/confirm.rs) cannot run over
        // stdio JSON-RPC. Refuse tools the registry classifies as
        // destructive (the same Tool::is_destructive metadata the CLI
        // confirmation gate uses) unless the operator explicitly opted in
        // by starting the server with SELFWARE_MCP_ALLOW_DESTRUCTIVE=1.
        // The refusal is a normal tool result (isError: true), not a
        // protocol error, so the client can surface the reason to its user.
        if !mcp_destructive_tools_allowed() {
            let is_destructive = {
                let registry = self.registry.read().await;
                registry
                    .get(tool_name)
                    .is_some_and(|tool| tool.is_destructive())
            };
            if is_destructive {
                let response = serde_json::json!({
                    "content": [
                        {
                            "type": "text",
                            "text": format!(
                                "Refused: tool '{}' is classified as destructive and MCP provides no confirmation channel. \
                                 To allow destructive tools over MCP, restart the server with SELFWARE_MCP_ALLOW_DESTRUCTIVE=1.",
                                tool_name
                            )
                        }
                    ],
                    "isError": true
                });
                return (Some(response), None);
            }
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

                // Content-serving tools return raw file/diff text — the same
                // leak resources/read had before round 6: a client could
                // `file_read` selfware.toml and receive the raw api_key.
                // Redact secrets before the content leaves the process.
                // Structural tools (lsp_*, glob_find, directory_tree) return
                // locations/paths, not file content, and stay untouched.
                const CONTENT_SERVING_TOOLS: &[&str] = &["file_read", "grep_search", "git_diff"];
                let text = if CONTENT_SERVING_TOOLS.contains(&tool_name) {
                    crate::safety::redact::redact_secrets(&text).into_owned()
                } else {
                    text
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
                    Ok(cfg) => {
                        // SECURITY: never hand secrets to MCP clients.
                        // RedactedString's Serialize impl emits the REAL
                        // value (so TOML config round-trips keep working),
                        // which used to leak the raw
                        // SELFWARE_API_KEY/OPENROUTER_API_KEY — and the
                        // per-server MCP env maps (GITHUB_TOKEN etc.) — to
                        // any connected client. Serialize a redacted view.
                        let mut value = serde_json::to_value(&cfg).unwrap_or_default();
                        crate::config::model::redact_config_secrets(&mut value);
                        serde_json::to_string_pretty(&value).unwrap_or_default()
                    }
                    Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
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

        // Safety: ensure the resolved path is within the project root, and
        // READ VIA the canonical path (reading the unresolved path after a
        // canonical check is a classic TOCTOU symlink escape).
        let canonical = match full_path.canonicalize() {
            Ok(canonical) => canonical,
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
        };
        let root_canonical = match self.project_root.canonicalize() {
            Ok(root) => root,
            // No root canonicalization → no containment guarantee: deny, don't
            // silently skip the check.
            Err(e) => {
                return (
                    None,
                    Some(JsonRpcError {
                        code: INTERNAL_ERROR,
                        message: format!("Cannot canonicalize project root: {e}"),
                        data: None,
                    }),
                );
            }
        };
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

        match std::fs::read_to_string(&canonical) {
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

                // Config files can hold credentials (selfware.toml api_key,
                // ~/.aws/credentials-style blocks): redact before the content
                // leaves the process (verified leak, review round 6 #4).
                let content = crate::safety::redact::redact_secrets(&content).into_owned();

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
#[path = "../../tests/unit/mcp/server/server_test.rs"]
mod tests;
