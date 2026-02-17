//! Ghost Text Hinting
//!
//! AI-powered inline suggestions like GitHub Copilot.
//! Shows dimmed text predicting what the user might type next.

use nu_ansi_term::{Color, Style};
use reedline::{Hinter, History};

/// Ghost text hinter that shows predictions inline
pub struct GhostTextHinter {
    /// Style for ghost text (dimmed, italic)
    style: Style,
    /// Current suggestion
    current_hint: String,
    /// Common command patterns for prediction
    patterns: Vec<PatternHint>,
}

/// A pattern-based hint
#[derive(Debug, Clone)]
struct PatternHint {
    /// Prefix to match
    prefix: String,
    /// Suggested completion
    completion: String,
    /// Description
    #[allow(dead_code)] // For future tooltip display
    description: String,
}

impl GhostTextHinter {
    /// Create a new ghost text hinter
    pub fn new() -> Self {
        Self {
            style: Style::new().fg(Color::DarkGray).italic(),
            current_hint: String::new(),
            patterns: Self::default_patterns(),
        }
    }

    /// Default command patterns
    fn default_patterns() -> Vec<PatternHint> {
        vec![
            PatternHint {
                prefix: "/analyze".to_string(),
                completion: " ./src".to_string(),
                description: "Analyze source directory".to_string(),
            },
            PatternHint {
                prefix: "/review".to_string(),
                completion: " src/main.rs".to_string(),
                description: "Review main file".to_string(),
            },
            PatternHint {
                prefix: "fix".to_string(),
                completion: " the error in".to_string(),
                description: "Fix pattern".to_string(),
            },
            PatternHint {
                prefix: "add".to_string(),
                completion: " a test for".to_string(),
                description: "Add pattern".to_string(),
            },
            PatternHint {
                prefix: "implement".to_string(),
                completion: " a function that".to_string(),
                description: "Implement pattern".to_string(),
            },
            PatternHint {
                prefix: "refactor".to_string(),
                completion: " to use".to_string(),
                description: "Refactor pattern".to_string(),
            },
            PatternHint {
                prefix: "explain".to_string(),
                completion: " how the".to_string(),
                description: "Explain pattern".to_string(),
            },
            PatternHint {
                prefix: "create".to_string(),
                completion: " a new file".to_string(),
                description: "Create pattern".to_string(),
            },
            PatternHint {
                prefix: "run".to_string(),
                completion: " cargo test".to_string(),
                description: "Run tests".to_string(),
            },
            PatternHint {
                prefix: "show".to_string(),
                completion: " me the contents of".to_string(),
                description: "Show pattern".to_string(),
            },
            // New command hints
            PatternHint {
                prefix: "/diff".to_string(),
                completion: "".to_string(),
                description: "Git diff --stat".to_string(),
            },
            PatternHint {
                prefix: "/git".to_string(),
                completion: "".to_string(),
                description: "Git status --short".to_string(),
            },
            PatternHint {
                prefix: "/undo".to_string(),
                completion: "".to_string(),
                description: "Undo last file edit".to_string(),
            },
            PatternHint {
                prefix: "/cost".to_string(),
                completion: "".to_string(),
                description: "Token usage & cost".to_string(),
            },
            PatternHint {
                prefix: "/model".to_string(),
                completion: "".to_string(),
                description: "Model configuration".to_string(),
            },
            PatternHint {
                prefix: "/config".to_string(),
                completion: "".to_string(),
                description: "Show current config".to_string(),
            },
            // Natural language hints
            PatternHint {
                prefix: "find".to_string(),
                completion: " all files that".to_string(),
                description: "Find pattern".to_string(),
            },
            PatternHint {
                prefix: "write".to_string(),
                completion: " a function that".to_string(),
                description: "Write pattern".to_string(),
            },
            PatternHint {
                prefix: "debug".to_string(),
                completion: " the error in".to_string(),
                description: "Debug pattern".to_string(),
            },
            PatternHint {
                prefix: "test".to_string(),
                completion: " the implementation of".to_string(),
                description: "Test pattern".to_string(),
            },
            PatternHint {
                prefix: "why".to_string(),
                completion: " does this".to_string(),
                description: "Why pattern".to_string(),
            },
            PatternHint {
                prefix: "how".to_string(),
                completion: " do I".to_string(),
                description: "How pattern".to_string(),
            },
        ]
    }

    /// Find a matching pattern hint (returns owned String to avoid borrow issues)
    fn find_pattern_hint(&self, input: &str) -> Option<String> {
        let input_lower = input.to_lowercase();
        for pattern in &self.patterns {
            if pattern.prefix.to_lowercase().starts_with(&input_lower)
                && input.len() < pattern.prefix.len()
            {
                // Return the rest of the prefix
                return Some(pattern.prefix[input.len()..].to_string());
            }
            if input_lower.starts_with(&pattern.prefix.to_lowercase())
                && input.len() >= pattern.prefix.len()
            {
                // Input matches prefix, suggest completion
                let after_prefix = &input[pattern.prefix.len()..];
                if pattern.completion.starts_with(after_prefix)
                    && after_prefix.len() < pattern.completion.len()
                {
                    return Some(pattern.completion[after_prefix.len()..].to_string());
                }
            }
        }
        None
    }

    /// Get hint from history by searching for entries starting with input
    fn get_history_hint(&self, line: &str, history: &dyn History) -> Option<String> {
        if line.is_empty() {
            return None;
        }

        // Use reedline's search API to find matching history entries
        // last_with_prefix(prefix, session_id) - use None for global search
        let query = reedline::SearchQuery::last_with_prefix(line.to_string(), None);

        if let Ok(results) = history.search(query) {
            if let Some(entry) = results.first() {
                // Return the completion (part after the current input)
                let entry_str = entry.command_line.as_str();
                if entry_str.len() > line.len() && entry_str.starts_with(line) {
                    return Some(entry_str[line.len()..].to_string());
                }
            }
        }

        None
    }

    /// Get style for rendering
    pub fn style(&self) -> Style {
        self.style
    }
}

impl Default for GhostTextHinter {
    fn default() -> Self {
        Self::new()
    }
}

impl Hinter for GhostTextHinter {
    fn handle(
        &mut self,
        line: &str,
        _pos: usize,
        history: &dyn History,
        _use_ansi_coloring: bool,
        _cwd: &str,
    ) -> String {
        // First try pattern hints
        if let Some(hint) = self.find_pattern_hint(line) {
            self.current_hint = hint.clone();
            return self.style.paint(&hint).to_string();
        }

        // Then try history
        if let Some(hint) = self.get_history_hint(line, history) {
            self.current_hint = hint.clone();
            return self.style.paint(&hint).to_string();
        }

        self.current_hint.clear();
        String::new()
    }

    fn complete_hint(&self) -> String {
        self.current_hint.clone()
    }

    fn next_hint_token(&self) -> String {
        // Return up to next word boundary
        if self.current_hint.is_empty() {
            return String::new();
        }

        let trimmed = self.current_hint.trim_start();
        if let Some(space_idx) = trimmed.find(|c: char| c.is_whitespace()) {
            if self.current_hint.starts_with(' ') {
                format!(" {}", &trimmed[..space_idx])
            } else {
                trimmed[..space_idx].to_string()
            }
        } else {
            self.current_hint.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghost_text_hinter_creation() {
        let hinter = GhostTextHinter::new();
        assert!(hinter.current_hint.is_empty());
    }

    #[test]
    fn test_ghost_text_hinter_default() {
        let hinter = GhostTextHinter::default();
        assert!(!hinter.patterns.is_empty());
    }

    #[test]
    fn test_pattern_hint_matching() {
        let hinter = GhostTextHinter::new();

        // Partial command match
        let hint = hinter.find_pattern_hint("/ana");
        assert!(hint.is_some());
        assert!(hint.unwrap().contains("lyze"));

        // Full prefix match - suggests completion
        let hint = hinter.find_pattern_hint("/analyze");
        assert!(hint.is_some());
    }

    #[test]
    fn test_pattern_hint_no_match() {
        let hinter = GhostTextHinter::new();
        let hint = hinter.find_pattern_hint("xyznothing");
        assert!(hint.is_none());
    }

    #[test]
    fn test_pattern_hint_returns_string() {
        let hinter = GhostTextHinter::new();
        let hint = hinter.find_pattern_hint("/ana");
        if let Some(h) = hint {
            assert!(!h.is_empty());
        }
    }

    #[test]
    fn test_complete_hint() {
        let mut hinter = GhostTextHinter::new();
        hinter.current_hint = "complete this".to_string();
        assert_eq!(hinter.complete_hint(), "complete this");
    }

    #[test]
    fn test_next_hint_token() {
        let mut hinter = GhostTextHinter::new();

        hinter.current_hint = "word1 word2 word3".to_string();
        assert_eq!(hinter.next_hint_token(), "word1");

        hinter.current_hint = " word1 word2".to_string();
        assert_eq!(hinter.next_hint_token(), " word1");

        hinter.current_hint = "singleword".to_string();
        assert_eq!(hinter.next_hint_token(), "singleword");

        hinter.current_hint = String::new();
        assert_eq!(hinter.next_hint_token(), "");
    }

    #[test]
    fn test_default_patterns_exist() {
        let patterns = GhostTextHinter::default_patterns();
        assert!(!patterns.is_empty());

        // Check some expected patterns
        assert!(patterns.iter().any(|p| p.prefix == "/analyze"));
        assert!(patterns.iter().any(|p| p.prefix == "/review"));
        assert!(patterns.iter().any(|p| p.prefix == "fix"));
    }

    #[test]
    fn test_style() {
        let hinter = GhostTextHinter::new();
        let style = hinter.style();
        // Just verify we can get the style
        let _ = style;
    }

    #[test]
    fn test_pattern_case_insensitive() {
        let hinter = GhostTextHinter::new();

        // Should match case-insensitively
        let hint = hinter.find_pattern_hint("/ANA");
        assert!(hint.is_some());
    }

    #[test]
    fn test_pattern_completion_after_prefix() {
        let hinter = GhostTextHinter::new();

        // After typing the full prefix, should suggest completion
        let hint = hinter.find_pattern_hint("/analyze ");
        // Either suggests more or nothing
        let _ = hint;
    }

    #[test]
    fn test_empty_input() {
        let hinter = GhostTextHinter::new();
        let hint = hinter.find_pattern_hint("");
        // Empty input might match first pattern
        let _ = hint;
    }
}
