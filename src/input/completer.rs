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
    #[cfg(test)]
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
                            display_override: None,
                        },
                    )
                })
            })
            .collect();

        suggestions.sort_by_key(|x| std::cmp::Reverse(x.0));
        suggestions.into_iter().map(|(_, s)| s).collect()
    }

    /// Get tool completions
    #[cfg(test)]
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
                            display_override: None,
                        },
                    )
                })
            })
            .collect();

        suggestions.sort_by_key(|x| std::cmp::Reverse(x.0));
        suggestions.into_iter().map(|(_, s)| s).collect()
    }

    /// Get file path completions
    #[cfg(test)]
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
                            display_override: None,
                        },
                    ));
                }
            }
        }

        suggestions.sort_by_key(|x| std::cmp::Reverse(x.0));
        suggestions.into_iter().take(20).map(|(_, s)| s).collect()
    }

    /// Get description for a command
    fn command_description(&self, cmd: &str) -> String {
        if let Some(desc) = super::command_registry::command_description(cmd) {
            desc.to_string()
        } else if cmd == "exit" || cmd == "quit" {
            "Exit interactive mode".to_string()
        } else {
            "Command".to_string()
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
            if let Some(path_prefix) = last.strip_prefix('@') {
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
#[path = "../../tests/unit/input/completer/completer_test.rs"]
mod tests;
