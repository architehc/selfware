#![allow(dead_code, unused_imports, unused_variables)]
//! MCP tool discovery.
//!
//! Converts MCP tool schemas (JSON Schema format) into native Selfware tool schemas.

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, info};

use super::client::McpClient;
use super::tool_bridge::McpTool;

/// Discover all tools from an MCP client and create McpTool wrappers.
pub async fn discover_tools(client: &std::sync::Arc<McpClient>) -> Result<Vec<McpTool>> {
    let tool_schemas = client.list_tools().await?;
    let mut tools = Vec::new();

    for schema in tool_schemas {
        let name = schema
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let description = schema
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("No description");
        let input_schema = schema
            .get("inputSchema")
            .cloned()
            .unwrap_or(serde_json::json!({"type": "object", "properties": {}}));

        debug!(
            "Discovered MCP tool: {} — {}",
            name, description
        );

        // Prefix MCP tool names with "mcp_<server>_" to avoid collisions
        let prefixed_name = format!("mcp_{}_{}", client.server_name(), name);

        tools.push(McpTool::new(
            prefixed_name,
            description.to_string(),
            input_schema,
            std::sync::Arc::clone(client),
            name.to_string(),
        ));
    }

    info!(
        "Discovered {} MCP tool(s) from server '{}'",
        tools.len(),
        client.server_name()
    );

    Ok(tools)
}
