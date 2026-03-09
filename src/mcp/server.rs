#![allow(dead_code, unused_imports, unused_variables)]
//! MCP server implementation.
//!
//! Exposes Selfware's tools and project resources to external AI clients
//! via the Model Context Protocol (JSON-RPC 2.0 over stdio).
//!
//! The server reads Content-Length framed JSON-RPC requests from stdin and
//! writes Content-Length framed JSON-RPC responses to stdout. This is the
//! same framing used by LSP (Language Server Protocol).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, error, info, warn};

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
// Content-Length framed I/O helpers
// ---------------------------------------------------------------------------

/// Read a single Content-Length framed message from `reader`.
///
/// The format is:
/// ```text
/// Content-Length: <n>\r\n
/// \r\n
/// <n bytes of JSON>
/// ```
async fn read_message<R: tokio::io::AsyncRead + Unpin>(
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

    String::from_utf8(buf).context("Message body is not valid UTF-8").map(Some)
}

/// Write a Content-Length framed message to `writer`.
async fn write_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// MCP Server
// ---------------------------------------------------------------------------

/// MCP server that exposes Selfware tools and project resources.
pub struct McpServer {
    /// The tool registry containing all available tools.
    registry: ToolRegistry,
    /// Root directory for project resource exposure.
    project_root: PathBuf,
    /// Whether the server has been initialized.
    initialized: bool,
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl McpServer {
    /// Create a new MCP server with the default tool registry.
    pub fn new() -> Self {
        let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            registry: ToolRegistry::new(),
            project_root,
            initialized: false,
        }
    }

    /// Create a new MCP server with a custom project root.
    pub fn with_project_root(project_root: PathBuf) -> Self {
        Self {
            registry: ToolRegistry::new(),
            project_root,
            initialized: false,
        }
    }

    /// Handle a parsed JSON-RPC request and return a response (or `None` for notifications).
    pub async fn handle_request(&mut self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        let id = match &request.id {
            Some(id) => id.clone(),
            None => {
                // This is a notification — no response needed.
                self.handle_notification(request).await;
                return None;
            }
        };

        let (result, error) = match request.method.as_str() {
            "initialize" => self.handle_initialize(&request.params),
            "tools/list" => self.handle_tools_list(&request.params),
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
    async fn handle_notification(&mut self, request: &JsonRpcRequest) {
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
            _ => {
                debug!("Unhandled notification: {}", request.method);
            }
        }
    }

    // -- Method handlers -----------------------------------------------------

    /// Handle `initialize` request.
    fn handle_initialize(
        &mut self,
        params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        self.initialized = true;

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

        info!(
            "MCP server initialized (protocol {})",
            MCP_PROTOCOL_VERSION
        );
        (Some(result), None)
    }

    /// Handle `tools/list` request.
    fn handle_tools_list(
        &self,
        params: &Option<Value>,
    ) -> (Option<Value>, Option<JsonRpcError>) {
        let tools: Vec<Value> = self
            .registry
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

        match self.registry.execute(tool_name, arguments).await {
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
        params: &Option<Value>,
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
                let config = match crate::config::Config::load(None) {
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
            _ if uri.starts_with("selfware://project/file/") => {
                let file_path = uri.strip_prefix("selfware://project/file/").unwrap();
                self.read_project_file(uri, file_path)
            }
            _ => (
                None,
                Some(JsonRpcError {
                    code: INVALID_PARAMS,
                    message: format!("Unknown resource URI: {}", uri),
                    data: None,
                }),
            ),
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
                    result.push_str(&self.build_directory_tree(&entry.path(), depth + 1, max_depth));
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
pub async fn run_mcp_server() -> Result<()> {
    // Redirect tracing output to stderr so it doesn't interfere with the JSON-RPC
    // protocol on stdout.
    eprintln!("selfware MCP server v{} starting...", SERVER_VERSION);
    info!("MCP server starting on stdio transport");

    let mut server = McpServer::new();

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);

    loop {
        let message = match read_message(&mut reader).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                info!("MCP server: stdin closed, shutting down");
                break;
            }
            Err(e) => {
                // Send a parse error response and continue.
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
                let body = serde_json::to_string(&error_response)?;
                write_message(&mut stdout, &body).await?;
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
                let body = serde_json::to_string(&error_response)?;
                write_message(&mut stdout, &body).await?;
                continue;
            }
        };

        debug!(
            "MCP server received: {} (id={:?})",
            request.method, request.id
        );

        // Check if this is a shutdown request.
        let is_shutdown = request.method == "shutdown";

        // Handle the request.
        if let Some(response) = server.handle_request(&request).await {
            let body = serde_json::to_string(&response)?;
            write_message(&mut stdout, &body).await?;
        }

        if is_shutdown {
            info!("MCP server shutting down after shutdown request");
            break;
        }
    }

    eprintln!("selfware MCP server stopped.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(format!("{}", err), "JSON-RPC error -32601: Method not found");
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let mut server = McpServer::new();

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
        assert!(server.initialized);
    }

    #[tokio::test]
    async fn test_handle_tools_list() {
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
    async fn test_handle_resources_list() {
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

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
        let mut server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: None, // Notification
            method: "notifications/initialized".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await;
        assert!(response.is_none(), "Notifications should not produce a response");
    }

    #[tokio::test]
    async fn test_content_length_framing_roundtrip() {
        let message_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

        // Write
        let mut buffer: Vec<u8> = Vec::new();
        write_message(&mut buffer, message_body).await.unwrap();

        let expected = format!("Content-Length: {}\r\n\r\n{}", message_body.len(), message_body);
        assert_eq!(String::from_utf8(buffer.clone()).unwrap(), expected);

        // Read back
        let mut reader = BufReader::new(buffer.as_slice());
        let read_back = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(read_back, message_body);
    }

    #[tokio::test]
    async fn test_read_message_eof() {
        let empty: &[u8] = b"";
        let mut reader = BufReader::new(empty);
        let result = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_handle_resources_read_file_path_escape() {
        let mut server = McpServer::new();

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
                        assert!(
                            !text.contains("root:"),
                            "Path traversal should be blocked"
                        );
                    }
                }
            }
        }
        // Having an error response is also acceptable
    }

    #[tokio::test]
    async fn test_tool_list_has_correct_schema_format() {
        let mut server = McpServer::new();

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
            assert!(!description.is_empty(), "Tool '{}' description should not be empty", name);
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
        let mut server = McpServer::new();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(Value::from(15)),
            method: "shutdown".to_string(),
            params: None,
        };

        let response = server.handle_request(&request).await.unwrap();
        assert!(response.error.is_none());
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
}
