//! Selfware Autocomplete System
//!
//! Context-aware completion for commands, tools, and file paths.

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use reedline::{Completer, Span, Suggestion};
use std::path::Path;

/// Multi-source completer for the Selfware workshop
pub struct SelfwareCompleter {
    /// Tool names (file_read, git_status, etc.)
    tool_names: Vec<String>,
    /// Slash commands (/help, /status, etc.)
    commands: Vec<String>,
    /// Fuzzy matcher for smart matching
    matcher: SkimMatcherV2,
}

impl SelfwareCompleter {
    /// Create a new completer with tools and commands
    pub fn new(tool_names: Vec<String>, commands: Vec<String>) -> Self {
        Self {
            tool_names,
            commands,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Get command completions
    #[allow(dead_code)] // Convenience wrapper
    fn complete_commands(&self, prefix: &str) -> Vec<Suggestion> {
        self.complete_commands_with_span(prefix, 0, prefix.len())
    }

    /// Get command completions with explicit span
    fn complete_commands_with_span(
        &self,
        prefix: &str,
        span_start: usize,
        span_end: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions: Vec<(i64, Suggestion)> = self
            .commands
            .iter()
            .filter_map(|cmd| {
                self.matcher.fuzzy_match(cmd, prefix).map(|score| {
                    (
                        score,
                        Suggestion {
                            value: cmd.clone(),
                            description: Some(self.command_description(cmd)),
                            style: None,
                            extra: None,
                            span: Span::new(span_start, span_end),
                            append_whitespace: true,
                            match_indices: None,
                        },
                    )
                })
            })
            .collect();

        suggestions.sort_by(|a, b| b.0.cmp(&a.0));
        suggestions.into_iter().map(|(_, s)| s).collect()
    }

    /// Get tool completions
    #[allow(dead_code)] // Convenience wrapper
    fn complete_tools(&self, prefix: &str) -> Vec<Suggestion> {
        self.complete_tools_with_span(prefix, 0, prefix.len())
    }

    /// Get tool completions with explicit span
    fn complete_tools_with_span(
        &self,
        prefix: &str,
        span_start: usize,
        span_end: usize,
    ) -> Vec<Suggestion> {
        let mut suggestions: Vec<(i64, Suggestion)> = self
            .tool_names
            .iter()
            .filter_map(|tool| {
                self.matcher.fuzzy_match(tool, prefix).map(|score| {
                    (
                        score,
                        Suggestion {
                            value: tool.clone(),
                            description: Some(format!("Tool: {}", tool)),
                            style: None,
                            extra: None,
                            span: Span::new(span_start, span_end),
                            append_whitespace: true,
                            match_indices: None,
                        },
                    )
                })
            })
            .collect();

        suggestions.sort_by(|a, b| b.0.cmp(&a.0));
        suggestions.into_iter().map(|(_, s)| s).collect()
    }

    /// Get file path completions
    #[allow(dead_code)] // Convenience wrapper
    fn complete_paths(&self, prefix: &str) -> Vec<Suggestion> {
        self.complete_paths_with_span(prefix, 0, prefix.len())
    }

    /// Get file path completions with explicit span
    fn complete_paths_with_span(
        &self,
        prefix: &str,
        span_start: usize,
        span_end: usize,
    ) -> Vec<Suggestion> {
        let path = Path::new(prefix);
        let (dir, file_prefix): (String, String) =
            if prefix.ends_with('/') || prefix.ends_with('\\') {
                (prefix.to_string(), String::new())
            } else {
                let parent = path.parent().map(|p| p.to_string_lossy().to_string());
                let file = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                (parent.unwrap_or_else(|| ".".to_string()), file)
            };

        let dir_path = if dir.is_empty() { "." } else { &dir };

        let mut suggestions = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip hidden files unless prefix starts with .
                if name.starts_with('.') && !file_prefix.starts_with('.') && !file_prefix.is_empty()
                {
                    continue;
                }

                if let Some(score) = self.matcher.fuzzy_match(&name, &file_prefix) {
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let full_path = if dir == "." {
                        if is_dir {
                            format!("{}/", name)
                        } else {
                            name.clone()
                        }
                    } else if is_dir {
                        format!("{}/{}/", dir.trim_end_matches('/'), name)
                    } else {
                        format!("{}/{}", dir.trim_end_matches('/'), name)
                    };

                    suggestions.push((
                        score,
                        Suggestion {
                            value: full_path,
                            description: Some(if is_dir {
                                "Directory".to_string()
                            } else {
                                "File".to_string()
                            }),
                            style: None,
                            extra: None,
                            span: Span::new(span_start, span_end),
                            append_whitespace: !is_dir,
                            match_indices: None,
                        },
                    ));
                }
            }
        }

        suggestions.sort_by(|a, b| b.0.cmp(&a.0));
        suggestions.into_iter().take(20).map(|(_, s)| s).collect()
    }

    /// Get description for a command
    fn command_description(&self, cmd: &str) -> String {
        match cmd {
            "/help" => "Show available commands".to_string(),
            "/status" => "Show agent status".to_string(),
            "/stats" => "Detailed session statistics".to_string(),
            "/mode" => "Cycle execution mode".to_string(),
            "/ctx" => "Context window stats".to_string(),
            "/ctx clear" => "Clear all context".to_string(),
            "/ctx load" => "Load files into context".to_string(),
            "/ctx reload" => "Reload loaded files".to_string(),
            "/ctx copy" => "Copy sources to clipboard".to_string(),
            "/compress" => "Compress context".to_string(),
            "/context" => "Context window stats".to_string(),
            "/memory" => "Show memory statistics".to_string(),
            "/clear" => "Clear conversation history".to_string(),
            "/tools" => "List available tools".to_string(),
            "/analyze" => "Analyze codebase structure".to_string(),
            "/review" => "Review code in file".to_string(),
            "/plan" => "Create a plan for a task".to_string(),
            "/diff" => "Git diff --stat".to_string(),
            "/git" => "Git status --short".to_string(),
            "/undo" => "Undo last file edit".to_string(),
            "/cost" => "Token usage & cost estimate".to_string(),
            "/model" => "Model configuration".to_string(),
            "/compact" => "Toggle compact mode".to_string(),
            "/verbose" => "Toggle verbose mode".to_string(),
            "/config" => "Show current config".to_string(),
            "/garden" => "View digital garden".to_string(),
            "/journal" => "Browse journal entries".to_string(),
            "/palette" => "Open command palette".to_string(),
            "/vim" => "Toggle vim/emacs mode".to_string(),
            "/copy" => "Copy last response to clipboard".to_string(),
            "/restore" => "List/restore edit checkpoints".to_string(),
            "/chat" => "Chat session management".to_string(),
            "/chat save" => "Save current chat session".to_string(),
            "/chat resume" => "Resume a saved chat".to_string(),
            "/chat list" => "List saved chats".to_string(),
            "/chat delete" => "Delete a saved chat".to_string(),
            "/theme" => "Switch color theme".to_string(),
            "exit" => "Exit interactive mode".to_string(),
            "quit" => "Exit interactive mode".to_string(),
            _ => "Command".to_string(),
        }
    }

    /// Detect what type of completion is needed
    fn detect_context(&self, line: &str, pos: usize) -> CompletionContext {
        let before_cursor = &line[..pos];

        // If we're after a command that takes a path argument (check first!)
        if before_cursor.starts_with("/analyze ")
            || before_cursor.starts_with("/review ")
            || before_cursor.starts_with("/ctx load ")
            || before_cursor.starts_with("/context load ")
            || before_cursor.starts_with("/chat save ")
            || before_cursor.starts_with("/chat resume ")
            || before_cursor.starts_with("/chat delete ")
            || before_cursor.starts_with("/theme ")
            || before_cursor.starts_with("/restore ")
        {
            let prefix = before_cursor.split_whitespace().last().unwrap_or("");
            return CompletionContext::Path(prefix.to_string());
        }

        // If starts with /, complete commands
        if before_cursor.starts_with('/') {
            return CompletionContext::Command(before_cursor.to_string());
        }

        // Check if the last word starts with @ (file reference)
        let words: Vec<&str> = before_cursor.split_whitespace().collect();
        if let Some(last) = words.last() {
            if last.starts_with('@') {
                let path_prefix = &last[1..]; // strip the @
                return CompletionContext::FileReference(path_prefix.to_string());
            }

            // If the last word looks like a tool name prefix
            if last.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return CompletionContext::Tool(last.to_string());
            }

            // If it looks like a path
            if last.contains('/') || last.contains('.') {
                return CompletionContext::Path(last.to_string());
            }
        }

        CompletionContext::None
    }

    /// Complete file references (@ prefix paths)
    fn complete_file_refs_with_span(
        &self,
        prefix: &str,
        span_start: usize,
        span_end: usize,
    ) -> Vec<Suggestion> {
        // Reuse path completion but prepend @ to results
        let path_suggestions = self.complete_paths_with_span(prefix, span_start, span_end);
        path_suggestions
            .into_iter()
            .map(|mut s| {
                s.value = format!("@{}", s.value);
                s.description = Some(
                    s.description
                        .map(|d| format!("Include {}", d.to_lowercase()))
                        .unwrap_or_else(|| "Include file".to_string()),
                );
                s
            })
            .collect()
    }
}

/// What type of completion is being requested
enum CompletionContext {
    Command(String),
    Tool(String),
    Path(String),
    /// @file reference completion
    FileReference(String),
    None,
}

impl Completer for SelfwareCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let before_cursor = &line[..pos];

        match self.detect_context(line, pos) {
            CompletionContext::Command(prefix) => self.complete_commands_with_span(&prefix, 0, pos),
            CompletionContext::Tool(prefix) => {
                // Tool completion starts at the beginning of the current word
                let word_start = before_cursor
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.complete_tools_with_span(&prefix, word_start, pos)
            }
            CompletionContext::Path(prefix) => {
                // Path completion starts at the beginning of the path
                let word_start = before_cursor
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.complete_paths_with_span(&prefix, word_start, pos)
            }
            CompletionContext::FileReference(prefix) => {
                // @ file reference: span starts at the @ character
                let word_start = before_cursor
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.complete_file_refs_with_span(&prefix, word_start, pos)
            }
            CompletionContext::None => {
                // If empty or very short, show commands but with correct span
                if before_cursor.is_empty() || before_cursor.len() < 2 {
                    self.complete_commands_with_span("/", 0, pos)
                } else {
                    Vec::new()
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completer_creation() {
        let completer = SelfwareCompleter::new(
            vec!["file_read".into(), "git_status".into()],
            vec!["/help".into(), "/status".into()],
        );
        assert_eq!(completer.tool_names.len(), 2);
        assert_eq!(completer.commands.len(), 2);
    }

    #[test]
    fn test_command_completions() {
        let completer = SelfwareCompleter::new(
            vec![],
            vec!["/help".into(), "/status".into(), "/memory".into()],
        );

        let suggestions = completer.complete_commands("/he");
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.value == "/help"));
    }

    #[test]
    fn test_command_completions_all() {
        let completer = SelfwareCompleter::new(
            vec![],
            vec![
                "/help".into(),
                "/status".into(),
                "/memory".into(),
                "/clear".into(),
            ],
        );

        // Empty prefix should return all commands
        let suggestions = completer.complete_commands("/");
        assert_eq!(suggestions.len(), 4);
    }

    #[test]
    fn test_fuzzy_matching() {
        let completer = SelfwareCompleter::new(
            vec!["file_read".into(), "file_write".into(), "git_status".into()],
            vec![],
        );

        // "fr" should match "file_read" (fuzzy)
        let suggestions = completer.complete_tools("fr");
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_fuzzy_matching_order() {
        let completer = SelfwareCompleter::new(
            vec!["file_read".into(), "file_write".into(), "git_status".into()],
            vec![],
        );

        // "file" should have file_read and file_write at top
        let suggestions = completer.complete_tools("file");
        assert!(suggestions.len() >= 2);
        assert!(suggestions[0].value.starts_with("file"));
    }

    #[test]
    fn test_tool_completions_empty() {
        let completer = SelfwareCompleter::new(vec![], vec![]);
        let suggestions = completer.complete_tools("anything");
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_command_description() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        assert_eq!(
            completer.command_description("/help"),
            "Show available commands"
        );
        assert_eq!(
            completer.command_description("/status"),
            "Show agent status"
        );
        assert_eq!(
            completer.command_description("/memory"),
            "Show memory statistics"
        );
        assert_eq!(
            completer.command_description("/clear"),
            "Clear conversation history"
        );
        assert_eq!(
            completer.command_description("/tools"),
            "List available tools"
        );
        assert_eq!(
            completer.command_description("/analyze"),
            "Analyze codebase structure"
        );
        assert_eq!(
            completer.command_description("/review"),
            "Review code in file"
        );
        assert_eq!(completer.command_description("/unknown"), "Command");
    }

    #[test]
    fn test_detect_context_command() {
        let completer = SelfwareCompleter::new(vec![], vec!["/help".into()]);

        // Starting with / should be command context
        match completer.detect_context("/hel", 4) {
            CompletionContext::Command(prefix) => assert_eq!(prefix, "/hel"),
            _ => panic!("Expected Command context"),
        }
    }

    #[test]
    fn test_detect_context_path_after_analyze() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        // After /analyze, should be path context
        match completer.detect_context("/analyze ./src", 14) {
            CompletionContext::Path(prefix) => assert_eq!(prefix, "./src"),
            _ => panic!("Expected Path context"),
        }
    }

    #[test]
    fn test_detect_context_path_after_review() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        match completer.detect_context("/review main.rs", 15) {
            CompletionContext::Path(prefix) => assert_eq!(prefix, "main.rs"),
            _ => panic!("Expected Path context"),
        }
    }

    #[test]
    fn test_detect_context_tool() {
        let completer = SelfwareCompleter::new(vec!["file_read".into()], vec![]);

        // Alphanumeric with underscore should be tool context
        match completer.detect_context("file_re", 7) {
            CompletionContext::Tool(prefix) => assert_eq!(prefix, "file_re"),
            _ => panic!("Expected Tool context"),
        }
    }

    #[test]
    fn test_detect_context_none() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        // Empty should be None
        match completer.detect_context("", 0) {
            CompletionContext::None => {}
            _ => panic!("Expected None context"),
        }
    }

    #[test]
    fn test_complete_interface() {
        let mut completer = SelfwareCompleter::new(vec!["file_read".into()], vec!["/help".into()]);

        // Test the Completer trait implementation
        let suggestions = completer.complete("/he", 3);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_complete_empty_line() {
        let mut completer = SelfwareCompleter::new(vec![], vec!["/help".into(), "/status".into()]);

        // Empty line should show commands
        let suggestions = completer.complete("", 0);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_suggestion_has_description() {
        let completer = SelfwareCompleter::new(vec![], vec!["/help".into()]);

        let suggestions = completer.complete_commands("/help");
        assert!(!suggestions.is_empty());
        assert!(suggestions[0].description.is_some());
    }

    #[test]
    fn test_path_completions_current_dir() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        // Should be able to complete from current directory
        let suggestions = completer.complete_paths("./");
        // May or may not have results depending on current directory
        // Just verify it doesn't panic
        let _ = suggestions;
    }

    #[test]
    fn test_path_completions_nonexistent() {
        let completer = SelfwareCompleter::new(vec![], vec![]);

        // Nonexistent directory should return empty
        let suggestions = completer.complete_paths("/nonexistent/path/here/");
        assert!(suggestions.is_empty());
    }
}
