//! Unified command registry — single source of truth for all slash commands.
//!
//! All command metadata (name, description, aliases, category) is defined here.
//! The completer, highlighter, help system, and input defaults all derive from this registry.

/// A registered slash command.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    /// The canonical command string (e.g., "/help", "/ctx load")
    pub name: &'static str,
    /// Short description for completion hints and help
    pub description: &'static str,
    /// Category for grouping in help output
    pub category: CommandCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    General,
    Context,
    Navigation,
    Git,
    Tools,
    Session,
    Display,
}

/// The complete command registry. Every slash command must be registered here.
pub static COMMANDS: &[CommandEntry] = &[
    // General
    CommandEntry {
        name: "/help",
        description: "Show help and available commands",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/status",
        description: "Show agent status and context usage",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/stats",
        description: "Show session statistics",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/mode",
        description: "Switch execution mode (normal/autoedit/yolo/daemon)",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/config",
        description: "Show current configuration",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/cost",
        description: "Show token usage and cost estimate",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/model",
        description: "Show or switch the current model",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/garden",
        description: "Show the garden status",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/journal",
        description: "Show the session journal",
        category: CommandCategory::General,
    },
    CommandEntry {
        name: "/palette",
        description: "Open the command palette",
        category: CommandCategory::General,
    },
    // Context
    CommandEntry {
        name: "/ctx",
        description: "Show context window usage",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/ctx clear",
        description: "Clear loaded context files",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/ctx load",
        description: "Load files into context",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/ctx reload",
        description: "Reload previously loaded context files",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/ctx copy",
        description: "Copy context to clipboard",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/context",
        description: "Show context window usage (alias for /ctx)",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/compress",
        description: "Compress context to free token budget",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/scan",
        description: "Index a folder/file for RAG semantic search",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/memory",
        description: "Show memory hierarchy status",
        category: CommandCategory::Context,
    },
    // Display
    CommandEntry {
        name: "/compact",
        description: "Compress context (auto mode)",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/compact micro",
        description: "Fast local compression (no API call)",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/compact auto",
        description: "LLM-based summarization",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/compact full",
        description: "Full compression with file re-injection",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/compact stats",
        description: "Show compression statistics",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/verbose",
        description: "Switch to verbose output mode",
        category: CommandCategory::Display,
    },
    CommandEntry {
        name: "/clear",
        description: "Clear the screen",
        category: CommandCategory::Display,
    },
    CommandEntry {
        name: "/theme",
        description: "Switch color theme",
        category: CommandCategory::Display,
    },
    // Tools & Analysis
    CommandEntry {
        name: "/last",
        description: "Show details of the last tool execution",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug",
        description: "Show current task tool history and recent errors",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug full",
        description: "Show full args/results for the current task",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug tool",
        description: "Show one tool call with full details",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug state",
        description: "Show task-state memory for the current task",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug-log",
        description: "Show the persistent session execution log",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/debug-log full",
        description: "Show full recent session log entries",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/tools",
        description: "List available tools",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/analyze",
        description: "Analyze the current codebase",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/review",
        description: "Review recent changes",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/plan",
        description: "Enter plan mode (read-only tools only)",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/execute",
        description: "Approve plan and start execution",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/modify",
        description: "Exit plan mode without executing",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/swarm",
        description: "Launch multi-agent swarm",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/queue",
        description: "Enqueue a message for later processing",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/queue list",
        description: "Show queued messages",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/queue clear",
        description: "Clear all queued messages",
        category: CommandCategory::Tools,
    },
    CommandEntry {
        name: "/queue drop",
        description: "Remove a queued message by index",
        category: CommandCategory::Tools,
    },
    // Git
    CommandEntry {
        name: "/diff",
        description: "Show git diff --stat",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/git",
        description: "Show git status",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/undo",
        description: "Undo the last file edit",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/worktree",
        description: "Worktree management commands",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/worktree enter",
        description: "Create and enter a git worktree",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/worktree exit",
        description: "Exit current worktree",
        category: CommandCategory::Git,
    },
    CommandEntry {
        name: "/worktree list",
        description: "List all git worktrees",
        category: CommandCategory::Git,
    },
    // Session
    CommandEntry {
        name: "/copy",
        description: "Copy last response to clipboard",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/restore",
        description: "Restore from checkpoint",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/chat",
        description: "Chat session management",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/chat save",
        description: "Save the current chat session",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/chat resume",
        description: "Resume a saved chat session",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/chat list",
        description: "List saved chat sessions",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/chat delete",
        description: "Delete a saved chat session",
        category: CommandCategory::Session,
    },
    CommandEntry {
        name: "/vim",
        description: "Switch to vim input mode",
        category: CommandCategory::General,
    },
    // Learning / Code Education
    CommandEntry {
        name: "/explain",
        description: "Explain code in a file or set explanation level",
        category: CommandCategory::Tools,
    },
    // Dream System (Memory Consolidation)
    CommandEntry {
        name: "/dream",
        description: "Trigger or check status of memory consolidation (dream)",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/dream status",
        description: "Show dream system status",
        category: CommandCategory::Context,
    },
    CommandEntry {
        name: "/dream force",
        description: "Force immediate memory consolidation",
        category: CommandCategory::Context,
    },
];

/// Exit commands (not slash commands but valid input to exit)
pub static EXIT_COMMANDS: &[&str] = &["exit", "quit"];

/// Get all command names (for completions and highlighting)
pub fn command_names() -> Vec<String> {
    let mut names: Vec<String> = COMMANDS.iter().map(|c| c.name.to_string()).collect();
    names.extend(EXIT_COMMANDS.iter().map(|s| s.to_string()));
    names
}

/// Get command description by name (for completion hints)
pub fn command_description(name: &str) -> Option<&'static str> {
    COMMANDS
        .iter()
        .find(|c| c.name == name)
        .map(|c| c.description)
}

/// Check if a string is a recognized command (for highlighting)
pub fn is_known_command(input: &str) -> bool {
    let trimmed = input.trim();
    COMMANDS
        .iter()
        .any(|c| trimmed == c.name || trimmed.starts_with(&format!("{} ", c.name)))
        || EXIT_COMMANDS.contains(&trimmed)
}

#[cfg(test)]
#[path = "../../tests/unit/input/command_registry/command_registry_test.rs"]
mod tests;
