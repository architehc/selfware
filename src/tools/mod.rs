//! Tool subsystem: trait definition, registry, and built-in tool implementations.
//!
//! Every capability the agent can invoke (file I/O, git, shell, browser, etc.) is
//! a struct that implements the [`Tool`] trait. Tools are collected into a
//! [`ToolRegistry`] at agent startup — `ToolRegistry::new()` pre-registers all
//! built-in tools, and additional tools (MCP server tools, FIM edit) can be added
//! via [`ToolRegistry::register`].
//!
//! ## Tool execution lifecycle
//!
//! 1. The LLM emits a tool call with a name and JSON arguments.
//! 2. The agent resolves the name against [`ToolRegistry::get`].
//! 3. Arguments are validated against the tool's [`Tool::schema`] via
//!    [`validate_tool_arguments_schema`] (required-field and type checks).
//! 4. The safety checker gates execution (confirmation prompts, deny-lists).
//! 5. [`Tool::execute`] runs asynchronously and returns a JSON result.
//! 6. The result is fed back into the conversation for the next LLM turn.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub mod analyzer;
pub mod browser;
pub mod cargo;
pub mod code_metrics;
pub mod codemap;
pub mod computer;
pub mod container;
pub mod context;
pub mod file;
pub mod file_read;
pub mod fim;
pub mod git;
pub mod git_worktree;
pub mod grep_search;
#[cfg(feature = "hot-reload")]
pub mod hot_reload;
pub mod http;
pub mod introspect;
pub mod knowledge;
pub mod lsp_tools;
pub mod net_policy;
pub mod package;
pub mod page_controller;
pub mod process;
pub mod prompt;
pub mod pty_shell;
pub mod radarcam;
pub mod screen_capture;
pub mod search;
pub mod shell;
pub mod shell_exec;
pub mod swarm_tool;
pub mod task_focus;
pub mod tool_search;
pub mod vision;

use browser::{BrowserEval, BrowserFetch, BrowserLinks, BrowserPdf, BrowserScreenshot};
use cargo::{CargoCheck, CargoClippy, CargoFmt, CargoTest};
use container::{
    ComposeDown, ComposeUp, ContainerBuild, ContainerExec, ContainerImages, ContainerList,
    ContainerLogs, ContainerPull, ContainerRemove, ContainerRun, ContainerStop,
};
use file::{DirectoryTree, FileDelete, FileEdit, FileWrite};
use file_read::FileRead;
use git::{GitCheckpoint, GitCommit, GitDiff, GitPush, GitStatus};
use git_worktree::{EnterWorktreeTool, ExitWorktreeTool, ListWorktreesTool};
use grep_search::GrepSearch;
use http::HttpRequest;
use knowledge::{
    KnowledgeAdd, KnowledgeAutoExtract, KnowledgeClear, KnowledgeExport, KnowledgeQuery,
    KnowledgeRelate, KnowledgeRemove, KnowledgeStats as KnowledgeStatsTool,
};
use package::{NpmInstall, NpmRun, NpmScripts, PipFreeze, PipInstall, PipList, YarnInstall};
use page_controller::PageControlTool;
use process::{PortCheck, ProcessList, ProcessLogs, ProcessRestart, ProcessStart, ProcessStop};
use pty_shell::PtyShellTool;
use radarcam::{
    RadarCamControl, RadarCamFrame, RadarCamIntrospect, RadarCamLogs, RadarCamStatus, RadarCamTest,
};
use screen_capture::ScreenCapture;
use search::{GlobFind, SymbolSearch};
use shell_exec::ShellExec;
use vision::{VisionAnalyze, VisionCompare};

/// Pagination metadata for truncated tool output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// Character offset from which this page starts.
    pub offset: usize,
    /// Maximum characters returned in this page.
    pub limit: usize,
    /// Total character count of the full output.
    pub total_chars: usize,
    /// Whether more data is available beyond this page.
    pub has_more: bool,
}

/// Truncate `output` at character boundaries, returning the page and pagination metadata.
///
/// Uses `chars().skip(offset).take(limit)` for Unicode safety.
pub fn truncate_with_pagination(
    output: &str,
    offset: usize,
    limit: usize,
) -> (String, PaginationInfo) {
    let total_chars = output.chars().count();
    let page: String = output.chars().skip(offset).take(limit).collect();
    let consumed = offset + page.chars().count();
    let info = PaginationInfo {
        offset,
        limit,
        total_chars,
        has_more: consumed < total_chars,
    };
    (page, info)
}

pub(crate) const DANGEROUS_SHELL_PATTERNS: &[&str] = &[
    "/dev/tcp/",
    "/dev/udp/",
    "| bash -i",
    "| sh -i",
    "mkfifo /tmp",
];

pub(crate) fn find_dangerous_shell_pattern(command: &str) -> Option<&'static str> {
    let normalized = command
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    DANGEROUS_SHELL_PATTERNS
        .iter()
        .find(|pattern| normalized.contains(**pattern))
        .copied()
}

pub(crate) fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() <= max_len {
        return output.to_string();
    }

    let mut end = max_len;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }

    format!(
        "{}... [truncated, {} total chars]",
        &output[..end],
        output.len()
    )
}

/// Validate tool arguments against the top-level JSON schema contract exposed to the model.
///
/// This is intentionally conservative: it enforces that object-shaped tool schemas
/// receive JSON objects and that all top-level `required` fields are present and
/// non-null before execution begins.
pub fn validate_tool_arguments_schema(tool_name: &str, schema: &Value, args: &Value) -> Result<()> {
    if schema.get("type").and_then(|v| v.as_str()) == Some("object") && !args.is_object() {
        anyhow::bail!(
            "Schema validation failed for tool '{}': expected JSON object arguments",
            tool_name
        );
    }

    let Some(required) = schema.get("required").and_then(|v| v.as_array()) else {
        return Ok(());
    };

    let Some(args_obj) = args.as_object() else {
        return Ok(());
    };

    let missing: Vec<&str> = required
        .iter()
        .filter_map(|value| value.as_str())
        .filter(|field| args_obj.get(*field).is_none_or(|value| value.is_null()))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "Schema validation failed for tool '{}': missing required field(s): {}",
            tool_name,
            missing.join(", ")
        );
    }
}

/// A capability the agent can invoke during task execution.
///
/// Each implementation represents a single action (e.g. reading a file, running
/// a shell command). The trait is object-safe and stored as `Box<dyn Tool>` in
/// the [`ToolRegistry`]. Tool definitions are serialised to the LLM via
/// [`ToolRegistry::definitions`] so the model knows what it can call.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique identifier used by the LLM to invoke this tool (e.g. `"file_read"`).
    fn name(&self) -> &str;
    /// Human-readable summary sent to the LLM as part of the tool definition.
    fn description(&self) -> &str;
    /// JSON Schema describing the expected argument object. Must include `"type": "object"`
    /// and a `"required"` array for mandatory fields; validated before execution by
    /// [`validate_tool_arguments_schema`].
    fn schema(&self) -> Value;
    /// Execute the tool with the given arguments and return a JSON result.
    /// The result is injected into the conversation as the tool's response.
    async fn execute(&self, args: Value) -> Result<Value>;

    /// Returns true if this tool only reads data, never modifies.
    ///
    /// Default implementation delegates to `metadata()`.
    /// Tools should override `metadata()` to provide custom safety information.
    fn is_readonly(&self) -> bool {
        self.metadata().read_only
    }

    /// Returns true if this tool can cause data loss (rm, overwrite, etc.)
    ///
    /// Default implementation delegates to `metadata()`.
    /// Tools should override `metadata()` to provide custom safety information.
    fn is_destructive(&self) -> bool {
        self.metadata().destructive
    }

    /// Returns the risk level for this tool.
    ///
    /// Default implementation delegates to `metadata()`.
    /// Tools should override `metadata()` to provide custom safety information.
    fn risk_level(&self) -> crate::safety::RiskLevel {
        self.metadata().risk_level
    }

    /// Returns the full safety metadata for this tool.
    ///
    /// Default implementation uses the tool name to look up metadata.
    /// Tools should override this to provide custom metadata.
    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::default_tool_metadata(self.name())
    }
}

// Implement Tool for Arc<dyn Tool> so we can store tools in Arc and still use them
#[async_trait]
impl Tool for Arc<dyn Tool> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn description(&self) -> &str {
        (**self).description()
    }

    fn schema(&self) -> Value {
        (**self).schema()
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        (**self).execute(args).await
    }
}

/// Metadata about a registered tool for search and categorization.
#[derive(Clone)]
pub struct ToolInfo {
    /// The tool instance
    pub tool: Arc<dyn Tool>,
    /// Whether this tool is critical (always available) or deferred
    pub is_critical: bool,
    /// Category for grouping (e.g., "git", "file", "container")
    pub category: String,
}

/// Name-keyed registry of available tools with support for deferred loading.
///
/// Critical tools are always included in the system prompt and immediately available.
/// Deferred tools are only included after being discovered via `tool_search`.
/// This reduces context window usage by 60-80%.
pub struct ToolRegistry {
    /// All registered tools (both critical and deferred)
    all_tools: HashMap<String, ToolInfo>,
    /// Set of tool names that are currently "activated" (available for use)
    /// This starts with critical tools and grows as tools are discovered.
    activated_tools: HashSet<String>,
}

/// List of critical tools that are always available.
/// These are the minimal tools needed for basic operations.
pub const CRITICAL_TOOLS: &[&str] = &[
    // File operations - essential for reading/writing files
    "file_read",
    "file_write",
    "file_edit",
    "file_delete",
    "directory_tree",
    // Shell execution - essential for running commands
    "shell_exec",
    // Search operations - essential for finding code
    "grep_search",
    "glob_find",
    // Tool search - essential for discovering deferred tools
    "tool_search",
];

impl ToolRegistry {
    /// Create a new registry pre-populated with all built-in tools.
    ///
    /// Tools created this way fall back to the process-global `SAFETY_CONFIG`
    /// (set by `init_safety_config()` during agent startup). If the global has
    /// not been initialized, they use `SafetyConfig::default()`.
    ///
    /// Prefer [`Self::with_safety_config`] for production use so each tool carries
    /// its own config and doesn't depend on ambient global state.
    pub fn new() -> Self {
        Self::with_safety_config(None)
    }

    /// Create a registry with default tools using the given safety config.
    ///
    /// When `safety_config` is `Some`, all file/git tools are initialized with
    /// per-instance configs instead of relying on the deprecated global fallback.
    pub fn with_safety_config(safety_config: Option<&crate::config::SafetyConfig>) -> Self {
        let mut registry = Self {
            all_tools: HashMap::new(),
            activated_tools: HashSet::new(),
        };

        // Register critical tools first (File operations)
        if let Some(cfg) = safety_config {
            registry.register_critical(FileRead::with_safety_config(cfg.clone()));
            registry.register_critical(FileWrite::with_safety_config(cfg.clone()));
            registry.register_critical(FileEdit::with_safety_config(cfg.clone()));
            registry.register_critical(FileDelete::with_safety_config(cfg.clone()));
            registry.register_critical(DirectoryTree::with_safety_config(cfg.clone()));
        } else {
            registry.register_critical(FileRead::new());
            registry.register_critical(FileWrite::new());
            registry.register_critical(FileEdit::new());
            registry.register_critical(FileDelete::new());
            registry.register_critical(DirectoryTree::new());
        }

        // Critical: Shell execution
        registry.register_critical(ShellExec);

        // Critical: Search operations
        registry.register_critical(GrepSearch);
        registry.register_critical(GlobFind);
        // SymbolSearch is deferred - less commonly used

        // Critical: ToolSearch - enables discovering deferred tools
        // Note: tool_search is handled specially by the agent dispatch
        registry.register_critical(tool_search::ToolSearchTool::placeholder());

        // Deferred: Git operations (can be discovered via tool_search)
        if let Some(cfg) = safety_config {
            registry.register_deferred(GitStatus::with_safety_config(cfg.clone()));
            registry.register_deferred(GitDiff::with_safety_config(cfg.clone()));
            registry.register_deferred(GitCommit::with_safety_config(cfg.clone()));
            registry.register_deferred(GitPush::with_safety_config(cfg.clone()));
            registry.register_deferred(GitCheckpoint::with_safety_config(cfg.clone()));
            registry.register_deferred(EnterWorktreeTool::with_safety_config(cfg.clone()));
            registry.register_deferred(ExitWorktreeTool::with_safety_config(cfg.clone()));
            registry.register_deferred(ListWorktreesTool::with_safety_config(cfg.clone()));
        } else {
            registry.register_deferred(GitStatus::new());
            registry.register_deferred(EnterWorktreeTool::new());
            registry.register_deferred(ExitWorktreeTool::new());
            registry.register_deferred(ListWorktreesTool::new());
            registry.register_deferred(GitDiff::new());
            registry.register_deferred(GitCommit::new());
            registry.register_deferred(GitPush::new());
            registry.register_deferred(GitCheckpoint::new());
        }

        // Deferred: Cargo/Build operations
        registry.register_deferred(CargoTest);
        registry.register_deferred(CargoCheck);
        registry.register_deferred(CargoClippy);
        registry.register_deferred(CargoFmt);

        // Deferred: System operations
        registry.register_deferred(PtyShellTool);

        // Deferred: Search operations
        registry.register_deferred(SymbolSearch);

        // Deferred: HTTP/Web operations
        registry.register_deferred(HttpRequest);

        // Deferred: Process management operations
        registry.register_deferred(ProcessStart);
        registry.register_deferred(ProcessStop);
        registry.register_deferred(ProcessList);
        registry.register_deferred(ProcessLogs);
        registry.register_deferred(ProcessRestart);
        registry.register_deferred(PortCheck);

        // Deferred: Package manager operations
        registry.register_deferred(NpmInstall);
        registry.register_deferred(NpmRun);
        registry.register_deferred(NpmScripts);
        registry.register_deferred(PipInstall);
        registry.register_deferred(PipList);
        registry.register_deferred(PipFreeze);
        registry.register_deferred(YarnInstall);

        // Deferred: Container operations (Docker/Podman)
        registry.register_deferred(ContainerRun);
        registry.register_deferred(ContainerStop);
        registry.register_deferred(ContainerList);
        registry.register_deferred(ContainerLogs);
        registry.register_deferred(ContainerExec);
        registry.register_deferred(ContainerBuild);
        registry.register_deferred(ContainerImages);
        registry.register_deferred(ContainerPull);
        registry.register_deferred(ContainerRemove);
        registry.register_deferred(ComposeUp);
        registry.register_deferred(ComposeDown);

        // Deferred: RadarCam integration tools
        registry.register_deferred(RadarCamStatus);
        registry.register_deferred(RadarCamFrame);
        registry.register_deferred(RadarCamControl);
        registry.register_deferred(RadarCamLogs);
        registry.register_deferred(RadarCamTest);
        registry.register_deferred(RadarCamIntrospect);

        // Deferred: Screen capture
        registry.register_deferred(ScreenCapture);

        // Deferred: Vision tools
        registry.register_deferred(VisionAnalyze);
        registry.register_deferred(VisionCompare);

        // Deferred: Browser automation
        registry.register_deferred(BrowserFetch);
        registry.register_deferred(BrowserScreenshot);
        registry.register_deferred(BrowserPdf);
        registry.register_deferred(BrowserEval);
        registry.register_deferred(BrowserLinks);

        // Deferred: Playwright page controller
        registry.register_deferred(PageControlTool::new());

        // Deferred: Knowledge graph
        registry.register_deferred(KnowledgeAdd);
        registry.register_deferred(KnowledgeAutoExtract);
        registry.register_deferred(KnowledgeRelate);
        registry.register_deferred(KnowledgeQuery);
        registry.register_deferred(KnowledgeStatsTool);
        registry.register_deferred(KnowledgeClear);
        registry.register_deferred(KnowledgeRemove);
        registry.register_deferred(KnowledgeExport);

        // Deferred: Computer control (mouse, keyboard, screen, window)
        registry.register_deferred(computer::ComputerMouseTool);
        registry.register_deferred(computer::ComputerKeyboardTool);
        registry.register_deferred(computer::ComputerScreenTool);
        registry.register_deferred(computer::ComputerWindowTool);

        // Deferred: LSP code intelligence tools
        let project_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let (lsp_goto, lsp_refs, lsp_syms, lsp_hover) = lsp_tools::create_lsp_tools(project_root);
        registry.register_deferred(lsp_goto);
        registry.register_deferred(lsp_refs);
        registry.register_deferred(lsp_syms);
        registry.register_deferred(lsp_hover);

        // Deferred: Code introspection tools for evolution
        registry.register_deferred(introspect::CodeIntrospect::new());
        registry.register_deferred(introspect::CodeQuery::new());
        registry.register_deferred(introspect::CodePlan::new());
        registry.register_deferred(introspect::CodeDiffPlan::new());

        // Deferred: Code metrics tool
        registry.register_deferred(code_metrics::CodeMetricsTool::new());

        // Deferred: Code map / context budget tools
        registry.register_deferred(codemap::CodeMapTool);
        registry.register_deferred(codemap::ContextBudgetTool);
        registry.register_deferred(codemap::ContextActionTool);

        registry
    }

    /// Register a critical tool that's always available.
    pub fn register_critical<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        let category = tool_search::categorize_tool(&name).to_string();
        self.all_tools.insert(
            name.clone(),
            ToolInfo {
                tool: Arc::new(tool),
                is_critical: true,
                category,
            },
        );
        self.activated_tools.insert(name);
    }

    /// Register a deferred tool that's only available after discovery.
    pub fn register_deferred<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        let category = tool_search::categorize_tool(&name).to_string();
        self.all_tools.insert(
            name.clone(),
            ToolInfo {
                tool: Arc::new(tool),
                is_critical: false,
                category,
            },
        );
    }

    /// Register a tool, replacing any existing tool with the same name.
    /// Defaults to deferred registration.
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.register_deferred(tool);
    }

    /// Look up a tool by name, returning `None` if not found.
    /// This checks all tools, not just activated ones.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.all_tools.get(name).map(|info| info.tool.as_ref())
    }

    /// Look up a tool by name, but only if it's been activated.
    pub fn get_activated(&self, name: &str) -> Option<&dyn Tool> {
        if self.activated_tools.contains(name) {
            self.all_tools.get(name).map(|info| info.tool.as_ref())
        } else {
            None
        }
    }

    /// Check if a tool is activated (available for use).
    pub fn is_activated(&self, name: &str) -> bool {
        self.activated_tools.contains(name)
    }

    /// Activate a tool by name, making it available for use.
    /// Returns true if the tool was found and activated, false otherwise.
    pub fn activate(&mut self, name: &str) -> bool {
        if self.all_tools.contains_key(name) {
            self.activated_tools.insert(name.to_string());
            true
        } else {
            false
        }
    }

    /// Activate multiple tools at once.
    pub fn activate_many(&mut self, names: &[&str]) {
        for name in names {
            self.activate(name);
        }
    }

    /// Return references to all registered tools (both critical and deferred).
    pub fn list(&self) -> Vec<&dyn Tool> {
        self.all_tools
            .values()
            .map(|info| info.tool.as_ref())
            .collect()
    }

    /// Return only the activated tools (available for use).
    pub fn list_activated(&self) -> Vec<&dyn Tool> {
        self.activated_tools
            .iter()
            .filter_map(|name| self.all_tools.get(name).map(|info| info.tool.as_ref()))
            .collect()
    }

    /// Return only the critical tools.
    pub fn list_critical(&self) -> Vec<&dyn Tool> {
        self.all_tools
            .values()
            .filter(|info| info.is_critical)
            .map(|info| info.tool.as_ref())
            .collect()
    }

    /// Return references to all deferred (not yet activated) tools.
    pub fn list_deferred(&self) -> Vec<&dyn Tool> {
        self.all_tools
            .values()
            .filter(|info| !info.is_critical && !self.activated_tools.contains(info.tool.name()))
            .map(|info| info.tool.as_ref())
            .collect()
    }

    /// Execute a tool by name with the given arguments.
    /// Only activated tools can be executed.
    pub async fn execute(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        let tool = self
            .get_activated(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown or inactive tool: {}", name))?;
        tool.execute(args).await
    }

    /// Execute any tool (including deferred ones) - for internal use.
    pub async fn execute_any(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let tool = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown tool: {}", name))?;
        tool.execute(args).await
    }

    /// Build API-compatible tool definitions for all activated tools.
    pub fn definitions(&self) -> Vec<crate::api::types::ToolDefinition> {
        self.list_activated()
            .into_iter()
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

    /// Build API-compatible tool definitions for critical tools only.
    /// Use this for the initial system prompt to reduce context window usage.
    pub fn critical_definitions(&self) -> Vec<crate::api::types::ToolDefinition> {
        self.list_critical()
            .into_iter()
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

    /// Search for tools by name or description.
    /// Returns up to `limit` matching tools.
    pub fn search(&self, query: &str, limit: usize) -> Vec<tool_search::ToolSearchResult> {
        let query_lower = query.to_lowercase();
        self.all_tools
            .values()
            .filter(|info| {
                let name_match = info.tool.name().to_lowercase().contains(&query_lower);
                let desc_match = info
                    .tool
                    .description()
                    .to_lowercase()
                    .contains(&query_lower);
                name_match || desc_match
            })
            .take(limit)
            .map(|info| tool_search::ToolSearchResult {
                name: info.tool.name().to_string(),
                description: info.tool.description().to_string(),
                schema: info.tool.schema(),
                is_critical: info.is_critical,
                category: info.category.clone(),
            })
            .collect()
    }

    /// Get the count of all registered tools.
    pub fn total_count(&self) -> usize {
        self.all_tools.len()
    }

    /// Get the count of activated tools.
    pub fn activated_count(&self) -> usize {
        self.activated_tools.len()
    }

    /// Get the count of critical tools.
    pub fn critical_count(&self) -> usize {
        self.all_tools
            .values()
            .filter(|info| info.is_critical)
            .count()
    }

    /// Get info about a tool by name.
    pub fn get_info(&self, name: &str) -> Option<&ToolInfo> {
        self.all_tools.get(name)
    }

    /// Return only read-only tools (tools that don't modify files or state).
    /// This is used in plan mode to restrict available tools.
    pub fn filter_by_readonly(&self) -> Vec<&dyn Tool> {
        self.list_activated()
            .into_iter()
            .filter(|tool| tool.is_readonly())
            .collect()
    }

    /// Return tool definitions for read-only tools only.
    /// Used when building API tool definitions in plan mode.
    pub fn readonly_definitions(&self) -> Vec<crate::api::types::ToolDefinition> {
        self.filter_by_readonly()
            .into_iter()
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
    fn test_schema_validator_rejects_non_object_args() {
        let registry = ToolRegistry::new();
        let tool = registry.get("shell_exec").unwrap();

        let err =
            validate_tool_arguments_schema(tool.name(), &tool.schema(), &serde_json::json!("ls"))
                .unwrap_err()
                .to_string();
        assert!(err.contains("expected JSON object arguments"));
    }

    #[test]
    fn test_schema_validator_rejects_missing_required_fields() {
        let registry = ToolRegistry::new();
        let tool = registry.get("process_start").unwrap();

        let err =
            validate_tool_arguments_schema(tool.name(), &tool.schema(), &serde_json::json!({}))
                .unwrap_err()
                .to_string();
        assert!(err.contains("missing required field(s): id, command"));
    }

    #[test]
    fn test_schema_validator_accepts_required_fields_present() {
        let registry = ToolRegistry::new();
        let tool = registry.get("file_write").unwrap();

        let args = serde_json::json!({
            "path": "/tmp/example.txt",
            "content": "hello"
        });
        validate_tool_arguments_schema(tool.name(), &tool.schema(), &args).unwrap();
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
    fn test_git_worktree_tools_registered() {
        let registry = ToolRegistry::new();

        assert!(registry.get("enter_worktree").is_some());
        assert!(registry.get("exit_worktree").is_some());
        assert!(registry.get("list_worktrees").is_some());
    }

    #[test]
    fn test_git_worktree_tools_deferred() {
        let registry = ToolRegistry::new();

        // Worktree tools should exist but not be activated initially
        assert!(registry.get("enter_worktree").is_some());
        assert!(registry.get("exit_worktree").is_some());
        assert!(registry.get("list_worktrees").is_some());
        assert!(!registry.is_activated("enter_worktree"));
        assert!(!registry.is_activated("exit_worktree"));
        assert!(!registry.is_activated("list_worktrees"));
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

    // ---- Pagination tests ----

    #[test]
    fn test_truncate_with_pagination_full() {
        let (page, info) = truncate_with_pagination("hello", 0, 100);
        assert_eq!(page, "hello");
        assert_eq!(info.total_chars, 5);
        assert!(!info.has_more);
        assert_eq!(info.offset, 0);
    }

    #[test]
    fn test_truncate_with_pagination_truncated() {
        let (page, info) = truncate_with_pagination("hello world", 0, 5);
        assert_eq!(page, "hello");
        assert!(info.has_more);
        assert_eq!(info.total_chars, 11);
    }

    #[test]
    fn test_truncate_with_pagination_offset() {
        let (page, info) = truncate_with_pagination("hello world", 6, 100);
        assert_eq!(page, "world");
        assert!(!info.has_more);
        assert_eq!(info.offset, 6);
    }

    #[test]
    fn test_truncate_with_pagination_unicode() {
        let input = "héllo wörld";
        let (page, info) = truncate_with_pagination(input, 0, 5);
        assert_eq!(page, "héllo");
        assert!(info.has_more);
    }

    #[test]
    fn test_truncate_with_pagination_empty() {
        let (page, info) = truncate_with_pagination("", 0, 100);
        assert_eq!(page, "");
        assert_eq!(info.total_chars, 0);
        assert!(!info.has_more);
    }

    #[test]
    fn test_truncate_with_pagination_offset_beyond() {
        let (page, info) = truncate_with_pagination("hello", 100, 10);
        assert_eq!(page, "");
        assert!(!info.has_more);
    }

    // ---- Deferred tool loading tests ----

    #[test]
    fn test_critical_tools_are_activated() {
        let registry = ToolRegistry::new();

        // Critical tools should be activated by default
        for tool_name in CRITICAL_TOOLS {
            assert!(
                registry.is_activated(tool_name),
                "Critical tool {} should be activated",
                tool_name
            );
        }
    }

    #[test]
    fn test_tool_search_is_critical() {
        let registry = ToolRegistry::new();
        assert!(registry.is_activated("tool_search"));
    }

    #[test]
    fn test_git_tools_deferred() {
        let mut registry = ToolRegistry::new();

        // Git tools should exist but not be activated initially
        assert!(registry.get("git_status").is_some());
        assert!(!registry.is_activated("git_status"));

        // Activate and check
        assert!(registry.activate("git_status"));
        assert!(registry.is_activated("git_status"));
    }

    #[test]
    fn test_list_critical_vs_list_activated() {
        let registry = ToolRegistry::new();

        let critical = registry.list_critical();
        let activated = registry.list_activated();

        // Initially, activated should equal critical
        assert_eq!(critical.len(), activated.len());

        // But total tools should be more
        assert!(registry.total_count() > critical.len());
    }

    #[test]
    fn test_search_and_activate() {
        let mut registry = ToolRegistry::new();

        // Search for git tools
        let results = registry.search("git", 10);
        assert!(!results.is_empty());

        // Activate git tools
        for result in &results {
            if !result.is_critical {
                registry.activate(&result.name);
            }
        }

        // Now git_status should be activated
        assert!(registry.is_activated("git_status"));
    }

    #[test]
    fn test_definitions_returns_activated_only() {
        let mut registry = ToolRegistry::new();

        let initial_count = registry.definitions().len();
        assert_eq!(initial_count, registry.activated_count());

        // Activate a deferred tool
        registry.activate("cargo_test");

        // Definitions should now include the activated tool
        let new_count = registry.definitions().len();
        assert_eq!(new_count, initial_count + 1);
    }

    #[test]
    fn test_critical_definitions_count() {
        let registry = ToolRegistry::new();

        let critical_defs = registry.critical_definitions();
        let all_defs = registry.definitions();

        // Initially, critical_definitions should equal definitions
        assert_eq!(critical_defs.len(), all_defs.len());
        assert_eq!(critical_defs.len(), CRITICAL_TOOLS.len());
    }

    #[test]
    fn test_cargo_tools_deferred() {
        let registry = ToolRegistry::new();

        // Cargo tools should exist but not be activated
        assert!(registry.get("cargo_test").is_some());
        assert!(!registry.is_activated("cargo_test"));
        assert!(!registry.is_activated("cargo_check"));
    }

    #[test]
    fn test_container_tools_deferred() {
        let registry = ToolRegistry::new();

        // Container tools should exist but not be activated
        assert!(registry.get("container_run").is_some());
        assert!(!registry.is_activated("container_run"));
    }

    // =========================================================================
    // Tool Metadata Tests
    // =========================================================================

    #[test]
    fn test_file_read_is_readonly() {
        let tool = FileRead::new();
        assert!(tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
        assert!(!tool.is_destructive());
    }

    #[test]
    fn test_file_write_is_not_readonly() {
        let tool = FileWrite::new();
        assert!(!tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Medium);
        assert!(!tool.is_destructive());
    }

    #[test]
    fn test_file_delete_is_destructive() {
        let tool = FileDelete::new();
        assert!(!tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::High);
        assert!(tool.is_destructive());
    }

    #[test]
    fn test_shell_exec_is_high_risk() {
        let tool = ShellExec;
        assert!(!tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::High);
        assert!(tool.is_destructive());
    }

    #[test]
    fn test_directory_tree_is_readonly() {
        let tool = DirectoryTree::new();
        assert!(tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
    }

    #[test]
    fn test_grep_search_is_readonly() {
        use crate::tools::search::GrepSearch;
        let tool = GrepSearch;
        assert!(tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
    }

    #[test]
    fn test_git_status_is_readonly() {
        use crate::tools::git::GitStatus;
        let tool = GitStatus::new();
        assert!(tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::Low);
    }

    #[test]
    fn test_git_push_is_high_risk() {
        use crate::tools::git::GitPush;
        let tool = GitPush::new();
        assert!(!tool.is_readonly());
        assert_eq!(tool.risk_level(), crate::safety::RiskLevel::High);
    }

    #[test]
    fn test_tool_metadata_via_registry() {
        let registry = ToolRegistry::new();

        // Test that we can get metadata for registered tools
        let file_read = registry.get("file_read").unwrap();
        assert!(file_read.is_readonly());
        assert_eq!(file_read.risk_level(), crate::safety::RiskLevel::Low);

        let file_write = registry.get("file_write").unwrap();
        assert!(!file_write.is_readonly());
        assert_eq!(file_write.risk_level(), crate::safety::RiskLevel::Medium);

        let file_delete = registry.get("file_delete").unwrap();
        assert!(file_delete.is_destructive());
        assert_eq!(file_delete.risk_level(), crate::safety::RiskLevel::High);

        let shell_exec = registry.get("shell_exec").unwrap();
        assert!(!shell_exec.is_readonly());
        assert_eq!(shell_exec.risk_level(), crate::safety::RiskLevel::High);
        assert!(shell_exec.is_destructive());
    }
}
