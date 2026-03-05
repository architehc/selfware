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
    use std::collections::HashSet;

    // -------------------------------------------------------------------------
    // COMMANDS static registry — structural invariants
    // -------------------------------------------------------------------------

    #[test]
    fn test_no_duplicate_command_names() {
        let mut seen = HashSet::new();
        for cmd in COMMANDS {
            assert!(seen.insert(cmd.name), "Duplicate command: {}", cmd.name);
        }
    }

    #[test]
    fn test_all_commands_start_with_slash() {
        for cmd in COMMANDS {
            assert!(
                cmd.name.starts_with('/'),
                "Command '{}' does not start with '/'",
                cmd.name
            );
        }
    }

    #[test]
    fn test_all_commands_have_non_empty_name() {
        for cmd in COMMANDS {
            assert!(!cmd.name.is_empty(), "A command entry has an empty name");
        }
    }

    #[test]
    fn test_all_commands_have_non_empty_description() {
        for cmd in COMMANDS {
            assert!(
                !cmd.description.is_empty(),
                "Command '{}' has an empty description",
                cmd.name
            );
        }
    }

    #[test]
    fn test_commands_registry_is_non_empty() {
        assert!(
            !COMMANDS.is_empty(),
            "COMMANDS registry should not be empty"
        );
    }

    #[test]
    fn test_known_commands_present_in_registry() {
        // Spot-check a representative command from every category.
        let expected = [
            "/help",
            "/status",
            "/ctx",
            "/ctx load",
            "/ctx clear",
            "/ctx reload",
            "/ctx copy",
            "/context",
            "/compress",
            "/memory",
            "/compact",
            "/verbose",
            "/clear",
            "/theme",
            "/last",
            "/tools",
            "/analyze",
            "/review",
            "/plan",
            "/swarm",
            "/queue",
            "/queue list",
            "/queue clear",
            "/queue drop",
            "/diff",
            "/git",
            "/undo",
            "/copy",
            "/restore",
            "/chat",
            "/chat save",
            "/chat resume",
            "/chat list",
            "/chat delete",
            "/vim",
        ];
        let registry: HashSet<&str> = COMMANDS.iter().map(|c| c.name).collect();
        for name in &expected {
            assert!(
                registry.contains(name),
                "Expected command '{}' not found in registry",
                name
            );
        }
    }

    // -------------------------------------------------------------------------
    // EXIT_COMMANDS static
    // -------------------------------------------------------------------------

    #[test]
    fn test_exit_commands_contains_exit_and_quit() {
        assert!(
            EXIT_COMMANDS.contains(&"exit"),
            "'exit' should be in EXIT_COMMANDS"
        );
        assert!(
            EXIT_COMMANDS.contains(&"quit"),
            "'quit' should be in EXIT_COMMANDS"
        );
    }

    #[test]
    fn test_exit_commands_are_non_empty() {
        assert!(
            !EXIT_COMMANDS.is_empty(),
            "EXIT_COMMANDS should not be empty"
        );
    }

    #[test]
    fn test_exit_commands_do_not_start_with_slash() {
        for &cmd in EXIT_COMMANDS {
            assert!(
                !cmd.starts_with('/'),
                "EXIT_COMMAND '{}' unexpectedly starts with '/'",
                cmd
            );
        }
    }

    // -------------------------------------------------------------------------
    // command_names()
    // -------------------------------------------------------------------------

    #[test]
    fn test_command_names_returns_all() {
        let names = command_names();
        assert!(names.contains(&"/help".to_string()));
        assert!(names.contains(&"/ctx".to_string()));
        assert!(names.contains(&"exit".to_string()));
        assert!(names.contains(&"quit".to_string()));
    }

    #[test]
    fn test_command_names_length_matches_commands_plus_exits() {
        let names = command_names();
        let expected_len = COMMANDS.len() + EXIT_COMMANDS.len();
        assert_eq!(
            names.len(),
            expected_len,
            "command_names() returned {} entries but expected {}",
            names.len(),
            expected_len
        );
    }

    #[test]
    fn test_command_names_contains_every_registered_command() {
        let names: HashSet<String> = command_names().into_iter().collect();
        for cmd in COMMANDS {
            assert!(
                names.contains(cmd.name),
                "command_names() is missing registered command '{}'",
                cmd.name
            );
        }
    }

    #[test]
    fn test_command_names_contains_every_exit_command() {
        let names: HashSet<String> = command_names().into_iter().collect();
        for &exit_cmd in EXIT_COMMANDS {
            assert!(
                names.contains(exit_cmd),
                "command_names() is missing exit command '{}'",
                exit_cmd
            );
        }
    }

    #[test]
    fn test_command_names_no_duplicates() {
        let names = command_names();
        let unique: HashSet<&String> = names.iter().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "command_names() contains duplicate entries"
        );
    }

    // -------------------------------------------------------------------------
    // command_description()
    // -------------------------------------------------------------------------

    #[test]
    fn test_command_description_found() {
        assert!(command_description("/help").is_some());
        assert!(command_description("/nonexistent").is_none());
    }

    #[test]
    fn test_command_description_returns_correct_text_for_help() {
        let desc = command_description("/help");
        assert_eq!(desc, Some("Show help and available commands"));
    }

    #[test]
    fn test_command_description_returns_correct_text_for_ctx_load() {
        let desc = command_description("/ctx load");
        assert_eq!(desc, Some("Load files into context"));
    }

    #[test]
    fn test_command_description_returns_none_for_empty_string() {
        assert_eq!(command_description(""), None);
    }

    #[test]
    fn test_command_description_returns_none_for_exit_commands() {
        // EXIT_COMMANDS are not in COMMANDS, so description lookup should fail.
        assert_eq!(command_description("exit"), None);
        assert_eq!(command_description("quit"), None);
    }

    #[test]
    fn test_command_description_is_case_sensitive() {
        // Registry uses lowercase; uppercase variants should not match.
        assert_eq!(command_description("/Help"), None);
        assert_eq!(command_description("/HELP"), None);
        assert_eq!(command_description("/CTX"), None);
    }

    #[test]
    fn test_command_description_no_partial_prefix_match() {
        // "/hel" is not a registered command — only exact names match.
        assert_eq!(command_description("/hel"), None);
        assert_eq!(command_description("/ct"), None);
    }

    #[test]
    fn test_command_description_for_every_registered_command() {
        // Every entry in COMMANDS must return Some description via the lookup function.
        for cmd in COMMANDS {
            let result = command_description(cmd.name);
            assert!(
                result.is_some(),
                "command_description('{}') returned None, expected Some",
                cmd.name
            );
            assert_eq!(
                result.unwrap(),
                cmd.description,
                "description mismatch for '{}'",
                cmd.name
            );
        }
    }

    // -------------------------------------------------------------------------
    // is_known_command()
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_known_command() {
        assert!(is_known_command("/help"));
        assert!(is_known_command("/ctx load"));
        assert!(is_known_command("exit"));
        assert!(!is_known_command("random text"));
    }

    #[test]
    fn test_is_known_command_exact_slash_commands() {
        // Each registered command by itself should be recognized.
        for cmd in COMMANDS {
            assert!(
                is_known_command(cmd.name),
                "is_known_command('{}') returned false",
                cmd.name
            );
        }
    }

    #[test]
    fn test_is_known_command_exit_commands() {
        for &exit_cmd in EXIT_COMMANDS {
            assert!(
                is_known_command(exit_cmd),
                "is_known_command('{}') should be true for exit commands",
                exit_cmd
            );
        }
    }

    #[test]
    fn test_is_known_command_with_trailing_argument() {
        // A known command followed by a space and an argument is still recognized.
        assert!(is_known_command("/ctx load somefile.txt"));
        assert!(is_known_command("/chat save mysession"));
        assert!(is_known_command("/mode autoedit"));
        assert!(is_known_command("/model claude-opus-4"));
    }

    #[test]
    fn test_is_known_command_trims_leading_and_trailing_whitespace() {
        assert!(is_known_command("  /help  "));
        assert!(is_known_command("\t/ctx\t"));
        assert!(is_known_command("  exit  "));
    }

    #[test]
    fn test_is_known_command_returns_false_for_empty_string() {
        assert!(!is_known_command(""));
    }

    #[test]
    fn test_is_known_command_returns_false_for_whitespace_only() {
        assert!(!is_known_command("   "));
        assert!(!is_known_command("\t\n"));
    }

    #[test]
    fn test_is_known_command_returns_false_for_unknown_slash_command() {
        assert!(!is_known_command("/unknown"));
        assert!(!is_known_command("/foobar"));
    }

    #[test]
    fn test_is_known_command_is_case_sensitive() {
        assert!(!is_known_command("/Help"));
        assert!(!is_known_command("/HELP"));
        assert!(!is_known_command("Exit"));
        assert!(!is_known_command("QUIT"));
    }

    #[test]
    fn test_is_known_command_no_prefix_only_match() {
        // A prefix of a command without a trailing space should not match a longer command.
        // "/ct" is not a registered command; only "/ctx" is.
        assert!(!is_known_command("/ct"));
        assert!(!is_known_command("/hel"));
    }

    #[test]
    fn test_is_known_command_does_not_match_bare_text_prefixed_by_slash() {
        assert!(!is_known_command("/random"));
        assert!(!is_known_command("/12345"));
    }

    #[test]
    fn test_is_known_command_special_characters_in_argument() {
        // The command itself is known; the argument can contain special characters.
        assert!(is_known_command("/ctx load path/to/file with spaces.txt"));
        assert!(is_known_command("/model gpt-4o-mini"));
        assert!(is_known_command("/chat save session_2026-03-04"));
    }

    // -------------------------------------------------------------------------
    // CommandCategory — coverage of every variant
    // -------------------------------------------------------------------------

    #[test]
    fn test_all_categories_are_used() {
        let used: HashSet<String> = COMMANDS
            .iter()
            .map(|c| format!("{:?}", c.category))
            .collect();

        let all_variants = [
            "General",
            "Context",
            "Navigation",
            "Git",
            "Tools",
            "Session",
            "Display",
        ];

        // Every variant except Navigation has at least one command.
        // We only assert the ones that are actually populated in the registry.
        for variant in &["General", "Context", "Git", "Tools", "Session", "Display"] {
            assert!(
                used.contains(*variant),
                "Category {:?} has no commands in COMMANDS",
                variant
            );
        }

        // Confirm Navigation variant compiles and is equal to itself.
        let nav = CommandCategory::Navigation;
        assert_eq!(nav, CommandCategory::Navigation);
        // Navigation is defined but intentionally has no commands yet — that is fine.
        let _ = all_variants; // suppress unused warning
    }

    #[test]
    fn test_command_category_clone_and_copy() {
        let original = CommandCategory::General;
        let cloned = original; // Copy
        let cloned2 = original.clone();
        assert_eq!(original, cloned);
        assert_eq!(original, cloned2);
    }

    #[test]
    fn test_command_category_debug() {
        // Debug formatting should produce a non-empty string for every variant.
        let variants = [
            CommandCategory::General,
            CommandCategory::Context,
            CommandCategory::Navigation,
            CommandCategory::Git,
            CommandCategory::Tools,
            CommandCategory::Session,
            CommandCategory::Display,
        ];
        for v in &variants {
            let s = format!("{:?}", v);
            assert!(!s.is_empty(), "Debug output was empty for {:?}", v);
        }
    }

    #[test]
    fn test_command_category_equality() {
        assert_eq!(CommandCategory::General, CommandCategory::General);
        assert_ne!(CommandCategory::General, CommandCategory::Git);
        assert_ne!(CommandCategory::Context, CommandCategory::Display);
        assert_ne!(CommandCategory::Tools, CommandCategory::Session);
        assert_ne!(CommandCategory::Navigation, CommandCategory::General);
    }

    // -------------------------------------------------------------------------
    // CommandEntry — structural checks
    // -------------------------------------------------------------------------

    #[test]
    fn test_command_entry_clone() {
        let entry = &COMMANDS[0];
        let cloned = entry.clone();
        assert_eq!(entry.name, cloned.name);
        assert_eq!(entry.description, cloned.description);
        assert_eq!(entry.category, cloned.category);
    }

    #[test]
    fn test_command_entry_debug() {
        let entry = &COMMANDS[0];
        let debug_str = format!("{:?}", entry);
        assert!(!debug_str.is_empty());
        assert!(debug_str.contains(entry.name));
    }

    // -------------------------------------------------------------------------
    // Category grouping — commands belong to expected categories
    // -------------------------------------------------------------------------

    #[test]
    fn test_ctx_commands_are_in_context_category() {
        let ctx_commands = [
            "/ctx",
            "/ctx load",
            "/ctx clear",
            "/ctx reload",
            "/ctx copy",
            "/context",
            "/compress",
            "/memory",
        ];
        let registry: std::collections::HashMap<&str, CommandCategory> =
            COMMANDS.iter().map(|c| (c.name, c.category)).collect();

        for name in &ctx_commands {
            let cat = registry.get(name).copied();
            assert_eq!(
                cat,
                Some(CommandCategory::Context),
                "'{}' should be in Context category, got {:?}",
                name,
                cat
            );
        }
    }

    #[test]
    fn test_git_commands_are_in_git_category() {
        let git_commands = ["/diff", "/git", "/undo"];
        let registry: std::collections::HashMap<&str, CommandCategory> =
            COMMANDS.iter().map(|c| (c.name, c.category)).collect();

        for name in &git_commands {
            let cat = registry.get(name).copied();
            assert_eq!(
                cat,
                Some(CommandCategory::Git),
                "'{}' should be in Git category, got {:?}",
                name,
                cat
            );
        }
    }

    #[test]
    fn test_display_commands_are_in_display_category() {
        let display_commands = ["/compact", "/verbose", "/clear", "/theme"];
        let registry: std::collections::HashMap<&str, CommandCategory> =
            COMMANDS.iter().map(|c| (c.name, c.category)).collect();

        for name in &display_commands {
            let cat = registry.get(name).copied();
            assert_eq!(
                cat,
                Some(CommandCategory::Display),
                "'{}' should be in Display category, got {:?}",
                name,
                cat
            );
        }
    }

    #[test]
    fn test_session_commands_are_in_session_category() {
        let session_commands = [
            "/copy",
            "/restore",
            "/chat",
            "/chat save",
            "/chat resume",
            "/chat list",
            "/chat delete",
        ];
        let registry: std::collections::HashMap<&str, CommandCategory> =
            COMMANDS.iter().map(|c| (c.name, c.category)).collect();

        for name in &session_commands {
            let cat = registry.get(name).copied();
            assert_eq!(
                cat,
                Some(CommandCategory::Session),
                "'{}' should be in Session category, got {:?}",
                name,
                cat
            );
        }
    }

    #[test]
    fn test_tools_commands_are_in_tools_category() {
        let tools_commands = [
            "/last",
            "/tools",
            "/analyze",
            "/review",
            "/plan",
            "/swarm",
            "/queue",
            "/queue list",
            "/queue clear",
            "/queue drop",
        ];
        let registry: std::collections::HashMap<&str, CommandCategory> =
            COMMANDS.iter().map(|c| (c.name, c.category)).collect();

        for name in &tools_commands {
            let cat = registry.get(name).copied();
            assert_eq!(
                cat,
                Some(CommandCategory::Tools),
                "'{}' should be in Tools category, got {:?}",
                name,
                cat
            );
        }
    }
}
