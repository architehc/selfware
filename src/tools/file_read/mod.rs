//! File read tool - reads file contents with optional line range selection.

use crate::config::SafetyConfig;
use crate::errors::ToolError;
use crate::safety::path_validator::PathValidator;
use crate::tools::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

pub mod prompt;

/// Global safety configuration set at startup from the user-loaded config.
pub(super) static SAFETY_CONFIG: OnceLock<RwLock<SafetyConfig>> = OnceLock::new();

/// Maximum file size for reads (50 MB) to prevent OOM from accidentally reading huge files.
const MAX_READ_SIZE: u64 = 50 * 1024 * 1024;

/// Read file contents. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileRead::with_safety_config`].
#[derive(Default)]
pub struct FileRead {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    /// When `None`, falls back to the global or default config (backward compatible).
    pub safety_config: Option<SafetyConfig>,
}

impl FileRead {
    /// Create a new FileRead tool with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new FileRead tool with the specified safety configuration.
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

#[async_trait]
impl Tool for FileRead {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        // Use the rich description from prompt module
        "Read file contents. Use for examining code, configs, or any text file. \
         Supports optional line_range parameter to read specific sections."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file"
                },
                "line_range": {
                    "type": "array",
                    "items": {"type": "integer"},
                    "minItems": 2,
                    "maxItems": 2,
                    "description": "Optional [start, end] line range (1-indexed, inclusive)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            line_range: Option<(usize, usize)>,
        }

        let args: Args = serde_json::from_value(args)?;
        let safety = resolve_safety_config(self.safety_config.as_ref());
        validate_tool_path(&args.path, &safety)?;
        let path = PathBuf::from(&args.path);

        // Check file size before reading to prevent OOM on huge files
        if let Ok(metadata) = tokio::fs::metadata(&path).await {
            if metadata.len() > MAX_READ_SIZE {
                return Err(ToolError::FileTooLarge {
                    size: metadata.len(),
                    limit: MAX_READ_SIZE,
                }
                .into());
            }
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .with_context(|| format!("Failed to read file: {}", args.path))?;

        let total_lines = content.lines().count();
        let (start, end) = args.line_range.unwrap_or((1, total_lines));

        let selected_content: String = content
            .lines()
            .skip(start.saturating_sub(1))
            .take(end.saturating_sub(start) + 1)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(serde_json::json!({
            "content": selected_content,
            "total_lines": total_lines,
            "truncated": args.line_range.is_some(),
            "encoding": "utf-8"
        }))
    }
}

/// Resolve which `SafetyConfig` to use for path validation.
fn resolve_safety_config(instance_config: Option<&SafetyConfig>) -> SafetyConfig {
    if let Some(cfg) = instance_config {
        return cfg.clone();
    }
    SAFETY_CONFIG
        .get()
        .and_then(|lock| lock.read().ok().map(|guard| guard.clone()))
        .unwrap_or_default()
}

/// Validate that a tool path is safe to access.
fn validate_tool_path(path: &str, config: &SafetyConfig) -> Result<()> {
    #[cfg(test)]
    {
        if std::env::var("SELFWARE_TEST_MODE").is_ok() {
            if !path.starts_with("tests/e2e-projects/") && !path.starts_with("/tmp/selfware-test-")
            {
                anyhow::bail!("Test mode only valid for test fixtures, got: {}", path);
            }
            return Ok(());
        }
    }
    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    PathValidator::new(config, working_dir)
        .validate(path)
        .map_err(|e| anyhow::anyhow!(e))
}

/// Register the runtime-loaded safety configuration for tool path validation.
///
/// DEPRECATED: Use `FileRead::with_safety_config()` for per-instance configs.
pub fn init_safety_config(config: &SafetyConfig) {
    let lock = SAFETY_CONFIG.get_or_init(|| RwLock::new(config.clone()));
    if let Ok(mut guard) = lock.write() {
        *guard = config.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn permissive_safety_config() -> SafetyConfig {
        SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..SafetyConfig::default()
        }
    }

    #[test]
    fn test_file_read_name() {
        let tool = FileRead::new();
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_file_read_description() {
        let tool = FileRead::new();
        assert!(tool.description().contains("Read file"));
    }

    #[test]
    fn test_file_read_schema() {
        let tool = FileRead::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[tokio::test]
    async fn test_file_read_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 3);
        assert!(result["content"].as_str().unwrap().contains("line1"));
    }

    #[tokio::test]
    async fn test_file_read_with_line_range() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [2, 4]
        });

        let result = tool.execute(args).await.unwrap();
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("line2"));
        assert!(content.contains("line4"));
        assert!(!content.contains("line1"));
    }

    #[tokio::test]
    async fn test_file_read_not_found() {
        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": "/nonexistent/file.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 0);
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_with_unicode() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("unicode.txt");
        fs::write(&file_path, "日本語\n한국어\n中文").unwrap();

        let tool = FileRead::with_safety_config(permissive_safety_config());
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 3);
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("日本語"));
    }
}
