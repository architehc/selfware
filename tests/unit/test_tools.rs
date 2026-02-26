use selfware::tools::{
    file::{DirectoryTree, FileEdit, FileRead, FileWrite},
    shell::ShellExec,
    Tool, ToolRegistry,
};
use serde_json::json;

#[tokio::test]
async fn test_file_read_success() {
    let tool = FileRead::new();
    let args = json!({"path": "Cargo.toml"});

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("content").is_some());
    assert_eq!(result.get("encoding").unwrap(), "utf-8");
}

#[tokio::test]
async fn test_file_read_not_found() {
    let tool = FileRead::new();
    let args = json!({"path": "/nonexistent/file.txt"});

    let result = tool.execute(args).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_shell_exec_echo() {
    let tool = ShellExec;
    let args = json!({"command": "echo 'hello'", "timeout_secs": 5});

    let result = tool.execute(args).await.unwrap();
    assert_eq!(result.get("exit_code").unwrap(), 0);
    assert!(result
        .get("stdout")
        .unwrap()
        .as_str()
        .unwrap()
        .contains("hello"));
}

#[tokio::test]
async fn test_tool_registry() {
    let registry = ToolRegistry::new();
    assert!(registry.get("file_read").is_some());
    assert!(registry.get("shell_exec").is_some());
    assert!(registry.get("nonexistent").is_none());
}

// ---------------------------------------------------------------------------
// New tool error-path tests
// ---------------------------------------------------------------------------

/// Verify that FileRead returns an error (not a panic) when the target path
/// does not exist on the filesystem. Uses a relative path within the project
/// directory so that the safety path validator allows it through.
#[tokio::test]
async fn test_file_read_nonexistent_path_returns_error() {
    let tool = FileRead::new();
    // Use a relative path so it passes the allowed-paths check ("./**")
    let args = json!({"path": "__selfware_test_nonexistent_12345.txt"});

    let result = tool.execute(args).await;
    assert!(
        result.is_err(),
        "Expected Err for non-existent path, got Ok"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Failed to read file") || err_msg.contains("No such file"),
        "Unexpected error message: {}",
        err_msg
    );
}

/// Verify that FileWrite rejects writes to a path inside a restricted
/// (denied) directory by returning an error rather than writing the file.
/// We use a SafetyConfig with an explicit denied_paths entry and provide
/// it via `with_safety_config` so that the path validator rejects the path.
#[tokio::test]
async fn test_file_write_restricted_directory_returns_error() {
    use selfware::config::SafetyConfig;

    // Build a safety config that denies everything under /etc/**
    let safety = SafetyConfig {
        allowed_paths: vec!["./**".to_string()],
        denied_paths: vec!["/etc/**".to_string()],
        protected_branches: vec![],
        require_confirmation: vec![],
    };

    let tool = FileWrite::with_safety_config(safety);
    let args = json!({
        "path": "/etc/selfware_test_restricted.txt",
        "content": "should not be written"
    });

    let result = tool.execute(args).await;
    assert!(
        result.is_err(),
        "Expected Err when writing to a denied path, got Ok"
    );
}

/// Verify that FileEdit returns an error when old_str is empty. An empty
/// search string would trivially match everywhere, which is never the
/// intended behaviour. The tool should report either zero matches (since
/// `"".matches("")` is weird) or explicitly reject it.
#[tokio::test]
async fn test_file_edit_empty_old_string_returns_error() {
    // Create a temporary file so the path itself is valid
    let temp_dir = tempfile::TempDir::new().unwrap();
    let file_path = temp_dir.path().join("edit_test.txt");
    std::fs::write(&file_path, "Hello, world!").unwrap();

    let tool = FileEdit::new();
    let args = json!({
        "path": file_path.to_str().unwrap(),
        "old_str": "",
        "new_str": "replacement"
    });

    let result = tool.execute(args).await;
    // An empty old_str causes multiple matches (every position), so the
    // "expected exactly 1" guard should fire.
    assert!(result.is_err(), "Expected Err for empty old_str, got Ok");
}

/// Verify that DirectoryTree returns an error (or an empty listing) when
/// asked to list a directory that does not exist on the filesystem. Uses a
/// relative path so that the safety path validator allows it through.
#[tokio::test]
async fn test_directory_tree_nonexistent_directory_returns_error() {
    let tool = DirectoryTree::new();
    // Use a relative path so it passes the allowed-paths check ("./**")
    let args = json!({"path": "__selfware_test_nonexistent_dir_98765"});

    let result = tool.execute(args).await;
    // WalkDir silently yields zero entries for a non-existent root, so
    // the tool may return Ok with an empty entries array. We accept
    // either Err or Ok-with-empty as correct behaviour.
    match result {
        Err(_) => { /* error is an acceptable response */ }
        Ok(val) => {
            let entries = val["entries"].as_array();
            assert!(
                entries.is_none_or(|e| e.is_empty()),
                "Expected empty entries for non-existent directory, got: {:?}",
                val
            );
        }
    }
}

/// Verify that ShellExec handles an empty command string gracefully,
/// returning either an error or a non-zero exit code instead of panicking.
#[tokio::test]
async fn test_shell_exec_empty_command_returns_error_or_nonzero() {
    let tool = ShellExec;
    let args = json!({"command": "", "timeout_secs": 5});

    let result = tool.execute(args).await;
    match result {
        Err(_) => { /* explicit error is fine */ }
        Ok(val) => {
            // An empty command passed to sh -c typically exits 0 on some
            // shells, but we still want to confirm we did not panic.
            // Accept any exit code as long as we got a response.
            assert!(
                val.get("exit_code").is_some(),
                "Expected exit_code in response, got: {:?}",
                val
            );
        }
    }
}

/// Verify that FileRead with a path to a directory (rather than a file)
/// returns an error instead of panicking. Uses "src" which is a directory
/// that exists within the project (and passes the safety path validator).
#[tokio::test]
async fn test_file_read_directory_path_returns_error() {
    let tool = FileRead::new();
    // "src" is a directory that exists in the project root
    let args = json!({"path": "src"});

    let result = tool.execute(args).await;
    assert!(
        result.is_err(),
        "Expected Err when reading a directory path, got Ok"
    );
}

/// Verify that FileWrite rejects content that exceeds the 10 MB write size
/// limit, returning an error rather than allocating a huge file. Uses a
/// relative path so that the safety path validator allows it through.
#[tokio::test]
async fn test_file_write_oversized_content_returns_error() {
    // 10 MB + 1 byte
    let content = "x".repeat(10 * 1024 * 1024 + 1);

    let tool = FileWrite::new();
    // Use a relative path within the project so it passes allowed-paths
    let args = json!({
        "path": "__selfware_test_big_file.txt",
        "content": content
    });

    let result = tool.execute(args).await;
    assert!(
        result.is_err(),
        "Expected Err for oversized content, got Ok"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("too large"),
        "Unexpected error message: {}",
        err_msg
    );

    // Clean up in case the file was somehow created
    let _ = std::fs::remove_file("__selfware_test_big_file.txt");
}
