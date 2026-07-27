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
