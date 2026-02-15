//! Process Management Tools
//!
//! Tools for managing background processes like dev servers, file watchers,
//! and database connections. Essential for web/mobile development workflows.

use super::Tool;
use crate::process_manager::{
    find_available_port, is_port_available, port_info, ProcessConfig, ProcessManager,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global process manager instance
static PROCESS_MANAGER: Lazy<Arc<RwLock<ProcessManager>>> =
    Lazy::new(|| Arc::new(RwLock::new(ProcessManager::new())));

pub struct ProcessStart;
pub struct ProcessStop;
pub struct ProcessList;
pub struct ProcessLogs;
pub struct ProcessRestart;
pub struct PortCheck;

#[async_trait]
impl Tool for ProcessStart {
    fn name(&self) -> &str {
        "process_start"
    }

    fn description(&self) -> &str {
        "Start a background process (e.g., dev server, file watcher). The process persists across agent steps. \
         Use health_check_pattern to wait for readiness (e.g., 'Ready on http' for Next.js, 'Compiled successfully' for webpack)."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Unique identifier for this process (e.g., 'dev-server', 'db-watcher')"
                },
                "command": {
                    "type": "string",
                    "description": "Command to execute (e.g., 'npm', 'cargo', 'python')"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Command arguments (e.g., ['run', 'dev'] for npm)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to current)"
                },
                "env": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                    "description": "Environment variables to set"
                },
                "health_check_pattern": {
                    "type": "string",
                    "description": "Regex pattern that indicates the process is ready (e.g., 'Ready|Compiled|Listening')"
                },
                "health_check_timeout_secs": {
                    "type": "integer",
                    "description": "Timeout for health check in seconds (default: 60)"
                },
                "expected_port": {
                    "type": "integer",
                    "description": "Port the process will listen on (used for conflict detection)"
                },
                "auto_restart": {
                    "type": "boolean",
                    "default": false,
                    "description": "Automatically restart if the process crashes"
                },
                "max_restart_attempts": {
                    "type": "integer",
                    "default": 3,
                    "description": "Maximum restart attempts (0 = unlimited)"
                }
            },
            "required": ["id", "command"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: id")?
            .to_string();

        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: command")?
            .to_string();

        let args_list: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let cwd = args.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from);

        let env: HashMap<String, String> = args
            .get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let health_check_pattern = args
            .get("health_check_pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let health_check_timeout_secs = args
            .get("health_check_timeout_secs")
            .and_then(|v| v.as_u64());

        let expected_port = args
            .get("expected_port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16);

        let auto_restart = args
            .get("auto_restart")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let max_restart_attempts = args
            .get("max_restart_attempts")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as u32;

        let config = ProcessConfig {
            id,
            command,
            args: args_list,
            cwd,
            env,
            health_check_pattern,
            health_check_timeout_secs,
            expected_port,
            auto_restart,
            max_restart_attempts,
        };

        let manager = PROCESS_MANAGER.read().await;
        let summary = manager.start(config).await?;

        Ok(serde_json::to_value(summary)?)
    }
}

#[async_trait]
impl Tool for ProcessStop {
    fn name(&self) -> &str {
        "process_stop"
    }

    fn description(&self) -> &str {
        "Stop a managed background process. Use force=true for immediate termination."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Process identifier"
                },
                "force": {
                    "type": "boolean",
                    "default": false,
                    "description": "Force kill (SIGKILL) instead of graceful shutdown (SIGTERM)"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: id")?;

        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        let manager = PROCESS_MANAGER.read().await;
        let summary = manager.stop(id, force).await?;

        Ok(serde_json::to_value(summary)?)
    }
}

#[async_trait]
impl Tool for ProcessList {
    fn name(&self) -> &str {
        "process_list"
    }

    fn description(&self) -> &str {
        "List all managed background processes with their status, uptime, and recent logs."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        let manager = PROCESS_MANAGER.read().await;
        let processes = manager.list().await;

        Ok(serde_json::json!({
            "processes": processes,
            "count": processes.len()
        }))
    }
}

#[async_trait]
impl Tool for ProcessLogs {
    fn name(&self) -> &str {
        "process_logs"
    }

    fn description(&self) -> &str {
        "Get recent log output from a managed process. Useful for debugging startup issues or runtime errors."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Process identifier"
                },
                "lines": {
                    "type": "integer",
                    "default": 50,
                    "description": "Number of recent log lines to retrieve (max 500)"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: id")?;

        let lines = args
            .get("lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(500) as usize;

        let manager = PROCESS_MANAGER.read().await;
        let logs = manager.logs(id, lines).await?;
        let summary = manager.get(id).await?;

        Ok(serde_json::json!({
            "id": id,
            "status": summary.status,
            "logs": logs,
            "log_count": logs.len()
        }))
    }
}

#[async_trait]
impl Tool for ProcessRestart {
    fn name(&self) -> &str {
        "process_restart"
    }

    fn description(&self) -> &str {
        "Restart a managed process. Useful after configuration changes or to recover from errors."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Process identifier"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let id = args
            .get("id")
            .and_then(|v| v.as_str())
            .context("Missing required parameter: id")?;

        let manager = PROCESS_MANAGER.read().await;
        let summary = manager.restart(id).await?;

        Ok(serde_json::to_value(summary)?)
    }
}

#[async_trait]
impl Tool for PortCheck {
    fn name(&self) -> &str {
        "port_check"
    }

    fn description(&self) -> &str {
        "Check port availability and find open ports. Use before starting servers to avoid conflicts."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "port": {
                    "type": "integer",
                    "description": "Specific port to check"
                },
                "find_available": {
                    "type": "boolean",
                    "default": false,
                    "description": "Find an available port in the range"
                },
                "range_start": {
                    "type": "integer",
                    "default": 3000,
                    "description": "Start of port range to search"
                },
                "range_end": {
                    "type": "integer",
                    "default": 9000,
                    "description": "End of port range to search"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let specific_port = args.get("port").and_then(|v| v.as_u64()).map(|p| p as u16);

        let find_available = args
            .get("find_available")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let range_start = args
            .get("range_start")
            .and_then(|v| v.as_u64())
            .unwrap_or(3000) as u16;

        let range_end = args
            .get("range_end")
            .and_then(|v| v.as_u64())
            .unwrap_or(9000) as u16;

        if let Some(port) = specific_port {
            let available = is_port_available(port).await;
            let info = if !available {
                port_info(port).await
            } else {
                None
            };

            return Ok(serde_json::json!({
                "port": port,
                "available": available,
                "process_info": info
            }));
        }

        if find_available {
            let port = find_available_port(range_start, range_end).await;
            return Ok(serde_json::json!({
                "available_port": port,
                "range_searched": format!("{}-{}", range_start, range_end)
            }));
        }

        // Default: check common dev ports
        let common_ports = [3000, 3001, 4000, 5000, 5173, 8000, 8080, 8888, 9000];
        let mut results = Vec::new();

        for port in common_ports {
            let available = is_port_available(port).await;
            results.push(serde_json::json!({
                "port": port,
                "available": available
            }));
        }

        Ok(serde_json::json!({
            "ports": results
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_start_name() {
        let tool = ProcessStart;
        assert_eq!(tool.name(), "process_start");
    }

    #[test]
    fn test_process_start_description() {
        let tool = ProcessStart;
        assert!(tool.description().contains("background process"));
    }

    #[test]
    fn test_process_start_schema() {
        let tool = ProcessStart;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["id"].is_object());
        assert!(schema["properties"]["command"].is_object());
        assert!(schema["properties"]["health_check_pattern"].is_object());
    }

    #[test]
    fn test_process_stop_name() {
        let tool = ProcessStop;
        assert_eq!(tool.name(), "process_stop");
    }

    #[test]
    fn test_process_stop_schema() {
        let tool = ProcessStop;
        let schema = tool.schema();
        assert!(schema["properties"]["force"].is_object());
    }

    #[test]
    fn test_process_list_name() {
        let tool = ProcessList;
        assert_eq!(tool.name(), "process_list");
    }

    #[test]
    fn test_process_logs_name() {
        let tool = ProcessLogs;
        assert_eq!(tool.name(), "process_logs");
    }

    #[test]
    fn test_process_logs_schema() {
        let tool = ProcessLogs;
        let schema = tool.schema();
        assert!(schema["properties"]["lines"].is_object());
    }

    #[test]
    fn test_process_restart_name() {
        let tool = ProcessRestart;
        assert_eq!(tool.name(), "process_restart");
    }

    #[test]
    fn test_port_check_name() {
        let tool = PortCheck;
        assert_eq!(tool.name(), "port_check");
    }

    #[test]
    fn test_port_check_schema() {
        let tool = PortCheck;
        let schema = tool.schema();
        assert!(schema["properties"]["port"].is_object());
        assert!(schema["properties"]["find_available"].is_object());
        assert!(schema["properties"]["range_start"].is_object());
    }

    #[tokio::test]
    async fn test_process_list_empty() {
        let tool = ProcessList;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("processes").is_some());
        assert!(output.get("count").is_some());
    }

    #[tokio::test]
    async fn test_port_check_common_ports() {
        let tool = PortCheck;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("ports").is_some());
    }

    #[tokio::test]
    async fn test_port_check_specific_port() {
        let tool = PortCheck;
        let result = tool.execute(serde_json::json!({"port": 12345})).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("available").is_some());
    }

    #[tokio::test]
    async fn test_port_check_find_available() {
        let tool = PortCheck;
        let result = tool
            .execute(serde_json::json!({
                "find_available": true,
                "range_start": 50000,
                "range_end": 50100
            }))
            .await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("available_port").is_some());
    }

    #[tokio::test]
    async fn test_process_start_echo() {
        let tool = ProcessStart;
        let result = tool
            .execute(serde_json::json!({
                "id": "test-echo-tool",
                "command": "echo",
                "args": ["hello"]
            }))
            .await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output["id"], "test-echo-tool");
    }

    #[tokio::test]
    async fn test_process_stop_nonexistent() {
        let tool = ProcessStop;
        let result = tool
            .execute(serde_json::json!({"id": "nonexistent-process"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_logs_nonexistent() {
        let tool = ProcessLogs;
        let result = tool
            .execute(serde_json::json!({"id": "nonexistent-process"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_restart_nonexistent() {
        let tool = ProcessRestart;
        let result = tool
            .execute(serde_json::json!({"id": "nonexistent-process"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_start_with_health_check() {
        let tool = ProcessStart;
        let result = tool
            .execute(serde_json::json!({
                "id": "test-health-tool",
                "command": "echo",
                "args": ["Ready on http://localhost:3000"],
                "health_check_pattern": "Ready",
                "health_check_timeout_secs": 5
            }))
            .await;
        assert!(result.is_ok());

        let output = result.unwrap();
        // Echo command should match health check immediately
        assert!(output["health_matched"].as_bool().unwrap_or(false));
    }

    #[tokio::test]
    async fn test_process_start_missing_id() {
        let tool = ProcessStart;
        let result = tool.execute(serde_json::json!({"command": "echo"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("id"));
    }

    #[tokio::test]
    async fn test_process_start_missing_command() {
        let tool = ProcessStart;
        let result = tool.execute(serde_json::json!({"id": "test"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("command"));
    }

    #[tokio::test]
    async fn test_process_start_with_env() {
        let tool = ProcessStart;
        let result = tool
            .execute(serde_json::json!({
                "id": "test-env-tool",
                "command": "sh",
                "args": ["-c", "echo $MY_VAR"],
                "env": {"MY_VAR": "test_value"}
            }))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_start_with_cwd() {
        let tool = ProcessStart;
        let result = tool
            .execute(serde_json::json!({
                "id": "test-cwd-tool",
                "command": "pwd",
                "cwd": "/tmp"
            }))
            .await;
        assert!(result.is_ok());
    }
}
