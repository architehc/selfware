use super::*;
use std::fs;
use tempfile::{tempdir, NamedTempFile};

#[test]
fn test_page_control_tool_name() {
    let tool = PageControlTool::new();
    assert_eq!(tool.name(), "page_control");
}

#[test]
fn test_page_control_tool_description() {
    let tool = PageControlTool::new();
    let desc = tool.description();
    assert!(desc.contains("Playwright"));
    assert!(desc.contains("navigation"));
    assert!(desc.contains("interaction"));
    assert!(desc.contains("multi-tab"));
}

#[test]
fn test_page_control_schema() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].get("action").is_some());
    assert!(schema["properties"].get("url").is_some());
    assert!(schema["properties"].get("selector").is_some());
    assert!(schema["properties"].get("text").is_some());
    assert!(schema["properties"].get("timeout_ms").is_some());
    assert!(schema["properties"].get("tab_index").is_some());
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("action")));
}

#[test]
fn test_page_control_schema_action_enum() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    let action_enum = schema["properties"]["action"]["enum"].as_array().unwrap();
    assert!(action_enum.contains(&json!("goto")));
    assert!(action_enum.contains(&json!("click")));
    assert!(action_enum.contains(&json!("type")));
    assert!(action_enum.contains(&json!("fill")));
    assert!(action_enum.contains(&json!("text")));
    assert!(action_enum.contains(&json!("screenshot")));
    assert!(action_enum.contains(&json!("evaluate")));
    assert!(action_enum.contains(&json!("new_tab")));
    assert!(action_enum.contains(&json!("shutdown")));
}

#[test]
fn test_page_control_rejects_unsafe_output_path() {
    crate::tools::file::reset_safety_config_for_tests();
    let err = validate_page_output_path("/etc/selfware-page.png", "page_control")
        .expect_err("absolute output outside allowed paths must be rejected");
    assert!(err.to_string().contains("output path validation failed"));
}

#[test]
fn test_valid_actions_completeness() {
    // Ensure all documented action categories are present
    let nav_actions = ["goto", "back", "forward", "reload", "wait_for"];
    let interaction_actions = [
        "click", "type", "fill", "select", "check", "uncheck", "hover", "press",
    ];
    let content_actions = ["text", "html", "attribute", "value", "count", "visible"];
    let page_info_actions = ["title", "url", "screenshot", "pdf"];
    let js_actions = ["evaluate", "evaluate_handle"];
    let tab_actions = ["new_tab", "switch_tab", "close_tab", "list_tabs"];
    let lifecycle_actions = ["shutdown"];

    for action in nav_actions
        .iter()
        .chain(interaction_actions.iter())
        .chain(content_actions.iter())
        .chain(page_info_actions.iter())
        .chain(js_actions.iter())
        .chain(tab_actions.iter())
        .chain(lifecycle_actions.iter())
    {
        assert!(VALID_ACTIONS.contains(action), "Missing action: {}", action);
    }
}

#[tokio::test]
async fn test_page_control_missing_action() {
    let tool = PageControlTool::new();
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("action is required"));
}

#[tokio::test]
async fn test_page_control_invalid_action() {
    let tool = PageControlTool::new();
    let result = tool.execute(json!({"action": "nonexistent"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown action"));
}

#[test]
fn test_validate_url_allows_workspace_file() {
    let workspace_file = NamedTempFile::new_in(std::env::current_dir().unwrap()).unwrap();
    fs::write(workspace_file.path(), "<html><body>ok</body></html>").unwrap();
    let url = format!("file://{}", workspace_file.path().display());

    let result = validate_url(&url);
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_blocks_file_outside_workspace() {
    let outside = tempdir().unwrap();
    let file = outside.path().join("external.html");
    fs::write(&file, "<html><body>blocked</body></html>").unwrap();

    let result = validate_url(&format!("file://{}", file.display()));
    assert!(result.is_err());
}

#[test]
fn test_validate_url_blocks_private_ip() {
    let result = validate_url("http://192.168.1.10/test");
    assert!(result.is_err());
}

#[test]
fn test_validate_url_allows_localhost() {
    let result = validate_url("http://localhost/test");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_private_ip_allowed_with_opt_in() {
    let result = validate_url_with_allow_private("http://192.168.1.10/test", true);
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_allows_public() {
    let result = validate_url("https://example.com");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_allows_public_ip() {
    let result = validate_url("https://1.1.1.1/");
    assert!(result.is_ok());
}

#[test]
fn test_validate_url_blocks_ftp() {
    let result = validate_url("ftp://example.com/file");
    assert!(result.is_err());
}

#[test]
fn test_validate_url_allows_zero_ip_binding() {
    let result = validate_url("http://0.0.0.0/");
    assert!(result.is_ok());
}

#[test]
fn test_is_private_ip_v4() {
    assert!(net_policy::is_private_or_internal_ip(
        &"127.0.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"10.0.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"192.168.1.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"172.16.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"169.254.0.1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"0.0.0.0".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"8.8.8.8".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"1.1.1.1".parse().unwrap()
    ));
}

#[test]
fn test_is_private_ip_v6() {
    assert!(net_policy::is_private_or_internal_ip(
        &"::1".parse().unwrap()
    ));
    assert!(net_policy::is_private_or_internal_ip(
        &"::".parse().unwrap()
    ));
    assert!(!net_policy::is_private_or_internal_ip(
        &"2606:4700::1111".parse().unwrap()
    ));
}

#[test]
fn test_page_controller_default() {
    let _controller = PageController::default();
}

#[test]
fn test_page_control_tool_default() {
    let _tool = PageControlTool::default();
}

#[test]
fn test_page_control_schema_has_all_params() {
    let tool = PageControlTool::new();
    let schema = tool.schema();
    let props = schema["properties"].as_object().unwrap();

    // Verify all expected parameters exist
    let expected_params = vec![
        "action",
        "url",
        "selector",
        "text",
        "value",
        "values",
        "key",
        "name",
        "expression",
        "timeout_ms",
        "tab_index",
        "path",
        "full_page",
        "all",
        "outer",
        "wait_until",
        "load_state",
        "state",
        "button",
        "click_count",
        "delay",
        "format",
    ];

    for param in &expected_params {
        assert!(
            props.contains_key(*param),
            "Missing schema param: {}",
            param
        );
    }
}

#[tokio::test]
async fn test_page_control_url_validation_on_goto() {
    let tool = PageControlTool::new();
    // This should fail URL validation before even trying to spawn the bridge
    let result = tool
        .execute(json!({"action": "goto", "url": "file:///etc/passwd"}))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_bridge_response_deserialization() {
    let json_str = r#"{"id":1,"success":true,"result":{"url":"https://example.com"},"error":null}"#;
    let resp: BridgeResponse = serde_json::from_str(json_str).unwrap();
    assert_eq!(resp.id, Some(1));
    assert!(resp.success);
    assert!(resp.result.is_some());
    assert!(resp.error.is_none());
}

#[test]
fn test_bridge_response_error() {
    let json_str = r#"{"id":2,"success":false,"result":null,"error":"something broke"}"#;
    let resp: BridgeResponse = serde_json::from_str(json_str).unwrap();
    assert_eq!(resp.id, Some(2));
    assert!(!resp.success);
    assert_eq!(resp.error, Some("something broke".to_string()));
}

#[test]
fn embedded_bridge_is_present() {
    assert!(!EMBEDDED_BRIDGE_JS.is_empty());
    assert!(EMBEDDED_BRIDGE_JS.contains("playwright"));
}

#[test]
fn extract_embedded_bridge_writes_and_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("sw_bridge_test_{}", std::process::id()));
    let script = PlaywrightBridge::extract_embedded_bridge_to(&dir).unwrap();
    assert!(script.exists());
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        EMBEDDED_BRIDGE_JS
    );
    assert!(dir.join("package.json").exists());
    // second call is a no-op (content already matches)
    let script2 = PlaywrightBridge::extract_embedded_bridge_to(&dir).unwrap();
    assert_eq!(script, script2);
    let _ = std::fs::remove_dir_all(&dir);
}
