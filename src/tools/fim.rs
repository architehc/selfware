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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::io::Write;

    /// Helper: build an ApiClient from a default Config for testing.
    /// The client is never used for real HTTP calls in these tests --
    /// all validation tests fail before reaching the API.
    fn test_client() -> Arc<ApiClient> {
        let config = Config::default();
        Arc::new(ApiClient::new(&config).expect("ApiClient::new should succeed with defaults"))
    }

    // ── Construction ─────────────────────────────────────────────────

    #[test]
    fn new_creates_instance_without_safety_config() {
        let client = test_client();
        let tool = FileFimEdit::new(client);
        assert!(tool.safety_config.is_none());
    }

    #[test]
    fn with_safety_config_stores_config() {
        let client = test_client();
        let config = SafetyConfig::default();
        let tool = FileFimEdit::with_safety_config(client, config);
        assert!(tool.safety_config.is_some());
    }

    #[test]
    fn with_safety_config_preserves_values() {
        let client = test_client();
        let config = SafetyConfig {
            strict_permissions: true,
            ..Default::default()
        };
        let tool = FileFimEdit::with_safety_config(client, config);
        assert!(tool.safety_config.as_ref().unwrap().strict_permissions);
    }

    // ── Tool trait: name() and description() ─────────────────────────

    #[test]
    fn name_returns_file_fim_edit() {
        let tool = FileFimEdit::new(test_client());
        assert_eq!(tool.name(), "file_fim_edit");
    }

    #[test]
    fn description_is_non_empty() {
        let tool = FileFimEdit::new(test_client());
        assert!(
            !tool.description().is_empty(),
            "description() must not be empty"
        );
    }

    #[test]
    fn description_mentions_fim() {
        let tool = FileFimEdit::new(test_client());
        let desc = tool.description().to_lowercase();
        assert!(
            desc.contains("fill-in-the-middle") || desc.contains("fim"),
            "description should mention FIM: {}",
            tool.description()
        );
    }

    // ── Schema validation ────────────────────────────────────────────

    #[test]
    fn schema_has_required_fields() {
        let tool = FileFimEdit::new(test_client());
        let schema = tool.schema();

        let required = schema["required"]
            .as_array()
            .expect("schema should have 'required' array");
        let required_strs: Vec<&str> = required.iter().filter_map(|v| v.as_str()).collect();

        assert!(
            required_strs.contains(&"path"),
            "required should include 'path'"
        );
        assert!(
            required_strs.contains(&"start_line"),
            "required should include 'start_line'"
        );
        assert!(
            required_strs.contains(&"end_line"),
            "required should include 'end_line'"
        );
        assert!(
            required_strs.contains(&"instruction"),
            "required should include 'instruction'"
        );
    }

    #[test]
    fn schema_type_is_object() {
        let tool = FileFimEdit::new(test_client());
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn schema_properties_exist() {
        let tool = FileFimEdit::new(test_client());
        let schema = tool.schema();
        let props = schema["properties"]
            .as_object()
            .expect("properties should be an object");
        assert!(props.contains_key("path"));
        assert!(props.contains_key("start_line"));
        assert!(props.contains_key("end_line"));
        assert!(props.contains_key("instruction"));
    }

    // ── Line range validation via execute() ──────────────────────────
    //
    // These tests create a real temp file and invoke execute() with
    // invalid line ranges. Validation fails *before* any API call, so
    // no network access is required.

    /// Create a temp file with known content and return its path as a String.
    fn temp_file_with_lines(lines: &[&str]) -> (tempfile::NamedTempFile, String) {
        let mut f = tempfile::NamedTempFile::new().expect("create temp file");
        for line in lines {
            writeln!(f, "{}", line).expect("write line");
        }
        f.flush().expect("flush");
        let path = f.path().to_string_lossy().into_owned();
        (f, path)
    }

    #[tokio::test]
    async fn execute_rejects_start_line_zero() {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
        let tool = FileFimEdit::new(test_client());
        let (_tmp, path) = temp_file_with_lines(&["line1", "line2", "line3"]);

        let args = serde_json::json!({
            "path": path,
            "start_line": 0,
            "end_line": 2,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "start_line=0 should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid line range"),
            "Expected 'Invalid line range', got: {}",
            err_msg
        );
    }

    #[tokio::test]
    async fn execute_rejects_end_line_before_start_line() {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
        let tool = FileFimEdit::new(test_client());
        let (_tmp, path) = temp_file_with_lines(&["line1", "line2", "line3"]);

        let args = serde_json::json!({
            "path": path,
            "start_line": 3,
            "end_line": 1,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "end_line < start_line should be rejected");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid line range"),
            "Expected 'Invalid line range'"
        );
    }

    #[tokio::test]
    async fn execute_rejects_lines_beyond_file_length() {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
        let tool = FileFimEdit::new(test_client());
        let (_tmp, path) = temp_file_with_lines(&["only_one_line"]);

        let args = serde_json::json!({
            "path": path,
            "start_line": 1,
            "end_line": 5,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(
            result.is_err(),
            "end_line beyond file length should be rejected"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid line range"),
            "Expected 'Invalid line range'"
        );
    }

    #[tokio::test]
    async fn execute_rejects_start_line_beyond_file_length() {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
        let tool = FileFimEdit::new(test_client());
        let (_tmp, path) = temp_file_with_lines(&["a", "b"]);

        let args = serde_json::json!({
            "path": path,
            "start_line": 10,
            "end_line": 12,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(
            result.is_err(),
            "start_line beyond file length should be rejected"
        );
    }

    #[tokio::test]
    async fn execute_rejects_missing_path() {
        let tool = FileFimEdit::new(test_client());
        let args = serde_json::json!({
            "start_line": 1,
            "end_line": 2,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "missing path should be rejected");
        assert!(
            result.unwrap_err().to_string().contains("Missing path"),
            "Expected 'Missing path'"
        );
    }

    #[tokio::test]
    async fn execute_rejects_missing_start_line() {
        let tool = FileFimEdit::new(test_client());
        let args = serde_json::json!({
            "path": "/tmp/test.txt",
            "end_line": 2,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "missing start_line should be rejected");
    }

    #[tokio::test]
    async fn execute_rejects_missing_end_line() {
        let tool = FileFimEdit::new(test_client());
        let args = serde_json::json!({
            "path": "/tmp/test.txt",
            "start_line": 1,
            "instruction": "test"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "missing end_line should be rejected");
    }

    #[tokio::test]
    async fn execute_rejects_missing_instruction() {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
        let tool = FileFimEdit::new(test_client());
        let (_tmp, path) = temp_file_with_lines(&["line1", "line2"]);

        let args = serde_json::json!({
            "path": path,
            "start_line": 1,
            "end_line": 2
        });

        let result = tool.execute(args).await;
        assert!(result.is_err(), "missing instruction should be rejected");
    }
}
