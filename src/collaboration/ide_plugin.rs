//! IDE Plugin Framework
//!
//! Integration framework for IDE plugins:
//! - VS Code, Neovim, JetBrains plugins
//! - LSP integration
//! - Inline suggestions
//! - Command palette
//! - Status indicators

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counters for unique IDs
static COMMAND_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static SUGGESTION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static STATUS_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_command_id() -> String {
    format!(
        "cmd_{}_{:x}",
        COMMAND_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_suggestion_id() -> String {
    format!(
        "sug_{}_{:x}",
        SUGGESTION_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_status_id() -> String {
    format!(
        "status_{}_{:x}",
        STATUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// IDE Types
// ============================================================================

/// Supported IDE types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdeType {
    VsCode,
    Neovim,
    Vim,
    IntelliJ,
    PyCharm,
    WebStorm,
    Rider,
    Clion,
    GoLand,
    RustRover,
    Emacs,
    SublimeText,
    Atom,
    Zed,
    Helix,
    Other,
}

impl IdeType {
    pub fn as_str(&self) -> &str {
        match self {
            IdeType::VsCode => "vscode",
            IdeType::Neovim => "neovim",
            IdeType::Vim => "vim",
            IdeType::IntelliJ => "intellij",
            IdeType::PyCharm => "pycharm",
            IdeType::WebStorm => "webstorm",
            IdeType::Rider => "rider",
            IdeType::Clion => "clion",
            IdeType::GoLand => "goland",
            IdeType::RustRover => "rustrover",
            IdeType::Emacs => "emacs",
            IdeType::SublimeText => "sublime",
            IdeType::Atom => "atom",
            IdeType::Zed => "zed",
            IdeType::Helix => "helix",
            IdeType::Other => "other",
        }
    }

    /// Check if IDE supports LSP
    pub fn supports_lsp(&self) -> bool {
        matches!(
            self,
            IdeType::VsCode
                | IdeType::Neovim
                | IdeType::Vim
                | IdeType::IntelliJ
                | IdeType::PyCharm
                | IdeType::WebStorm
                | IdeType::Rider
                | IdeType::Clion
                | IdeType::GoLand
                | IdeType::RustRover
                | IdeType::Emacs
                | IdeType::SublimeText
                | IdeType::Zed
                | IdeType::Helix
        )
    }

    /// Check if this is a JetBrains IDE
    pub fn is_jetbrains(&self) -> bool {
        matches!(
            self,
            IdeType::IntelliJ
                | IdeType::PyCharm
                | IdeType::WebStorm
                | IdeType::Rider
                | IdeType::Clion
                | IdeType::GoLand
                | IdeType::RustRover
        )
    }
}

/// Plugin capability
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    /// Inline code completions
    InlineCompletion,
    /// Ghost text suggestions
    GhostText,
    /// Code actions (quick fixes)
    CodeActions,
    /// Diagnostics (errors, warnings)
    Diagnostics,
    /// Hover information
    Hover,
    /// Go to definition
    GoToDefinition,
    /// Find references
    FindReferences,
    /// Rename symbol
    Rename,
    /// Code formatting
    Formatting,
    /// Signature help
    SignatureHelp,
    /// Document symbols
    DocumentSymbols,
    /// Workspace symbols
    WorkspaceSymbols,
    /// Semantic tokens
    SemanticTokens,
    /// Inlay hints
    InlayHints,
    /// Command palette
    CommandPalette,
    /// Status bar
    StatusBar,
    /// Chat interface
    Chat,
}

impl PluginCapability {
    pub fn as_str(&self) -> &str {
        match self {
            PluginCapability::InlineCompletion => "inline_completion",
            PluginCapability::GhostText => "ghost_text",
            PluginCapability::CodeActions => "code_actions",
            PluginCapability::Diagnostics => "diagnostics",
            PluginCapability::Hover => "hover",
            PluginCapability::GoToDefinition => "go_to_definition",
            PluginCapability::FindReferences => "find_references",
            PluginCapability::Rename => "rename",
            PluginCapability::Formatting => "formatting",
            PluginCapability::SignatureHelp => "signature_help",
            PluginCapability::DocumentSymbols => "document_symbols",
            PluginCapability::WorkspaceSymbols => "workspace_symbols",
            PluginCapability::SemanticTokens => "semantic_tokens",
            PluginCapability::InlayHints => "inlay_hints",
            PluginCapability::CommandPalette => "command_palette",
            PluginCapability::StatusBar => "status_bar",
            PluginCapability::Chat => "chat",
        }
    }
}

// ============================================================================
// Command Palette
// ============================================================================

/// Command category
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    /// AI assistance commands
    Ai,
    /// Code editing commands
    Edit,
    /// Navigation commands
    Navigate,
    /// Refactoring commands
    Refactor,
    /// Testing commands
    Test,
    /// Debug commands
    Debug,
    /// Git commands
    Git,
    /// Settings commands
    Settings,
    /// Custom category
    Custom(String),
}

impl CommandCategory {
    pub fn as_str(&self) -> &str {
        match self {
            CommandCategory::Ai => "AI",
            CommandCategory::Edit => "Edit",
            CommandCategory::Navigate => "Navigate",
            CommandCategory::Refactor => "Refactor",
            CommandCategory::Test => "Test",
            CommandCategory::Debug => "Debug",
            CommandCategory::Git => "Git",
            CommandCategory::Settings => "Settings",
            CommandCategory::Custom(s) => s.as_str(),
        }
    }
}

/// A command in the command palette
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// Command ID
    pub id: String,
    /// Command name/title
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Category
    pub category: CommandCategory,
    /// Keyboard shortcut
    pub keybinding: Option<String>,
    /// When clause (context when command is available)
    pub when: Option<String>,
    /// Icon
    pub icon: Option<String>,
    /// Is enabled
    pub enabled: bool,
}

impl PaletteCommand {
    pub fn new(name: impl Into<String>, category: CommandCategory) -> Self {
        Self {
            id: generate_command_id(),
            name: name.into(),
            description: None,
            category,
            keybinding: None,
            when: None,
            icon: None,
            enabled: true,
        }
    }

    /// Builder: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Builder: set keybinding
    pub fn with_keybinding(mut self, keybinding: impl Into<String>) -> Self {
        self.keybinding = Some(keybinding.into());
        self
    }

    /// Builder: set when clause
    pub fn when(mut self, condition: impl Into<String>) -> Self {
        self.when = Some(condition.into());
        self
    }

    /// Builder: set icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Enable/disable command
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

/// Command palette manager
#[derive(Debug, Default)]
pub struct CommandPalette {
    /// All commands
    commands: HashMap<String, PaletteCommand>,
    /// Index by category
    by_category: HashMap<CommandCategory, Vec<String>>,
    /// Recently used commands
    recent: Vec<String>,
    /// Max recent commands
    max_recent: usize,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
            by_category: HashMap::new(),
            recent: Vec::new(),
            max_recent: 10,
        }
    }

    /// Register a command
    pub fn register(&mut self, command: PaletteCommand) -> String {
        let id = command.id.clone();
        let category = command.category.clone();

        self.by_category
            .entry(category)
            .or_default()
            .push(id.clone());

        self.commands.insert(id.clone(), command);
        id
    }

    /// Unregister a command
    pub fn unregister(&mut self, id: &str) -> Option<PaletteCommand> {
        if let Some(cmd) = self.commands.remove(id) {
            if let Some(ids) = self.by_category.get_mut(&cmd.category) {
                ids.retain(|i| i != id);
            }
            Some(cmd)
        } else {
            None
        }
    }

    /// Get a command
    pub fn get(&self, id: &str) -> Option<&PaletteCommand> {
        self.commands.get(id)
    }

    /// Get commands by category
    pub fn by_category(&self, category: &CommandCategory) -> Vec<&PaletteCommand> {
        self.by_category
            .get(category)
            .map(|ids| ids.iter().filter_map(|id| self.commands.get(id)).collect())
            .unwrap_or_default()
    }

    /// Search commands
    pub fn search(&self, query: &str) -> Vec<&PaletteCommand> {
        let query_lower = query.to_lowercase();
        self.commands
            .values()
            .filter(|cmd| {
                cmd.name.to_lowercase().contains(&query_lower)
                    || cmd
                        .description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Execute a command (record in recent)
    pub fn execute(&mut self, id: &str) -> Option<&PaletteCommand> {
        if let Some(cmd) = self.commands.get(id) {
            if cmd.enabled {
                // Add to recent
                self.recent.retain(|i| i != id);
                self.recent.insert(0, id.to_string());
                self.recent.truncate(self.max_recent);
                return Some(cmd);
            }
        }
        None
    }

    /// Get recent commands
    pub fn recent(&self) -> Vec<&PaletteCommand> {
        self.recent
            .iter()
            .filter_map(|id| self.commands.get(id))
            .collect()
    }

    /// Get all enabled commands
    pub fn enabled(&self) -> Vec<&PaletteCommand> {
        self.commands.values().filter(|c| c.enabled).collect()
    }

    /// Get command count
    pub fn count(&self) -> usize {
        self.commands.len()
    }
}

// ============================================================================
// Inline Suggestions
// ============================================================================

/// Suggestion type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionType {
    /// Code completion
    Completion,
    /// Code fix
    Fix,
    /// Refactoring suggestion
    Refactor,
    /// Documentation suggestion
    Documentation,
    /// Test suggestion
    Test,
    /// Import suggestion
    Import,
}

impl SuggestionType {
    pub fn as_str(&self) -> &str {
        match self {
            SuggestionType::Completion => "completion",
            SuggestionType::Fix => "fix",
            SuggestionType::Refactor => "refactor",
            SuggestionType::Documentation => "documentation",
            SuggestionType::Test => "test",
            SuggestionType::Import => "import",
        }
    }
}

/// Text edit for a suggestion
#[derive(Debug, Clone)]
pub struct TextEdit {
    /// Start line (0-indexed)
    pub start_line: u32,
    /// Start column (0-indexed)
    pub start_column: u32,
    /// End line (0-indexed)
    pub end_line: u32,
    /// End column (0-indexed)
    pub end_column: u32,
    /// New text
    pub new_text: String,
}

impl TextEdit {
    pub fn new(
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        new_text: impl Into<String>,
    ) -> Self {
        Self {
            start_line,
            start_column,
            end_line,
            end_column,
            new_text: new_text.into(),
        }
    }

    /// Create an insert edit
    pub fn insert(line: u32, column: u32, text: impl Into<String>) -> Self {
        Self::new(line, column, line, column, text)
    }

    /// Create a replace edit for a range
    pub fn replace_range(
        start_line: u32,
        start_column: u32,
        end_line: u32,
        end_column: u32,
        text: impl Into<String>,
    ) -> Self {
        Self::new(start_line, start_column, end_line, end_column, text)
    }

    /// Check if this is a pure insertion
    pub fn is_insertion(&self) -> bool {
        self.start_line == self.end_line && self.start_column == self.end_column
    }

    /// Check if this is a deletion
    pub fn is_deletion(&self) -> bool {
        self.new_text.is_empty()
    }
}

/// Inline suggestion
#[derive(Debug, Clone)]
pub struct InlineSuggestion {
    /// Suggestion ID
    pub id: String,
    /// Suggestion type
    pub suggestion_type: SuggestionType,
    /// File path
    pub file: String,
    /// Text edits
    pub edits: Vec<TextEdit>,
    /// Preview text (displayed inline)
    pub preview: String,
    /// Full text (when expanded)
    pub full_text: Option<String>,
    /// Label
    pub label: String,
    /// Description
    pub description: Option<String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Is accepted
    pub accepted: bool,
    /// Is dismissed
    pub dismissed: bool,
    /// Created timestamp
    pub created_at: u64,
}

impl InlineSuggestion {
    pub fn new(
        suggestion_type: SuggestionType,
        file: impl Into<String>,
        preview: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_suggestion_id(),
            suggestion_type,
            file: file.into(),
            edits: Vec::new(),
            preview: preview.into(),
            full_text: None,
            label: label.into(),
            description: None,
            confidence: 1.0,
            accepted: false,
            dismissed: false,
            created_at: current_timestamp(),
        }
    }

    /// Builder: add edit
    pub fn with_edit(mut self, edit: TextEdit) -> Self {
        self.edits.push(edit);
        self
    }

    /// Builder: set full text
    pub fn with_full_text(mut self, text: impl Into<String>) -> Self {
        self.full_text = Some(text.into());
        self
    }

    /// Builder: set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Builder: set confidence
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Accept the suggestion
    pub fn accept(&mut self) {
        self.accepted = true;
        self.dismissed = false;
    }

    /// Dismiss the suggestion
    pub fn dismiss(&mut self) {
        self.dismissed = true;
        self.accepted = false;
    }

    /// Check if suggestion is pending (not accepted or dismissed)
    pub fn is_pending(&self) -> bool {
        !self.accepted && !self.dismissed
    }
}

/// Suggestion provider
#[derive(Debug, Default)]
pub struct SuggestionProvider {
    /// Active suggestions by file
    suggestions: HashMap<String, Vec<InlineSuggestion>>,
    /// Accepted count
    accepted_count: u64,
    /// Dismissed count
    dismissed_count: u64,
}

impl SuggestionProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a suggestion
    pub fn add(&mut self, suggestion: InlineSuggestion) -> String {
        let id = suggestion.id.clone();
        let file = suggestion.file.clone();

        self.suggestions.entry(file).or_default().push(suggestion);

        id
    }

    /// Get suggestions for a file
    pub fn for_file(&self, file: &str) -> Vec<&InlineSuggestion> {
        self.suggestions
            .get(file)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get pending suggestions for a file
    pub fn pending_for_file(&self, file: &str) -> Vec<&InlineSuggestion> {
        self.suggestions
            .get(file)
            .map(|v| v.iter().filter(|s| s.is_pending()).collect())
            .unwrap_or_default()
    }

    /// Accept a suggestion
    pub fn accept(&mut self, file: &str, id: &str) -> bool {
        if let Some(suggestions) = self.suggestions.get_mut(file) {
            if let Some(suggestion) = suggestions.iter_mut().find(|s| s.id == id) {
                suggestion.accept();
                self.accepted_count += 1;
                return true;
            }
        }
        false
    }

    /// Dismiss a suggestion
    pub fn dismiss(&mut self, file: &str, id: &str) -> bool {
        if let Some(suggestions) = self.suggestions.get_mut(file) {
            if let Some(suggestion) = suggestions.iter_mut().find(|s| s.id == id) {
                suggestion.dismiss();
                self.dismissed_count += 1;
                return true;
            }
        }
        false
    }

    /// Clear suggestions for a file
    pub fn clear_file(&mut self, file: &str) {
        self.suggestions.remove(file);
    }

    /// Get acceptance rate
    pub fn acceptance_rate(&self) -> f32 {
        let total = self.accepted_count + self.dismissed_count;
        if total == 0 {
            0.0
        } else {
            self.accepted_count as f32 / total as f32
        }
    }

    /// Get stats
    pub fn stats(&self) -> SuggestionStats {
        SuggestionStats {
            total_files: self.suggestions.len(),
            total_suggestions: self.suggestions.values().map(|v| v.len()).sum(),
            pending: self
                .suggestions
                .values()
                .flat_map(|v| v.iter())
                .filter(|s| s.is_pending())
                .count(),
            accepted: self.accepted_count,
            dismissed: self.dismissed_count,
            acceptance_rate: self.acceptance_rate(),
        }
    }
}

/// Suggestion statistics
#[derive(Debug, Clone)]
pub struct SuggestionStats {
    pub total_files: usize,
    pub total_suggestions: usize,
    pub pending: usize,
    pub accepted: u64,
    pub dismissed: u64,
    pub acceptance_rate: f32,
}

// ============================================================================
// Status Indicators
// ============================================================================

/// Status severity
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusSeverity {
    Info,
    Warning,
    Error,
    Success,
}

impl StatusSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            StatusSeverity::Info => "info",
            StatusSeverity::Warning => "warning",
            StatusSeverity::Error => "error",
            StatusSeverity::Success => "success",
        }
    }
}

/// Status bar position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusPosition {
    Left,
    #[default]
    Right,
    Center,
}

impl StatusPosition {
    pub fn as_str(&self) -> &str {
        match self {
            StatusPosition::Left => "left",
            StatusPosition::Right => "right",
            StatusPosition::Center => "center",
        }
    }
}

/// Status bar item
#[derive(Debug, Clone)]
pub struct StatusItem {
    /// Item ID
    pub id: String,
    /// Display text
    pub text: String,
    /// Tooltip
    pub tooltip: Option<String>,
    /// Icon
    pub icon: Option<String>,
    /// Severity
    pub severity: StatusSeverity,
    /// Position
    pub position: StatusPosition,
    /// Priority (higher = more important)
    pub priority: i32,
    /// Command to execute on click
    pub command: Option<String>,
    /// Is visible
    pub visible: bool,
    /// Is loading/busy
    pub busy: bool,
}

impl StatusItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            id: generate_status_id(),
            text: text.into(),
            tooltip: None,
            icon: None,
            severity: StatusSeverity::Info,
            position: StatusPosition::default(),
            priority: 0,
            command: None,
            visible: true,
            busy: false,
        }
    }

    /// Builder: set tooltip
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Builder: set icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Builder: set severity
    pub fn with_severity(mut self, severity: StatusSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Builder: set position
    pub fn at_position(mut self, position: StatusPosition) -> Self {
        self.position = position;
        self
    }

    /// Builder: set priority
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Builder: set command
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Update text
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Show/hide
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Set busy state
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }
}

/// Status bar manager
#[derive(Debug, Default)]
pub struct StatusBar {
    /// Status items
    items: HashMap<String, StatusItem>,
}

impl StatusBar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a status item
    pub fn add(&mut self, item: StatusItem) -> String {
        let id = item.id.clone();
        self.items.insert(id.clone(), item);
        id
    }

    /// Remove a status item
    pub fn remove(&mut self, id: &str) -> Option<StatusItem> {
        self.items.remove(id)
    }

    /// Get a status item
    pub fn get(&self, id: &str) -> Option<&StatusItem> {
        self.items.get(id)
    }

    /// Get a status item mutably
    pub fn get_mut(&mut self, id: &str) -> Option<&mut StatusItem> {
        self.items.get_mut(id)
    }

    /// Get visible items sorted by priority
    pub fn visible(&self) -> Vec<&StatusItem> {
        let mut items: Vec<_> = self.items.values().filter(|i| i.visible).collect();
        items.sort_by(|a, b| b.priority.cmp(&a.priority));
        items
    }

    /// Get items by position
    pub fn by_position(&self, position: StatusPosition) -> Vec<&StatusItem> {
        let mut items: Vec<_> = self
            .items
            .values()
            .filter(|i| i.visible && i.position == position)
            .collect();
        items.sort_by(|a, b| b.priority.cmp(&a.priority));
        items
    }

    /// Get item count
    pub fn count(&self) -> usize {
        self.items.len()
    }
}

// ============================================================================
// LSP Integration
// ============================================================================

/// LSP message type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspMessageType {
    Request,
    Response,
    Notification,
    Error,
}

impl LspMessageType {
    pub fn as_str(&self) -> &str {
        match self {
            LspMessageType::Request => "request",
            LspMessageType::Response => "response",
            LspMessageType::Notification => "notification",
            LspMessageType::Error => "error",
        }
    }
}

/// LSP method
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LspMethod {
    Initialize,
    Shutdown,
    TextDocumentDidOpen,
    TextDocumentDidClose,
    TextDocumentDidChange,
    TextDocumentCompletion,
    TextDocumentHover,
    TextDocumentDefinition,
    TextDocumentReferences,
    TextDocumentRename,
    TextDocumentFormatting,
    TextDocumentCodeAction,
    TextDocumentSignatureHelp,
    TextDocumentDocumentSymbol,
    WorkspaceSymbol,
    Custom(String),
}

impl LspMethod {
    pub fn as_str(&self) -> &str {
        match self {
            LspMethod::Initialize => "initialize",
            LspMethod::Shutdown => "shutdown",
            LspMethod::TextDocumentDidOpen => "textDocument/didOpen",
            LspMethod::TextDocumentDidClose => "textDocument/didClose",
            LspMethod::TextDocumentDidChange => "textDocument/didChange",
            LspMethod::TextDocumentCompletion => "textDocument/completion",
            LspMethod::TextDocumentHover => "textDocument/hover",
            LspMethod::TextDocumentDefinition => "textDocument/definition",
            LspMethod::TextDocumentReferences => "textDocument/references",
            LspMethod::TextDocumentRename => "textDocument/rename",
            LspMethod::TextDocumentFormatting => "textDocument/formatting",
            LspMethod::TextDocumentCodeAction => "textDocument/codeAction",
            LspMethod::TextDocumentSignatureHelp => "textDocument/signatureHelp",
            LspMethod::TextDocumentDocumentSymbol => "textDocument/documentSymbol",
            LspMethod::WorkspaceSymbol => "workspace/symbol",
            LspMethod::Custom(s) => s.as_str(),
        }
    }
}

/// LSP message
#[derive(Debug, Clone)]
pub struct LspMessage {
    /// Message ID
    pub id: Option<u64>,
    /// Message type
    pub message_type: LspMessageType,
    /// Method
    pub method: LspMethod,
    /// Params (JSON string)
    pub params: Option<String>,
    /// Result (JSON string)
    pub result: Option<String>,
    /// Error message
    pub error: Option<String>,
}

impl LspMessage {
    /// Create a request message
    pub fn request(id: u64, method: LspMethod, params: Option<String>) -> Self {
        Self {
            id: Some(id),
            message_type: LspMessageType::Request,
            method,
            params,
            result: None,
            error: None,
        }
    }

    /// Create a response message
    pub fn response(id: u64, result: Option<String>, error: Option<String>) -> Self {
        Self {
            id: Some(id),
            message_type: if error.is_some() {
                LspMessageType::Error
            } else {
                LspMessageType::Response
            },
            method: LspMethod::Custom("response".to_string()),
            params: None,
            result,
            error,
        }
    }

    /// Create a notification message
    pub fn notification(method: LspMethod, params: Option<String>) -> Self {
        Self {
            id: None,
            message_type: LspMessageType::Notification,
            method,
            params,
            result: None,
            error: None,
        }
    }

    /// Check if this is a request
    pub fn is_request(&self) -> bool {
        self.message_type == LspMessageType::Request
    }

    /// Check if this is a response
    pub fn is_response(&self) -> bool {
        self.message_type == LspMessageType::Response
    }

    /// Check if this is an error
    pub fn is_error(&self) -> bool {
        self.message_type == LspMessageType::Error
    }
}

/// LSP handler trait
pub trait LspHandler {
    /// Handle initialize request
    fn initialize(&mut self, params: &str) -> Result<String, String>;

    /// Handle shutdown request
    fn shutdown(&mut self) -> Result<(), String>;

    /// Handle completion request
    fn completion(&self, file: &str, line: u32, column: u32) -> Result<String, String>;

    /// Handle hover request
    fn hover(&self, file: &str, line: u32, column: u32) -> Result<Option<String>, String>;

    /// Handle go to definition request
    fn definition(&self, file: &str, line: u32, column: u32) -> Result<Option<String>, String>;
}

/// Mock LSP handler for testing
#[derive(Debug, Default)]
pub struct MockLspHandler {
    initialized: bool,
    capabilities: Vec<PluginCapability>,
}

impl MockLspHandler {
    pub fn new() -> Self {
        Self {
            initialized: false,
            capabilities: vec![
                PluginCapability::InlineCompletion,
                PluginCapability::Hover,
                PluginCapability::GoToDefinition,
                PluginCapability::CodeActions,
                PluginCapability::Diagnostics,
            ],
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn capabilities(&self) -> &[PluginCapability] {
        &self.capabilities
    }
}

impl LspHandler for MockLspHandler {
    fn initialize(&mut self, _params: &str) -> Result<String, String> {
        self.initialized = true;
        Ok(r#"{"capabilities":{}}"#.to_string())
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.initialized = false;
        Ok(())
    }

    fn completion(&self, file: &str, line: u32, column: u32) -> Result<String, String> {
        Ok(format!(
            r#"{{"items":[{{"label":"suggestion","detail":"at {}:{}:{}"}}]}}"#,
            file, line, column
        ))
    }

    fn hover(&self, file: &str, line: u32, column: u32) -> Result<Option<String>, String> {
        Ok(Some(format!("Hover info for {}:{}:{}", file, line, column)))
    }

    fn definition(&self, _file: &str, _line: u32, _column: u32) -> Result<Option<String>, String> {
        Ok(None)
    }
}

// ============================================================================
// Plugin Configuration
// ============================================================================

/// Plugin configuration
#[derive(Debug, Clone)]
pub struct PluginConfig {
    /// IDE type
    pub ide: IdeType,
    /// Enabled capabilities
    pub capabilities: Vec<PluginCapability>,
    /// Auto-suggest enabled
    pub auto_suggest: bool,
    /// Suggestion delay in milliseconds
    pub suggest_delay_ms: u32,
    /// Max suggestions to show
    pub max_suggestions: u32,
    /// Enable telemetry
    pub telemetry_enabled: bool,
    /// Custom settings
    pub settings: HashMap<String, String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            ide: IdeType::VsCode,
            capabilities: vec![
                PluginCapability::InlineCompletion,
                PluginCapability::GhostText,
                PluginCapability::CodeActions,
                PluginCapability::Diagnostics,
                PluginCapability::Hover,
                PluginCapability::CommandPalette,
                PluginCapability::StatusBar,
            ],
            auto_suggest: true,
            suggest_delay_ms: 300,
            max_suggestions: 5,
            telemetry_enabled: false,
            settings: HashMap::new(),
        }
    }
}

impl PluginConfig {
    pub fn new(ide: IdeType) -> Self {
        Self {
            ide,
            ..Default::default()
        }
    }

    /// Builder: add capability
    pub fn with_capability(mut self, capability: PluginCapability) -> Self {
        if !self.capabilities.contains(&capability) {
            self.capabilities.push(capability);
        }
        self
    }

    /// Builder: set auto-suggest
    pub fn auto_suggest(mut self, enabled: bool) -> Self {
        self.auto_suggest = enabled;
        self
    }

    /// Builder: set suggest delay
    pub fn suggest_delay(mut self, ms: u32) -> Self {
        self.suggest_delay_ms = ms;
        self
    }

    /// Builder: set max suggestions
    pub fn max_suggestions(mut self, max: u32) -> Self {
        self.max_suggestions = max;
        self
    }

    /// Builder: enable telemetry
    pub fn with_telemetry(mut self) -> Self {
        self.telemetry_enabled = true;
        self
    }

    /// Builder: add setting
    pub fn with_setting(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }

    /// Check if capability is enabled
    pub fn has_capability(&self, capability: PluginCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // IDE Type Tests

    #[test]
    fn test_ide_type_as_str() {
        assert_eq!(IdeType::VsCode.as_str(), "vscode");
        assert_eq!(IdeType::Neovim.as_str(), "neovim");
        assert_eq!(IdeType::IntelliJ.as_str(), "intellij");
    }

    #[test]
    fn test_ide_supports_lsp() {
        assert!(IdeType::VsCode.supports_lsp());
        assert!(IdeType::Neovim.supports_lsp());
        assert!(!IdeType::Other.supports_lsp());
    }

    #[test]
    fn test_ide_is_jetbrains() {
        assert!(IdeType::IntelliJ.is_jetbrains());
        assert!(IdeType::PyCharm.is_jetbrains());
        assert!(!IdeType::VsCode.is_jetbrains());
    }

    // Command Palette Tests

    #[test]
    fn test_palette_command_creation() {
        let cmd = PaletteCommand::new("Test Command", CommandCategory::Ai)
            .with_description("A test command")
            .with_keybinding("Ctrl+T");

        assert_eq!(cmd.name, "Test Command");
        assert_eq!(cmd.category, CommandCategory::Ai);
        assert!(cmd.description.is_some());
        assert!(cmd.keybinding.is_some());
        assert!(cmd.enabled);
    }

    #[test]
    fn test_command_palette_register() {
        let mut palette = CommandPalette::new();

        let cmd = PaletteCommand::new("Test", CommandCategory::Edit);
        let id = palette.register(cmd);

        assert_eq!(palette.count(), 1);
        assert!(palette.get(&id).is_some());
    }

    #[test]
    fn test_command_palette_search() {
        let mut palette = CommandPalette::new();

        palette.register(PaletteCommand::new("Generate Code", CommandCategory::Ai));
        palette.register(PaletteCommand::new("Format Code", CommandCategory::Edit));
        palette.register(PaletteCommand::new("Generate Tests", CommandCategory::Test));

        let results = palette.search("generate");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_command_palette_by_category() {
        let mut palette = CommandPalette::new();

        palette.register(PaletteCommand::new("AI Command 1", CommandCategory::Ai));
        palette.register(PaletteCommand::new("AI Command 2", CommandCategory::Ai));
        palette.register(PaletteCommand::new("Edit Command", CommandCategory::Edit));

        let ai_commands = palette.by_category(&CommandCategory::Ai);
        assert_eq!(ai_commands.len(), 2);
    }

    #[test]
    fn test_command_palette_execute() {
        let mut palette = CommandPalette::new();

        let cmd = PaletteCommand::new("Test", CommandCategory::Ai);
        let id = palette.register(cmd);

        let result = palette.execute(&id);
        assert!(result.is_some());

        let recent = palette.recent();
        assert_eq!(recent.len(), 1);
    }

    // Inline Suggestion Tests

    #[test]
    fn test_text_edit_creation() {
        let edit = TextEdit::new(0, 0, 0, 5, "Hello");
        assert_eq!(edit.start_line, 0);
        assert_eq!(edit.new_text, "Hello");
    }

    #[test]
    fn test_text_edit_insert() {
        let edit = TextEdit::insert(10, 5, "new text");
        assert!(edit.is_insertion());
        assert!(!edit.is_deletion());
    }

    #[test]
    fn test_inline_suggestion_creation() {
        let suggestion = InlineSuggestion::new(
            SuggestionType::Completion,
            "test.rs",
            "fn hello()",
            "Complete function",
        )
        .with_confidence(0.9);

        assert_eq!(suggestion.suggestion_type, SuggestionType::Completion);
        assert_eq!(suggestion.file, "test.rs");
        assert_eq!(suggestion.confidence, 0.9);
        assert!(suggestion.is_pending());
    }

    #[test]
    fn test_suggestion_accept_dismiss() {
        let mut suggestion =
            InlineSuggestion::new(SuggestionType::Fix, "test.rs", "fix", "Fix issue");

        assert!(suggestion.is_pending());

        suggestion.accept();
        assert!(suggestion.accepted);
        assert!(!suggestion.is_pending());

        suggestion.dismiss();
        assert!(suggestion.dismissed);
        assert!(!suggestion.accepted);
    }

    #[test]
    fn test_suggestion_provider_add() {
        let mut provider = SuggestionProvider::new();

        let suggestion =
            InlineSuggestion::new(SuggestionType::Completion, "test.rs", "preview", "label");

        provider.add(suggestion);
        let suggestions = provider.for_file("test.rs");
        assert_eq!(suggestions.len(), 1);
    }

    #[test]
    fn test_suggestion_provider_accept() {
        let mut provider = SuggestionProvider::new();

        let suggestion =
            InlineSuggestion::new(SuggestionType::Completion, "test.rs", "preview", "label");
        let id = suggestion.id.clone();

        provider.add(suggestion);
        assert!(provider.accept("test.rs", &id));

        let stats = provider.stats();
        assert_eq!(stats.accepted, 1);
    }

    // Status Bar Tests

    #[test]
    fn test_status_item_creation() {
        let item = StatusItem::new("Ready")
            .with_tooltip("AI is ready")
            .with_icon("$(check)")
            .with_severity(StatusSeverity::Success)
            .at_position(StatusPosition::Left);

        assert_eq!(item.text, "Ready");
        assert!(item.tooltip.is_some());
        assert!(item.visible);
    }

    #[test]
    fn test_status_bar_add() {
        let mut bar = StatusBar::new();

        let item = StatusItem::new("Test");
        let id = bar.add(item);

        assert_eq!(bar.count(), 1);
        assert!(bar.get(&id).is_some());
    }

    #[test]
    fn test_status_bar_visibility() {
        let mut bar = StatusBar::new();

        let visible = StatusItem::new("Visible");
        let mut hidden = StatusItem::new("Hidden");
        hidden.set_visible(false);

        bar.add(visible);
        bar.add(hidden);

        let visible_items = bar.visible();
        assert_eq!(visible_items.len(), 1);
    }

    #[test]
    fn test_status_bar_by_position() {
        let mut bar = StatusBar::new();

        bar.add(StatusItem::new("Left").at_position(StatusPosition::Left));
        bar.add(StatusItem::new("Right").at_position(StatusPosition::Right));

        let left = bar.by_position(StatusPosition::Left);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "Left");
    }

    // LSP Tests

    #[test]
    fn test_lsp_method_as_str() {
        assert_eq!(LspMethod::Initialize.as_str(), "initialize");
        assert_eq!(
            LspMethod::TextDocumentCompletion.as_str(),
            "textDocument/completion"
        );
    }

    #[test]
    fn test_lsp_message_request() {
        let msg = LspMessage::request(1, LspMethod::Initialize, Some("{}".to_string()));
        assert!(msg.is_request());
        assert_eq!(msg.id, Some(1));
    }

    #[test]
    fn test_lsp_message_response() {
        let msg = LspMessage::response(1, Some("{}".to_string()), None);
        assert!(msg.is_response());
        assert!(!msg.is_error());
    }

    #[test]
    fn test_lsp_message_error() {
        let msg = LspMessage::response(1, None, Some("Error".to_string()));
        assert!(msg.is_error());
    }

    #[test]
    fn test_mock_lsp_handler() {
        let mut handler = MockLspHandler::new();

        assert!(!handler.is_initialized());

        handler.initialize("{}").unwrap();
        assert!(handler.is_initialized());

        handler.shutdown().unwrap();
        assert!(!handler.is_initialized());
    }

    #[test]
    fn test_mock_lsp_completion() {
        let handler = MockLspHandler::new();
        let result = handler.completion("test.rs", 10, 5);
        assert!(result.is_ok());
    }

    // Plugin Config Tests

    #[test]
    fn test_plugin_config_default() {
        let config = PluginConfig::default();
        assert_eq!(config.ide, IdeType::VsCode);
        assert!(config.auto_suggest);
        assert!(!config.telemetry_enabled);
    }

    #[test]
    fn test_plugin_config_builder() {
        let config = PluginConfig::new(IdeType::Neovim)
            .with_capability(PluginCapability::Chat)
            .auto_suggest(false)
            .suggest_delay(500)
            .with_telemetry();

        assert_eq!(config.ide, IdeType::Neovim);
        assert!(config.has_capability(PluginCapability::Chat));
        assert!(!config.auto_suggest);
        assert_eq!(config.suggest_delay_ms, 500);
        assert!(config.telemetry_enabled);
    }

    #[test]
    fn test_plugin_config_has_capability() {
        let config = PluginConfig::default();
        assert!(config.has_capability(PluginCapability::InlineCompletion));
        assert!(!config.has_capability(PluginCapability::Chat));
    }

    // Unique ID Tests

    #[test]
    fn test_unique_command_ids() {
        let id1 = generate_command_id();
        let id2 = generate_command_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_suggestion_ids() {
        let id1 = generate_suggestion_id();
        let id2 = generate_suggestion_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_status_ids() {
        let id1 = generate_status_id();
        let id2 = generate_status_id();
        assert_ne!(id1, id2);
    }

    // Type String Tests

    #[test]
    fn test_capability_as_str() {
        assert_eq!(
            PluginCapability::InlineCompletion.as_str(),
            "inline_completion"
        );
        assert_eq!(PluginCapability::GhostText.as_str(), "ghost_text");
    }

    #[test]
    fn test_command_category_as_str() {
        assert_eq!(CommandCategory::Ai.as_str(), "AI");
        assert_eq!(CommandCategory::Refactor.as_str(), "Refactor");
    }

    #[test]
    fn test_suggestion_type_as_str() {
        assert_eq!(SuggestionType::Completion.as_str(), "completion");
        assert_eq!(SuggestionType::Fix.as_str(), "fix");
    }

    #[test]
    fn test_status_severity_as_str() {
        assert_eq!(StatusSeverity::Info.as_str(), "info");
        assert_eq!(StatusSeverity::Error.as_str(), "error");
    }

    #[test]
    fn test_status_position_as_str() {
        assert_eq!(StatusPosition::Left.as_str(), "left");
        assert_eq!(StatusPosition::Right.as_str(), "right");
    }

    #[test]
    fn test_lsp_message_type_as_str() {
        assert_eq!(LspMessageType::Request.as_str(), "request");
        assert_eq!(LspMessageType::Notification.as_str(), "notification");
    }

    #[test]
    fn test_text_edit_deletion() {
        let edit = TextEdit::new(0, 0, 0, 5, "");
        assert!(edit.is_deletion());
        assert!(!edit.is_insertion());
    }

    #[test]
    fn test_command_palette_unregister() {
        let mut palette = CommandPalette::new();

        let cmd = PaletteCommand::new("Test", CommandCategory::Ai);
        let id = palette.register(cmd);

        let removed = palette.unregister(&id);
        assert!(removed.is_some());
        assert_eq!(palette.count(), 0);
    }

    #[test]
    fn test_suggestion_provider_stats() {
        let mut provider = SuggestionProvider::new();

        let s1 = InlineSuggestion::new(SuggestionType::Completion, "a.rs", "a", "A");
        let s2 = InlineSuggestion::new(SuggestionType::Fix, "b.rs", "b", "B");
        let id1 = s1.id.clone();

        provider.add(s1);
        provider.add(s2);

        provider.accept("a.rs", &id1);

        let stats = provider.stats();
        assert_eq!(stats.total_files, 2);
        assert_eq!(stats.accepted, 1);
    }

    #[test]
    fn test_status_item_update() {
        let mut item = StatusItem::new("Initial");
        item.set_text("Updated");
        item.set_busy(true);

        assert_eq!(item.text, "Updated");
        assert!(item.busy);
    }
}
