use super::*;

#[tokio::test]
async fn test_tool_search_basic() {
    let index = vec![
        ToolSearchResult {
            name: "git_status".to_string(),
            description: "Get git repository status".to_string(),
            schema: json!({}),
            is_critical: false,
            category: "git".to_string(),
        },
        ToolSearchResult {
            name: "git_commit".to_string(),
            description: "Commit changes".to_string(),
            schema: json!({}),
            is_critical: false,
            category: "git".to_string(),
        },
    ];

    let tool = ToolSearchTool::new(Arc::new(std::sync::RwLock::new(index)));
    let result = tool
        .execute(json!({"query": "git", "limit": 10}))
        .await
        .unwrap();

    let found = result.get("found_tools").unwrap().as_array().unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(result.get("count").unwrap().as_u64(), Some(2));
}

#[tokio::test]
async fn test_tool_search_limit() {
    let index = vec![
        ToolSearchResult {
            name: "tool1".to_string(),
            description: "First tool".to_string(),
            schema: json!({}),
            is_critical: false,
            category: "test".to_string(),
        },
        ToolSearchResult {
            name: "tool2".to_string(),
            description: "Second tool".to_string(),
            schema: json!({}),
            is_critical: false,
            category: "test".to_string(),
        },
        ToolSearchResult {
            name: "tool3".to_string(),
            description: "Third tool".to_string(),
            schema: json!({}),
            is_critical: false,
            category: "test".to_string(),
        },
    ];

    let tool = ToolSearchTool::new(Arc::new(std::sync::RwLock::new(index)));
    let result = tool
        .execute(json!({"query": "tool", "limit": 2}))
        .await
        .unwrap();

    let found = result.get("found_tools").unwrap().as_array().unwrap();
    assert_eq!(found.len(), 2);
}

#[tokio::test]
async fn test_tool_search_missing_query() {
    let index: Vec<ToolSearchResult> = vec![];
    let tool = ToolSearchTool::new(Arc::new(std::sync::RwLock::new(index)));
    let result = tool.execute(json!({})).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tool_search_no_match_reports_failure() {
    // An empty index yields no matches; the result must signal success=false
    // with anti-loop guidance rather than the "now available" success note.
    let index: Vec<ToolSearchResult> = vec![];
    let tool = ToolSearchTool::new(Arc::new(std::sync::RwLock::new(index)));
    let result = tool
        .execute(json!({"query": "nonexistent_capability"}))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["count"], 0);
    let note = result["note"].as_str().unwrap();
    assert!(note.contains("No tools matched"));
    assert!(note.contains("do NOT repeat"));
}

#[tokio::test]
async fn test_tool_search_match_reports_success() {
    let index = vec![ToolSearchResult {
        name: "git_status".to_string(),
        description: "show git status".to_string(),
        schema: json!({}),
        is_critical: false,
        category: "git".to_string(),
    }];
    let tool = ToolSearchTool::new(Arc::new(std::sync::RwLock::new(index)));
    let result = tool.execute(json!({"query": "git"})).await.unwrap();
    assert_eq!(result["success"], true);
    assert!(result["count"].as_u64().unwrap() >= 1);
}

#[test]
fn test_categorize_tool() {
    assert_eq!(categorize_tool("git_status"), "git");
    assert_eq!(categorize_tool("file_read"), "file");
    assert_eq!(categorize_tool("cargo_test"), "cargo");
    assert_eq!(categorize_tool("container_run"), "container");
    assert_eq!(categorize_tool("browser_fetch"), "browser");
    assert_eq!(categorize_tool("unknown_tool"), "other");
}
