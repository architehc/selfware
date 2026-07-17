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
pub use transport::{Framing, StdioTransport, Transport};

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
    /// Wire framing for this server's stdio protocol
    /// (default: newline-delimited per the MCP spec).
    #[serde(default)]
    pub framing: Framing,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_default() {
        let config = McpConfig::default();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_config_serialize_empty() {
        let config = McpConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("servers"));
    }

    #[test]
    fn test_mcp_config_deserialize_empty() {
        let json = r#"{"servers": []}"#;
        let config: McpConfig = serde_json::from_str(json).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_config_deserialize_no_servers_field() {
        let json = "{}";
        let config: McpConfig = serde_json::from_str(json).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn test_mcp_server_config_serialize_roundtrip() {
        let server = McpServerConfig {
            name: "github".to_string(),
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "@modelcontextprotocol/server-github".to_string(),
            ],
            env: {
                let mut m = std::collections::HashMap::new();
                m.insert("GITHUB_TOKEN".to_string(), "ghp_test".to_string());
                m
            },
            init_timeout_secs: 30,
            framing: Default::default(),
        };
        let json = serde_json::to_string(&server).unwrap();
        let parsed: McpServerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "github");
        assert_eq!(parsed.command, "npx");
        assert_eq!(parsed.args.len(), 2);
        assert_eq!(
            parsed.env.get("GITHUB_TOKEN"),
            Some(&"ghp_test".to_string())
        );
        assert_eq!(parsed.init_timeout_secs, 30);
    }

    #[test]
    fn test_mcp_server_config_defaults() {
        let json = r#"{"name": "test", "command": "echo"}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "test");
        assert_eq!(config.command, "echo");
        assert!(config.args.is_empty());
        assert!(config.env.is_empty());
        assert_eq!(config.init_timeout_secs, 30);
    }

    #[test]
    fn test_mcp_config_with_multiple_servers() {
        let json = r#"{
            "servers": [
                {"name": "github", "command": "npx", "args": ["-y", "gh-server"]},
                {"name": "db", "command": "mcp-server-sqlite", "args": ["--db", "test.db"]}
            ]
        }"#;
        let config: McpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.servers[0].name, "github");
        assert_eq!(config.servers[1].name, "db");
    }

    #[test]
    fn test_mcp_server_config_clone() {
        let server = McpServerConfig {
            name: "test".to_string(),
            command: "cmd".to_string(),
            args: vec!["--arg".to_string()],
            env: std::collections::HashMap::new(),
            init_timeout_secs: 60,
            framing: Default::default(),
        };
        let cloned = server.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.init_timeout_secs, 60);
    }

    #[test]
    fn test_mcp_config_clone() {
        let config = McpConfig {
            servers: vec![McpServerConfig {
                name: "s1".to_string(),
                command: "c1".to_string(),
                args: vec![],
                env: std::collections::HashMap::new(),
                init_timeout_secs: 30,
                framing: Default::default(),
            }],
        };
        let cloned = config.clone();
        assert_eq!(cloned.servers.len(), 1);
        assert_eq!(cloned.servers[0].name, "s1");
    }

    #[test]
    fn test_default_init_timeout() {
        assert_eq!(default_init_timeout(), 30);
    }

    #[test]
    fn test_mcp_server_config_custom_timeout() {
        let json = r#"{"name": "slow", "command": "heavy-server", "init_timeout_secs": 120}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.init_timeout_secs, 120);
    }

    #[test]
    fn test_mcp_server_config_with_env() {
        let json = r#"{"name": "test", "command": "cmd", "env": {"KEY1": "val1", "KEY2": "val2"}}"#;
        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.env.get("KEY1"), Some(&"val1".to_string()));
    }

    #[test]
    fn test_mcp_config_debug() {
        let config = McpConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("McpConfig"));
    }

    #[test]
    fn test_mcp_server_config_debug() {
        let server = McpServerConfig {
            name: "test".to_string(),
            command: "cmd".to_string(),
            args: vec![],
            env: std::collections::HashMap::new(),
            init_timeout_secs: 30,
            framing: Default::default(),
        };
        let debug_str = format!("{:?}", server);
        assert!(debug_str.contains("test"));
    }
}
