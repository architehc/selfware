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

        // Browser operations
        "browser_fetch" | "browser_links" => ToolMetadata::network(),
        // browser_screenshot / browser_pdf ALWAYS write a destination file
        // (output_path defaults to `.selfware/browser-output/…` even when
        // omitted) — they are write-capable, not read-only; the network()
        // classification labelled them as reads and let them ride through
        // the MCP write gate without the opt-in (2026-09-21 review finding).
        // Risk stays Medium so the Normal-mode confirmation UX is unchanged.
        "browser_screenshot" | "browser_pdf" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, true, false)
        }
        // browser_eval executes arbitrary JS inside the page — it can mutate
        // DOM, storage, navigation and remote state — so it is NOT read-only.
        // The network() classification used here before silently classed it as
        // read-only, so the MCP write gate (keyed on read_only) let it execute
        // without the operator opt-in (2026-09-21 review finding). Risk stays
        // Medium so the Normal-mode confirmation UX is unchanged.
        "browser_eval" => ToolMetadata::custom(false, false, RiskLevel::Medium, true, false),

        // Process management
        "process_list" | "port_check" => ToolMetadata::read_only(),
        "process_start" | "process_stop" | "process_restart" | "process_logs" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, false, false)
        }

        // Package managers
        "npm_install" | "npm_run" | "pip_install" | "yarn_install" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, true, false)
        }
        "npm_scripts" | "pip_list" => ToolMetadata::read_only(),
        // pip_freeze can WRITE: its optional `output_file` argument dumps the
        // requirements list to disk. read_only() classified it as a pure read,
        // so the MCP write gate (keyed on read_only) let a freeze-to-file ride
        // through without the operator opt-in (2026-09-21 review finding).
        "pip_freeze" => ToolMetadata::file_write(),

        // Knowledge graph
        "knowledge_query" | "knowledge_stats" => ToolMetadata::read_only(),
        // knowledge_export's whole purpose is writing `output_path` to disk —
        // same write class as file_write (2026-09-21 review finding: it sat
        // in the read-only group and bypassed the MCP write gate).
        "knowledge_export" => ToolMetadata::file_write(),
        "knowledge_add" | "knowledge_relate" | "knowledge_clear" | "knowledge_remove" => {
            ToolMetadata::file_write()
        }
        "knowledge_auto_extract" => ToolMetadata::read_only(),

        // Computer control (high risk)
        "computer_mouse" | "computer_keyboard" | "computer_screen" | "computer_window" => {
            ToolMetadata::custom(false, false, RiskLevel::High, false, false)
        }

        // Screen capture — reads the screen but SENDS the image to a remote
        // model endpoint (network egress of potentially private content):
        // not read-only in the confirmation sense.
        "screen_capture" => ToolMetadata::custom(false, false, RiskLevel::Medium, true, false),

        // Vision tools — upload file bytes to the model endpoint (egress);
        // must confirm like any network tool.
        "vision_analyze" | "vision_compare" => {
            ToolMetadata::custom(false, false, RiskLevel::Medium, true, false)
        }

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

        // Evolve graph queries — read-only views over the cached graph.
        "graph_summary" | "context_pack" | "hotspots" | "impact" | "neighbors" | "test_map"
        | "cycles" | "dups" => ToolMetadata::read_only(),

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

        // Context-window management tools (dispatched outside the registry,
        // see `tools::context`). They read workspace files into / reshape the
        // agent's own context — no workspace, network or process effect.
        // `context_bulk_read` / `context_load_skeleton` resolve every path
        // through the same PathValidator as file_read before any I/O. Being
        // unlisted made them prompt in Normal mode like a write (0.9.1 field
        // report: `context_bulk_read` asked "Execute?" on every call).
        "context_status"
        | "context_focus"
        | "context_evict"
        | "context_recommend"
        | "context_load_skeleton"
        | "context_bulk_read"
        | "context_summary" => ToolMetadata::read_only(),

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
///
/// This variant is ARGUMENT-BLIND and therefore conservative: tools whose
/// safety depends on their arguments (the cargo verification tools) prompt.
/// The dispatcher uses [`normal_mode_call_needs_confirmation`], which sees
/// the arguments.
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

/// The dedicated verification tools Normal mode runs without asking, when
/// their arguments are the plain, non-writing form.
///
/// These run a fixed cargo subcommand in the workspace root (never a shell)
/// and write only build artefacts under `target/`:
///
/// - `cargo_check` — always (its arguments are booleans only);
/// - `cargo_clippy` — only without `fix: true` (`--fix` rewrites sources);
/// - `cargo_test` — the tests themselves are code in the workspace, which in
///   Normal mode only changed through confirmed edits.
///
/// Every string argument (`package`, `test_name`) must not start with `-`:
/// the tools pass them as positional cargo arguments, so a value such as
/// `--config=target.x.runner="sh -c …"` or `--manifest-path=/elsewhere` would
/// otherwise become a cargo FLAG. Such a call is not auto-allowed — it
/// prompts. `cargo_fmt` (rewrites sources) is never auto-allowed here.
///
/// Build scripts and proc macros run as part of any cargo build; like the
/// tests they come from the workspace/dependency set the operator controls.
pub fn verification_call_auto_allowed(tool_name: &str, args: &Value) -> bool {
    if !matches!(tool_name, "cargo_check" | "cargo_clippy" | "cargo_test") {
        return false;
    }
    if cargo_call_injects_flags(tool_name, args) {
        return false;
    }
    let Some(map) = args.as_object() else {
        return false;
    };
    // `fix` must be absent or literally false — `--fix` rewrites sources.
    tool_name != "cargo_clippy"
        || matches!(
            map.get("fix"),
            None | Some(Value::Null) | Some(Value::Bool(false))
        )
}

/// Whether a `cargo_*` tool call's arguments are NOT the plain form: not a
/// JSON object, a nested value, or a string that starts with `-`.
///
/// The cargo tools pass `package` / `test_name` positionally, so a leading
/// `-` turns the value into a cargo FLAG — `--config=target.<triple>.runner=…`
/// runs an arbitrary program, `--manifest-path=` builds another tree. Such a
/// call must be confirmed in every mode that otherwise auto-approves cargo
/// tools (Normal's plain-verification allowance, AutoEdit's list).
/// Non-cargo tools return `false`.
pub fn cargo_call_injects_flags(tool_name: &str, args: &Value) -> bool {
    if !matches!(
        tool_name,
        "cargo_check" | "cargo_clippy" | "cargo_test" | "cargo_fmt"
    ) {
        return false;
    }
    let Some(map) = args.as_object() else {
        return true;
    };
    !map.values().all(|v| match v {
        Value::String(s) => !s.trim_start().starts_with('-'),
        Value::Bool(_) | Value::Null | Value::Number(_) => true,
        _ => false,
    })
}

/// Normal-mode confirmation decision for one concrete call (arguments known).
///
/// Order:
/// 1. a session/config permission grant for the tool → no prompt;
/// 2. a session shell rule (`p` at the prompt) matching this `shell_exec`
///    call → no prompt (see [`crate::safety::permissions::ShellAllowRule`]);
/// 3. tools listed in `safety.require_confirmation` → prompt;
/// 4. plain-form verification calls ([`verification_call_auto_allowed`])
///    → no prompt;
/// 5. otherwise the metadata rule: explicitly classified read-only +
///    Low-risk tools run, everything else (writes, shell, network, installs,
///    git mutations, unclassified tools) prompts.
pub fn normal_mode_call_needs_confirmation(
    tool_name: &str,
    args: &Value,
    require_confirmation: &[String],
    grants: &crate::safety::permissions::PermissionStore,
) -> bool {
    if grants.is_authorized(tool_name, None) {
        return false;
    }
    if grants.shell_rule_allows(tool_name, args) {
        return false;
    }
    if require_confirmation.iter().any(|t| t == tool_name) {
        return true;
    }
    if verification_call_auto_allowed(tool_name, args) {
        return false;
    }
    normal_mode_needs_confirmation(tool_name, require_confirmation, grants)
}

/// [`normal_mode_call_needs_confirmation`] without the plain-verification
/// allowance ([`verification_call_auto_allowed`]): for headless Normal runs,
/// where a build/test cannot be approved by anyone and must stop the run.
/// Grants and shell rules still apply.
pub fn normal_mode_call_needs_confirmation_without_verification(
    tool_name: &str,
    args: &Value,
    require_confirmation: &[String],
    grants: &crate::safety::permissions::PermissionStore,
) -> bool {
    if grants.is_authorized(tool_name, None) || grants.shell_rule_allows(tool_name, args) {
        return false;
    }
    if verification_call_auto_allowed(tool_name, args) {
        return true;
    }
    normal_mode_call_needs_confirmation(tool_name, args, require_confirmation, grants)
}

#[cfg(test)]
#[path = "../../tests/unit/safety/tool_metadata/tool_metadata_test.rs"]
mod tests;
