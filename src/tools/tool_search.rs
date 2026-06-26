//! Tool Search - Allows LLM to discover deferred tools on demand.
//!
//! This tool enables the agent to search for and activate additional tools
//! beyond the critical set that's always available. This dramatically
//! reduces context window usage by deferring most tool schemas until needed.

use super::Tool;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

/// Search for available tools by name or description.
///
/// Use this when you need a specialized tool not in your current tool set.
/// Returns matching tools with their schemas, making them available for use.
pub struct ToolSearchTool {
    /// Shared, mutable index of tools that this search queries.
    /// Using a shared `Vec` avoids a circular dependency between the registry
    /// and the search tool and lets the registry refresh the index whenever
    /// tools are added. A std::sync::RwLock is used because the registry may
    /// rebuild the index from within an async runtime.
    index: Arc<std::sync::RwLock<Vec<ToolSearchResult>>>,
}

/// Trait for types that can search tools.
/// Kept for backward compatibility with existing call sites/tests.
pub trait ToolSearchable: Send + Sync {
    /// Search for tools matching the query string.
    /// Returns up to `limit` matching tool names.
    fn search(&self, query: &str, limit: usize) -> Vec<ToolSearchResult>;
}

/// Result of a tool search.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolSearchResult {
    /// Tool name (e.g., "git_status")
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for the tool's parameters
    pub schema: Value,
    /// Whether this tool is critical (always available) or deferred
    pub is_critical: bool,
    /// Category hint for the tool (e.g., "git", "file", "container")
    pub category: String,
}

impl ToolSearchable for Vec<ToolSearchResult> {
    fn search(&self, query: &str, limit: usize) -> Vec<ToolSearchResult> {
        let query_lower = query.to_lowercase();
        self.iter()
            .filter(|r| {
                r.name.to_lowercase().contains(&query_lower)
                    || r.description.to_lowercase().contains(&query_lower)
            })
            .take(limit)
            .cloned()
            .collect()
    }
}

impl ToolSearchTool {
    /// Create a new ToolSearchTool backed by a shared index.
    pub fn new(index: Arc<std::sync::RwLock<Vec<ToolSearchResult>>>) -> Self {
        Self { index }
    }

    /// Create a ToolSearchTool with an empty index.
    /// The index should be populated by `ToolRegistry` after all tools are registered.
    pub fn placeholder() -> Self {
        Self {
            index: Arc::new(std::sync::RwLock::new(Vec::new())),
        }
    }

    /// Return a clone of the shared index so the registry can populate it.
    pub fn index(&self) -> Arc<std::sync::RwLock<Vec<ToolSearchResult>>> {
        Arc::clone(&self.index)
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Search for available tools by name or description. \
         Use this when you need a specialized tool not in your current tool set. \
         Returns matching tools with their schemas. Once found, these tools become \
         available for the rest of the session."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query to find tools. Can be a tool name (e.g., 'git', 'cargo') or description keyword (e.g., 'container', 'browser')."
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5, max: 20)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("query parameter is required"))?;

        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(5)
            .clamp(1, 20);

        let index = self.index.read().expect("tool_search index poisoned");
        let results = index.search(query, limit);
        drop(index);

        let found_tools: Vec<Value> = results
            .into_iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "description": r.description,
                    "schema": r.schema,
                    "is_critical": r.is_critical,
                    "category": r.category,
                })
            })
            .collect();

        Ok(json!({
            "found_tools": found_tools,
            "count": found_tools.len(),
            "query": query,
            "note": "These tools are now available for use in this session."
        }))
    }
}

/// Helper function to categorize tools based on their name prefix.
pub fn categorize_tool(name: &str) -> &'static str {
    if name.starts_with("git_") {
        "git"
    } else if name.starts_with("file_") || name.starts_with("directory_") {
        "file"
    } else if name.starts_with("cargo_") {
        "cargo"
    } else if name.starts_with("container_") || name.starts_with("compose_") {
        "container"
    } else if name.starts_with("browser_") || name == "page_control" {
        "browser"
    } else if name.starts_with("process_") || name == "port_check" {
        "process"
    } else if name.starts_with("npm_") || name.starts_with("pip_") || name.starts_with("yarn_") {
        "package"
    } else if name.starts_with("vision_") || name == "screen_capture" {
        "vision"
    } else if name.starts_with("knowledge_") {
        "knowledge"
    } else if name.starts_with("lsp_") {
        "lsp"
    } else if name.starts_with("http_") {
        "http"
    } else if name.starts_with("code_") || name.starts_with("context_") {
        "code_intelligence"
    } else if name.starts_with("computer_") {
        "computer_control"
    } else if name == "shell_exec" || name == "pty_shell" {
        "shell"
    } else if name.starts_with("grep_") || name.starts_with("glob_") || name.starts_with("symbol_")
    {
        "search"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn test_categorize_tool() {
        assert_eq!(categorize_tool("git_status"), "git");
        assert_eq!(categorize_tool("file_read"), "file");
        assert_eq!(categorize_tool("cargo_test"), "cargo");
        assert_eq!(categorize_tool("container_run"), "container");
        assert_eq!(categorize_tool("browser_fetch"), "browser");
        assert_eq!(categorize_tool("unknown_tool"), "other");
    }
}
