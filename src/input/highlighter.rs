//! Syntax Highlighting for Selfware Input
//!
//! Highlights commands, paths, and tool names as you type.

use nu_ansi_term::{Color, Style};
use reedline::{Highlighter, StyledText};

/// Highlighter for Selfware input with organic colors
pub struct SelfwareHighlighter {
    /// Style for commands (/help, /status)
    command_style: Style,
    /// Style for file paths
    path_style: Style,
    /// Style for keywords
    keyword_style: Style,
    /// Style for strings
    string_style: Style,
    /// Default style
    default_style: Style,
}

impl SelfwareHighlighter {
    /// Create a new highlighter with the Selfware palette
    pub fn new() -> Self {
        Self {
            // Amber for commands
            command_style: Style::new().fg(Color::Rgb(212, 163, 115)).bold(),
            // Sage for paths
            path_style: Style::new().fg(Color::Rgb(143, 151, 121)).italic(),
            // Garden green for keywords
            keyword_style: Style::new().fg(Color::Rgb(96, 108, 56)),
            // Soil brown for strings
            string_style: Style::new().fg(Color::Rgb(188, 108, 37)),
            // Default
            default_style: Style::new(),
        }
    }

    /// Check if a word is a command
    fn is_command(&self, word: &str) -> bool {
        word.starts_with('/')
            && super::command_registry::COMMANDS
                .iter()
                .any(|c| c.name == word)
    }

    /// Check if a word looks like a path
    #[cfg(test)]
    fn is_path(&self, word: &str) -> bool {
        word.contains('/')
            || word.starts_with('.')
            || word.starts_with('~')
            || word.ends_with(".rs")
            || word.ends_with(".py")
            || word.ends_with(".js")
            || word.ends_with(".ts")
            || word.ends_with(".toml")
            || word.ends_with(".json")
            || word.ends_with(".md")
    }

    /// Check if a word is a known keyword
    #[cfg(test)]
    fn is_keyword(&self, word: &str) -> bool {
        matches!(
            word.to_lowercase().as_str(),
            "exit" | "quit" | "help" | "yes" | "no" | "true" | "false"
        )
    }

    /// Check if text is a quoted string
    fn find_strings(&self, line: &str) -> Vec<(usize, usize)> {
        let mut strings = Vec::new();
        let mut in_string = false;
        let mut string_start = 0;
        let mut quote_char = '"';

        for (i, c) in line.char_indices() {
            if !in_string && (c == '"' || c == '\'') {
                in_string = true;
                string_start = i;
                quote_char = c;
            } else if in_string && c == quote_char {
                strings.push((string_start, i + 1));
                in_string = false;
            }
        }

        // Handle unclosed string
        if in_string {
            strings.push((string_start, line.len()));
        }

        strings
    }

    /// Check if position is inside a string
    #[cfg(test)]
    fn in_string(&self, pos: usize, strings: &[(usize, usize)]) -> bool {
        strings
            .iter()
            .any(|(start, end)| pos >= *start && pos < *end)
    }
}

impl Default for SelfwareHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter for SelfwareHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled = StyledText::new();

        if line.is_empty() {
            return styled;
        }

        // Find all string positions
        let strings = self.find_strings(line);

        // Simpler approach: style the whole line based on first word
        let first_word = line.split_whitespace().next().unwrap_or("");

        if self.is_command(first_word) {
            // Command with arguments
            let cmd_end = first_word.len();
            styled.push((self.command_style, first_word.to_string()));

            if line.len() > cmd_end {
                let rest = &line[cmd_end..];
                // Check if the rest contains a path
                if rest.trim().contains('/') || rest.trim().starts_with('.') {
                    styled.push((self.path_style, rest.to_string()));
                } else {
                    styled.push((self.default_style, rest.to_string()));
                }
            }
        } else if first_word == "exit" || first_word == "quit" {
            styled.push((self.keyword_style, line.to_string()));
        } else {
            // Check for strings and highlight
            if !strings.is_empty() {
                let mut pos = 0;
                for (start, end) in &strings {
                    if *start > pos {
                        styled.push((self.default_style, line[pos..*start].to_string()));
                    }
                    styled.push((self.string_style, line[*start..*end].to_string()));
                    pos = *end;
                }
                if pos < line.len() {
                    styled.push((self.default_style, line[pos..].to_string()));
                }
            } else {
                styled.push((self.default_style, line.to_string()));
            }
        }

        styled
    }
}

#[cfg(test)]
#[path = "../../tests/unit/input/highlighter/highlighter_test.rs"]
mod tests;
