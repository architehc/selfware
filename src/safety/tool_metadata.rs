//! Tool safety metadata and permission checking infrastructure.
//!
//! This module provides:
//! - `RiskLevel`: Classification of tool operation risk
//! - `ToolMetadata`: Safety metadata for tools
//! - `PermissionChecker`: Centralized permission checking based on execution mode
//!
//! The design follows the principle that tools declare their own safety properties,
//! and a centralized checker uses those declarations along with the execution mode
//! to make permission decisions.

use serde_json::Value;

/// Risk level for tool operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    /// Safe read-only operations (file reads, searches)
    Low,
    /// Potentially modifying but generally safe (file writes, git operations)
    Medium,
    /// Dangerous operations that can cause data loss or system changes
    /// (file deletions, shell commands, network calls)
    High,
}

impl RiskLevel {
    /// Get the risk level as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
        }
    }

    /// Get ANSI color code for terminal output
    pub fn color_code(&self) -> &'static str {
        match self {
            RiskLevel::Low => "\x1b[32m",    // Green
            RiskLevel::Medium => "\x1b[33m", // Yellow
            RiskLevel::High => "\x1b[31m",   // Red
        }
    }

    /// Reset ANSI color
    pub fn reset_color(&self) -> &'static str {
        "\x1b[0m"
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Execution mode for the agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum ExecutionMode {
    /// Normal mode: prompt for medium and high risk operations
    #[default]
    Normal,
    /// Plan mode: only allow read-only operations
    Plan,
    /// Auto mode: auto-approve low and medium risk, prompt for high
    Auto,
    /// YOLO mode: auto-approve all operations (with audit logging)
    Yolo,
}

impl ExecutionMode {
    /// Get the execution mode as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionMode::Normal => "normal",
            ExecutionMode::Plan => "plan",
            ExecutionMode::Auto => "auto",
            ExecutionMode::Yolo => "yolo",
        }
    }
}

impl std::fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result of a permission check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    /// Operation is allowed
    Allow,
    /// Operation is denied
    Deny { reason: String },
    /// User should be prompted for confirmation
    Prompt { reason: String },
}

/// Safety metadata for a tool
///
/// This struct provides a standardized way to describe tool safety properties
/// without requiring methods on the tool trait that need input values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolMetadata {
    /// Whether the tool only reads data (never modifies)
    pub read_only: bool,
    /// Whether the tool can cause data loss (file deletion, overwrite, etc.)
    pub destructive: bool,
    /// The risk level for this tool
    pub risk_level: RiskLevel,
    /// Whether this tool accesses the network
    pub network_access: bool,
    /// Whether this tool executes shell commands
    pub shell_execution: bool,
}

impl ToolMetadata {
    /// Create metadata for a read-only tool (file_read, grep_search, etc.)
    pub fn read_only() -> Self {
        Self {
            read_only: true,
            destructive: false,
            risk_level: RiskLevel::Low,
            network_access: false,
            shell_execution: false,
        }
    }

    /// Create metadata for a file write tool
    pub fn file_write() -> Self {
        Self {
            read_only: false,
            destructive: false,
            risk_level: RiskLevel::Medium,
            network_access: false,
            shell_execution: false,
        }
    }

    /// Create metadata for a destructive file operation (file_delete)
    pub fn file_destructive() -> Self {
        Self {
            read_only: false,
            destructive: true,
            risk_level: RiskLevel::High,
            network_access: false,
            shell_execution: false,
        }
    }

    /// Create metadata for a shell execution tool
    pub fn shell() -> Self {
        Self {
            read_only: false,
            destructive: true,
            risk_level: RiskLevel::High,
            network_access: false,
            shell_execution: true,
        }
    }

    /// Create metadata for a network tool
    pub fn network() -> Self {
        Self {
            read_only: true,
            destructive: false,
            risk_level: RiskLevel::Medium,
            network_access: true,
            shell_execution: false,
        }
    }

    /// Create metadata for a git tool
    pub fn git() -> Self {
        Self {
            read_only: false,
            destructive: false,
            risk_level: RiskLevel::Medium,
            network_access: false,
            shell_execution: false,
        }
    }

    /// Create custom metadata
    pub fn custom(
        read_only: bool,
        destructive: bool,
        risk_level: RiskLevel,
        network_access: bool,
        shell_execution: bool,
    ) -> Self {
        Self {
            read_only,
            destructive,
            risk_level,
            network_access,
            shell_execution,
        }
    }
}

/// Permission checker that uses tool metadata and execution mode to make decisions
#[derive(Debug, Clone)]
pub struct PermissionChecker {
    mode: ExecutionMode,
    /// Whether to allow destructive operations in YOLO mode
    allow_destructive_in_yolo: bool,
    /// Whether to allow network operations
    allow_network: bool,
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new(ExecutionMode::Normal)
    }
}

impl PermissionChecker {
    /// Create a new permission checker with the given execution mode
    pub fn new(mode: ExecutionMode) -> Self {
        Self {
            mode,
            allow_destructive_in_yolo: false,
            allow_network: true,
        }
    }

    /// Set whether to allow destructive operations in YOLO mode
    pub fn with_destructive_in_yolo(mut self, allow: bool) -> Self {
        self.allow_destructive_in_yolo = allow;
        self
    }

    /// Set whether to allow network operations
    pub fn with_network(mut self, allow: bool) -> Self {
        self.allow_network = allow;
        self
    }

    /// Get the current execution mode
    pub fn mode(&self) -> ExecutionMode {
        self.mode
    }

    /// Check if a tool operation is permitted
    ///
    /// This is the main entry point for permission checking. It uses the tool's
    /// metadata and the current execution mode to decide whether to allow,
    /// deny, or prompt for the operation.
    pub fn check(
        &self,
        tool_name: &str,
        metadata: &ToolMetadata,
        _input: &Value,
    ) -> PermissionResult {
        match self.mode {
            ExecutionMode::Plan => self.check_plan_mode(metadata),
            ExecutionMode::Normal => self.check_normal_mode(metadata),
            ExecutionMode::Auto => self.check_auto_mode(tool_name, metadata),
            ExecutionMode::Yolo => self.check_yolo_mode(tool_name, metadata),
        }
    }

    /// Plan mode: only allow read-only operations
    fn check_plan_mode(&self, metadata: &ToolMetadata) -> PermissionResult {
        if metadata.read_only {
            PermissionResult::Allow
        } else {
            PermissionResult::Deny {
                reason: format!(
                    "Plan mode only allows read-only operations. '{}' is a modifying tool.",
                    if metadata.destructive {
                        "destructive"
                    } else {
                        "modifying"
                    }
                ),
            }
        }
    }

    /// Normal mode: prompt for medium and high risk
    fn check_normal_mode(&self, metadata: &ToolMetadata) -> PermissionResult {
        match metadata.risk_level {
            RiskLevel::Low => PermissionResult::Allow,
            RiskLevel::Medium => PermissionResult::Prompt {
                reason: "This operation may modify files or state".to_string(),
            },
            RiskLevel::High => PermissionResult::Prompt {
                reason: if metadata.destructive {
                    "This is a destructive operation that may cause data loss".to_string()
                } else if metadata.shell_execution {
                    "This executes a shell command which may be dangerous".to_string()
                } else {
                    "This is a high-risk operation".to_string()
                },
            },
        }
    }

    /// Auto mode: auto-approve low and medium, prompt for high
    fn check_auto_mode(&self, tool_name: &str, metadata: &ToolMetadata) -> PermissionResult {
        match metadata.risk_level {
            RiskLevel::Low | RiskLevel::Medium => PermissionResult::Allow,
            RiskLevel::High => {
                // Special cases that always need confirmation in auto mode
                if metadata.destructive && is_protected_tool(tool_name) {
                    PermissionResult::Prompt {
                        reason: format!(
                            "{} is a destructive operation that requires confirmation",
                            tool_name
                        ),
                    }
                } else {
                    PermissionResult::Allow
                }
            }
        }
    }

    /// YOLO mode: auto-approve everything (with some safety checks)
    fn check_yolo_mode(&self, _tool_name: &str, metadata: &ToolMetadata) -> PermissionResult {
        // Block network if not allowed
        if metadata.network_access && !self.allow_network {
            return PermissionResult::Prompt {
                reason: "Network operations require confirmation".to_string(),
            };
        }

        // Block destructive if not explicitly allowed
        if metadata.destructive && !self.allow_destructive_in_yolo {
            return PermissionResult::Prompt {
                reason: "Destructive operations require confirmation in YOLO mode".to_string(),
            };
        }

        PermissionResult::Allow
    }

    /// Check if a tool is read-only (convenience method)
    pub fn is_read_only(&self, metadata: &ToolMetadata) -> bool {
        metadata.read_only
    }

    /// Get the risk level for a tool (convenience method)
    pub fn risk_level(&self, metadata: &ToolMetadata) -> RiskLevel {
        metadata.risk_level
    }
}

/// Check if a tool is in the protected list (always requires confirmation for destructive ops)
fn is_protected_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "file_delete" | "shell_exec" | "container_remove" | "compose_down"
    )
}

/// Explicit safety classification for a tool, or `None` if the tool name is not
/// listed. `default_tool_metadata` wraps this with a permissive fallback; the
/// parity test asserts every REGISTERED tool is explicitly listed here so a new
/// tool cannot silently inherit an under-specified (permissive) default.
pub fn classify_tool_metadata(tool_name: &str) -> Option<ToolMetadata> {
    let meta = match tool_name {
        // Read-only file operations
        "file_read" | "directory_tree" => ToolMetadata::read_only(),

        // File write operations
        "file_write" | "file_edit" => ToolMetadata::file_write(),

        // Destructive file operations
        "file_delete" => ToolMetadata::file_destructive(),

        // Search operations (read-only)
        "grep_search" | "glob_find" | "symbol_search" | "tool_search" => ToolMetadata::read_only(),

        // Shell execution
        "shell_exec" | "pty_shell" => ToolMetadata::shell(),

        // Git operations
        "git_status" | "git_diff" => ToolMetadata::read_only(),
        "git_commit" | "git_push" | "git_checkpoint" => ToolMetadata::git(),

        // Cargo operations (safe but modify state)
        "cargo_test" | "cargo_check" | "cargo_clippy" | "cargo_fmt" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, false, false)
        }

        // Network operations
        "http_request" => ToolMetadata::network(),

        // Container operations
        "container_run" | "container_exec" => ToolMetadata::shell(),
        "container_list" | "container_logs" | "container_images" => ToolMetadata::read_only(),
        "container_stop" | "container_remove" | "compose_up" | "compose_down" => {
            ToolMetadata::custom(false, true, RiskLevel::High, false, false)
        }

        // Browser operations (network + potentially destructive)
        "browser_fetch" | "browser_screenshot" | "browser_pdf" | "browser_eval"
        | "browser_links" => ToolMetadata::network(),

        // Process management
        "process_list" | "port_check" => ToolMetadata::read_only(),
        "process_start" | "process_stop" | "process_restart" | "process_logs" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, false, false)
        }

        // Package managers
        "npm_install" | "npm_run" | "pip_install" | "yarn_install" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, true, false)
        }
        "npm_scripts" | "pip_list" | "pip_freeze" => ToolMetadata::read_only(),

        // Knowledge graph
        "knowledge_query" | "knowledge_stats" | "knowledge_export" => ToolMetadata::read_only(),
        "knowledge_add" | "knowledge_relate" | "knowledge_clear" | "knowledge_remove" => {
            ToolMetadata::file_write()
        }
        "knowledge_auto_extract" => ToolMetadata::read_only(),

        // Computer control (high risk)
        "computer_mouse" | "computer_keyboard" | "computer_screen" | "computer_window" => {
            ToolMetadata::custom(false, false, RiskLevel::High, false, false)
        }

        // Screen capture
        "screen_capture" => ToolMetadata::read_only(),

        // Vision tools
        "vision_analyze" | "vision_compare" => ToolMetadata::read_only(),

        // LSP tools
        "lsp_goto" | "lsp_references" | "lsp_symbols" | "lsp_hover" => ToolMetadata::read_only(),

        // Introspection tools
        "code_introspect" | "code_query" | "code_plan" | "code_diff_plan" => {
            ToolMetadata::read_only()
        }

        // Code metrics
        "code_metrics" => ToolMetadata::read_only(),

        // Code map
        "code_map" | "context_budget" | "context_action" => ToolMetadata::read_only(),

        // Additional file editors — these MUTATE files (found missing by the
        // registry↔safety parity test; the permissive default under-classified
        // them as non-mutating).
        "file_multi_edit" | "patch_apply" => ToolMetadata::file_write(),

        // LSP queries — read-only (diagnostics/navigation, no mutation).
        "lsp_diagnostics"
        | "lsp_goto_definition"
        | "lsp_goto_implementation"
        | "lsp_find_references"
        | "lsp_document_symbols"
        | "lsp_workspace_symbols" => ToolMetadata::read_only(),

        // Issue localization + user prompt — read-only, no side effects.
        "localize_issue" | "ask_user" => ToolMetadata::read_only(),

        // Browser page navigation.
        "page_control" => ToolMetadata::network(),

        // Container build/pull — state-changing.
        "container_build" => ToolMetadata::shell(),
        "container_pull" => ToolMetadata::network(),

        // Worktree management.
        "list_worktrees" => ToolMetadata::read_only(),
        "enter_worktree" | "exit_worktree" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, false, false)
        }

        // Hot reload — state change.
        "hot_reload" => ToolMetadata::custom(false, false, RiskLevel::Medium, false, false),

        // Unlisted tool: signal that no explicit classification exists.
        _ => return None,
    };
    Some(meta)
}

/// Safety metadata for a tool, falling back to a conservative default for any
/// name not explicitly classified. Prefer `classify_tool_metadata` when you
/// need to know whether the tool was explicitly listed.
pub fn default_tool_metadata(tool_name: &str) -> ToolMetadata {
    classify_tool_metadata(tool_name)
        .unwrap_or_else(|| ToolMetadata::custom(false, false, RiskLevel::Medium, false, false))
}

/// Normal-mode confirmation decision driven by tool metadata.
///
/// Historically the interactive Normal-mode prompt consulted a hardcoded
/// ~8-tool safe list, so harmless read-only tools (`lsp_diagnostics`,
/// `process_list`, even `ask_user`) demanded confirmation — pushing users
/// toward `yolo`. This routes the decision through the explicit tool
/// classification instead:
///
/// 1. A session permission grant (`PermissionGrant::session`, the "always
///    allow" prompt option) skips the prompt.
/// 2. Tools named in `safety.require_confirmation` always prompt.
/// 3. Explicitly classified read-only + Low-risk tools never prompt.
/// 4. Everything else (Medium/High risk: writes, shell, network, ...) prompts.
///
/// Unclassified tools (e.g. dynamic `mcp_*` names) keep the old behavior:
/// they prompt.
pub fn normal_mode_needs_confirmation(
    tool_name: &str,
    require_confirmation: &[String],
    grants: &crate::safety::permissions::PermissionStore,
) -> bool {
    if grants.is_authorized(tool_name, None) {
        return false;
    }
    if require_confirmation.iter().any(|t| t == tool_name) {
        return true;
    }
    match classify_tool_metadata(tool_name) {
        Some(meta) => !(meta.read_only && meta.risk_level == RiskLevel::Low),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
    }

    #[test]
    fn test_execution_mode_display() {
        assert_eq!(ExecutionMode::Normal.to_string(), "normal");
        assert_eq!(ExecutionMode::Plan.to_string(), "plan");
        assert_eq!(ExecutionMode::Auto.to_string(), "auto");
        assert_eq!(ExecutionMode::Yolo.to_string(), "yolo");
    }

    #[test]
    fn test_permission_checker_plan_mode() {
        let checker = PermissionChecker::new(ExecutionMode::Plan);
        let read_meta = ToolMetadata::read_only();
        let write_meta = ToolMetadata::file_write();

        assert_eq!(
            checker.check("file_read", &read_meta, &Value::Null),
            PermissionResult::Allow
        );
        assert!(matches!(
            checker.check("file_write", &write_meta, &Value::Null),
            PermissionResult::Deny { .. }
        ));
    }

    #[test]
    fn test_permission_checker_normal_mode() {
        let checker = PermissionChecker::new(ExecutionMode::Normal);
        let read_meta = ToolMetadata::read_only();
        let write_meta = ToolMetadata::file_write();
        let destructive_meta = ToolMetadata::file_destructive();

        assert!(matches!(
            checker.check("file_read", &read_meta, &Value::Null),
            PermissionResult::Allow
        ));
        assert!(matches!(
            checker.check("file_write", &write_meta, &Value::Null),
            PermissionResult::Prompt { .. }
        ));
        assert!(matches!(
            checker.check("file_delete", &destructive_meta, &Value::Null),
            PermissionResult::Prompt { .. }
        ));
    }

    #[test]
    fn test_permission_checker_auto_mode() {
        let checker = PermissionChecker::new(ExecutionMode::Auto);
        let read_meta = ToolMetadata::read_only();
        let write_meta = ToolMetadata::file_write();

        assert!(matches!(
            checker.check("file_read", &read_meta, &Value::Null),
            PermissionResult::Allow
        ));
        assert!(matches!(
            checker.check("file_write", &write_meta, &Value::Null),
            PermissionResult::Allow
        ));
    }

    #[test]
    fn test_permission_checker_yolo_mode() {
        let checker = PermissionChecker::new(ExecutionMode::Yolo);
        let read_meta = ToolMetadata::read_only();

        assert!(matches!(
            checker.check("file_read", &read_meta, &Value::Null),
            PermissionResult::Allow
        ));
    }

    #[test]
    fn test_shell_meta_prompts_in_auto_mode() {
        // A shell tool executes commands and must require confirmation in Auto mode.
        let checker = PermissionChecker::new(ExecutionMode::Auto);
        let shell_meta = ToolMetadata::shell();
        let result = checker.check("shell_exec", &shell_meta, &Value::Null);
        assert!(
            matches!(result, PermissionResult::Prompt { .. }),
            "a shell tool should prompt in Auto mode, got {:?}",
            result
        );
    }

    #[test]
    fn test_medium_risk_auto_approved_in_auto_mode() {
        // A Medium-risk, non-destructive tool should be auto-approved in Auto mode.
        let checker = PermissionChecker::new(ExecutionMode::Auto);
        let meta = ToolMetadata::custom(false, false, RiskLevel::Medium, true, false);
        let result = checker.check("http_request", &meta, &Value::Null);
        assert_eq!(result, PermissionResult::Allow);
    }

    #[test]
    fn test_default_tool_metadata() {
        assert!(default_tool_metadata("file_read").read_only);
        assert!(!default_tool_metadata("file_write").read_only);
        assert!(default_tool_metadata("file_delete").destructive);
        assert_eq!(
            default_tool_metadata("shell_exec").risk_level,
            RiskLevel::High
        );
        assert!(default_tool_metadata("tool_search").read_only);
        assert_eq!(
            default_tool_metadata("tool_search").risk_level,
            RiskLevel::Low
        );
    }

    // ===== normal_mode_needs_confirmation tests (P1-5) =====

    fn no_grants() -> crate::safety::permissions::PermissionStore {
        crate::safety::permissions::PermissionStore::new()
    }

    #[test]
    fn test_normal_mode_read_only_tools_skip_confirmation() {
        // The old hardcoded safe list prompted for these harmless read-only
        // tools; metadata classification must now let them through.
        for tool in [
            // previously safe-listed
            "file_read",
            "directory_tree",
            "glob_find",
            "grep_search",
            "symbol_search",
            "tool_search",
            "git_status",
            "git_diff",
            // the P1-5 report's examples
            "lsp_diagnostics",
            "process_list",
            "ask_user",
            // other metadata-classified read-only tools
            "port_check",
            "lsp_hover",
            "code_metrics",
            "list_worktrees",
            "knowledge_query",
            "vision_analyze",
        ] {
            assert!(
                !normal_mode_needs_confirmation(tool, &[], &no_grants()),
                "read-only tool '{}' must not prompt in Normal mode",
                tool
            );
        }
    }

    #[test]
    fn test_normal_mode_mutating_tools_still_prompt() {
        for tool in [
            "file_write",
            "file_edit",
            "file_multi_edit",
            "patch_apply",
            "file_delete",
            "shell_exec",
            "pty_shell",
            "git_commit",
            "git_push",
            "cargo_test",
        ] {
            assert!(
                normal_mode_needs_confirmation(tool, &[], &no_grants()),
                "mutating tool '{}' must still prompt in Normal mode",
                tool
            );
        }
    }

    #[test]
    fn test_normal_mode_network_tools_still_prompt() {
        // Network tools are read_only=true but Medium risk — they must keep
        // prompting (only Low-risk read-only tools are exempt).
        for tool in ["http_request", "browser_fetch", "page_control"] {
            assert!(
                normal_mode_needs_confirmation(tool, &[], &no_grants()),
                "network tool '{}' must still prompt in Normal mode",
                tool
            );
        }
    }

    #[test]
    fn test_normal_mode_unclassified_tool_prompts() {
        // Dynamic MCP tool names have no explicit classification — keep the
        // old prompt-by-default behavior.
        assert!(normal_mode_needs_confirmation(
            "mcp_server_thing",
            &[],
            &no_grants()
        ));
    }

    #[test]
    fn test_normal_mode_require_confirmation_overrides_metadata() {
        // An operator-listed read-only tool must still prompt when present
        // in safety.require_confirmation.
        let require = vec!["file_read".to_string()];
        assert!(normal_mode_needs_confirmation(
            "file_read",
            &require,
            &no_grants()
        ));
    }

    #[test]
    fn test_normal_mode_session_grant_skips_confirmation() {
        // The "always allow" prompt option records a session grant; a grant
        // must short-circuit even tools that would otherwise prompt.
        let mut store = crate::safety::permissions::PermissionStore::new();
        store.add(crate::safety::permissions::PermissionGrant::session(
            "shell_exec",
        ));
        assert!(!normal_mode_needs_confirmation("shell_exec", &[], &store));
        // The grant is tool-scoped: other tools are unaffected.
        assert!(normal_mode_needs_confirmation("file_write", &[], &store));
    }

    #[test]
    fn test_tool_metadata_builder() {
        let meta = ToolMetadata::custom(true, false, RiskLevel::Low, true, false);
        assert!(meta.read_only);
        assert!(!meta.destructive);
        assert_eq!(meta.risk_level, RiskLevel::Low);
        assert!(meta.network_access);
        assert!(!meta.shell_execution);
    }
}
