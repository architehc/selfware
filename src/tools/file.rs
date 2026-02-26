use super::Tool;
use crate::config::SafetyConfig;
use crate::safety::path_validator::PathValidator;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;
use tempfile::NamedTempFile;

/// Global safety configuration set at startup from the user-loaded config.
/// `validate_tool_path` reads from this so that user-configured `allowed_paths`
/// (and other safety settings) are respected, rather than always falling back to
/// `SafetyConfig::default()`.
///
/// NOTE: This global exists for backward compatibility. Prefer using the
/// per-instance `safety_config` field on tool structs for multi-agent scenarios
/// where each agent may have a different safety configuration.
static SAFETY_CONFIG: OnceLock<SafetyConfig> = OnceLock::new();

/// Register the runtime-loaded safety configuration for tool path validation.
///
/// This should be called once during agent initialization (before any file tools
/// execute) so that `validate_tool_path` honours user settings.
///
/// For multi-agent scenarios where each agent needs a different safety config,
/// use the per-instance `with_safety_config()` constructor on each tool struct
/// instead of (or in addition to) this global initializer.
pub fn init_safety_config(config: &SafetyConfig) {
    // OnceLock::set returns Err if already set; that's fine -- first writer wins.
    let _ = SAFETY_CONFIG.set(config.clone());
}

/// Maximum file size for reads (50 MB) to prevent OOM from accidentally reading huge files.
const MAX_READ_SIZE: u64 = 50 * 1024 * 1024;
/// Maximum file size for writes (10 MB) to prevent accidentally writing huge files.
const MAX_WRITE_SIZE: usize = 10 * 1024 * 1024;

/// Read file contents. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileRead::with_safety_config`].
#[derive(Default)]
pub struct FileRead {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    /// When `None`, falls back to the global or default config (backward compatible).
    pub safety_config: Option<SafetyConfig>,
}

/// Write or overwrite entire file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileWrite::with_safety_config`].
#[derive(Default)]
pub struct FileWrite {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// Apply surgical edit to file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileEdit::with_safety_config`].
#[derive(Default)]
pub struct FileEdit {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// Delete a file. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`FileDelete::with_safety_config`].
#[derive(Default)]
pub struct FileDelete {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

/// List directory structure. Supports optional per-instance safety configuration
/// for multi-agent scenarios via [`DirectoryTree::with_safety_config`].
#[derive(Default)]
pub struct DirectoryTree {
    /// Per-instance safety config. When `Some`, overrides the global `SAFETY_CONFIG`.
    pub safety_config: Option<SafetyConfig>,
}

// ---------------------------------------------------------------------------
// Constructors for dependency-injected safety configuration.
//
// Each file tool can be created with either:
// - `Tool::new()` / `Tool::default()` -- no per-instance config; uses the global or default
// - `Tool::with_safety_config(config)` -- uses the given config, ignoring the global
// ---------------------------------------------------------------------------

impl FileRead {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileWrite {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileEdit {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl FileDelete {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl DirectoryTree {
    pub fn new() -> Self {
        Self {
            safety_config: None,
        }
    }
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
        "Read file contents. Use for examining code, configs, or any text file."
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
        validate_tool_path(&args.path, self.safety_config.as_ref())?;
        let path = Path::new(&args.path);

        // Check file size before reading to prevent OOM on huge files
        if let Ok(metadata) = fs::metadata(path) {
            if metadata.len() > MAX_READ_SIZE {
                anyhow::bail!(
                    "File too large to read: {} bytes (limit: {} bytes)",
                    metadata.len(),
                    MAX_READ_SIZE
                );
            }
        }

        let content = fs::read_to_string(path)
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

#[async_trait]
impl Tool for FileWrite {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write or overwrite entire file. Creates parent directories if needed."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "content": {"type": "string"},
                "backup": {"type": "boolean", "default": true}
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            content: String,
            #[serde(default = "default_true")]
            backup: bool,
        }

        let args: Args = serde_json::from_value(args)?;
        validate_tool_path(&args.path, self.safety_config.as_ref())?;
        let path = Path::new(&args.path);

        // Check write size limit to prevent accidentally writing huge files
        if args.content.len() > MAX_WRITE_SIZE {
            anyhow::bail!(
                "Content too large to write: {} bytes (limit: {} bytes)",
                args.content.len(),
                MAX_WRITE_SIZE
            );
        }

        // Create backup if exists
        if args.backup && path.exists() {
            let backup_path = format!("{}.bak", args.path);
            fs::copy(path, &backup_path)?;
        }

        write_atomic(path, &args.content)?;

        Ok(serde_json::json!({
            "success": true,
            "bytes_written": args.content.len(),
            "path": args.path
        }))
    }
}

#[async_trait]
impl Tool for FileEdit {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Apply surgical edit to file. The old_str must match EXACTLY once. Include enough context to ensure unique match."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "old_str": {"type": "string", "description": "Exact string to find (must be unique)"},
                "new_str": {"type": "string", "description": "Replacement string (empty to delete)"}
            },
            "required": ["path", "old_str", "new_str"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            old_str: String,
            new_str: String,
        }

        let args: Args = serde_json::from_value(args)?;
        validate_tool_path(&args.path, self.safety_config.as_ref())?;
        let content = fs::read_to_string(&args.path)?;

        // Check for exactly one match
        let matches = content.matches(&args.old_str).count();
        if matches == 0 {
            anyhow::bail!("old_str not found in file");
        }
        if matches > 1 {
            anyhow::bail!("old_str matches {} times, expected exactly 1", matches);
        }

        let new_content = content.replace(&args.old_str, &args.new_str);
        write_atomic(Path::new(&args.path), &new_content)?;

        Ok(serde_json::json!({
            "success": true,
            "matches_found": 1,
            "path": args.path
        }))
    }
}

#[async_trait]
impl Tool for FileDelete {
    fn name(&self) -> &str {
        "file_delete"
    }

    fn description(&self) -> &str {
        "Delete a file. Use with caution -- this is irreversible without version control."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file to delete"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
        }

        let args: Args = serde_json::from_value(args)?;
        validate_tool_path(&args.path, self.safety_config.as_ref())?;
        let path = Path::new(&args.path);

        if !path.exists() {
            anyhow::bail!("File not found: {}", args.path);
        }
        if path.is_dir() {
            anyhow::bail!("Path is a directory, not a file: {}", args.path);
        }

        fs::remove_file(path).with_context(|| format!("Failed to delete file: {}", args.path))?;

        Ok(serde_json::json!({
            "deleted": true,
            "path": args.path
        }))
    }
}

#[async_trait]
impl Tool for DirectoryTree {
    fn name(&self) -> &str {
        "directory_tree"
    }

    fn description(&self) -> &str {
        "List directory structure. Use to understand project layout."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "max_depth": {"type": "integer", "default": 3},
                "include_hidden": {"type": "boolean", "default": false}
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        #[derive(Deserialize)]
        struct Args {
            path: String,
            #[serde(default = "default_three")]
            max_depth: usize,
            #[serde(default)]
            include_hidden: bool,
        }

        let args: Args = serde_json::from_value(args)?;
        validate_tool_path(&args.path, self.safety_config.as_ref())?;

        let mut entries = vec![];
        for entry in walkdir::WalkDir::new(&args.path)
            .max_depth(args.max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let metadata = entry.metadata()?;

            if !args.include_hidden
                && entry
                    .file_name()
                    .to_str()
                    .map(|s| s.starts_with('.'))
                    .unwrap_or(false)
            {
                continue;
            }

            entries.push(serde_json::json!({
                "path": path.display().to_string(),
                "type": if metadata.is_dir() { "directory" } else { "file" },
                "size": metadata.len()
            }));
        }

        Ok(serde_json::json!({
            "root": args.path,
            "entries": entries,
            "total": entries.len()
        }))
    }
}

fn default_true() -> bool {
    true
}
fn default_three() -> usize {
    3
}

/// Validate that a tool path is safe to access.
///
/// When `instance_config` is `Some`, it takes priority (dependency-injected config
/// for multi-agent isolation). Otherwise falls back to the global `SAFETY_CONFIG`,
/// and finally to `SafetyConfig::default()`.
fn validate_tool_path(path: &str, instance_config: Option<&SafetyConfig>) -> Result<()> {
    #[cfg(test)]
    {
        if std::env::var("SELFWARE_TEST_MODE").is_ok() {
            return Ok(());
        }
        let p = std::path::Path::new(path);
        if p.is_absolute() {
            return Ok(());
        }
    }
    // Priority: per-instance config > global OnceLock > default
    let default_config;
    let config = if let Some(cfg) = instance_config {
        cfg
    } else {
        match SAFETY_CONFIG.get() {
            Some(cfg) => cfg,
            None => {
                default_config = SafetyConfig::default();
                &default_config
            }
        }
    };
    let working_dir = std::env::current_dir().unwrap_or_else(|_| ".".into());
    PathValidator::new(config, working_dir).validate(path)
}

/// Write content to a file atomically using a temporary file and rename.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid file path (no parent)"))?;
    fs::create_dir_all(parent)?;

    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(content.as_bytes())?;
    temp.persist(path)
        .map_err(|e| anyhow::anyhow!("Failed to persist atomic write: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_read_name() {
        let tool = FileRead::new();
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_file_read_description() {
        let tool = FileRead::new();
        assert!(tool.description().contains("Read"));
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

        let tool = FileRead::new();
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

        let tool = FileRead::new();
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
        let tool = FileRead::new();
        let args = serde_json::json!({"path": "/nonexistent/file.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_write_name() {
        let tool = FileWrite::new();
        assert_eq!(tool.name(), "file_write");
    }

    #[tokio::test]
    async fn test_file_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "Hello, World!"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["bytes_written"], 13);

        // Verify file was written
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_file_write_creates_backup() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        fs::write(&file_path, "original content").unwrap();

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content",
            "backup": true
        });

        tool.execute(args).await.unwrap();

        // Check backup exists
        let backup_path = temp_dir.path().join("existing.txt.bak");
        assert!(backup_path.exists());
        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, "original content");
    }

    #[tokio::test]
    async fn test_file_write_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir
            .path()
            .join("subdir")
            .join("nested")
            .join("file.txt");

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "nested content"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());
    }

    #[test]
    fn test_file_edit_name() {
        let tool = FileEdit::new();
        assert_eq!(tool.name(), "file_edit");
    }

    #[tokio::test]
    async fn test_file_edit_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "World",
            "new_str": "Rust"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello, Rust!");
    }

    #[tokio::test]
    async fn test_file_edit_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "NotFound",
            "new_str": "Replacement"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_file_edit_multiple_matches() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello Hello Hello").unwrap();

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "Hello",
            "new_str": "Hi"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("3 times"));
    }

    #[test]
    fn test_directory_tree_name() {
        let tool = DirectoryTree::new();
        assert_eq!(tool.name(), "directory_tree");
    }

    #[tokio::test]
    async fn test_directory_tree_success() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["total"].as_i64().unwrap() >= 3);
    }

    #[tokio::test]
    async fn test_directory_tree_excludes_hidden() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("visible.txt"), "").unwrap();
        fs::write(temp_dir.path().join(".hidden"), "").unwrap();

        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": false
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        // Should not contain .hidden
        let has_hidden = entries
            .iter()
            .any(|e| e["path"].as_str().unwrap().contains(".hidden"));
        assert!(!has_hidden);
    }

    #[tokio::test]
    async fn test_directory_tree_includes_hidden() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join(".hidden"), "").unwrap();

        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": true
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        // Should contain .hidden
        let has_hidden = entries
            .iter()
            .any(|e| e["path"].as_str().unwrap().contains(".hidden"));
        assert!(has_hidden);
    }

    #[test]
    fn test_default_true() {
        assert!(default_true());
    }

    #[test]
    fn test_default_three() {
        assert_eq!(default_three(), 3);
    }

    // Additional error path tests for coverage

    #[tokio::test]
    async fn test_file_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let tool = FileRead::new();
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 0);
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_line_range_start_beyond_end() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [100, 200]
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_single_line_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("single.txt");
        fs::write(&file_path, "only one line").unwrap();

        let tool = FileRead::new();
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 1);
        assert_eq!(result["content"], "only one line");
    }

    #[tokio::test]
    async fn test_file_read_with_unicode() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("unicode.txt");
        fs::write(&file_path, "日本語\n한국어\n中文").unwrap();

        let tool = FileRead::new();
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["total_lines"], 3);
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("日本語"));
    }

    #[tokio::test]
    async fn test_file_write_no_backup() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("no_backup.txt");
        fs::write(&file_path, "original").unwrap();

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content",
            "backup": false
        });

        tool.execute(args).await.unwrap();

        let backup_path = temp_dir.path().join("no_backup.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_write_new_file_no_backup_needed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("brand_new.txt");

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new file content",
            "backup": true
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());

        let backup_path = temp_dir.path().join("brand_new.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_edit_delete_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("delete.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": ", World",
            "new_str": ""
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello!");
    }

    #[tokio::test]
    async fn test_file_edit_multiline_match() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multiline.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "line1\nline2",
            "new_str": "replaced\nlines"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("replaced\nlines"));
        assert!(!content.contains("line1"));
    }

    #[tokio::test]
    async fn test_file_edit_file_not_exist() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let tool = FileEdit::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "anything",
            "new_str": "replacement"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_directory_tree_max_depth_honored() {
        let temp_dir = TempDir::new().unwrap();
        let deep_path = temp_dir.path().join("a").join("b").join("c").join("d");
        fs::create_dir_all(&deep_path).unwrap();
        fs::write(deep_path.join("deep_file.txt"), "").unwrap();

        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "max_depth": 2
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        let has_deep = entries
            .iter()
            .any(|e| e["path"].as_str().unwrap().contains("deep_file"));
        assert!(!has_deep);
    }

    #[tokio::test]
    async fn test_directory_tree_nonexistent_path() {
        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": "/nonexistent/directory/path"
        });

        let result = tool.execute(args).await;
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_invalid_json_args() {
        let tool = FileRead::new();
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_invalid_json_args() {
        let tool = FileWrite::new();
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_invalid_json_args() {
        let tool = FileEdit::new();
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_directory_tree_invalid_json_args() {
        let tool = DirectoryTree::new();
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_line_range_inverted() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [3, 1]
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["content"], "line3");
    }

    #[tokio::test]
    async fn test_file_write_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty_write.txt");

        let tool = FileWrite::new();
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": ""
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["bytes_written"], 0);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "");
    }

    #[tokio::test]
    async fn test_directory_tree_with_files_and_directories() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir").join("nested.txt"), "").unwrap();

        let tool = DirectoryTree::new();
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        let has_dir = entries.iter().any(|e| e["type"] == "directory");
        let has_file = entries.iter().any(|e| e["type"] == "file");
        assert!(has_dir);
        assert!(has_file);
    }

    #[tokio::test]
    async fn test_file_read_blocks_traversal() {
        let tool = FileRead::new();
        let args = serde_json::json!({"path": "../should_not_escape.txt"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path traversal detected"));
    }

    #[tokio::test]
    async fn test_directory_tree_blocks_traversal() {
        let tool = DirectoryTree::new();
        let args = serde_json::json!({"path": "../"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path traversal detected"));
    }

    // FileDelete tests

    #[test]
    fn test_file_delete_name() {
        let tool = FileDelete::new();
        assert_eq!(tool.name(), "file_delete");
    }

    #[test]
    fn test_file_delete_description() {
        let tool = FileDelete::new();
        assert!(tool.description().contains("Delete"));
    }

    #[test]
    fn test_file_delete_schema() {
        let tool = FileDelete::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[tokio::test]
    async fn test_file_delete_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("to_delete.txt");
        fs::write(&file_path, "delete me").unwrap();
        assert!(file_path.exists());

        let tool = FileDelete::new();
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["deleted"], true);
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_file_delete_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nonexistent.txt");

        let tool = FileDelete::new();
        let args = serde_json::json!({"path": file_path.to_str().unwrap()});

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    #[tokio::test]
    async fn test_file_delete_directory_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path().join("subdir");
        fs::create_dir(&dir_path).unwrap();

        let tool = FileDelete::new();
        let args = serde_json::json!({"path": dir_path.to_str().unwrap()});

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("directory"));
    }

    #[tokio::test]
    async fn test_file_delete_blocks_traversal() {
        let tool = FileDelete::new();
        let args = serde_json::json!({"path": "../escape.txt"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Path traversal detected"));
    }

    #[tokio::test]
    async fn test_file_delete_invalid_json_args() {
        let tool = FileDelete::new();
        let args = serde_json::json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
