//! MCP client that manages the protocol lifecycle.
//!
//! Handles initialization, tool discovery, and tool execution via the transport.

use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info};

use super::transport::{McpTransportError, Transport};
use super::McpServerConfig;

/// MCP protocol version we support.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Information about the client sent during initialization.
const CLIENT_NAME: &str = "selfware";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A failed `tools/call`, carrying the underlying cause in its own message.
///
/// Tool dispatch renders errors with `to_string()`, which for an
/// `anyhow::Context` chain shows only the outermost message ("MCP tool call
/// 'x' failed on server 'y'") and hides *why*. This type puts the cause in
/// its `Display` so the model and user can tell an infrastructure failure
/// (server exited, broken pipe, timeout) from a tool/policy error, and keeps
/// the typed transport cause (if any) for programmatic inspection.
#[derive(Debug, Clone, thiserror::Error)]
#[error("MCP tool call '{tool}' failed on server '{server}': {cause}")]
pub struct McpToolCallError {
    pub tool: String,
    pub server: String,
    /// Human-readable cause (full error chain).
    pub cause: String,
    /// The typed transport failure, when the call failed at the transport
    /// level rather than with a server-returned JSON-RPC error.
    pub transport: Option<McpTransportError>,
}

/// One concise user-facing line for an MCP server that could not be brought
/// up at startup, e.g. `MCP server 'x' failed to start: <cause>; its tools are
/// unavailable`. The cause is the innermost typed transport failure when there
/// is one (it already names the server and the reason), otherwise the last two
/// links of the chain (e.g. "Failed to spawn MCP server: cmd: No such file or
/// directory"), collapsed to a single line.
pub fn startup_failure_line(server: &str, what: &str, err: &anyhow::Error) -> String {
    let cause = if let Some(t) = err
        .chain()
        .find_map(|c| c.downcast_ref::<McpTransportError>())
    {
        t.to_string()
    } else {
        let links: Vec<String> = err.chain().map(|c| c.to_string()).collect();
        let n = links.len();
        links[n.saturating_sub(2)..].join(": ")
    };
    let cause: String = cause.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("MCP server '{server}' {what}: {cause}; its tools are unavailable")
}

/// MCP client wrapping a transport connection to a single MCP server.
pub struct McpClient {
    transport: Arc<dyn Transport>,
    server_name: String,
    server_info: Option<Value>,
}

impl McpClient {
    /// Connect to an MCP server and perform the initialization handshake.
    pub async fn connect(config: &McpServerConfig) -> Result<Self> {
        let transport = super::StdioTransport::spawn_named(
            &config.name,
            &config.command,
            &config.args,
            &config.env,
        )
        .await
        .map(|t| t.with_framing(config.framing))
        .with_context(|| format!("Failed to spawn MCP server '{}'", config.name))?;

        let transport: Arc<dyn Transport> = Arc::new(transport);
        let mut client = Self {
            transport,
            server_name: config.name.clone(),
            server_info: None,
        };

        // Perform MCP initialization with timeout
        tokio::time::timeout(
            std::time::Duration::from_secs(config.init_timeout_secs.max(5)),
            client.initialize(),
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "MCP server '{}' initialization timed out after {}s",
                config.name,
                config.init_timeout_secs
            )
        })??;

        info!("MCP server '{}' initialized successfully", config.name);
        Ok(client)
    }

    /// Perform the MCP initialization handshake.
    async fn initialize(&mut self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "roots": { "listChanged": false },
            },
            "clientInfo": {
                "name": CLIENT_NAME,
                "version": CLIENT_VERSION,
            }
        });

        let result = self
            .transport
            .request("initialize", Some(params))
            .await
            .with_context(|| {
                format!("MCP initialize handshake failed for '{}'", self.server_name)
            })?;

        self.server_info = Some(result.clone());

        // Send initialized notification
        self.transport
            .notify("notifications/initialized", None)
            .await?;

        let server_name = result
            .get("serverInfo")
            .and_then(|i| i.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let protocol_version = result
            .get("protocolVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        info!(
            "MCP server '{}' (protocol: {})",
            server_name, protocol_version
        );

        Ok(())
    }

    /// List all tools available from this MCP server.
    pub async fn list_tools(&self) -> Result<Vec<Value>> {
        let result = self.transport.request("tools/list", None).await?;

        let tools = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        debug!(
            "MCP server '{}' offers {} tool(s)",
            self.server_name,
            tools.len()
        );

        Ok(tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let result = self
            .transport
            .request("tools/call", Some(params))
            .await
            .map_err(|e| {
                anyhow::Error::new(McpToolCallError {
                    tool: name.to_string(),
                    server: self.server_name.clone(),
                    cause: format!("{e:#}"),
                    transport: e.downcast_ref::<McpTransportError>().cloned(),
                })
            })?;

        // An MCP result with `isError: true` is a tool-level FAILURE, not a
        // success. Flag it with `success: false` so downstream success
        // detection (and therefore the result caches gated on it) never
        // treats the payload as successful — an error result must never be
        // cached and replayed as a success.
        let is_error = result
            .get("isError")
            .and_then(|e| e.as_bool())
            .unwrap_or(false);

        // MCP tool results have a `content` array with text/image/resource blocks
        // Extract text content for simple use
        if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
            let text_parts: Vec<&str> = content
                .iter()
                .filter_map(|block| {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        block.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();

            if !text_parts.is_empty() {
                return Ok(serde_json::json!({
                    "content": text_parts.join("\n"),
                    "isError": is_error,
                    "success": !is_error,
                }));
            }
        }

        if is_error {
            // Non-text (or empty) content: still flag the failure honestly.
            let mut flagged = result;
            if let Some(obj) = flagged.as_object_mut() {
                obj.insert("success".to_string(), Value::Bool(false));
            }
            return Ok(flagged);
        }

        Ok(result)
    }

    /// Get the server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// Construct a client over an arbitrary transport (test-only).
    #[cfg(test)]
    pub(crate) fn new_for_test(transport: Arc<dyn Transport>, server_name: &str) -> Self {
        Self {
            transport,
            server_name: server_name.to_string(),
            server_info: None,
        }
    }

    /// Shut down the client and its transport.
    pub async fn shutdown(&self) -> Result<()> {
        info!("Shutting down MCP client for '{}'", self.server_name);
        self.transport.shutdown().await
    }
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("server_name", &self.server_name)
            .finish()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp/client/client_test.rs"]
mod tests;
