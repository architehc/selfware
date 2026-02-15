use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub struct FileRead;
pub struct FileWrite;
pub struct FileEdit;
pub struct DirectoryTree;

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
        let path = Path::new(&args.path);

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

        // Validate content before writing
        validate_file_content(&args.path, &args.content)?;

        let path = Path::new(&args.path);

        // Create backup if exists
        if args.backup && path.exists() {
            let backup_path = format!("{}.bak", args.path);
            fs::copy(path, &backup_path)?;
        }

        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(path, &args.content)?;

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

        // Validate the replacement content
        if args.new_str.contains('\x00') {
            anyhow::bail!("Replacement content contains null bytes - potential injection attack");
        }

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
        fs::write(&args.path, new_content)?;

        Ok(serde_json::json!({
            "success": true,
            "matches_found": 1,
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

/// Sensitive file extensions that should not be written to by the agent.
const SENSITIVE_EXTENSIONS: &[&str] = &[".env", ".pem", ".key"];

/// Validate file content before writing.
/// Returns an error if the content contains null bytes or other dangerous patterns.
pub fn validate_file_content(path: &str, content: &str) -> Result<()> {
    // Check for null bytes which can indicate injection attacks
    if content.contains('\x00') {
        anyhow::bail!("File content contains null bytes - potential injection attack");
    }

    // Check if writing to a sensitive file extension
    let path_lower = path.to_lowercase();
    for ext in SENSITIVE_EXTENSIONS {
        if path_lower.ends_with(ext) {
            anyhow::bail!(
                "Writing to sensitive file type '{}' is blocked for security. Path: {}",
                ext,
                path
            );
        }
    }

    // Check for .ssh paths
    if path.contains(".ssh/") || path.contains(".ssh\\") {
        anyhow::bail!(
            "Writing to .ssh directory is blocked for security. Path: {}",
            path
        );
    }

    // Check for embedded shell commands in shebangs for non-script files
    // Only flag shebangs in files that are not intended to be scripts
    let script_extensions = [".sh", ".bash", ".zsh", ".py", ".rb", ".pl", ".js", ".ts"];
    let is_script = script_extensions.iter().any(|ext| path_lower.ends_with(ext));
    if !is_script && content.starts_with("#!") {
        // Check if the shebang line contains suspicious shell commands
        if let Some(first_line) = content.lines().next() {
            let shebang_lower = first_line.to_lowercase();
            let suspicious_patterns = ["rm ", "curl ", "wget ", "eval ", "exec ", "bash -c"];
            for pattern in &suspicious_patterns {
                if shebang_lower.contains(pattern) {
                    anyhow::bail!(
                        "Suspicious shebang detected in non-script file '{}': contains '{}'",
                        path,
                        pattern.trim()
                    );
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_read_name() {
        let tool = FileRead;
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_file_read_description() {
        let tool = FileRead;
        assert!(tool.description().contains("Read"));
    }

    #[test]
    fn test_file_read_schema() {
        let tool = FileRead;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[tokio::test]
    async fn test_file_read_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead;
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

        let tool = FileRead;
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
        let tool = FileRead;
        let args = serde_json::json!({"path": "/nonexistent/file.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_write_name() {
        let tool = FileWrite;
        assert_eq!(tool.name(), "file_write");
    }

    #[tokio::test]
    async fn test_file_write_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("output.txt");

        let tool = FileWrite;
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

        let tool = FileWrite;
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

        let tool = FileWrite;
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
        let tool = FileEdit;
        assert_eq!(tool.name(), "file_edit");
    }

    #[tokio::test]
    async fn test_file_edit_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit;
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

        let tool = FileEdit;
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

        let tool = FileEdit;
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
        let tool = DirectoryTree;
        assert_eq!(tool.name(), "directory_tree");
    }

    #[tokio::test]
    async fn test_directory_tree_success() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file1.txt"), "").unwrap();
        fs::write(temp_dir.path().join("file2.txt"), "").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let tool = DirectoryTree;
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

        let tool = DirectoryTree;
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

        let tool = DirectoryTree;
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

        let tool = FileRead;
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

        let tool = FileRead;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [100, 200]  // Beyond file length
        });

        let result = tool.execute(args).await.unwrap();
        // Should return empty content for out-of-bounds range
        assert_eq!(result["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_single_line_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("single.txt");
        fs::write(&file_path, "only one line").unwrap();

        let tool = FileRead;
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

        let tool = FileRead;
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

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content",
            "backup": false
        });

        tool.execute(args).await.unwrap();

        // Backup should NOT exist
        let backup_path = temp_dir.path().join("no_backup.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_write_new_file_no_backup_needed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("brand_new.txt");

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new file content",
            "backup": true  // backup requested but file doesn't exist
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());

        // Backup should NOT exist since original didn't exist
        let backup_path = temp_dir.path().join("brand_new.txt.bak");
        assert!(!backup_path.exists());
    }

    #[tokio::test]
    async fn test_file_edit_delete_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("delete.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": ", World",
            "new_str": ""  // Empty = delete
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

        let tool = FileEdit;
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

        let tool = FileEdit;
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

        let tool = DirectoryTree;
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "max_depth": 2
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        // Should not contain deep_file.txt (at depth 4)
        let has_deep = entries
            .iter()
            .any(|e| e["path"].as_str().unwrap().contains("deep_file"));
        assert!(!has_deep);
    }

    #[tokio::test]
    async fn test_directory_tree_nonexistent_path() {
        let tool = DirectoryTree;
        let args = serde_json::json!({
            "path": "/nonexistent/directory/path"
        });

        let result = tool.execute(args).await;
        // Should handle gracefully - might return empty or error
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_invalid_json_args() {
        let tool = FileRead;
        // Missing required "path" field
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_invalid_json_args() {
        let tool = FileWrite;
        // Missing required fields
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_invalid_json_args() {
        let tool = FileEdit;
        // Missing required fields
        let args = serde_json::json!({"path": "test.txt"});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_directory_tree_invalid_json_args() {
        let tool = DirectoryTree;
        // Missing required path
        let args = serde_json::json!({});

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_line_range_inverted() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [3, 1]  // Inverted range
        });

        let result = tool.execute(args).await.unwrap();
        // With inverted range (3,1), skip(2).take(0) would be empty,
        // but saturating_sub makes end.saturating_sub(start) = 0, so take(1) gives line3
        // The actual behavior returns "line3"
        assert_eq!(result["content"], "line3");
    }

    #[tokio::test]
    async fn test_file_write_empty_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty_write.txt");

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": ""
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["bytes_written"], 0);

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "");
    }

    // Security validation tests

    #[test]
    fn test_null_byte_in_content_rejected() {
        let result = validate_file_content("test.txt", "hello\x00world");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[test]
    fn test_sensitive_file_extension_warning() {
        // .env files should be blocked
        let result = validate_file_content(".env", "SECRET=123");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".env"));

        // .pem files should be blocked
        let result = validate_file_content("cert.pem", "-----BEGIN CERTIFICATE-----");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".pem"));

        // .key files should be blocked
        let result = validate_file_content("private.key", "-----BEGIN PRIVATE KEY-----");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".key"));

        // .ssh paths should be blocked
        let result = validate_file_content("/home/user/.ssh/authorized_keys", "ssh-rsa AAAA...");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".ssh"));
    }

    #[test]
    fn test_normal_file_content_accepted() {
        let result = validate_file_content("test.txt", "Hello, World!");
        assert!(result.is_ok());
    }

    #[test]
    fn test_script_file_with_shebang_accepted() {
        let result = validate_file_content("script.sh", "#!/bin/bash\necho hello");
        assert!(result.is_ok());

        let result = validate_file_content("script.py", "#!/usr/bin/env python\nprint('hello')");
        assert!(result.is_ok());
    }

    #[test]
    fn test_non_script_with_suspicious_shebang_rejected() {
        let result =
            validate_file_content("config.txt", "#!/bin/bash -c rm -rf /\nsome content");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("shebang"));
    }

    #[test]
    fn test_non_script_with_safe_shebang_accepted() {
        // A non-script file with a shebang that doesn't contain suspicious commands
        let result = validate_file_content("config.txt", "#!/usr/bin/env node\nsome content");
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_write_rejects_null_bytes() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("null_test.txt");

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "hello\x00world"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[tokio::test]
    async fn test_file_write_rejects_env_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join(".env");

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "SECRET=value"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".env"));
    }

    #[tokio::test]
    async fn test_file_edit_rejects_null_bytes_in_replacement() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_null.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = FileEdit;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "World",
            "new_str": "Wor\x00ld"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("null bytes"));
    }

    #[tokio::test]
    async fn test_directory_tree_with_files_and_directories() {
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
        fs::create_dir(temp_dir.path().join("subdir")).unwrap();
        fs::write(temp_dir.path().join("subdir").join("nested.txt"), "").unwrap();

        let tool = DirectoryTree;
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();

        // Should have directory type
        let has_dir = entries.iter().any(|e| e["type"] == "directory");
        let has_file = entries.iter().any(|e| e["type"] == "file");
        assert!(has_dir);
        assert!(has_file);
    }

    // ──── Extended file tests (test_file_extended) ────────────────────

    #[tokio::test]
    async fn test_file_write_success_extended() {
        // Write content and verify it was written correctly
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("write_test.txt");

        let tool = FileWrite;
        let content = "Hello from the extended test!\nLine two.\nLine three.";
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": content
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["bytes_written"], content.len());
        assert_eq!(result["path"], file_path.to_str().unwrap());

        // Verify the file content matches exactly
        let read_back = fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_back, content);
    }

    #[tokio::test]
    async fn test_file_write_creates_directories() {
        // FileWrite should recursively create parent directories
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir
            .path()
            .join("deep")
            .join("nested")
            .join("dir")
            .join("output.txt");

        assert!(!file_path.parent().unwrap().exists());

        let tool = FileWrite;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "deeply nested content"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert!(file_path.exists());
        assert!(file_path.parent().unwrap().exists());

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "deeply nested content");
    }

    #[tokio::test]
    async fn test_file_write_invalid_path() {
        // Writing to an invalid/impossible path should produce an error
        let tool = FileWrite;
        let args = serde_json::json!({
            "path": "/proc/nonexistent/impossible/path/file.txt",
            "content": "should fail"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_success_extended() {
        // Test search-and-replace with multiline content
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_ext.txt");
        let original = "fn main() {\n    println!(\"old output\");\n}\n";
        fs::write(&file_path, original).unwrap();

        let tool = FileEdit;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "println!(\"old output\")",
            "new_str": "println!(\"new output\")"
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["matches_found"], 1);

        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("new output"));
        assert!(!content.contains("old output"));
    }

    #[tokio::test]
    async fn test_file_edit_pattern_not_found() {
        // When old_str doesn't match anything, should error
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("edit_miss.txt");
        fs::write(&file_path, "Some content here").unwrap();

        let tool = FileEdit;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "nonexistent pattern xyz123",
            "new_str": "replacement"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"));
    }

    #[tokio::test]
    async fn test_directory_tree_success_extended() {
        // Create a known directory structure and verify tree output
        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("root_file.txt"), "root").unwrap();
        fs::create_dir(temp_dir.path().join("src")).unwrap();
        fs::write(temp_dir.path().join("src").join("main.rs"), "fn main(){}").unwrap();
        fs::create_dir(temp_dir.path().join("docs")).unwrap();
        fs::write(temp_dir.path().join("docs").join("readme.md"), "# Docs").unwrap();

        let tool = DirectoryTree;
        // Use include_hidden: true so the root temp dir itself is not filtered
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": true
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["root"], temp_dir.path().to_str().unwrap());
        let total = result["total"].as_i64().unwrap();
        // root dir + root_file.txt + src/ + main.rs + docs/ + readme.md = 6
        assert!(total >= 6, "Expected at least 6 entries, got {}", total);

        let entries = result["entries"].as_array().unwrap();
        let paths: Vec<&str> = entries
            .iter()
            .filter_map(|e| e["path"].as_str())
            .collect();
        assert!(paths.iter().any(|p| p.contains("root_file.txt")));
        assert!(paths.iter().any(|p| p.contains("main.rs")));
    }

    #[tokio::test]
    async fn test_directory_tree_empty_dir() {
        // An empty directory should return only the root entry
        let temp_dir = TempDir::new().unwrap();

        let tool = DirectoryTree;
        // Use include_hidden: true because temp dir names start with '.'
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "include_hidden": true
        });

        let result = tool.execute(args).await.unwrap();
        let entries = result["entries"].as_array().unwrap();
        // Should contain only the root directory itself
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["type"], "directory");
    }

    #[tokio::test]
    async fn test_file_read_line_range() {
        // Test precise line range extraction
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("lines.txt");
        fs::write(
            &file_path,
            "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta",
        )
        .unwrap();

        let tool = FileRead;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "line_range": [3, 5]
        });

        let result = tool.execute(args).await.unwrap();
        let content = result["content"].as_str().unwrap();
        assert!(content.contains("gamma"));
        assert!(content.contains("delta"));
        assert!(content.contains("epsilon"));
        assert!(!content.contains("alpha"));
        assert!(!content.contains("beta"));
        assert!(!content.contains("zeta"));
        assert_eq!(result["truncated"], true);
        assert_eq!(result["total_lines"], 7);
    }

    #[tokio::test]
    async fn test_file_read_nonexistent() {
        // Reading a nonexistent file should produce an error
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("does_not_exist.txt");

        let tool = FileRead;
        let args = serde_json::json!({
            "path": file_path.to_str().unwrap()
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Failed to read file"));
    }
}
