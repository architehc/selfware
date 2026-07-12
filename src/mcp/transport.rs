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
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Content-Length framed I/O helpers (LSP-style, mirrors server.rs)
// ---------------------------------------------------------------------------

/// Read a single Content-Length framed message from `reader`.
///
/// The format is:
/// ```text
/// Content-Length: <n>\r\n
/// \r\n
/// <n bytes of JSON>
/// ```
///
/// Returns `Ok(None)` on EOF, `Ok(Some(body))` on a successful read, and
/// `Err` when headers are malformed or the body is truncated.
pub(crate) async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut content_length: Option<usize> = None;

    // Read headers until we hit the empty line.
    loop {
        let mut header_line = String::new();
        let bytes_read = reader.read_line(&mut header_line).await?;
        if bytes_read == 0 {
            // EOF
            return Ok(None);
        }

        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            // End of headers.
            break;
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .context("Invalid Content-Length value")?,
            );
        }
        // Ignore other headers (e.g. Content-Type).
    }

    let length = content_length.context("Missing Content-Length header")?;

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await?;

    String::from_utf8(buf)
        .context("Message body is not valid UTF-8")
        .map(Some)
}

/// Write a Content-Length framed message to `writer`.
pub(crate) async fn write_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
) -> Result<()> {
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(body.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

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

        // Clear the inherited environment before applying the server's declared
        // env. An MCP server is third-party code; without this it would receive
        // SELFWARE_API_KEY and every operator-exported credential.
        crate::safety::process_env::sanitize_command_env(&mut cmd);

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
        // Take stderr so we can drain it — if we leave it piped but never
        // read it, a chatty server can fill the OS pipe buffer and block
        // forever.
        let stderr = child
            .stderr
            .take()
            .context("Failed to capture MCP server stderr")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = Arc::clone(&pending);

        // Spawn background task to read JSON-RPC responses from stdout
        // Uses Content-Length framing (LSP-style) per the MCP spec.
        let reader_handle = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            loop {
                match read_message(&mut reader).await {
                    Ok(Some(body)) => {
                        match serde_json::from_str::<JsonRpcResponse>(&body) {
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
                                debug!("Failed to parse JSON-RPC message from MCP server: {} (body: {})", e, body);
                            }
                        }
                    }
                    Ok(None) => {
                        // EOF
                        break;
                    }
                    Err(e) => {
                        debug!("MCP stdout framing error: {}", e);
                        break;
                    }
                }
            }

            debug!("MCP stdout reader exited");
        });

        // Spawn a background task to drain child stderr so it can't fill
        // the OS pipe buffer and deadlock the server.  Lines are forwarded
        // to tracing so they are visible for debugging but don't interfere
        // with the JSON-RPC protocol on stdout.
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            debug!("MCP server stderr: {}", trimmed);
                        }
                    }
                    Err(e) => {
                        debug!("MCP stderr read error: {}", e);
                        break;
                    }
                }
            }
            debug!("MCP stderr drain exited");
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

        let body = serde_json::to_string(&request)?;

        // Register pending response channel before sending
        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        // Send request with Content-Length framing
        {
            let mut stdin = self.stdin.lock().await;
            write_message(&mut *stdin, &body).await?;
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
        // Notifications omit the id field entirely.
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let body = serde_json::to_string(&notification)?;

        let mut stdin = self.stdin.lock().await;
        write_message(&mut *stdin, &body).await?;

        debug!("Sent JSON-RPC notification: {}", method);
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("Shutting down MCP transport");

        // Per the MCP spec: send a `shutdown` *request* (the server should
        // respond with an empty result), then an `exit` *notification*.
        // The previous code sent a non-standard `notifications/shutdown`.
        let _ = self.request("shutdown", None).await;
        let _ = self.notify("notifications/exit", None).await;

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

    // -----------------------------------------------------------------------
    // Content-Length framing tests (Bug A regression coverage)
    // -----------------------------------------------------------------------

    /// Round-trip: client write_message → server-style read_message recovers
    /// the exact body. Both sides share the same helpers (this module's
    /// `read_message`/`write_message` were copied from `server.rs` to fix
    /// the framing mismatch with spec-compliant external MCP servers).
    #[tokio::test]
    async fn test_framing_roundtrip_client_to_server() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_message(&mut buffer, body).await.unwrap();

        // Verify on-the-wire shape matches the LSP/MCP spec.
        let on_wire = String::from_utf8(buffer.clone()).unwrap();
        let expected_header = format!("Content-Length: {}\r\n\r\n", body.len());
        assert!(
            on_wire.starts_with(&expected_header),
            "framed bytes must start with 'Content-Length: <n>\\r\\n\\r\\n', got: {:?}",
            on_wire
        );
        assert!(on_wire.ends_with(body));

        // Read back through the same framing helper.
        let mut reader = BufReader::new(buffer.as_slice());
        let recovered = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(recovered, body);
    }

    /// Partial buffer: header advertises 100 bytes but only 50 are present
    /// initially. read_message must not return until the full body arrives.
    /// We simulate this with `tokio::io::duplex` — the writer holds the
    /// remaining bytes, and read_message awaits them.
    #[tokio::test]
    async fn test_framing_partial_buffer_waits_for_full_body() {
        // Build a 100-byte JSON-ish body.
        let body: String = format!("{}{}", "{\"a\":\"", "x".repeat(92)) + "\"}";
        assert_eq!(body.len(), 100);

        // 1MB-ish duplex; we'll write headers + first 50 bytes, then later the rest.
        let (client, mut server) = tokio::io::duplex(4096);
        let body_clone = body.clone();
        let writer_handle = tokio::spawn(async move {
            // Write header.
            server
                .write_all(b"Content-Length: 100\r\n\r\n")
                .await
                .unwrap();
            server.flush().await.unwrap();
            // Write first half.
            server
                .write_all(&body_clone.as_bytes()[..50])
                .await
                .unwrap();
            server.flush().await.unwrap();
            // Pause, then write the remainder.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            server
                .write_all(&body_clone.as_bytes()[50..])
                .await
                .unwrap();
            server.flush().await.unwrap();
            // Keep the writer alive briefly so EOF doesn't race.
            drop(server);
        });

        let mut reader = BufReader::new(client);
        let recovered = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(recovered, body);

        writer_handle.await.unwrap();
    }

    /// Multiple back-to-back messages must be parsed individually with no
    /// delimiter confusion (newline-delimited framing would have failed
    /// here for any body containing an unescaped newline).
    #[tokio::test]
    async fn test_framing_multiple_back_to_back_messages() {
        let bodies = [
            r#"{"jsonrpc":"2.0","id":1,"result":{"x":1}}"#.to_string(),
            // This body contains a literal newline-ish escape, which a
            // newline-delimited framer would mishandle. Content-Length is
            // byte-accurate and unaffected.
            r#"{"jsonrpc":"2.0","id":2,"result":{"text":"line1\nline2"}}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":3,"result":null}"#.to_string(),
        ];

        let mut buffer: Vec<u8> = Vec::new();
        for body in &bodies {
            write_message(&mut buffer, body).await.unwrap();
        }

        let mut reader = BufReader::new(buffer.as_slice());
        for expected in &bodies {
            let got = read_message(&mut reader).await.unwrap().unwrap();
            assert_eq!(&got, expected);
        }
        // After all messages, the next read should hit EOF cleanly.
        let eof = read_message(&mut reader).await.unwrap();
        assert!(eof.is_none(), "expected clean EOF after last message");
    }

    /// EOF before any data returns Ok(None).
    #[tokio::test]
    async fn test_framing_eof_returns_none() {
        let empty: &[u8] = b"";
        let mut reader = BufReader::new(empty);
        let result = read_message(&mut reader).await.unwrap();
        assert!(result.is_none());
    }

    /// Missing Content-Length header is a hard error (not silently treated
    /// as a 0-byte body).
    #[tokio::test]
    async fn test_framing_missing_content_length_errors() {
        let bytes: &[u8] = b"X-Other: foo\r\n\r\n";
        let mut reader = BufReader::new(bytes);
        let result = read_message(&mut reader).await;
        assert!(result.is_err(), "missing Content-Length must error");
    }
}
