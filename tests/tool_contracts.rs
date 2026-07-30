//! Contract/integration tests for the core tool interfaces.
//!
//! Each test verifies that a tool:
//! 1. Has a non-empty `name()`
//! 2. Has a non-empty `description()`
//! 3. Returns valid JSON from `schema()` with `"type": "object"` and `"properties"`
//! 4. Executes successfully with valid arguments
//! 5. Returns an error (not a panic) with invalid arguments

use selfware::config::SafetyConfig;
use selfware::tools::file::{DirectoryTree, FileEdit, FileRead, FileWrite};
use selfware::tools::git::GitStatus;
use selfware::tools::search::GrepSearch;
use selfware::tools::shell_exec::ShellExec;
use selfware::tools::Tool;

use serde_json::{json, Value};
use std::io::Write;
use tempfile::{NamedTempFile, TempDir};

/// Resolve the path the safety checker will see for a temp file/dir.
///
/// macOS canonicalizes `/var/folders/...` to `/private/var/folders/...`,
/// so a TempDir/NamedTempFile path used verbatim in the SafetyConfig
/// allow-list won't match the canonical path the tool sees during
/// execution. On Windows `Path::canonicalize()` returns UNC paths
/// (`\\?\C:\...`) that break the allow-list comparison, so we leave the
/// path alone there. On Linux there's no symlink to worry about.
fn safe_canonical(p: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
    }
    #[cfg(not(target_os = "macos"))]
    {
        p.to_path_buf()
    }
}

/// Build a SafetyConfig that allows access to the given path (and its children).
fn permissive_safety(path: &str) -> SafetyConfig {
    SafetyConfig {
        allowed_paths: vec![format!("{}/**", path), path.to_string()],
        denied_paths: vec![],
        protected_branches: vec![],
        require_confirmation: vec![],
        strict_permissions: false,
        permissions: vec![],
        trust_gate_tool_results: true,
    }
}

/// Assert the standard schema contract: type=object with a properties map.
fn assert_valid_schema(schema: &Value, tool_name: &str) {
    assert_eq!(
        schema.get("type").and_then(|v| v.as_str()),
        Some("object"),
        "{}: schema must have \"type\": \"object\"",
        tool_name
    );
    let props = schema.get("properties");
    assert!(
        props.is_some(),
        "{}: schema must have \"properties\"",
        tool_name
    );
    assert!(
        props.unwrap().is_object(),
        "{}: \"properties\" must be an object",
        tool_name
    );
    assert!(
        !props.unwrap().as_object().unwrap().is_empty(),
        "{}: \"properties\" must not be empty",
        tool_name
    );
}

/// Assert that the tool has non-empty name and description.
fn assert_metadata(tool: &dyn Tool) {
    assert!(!tool.name().is_empty(), "name() must not be empty");
    assert!(
        !tool.description().is_empty(),
        "description() must not be empty"
    );
}

// ---------------------------------------------------------------------------
// file_read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_read_schema_contract() {
    let tool = FileRead::new();
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    // Schema must declare "path" in properties
    let props = tool.schema()["properties"].clone();
    assert!(props.get("path").is_some(), "schema must include 'path'");
}

#[tokio::test]
async fn file_read_valid_execution() {
    let mut tmp = NamedTempFile::new().expect("create temp file");
    writeln!(tmp, "hello from test").expect("write temp file");
    // macOS canonicalizes /var/folders/... to /private/var/folders/...; the
    // tool sees the canonical path during safety checks, so we must
    // canonicalize here too or the allow-list won't match.
    let canonical = safe_canonical(tmp.path());
    let path = canonical.to_str().unwrap().to_string();
    let parent = canonical.parent().unwrap().to_str().unwrap().to_string();

    let tool = FileRead::with_safety_config(permissive_safety(&parent));
    let result = tool.execute(json!({"path": path})).await;
    assert!(
        result.is_ok(),
        "file_read with valid path should succeed: {:?}",
        result.err()
    );

    let val = result.unwrap();
    let content = val.get("content").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        content.contains("hello from test"),
        "file_read must return the file content"
    );
}

#[tokio::test]
async fn file_read_invalid_args() {
    let tool = FileRead::new();
    // Missing required "path" field
    let result = tool.execute(json!({})).await;
    assert!(result.is_err(), "file_read with missing path should error");
}

#[tokio::test]
async fn file_read_nonexistent_file() {
    let dir = TempDir::new().expect("create temp dir");
    let parent = safe_canonical(dir.path()).to_str().unwrap().to_string();
    let tool = FileRead::with_safety_config(permissive_safety(&parent));
    let path = dir.path().join("no_such_file.txt");
    let result = tool.execute(json!({"path": path.to_str().unwrap()})).await;
    assert!(
        result.is_err(),
        "file_read on nonexistent file should error"
    );
}

// ---------------------------------------------------------------------------
// file_write
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_write_schema_contract() {
    let tool = FileWrite::new();
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    let props = tool.schema()["properties"].clone();
    assert!(props.get("path").is_some(), "schema must include 'path'");
    assert!(
        props.get("content").is_some(),
        "schema must include 'content'"
    );
}

#[tokio::test]
async fn file_write_valid_execution() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("output.txt");
    let parent = safe_canonical(dir.path()).to_str().unwrap().to_string();

    let tool = FileWrite::with_safety_config(permissive_safety(&parent));
    let result = tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "content": "written by test"
        }))
        .await;
    assert!(
        result.is_ok(),
        "file_write should succeed: {:?}",
        result.err()
    );

    let on_disk = std::fs::read_to_string(&file_path).expect("read back");
    assert_eq!(on_disk, "written by test");
}

#[tokio::test]
async fn file_write_invalid_args() {
    let tool = FileWrite::new();
    // Missing required fields
    let result = tool.execute(json!({})).await;
    assert!(result.is_err(), "file_write with missing args should error");
}

// ---------------------------------------------------------------------------
// file_edit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn file_edit_schema_contract() {
    let tool = FileEdit::new();
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    let props = tool.schema()["properties"].clone();
    assert!(props.get("path").is_some());
    assert!(props.get("old_str").is_some());
    assert!(props.get("new_str").is_some());
}

#[tokio::test]
async fn file_edit_valid_execution() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("editable.txt");
    std::fs::write(&file_path, "alpha beta gamma").expect("seed file");
    let parent = safe_canonical(dir.path()).to_str().unwrap().to_string();

    let tool = FileEdit::with_safety_config(permissive_safety(&parent));
    let result = tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "beta",
            "new_str": "BETA"
        }))
        .await;
    assert!(
        result.is_ok(),
        "file_edit should succeed: {:?}",
        result.err()
    );

    let on_disk = std::fs::read_to_string(&file_path).expect("read back");
    assert!(
        on_disk.contains("BETA"),
        "file_edit must apply the replacement"
    );
    assert!(
        !on_disk.contains("beta"),
        "old_str should be gone after edit"
    );
}

#[tokio::test]
async fn file_edit_invalid_args() {
    let tool = FileEdit::new();
    // Missing required fields
    let result = tool.execute(json!({})).await;
    assert!(result.is_err(), "file_edit with missing args should error");
}

#[tokio::test]
async fn file_edit_old_str_not_found() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("no_match.txt");
    std::fs::write(&file_path, "unchanged content").expect("seed file");
    let parent = safe_canonical(dir.path()).to_str().unwrap().to_string();

    let tool = FileEdit::with_safety_config(permissive_safety(&parent));
    let result = tool
        .execute(json!({
            "path": file_path.to_str().unwrap(),
            "old_str": "DOES_NOT_EXIST",
            "new_str": "replacement"
        }))
        .await;
    assert!(
        result.is_err(),
        "file_edit should error when old_str not found"
    );
}

// ---------------------------------------------------------------------------
// shell_exec
// ---------------------------------------------------------------------------

#[tokio::test]
async fn shell_exec_schema_contract() {
    let tool = ShellExec;
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    let props = tool.schema()["properties"].clone();
    assert!(
        props.get("command").is_some(),
        "schema must include 'command'"
    );
}

#[tokio::test]
async fn shell_exec_valid_execution() {
    let tool = ShellExec;
    let result = tool
        .execute(json!({"command": "echo hello", "timeout_secs": 5}))
        .await;
    assert!(
        result.is_ok(),
        "shell_exec 'echo hello' should succeed: {:?}",
        result.err()
    );

    let val = result.unwrap();
    let stdout = val.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
    assert!(stdout.contains("hello"), "shell_exec should capture stdout");
}

#[tokio::test]
async fn shell_exec_invalid_args() {
    let tool = ShellExec;
    // Missing required "command"
    let result = tool.execute(json!({})).await;
    assert!(
        result.is_err(),
        "shell_exec with missing command should error"
    );
}

// ---------------------------------------------------------------------------
// directory_tree
// ---------------------------------------------------------------------------

#[tokio::test]
async fn directory_tree_schema_contract() {
    let tool = DirectoryTree::new();
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    let props = tool.schema()["properties"].clone();
    assert!(props.get("path").is_some(), "schema must include 'path'");
}

#[tokio::test]
async fn directory_tree_valid_execution() {
    let dir = TempDir::new().expect("create temp dir");
    // Create some structure
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).expect("mkdir sub");
    std::fs::write(sub.join("file.txt"), "hi").expect("create file");
    let parent = safe_canonical(dir.path()).to_str().unwrap().to_string();

    let tool = DirectoryTree::with_safety_config(permissive_safety(&parent));
    let result = tool.execute(json!({"path": parent, "max_depth": 2})).await;
    assert!(
        result.is_ok(),
        "directory_tree should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn directory_tree_invalid_args() {
    let tool = DirectoryTree::new();
    // Missing required "path"
    let result = tool.execute(json!({})).await;
    assert!(
        result.is_err(),
        "directory_tree with missing path should error"
    );
}

// ---------------------------------------------------------------------------
// grep_search
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grep_search_schema_contract() {
    let tool = GrepSearch;
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());

    let props = tool.schema()["properties"].clone();
    assert!(
        props.get("pattern").is_some(),
        "schema must include 'pattern'"
    );
    assert!(props.get("path").is_some(), "schema must include 'path'");
}

#[tokio::test]
async fn grep_search_valid_execution() {
    let dir = TempDir::new().expect("create temp dir");
    let file_path = dir.path().join("searchable.txt");
    std::fs::write(&file_path, "line one\nfind_this_needle\nline three\n")
        .expect("create searchable file");

    let tool = GrepSearch;
    let result = tool
        .execute(json!({
            "pattern": "find_this_needle",
            "path": dir.path().to_str().unwrap()
        }))
        .await;
    assert!(
        result.is_ok(),
        "grep_search should succeed: {:?}",
        result.err()
    );

    let val = result.unwrap();
    let output = serde_json::to_string(&val).unwrap();
    assert!(
        output.contains("find_this_needle"),
        "grep_search must find the pattern in results"
    );
}

#[tokio::test]
async fn grep_search_invalid_args() {
    let tool = GrepSearch;
    // Missing required fields
    let result = tool.execute(json!({})).await;
    assert!(
        result.is_err(),
        "grep_search with missing args should error"
    );
}

// ---------------------------------------------------------------------------
// git_status
// ---------------------------------------------------------------------------

#[tokio::test]
async fn git_status_schema_contract() {
    let tool = GitStatus::default();
    assert_metadata(&tool);
    assert_valid_schema(&tool.schema(), tool.name());
}

#[tokio::test]
async fn git_status_valid_execution() {
    // Run against the selfware repo itself
    let tool = GitStatus::default();
    let result = tool.execute(json!({"repo_path": "."})).await;
    assert!(
        result.is_ok(),
        "git_status on selfware repo should succeed: {:?}",
        result.err()
    );

    let val = result.unwrap();
    // Should contain a branch field
    assert!(
        val.get("branch").is_some(),
        "git_status result must include 'branch'"
    );
}

#[tokio::test]
async fn git_status_invalid_repo() {
    let dir = TempDir::new().expect("create temp dir");
    let tool = GitStatus::default();
    let result = tool
        .execute(json!({"repo_path": dir.path().to_str().unwrap()}))
        .await;
    assert!(
        result.is_err(),
        "git_status on non-repo directory should error"
    );
}

// ---------------------------------------------------------------------------
// Cross-cutting: schema properties match what execute() accepts
// ---------------------------------------------------------------------------

#[tokio::test]
async fn all_core_tools_schema_has_required_subset_of_properties() {
    // For each tool, verify that every field listed in "required" also
    // appears in "properties" -- a common contract violation.
    let tools: Vec<Box<dyn Tool>> = vec![
        Box::new(FileRead::new()),
        Box::new(FileWrite::new()),
        Box::new(FileEdit::new()),
        Box::new(DirectoryTree::new()),
        Box::new(ShellExec),
        Box::new(GrepSearch),
        Box::new(GitStatus::default()),
    ];

    for tool in &tools {
        let schema = tool.schema();
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("{}: missing properties", tool.name()));

        if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
            for req in required {
                let key = req.as_str().unwrap_or("");
                assert!(
                    props.contains_key(key),
                    "{}: required field '{}' not in properties",
                    tool.name(),
                    key
                );
            }
        }
    }
}
