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
    #[allow(clippy::wrong_self_convention)]
    fn into_tool_prompt(&self) -> Option<&dyn ToolPrompt>;
}

/// Blanket implementation for Option<&dyn ToolPrompt>.
impl IntoToolPrompt for &dyn ToolPrompt {
    fn into_tool_prompt(&self) -> Option<&dyn ToolPrompt> {
        Some(*self)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/prompt/prompt_test.rs"]
mod tests;
