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
pub mod clarify;
pub mod code_metrics;
pub mod codemap;
pub mod computer;
pub mod container;
pub mod context;
pub mod file;
pub mod fim;
pub mod git;
pub mod git_worktree;
pub mod grep_search;
#[cfg(feature = "hot-reload")]
pub mod hot_reload;
pub mod http;
pub mod introspect;
pub mod knowledge;
pub mod localize_issue;
pub mod lsp_tools;
pub mod net_policy;
pub mod package;
pub mod page_controller;
pub mod patch_apply;
pub mod process;
pub mod prompt;
pub mod pty_shell;
pub mod screen_capture;
pub mod search;
pub mod shell_exec;
pub mod task_focus;
pub mod tool_search;
pub mod vision;

use browser::{BrowserEval, BrowserFetch, BrowserLinks, BrowserPdf, BrowserScreenshot};
use cargo::{CargoCheck, CargoClippy, CargoFmt, CargoTest};
use container::{
    ComposeDown, ComposeUp, ContainerBuild, ContainerExec, ContainerImages, ContainerList,
    ContainerLogs, ContainerPull, ContainerRemove, ContainerRun, ContainerStop,
};
use file::{DirectoryTree, FileDelete, FileEdit, FileMultiEdit, FileRead, FileWrite};
use git::{GitCheckpoint, GitCommit, GitDiff, GitPush, GitStatus};
use git_worktree::{EnterWorktreeTool, ExitWorktreeTool, ListWorktreesTool};
use grep_search::GrepSearch;
use http::HttpRequest;
use knowledge::{
    KnowledgeAdd, KnowledgeAutoExtract, KnowledgeClear, KnowledgeExport, KnowledgeQuery,
    KnowledgeRelate, KnowledgeRemove, KnowledgeStats as KnowledgeStatsTool,
};
use localize_issue::LocalizeIssue;
use package::{NpmInstall, NpmRun, NpmScripts, PipFreeze, PipInstall, PipList, YarnInstall};
use page_controller::PageControlTool;
use patch_apply::PatchApply;
use process::{PortCheck, ProcessList, ProcessLogs, ProcessRestart, ProcessStart, ProcessStop};
use pty_shell::PtyShellTool;
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
    "| bash",
    "| sh",
    "bash -i",
    "sh -i",
    "exec bash -i",
    "exec sh -i",
    "mkfifo /tmp",
    "rm -rf /",
    "rm -rf ~",
    "rm -rf *",
    ":(){ :|:& };:",
    "> /dev/sda",
    "dd if=/dev/zero of=/dev/sda",
    "chmod -R 777 /",
    "chown -R 0:0 /",
];

pub(crate) fn find_dangerous_shell_pattern(command: &str) -> Option<&'static str> {
    let normalized = command
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    DANGEROUS_SHELL_PATTERNS
        .iter()
        .find(|pattern| {
            if pattern.contains("sh") {
                // Patterns containing "sh"/"bash" as a command word require
                // word-boundary checking to avoid false positives on commands
                // like "shellcheck", "sha256sum", or "shuf".
                pattern_matches_sh_word(&normalized, pattern)
            } else {
                normalized.contains(**pattern)
            }
        })
        .copied()
}

/// Check if a sh/bash pattern matches with word-boundary awareness.
///
/// The "sh" or "bash" token must be followed by end-of-string or a shell
/// delimiter character (whitespace, ; & | < > ( )) to count as a match.
/// This prevents false positives where "sh" is merely a prefix of a longer
/// word like "shellcheck", "sha256sum", or "shuf".
fn pattern_matches_sh_word(normalized: &str, pattern: &str) -> bool {
    // Find the offset and length of "bash" or "sh" within the pattern.
    // Check "bash" first since "bash" contains "sh" as a substring.
    let (sh_offset, sh_len) = pattern
        .find("bash")
        .map(|p| (p, 4))
        .or_else(|| pattern.find("sh").map(|p| (p, 2)))
        .expect("sh/bash pattern must contain 'sh' or 'bash'");

    let mut search_from = 0;
    while let Some(pos) = normalized[search_from..].find(pattern) {
        let abs_pos = search_from + pos;
        let after_sh = abs_pos + sh_offset + sh_len;
        let is_boundary = after_sh >= normalized.len()
            || normalized[after_sh..]
                .starts_with(|c: char| c.is_whitespace() || ";&|<>()".contains(c));
        if is_boundary {
            return true;
        }
        search_from = abs_pos + 1;
    }
    false
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
    /// Shared index backing the `tool_search` tool. Populated after registration
    /// so the search tool can return real results instead of the placeholder.
    /// Uses a std::sync::RwLock because the index is updated from sync
    /// registry methods that may be called inside an async runtime.
    tool_search_index: Arc<std::sync::RwLock<Vec<tool_search::ToolSearchResult>>>,
}

/// List of critical tools that are always available.
/// These are the minimal tools needed for basic operations.
pub const CRITICAL_TOOLS: &[&str] = &[
    // File operations - essential for reading/writing files
    "file_read",
    "file_write",
    "file_edit",
    "file_multi_edit",
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
        let tool_search_index = Arc::new(std::sync::RwLock::new(Vec::new()));
        let mut registry = Self {
            all_tools: HashMap::new(),
            activated_tools: HashSet::new(),
            tool_search_index: Arc::clone(&tool_search_index),
        };

        // Register critical tools first (File operations)
        registry.register_critical(tool_search::ToolSearchTool::new(tool_search_index));
        if let Some(cfg) = safety_config {
            registry.register_critical(FileRead::with_safety_config(cfg.clone()));
            registry.register_critical(FileWrite::with_safety_config(cfg.clone()));
            registry.register_critical(FileEdit::with_safety_config(cfg.clone()));
            registry.register_critical(FileMultiEdit::with_safety_config(cfg.clone()));
            registry.register_critical(FileDelete::with_safety_config(cfg.clone()));
            registry.register_critical(DirectoryTree::with_safety_config(cfg.clone()));
        } else {
            registry.register_critical(FileRead::new());
            registry.register_critical(FileWrite::new());
            registry.register_critical(FileEdit::new());
            registry.register_critical(FileMultiEdit::new());
            registry.register_critical(FileDelete::new());
            registry.register_critical(DirectoryTree::new());
        }

        // Critical: Shell execution
        registry.register_critical(ShellExec);

        // Critical: Search operations
        registry.register_critical(GrepSearch);
        registry.register_critical(GlobFind);
        // SymbolSearch is deferred - less commonly used

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

        // Deferred: Issue localization
        registry.register_deferred(LocalizeIssue);

        // Deferred: LSP code intelligence tools
        let project_root =
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let (lsp_goto, lsp_refs, lsp_syms, lsp_hover) =
            lsp_tools::create_lsp_tools(project_root.clone(), safety_config.cloned());
        registry.register_deferred(lsp_goto);
        registry.register_deferred(lsp_refs);
        registry.register_deferred(lsp_syms);
        registry.register_deferred(lsp_hover);

        let (lsp_diag, lsp_ws, lsp_impl) =
            lsp_tools::create_extra_lsp_tools(project_root, safety_config.cloned());
        registry.register_deferred(lsp_diag);
        registry.register_deferred(lsp_ws);
        registry.register_deferred(lsp_impl);

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

        // Deferred: Patch apply tool
        registry.register_deferred(PatchApply);

        // Deferred: Hot-reload tool for dynamic plugin loading
        #[cfg(feature = "hot-reload")]
        registry.register_deferred(hot_reload::HotReloadTool::new());

        // Deferred: Clarification tool (ask_user) — rate-limited, user-disableable,
        // safe in headless/TUI mode. Registered here with default enabled=true so
        // the tool is always discoverable; the agent re-registers it in
        // Agent::new() with the actual `config.ui.allow_clarification` value.
        registry.register_deferred(clarify::ClarificationTool::default());

        registry.rebuild_search_index();
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

    /// Rebuild the index used by the `tool_search` tool from the current set
    /// of registered tools. Call this after registering any tools that should
    /// be discoverable.
    pub fn rebuild_search_index(&mut self) {
        let mut index = self
            .tool_search_index
            .write()
            .expect("tool_search index poisoned");
        index.clear();
        index.extend(
            self.all_tools
                .values()
                .map(|info| tool_search::ToolSearchResult {
                    name: info.tool.name().to_string(),
                    description: info.tool.description().to_string(),
                    schema: info.tool.schema(),
                    is_critical: info.is_critical,
                    category: info.category.clone(),
                }),
        );
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

    /// Return only read-only tools (tools that don't modify files or state).
    /// This is used in plan mode to restrict available tools.
    pub fn filter_by_readonly(&self) -> Vec<&dyn Tool> {
        self.list_activated()
            .into_iter()
            .filter(|tool| tool.is_readonly())
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/mod_test.rs"]
mod tests;
