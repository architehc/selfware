#![allow(dead_code, unused_imports, unused_variables)]
//! MCP transport layer.
//!
//! Provides the `Transport` trait and a `StdioTransport` implementation that
//! communicates with an MCP server via stdin/stdout using JSON-RPC 2.0.

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info, warn};

/// JSON-RPC 2.0 request.
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

/// Trait for MCP transport implementations.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a JSON-RPC request and wait for the response.
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value>;

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()>;

    /// Shut down the transport and clean up resources.
    async fn shutdown(&self) -> Result<()>;
}

/// Stdio-based transport: spawns a child process and communicates via stdin/stdout.
pub struct StdioTransport {
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    /// Pending responses keyed by request ID.
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    next_id: AtomicU64,
    child: Arc<Mutex<Child>>,
    /// Background reader task handle.
    reader_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl StdioTransport {
    /// Spawn a child process and set up the stdio transport.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        info!("Spawning MCP server: {} {:?}", command, args);

        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {} {:?}", command, args))?;

        let stdin = child
            .stdin
            .take()
            .context("Failed to capture MCP server stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("Failed to capture MCP server stdout")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);

        // Spawn background task to read JSON-RPC responses from stdout
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();

            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(response) => {
                        if let Some(id) = response.id {
                            let mut pending = pending_clone.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let _ = tx.send(response);
                            } else {
                                debug!(
                                    "Received response for unknown request ID {}: {:?}",
                                    id, response
                                );
                            }
                        } else {
                            // Notification from server (no ID)
                            debug!("MCP server notification: {:?}", response);
                        }
                    }
                    Err(e) => {
                        debug!("Non-JSON line from MCP server: {}", line);
                    }
                }
            }

            debug!("MCP stdout reader exited");
        });

        Ok(Self {
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            child: Arc::new(Mutex::new(child)),
            reader_handle: Mutex::new(Some(reader_handle)),
        })
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let mut request_bytes = serde_json::to_vec(&request)?;
        request_bytes.push(b'\n');

        // Register pending response channel before sending
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(&request_bytes).await?;
            stdin.flush().await?;
        }

        debug!("Sent JSON-RPC request: {} (id={})", method, id);

        // Wait for response with timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(60), rx)
            .await
            .map_err(|_| anyhow::anyhow!("MCP request '{}' timed out after 60s", method))?
            .map_err(|_| anyhow::anyhow!("MCP response channel closed for '{}'", method))?;

        if let Some(error) = response.error {
            bail!("MCP error for '{}': {}", method, error);
        }

        response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response for '{}' has no result", method))
    }

    async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        // Notifications use id: null (which we represent by not including id)
        // For simplicity, we use id 0 as a sentinel that won't have a response registered
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let mut bytes = serde_json::to_vec(&notification)?;
        bytes.push(b'\n');

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;

        debug!("Sent JSON-RPC notification: {}", method);
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down MCP transport");

        // Try graceful shutdown notification
        let _ = self.notify("notifications/shutdown", None).await;

        // Give server a moment to clean up, then kill
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let mut child = self.child.lock().await;
        let _ = child.kill().await;

        // Cancel reader task
        let mut handle = self.reader_handle.lock().await;
        if let Some(h) = handle.take() {
            h.abort();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "test_method".to_string(),
            params: Some(serde_json::json!({"key": "value"})),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"test_method\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_json_rpc_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some(1));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_error_deserialization() {
        let json =
            r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some(2));
        assert!(response.error.is_some());
        let err = response.error.unwrap();
        assert_eq!(err.code, -32601);
        assert_eq!(err.message, "Method not found");
    }
}
