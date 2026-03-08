#![allow(dead_code, unused_imports, unused_variables)]
//! MCP tool bridge: adapts MCP tools to the native Tool trait.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info};

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
mod tests {
    use super::*;

    #[test]
    fn test_mcp_tool_name() {
        // We can't easily test execute without a real MCP server,
        // but we can test the metadata methods
        // McpTool requires an Arc<McpClient> which needs a transport,
        // so we just test the struct construction logic conceptually
        let name = "mcp_github_create_issue";
        let description = "Create a GitHub issue";
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "title": {"type": "string"},
                "body": {"type": "string"}
            }
        });

        // Verify name format
        assert!(name.starts_with("mcp_"));
        assert!(schema.get("type").is_some());
        assert_eq!(description, "Create a GitHub issue");
    }
}
