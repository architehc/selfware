//! Tool prompt trait for rich LLM-facing descriptions.
//!
//! This trait allows tools to provide rich descriptions, examples, and guidance
//! that helps LLMs understand WHEN and HOW to use each tool effectively.

use serde_json::Value;

/// Rich description of a tool example.
#[derive(Debug, Clone)]
pub struct ToolExample {
    /// Description of what this example demonstrates.
    pub description: String,
    /// Example input arguments as JSON.
    pub input: Value,
    /// Optional expected output description.
    pub output: Option<String>,
}

/// Trait for tools that provide rich LLM-facing descriptions.
///
/// Implementing this trait allows tools to provide:
/// - Detailed descriptions of what the tool does
/// - Guidance on when to use the tool
/// - Concrete examples of tool usage
/// - Important notes and caveats
///
/// This improves tool selection accuracy by helping the LLM understand
/// the context in which each tool should be used.
pub trait ToolPrompt: Send + Sync {
    /// Returns a detailed description of what this tool does.
    ///
    /// This should explain the tool's purpose clearly and include
    /// guidance on when to use it vs. alternative tools.
    fn description(&self) -> String;

    /// Returns guidance on when to use this tool.
    ///
    /// This helps the LLM understand the appropriate contexts
    /// for using this tool over alternatives.
    fn when_to_use(&self) -> String;

    /// Returns example usages of this tool.
    ///
    /// Examples should cover common use cases and demonstrate
    /// proper argument structure.
    fn examples(&self) -> Vec<ToolExample>;

    /// Returns important notes or caveats about using this tool.
    ///
    /// This is optional and can include warnings, limitations,
    /// or special considerations.
    fn important_notes(&self) -> Option<String>;
}

/// Extension trait for converting any Tool to a rich prompt-enabled tool.
///
/// This provides a bridge between the basic Tool trait and the
/// rich ToolPrompt trait.
pub trait IntoToolPrompt {
    /// Convert to a ToolPrompt if rich descriptions are available.
    fn into_tool_prompt(&self) -> Option<&dyn ToolPrompt>;
}

/// Blanket implementation for Option<&dyn ToolPrompt>.
impl IntoToolPrompt for &dyn ToolPrompt {
    fn into_tool_prompt(&self) -> Option<&dyn ToolPrompt> {
        Some(*self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockToolPrompt;

    impl ToolPrompt for MockToolPrompt {
        fn description(&self) -> String {
            "A mock tool for testing.".to_string()
        }

        fn when_to_use(&self) -> String {
            "Use this when testing the ToolPrompt trait.".to_string()
        }

        fn examples(&self) -> Vec<ToolExample> {
            vec![ToolExample {
                description: "Basic usage".to_string(),
                input: serde_json::json!({"arg": "value"}),
                output: Some("Result".to_string()),
            }]
        }

        fn important_notes(&self) -> Option<String> {
            Some("This is a test tool.".to_string())
        }
    }

    #[test]
    fn test_tool_prompt_methods() {
        let tool = MockToolPrompt;

        assert_eq!(tool.description(), "A mock tool for testing.");
        assert_eq!(
            tool.when_to_use(),
            "Use this when testing the ToolPrompt trait."
        );
        assert_eq!(tool.examples().len(), 1);
        assert_eq!(
            tool.important_notes(),
            Some("This is a test tool.".to_string())
        );
    }

    #[test]
    fn test_tool_example_structure() {
        let example = ToolExample {
            description: "Test example".to_string(),
            input: serde_json::json!({"key": "value"}),
            output: Some("Expected output".to_string()),
        };

        assert_eq!(example.description, "Test example");
        assert_eq!(example.input["key"], "value");
        assert_eq!(example.output, Some("Expected output".to_string()));
    }
}
