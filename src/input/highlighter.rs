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
    /// Style for tool names
    #[allow(dead_code)] // For future tool highlighting
    tool_style: Style,
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
            // Copper for tools
            tool_style: Style::new().fg(Color::Rgb(184, 115, 51)).bold(),
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
            && matches!(
                word,
                "/help"
                    | "/status"
                    | "/stats"
                    | "/mode"
                    | "/ctx"
                    | "/context"
                    | "/compress"
                    | "/memory"
                    | "/clear"
                    | "/tools"
                    | "/analyze"
                    | "/review"
                    | "/plan"
                    | "/diff"
                    | "/git"
                    | "/undo"
                    | "/cost"
                    | "/model"
                    | "/compact"
                    | "/verbose"
                    | "/config"
                    | "/garden"
                    | "/journal"
                    | "/palette"
                    | "/vim"
                    | "/copy"
                    | "/restore"
                    | "/chat"
                    | "/theme"
            )
    }

    /// Check if a word looks like a path
    #[allow(dead_code)] // For future path highlighting
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
    #[allow(dead_code)] // For future keyword highlighting
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
    #[allow(dead_code)] // For future string context detection
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
mod tests {
    use super::*;

    #[test]
    fn test_highlighter_creation() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_command("/help"));
        assert!(!h.is_command("help"));
    }

    #[test]
    fn test_highlighter_default() {
        let h = SelfwareHighlighter::default();
        assert!(h.is_command("/status"));
    }

    #[test]
    fn test_all_commands_recognized() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_command("/help"));
        assert!(h.is_command("/status"));
        assert!(h.is_command("/stats"));
        assert!(h.is_command("/mode"));
        assert!(h.is_command("/ctx"));
        assert!(h.is_command("/context"));
        assert!(h.is_command("/compress"));
        assert!(h.is_command("/memory"));
        assert!(h.is_command("/clear"));
        assert!(h.is_command("/tools"));
        assert!(h.is_command("/analyze"));
        assert!(h.is_command("/review"));
        assert!(h.is_command("/plan"));
        assert!(h.is_command("/diff"));
        assert!(h.is_command("/git"));
        assert!(h.is_command("/undo"));
        assert!(h.is_command("/cost"));
        assert!(h.is_command("/model"));
        assert!(h.is_command("/compact"));
        assert!(h.is_command("/verbose"));
        assert!(h.is_command("/config"));
        assert!(h.is_command("/garden"));
        assert!(h.is_command("/journal"));
        assert!(h.is_command("/palette"));
    }

    #[test]
    fn test_invalid_commands() {
        let h = SelfwareHighlighter::new();
        assert!(!h.is_command("/unknown"));
        assert!(!h.is_command("help"));
        assert!(!h.is_command(""));
        assert!(!h.is_command("//help"));
    }

    #[test]
    fn test_path_detection() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_path("./src/main.rs"));
        assert!(h.is_path("config.toml"));
        assert!(h.is_path("~/projects"));
        assert!(!h.is_path("hello"));
    }

    #[test]
    fn test_path_detection_extensions() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_path("main.rs"));
        assert!(h.is_path("script.py"));
        assert!(h.is_path("app.js"));
        assert!(h.is_path("component.ts"));
        assert!(h.is_path("config.toml"));
        assert!(h.is_path("data.json"));
        assert!(h.is_path("README.md"));
    }

    #[test]
    fn test_path_detection_prefixes() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_path("./file"));
        assert!(h.is_path("../parent"));
        assert!(h.is_path("~/home"));
        assert!(h.is_path("/absolute/path"));
    }

    #[test]
    fn test_keyword_detection() {
        let h = SelfwareHighlighter::new();
        assert!(h.is_keyword("exit"));
        assert!(h.is_keyword("quit"));
        assert!(h.is_keyword("help"));
        assert!(h.is_keyword("yes"));
        assert!(h.is_keyword("no"));
        assert!(h.is_keyword("true"));
        assert!(h.is_keyword("false"));
        // Case insensitive
        assert!(h.is_keyword("EXIT"));
        assert!(h.is_keyword("True"));
    }

    #[test]
    fn test_string_finding() {
        let h = SelfwareHighlighter::new();
        let strings = h.find_strings(r#"hello "world" and 'test'"#);
        assert_eq!(strings.len(), 2);
    }

    #[test]
    fn test_string_finding_double_quotes() {
        let h = SelfwareHighlighter::new();
        let strings = h.find_strings(r#""hello world""#);
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0], (0, 13));
    }

    #[test]
    fn test_string_finding_single_quotes() {
        let h = SelfwareHighlighter::new();
        let strings = h.find_strings("'hello world'");
        assert_eq!(strings.len(), 1);
        assert_eq!(strings[0], (0, 13));
    }

    #[test]
    fn test_string_finding_unclosed() {
        let h = SelfwareHighlighter::new();
        let line = r#"hello "unclosed"#;
        let strings = h.find_strings(line);
        assert_eq!(strings.len(), 1);
        // Should extend to end of line
        assert_eq!(strings[0], (6, line.len()));
    }

    #[test]
    fn test_string_finding_empty() {
        let h = SelfwareHighlighter::new();
        let strings = h.find_strings("no strings here");
        assert!(strings.is_empty());
    }

    #[test]
    fn test_string_finding_adjacent() {
        let h = SelfwareHighlighter::new();
        let strings = h.find_strings(r#""first""second""#);
        assert_eq!(strings.len(), 2);
    }

    #[test]
    fn test_in_string() {
        let h = SelfwareHighlighter::new();
        let strings = vec![(5, 10), (15, 20)];

        assert!(!h.in_string(0, &strings));
        assert!(!h.in_string(4, &strings));
        assert!(h.in_string(5, &strings));
        assert!(h.in_string(7, &strings));
        assert!(h.in_string(9, &strings));
        assert!(!h.in_string(10, &strings));
        assert!(!h.in_string(14, &strings));
        assert!(h.in_string(15, &strings));
    }

    #[test]
    fn test_highlight_command() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("/help", 0);
        assert!(!styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_command_with_path() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("/analyze ./src", 0);
        assert!(!styled.buffer.is_empty());
        // Should have at least 2 parts (command and path)
        assert!(styled.buffer.len() >= 2);
    }

    #[test]
    fn test_highlight_exit() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("exit", 0);
        assert!(!styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_quit() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("quit", 0);
        assert!(!styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_empty() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("", 0);
        assert!(styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_with_string() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight(r#"echo "hello world""#, 0);
        assert!(!styled.buffer.is_empty());
    }

    #[test]
    fn test_highlight_plain_text() {
        let h = SelfwareHighlighter::new();
        let styled = h.highlight("hello world", 0);
        assert!(!styled.buffer.is_empty());
        assert_eq!(styled.buffer.len(), 1);
    }
}
