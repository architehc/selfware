//! MCP client that manages the protocol lifecycle.
//!
//! Handles initialization, tool discovery, and tool execution via the transport.

use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info};

use super::transport::Transport;
use super::McpServerConfig;

/// MCP protocol version we support.
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// Information about the client sent during initialization.
const CLIENT_NAME: &str = "selfware";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// MCP client wrapping a transport connection to a single MCP server.
pub struct McpClient {
    transport: Arc<dyn Transport>,
    server_name: String,
    server_info: Option<Value>,
}

impl McpClient {
    /// Connect to an MCP server and perform the initialization handshake.
    pub async fn connect(config: &McpServerConfig) -> Result<Self> {
        let transport = super::StdioTransport::spawn(&config.command, &config.args, &config.env)
            .await
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
            .with_context(|| {
                format!(
                    "MCP tool call '{}' failed on server '{}'",
                    name, self.server_name
                )
            })?;

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
                    "isError": result.get("isError").and_then(|e| e.as_bool()).unwrap_or(false),
                }));
            }
        }

        Ok(result)
    }

    /// Get the server name.
    pub fn server_name(&self) -> &str {
        &self.server_name
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
