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
            write_framed_message(&mut *stdin, &body, self.framing).await?;
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
    // Framing tests (Content-Length regression + newline-delimited spec path)
    // -----------------------------------------------------------------------

    /// Round-trip: client write_message → read_message recovers the exact
    /// body. `read_message` auto-detects the framing, so a Content-Length
    /// framed buffer takes the header-framed read path (both helpers mirror
    /// `server.rs`, which accepts either framing).
    #[tokio::test]
    async fn test_framing_roundtrip_client_to_server() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_message(&mut buffer, body).await.unwrap();

        // Verify on-the-wire shape matches LSP-style Content-Length framing.
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
    /// as a 0-byte body). The buffer starts with `C` so auto-detection takes
    /// the Content-Length read path; a buffer starting with any other byte
    /// would be read as newline-delimited instead.
    #[tokio::test]
    async fn test_framing_missing_content_length_errors() {
        let bytes: &[u8] = b"Content-Type: application/json\r\n\r\n";
        let mut reader = BufReader::new(bytes);
        let result = read_message(&mut reader).await;
        assert!(result.is_err(), "missing Content-Length must error");
    }

    // -----------------------------------------------------------------------
    // Send-framing selection (newline-delimited default, Content-Length opt-in)
    // -----------------------------------------------------------------------

    /// The spec default: newline-delimited framing writes exactly the JSON
    /// body followed by a single `\n` — no headers.
    #[tokio::test]
    async fn test_write_framed_message_newline_delimited_wire_bytes() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_framed_message(&mut buffer, body, Framing::NewlineDelimited)
            .await
            .unwrap();

        let on_wire = String::from_utf8(buffer).unwrap();
        assert_eq!(
            on_wire,
            format!("{}\n", body),
            "newline-delimited framing must be '<json>\\n', got: {:?}",
            on_wire
        );
        assert!(!on_wire.starts_with("Content-Length:"));
    }

    /// The legacy escape hatch: Content-Length framing remains available.
    #[tokio::test]
    async fn test_write_framed_message_content_length_wire_bytes() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_framed_message(&mut buffer, body, Framing::ContentLength)
            .await
            .unwrap();

        let on_wire = String::from_utf8(buffer).unwrap();
        let expected = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        assert_eq!(on_wire, expected);
    }

    /// The read path must accept both framings, even mixed on one stream:
    /// spec-compliant servers reply newline-delimited, legacy ones with
    /// Content-Length headers.
    #[tokio::test]
    async fn test_read_message_parses_both_framings() {
        let nl_body = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#;
        let cl_body = r#"{"jsonrpc":"2.0","id":2,"result":{"ok":false}}"#;

        let mut buffer: Vec<u8> = Vec::new();
        write_framed_message(&mut buffer, nl_body, Framing::NewlineDelimited)
            .await
            .unwrap();
        write_framed_message(&mut buffer, cl_body, Framing::ContentLength)
            .await
            .unwrap();

        let mut reader = BufReader::new(buffer.as_slice());
        let first = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(first, nl_body);
        let second = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(second, cl_body);
        assert!(read_message(&mut reader).await.unwrap().is_none());
    }

    /// Newline-delimited read path over a duplex stream: a body containing an
    /// escaped `\n` inside a string stays one line on the wire and parses as
    /// a single message.
    #[tokio::test]
    async fn test_read_newline_message_with_escaped_newline() {
        let body = r#"{"jsonrpc":"2.0","id":2,"result":{"text":"line1\nline2"}}"#;

        let (client, mut server) = tokio::io::duplex(4096);
        let writer_handle = tokio::spawn(async move {
            server.write_all(body.as_bytes()).await.unwrap();
            server.write_all(b"\n").await.unwrap();
            server.flush().await.unwrap();
        });

        let mut reader = BufReader::new(client);
        let got = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(got, body);

        writer_handle.await.unwrap();
    }

    /// The transport sends newline-delimited by default (the MCP stdio spec
    /// framing), and `with_framing` opts back into Content-Length for legacy
    /// servers.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_transport_send_framing_default_and_override() {
        use std::collections::HashMap;
        assert_eq!(Framing::default(), Framing::NewlineDelimited);

        let transport = StdioTransport::spawn("sleep", &["300".to_string()], &HashMap::new())
            .await
            .expect("spawn sleep");
        assert_eq!(transport.framing, Framing::NewlineDelimited);

        let transport = transport.with_framing(Framing::ContentLength);
        assert_eq!(transport.framing, Framing::ContentLength);
        // Dropping the transport reaps the child (covered by the drop test).
    }

    /// `Framing` deserializes from the config strings the per-server escape
    /// hatch would use (`framing = "content_length"` / `"newline_delimited"`).
    #[test]
    fn test_framing_serde_config_strings() {
        let cl: Framing = serde_json::from_str(r#""content_length""#).unwrap();
        assert_eq!(cl, Framing::ContentLength);
        let nl: Framing = serde_json::from_str(r#""newline_delimited""#).unwrap();
        assert_eq!(nl, Framing::NewlineDelimited);
        // Serializing the default round-trips to the spec spelling.
        assert_eq!(
            serde_json::to_string(&Framing::default()).unwrap(),
            r#""newline_delimited""#
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn drop_reaps_the_child_process() {
        use std::collections::HashMap;
        // A stand-in "server" that just sleeps; StdioTransport::spawn doesn't do
        // an MCP handshake, so any long-lived process works.
        let transport = StdioTransport::spawn("sleep", &["300".to_string()], &HashMap::new())
            .await
            .expect("spawn sleep");
        let pid = transport.child.lock().await.id().expect("child has a pid");

        drop(transport);

        // Poll until the kernel reaps the child (5s deadline) instead of one
        // fixed sleep, which flakes on loaded machines.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let alive = loop {
            // `kill -0 <pid>` fails once the process is gone.
            let alive = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !alive {
                break false;
            }
            if std::time::Instant::now() >= deadline {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        };
        assert!(
            !alive,
            "MCP child (pid {pid}) must be dead after transport drop"
        );
    }
}
