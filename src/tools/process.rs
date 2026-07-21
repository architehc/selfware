//! Process Management Tools
//!
//! Tools for managing background processes like dev servers, file watchers,
//! and database connections. Essential for web/mobile development workflows.

use super::Tool;
use crate::process_manager::{
    find_available_port, is_port_available, port_info, ProcessConfig, ProcessInventory,
    ProcessManager, ProcessReconcileReport,
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
         If the same id is already running with the same configuration, the existing process is reused. \
         Use health_check_pattern to wait for readiness (e.g., 'Ready on http' for Next.js, 'Compiled successfully' for webpack). \
         When expected_port is provided, selfware automatically reserves that port until the child is spawned."
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

        const FORBIDDEN_CHARS: &[char] = &[';', '&', '|', '`', '$', '(', ')', '<', '>'];
        if command.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
            anyhow::bail!("Blocked forbidden metacharacter in process command.");
        }

        let args_list: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Defense-in-depth: validate each argument for shell metacharacters.
        // Although args are passed via Command::args() (not through a shell),
        // which is inherently safe against shell injection, we still reject
        // suspicious characters to prevent abuse when the command itself is a
        // shell (e.g. "sh" with args ["-c", "rm -rf /"]).
        for (i, arg) in args_list.iter().enumerate() {
            if arg.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
                anyhow::bail!(
                    "Blocked forbidden shell metacharacter in argument {}: {:?}",
                    i,
                    arg
                );
            }
        }

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

        // Double-check: the process manager should now return Err for failure
        // states, but guard against future regressions by checking status here.
        use crate::process_manager::ProcessStatus;
        match &summary.status {
            ProcessStatus::Running => Ok(serde_json::to_value(summary)?),
            ProcessStatus::HealthCheckFailed => {
                anyhow::bail!(
                    "Process '{}' started but health check failed. Check logs with process_logs.",
                    summary.id
                );
            }
            ProcessStatus::Crashed { exit_code } => {
                anyhow::bail!(
                    "Process '{}' exited immediately with code {}. Check logs with process_logs.",
                    summary.id,
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                );
            }
            other => {
                // Starting, Restarting, Stopped -- none should happen here
                // but surface it clearly rather than silently succeeding
                anyhow::bail!(
                    "Process '{}' is in unexpected state {:?} after start. Check logs with process_logs.",
                    summary.id,
                    other,
                );
            }
        }
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
        "Check port availability and find open ports. Use reserve=true to hold a port for a later process_start call and reduce races."
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
                "reserve": {
                    "type": "boolean",
                    "default": false,
                    "description": "Reserve the checked/found port until process_start consumes it or cleanup runs"
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
        let reserve = args
            .get("reserve")
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

        let manager = PROCESS_MANAGER.read().await;

        if let Some(port) = specific_port {
            if reserve {
                manager.reserve_port(port).await?;
                return Ok(serde_json::json!({
                    "port": port,
                    "available": true,
                    "reserved": true,
                    "reservation_ttl_secs": 30
                }));
            }

            let reserved = manager.has_reserved_port(port).await;
            let available = is_port_available(port).await;
            let info = if !available {
                port_info(port).await
            } else {
                None
            };

            return Ok(serde_json::json!({
                "port": port,
                "available": available,
                "reserved": reserved,
                "process_info": info
            }));
        }

        if find_available {
            let port = if reserve {
                Some(
                    manager
                        .reserve_available_port(range_start, range_end)
                        .await?,
                )
            } else {
                find_available_port(range_start, range_end).await
            };
            return Ok(serde_json::json!({
                "available_port": port,
                "range_searched": format!("{}-{}", range_start, range_end),
                "reserved": reserve
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

/// Stop all managed background processes and print a summary.
///
/// Call this when the interactive loop exits to ensure cleanup.
pub async fn cleanup_all_processes() {
    let manager = PROCESS_MANAGER.read().await;
    let reconcile_before = manager.reconcile(false).await;
    let stopped = manager.stop_all().await;
    let reconcile_after = manager.reconcile(true).await;
    let released_reservations = manager.clear_port_reservations().await;
    let orphaned = reconcile_before.orphaned_entries + reconcile_after.orphaned_entries;
    let removed_inactive = reconcile_after.removed_inactive;
    if stopped > 0 {
        println!(
            "Stopped {} background process{}",
            stopped,
            if stopped == 1 { "" } else { "es" }
        );
    }
    if released_reservations > 0 {
        println!(
            "Released {} port reservation{}",
            released_reservations,
            if released_reservations == 1 { "" } else { "s" }
        );
    }
    if orphaned > 0 {
        println!(
            "Reconciled {} orphaned process entr{}",
            orphaned,
            if orphaned == 1 { "y" } else { "ies" }
        );
    }
    if removed_inactive > 0 {
        println!(
            "Pruned {} inactive process entr{}",
            removed_inactive,
            if removed_inactive == 1 { "y" } else { "ies" }
        );
    }
}

pub async fn reconcile_managed_processes(prune_inactive: bool) -> ProcessReconcileReport {
    let manager = PROCESS_MANAGER.read().await;
    manager.reconcile(prune_inactive).await
}

pub async fn process_inventory(log_lines: usize) -> ProcessInventory {
    let manager = PROCESS_MANAGER.read().await;
    manager.inventory(log_lines).await
}

pub fn try_process_inventory(log_lines: usize) -> Option<ProcessInventory> {
    let manager = PROCESS_MANAGER.try_read().ok()?;
    manager.try_inventory(log_lines)
}

#[cfg(test)]
#[path = "../../tests/unit/tools/process/process_test.rs"]
mod tests;
