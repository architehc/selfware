use super::file::validate_tool_path;
use super::Tool;
use crate::api::ApiClient;
use crate::config::SafetyConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::fs;

/// A tool that uses Fill-in-the-Middle (FIM) to intelligently edit code.
/// Supports optional per-instance safety configuration for multi-agent
/// scenarios via [`FileFimEdit::with_safety_config`].
pub struct FileFimEdit {
    client: Arc<ApiClient>,
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    /// When `None`, falls back to the global or default config (backward compatible).
    pub safety_config: Option<SafetyConfig>,
}

impl FileFimEdit {
    pub fn new(client: Arc<ApiClient>) -> Self {
        Self {
            client,
            safety_config: None,
        }
    }

    pub fn with_safety_config(client: Arc<ApiClient>, config: SafetyConfig) -> Self {
        Self {
            client,
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for FileFimEdit {
    fn name(&self) -> &str {
        "file_fim_edit"
    }

    fn description(&self) -> &str {
        "Use intelligent Fill-in-the-Middle (FIM) to replace a block of code. Provide path, start_line, and end_line of the block to replace, and the instruction of what should go there. The AI will intelligently generate the middle part based on context."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "start_line": {"type": "integer", "description": "1-based starting line number to replace"},
                "end_line": {"type": "integer", "description": "1-based ending line number to replace"},
                "instruction": {"type": "string", "description": "What should the model generate to replace this block?"}
            },
            "required": ["path", "start_line", "end_line", "instruction"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path = args["path"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing path"))?;
        let start_line = args["start_line"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing start_line"))? as usize;
        let end_line = args["end_line"]
            .as_u64()
            .ok_or_else(|| anyhow!("Missing end_line"))? as usize;
        let instruction = args["instruction"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing instruction"))?;

        // Validate path safety BEFORE any file I/O
        validate_tool_path(path, self.safety_config.as_ref())?;

        let content = fs::read_to_string(path).await?;
        let lines: Vec<&str> = content.lines().collect();

        if start_line == 0
            || start_line > lines.len()
            || end_line < start_line
            || end_line > lines.len()
        {
            return Err(anyhow!("Invalid line range"));
        }

        let prefix = lines[..start_line - 1].join(
            "
",
        );
        let suffix = lines[end_line..].join(
            "
",
        );

        // Format prompt using Qwen's specific FIM tokens (or standard FIM)
        // Qwen 2.5 Coder uses <|fim_prefix|>, <|fim_suffix|>, <|fim_middle|>
        // We inject the instruction as a comment at the very end of prefix if needed, or rely on model completion.
        let prompt = format!(
            "<|fim_prefix|>{}
// Instruction: {}
<|fim_suffix|>{}
<|fim_middle|>",
            prefix, instruction, suffix
        );

        let response = self
            .client
            .completion(
                &prompt,
                Some(2048),
                Some(vec![
                    "<|file_separator|>".to_string(),
                    "<|endoftext|>".to_string(),
                ]),
            )
            .await?;

        let middle = response
            .choices
            .first()
            .map(|c| c.text.clone())
            .unwrap_or_default();

        let new_content = format!("{}{}{}", prefix, middle, suffix);
        fs::write(path, new_content).await?;

        Ok(serde_json::json!({
            "status": "success",
            "message": format!("Successfully replaced lines {}-{} using FIM.", start_line, end_line)
        }))
    }
}
