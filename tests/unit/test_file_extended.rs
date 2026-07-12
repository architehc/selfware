//! Extended file tool tests
//!
//! Tests for FileWrite, FileEdit, and DirectoryTree tools
//! with comprehensive coverage of success and error paths.

use selfware::tools::{
    file::{DirectoryTree, FileEdit, FileRead, FileWrite},
    Tool,
};
use serde_json::json;
use std::fs;
use std::sync::Once;
use tempfile::TempDir;

static INIT: Once = Once::new();

fn setup_test_mode() {
    INIT.call_once(|| {
        let cfg = selfware::config::SafetyConfig {
            allowed_paths: vec!["/**".to_string()],
            ..Default::default()
        };
        selfware::tools::file::init_safety_config(&cfg);
    });
}

// ==================== FileRead Extended Tests ====================

#[tokio::test]
async fn test_file_read_with_line_range() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("test.txt");
    fs::write(&file_path, "line1\nline2\nline3\nline4\nline5").unwrap();

    let tool = FileRead::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "line_range": [2, 4]
    });

    let result = tool.execute(args).await.unwrap();
    let content = result.get("content").unwrap().as_str().unwrap();
    assert_eq!(content, "line2\nline3\nline4");
    assert!(result.get("truncated").unwrap().as_bool().unwrap());
}

#[tokio::test]
async fn test_file_read_empty_file() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("empty.txt");
    fs::write(&file_path, "").unwrap();

    let tool = FileRead::new();
    let args = json!({"path": file_path.to_str().unwrap()});

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result.get("content").unwrap(), "");
    assert_eq!(result.get("total_lines").unwrap(), 0);
}

// ==================== FileWrite Tests ====================

#[tokio::test]
async fn test_file_write_success() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("new_file.txt");

    let tool = FileWrite::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": "Hello, World!"
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
    assert_eq!(result.get("bytes_written").unwrap(), 13);

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Hello, World!");
}

#[tokio::test]
async fn test_file_write_creates_directories() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("nested/deep/file.txt");

    let tool = FileWrite::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": "nested content"
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
    assert!(file_path.exists());
}

#[tokio::test]
async fn test_file_write_with_backup() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("existing.txt");
    fs::write(&file_path, "original").unwrap();

    let tool = FileWrite::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": "new content",
        "backup": true
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());

    let backup_path = dir.path().join("existing.txt.bak");
    assert!(backup_path.exists());
    assert_eq!(fs::read_to_string(backup_path).unwrap(), "original");
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "new content");
}

#[tokio::test]
async fn test_file_write_without_backup() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("existing.txt");
    fs::write(&file_path, "original").unwrap();

    let tool = FileWrite::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": "new content",
        "backup": false
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());

    let backup_path = dir.path().join("existing.txt.bak");
    assert!(!backup_path.exists());
}

#[tokio::test]
async fn test_file_write_empty_content() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("empty.txt");

    let tool = FileWrite::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": ""
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
    assert_eq!(result.get("bytes_written").unwrap(), 0);
}

#[tokio::test]
async fn test_file_write_unicode_content() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("unicode.txt");

    let tool = FileWrite::new();
    let content = "Hello 世界 🌍 émoji";
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "content": content
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());

    let read_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(read_content, content);
}

// ==================== FileEdit Tests ====================

#[tokio::test]
async fn test_file_edit_success() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let tool = FileEdit::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "World",
        "new_str": "Rust"
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
    assert_eq!(result.get("matches_found").unwrap(), 1);

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Hello, Rust!");
}

#[tokio::test]
async fn test_file_edit_not_found() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let tool = FileEdit::new();
    let args = json!({
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
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "foo bar foo").unwrap();

    let tool = FileEdit::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "foo",
        "new_str": "baz"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("2 times"));
}

#[tokio::test]
async fn test_file_edit_delete_text() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "Hello, World!").unwrap();

    let tool = FileEdit::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": ", World",
        "new_str": ""
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "Hello!");
}

#[tokio::test]
async fn test_file_edit_multiline() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("edit.txt");
    fs::write(&file_path, "fn foo() {\n    println!(\"old\");\n}").unwrap();

    let tool = FileEdit::new();
    // Targeted edit (not the whole file — file_edit now rejects a 100% match
    // and directs you to file_write for full-file replacement).
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "    println!(\"old\");",
        "new_str": "    println!(\"new\");"
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());

    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("\"new\""));
}

#[tokio::test]
async fn test_file_edit_nonexistent_file() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("nonexistent.txt");

    let tool = FileEdit::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "foo",
        "new_str": "bar"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
}

// ==================== DirectoryTree Tests ====================

/// Flatten a DirectoryTree result's nested `tree` into every descendant node
/// (excluding the root). Each node is `{"name", "type", "size"?, "children"?}`.
/// The tool switched from a flat `entries` array to a nested `tree`.
fn collect_tree_nodes(result: &serde_json::Value) -> Vec<serde_json::Value> {
    fn walk(node: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                out.push(child.clone());
                walk(child, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Some(tree) = result.get("tree") {
        walk(tree, &mut out);
    }
    out
}

#[tokio::test]
async fn test_directory_tree_success() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file1.txt"), "content").unwrap();
    fs::create_dir(dir.path().join("subdir")).unwrap();
    fs::write(dir.path().join("subdir/file2.txt"), "content").unwrap();

    let tool = DirectoryTree::new();
    let args = json!({
        "path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let nodes = collect_tree_nodes(&result);
    assert!(nodes.len() >= 3); // file1, subdir, file2
}

#[tokio::test]
async fn test_directory_tree_max_depth() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("a/b/c/d")).unwrap();
    fs::write(dir.path().join("a/b/c/d/deep.txt"), "content").unwrap();

    let tool = DirectoryTree::new();
    let args = json!({
        "path": dir.path().to_str().unwrap(),
        "max_depth": 2
    });

    let result = tool.execute(args).await.unwrap();
    let nodes = collect_tree_nodes(&result);

    // Should not contain the deep file
    assert!(!nodes
        .iter()
        .any(|n| n.get("name").and_then(|v| v.as_str()) == Some("deep.txt")));
}

#[tokio::test]
async fn test_directory_tree_hidden_files() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("visible.txt"), "content").unwrap();
    fs::write(dir.path().join(".hidden"), "content").unwrap();

    let tool = DirectoryTree::new();

    // Without hidden files
    let args = json!({
        "path": dir.path().to_str().unwrap(),
        "include_hidden": false
    });
    let result = tool.execute(args).await.unwrap();
    let nodes = collect_tree_nodes(&result);
    let has_hidden = nodes.iter().any(|n| {
        n.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains(".hidden")
    });
    assert!(!has_hidden);

    // With hidden files
    let args = json!({
        "path": dir.path().to_str().unwrap(),
        "include_hidden": true
    });
    let result = tool.execute(args).await.unwrap();
    let nodes = collect_tree_nodes(&result);
    let has_hidden = nodes.iter().any(|n| {
        n.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains(".hidden")
    });
    assert!(has_hidden);
}

#[tokio::test]
async fn test_directory_tree_nonexistent() {
    setup_test_mode();
    let tool = DirectoryTree::new();
    let args = json!({
        "path": "/nonexistent/path/here"
    });

    let result = tool.execute(args).await.unwrap();
    // WalkDir returns an empty tree for non-existent directories
    let nodes = collect_tree_nodes(&result);
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn test_directory_tree_file_metadata() {
    setup_test_mode();
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("file.txt");
    fs::write(&file_path, "hello").unwrap();

    let tool = DirectoryTree::new();
    let args = json!({
        "path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let nodes = collect_tree_nodes(&result);

    let file_entry = nodes
        .iter()
        .find(|n| n.get("name").and_then(|v| v.as_str()) == Some("file.txt"))
        .unwrap();

    assert_eq!(file_entry.get("type").unwrap(), "file");
    assert_eq!(file_entry.get("size").unwrap(), 5);
}

// ==================== Tool Metadata Tests ====================

#[test]
fn test_file_read_metadata() {
    let tool = FileRead::new();
    assert_eq!(tool.name(), "file_read");
    assert!(!tool.description().is_empty());
    let schema = tool.schema();
    assert!(schema.get("properties").is_some());
}

#[test]
fn test_file_write_metadata() {
    let tool = FileWrite::new();
    assert_eq!(tool.name(), "file_write");
    assert!(!tool.description().is_empty());
}

#[test]
fn test_file_edit_metadata() {
    let tool = FileEdit::new();
    assert_eq!(tool.name(), "file_edit");
    assert!(tool.description().contains("surgical"));
}

#[test]
fn test_directory_tree_metadata() {
    let tool = DirectoryTree::new();
    assert_eq!(tool.name(), "directory_tree");
    assert!(!tool.description().is_empty());
}
