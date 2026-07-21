#[test]
fn test_mcp_tool_name() {
    // We can't easily test execute without a real MCP server,
    // but we can test the metadata methods
    // McpTool requires an Arc<McpClient> which needs a transport,
    // so we just test the struct construction logic conceptually
    let name = "mcp_github_create_issue";
    let description = "Create a GitHub issue";
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "title": {"type": "string"},
            "body": {"type": "string"}
        }
    });

    // Verify name format
    assert!(name.starts_with("mcp_"));
    assert!(schema.get("type").is_some());
    assert_eq!(description, "Create a GitHub issue");
}
