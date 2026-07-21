use super::*;

/// Helper: initialize a server so post-init methods can be tested.
async fn initialize_server(server: &McpServer) {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(0)),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({})),
    };
    server.handle_request(&req).await;
}

/// Serializes tests that mutate SELFWARE_MCP_ALLOW_DESTRUCTIVE
/// (process-wide env state) so they don't race each other.
static DESTRUCTIVE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Set or clear SELFWARE_MCP_ALLOW_DESTRUCTIVE. Callers must hold
/// DESTRUCTIVE_ENV_LOCK for the whole test.
fn set_destructive_opt_in(value: Option<&str>) {
    // SAFETY: every test that touches this variable serializes on
    // DESTRUCTIVE_ENV_LOCK; the code under test only reads it.
    unsafe {
        match value {
            Some(v) => std::env::set_var("SELFWARE_MCP_ALLOW_DESTRUCTIVE", v),
            None => std::env::remove_var("SELFWARE_MCP_ALLOW_DESTRUCTIVE"),
        }
    }
}

#[test]
fn test_json_rpc_request_parsing() {
    let json = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#;
    let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

    assert_eq!(request.jsonrpc, "2.0");
    assert_eq!(request.id, Some(Value::from(1)));
    assert_eq!(request.method, "initialize");
    assert!(request.params.is_some());
}

#[test]
fn test_json_rpc_request_notification_parsing() {
    let json = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let request: JsonRpcRequest = serde_json::from_str(json).unwrap();

    assert_eq!(request.method, "notifications/initialized");
    assert!(request.id.is_none());
    assert!(request.params.is_none());
}

#[test]
fn test_json_rpc_response_serialization() {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        id: Value::from(1),
        result: Some(serde_json::json!({"ok": true})),
        error: None,
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"jsonrpc\":\"2.0\""));
    assert!(json.contains("\"id\":1"));
    assert!(json.contains("\"ok\":true"));
    // error field should be skipped
    assert!(!json.contains("\"error\""));
}

#[test]
fn test_json_rpc_error_response_serialization() {
    let response = JsonRpcResponse {
        jsonrpc: "2.0",
        id: Value::from(2),
        result: None,
        error: Some(JsonRpcError {
            code: METHOD_NOT_FOUND,
            message: "Method not found".to_string(),
            data: None,
        }),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"code\":-32601"));
    assert!(json.contains("\"Method not found\""));
    // result field should be skipped
    assert!(!json.contains("\"result\""));
}

#[test]
fn test_json_rpc_error_display() {
    let err = JsonRpcError {
        code: -32601,
        message: "Method not found".to_string(),
        data: None,
    };
    assert_eq!(
        format!("{}", err),
        "JSON-RPC error -32601: Method not found"
    );
}

#[tokio::test]
async fn test_handle_initialize() {
    let server = McpServer::new();

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(1)),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    assert_eq!(
        result.get("protocolVersion").and_then(|v| v.as_str()),
        Some(MCP_PROTOCOL_VERSION)
    );
    assert_eq!(
        result
            .get("serverInfo")
            .and_then(|i| i.get("name"))
            .and_then(|n| n.as_str()),
        Some("selfware")
    );
    assert!(result.get("capabilities").is_some());
    assert!(server.initialized.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_handle_tools_list() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(2)),
        method: "tools/list".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    let tools = result.get("tools").and_then(|t| t.as_array()).unwrap();
    // ToolRegistry::new() registers many tools; check that we got a reasonable number
    assert!(tools.len() > 10, "Expected many tools, got {}", tools.len());

    // Verify tool structure
    let first_tool = &tools[0];
    assert!(first_tool.get("name").is_some());
    assert!(first_tool.get("description").is_some());
    assert!(first_tool.get("inputSchema").is_some());
}

#[tokio::test]
async fn test_handle_tools_call_missing_params() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(3)),
        method: "tools/call".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
}

#[tokio::test]
async fn test_handle_tools_call_missing_name() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(4)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({"arguments": {}})),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
}

#[tokio::test]
async fn test_handle_tools_call_unknown_tool() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(5)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "nonexistent_tool_xyz",
            "arguments": {}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    // Tool errors are returned as isError content, not JSON-RPC errors
    let result = response.result.unwrap();
    assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(true));
}

#[tokio::test]
async fn test_handle_tools_call_blocks_dangerous_shell_command() {
    // Regression test: an MCP client (any external tool speaking the
    // protocol) used to be able to invoke any registered tool, including
    // shell_exec, with none of the src/safety/ checks applied at all.
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(7)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "shell_exec",
            "arguments": {"command": "rm -rf /"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    let result = response.result.unwrap();
    assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(true));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Blocked by safety checker"),
        "expected the dangerous command to be blocked, got: {text}"
    );
}

#[tokio::test]
async fn test_handle_tools_call_allows_safe_shell_command() {
    // shell_exec is classified destructive, so over MCP it needs the
    // operator opt-in (see mcp_destructive_tools_allowed).
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(Some("1"));
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(8)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "shell_exec",
            "arguments": {"command": "echo hello"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    let result = response.result.unwrap();
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        !text.contains("Blocked by safety checker"),
        "expected the safe command to run, got: {text}"
    );
    set_destructive_opt_in(None);
}

#[tokio::test]
async fn test_handle_resources_list() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(6)),
        method: "resources/list".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    let resources = result.get("resources").and_then(|r| r.as_array()).unwrap();
    assert_eq!(resources.len(), 3);

    // Check URIs
    let uris: Vec<&str> = resources
        .iter()
        .filter_map(|r| r.get("uri").and_then(|u| u.as_str()))
        .collect();
    assert!(uris.contains(&"selfware://project/files"));
    assert!(uris.contains(&"selfware://project/structure"));
    assert!(uris.contains(&"selfware://config"));
}

#[tokio::test]
async fn test_handle_resources_read_missing_params() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(7)),
        method: "resources/read".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
}

#[tokio::test]
async fn test_handle_resources_read_unknown_uri() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(8)),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({"uri": "selfware://unknown"})),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_PARAMS);
}

#[tokio::test]
async fn test_handle_resources_read_project_files() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(9)),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({"uri": "selfware://project/files"})),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    assert!(result.get("contents").is_some());
}

#[tokio::test]
async fn test_handle_resources_read_project_structure() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(10)),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({"uri": "selfware://project/structure"})),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());

    let result = response.result.unwrap();
    let contents = result.get("contents").and_then(|c| c.as_array()).unwrap();
    assert!(!contents.is_empty());
}

#[tokio::test]
async fn test_handle_method_not_found() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(11)),
        method: "unknown/method".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, METHOD_NOT_FOUND);
}

#[tokio::test]
async fn test_handle_ping() {
    let server = McpServer::new();

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(12)),
        method: "ping".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_handle_notification_no_response() {
    let server = McpServer::new();

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: None, // Notification
        method: "notifications/initialized".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await;
    assert!(
        response.is_none(),
        "Notifications should not produce a response"
    );
}

#[tokio::test]
async fn test_content_length_framing_roundtrip() {
    let message_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    // Write
    let mut buffer: Vec<u8> = Vec::new();
    write_message(&mut buffer, message_body).await.unwrap();

    let expected = format!(
        "Content-Length: {}\r\n\r\n{}",
        message_body.len(),
        message_body
    );
    assert_eq!(String::from_utf8(buffer.clone()).unwrap(), expected);

    // Read back — auto-detection must pick Content-Length framing.
    let mut reader = BufReader::new(buffer.as_slice());
    let (read_back, framing) = read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(read_back, message_body);
    assert_eq!(framing, Framing::ContentLength);
}

#[tokio::test]
async fn test_read_message_eof() {
    let empty: &[u8] = b"";
    let mut reader = BufReader::new(empty);
    let result = read_message(&mut reader).await.unwrap();
    assert!(result.is_none());
}

// -----------------------------------------------------------------------
// Wire framing: auto-detection and framing-matched responses (P0-8)
//
// The MCP stdio spec (2024-11-05) frames messages as newline-delimited
// JSON-RPC, but this server originally only spoke LSP-style
// Content-Length framing — spec-standard clients got zero bytes back and
// the server looked dead. The server now auto-detects the framing per
// message and answers in the same framing.
// -----------------------------------------------------------------------

#[tokio::test]
async fn test_newline_framing_roundtrip() {
    let message_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;

    let mut buffer: Vec<u8> = Vec::new();
    write_framed_message(&mut buffer, message_body, Framing::NewlineDelimited)
        .await
        .unwrap();

    // On the wire: exactly the body plus a trailing newline, no headers.
    let on_wire = String::from_utf8(buffer.clone()).unwrap();
    assert_eq!(on_wire, format!("{}\n", message_body));

    let mut reader = BufReader::new(buffer.as_slice());
    let (read_back, framing) = read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(read_back, message_body);
    assert_eq!(framing, Framing::NewlineDelimited);

    // After the message, the next read must hit clean EOF.
    let eof = read_message(&mut reader).await.unwrap();
    assert!(eof.is_none());
}

#[tokio::test]
async fn test_detect_framing_per_message() {
    // A Content-Length message followed by a newline-delimited one on the
    // same stream must each be detected correctly.
    let cl_body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    let nl_body = r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;

    let mut buffer: Vec<u8> = Vec::new();
    write_message(&mut buffer, cl_body).await.unwrap();
    write_framed_message(&mut buffer, nl_body, Framing::NewlineDelimited)
        .await
        .unwrap();

    let mut reader = BufReader::new(buffer.as_slice());
    let (body1, framing1) = read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(
        (body1.as_str(), framing1),
        (cl_body, Framing::ContentLength)
    );
    let (body2, framing2) = read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(
        (body2.as_str(), framing2),
        (nl_body, Framing::NewlineDelimited)
    );
    assert!(read_message(&mut reader).await.unwrap().is_none());
}

/// A newline-delimited message that arrives byte-by-byte must still be
/// read as one message (BufReader awaits the rest of the line).
#[tokio::test]
async fn test_newline_framing_partial_writes() {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;

    let (client, mut server_side) = tokio::io::duplex(4096);
    let writer = tokio::spawn(async move {
        for chunk in body.as_bytes().chunks(7) {
            server_side.write_all(chunk).await.unwrap();
            server_side.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        server_side.write_all(b"\n").await.unwrap();
        server_side.flush().await.unwrap();
    });

    let mut reader = BufReader::new(client);
    let (recovered, framing) = read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(recovered, body);
    assert_eq!(framing, Framing::NewlineDelimited);
    writer.await.unwrap();
}

// -- Full serve_io sessions over in-memory duplex streams ---------------

/// Spin up `serve_io` on one end of a duplex pair; return the client end.
fn spawn_test_server() -> (tokio::io::DuplexStream, tokio::task::JoinHandle<Result<()>>) {
    let server = Arc::new(McpServer::new());
    let (client, server_side) = tokio::io::duplex(1024 * 1024);
    let (server_in, server_out) = tokio::io::split(server_side);
    let handle = tokio::spawn(serve_io(server, server_in, server_out));
    (client, handle)
}

/// Send one newline-delimited request and read the newline-delimited
/// response, asserting the wire shape (single line, no headers).
async fn newline_roundtrip(
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    request: &str,
) -> Value {
    writer.write_all(request.as_bytes()).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    writer.flush().await.unwrap();

    let mut line = String::new();
    let n = reader.read_line(&mut line).await.unwrap();
    assert!(n > 0, "server must answer a newline-delimited request");
    assert!(
        !line.starts_with("Content-Length:"),
        "newline-delimited request must not get a header-framed response: {:?}",
        line
    );
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("response is not JSON: {e}: {line:?}"))
}

/// Send one Content-Length framed request and read the response,
/// asserting the exact LSP-style wire shape.
async fn content_length_roundtrip(
    writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>,
    reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    request: &str,
) -> Value {
    write_message(writer, request).await.unwrap();

    let mut header = String::new();
    let n = reader.read_line(&mut header).await.unwrap();
    assert!(n > 0, "server must answer a Content-Length request");
    let length: usize = header
        .strip_prefix("Content-Length: ")
        .unwrap_or_else(|| panic!("expected Content-Length header, got: {header:?}"))
        .trim()
        .parse()
        .expect("Content-Length must be a number");
    let mut blank = String::new();
    reader.read_line(&mut blank).await.unwrap();
    assert_eq!(blank, "\r\n", "expected CRLF after headers");

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await.unwrap();
    serde_json::from_slice(&buf).unwrap_or_else(|e| panic!("response body is not JSON: {e}"))
}

/// Spec-standard session: newline-delimited initialize → tools/list →
/// tools/call → shutdown, all answered newline-delimited. This is the
/// exact scenario that previously got zero bytes back (P0-8).
#[tokio::test]
async fn test_serve_io_newline_delimited_session() {
    // shell_exec is destructive-classified: opt in for this session.
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(Some("1"));
    let (client, server_task) = spawn_test_server();
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut reader = BufReader::new(client_read);

    // initialize
    let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"spec-client","version":"1.0"}}}"#,
        )
        .await;
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);
    assert_eq!(resp["result"]["serverInfo"]["name"], "selfware");

    // notifications/initialized — no response expected.
    client_write
        .write_all(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
        .await
        .unwrap();
    client_write.write_all(b"\n").await.unwrap();
    client_write.flush().await.unwrap();

    // tools/list
    let resp = newline_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    )
    .await;
    assert_eq!(resp["id"], 2);
    let tools = resp["result"]["tools"].as_array().unwrap();
    assert!(tools.len() > 10, "tools/list should return the catalog");
    assert!(tools[0]["name"].is_string());
    assert!(tools[0]["inputSchema"].is_object());

    // tools/call
    let resp = newline_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shell_exec","arguments":{"command":"echo hello"}}}"#,
        )
        .await;
    assert_eq!(resp["id"], 3);
    assert_eq!(resp["result"]["isError"], false);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("hello"), "expected echo output, got: {text}");

    // shutdown — server answers, then the serve loop exits cleanly.
    let resp = newline_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
    )
    .await;
    assert_eq!(resp["id"], 4);

    tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
        .await
        .expect("server must exit after shutdown")
        .unwrap()
        .unwrap();
    set_destructive_opt_in(None);
}

/// Back-compat session: a legacy client speaking Content-Length framing
/// must keep working, with Content-Length framed responses.
#[tokio::test]
async fn test_serve_io_content_length_session() {
    // shell_exec is destructive-classified: opt in for this session.
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(Some("1"));
    let (client, server_task) = spawn_test_server();
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut reader = BufReader::new(client_read);

    // initialize
    let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"legacy-client","version":"1.0"}}}"#,
        )
        .await;
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], MCP_PROTOCOL_VERSION);

    // tools/list
    let resp = content_length_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    )
    .await;
    assert_eq!(resp["id"], 2);
    assert!(resp["result"]["tools"].as_array().unwrap().len() > 10);

    // tools/call
    let resp = content_length_roundtrip(
            &mut client_write,
            &mut reader,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"shell_exec","arguments":{"command":"echo hello"}}}"#,
        )
        .await;
    assert_eq!(resp["id"], 3);
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("hello"), "expected echo output, got: {text}");

    // shutdown
    let resp = content_length_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":4,"method":"shutdown"}"#,
    )
    .await;
    assert_eq!(resp["id"], 4);

    tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
        .await
        .expect("server must exit after shutdown")
        .unwrap()
        .unwrap();
    set_destructive_opt_in(None);
}

/// Framing is detected per message, not per connection: a client may mix
/// framings on one stream and each response matches its request.
#[tokio::test]
async fn test_serve_io_mixed_framing_session() {
    let (client, server_task) = spawn_test_server();
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut reader = BufReader::new(client_read);

    // newline-delimited initialize → newline-delimited response
    let resp = newline_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
    )
    .await;
    assert_eq!(resp["id"], 1);

    // Content-Length ping → Content-Length response
    let resp = content_length_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
    )
    .await;
    assert_eq!(resp["id"], 2);
    assert!(resp["result"].is_object());

    // newline-delimited shutdown → newline-delimited response
    let resp = newline_roundtrip(
        &mut client_write,
        &mut reader,
        r#"{"jsonrpc":"2.0","id":3,"method":"shutdown"}"#,
    )
    .await;
    assert_eq!(resp["id"], 3);

    tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
        .await
        .expect("server must exit after shutdown")
        .unwrap()
        .unwrap();
}

/// Invalid JSON on a newline-delimited stream gets a newline-delimited
/// JSON-RPC parse error, not silence.
#[tokio::test]
async fn test_serve_io_parse_error_matches_newline_framing() {
    let (client, server_task) = spawn_test_server();
    let (client_read, mut client_write) = tokio::io::split(client);
    let mut reader = BufReader::new(client_read);

    let resp = newline_roundtrip(&mut client_write, &mut reader, "this is not json").await;
    assert_eq!(resp["error"]["code"], PARSE_ERROR);
    assert!(resp["id"].is_null());

    // EOF on the client side shuts the server down cleanly. Both halves
    // of the duplex stream must go: tokio's split shares one underlying
    // stream, which only signals EOF once it is fully dropped.
    drop(client_write);
    drop(reader);
    tokio::time::timeout(std::time::Duration::from_secs(5), server_task)
        .await
        .expect("server must exit on EOF")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn test_handle_resources_read_file_path_escape() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(13)),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({
            "uri": "selfware://project/file/../../../etc/passwd"
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    // Should either fail with error or be blocked by path validation
    // The exact behavior depends on canonicalize, but it should not
    // return /etc/passwd content.
    if let Some(result) = &response.result {
        // If it somehow returned content, verify it's not /etc/passwd
        if let Some(contents) = result.get("contents").and_then(|c| c.as_array()) {
            for content in contents {
                if let Some(text) = content.get("text").and_then(|t| t.as_str()) {
                    assert!(!text.contains("root:"), "Path traversal should be blocked");
                }
            }
        }
    }
    // Having an error response is also acceptable
}

#[tokio::test]
async fn test_tool_list_has_correct_schema_format() {
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(14)),
        method: "tools/list".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    let result = response.result.unwrap();
    let tools = result.get("tools").and_then(|t| t.as_array()).unwrap();

    for tool in tools {
        let name = tool.get("name").and_then(|n| n.as_str()).unwrap();
        let description = tool.get("description").and_then(|d| d.as_str()).unwrap();
        let schema = tool.get("inputSchema").unwrap();

        assert!(!name.is_empty(), "Tool name should not be empty");
        assert!(
            !description.is_empty(),
            "Tool '{}' description should not be empty",
            name
        );
        // MCP requires inputSchema to be a JSON Schema object
        assert!(
            schema.is_object(),
            "Tool '{}' inputSchema should be an object",
            name
        );
    }
}

#[tokio::test]
async fn test_handle_shutdown() {
    let server = McpServer::new();

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(15)),
        method: "shutdown".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
}

#[tokio::test]
async fn test_tools_call_rejected_before_initialize() {
    // A client must not be able to call tools/call before the
    // `initialize` handshake has been completed.
    let server = McpServer::new();
    assert!(!server.initialized.load(Ordering::SeqCst));

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(20)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "shell_exec",
            "arguments": {"command": "echo hi"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_REQUEST);
}

#[tokio::test]
async fn test_tools_list_rejected_before_initialize() {
    let server = McpServer::new();

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(21)),
        method: "tools/list".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_some());
    assert_eq!(response.error.as_ref().unwrap().code, INVALID_REQUEST);
}

#[tokio::test]
async fn test_ping_allowed_before_initialize() {
    let server = McpServer::new();
    assert!(!server.initialized.load(Ordering::SeqCst));

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(22)),
        method: "ping".to_string(),
        params: None,
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    assert!(response.result.is_some());
}

#[tokio::test]
async fn test_tools_call_allowed_after_initialize() {
    let server = McpServer::new();

    // Initialize first
    let init_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(23)),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({})),
    };
    server.handle_request(&init_req).await;
    assert!(server.initialized.load(Ordering::SeqCst));

    // Now tools/call should not be rejected with INVALID_REQUEST
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(24)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "nonexistent_tool_xyz",
            "arguments": {}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    // Should not get INVALID_REQUEST — the error (if any) should be
    // a tool-not-found content response, not a protocol error.
    if let Some(err) = &response.error {
        assert_ne!(err.code, INVALID_REQUEST);
    }
}

#[test]
fn test_list_project_files_bounded() {
    let server = McpServer::new();
    let files = server.list_project_files();
    // Should return some files but be bounded
    assert!(files.len() <= 10_000);
}

#[test]
fn test_build_directory_tree() {
    let server = McpServer::new();
    let tree = server.build_directory_tree(&server.project_root, 0, 2);
    // Should produce some output for the project root
    assert!(!tree.is_empty());
    // Should contain directory markers
    assert!(tree.contains('/'));
}

/// Verify that a cloned `McpServer` (as used by per-request tasks)
/// shares the `initialized` flag — i.e. that `initialize` on one clone
/// is visible to all others.  This is the core concurrency invariant.
#[tokio::test]
async fn test_concurrent_handle_request_via_shared_handle() {
    let server = std::sync::Arc::new(McpServer::new());

    // Initialize via the shared handle.
    let init_req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(100)),
        method: "initialize".to_string(),
        params: Some(serde_json::json!({})),
    };
    let _ = server.handle_request(&init_req).await.unwrap();
    assert!(server.initialized.load(std::sync::atomic::Ordering::SeqCst));

    // Spawn multiple concurrent tools/list requests through cloned handles.
    // If the shared state works, all should succeed (no INVALID_REQUEST).
    let mut handles = Vec::new();
    for i in 0..5u64 {
        let server = std::sync::Arc::clone(&server);
        handles.push(tokio::spawn(async move {
            let req = JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Some(Value::from(200 + i)),
                method: "tools/list".to_string(),
                params: None,
            };
            server.handle_request(&req).await.unwrap()
        }));
    }

    for handle in handles {
        let response = handle.await.unwrap();
        assert!(
            response.error.is_none(),
            "concurrent tools/list should succeed"
        );
        assert!(response.result.is_some());
    }
}

// -- Secret redaction & destructive-tool gate (whole-repo review P1/P2) --

/// The MCP `selfware://config` resource must never hand a client the raw
/// API key or MCP server env secrets — RedactedString's Serialize impl
/// emits the real value, so the resource serializes a redacted view.
#[tokio::test]
async fn test_resources_read_config_redacts_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("selfware.toml");
    std::fs::write(
        &config_path,
        r#"
api_key = "sk-live-test-secret-12345"

[[mcp.servers]]
name = "github"
command = "npx"
env = { GITHUB_TOKEN = "ghp-test-secret-67890" }
"#,
    )
    .unwrap();

    let server = McpServer::with_config(Some(config_path.to_string_lossy().to_string()));
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(31)),
        method: "resources/read".to_string(),
        params: Some(serde_json::json!({"uri": "selfware://config"})),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    let result = response.result.unwrap();
    let text = result["contents"][0]["text"].as_str().unwrap_or("");
    assert!(
        !text.contains("sk-live-test-secret-12345"),
        "raw api_key must not leak to MCP clients: {text}"
    );
    assert!(
        !text.contains("ghp-test-secret-67890"),
        "MCP server env secrets must not leak to MCP clients: {text}"
    );
    assert!(
        text.contains("<redacted>"),
        "expected redaction markers in the config view: {text}"
    );
}

/// Destructive tools are refused over MCP by default: the protocol has
/// no confirmation channel, so the refusal payload documents the opt-in.
#[tokio::test]
async fn test_tools_call_destructive_refused_without_opt_in() {
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(None);
    // Hermetic: use the code-default safety config so the test does not depend
    // on the developer's on-disk `denied_paths` (e.g. a local `**/target/**`
    // rule would block this path before the destructive gate under test).
    let server = McpServer::with_explicit_safety_config(crate::config::SafetyConfig::default());
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(32)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "file_delete",
            "arguments": {"path": "target/tmp-mcp-destructive-test"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    assert!(response.error.is_none());
    let result = response.result.unwrap();
    assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(true));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("destructive"),
        "refusal must say why, got: {text}"
    );
    assert!(
        text.contains("SELFWARE_MCP_ALLOW_DESTRUCTIVE"),
        "refusal must document the opt-in, got: {text}"
    );
}

/// With the operator opt-in set, a destructive-classified tool executes.
#[tokio::test]
async fn test_tools_call_destructive_allowed_with_opt_in() {
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(Some("1"));
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(33)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "shell_exec",
            "arguments": {"command": "echo hello"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    let result = response.result.unwrap();
    assert_eq!(result.get("isError").and_then(|v| v.as_bool()), Some(false));
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("hello"), "expected echo output, got: {text}");
    set_destructive_opt_in(None);
}

/// Read-only tools are not affected by the destructive gate.
#[tokio::test]
async fn test_tools_call_readonly_tool_not_gated() {
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(None);
    let server = McpServer::new();
    initialize_server(&server).await;

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(Value::from(34)),
        method: "tools/call".to_string(),
        params: Some(serde_json::json!({
            "name": "file_read",
            "arguments": {"path": "Cargo.toml"}
        })),
    };

    let response = server.handle_request(&request).await.unwrap();
    let result = response.result.unwrap();
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        !text.contains("confirmation channel"),
        "read-only tools must not hit the destructive gate, got: {text}"
    );
}

#[tokio::test]
async fn test_mcp_destructive_tools_allowed_parsing() {
    let _guard = DESTRUCTIVE_ENV_LOCK.lock().await;
    set_destructive_opt_in(None);
    assert!(!mcp_destructive_tools_allowed());
    set_destructive_opt_in(Some("1"));
    assert!(mcp_destructive_tools_allowed());
    set_destructive_opt_in(Some("true"));
    assert!(mcp_destructive_tools_allowed());
    set_destructive_opt_in(Some("0"));
    assert!(!mcp_destructive_tools_allowed());
    set_destructive_opt_in(None);
}
