//! Model Context Protocol (MCP) client implementation.
//!
//! Enables Selfware to connect to MCP servers (GitHub, Playwright, databases, etc.)
//! and use their tools as native tools in the agent's tool registry.
//!
//! MCP uses JSON-RPC 2.0 over stdio transport. Each server is a child process
//! that communicates via stdin/stdout.

pub mod client;
pub mod discovery;
pub mod server;
pub mod tool_bridge;
pub mod transport;

pub use client::McpClient;
pub use discovery::discover_tools;
pub use tool_bridge::McpTool;
pub use transport::{StdioTransport, Transport};

use serde::{Deserialize, Serialize};

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Human-readable name for this server.
    pub name: String,
    /// Command to spawn the server process.
    pub command: String,
    /// Arguments to pass to the command.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables to set for the server process.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    /// Timeout in seconds for server initialization (default: 30).
    #[serde(default = "default_init_timeout")]
    pub init_timeout_secs: u64,
}

fn default_init_timeout() -> u64 {
    30
}

/// Top-level MCP configuration section.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// List of MCP servers to connect to.
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}
