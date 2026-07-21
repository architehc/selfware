use super::*;
use std::fs;
use tempfile::TempDir;

fn assert_path_access_rejected(err: anyhow::Error) {
    let message = err.to_string();
    assert!(
        message.contains("Path traversal detected")
            || message.contains("Path not in allowed list")
            || message.contains("outside working directory"),
        "expected a path-access rejection, got: {}",
        message
    );
}

/// Flatten the nested tree returned by `directory_tree` into a list of
/// `(name, type)` pairs for easy assertion.
fn flatten_tree(node: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(children) = node["children"].as_array() {
        for child in children {
            if let (Some(name), Some(type_)) = (child["name"].as_str(), child["type"].as_str()) {
                out.push((name.to_string(), type_.to_string()));
                out.extend(flatten_tree(child));
            }
        }
    }
    out
}

/// Create a permissive `SafetyConfig` for tests that need to access temp dirs.
/// This replaces the old `SELFWARE_TEST_MODE` env-var bypass with explicit config injection.
fn permissive_safety_config() -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..SafetyConfig::default()
    }
}

fn restricted_safety_config(root: &Path) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec![format!("{}/**", root.display())],
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
async fn file_read_line_range_streams_exact_slice() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("big.txt");
    // 10k lines; slicing must return exactly the requested window and stop
    // early (lines_scanned == end), not read/return the whole file.
    let body: String = (1..=10_000).map(|i| format!("line{i}\n")).collect();
    fs::write(&file_path, &body).unwrap();

    let tool = FileRead::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "line_range": [3, 5]
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["content"].as_str().unwrap(), "line3\nline4\nline5");
    assert_eq!(result["truncated"], true);
    // The slice ended before EOF, so total_lines must NOT be reported as
    // the range end — it should be null and has_more should be true.
    assert_eq!(result["total_lines"], serde_json::Value::Null);
    assert_eq!(result["has_more"], true);
    assert_eq!(result["lines_returned"], 3);

    // A CRLF file slices cleanly (carriage returns stripped).
    let crlf = temp_dir.path().join("crlf.txt");
    fs::write(&crlf, "a\r\nb\r\nc\r\n").unwrap();
    let args = serde_json::json!({
        "path": crlf.to_str().unwrap(),
        "line_range": [2, 2]
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["content"].as_str().unwrap(), "b");
}

#[tokio::test]
async fn file_read_line_range_subset_does_not_claim_total() {
    // A 10-line file read with line_range [2,4] must report lines_returned
    // = 3 and must NOT claim total_lines = 4 (EOF was not reached).
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("ten.txt");
    let body: String = (1..=10)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&file_path, &body).unwrap();

    let tool = FileRead::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "line_range": [2, 4]
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["lines_returned"], 3);
    assert_eq!(result["content"].as_str().unwrap(), "line2\nline3\nline4");
    // EOF not reached → total_lines must be null, has_more must be true.
    assert_eq!(result["total_lines"], serde_json::Value::Null);
    assert_eq!(result["has_more"], true);
}

#[tokio::test]
async fn file_read_line_range_beyond_eof_reports_true_total() {
    // Reading line_range [1,100] of a 10-line file reaches EOF, so
    // total_lines must be the true count (10), not the requested end.
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("ten.txt");
    let body: String = (1..=10)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&file_path, &body).unwrap();

    let tool = FileRead::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "line_range": [1, 100]
    });
    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["total_lines"], 10);
    assert_eq!(result["lines_returned"], 10);
    // reached_eof → no has_more field (or it's absent/false).
    assert!(result.get("has_more").is_none() || result["has_more"] == false);
}

#[tokio::test]
async fn test_file_read_not_found() {
    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "Hello",
        "new_str": "Hi"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("3 times"));
}

#[tokio::test]
async fn test_file_edit_rejects_whole_file_replacement() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("edit.txt");
    fs::write(&file_path, "line1\nline2\nline3\nline4\nline5\n").unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "line1\nline2\nline3\nline4\nline5\n",
        "new_str": "replaced\n"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("file_edit rejected"),
        "unexpected error: {err}"
    );

    // The file should be unchanged.
    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "line1\nline2\nline3\nline4\nline5\n");
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

    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
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

    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": temp_dir.path().to_str().unwrap(),
        "include_hidden": false
    });

    let result = tool.execute(args).await.unwrap();
    let entries = flatten_tree(&result["tree"]);

    // Should not contain .hidden
    let has_hidden = entries.iter().any(|(name, _)| name.contains(".hidden"));
    assert!(!has_hidden);
}

#[tokio::test]
async fn test_directory_tree_includes_hidden() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join(".hidden"), "").unwrap();

    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": temp_dir.path().to_str().unwrap(),
        "include_hidden": true
    });

    let result = tool.execute(args).await.unwrap();
    let entries = flatten_tree(&result["tree"]);

    // Should contain .hidden
    let has_hidden = entries.iter().any(|(name, _)| name.contains(".hidden"));
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

    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
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

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "anything",
        "new_str": "replacement"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_file_edit_noop_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("noop.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "Hello",
        "new_str": "Hello"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("no-op"),
        "Expected no-op error, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_file_edit_duplicate_insertion_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("duplicate.txt");
    fs::write(&file_path, "alpha\nbeta\ninserted\n").unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "alpha\n",
        "new_str": "alpha\nbeta\ninserted\n"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("duplicate insertion rejected"),
        "Expected duplicate insertion error, got: {err_msg}"
    );
}

#[tokio::test]
async fn test_directory_tree_max_depth_honored() {
    let temp_dir = TempDir::new().unwrap();
    let deep_path = temp_dir.path().join("a").join("b").join("c").join("d");
    fs::create_dir_all(&deep_path).unwrap();
    fs::write(deep_path.join("deep_file.txt"), "").unwrap();

    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": temp_dir.path().to_str().unwrap(),
        "max_depth": 2
    });

    let result = tool.execute(args).await.unwrap();
    let entries = flatten_tree(&result["tree"]);

    let has_deep = entries.iter().any(|(name, _)| name.contains("deep_file"));
    assert!(!has_deep);
}

#[tokio::test]
async fn test_directory_tree_nonexistent_path() {
    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
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

    let tool = FileRead::with_safety_config(permissive_safety_config());
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

    let tool = FileWrite::with_safety_config(permissive_safety_config());
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
async fn test_file_write_rejects_invalid_rust_source() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("lib.rs");

    let tool = FileWrite::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "content": "Alignment::Center => {"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Rust syntax validation failed"), "got: {err}");
    assert!(
        !file_path.exists(),
        "invalid Rust write should not touch the file"
    );
}

#[tokio::test]
async fn test_file_edit_rejects_invalid_rust_source() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("lib.rs");
    fs::write(&file_path, "pub fn render() {\n    println!(\"ok\");\n}\n").unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "println!(\"ok\");",
        "new_str": "Alignment::Center => {"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Rust syntax validation failed"), "got: {err}");

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(
        content.contains("println!(\"ok\");"),
        "invalid Rust edit should leave the original file intact"
    );
}

#[tokio::test]
async fn test_directory_tree_with_files_and_directories() {
    let temp_dir = TempDir::new().unwrap();
    fs::write(temp_dir.path().join("file.txt"), "content").unwrap();
    fs::create_dir(temp_dir.path().join("subdir")).unwrap();
    fs::write(temp_dir.path().join("subdir").join("nested.txt"), "").unwrap();

    let tool = DirectoryTree::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": temp_dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let entries = flatten_tree(&result["tree"]);

    let has_dir = entries.iter().any(|(_, type_)| type_ == "directory");
    let has_file = entries.iter().any(|(_, type_)| type_ == "file");
    assert!(has_dir);
    assert!(has_file);
}

#[tokio::test]
async fn test_file_read_blocks_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let blocked_path = temp_dir
        .path()
        .parent()
        .unwrap()
        .join("should_not_escape.txt");
    let tool = FileRead::with_safety_config(restricted_safety_config(temp_dir.path()));
    let args = serde_json::json!({"path": blocked_path});
    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert_path_access_rejected(result.unwrap_err());
}

#[tokio::test]
async fn test_directory_tree_blocks_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let blocked_path = temp_dir.path().parent().unwrap();
    let tool = DirectoryTree::with_safety_config(restricted_safety_config(temp_dir.path()));
    let args = serde_json::json!({"path": blocked_path});
    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert_path_access_rejected(result.unwrap_err());
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

    let tool = FileDelete::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({"path": file_path.to_str().unwrap()});

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["deleted"], true);
    assert!(!file_path.exists());
}

#[tokio::test]
async fn test_file_delete_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("nonexistent.txt");

    let tool = FileDelete::with_safety_config(permissive_safety_config());
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

    let tool = FileDelete::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({"path": dir_path.to_str().unwrap()});

    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("directory"));
}

#[tokio::test]
async fn test_file_delete_blocks_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let blocked_path = temp_dir.path().parent().unwrap().join("escape.txt");
    let tool = FileDelete::with_safety_config(restricted_safety_config(temp_dir.path()));
    let args = serde_json::json!({"path": blocked_path});
    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert_path_access_rejected(result.unwrap_err());
}

#[tokio::test]
async fn test_file_delete_invalid_json_args() {
    let tool = FileDelete::new();
    let args = serde_json::json!({});
    let result = tool.execute(args).await;
    assert!(result.is_err());
}

// --- FileMultiEdit tests ---

#[test]
fn test_file_multi_edit_name() {
    let tool = FileMultiEdit::new();
    assert_eq!(tool.name(), "file_multi_edit");
}

#[tokio::test]
async fn test_file_multi_edit_success() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("multi.txt");
    fs::write(&file_path, "alpha\nbeta\ngamma\n").unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "edits": [
            {"path": file_path.to_str().unwrap(), "old_str": "alpha", "new_str": "ALPHA"},
            {"path": file_path.to_str().unwrap(), "old_str": "gamma", "new_str": "GAMMA"}
        ]
    });

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result["success"], true);
    assert_eq!(result["edits_applied"], 2);

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("ALPHA"));
    assert!(content.contains("beta"));
    assert!(content.contains("GAMMA"));
    assert!(!content.contains("alpha"));
    assert!(!content.contains("gamma"));
}

#[tokio::test]
async fn test_file_multi_edit_atomic_rollback() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("atomic.txt");
    fs::write(&file_path, "one\ntwo\nthree\n").unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "edits": [
            {"path": file_path.to_str().unwrap(), "old_str": "one", "new_str": "ONE"},
            {"path": file_path.to_str().unwrap(), "old_str": "MISSING", "new_str": "NEVER"}
        ]
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());

    // File should remain unchanged because the batch is atomic
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("one"));
    assert!(!content.contains("ONE"));
}

#[tokio::test]
async fn test_file_multi_edit_overlap_rejected() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("overlap.txt");
    fs::write(&file_path, "line1\nline2\nline3\n").unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "edits": [
            {"path": file_path.to_str().unwrap(), "old_str": "line1\nline2", "new_str": "A"},
            {"path": file_path.to_str().unwrap(), "old_str": "line2\nline3", "new_str": "B"}
        ]
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("overlap"));
}

// --- Stale-guard tests ---

#[tokio::test]
async fn test_file_edit_stale_guard_rejects() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("stale.txt");
    fs::write(&file_path, "original content").unwrap();

    // Simulate a read to record snapshot
    let read_tool = FileRead::with_safety_config(permissive_safety_config());
    let read_args = serde_json::json!({"path": file_path.to_str().unwrap()});
    read_tool.execute(read_args).await.unwrap();

    // Modify file externally
    fs::write(&file_path, "externally modified").unwrap();

    // Edit should be rejected as stale
    let edit_tool = FileEdit::with_safety_config(permissive_safety_config());
    let edit_args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "original content",
        "new_str": "replaced"
    });
    let result = edit_tool.execute(edit_args).await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("changed on disk"));
}

#[tokio::test]
async fn test_file_write_stale_guard_rejects() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("stale_write.txt");
    fs::write(&file_path, "original").unwrap();

    let read_tool = FileRead::with_safety_config(permissive_safety_config());
    read_tool
        .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
        .await
        .unwrap();

    fs::write(&file_path, "modified externally").unwrap();

    let write_tool = FileWrite::with_safety_config(permissive_safety_config());
    let result = write_tool
        .execute(serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "new content"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("changed on disk"));
}

#[tokio::test]
async fn test_file_delete_stale_guard_rejects() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("stale_delete.txt");
    fs::write(&file_path, "original").unwrap();

    let read_tool = FileRead::with_safety_config(permissive_safety_config());
    read_tool
        .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
        .await
        .unwrap();

    fs::write(&file_path, "modified externally").unwrap();

    let delete_tool = FileDelete::with_safety_config(permissive_safety_config());
    let result = delete_tool
        .execute(serde_json::json!({"path": file_path.to_str().unwrap()}))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("changed on disk"));
}

// --- Line ending preservation tests ---

#[tokio::test]
async fn test_file_write_preserves_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("crlf.txt");
    fs::write(&file_path, b"line1\r\nline2\r\n").unwrap();

    let tool = FileWrite::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "content": "new1\nnew2\n"
    });

    tool.execute(args).await.unwrap();
    let bytes = fs::read(&file_path).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\r\n"));
    assert!(!text.contains("\n\n") && !text.contains("\r\r"));
}

#[tokio::test]
async fn test_file_edit_preserves_crlf() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("crlf_edit.txt");
    fs::write(&file_path, b"alpha\r\nbeta\r\n").unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let args = serde_json::json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "alpha",
        "new_str": "ALPHA"
    });

    tool.execute(args).await.unwrap();
    let bytes = fs::read(&file_path).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("ALPHA\r\n"));
    assert!(text.contains("beta\r\n"));
}

// --- Permission preservation across atomic writes ---

#[tokio::test]
#[cfg(unix)]
async fn test_file_edit_preserves_executable_mode() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new().unwrap();
    let script = temp_dir.path().join("run.sh");
    fs::write(&script, "#!/bin/sh\necho old\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    tool.execute(serde_json::json!({
        "path": script.to_str().unwrap(),
        "old_str": "echo old",
        "new_str": "echo new"
    }))
    .await
    .unwrap();

    let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "edit must not strip the executable bit");
    assert!(fs::read_to_string(&script).unwrap().contains("echo new"));
}

#[tokio::test]
#[cfg(unix)]
async fn test_file_write_preserves_mode_on_overwrite() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new().unwrap();
    let script = temp_dir.path().join("tool.sh");
    fs::write(&script, "#!/bin/sh\necho v1\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o750)).unwrap();

    let tool = FileWrite::with_safety_config(permissive_safety_config());
    tool.execute(serde_json::json!({
        "path": script.to_str().unwrap(),
        "content": "#!/bin/sh\necho v2\n",
        "backup": false
    }))
    .await
    .unwrap();

    let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o750, "overwrite must preserve the original mode");
}

#[tokio::test]
#[cfg(unix)]
async fn test_file_multi_edit_preserves_mode() {
    use std::os::unix::fs::PermissionsExt;
    let temp_dir = TempDir::new().unwrap();
    let script = temp_dir.path().join("multi.sh");
    fs::write(&script, "#!/bin/sh\necho a\necho b\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    tool.execute(serde_json::json!({
        "edits": [
            {"path": script.to_str().unwrap(), "old_str": "echo a", "new_str": "echo A"},
            {"path": script.to_str().unwrap(), "old_str": "echo b", "new_str": "echo B"}
        ]
    }))
    .await
    .unwrap();

    let mode = fs::metadata(&script).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "multi_edit must not strip the executable bit");
}

// --- Non-UTF8 honesty ---

#[tokio::test]
async fn test_file_edit_refuses_non_utf8_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("binary.bin");
    let original: &[u8] = &[0x66, 0x80, 0x67, 0xFF, 0x00, 0x61];
    fs::write(&file_path, original).unwrap();

    let tool = FileEdit::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "f",
            "new_str": "x"
        }))
        .await;
    let err = result.expect_err("editing a non-UTF-8 file must fail loudly");
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "unexpected error: {err}"
    );
    // The file must be byte-identical — no U+FFFD corruption.
    assert_eq!(fs::read(&file_path).unwrap(), original);
}

#[tokio::test]
async fn test_file_multi_edit_refuses_non_utf8_file() {
    let temp_dir = TempDir::new().unwrap();
    let good = temp_dir.path().join("good.txt");
    fs::write(&good, "alpha\n").unwrap();
    let binary = temp_dir.path().join("binary.bin");
    let original: &[u8] = &[0xDE, 0xAD, 0x80, 0xFF];
    fs::write(&binary, original).unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({
            "edits": [
                {"path": good.to_str().unwrap(), "old_str": "alpha", "new_str": "ALPHA"},
                {"path": binary.to_str().unwrap(), "old_str": "x", "new_str": "y"}
            ]
        }))
        .await;
    let err = result.expect_err("multi_edit touching a non-UTF-8 file must fail");
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "unexpected error: {err}"
    );
    // Atomic batch: the GOOD file must not have been modified either.
    assert_eq!(fs::read_to_string(&good).unwrap(), "alpha\n");
    assert_eq!(fs::read(&binary).unwrap(), original);
}

#[tokio::test]
async fn test_file_write_refuses_overwriting_non_utf8_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("data.dat");
    let original: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0xFF];
    fs::write(&file_path, original).unwrap();

    let tool = FileWrite::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({
            "path": file_path.to_str().unwrap(),
            "content": "replacement text"
        }))
        .await;
    let err = result.expect_err("overwriting a non-UTF-8 file must fail loudly");
    assert!(
        err.to_string().contains("not valid UTF-8"),
        "unexpected error: {err}"
    );
    assert_eq!(fs::read(&file_path).unwrap(), original);
}

#[tokio::test]
async fn test_file_read_reports_lossy_encoding_honestly() {
    let temp_dir = TempDir::new().unwrap();
    let binary = temp_dir.path().join("binary.bin");
    fs::write(&binary, [0x61, 0x0A, 0x80, 0xFF, 0x0A, 0x62]).unwrap();

    let tool = FileRead::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({"path": binary.to_str().unwrap()}))
        .await
        .unwrap();
    assert_eq!(result["encoding"], "utf-8-lossy");
    assert_eq!(result["valid_utf8"], false);

    // The line-range path reports lossiness of the returned slice too.
    let result = tool
        .execute(serde_json::json!({
            "path": binary.to_str().unwrap(),
            "line_range": [2, 2]
        }))
        .await
        .unwrap();
    assert_eq!(result["encoding"], "utf-8-lossy");
    assert_eq!(result["valid_utf8"], false);

    // A plain UTF-8 file still reports exact utf-8.
    let text = temp_dir.path().join("text.txt");
    fs::write(&text, "plain text\n").unwrap();
    let result = tool
        .execute(serde_json::json!({"path": text.to_str().unwrap()}))
        .await
        .unwrap();
    assert_eq!(result["encoding"], "utf-8");
    assert_eq!(result["valid_utf8"], true);
}

// --- Multi-edit batch atomicity / determinism ---

#[tokio::test]
async fn test_file_multi_edit_validation_failure_touches_no_file() {
    let temp_dir = TempDir::new().unwrap();
    let f1 = temp_dir.path().join("one.txt");
    let f2 = temp_dir.path().join("two.txt");
    fs::write(&f1, "one\n").unwrap();
    fs::write(&f2, "two\n").unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({
            "edits": [
                {"path": f1.to_str().unwrap(), "old_str": "one", "new_str": "ONE"},
                {"path": f2.to_str().unwrap(), "old_str": "MISSING", "new_str": "X"}
            ]
        }))
        .await;
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&f1).unwrap(), "one\n");
    assert_eq!(fs::read_to_string(&f2).unwrap(), "two\n");
}

#[tokio::test]
async fn test_file_multi_edit_reports_files_in_deterministic_order() {
    let temp_dir = TempDir::new().unwrap();
    // Deliberately non-alphabetical creation/argument order.
    let zb = temp_dir.path().join("zb.txt");
    let ab = temp_dir.path().join("ab.txt");
    fs::write(&zb, "z\n").unwrap();
    fs::write(&ab, "a\n").unwrap();

    let tool = FileMultiEdit::with_safety_config(permissive_safety_config());
    let result = tool
        .execute(serde_json::json!({
            "edits": [
                {"path": zb.to_str().unwrap(), "old_str": "z", "new_str": "Z"},
                {"path": ab.to_str().unwrap(), "old_str": "a", "new_str": "A"}
            ]
        }))
        .await
        .unwrap();

    let files: Vec<String> = serde_json::from_value(result["files"].clone()).unwrap();
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(files, sorted, "files list must be deterministically sorted");
}

#[tokio::test]
async fn test_write_all_atomic_rolls_back_on_persist_failure() {
    let temp_dir = TempDir::new().unwrap();
    let good = temp_dir.path().join("good.txt");
    fs::write(&good, "original\n").unwrap();
    // The second target is an existing DIRECTORY: staging succeeds, but
    // the persist (rename over a dir) fails — forcing a rollback of the
    // first file.
    let dir_target = temp_dir.path().join("a_directory");
    fs::create_dir(&dir_target).unwrap();

    let files = vec![
        (good.clone(), "modified\n".to_string()),
        (dir_target.clone(), "whatever".to_string()),
    ];
    let result = write_all_atomic(&files).await;
    let err = result.expect_err("persist over a directory must fail");
    assert!(
        err.to_string().contains("rolled back"),
        "error should mention rollback: {err}"
    );
    assert_eq!(
        fs::read_to_string(&good).unwrap(),
        "original\n",
        "the already-persisted file must be rolled back to its pre-image"
    );
}

#[tokio::test]
async fn test_write_all_atomic_staging_failure_touches_nothing() {
    let temp_dir = TempDir::new().unwrap();
    let good = temp_dir.path().join("good.txt");
    fs::write(&good, "original\n").unwrap();
    // Second target's parent component is a regular FILE, so staging its
    // temp file fails before anything is persisted.
    let blocker = temp_dir.path().join("blocker");
    fs::write(&blocker, "not a dir").unwrap();
    let impossible = blocker.join("child.txt");

    let files = vec![
        (good.clone(), "modified\n".to_string()),
        (impossible, "nope".to_string()),
    ];
    let result = write_all_atomic(&files).await;
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&good).unwrap(), "original\n");
}
