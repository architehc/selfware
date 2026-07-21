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
    let json = r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32601,"message":"Method not found"}}"#;
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
