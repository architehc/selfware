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
        name: "/memory",
        description: "Show memory hierarchy status",
        category: CommandCategory::Context,
    },
    // Display
    CommandEntry {
        name: "/compact",
        description: "Switch to compact output mode",
        category: CommandCategory::Display,
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
        description: "Create an execution plan",
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
mod tests {
    use super::*;

    #[test]
    fn test_no_duplicate_command_names() {
        let mut seen = std::collections::HashSet::new();
        for cmd in COMMANDS {
            assert!(seen.insert(cmd.name), "Duplicate command: {}", cmd.name);
        }
    }

    #[test]
    fn test_command_names_returns_all() {
        let names = command_names();
        assert!(names.contains(&"/help".to_string()));
        assert!(names.contains(&"/ctx".to_string()));
        assert!(names.contains(&"exit".to_string()));
        assert!(names.contains(&"quit".to_string()));
    }

    #[test]
    fn test_command_description_found() {
        assert!(command_description("/help").is_some());
        assert!(command_description("/nonexistent").is_none());
    }

    #[test]
    fn test_is_known_command() {
        assert!(is_known_command("/help"));
        assert!(is_known_command("/ctx load"));
        assert!(is_known_command("exit"));
        assert!(!is_known_command("random text"));
    }
}
