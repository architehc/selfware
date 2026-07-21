use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_grep_search_name() {
    let tool = GrepSearch;
    assert_eq!(tool.name(), "grep_search");
}

#[test]
fn test_grep_search_description() {
    let tool = GrepSearch;
    assert!(tool.description().contains("regex"));
}

#[test]
fn test_grep_search_schema() {
    let tool = GrepSearch;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("pattern")));
}

#[tokio::test]
async fn test_grep_search_basic() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "line one\nline two\nline three").unwrap();

    let tool = GrepSearch;
    let args = serde_json::json!({
        "pattern": "line two",
        "path": temp_dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["content"].as_str().unwrap().contains("line two"));
}

#[tokio::test]
async fn test_grep_search_case_insensitive() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "HELLO world").unwrap();

    let tool = GrepSearch;
    let args = serde_json::json!({
        "pattern": "hello",
        "path": temp_dir.path().to_str().unwrap(),
        "case_insensitive": true
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
}

#[tokio::test]
async fn test_grep_search_with_context() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "line 1\nline 2\nline 3\nline 4\nline 5").unwrap();

    let tool = GrepSearch;
    let args = serde_json::json!({
        "pattern": "line 3",
        "path": temp_dir.path().to_str().unwrap(),
        "context_lines": 1
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);

    let match_obj = &matches[0];
    let context_before = match_obj["context_before"].as_array().unwrap();
    let context_after = match_obj["context_after"].as_array().unwrap();

    assert_eq!(context_before.len(), 1);
    assert_eq!(context_after.len(), 1);
}

#[tokio::test]
async fn test_grep_search_pagination() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "match\nmatch\nmatch\nmatch\nmatch").unwrap();

    let tool = GrepSearch;
    let args = serde_json::json!({
        "pattern": "match",
        "path": temp_dir.path().to_str().unwrap(),
        "max_matches": 2,
        "offset": 0
    });

    let result = tool.execute(args).await.unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(result["total_matches"], 5);
}

#[test]
fn test_standalone_grep_search() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "hello world\nfoo bar\nhello again").unwrap();

    let result = grep_search("hello", temp_dir.path().to_str().unwrap(), true, 100, 0);
    assert_eq!(result.matches.len(), 2);
    assert_eq!(result.total_matches, 2);
}
