use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

pub mod analyzer;
pub mod browser;
pub mod cargo;
pub mod container;
pub mod file;
pub mod fim;
pub mod git;
#[cfg(feature = "hot-reload")]
pub mod hot_reload;
pub mod http;
pub mod knowledge;
pub mod package;
pub mod process;
pub mod search;
pub mod shell;

use browser::{BrowserEval, BrowserFetch, BrowserLinks, BrowserPdf, BrowserScreenshot};
use cargo::{CargoCheck, CargoClippy, CargoFmt, CargoTest};
use container::{
    ComposeDown, ComposeUp, ContainerBuild, ContainerExec, ContainerImages, ContainerList,
    ContainerLogs, ContainerPull, ContainerRemove, ContainerRun, ContainerStop,
};
use file::{DirectoryTree, FileDelete, FileEdit, FileRead, FileWrite};
use git::{GitCheckpoint, GitCommit, GitDiff, GitPush, GitStatus};
use http::HttpRequest;
use knowledge::{
    KnowledgeAdd, KnowledgeClear, KnowledgeExport, KnowledgeQuery, KnowledgeRelate,
    KnowledgeRemove, KnowledgeStats as KnowledgeStatsTool,
};
use package::{NpmInstall, NpmRun, NpmScripts, PipFreeze, PipInstall, PipList, YarnInstall};
use process::{PortCheck, ProcessList, ProcessLogs, ProcessRestart, ProcessStart, ProcessStop};
use search::{GlobFind, GrepSearch, SymbolSearch};
use shell::ShellExec;

/// A tool that can be executed by the agent. Each tool has a name, description,
/// JSON schema for its arguments, and an async `execute` method. Tools are
/// registered in a [`ToolRegistry`] and invoked by name during agent execution.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Value;
    async fn execute(&self, args: Value) -> Result<Value>;
}

/// Name-keyed registry of available tools. Created with all built-in tools
/// pre-registered; additional tools can be added at runtime via [`register`](Self::register).
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    /// Create a new registry pre-populated with all built-in tools.
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };

        // File operations
        registry.register(FileRead::new());
        registry.register(FileWrite::new());
        registry.register(FileEdit::new());
        registry.register(FileDelete::new());
        registry.register(DirectoryTree::new());

        // Git operations
        registry.register(GitStatus);
        registry.register(GitDiff);
        registry.register(GitCommit);
        registry.register(GitPush);
        registry.register(GitCheckpoint);

        // Cargo/Build operations
        registry.register(CargoTest);
        registry.register(CargoCheck);
        registry.register(CargoClippy);
        registry.register(CargoFmt);

        // System operations
        registry.register(ShellExec);

        // Search operations
        registry.register(GrepSearch);
        registry.register(GlobFind);
        registry.register(SymbolSearch);

        // HTTP/Web operations
        registry.register(HttpRequest);

        // Process management operations
        registry.register(ProcessStart);
        registry.register(ProcessStop);
        registry.register(ProcessList);
        registry.register(ProcessLogs);
        registry.register(ProcessRestart);
        registry.register(PortCheck);

        // Package manager operations
        registry.register(NpmInstall);
        registry.register(NpmRun);
        registry.register(NpmScripts);
        registry.register(PipInstall);
        registry.register(PipList);
        registry.register(PipFreeze);
        registry.register(YarnInstall);

        // Container operations (Docker/Podman)
        registry.register(ContainerRun);
        registry.register(ContainerStop);
        registry.register(ContainerList);
        registry.register(ContainerLogs);
        registry.register(ContainerExec);
        registry.register(ContainerBuild);
        registry.register(ContainerImages);
        registry.register(ContainerPull);
        registry.register(ContainerRemove);
        registry.register(ComposeUp);
        registry.register(ComposeDown);

        // Browser automation
        registry.register(BrowserFetch);
        registry.register(BrowserScreenshot);
        registry.register(BrowserPdf);
        registry.register(BrowserEval);
        registry.register(BrowserLinks);

        // Knowledge graph
        registry.register(KnowledgeAdd);
        registry.register(KnowledgeRelate);
        registry.register(KnowledgeQuery);
        registry.register(KnowledgeStatsTool);
        registry.register(KnowledgeClear);
        registry.register(KnowledgeRemove);
        registry.register(KnowledgeExport);

        registry
    }

    /// Register a tool, replacing any existing tool with the same name.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    /// Look up a tool by name, returning `None` if not found.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Return references to all registered tools.
    pub fn list(&self) -> Vec<&dyn Tool> {
        self.tools.values().map(|t| t.as_ref()).collect()
    }

    /// Execute a tool by name with the given arguments
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        let tool = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;
        tool.execute(args).await
    }

    /// Build API-compatible tool definitions for all registered tools.
    pub fn definitions(&self) -> Vec<crate::api::types::ToolDefinition> {
        self.tools
            .values()
            .map(|tool| crate::api::types::ToolDefinition {
                def_type: "function".to_string(),
                function: crate::api::types::FunctionDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.schema(),
                },
            })
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry_new() {
        let registry = ToolRegistry::new();
        // Should have all the default tools registered
        assert!(registry.get("file_read").is_some());
        assert!(registry.get("file_write").is_some());
        assert!(registry.get("shell_exec").is_some());
        assert!(registry.get("cargo_test").is_some());
    }

    #[test]
    fn test_tool_registry_get_nonexistent() {
        let registry = ToolRegistry::new();
        assert!(registry.get("nonexistent_tool").is_none());
    }

    #[test]
    fn test_tool_registry_list() {
        let registry = ToolRegistry::new();
        let tools = registry.list();
        // Should have multiple tools
        assert!(tools.len() > 5);
    }

    #[test]
    fn test_tool_registry_default() {
        let registry = ToolRegistry::default();
        assert!(registry.get("file_read").is_some());
    }

    #[test]
    fn test_tool_registry_definitions() {
        let registry = ToolRegistry::new();
        let definitions = registry.definitions();

        assert!(!definitions.is_empty());

        // Check that definitions have correct structure
        for def in &definitions {
            assert_eq!(def.def_type, "function");
            assert!(!def.function.name.is_empty());
            assert!(!def.function.description.is_empty());
        }
    }

    #[test]
    fn test_file_read_tool_properties() {
        let registry = ToolRegistry::new();
        let tool = registry.get("file_read").unwrap();

        assert_eq!(tool.name(), "file_read");
        assert!(!tool.description().is_empty());

        let schema = tool.schema();
        assert!(schema.get("type").is_some());
    }

    #[test]
    fn test_shell_exec_tool_properties() {
        let registry = ToolRegistry::new();
        let tool = registry.get("shell_exec").unwrap();

        assert_eq!(tool.name(), "shell_exec");
        assert!(tool.description().contains("Execute"));
    }

    #[test]
    fn test_git_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("git_status").is_some());
        assert!(registry.get("git_diff").is_some());
        assert!(registry.get("git_commit").is_some());
        assert!(registry.get("git_push").is_some());
        assert!(registry.get("git_checkpoint").is_some());
    }

    #[test]
    fn test_cargo_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("cargo_test").is_some());
        assert!(registry.get("cargo_check").is_some());
        assert!(registry.get("cargo_clippy").is_some());
        assert!(registry.get("cargo_fmt").is_some());
    }

    #[test]
    fn test_file_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("file_read").is_some());
        assert!(registry.get("file_write").is_some());
        assert!(registry.get("file_edit").is_some());
        assert!(registry.get("file_delete").is_some());
        assert!(registry.get("directory_tree").is_some());
    }

    #[test]
    fn test_search_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("grep_search").is_some());
        assert!(registry.get("glob_find").is_some());
        assert!(registry.get("symbol_search").is_some());
    }

    #[test]
    fn test_process_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("process_start").is_some());
        assert!(registry.get("process_stop").is_some());
        assert!(registry.get("process_list").is_some());
        assert!(registry.get("process_logs").is_some());
        assert!(registry.get("process_restart").is_some());
        assert!(registry.get("port_check").is_some());
    }

    #[test]
    fn test_package_tools_registered() {
        let registry = ToolRegistry::new();

        // npm tools
        assert!(registry.get("npm_install").is_some());
        assert!(registry.get("npm_run").is_some());
        assert!(registry.get("npm_scripts").is_some());

        // pip tools
        assert!(registry.get("pip_install").is_some());
        assert!(registry.get("pip_list").is_some());
        assert!(registry.get("pip_freeze").is_some());

        // yarn tools
        assert!(registry.get("yarn_install").is_some());
    }

    #[test]
    fn test_container_tools_registered() {
        let registry = ToolRegistry::new();

        // Container management
        assert!(registry.get("container_run").is_some());
        assert!(registry.get("container_stop").is_some());
        assert!(registry.get("container_list").is_some());
        assert!(registry.get("container_logs").is_some());
        assert!(registry.get("container_exec").is_some());
        assert!(registry.get("container_build").is_some());
        assert!(registry.get("container_images").is_some());
        assert!(registry.get("container_pull").is_some());
        assert!(registry.get("container_remove").is_some());

        // Compose tools
        assert!(registry.get("compose_up").is_some());
        assert!(registry.get("compose_down").is_some());
    }

    #[test]
    fn test_browser_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("browser_fetch").is_some());
        assert!(registry.get("browser_screenshot").is_some());
        assert!(registry.get("browser_pdf").is_some());
        assert!(registry.get("browser_eval").is_some());
        assert!(registry.get("browser_links").is_some());
    }

    #[test]
    fn test_knowledge_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("knowledge_add").is_some());
        assert!(registry.get("knowledge_relate").is_some());
        assert!(registry.get("knowledge_query").is_some());
        assert!(registry.get("knowledge_stats").is_some());
        assert!(registry.get("knowledge_clear").is_some());
        assert!(registry.get("knowledge_remove").is_some());
        assert!(registry.get("knowledge_export").is_some());
    }
}
