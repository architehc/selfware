//! Rich LLM-facing descriptions for the file_read tool.

use crate::tools::prompt::{ToolExample, ToolPrompt};

/// Rich prompt implementation for FileRead tool.
pub struct FileReadPrompt;

impl Default for FileReadPrompt {
    fn default() -> Self {
        Self::new()
    }
}

impl FileReadPrompt {
    /// Create a new FileReadPrompt instance.
    pub fn new() -> Self {
        Self
    }
}

impl ToolPrompt for FileReadPrompt {
    fn description(&self) -> String {
        "Read file contents. Use this to examine source code, configuration files, \
         documentation, and any text files. \\n\\n\
         This tool handles large files efficiently, supports line range selection, \
         and properly handles various text encodings. \\n\\n\
         **When to use:** Use this tool whenever you need to see what's in a file. \
         It's the primary way to understand existing code, read logs, or examine \
         configuration."
            .to_string()
    }

    fn when_to_use(&self) -> String {
        "Use file_read when:\\n\
         - You need to examine source code, configs, or documentation\\n\
         - You want to understand the structure of an existing file\\n\
         - You need to read error logs or output files\\n\
         - You want to verify the content of a file before or after editing\\n\\n\
         Prefer file_read over shell_exec for reading files because:\\n\
         - It handles large files without memory issues\\n\
         - It properly handles text encodings\\n\
         - It provides structured output with line counts\\n\
         - It supports reading specific line ranges"
            .to_string()
    }

    fn examples(&self) -> Vec<ToolExample> {
        vec![
            ToolExample {
                description: "Read an entire file to understand its contents".to_string(),
                input: serde_json::json!({
                    "path": "src/main.rs"
                }),
                output: Some(
                    "Returns the full file content with total line count. \
                     Use this when you need to understand the complete file."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Read a specific section of a file using line range".to_string(),
                input: serde_json::json!({
                    "path": "src/main.rs",
                    "line_range": [1, 50]
                }),
                output: Some(
                    "Returns only lines 1-50. Use this for large files when you \
                     only need to see a specific section, like imports or a particular function."
                        .to_string(),
                ),
            },
            ToolExample {
                description: "Read a configuration file".to_string(),
                input: serde_json::json!({
                    "path": "Cargo.toml"
                }),
                output: Some(
                    "Returns the configuration file content. Use this to understand \
                     project dependencies, settings, and metadata."
                        .to_string(),
                ),
            },
        ]
    }

    fn important_notes(&self) -> Option<String> {
        Some(
            "**Important Notes:**\\n\
             - Maximum file size: 50MB (returns error for larger files)\\n\
             - Line ranges are 1-indexed and inclusive: [1, 10] means lines 1 through 10\\n\
             - Returns UTF-8 encoded content; binary files may not read correctly\\n\
             - If the file doesn't exist, the tool will return an error\\n\
             - For very large files, consider using line_range to read specific sections"
                .to_string(),
        )
    }
}

/// Get the rich description for file_read tool.
pub fn get_description() -> String {
    FileReadPrompt::new().description()
}

/// Get the rich when_to_use guidance for file_read tool.
pub fn get_when_to_use() -> String {
    FileReadPrompt::new().when_to_use()
}

/// Get examples for file_read tool.
pub fn get_examples() -> Vec<ToolExample> {
    FileReadPrompt::new().examples()
}

/// Get important notes for file_read tool.
pub fn get_important_notes() -> Option<String> {
    FileReadPrompt::new().important_notes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_read_prompt_description() {
        let prompt = FileReadPrompt::new();
        let desc = prompt.description();
        assert!(desc.contains("Read file contents"));
        assert!(desc.contains("source code"));
    }

    #[test]
    fn test_file_read_prompt_when_to_use() {
        let prompt = FileReadPrompt::new();
        let when = prompt.when_to_use();
        assert!(when.contains("file_read"));
        assert!(when.contains("shell_exec"));
    }

    #[test]
    fn test_file_read_prompt_examples() {
        let prompt = FileReadPrompt::new();
        let examples = prompt.examples();
        assert_eq!(examples.len(), 3);

        // Check first example
        assert!(examples[0].description.contains("entire file"));
        assert!(examples[0].input.get("path").is_some());
    }

    #[test]
    fn test_file_read_prompt_notes() {
        let prompt = FileReadPrompt::new();
        let notes = prompt.important_notes();
        assert!(notes.is_some());
        assert!(notes.unwrap().contains("50MB"));
    }
}
