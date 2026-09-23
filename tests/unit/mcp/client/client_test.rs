use super::*;
use async_trait::async_trait;

/// Mock transport returning a canned `tools/call` result.
struct MockTransport {
    response: Value,
}

#[async_trait]
impl Transport for MockTransport {
    async fn request(&self, _method: &str, _params: Option<Value>) -> Result<Value> {
        Ok(self.response.clone())
    }

    async fn notify(&self, _method: &str, _params: Option<Value>) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

fn client_returning(response: Value) -> McpClient {
    McpClient::new_for_test(Arc::new(MockTransport { response }), "mock")
}

#[tokio::test]
async fn is_error_result_is_flagged_as_failure_not_cached_as_success() {
    let client = client_returning(serde_json::json!({
        "content": [{"type": "text", "text": "boom: something failed"}],
        "isError": true,
    }));

    let value = client
        .call_tool("explode", serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(value.get("isError").and_then(|v| v.as_bool()), Some(true));
    // The dispatch layer gates cache inserts on the `success` field (via
    // tool_result_value_indicates_success), so an isError payload must carry
    // success:false — otherwise a later replay would serve the failure as a
    // successful result.
    assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(false));
}

#[tokio::test]
async fn successful_result_is_flagged_as_success() {
    let client = client_returning(serde_json::json!({
        "content": [{"type": "text", "text": "all good"}],
        "isError": false,
    }));

    let value = client
        .call_tool("ping", serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(true));
}

#[tokio::test]
async fn non_text_is_error_result_is_flagged_as_failure() {
    // Results without extractable text content must also be flagged.
    let client = client_returning(serde_json::json!({
        "content": [{"type": "image", "data": "..."}],
        "isError": true,
    }));

    let value = client
        .call_tool("shot", serde_json::json!({}))
        .await
        .unwrap();

    assert_eq!(value.get("success").and_then(|v| v.as_bool()), Some(false));
}

// -----------------------------------------------------------------------
// Dead-server handling end to end over real stdio (tiny `sh` fake servers).
// -----------------------------------------------------------------------

#[cfg(unix)]
fn sh_server(name: &str, script: &str, init_timeout_secs: u64) -> McpServerConfig {
    McpServerConfig {
        name: name.to_string(),
        command: "sh".to_string(),
        args: vec!["-c".to_string(), script.to_string()],
        env: Default::default(),
        init_timeout_secs,
        framing: Default::default(),
    }
}

/// A server that exits immediately must fail `connect` right away with the
/// exit as the cause — not after the (here 30s) init timeout.
#[cfg(unix)]
#[tokio::test]
async fn connect_to_server_that_exits_immediately_fails_fast() {
    let config = sh_server("closer", "exit 0", 30);
    let start = std::time::Instant::now();
    let err = McpClient::connect(&config)
        .await
        .expect_err("connect must fail");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "must fail well under the 30s init timeout, took {:?}",
        start.elapsed()
    );
    let full = format!("{err:#}");
    assert!(
        !full.contains("timed out"),
        "must not be reported as a timeout: {full}"
    );
    assert!(
        err.chain()
            .any(|c| c.downcast_ref::<McpTransportError>().is_some()),
        "cause must be a typed transport error: {full}"
    );
    let line = startup_failure_line("closer", "failed to start", &err);
    assert!(
        line.starts_with("MCP server 'closer' failed to start: MCP server 'closer' "),
        "{line}"
    );
    assert!(line.ends_with("; its tools are unavailable"), "{line}");
    assert!(!line.contains('\n'), "{line}");
}

/// A server that completes the handshake and then dies must fail the next
/// tool call fast, and the rendered error (what the model sees via
/// `to_string()`) must carry the cause, not just "call failed".
#[cfg(unix)]
#[tokio::test]
async fn tool_call_after_server_exit_fails_fast_with_cause() {
    let script = r#"read l; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","serverInfo":{"name":"t"}}}'; read l; exit 0"#;
    let config = sh_server("closer", script, 30);
    let client = Arc::new(McpClient::connect(&config).await.expect("handshake"));

    // Drive it through the tool bridge, as the agent does.
    let tool = crate::mcp::McpTool::new(
        "mcp_closer_probe_echo".into(),
        "probe".into(),
        serde_json::json!({"type": "object"}),
        Arc::clone(&client),
        "probe_echo".into(),
    );
    let start = std::time::Instant::now();
    let err = crate::tools::Tool::execute(&tool, serde_json::json!({"x": 1}))
        .await
        .expect_err("dead server");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "must fail fast, took {:?}",
        start.elapsed()
    );
    let shown = err.to_string();
    assert!(
        shown.starts_with("MCP tool call 'probe_echo' failed on server 'closer': "),
        "{shown}"
    );
    assert!(
        shown.contains("exited / closed its output") || shown.contains("broken pipe"),
        "cause must be visible in to_string(): {shown}"
    );
    let typed = err
        .downcast_ref::<McpToolCallError>()
        .expect("typed call error");
    assert!(matches!(
        typed.transport,
        Some(McpTransportError::ServerExited { .. }) | Some(McpTransportError::BrokenPipe { .. })
    ));

    // A second call is equally fast.
    let start = std::time::Instant::now();
    assert!(client
        .call_tool("probe_echo", serde_json::json!({}))
        .await
        .is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}

/// Transport that fails every request with a server-returned JSON-RPC error.
struct RejectingTransport;

#[async_trait]
impl Transport for RejectingTransport {
    async fn request(&self, method: &str, _params: Option<Value>) -> Result<Value> {
        anyhow::bail!(
            "MCP error for '{}': JSON-RPC error -32000: denied by policy",
            method
        )
    }
    async fn notify(&self, _method: &str, _params: Option<Value>) -> Result<()> {
        Ok(())
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// A server-returned (policy) error is distinguishable from infra failure:
/// the cause is shown and no transport cause is attached.
#[tokio::test]
async fn server_returned_error_is_shown_and_not_typed_as_transport() {
    let client = McpClient::new_for_test(Arc::new(RejectingTransport), "strict");
    let err = client
        .call_tool("write", serde_json::json!({}))
        .await
        .unwrap_err();
    let shown = err.to_string();
    assert!(shown.contains("denied by policy"), "{shown}");
    let typed = err.downcast_ref::<McpToolCallError>().unwrap();
    assert!(typed.transport.is_none());
}

#[test]
fn startup_failure_line_uses_last_links_for_untyped_errors() {
    let err = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::NotFound))
        .context("Failed to spawn MCP server: nope []")
        .context("Failed to spawn MCP server 'x'");
    let line = startup_failure_line("x", "failed to start", &err);
    assert!(
        line.starts_with("MCP server 'x' failed to start: Failed to spawn MCP server: nope []: "),
        "{line}"
    );
    assert!(line.ends_with("; its tools are unavailable"), "{line}");
}
