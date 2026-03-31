//! Rich LLM-facing descriptions for the grep_search tool.

use crate::tools::prompt::{ToolExample, ToolPrompt};

/// Rich prompt implementation for GrepSearch tool.
pub struct GrepSearchPrompt;

impl Default for GrepSearchPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl GrepSearchPrompt {
    /// Create a new GrepSearchPrompt instance.
    pub fn new() -> Self {
        Self
    }
}

impl ToolPrompt for GrepSearchPrompt {
    fn description(&self) -> String {
        "Search file contents using regex patterns. Returns matching lines with context, \
         file paths, and line/column numbers. \\n\\n\
         This is the primary tool for finding code patterns, function usages, \
         error messages, or any text within files. It supports case-insensitive search, \
         file filtering, and returns context lines around each match."
            .to_string()
    }

    fn when_to_use(&self) -> String {
        "Use grep_search when:\\n\
         - Finding where a function, variable, or type is defined or used\\n\
         - Searching for error messages or log patterns\\n\
         - Finding examples of how to use a particular API\\n\
         - Locating configuration settings or constants\\n\
         - Searching for TODO, FIXME, or similar markers\\n\
         - Finding imports, includes, or dependencies\\n\\n\
         **Pattern selection:**\\n\
         - Use literal text for simple searches: \"my_function\"\\n\
         - Use regex for patterns: \"fn\\s+\\w+\\s*\\(\" to find function definitions\\n\
         - Use word boundaries: \"\\bTODO\\b\" to match whole word TODO\\n\\n\
         **Prefer other tools when:**\\n\
         - Finding files by name: Use glob_find instead\\n\
         - Finding Rust symbols (structs, traits): Use symbol_search instead\\n\
         - Reading file contents: Use file_read instead"
            .to_string()
    }

    fn examples(&self) -> Vec<ToolExample> {
        vec![
            ToolExample {
                description: "Find all occurrences of a function name".to_string(),
                input: serde_json::json!({
                    "pattern": "my_function",
                    "path": "src",
                    "recursive": true,
                    "context_lines": 2
                }),
                output: Some(
                    "Returns all lines containing 'my_function' with 2 lines of context \
                     before and after. Shows file path, line number, and column for each match."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Search for function definitions in Rust code".to_string(),
                input: serde_json::json!({
                    "pattern": "^\\s*(pub\\s+)?(async\\s+)?fn\\s+\\w+",
                    "path": "src",
                    "include": "*.rs",
                    "max_matches": 50
                }),
                output: Some(
                    "Finds Rust function definitions. The regex matches optional pub/async \
                     keywords followed by 'fn' and the function name. Filters to .rs files."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Case-insensitive search for error handling".to_string(),
                input: serde_json::json!({
                    "pattern": "unwrap|expect|panic",
                    "path": "src",
                    "case_insensitive": true,
                    "context_lines": 1
                }),
                output: Some(
                    "Finds all unwrap(), expect(), and panic!() calls regardless of case. \
                     Useful for finding potential panic points in code."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Find TODO markers in a specific file type".to_string(),
                input: serde_json::json!({
                    "pattern": "\\bTODO\\b|\\bFIXME\\b",
                    "path": ".",
                    "include": "*.rs",
                    "recursive": true
                }),
                output: Some(
                    "Finds all TODO and FIXME markers in Rust files. The \\b ensures \
                     word boundaries so it doesn't match 'TODO' inside other words."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Search with pagination for large result sets".to_string(),
                input: serde_json::json!({
                    "pattern": "println!",
                    "path": "src",
                    "max_matches": 20,
                    "offset": 0
                }),
                output: Some(
                    "Returns first 20 matches. Use offset: 20 to get the next page. \
                     Check the pagination.has_more field to see if more results exist."
                        .to_string(),
                ),
            },
        ]
    }

    fn important_notes(&self) -> Option<String> {
        Some(
            "**Important Notes:**\\n\
             - Uses regex patterns (not glob patterns) - escape special chars like . with \\\\ \\\\n\
             - Pattern length limit: 1000 characters\\n\
             - Compiled regex cache: 64 patterns (LRU eviction)\\n\
             - Max matches: 100 by default (use pagination for more)\\n\
             - Context lines: 2 by default (0-10 recommended)\\n\
             - Automatically skips: hidden files, /target/, /.git/, /node_modules/\\n\
             - Line/column numbers are 1-indexed\\n\
             - Binary files are skipped silently\\n\
             - Case-insensitive search adds (?i) prefix to regex"
                .to_string(),
        )
    }
}

/// Get the rich description for grep_search tool.
pub fn get_description() -> String {
    GrepSearchPrompt::new().description()
}

/// Get the rich when_to_use guidance for grep_search tool.
pub fn get_when_to_use() -> String {
    GrepSearchPrompt::new().when_to_use()
}

/// Get examples for grep_search tool.
pub fn get_examples() -> Vec<ToolExample> {
    GrepSearchPrompt::new().examples()
}

/// Get important notes for grep_search tool.
pub fn get_important_notes() -> Option<String> {
    GrepSearchPrompt::new().important_notes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grep_search_prompt_description() {
        let prompt = GrepSearchPrompt::new();
        let desc = prompt.description();
        assert!(desc.contains("regex"));
        assert!(desc.contains("context"));
    }

    #[test]
    fn test_grep_search_prompt_when_to_use() {
        let prompt = GrepSearchPrompt::new();
        let when = prompt.when_to_use();
        assert!(when.contains("function"));
        assert!(when.contains("glob_find"));
        assert!(when.contains("symbol_search"));
    }

    #[test]
    fn test_grep_search_prompt_examples() {
        let prompt = GrepSearchPrompt::new();
        let examples = prompt.examples();
        assert_eq!(examples.len(), 5);

        // Check TODO example
        let todo_example = &examples[3];
        assert!(todo_example.description.contains("TODO"));
        assert!(todo_example
            .input
            .get("pattern")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("TODO"));
    }

    #[test]
    fn test_grep_search_prompt_notes() {
        let prompt = GrepSearchPrompt::new();
        let notes = prompt.important_notes();
        assert!(notes.is_some());
        let notes_str = notes.unwrap();
        assert!(notes_str.contains("1000"));
        assert!(notes_str.contains("regex"));
    }
}
