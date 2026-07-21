//! MCP tool bridge: adapts MCP tools to the native Tool trait.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::debug;

use super::client::McpClient;
use crate::tools::Tool;

/// An MCP tool that implements the native Tool trait.
///
/// Forwards `execute()` calls to the MCP server via `McpClient::call_tool()`.
pub struct McpTool {
    /// Prefixed tool name (e.g., "mcp_github_create_issue").
    name: String,
    /// Tool description from the MCP server.
    description: String,
    /// JSON Schema for tool arguments.
    schema: Value,
    /// Shared MCP client connection.
    client: Arc<McpClient>,
    /// Original (unprefixed) tool name on the MCP server.
    remote_name: String,
}

impl McpTool {
    pub fn new(
        name: String,
        description: String,
        schema: Value,
        client: Arc<McpClient>,
        remote_name: String,
    ) -> Self {
        Self {
            name,
            description,
            schema,
            client,
            remote_name,
        }
    }
}

#[async_trait]
impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        debug!(
            "Executing MCP tool '{}' (remote: '{}') with args: {}",
            self.name, self.remote_name, args
        );

        self.client.call_tool(&self.remote_name, args).await
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp/tool_bridge/tool_bridge_test.rs"]
mod tests;
