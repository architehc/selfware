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
// Wire framing (newline-delimited JSON-RPC and LSP-style Content-Length,
// mirrors server.rs)
// ---------------------------------------------------------------------------

/// Wire framing for JSON-RPC messages.
///
/// The MCP stdio spec (protocol 2024-11-05) frames messages as
/// newline-delimited JSON-RPC, but some legacy/LSP-derived servers speak
/// `Content-Length` header framing. The client *sends* in the configured
/// framing ([`NewlineDelimited`](Framing::NewlineDelimited) by default) and
/// *reads* both via auto-detection, since a server may reply in either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    /// LSP-style `Content-Length: <n>\r\n\r\n<body>` (legacy servers).
    ContentLength,
    /// One JSON-RPC message per line (MCP stdio spec, 2024-11-05). Default.
    #[default]
    NewlineDelimited,
}

/// Peek at the buffered bytes to detect which framing the peer is using.
///
/// A header-framed message always starts with `Content-Length:`, while a
/// newline-delimited JSON-RPC message is a single JSON object on one line —
/// and a JSON document can never start with `C`. The first byte therefore
/// decides unambiguously, even if the peer's write arrived in several chunks.
/// Returns `Ok(None)` on EOF.
pub(crate) async fn detect_framing<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<Framing>> {
    let buf = reader.fill_buf().await?;
    if buf.is_empty() {
        // EOF
        return Ok(None);
    }
    Ok(Some(if buf[0] == b'C' {
        Framing::ContentLength
    } else {
        Framing::NewlineDelimited
    }))
}

/// Read the next message from `reader`, auto-detecting its framing.
///
/// Servers may reply in either framing (spec-compliant ones newline-delimited,
/// legacy ones with `Content-Length` headers), so the read path accepts both.
/// Returns `Ok(None)` on clean EOF and `Ok(Some(body))` on a successful read.
pub(crate) async fn read_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    match detect_framing(reader).await? {
        Some(Framing::ContentLength) => read_content_length_message(reader).await,
        Some(Framing::NewlineDelimited) => read_newline_message(reader).await,
        None => Ok(None),
    }
}

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
pub(crate) async fn read_content_length_message<R: tokio::io::AsyncRead + Unpin>(
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

/// Read a single newline-delimited JSON-RPC message (one line).
///
/// Per the MCP stdio spec (2024-11-05), messages are UTF-8 JSON-RPC with no
/// embedded newlines, delimited by `\n`. Returns `Ok(None)` on EOF.
pub(crate) async fn read_newline_message<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line).await?;
    if bytes_read == 0 {
        // EOF
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
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

/// Write a message to `writer` using the requested framing.
///
/// The compact `serde_json` serialization escapes control characters inside
/// strings, so a newline-delimited body never contains a raw `\n` of its own.
pub(crate) async fn write_framed_message<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
    framing: Framing,
) -> Result<()> {
    match framing {
        Framing::ContentLength => write_message(writer, body).await,
        Framing::NewlineDelimited => {
            writer.write_all(body.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
            Ok(())
        }
    }
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
    /// Framing used for *outgoing* requests/notifications. Incoming messages
    /// are always auto-detected, so a server may reply in either framing.
    framing: Framing,
}

impl StdioTransport {
    /// Spawn a child process and set up the stdio transport.
    ///
    /// Requests are sent newline-delimited (the MCP stdio spec framing,
    /// protocol 2024-11-05). Use [`with_framing`](Self::with_framing) to
    /// override this for legacy `Content-Length`-framed servers.
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

        // Spawn background task to read JSON-RPC responses from stdout.
        // The framing of each incoming message is auto-detected (see
        // `detect_framing`): spec-compliant servers reply newline-delimited
        // (MCP stdio spec, 2024-11-05), legacy ones with `Content-Length`
        // headers — both are accepted regardless of the send framing.
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
            framing: Framing::default(),
        })
    }

    /// Override the framing used for outgoing requests/notifications.
    ///
    /// The default is [`Framing::NewlineDelimited`] (the MCP stdio spec);
    /// select [`Framing::ContentLength`] only for legacy servers that expect
    /// LSP-style headers. The read path auto-detects either way.
    pub fn with_framing(mut self, framing: Framing) -> Self {
        self.framing = framing;
        self
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

        // Send the request in the configured framing (newline-delimited per
        // the MCP stdio spec unless overridden via `with_framing`).
        {
            let mut stdin = self.stdin.lock().await;
            // On a write failure, drop the now-orphaned pending entry so the map
            // doesn't leak one slot per failed request.
            if let Err(e) = write_framed_message(&mut *stdin, &body, self.framing).await {
                self.pending.lock().await.remove(&id);
                return Err(e);
            }
        }

        debug!("Sent JSON-RPC request: {} (id={})", method, id);

        // Wait for response with timeout
        let response = match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                // Sender dropped without replying: drop the pending entry.
                self.pending.lock().await.remove(&id);
                bail!("MCP response channel closed for '{}'", method);
            }
            Err(_) => {
                // Timed out: drop the pending entry so it doesn't leak a slot.
                self.pending.lock().await.remove(&id);
                bail!("MCP request '{}' timed out after 60s", method);
            }
        };

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
        write_framed_message(&mut *stdin, &body, self.framing).await?;

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

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Owned teardown: if the transport is dropped without an explicit
        // async shutdown() (e.g. on a connection failure), still reap the child
        // process and its reader task so they cannot leak. Best-effort and
        // synchronous — no await, so use try_lock + Child::start_kill (SIGKILL).
        if let Ok(mut child) = self.child.try_lock() {
            let _ = child.start_kill();
        }
        if let Ok(mut handle) = self.reader_handle.try_lock() {
            if let Some(h) = handle.take() {
                h.abort();
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mcp/transport/transport_test.rs"]
mod tests;
