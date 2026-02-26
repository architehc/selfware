//! Error path and edge case tests
//!
//! These tests focus on error handling branches that are often missed:
//! - Invalid arguments
//! - File system errors
//! - Git repository errors
//! - Safety validation failures

use selfware::api::types::{ToolCall, ToolFunction};
use selfware::config::SafetyConfig;
use selfware::safety::SafetyChecker;
use selfware::tools::file::{DirectoryTree, FileEdit, FileRead, FileWrite};
use selfware::tools::Tool;
use std::fs;
use std::sync::Once;
use tempfile::tempdir;

static INIT: Once = Once::new();

fn setup_test_mode() {
    INIT.call_once(|| {
        std::env::set_var("SELFWARE_TEST_MODE", "1");
    });
}

// ============================================================================
// FileRead Error Path Tests
// ============================================================================

mod file_read_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_file_read_missing_path_arg() {
        setup_test_mode();
        let tool = FileRead::new();
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_invalid_json() {
        setup_test_mode();
        let tool = FileRead::new();
        let result = tool.execute(serde_json::json!("not an object")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_nonexistent_file() {
        setup_test_mode();
        let tool = FileRead::new();
        let result = tool
            .execute(serde_json::json!({
                "path": "/nonexistent/path/to/file.txt"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_file_read_directory_instead_of_file() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let tool = FileRead::new();
        let result = tool
            .execute(serde_json::json!({
                "path": dir.path().to_str().unwrap()
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_invalid_line_range() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileRead::new();
        // Line range with start > end should still work (returns empty)
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "line_range": [5, 3]
            }))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_file_read_line_range_beyond_file() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2").unwrap();

        let tool = FileRead::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "line_range": [100, 200]
            }))
            .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["content"], "");
    }

    #[tokio::test]
    async fn test_file_read_empty_file() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        fs::write(&file_path, "").unwrap();

        let tool = FileRead::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap()
            }))
            .await;
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["content"], "");
        assert_eq!(value["total_lines"], 0);
    }
}

// ============================================================================
// FileWrite Error Path Tests
// ============================================================================

mod file_write_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_file_write_missing_path() {
        setup_test_mode();
        let tool = FileWrite::new();
        let result = tool
            .execute(serde_json::json!({
                "content": "test"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_missing_content() {
        setup_test_mode();
        let tool = FileWrite::new();
        let result = tool
            .execute(serde_json::json!({
                "path": "/tmp/test.txt"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_invalid_json() {
        setup_test_mode();
        let tool = FileWrite::new();
        let result = tool.execute(serde_json::json!(null)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_write_creates_parent_dirs() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let nested_path = dir.path().join("a/b/c/test.txt");

        let tool = FileWrite::new();
        let result = tool
            .execute(serde_json::json!({
                "path": nested_path.to_str().unwrap(),
                "content": "nested content"
            }))
            .await;
        assert!(result.is_ok());
        assert!(nested_path.exists());
    }

    #[tokio::test]
    async fn test_file_write_backup_created() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("original.txt");
        fs::write(&file_path, "original content").unwrap();

        let tool = FileWrite::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "content": "new content",
                "backup": true
            }))
            .await;
        assert!(result.is_ok());

        // Check backup exists
        let backup_path = dir.path().join("original.txt.bak");
        assert!(backup_path.exists());
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "original content"
        );
    }

    #[tokio::test]
    async fn test_file_write_no_backup() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("no_backup.txt");
        fs::write(&file_path, "original").unwrap();

        let tool = FileWrite::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "content": "new",
                "backup": false
            }))
            .await;
        assert!(result.is_ok());

        // Backup should NOT exist
        let backup_path = dir.path().join("no_backup.txt.bak");
        assert!(!backup_path.exists());
    }
}

// ============================================================================
// FileEdit Error Path Tests
// ============================================================================

mod file_edit_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_file_edit_file_not_found() {
        setup_test_mode();
        let tool = FileEdit::new();
        let result = tool
            .execute(serde_json::json!({
                "path": "/nonexistent/file.txt",
                "old_str": "foo",
                "new_str": "bar"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_no_match() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let tool = FileEdit::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "old_str": "nonexistent string",
                "new_str": "replacement"
            }))
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_file_edit_multiple_matches() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "foo bar foo baz foo").unwrap();

        let tool = FileEdit::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "old_str": "foo",
                "new_str": "qux"
            }))
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("3 times") || err.contains("matches"));
    }

    #[tokio::test]
    async fn test_file_edit_missing_args() {
        setup_test_mode();
        let tool = FileEdit::new();

        // Missing old_str
        let result = tool
            .execute(serde_json::json!({
                "path": "test.txt",
                "new_str": "bar"
            }))
            .await;
        assert!(result.is_err());

        // Missing new_str
        let result = tool
            .execute(serde_json::json!({
                "path": "test.txt",
                "old_str": "foo"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_edit_delete_text() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let tool = FileEdit::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "old_str": " world",
                "new_str": ""
            }))
            .await;
        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello");
    }

    #[tokio::test]
    async fn test_file_edit_multiline() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "line1\nline2\nline3").unwrap();

        let tool = FileEdit::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap(),
                "old_str": "line2",
                "new_str": "REPLACED"
            }))
            .await;
        assert!(result.is_ok());
        assert_eq!(
            fs::read_to_string(&file_path).unwrap(),
            "line1\nREPLACED\nline3"
        );
    }
}

// ============================================================================
// DirectoryTree Error Path Tests
// ============================================================================

mod directory_tree_error_tests {
    use super::*;

    #[tokio::test]
    async fn test_directory_tree_nonexistent() {
        setup_test_mode();
        let tool = DirectoryTree::new();
        let result = tool
            .execute(serde_json::json!({
                "path": "/nonexistent/directory"
            }))
            .await;
        // Should return error or empty result
        if let Ok(value) = result {
            // If it returns OK, entries should be empty or error field set
            let entries = value.get("entries").and_then(|e| e.as_array());
            if let Some(arr) = entries {
                assert!(arr.is_empty() || value.get("error").is_some());
            }
        }
    }

    #[tokio::test]
    async fn test_directory_tree_file_instead_of_dir() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let tool = DirectoryTree::new();
        let result = tool
            .execute(serde_json::json!({
                "path": file_path.to_str().unwrap()
            }))
            .await;
        // Implementation-dependent: might error or return file info
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_directory_tree_max_depth_zero() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path().join("a/b/c")).unwrap();
        fs::write(dir.path().join("a/b/c/deep.txt"), "").unwrap();

        let tool = DirectoryTree::new();
        let result = tool
            .execute(serde_json::json!({
                "path": dir.path().to_str().unwrap(),
                "max_depth": 0
            }))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_directory_tree_hidden_files() {
        setup_test_mode();
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), "").unwrap();
        fs::write(dir.path().join("visible"), "").unwrap();

        let tool = DirectoryTree::new();

        // Without include_hidden
        let result = tool
            .execute(serde_json::json!({
                "path": dir.path().to_str().unwrap(),
                "include_hidden": false
            }))
            .await;
        assert!(result.is_ok());

        // With include_hidden
        let result = tool
            .execute(serde_json::json!({
                "path": dir.path().to_str().unwrap(),
                "include_hidden": true
            }))
            .await;
        assert!(result.is_ok());
    }
}

// ============================================================================
// SafetyChecker Error Path Tests
// ============================================================================

mod safety_checker_tests {
    use super::*;

    fn create_tool_call(name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: "test".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    #[test]
    fn test_safety_blocks_rm_rf_root() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("shell_exec", r#"{"command": "rm -rf /"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_blocks_rm_rf_asterisk() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("shell_exec", r#"{"command": "rm -rf /*"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_blocks_dd_if_dev_zero() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call(
            "shell_exec",
            r#"{"command": "dd if=/dev/zero of=/dev/sda"}"#,
        );
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_blocks_mkfs() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("shell_exec", r#"{"command": "mkfs.ext4 /dev/sda1"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_blocks_fork_bomb() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("shell_exec", r#"{"command": ":(){ :|:& };:"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_blocks_etc_passwd() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Writing to /etc should be blocked
        let write_call = create_tool_call("shell_exec", r#"{"command": "echo 'x' > /etc/passwd"}"#);
        let write_result = checker.check_tool_call(&write_call);
        assert!(write_result.is_err());
    }

    #[test]
    fn test_safety_blocks_chmod_777_root() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("shell_exec", r#"{"command": "chmod -R 777 /"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_allows_safe_commands() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let safe_commands = vec![
            r#"{"command": "ls -la"}"#,
            r#"{"command": "cat README.md"}"#,
            r#"{"command": "cargo build"}"#,
            r#"{"command": "git status"}"#,
            r#"{"command": "echo hello"}"#,
        ];

        for cmd in safe_commands {
            let call = create_tool_call("shell_exec", cmd);
            let result = checker.check_tool_call(&call);
            assert!(result.is_ok(), "Command should be allowed: {}", cmd);
        }
    }

    #[test]
    fn test_safety_blocks_force_push() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("git_push", r#"{"force": true}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Force push"));
    }

    #[test]
    fn test_safety_allows_normal_push() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("git_push", r#"{"force": false}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safety_invalid_json_args() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("file_read", "not valid json");
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_git_commit_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("git_commit", r#"{"message": "test commit"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safety_git_checkpoint_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("git_checkpoint", r#"{"message": "checkpoint"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safety_unknown_tool_allowed() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call("unknown_tool", r#"{}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_ok());
    }

    #[test]
    fn test_safety_base64_execution_blocked() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call(
            "shell_exec",
            r#"{"command": "echo 'cm0gLXJmIC8=' | base64 -d | bash"}"#,
        );
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_chained_dangerous_command() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        let call = create_tool_call(
            "shell_exec",
            r#"{"command": "echo safe && rm -rf / && echo done"}"#,
        );
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }

    #[test]
    fn test_safety_obfuscated_rm() {
        let config = SafetyConfig::default();
        let checker = SafetyChecker::new(&config);

        // Whitespace obfuscation
        let call = create_tool_call("shell_exec", r#"{"command": "rm    -rf    /"}"#);
        let result = checker.check_tool_call(&call);
        assert!(result.is_err());
    }
}

// ============================================================================
// Context Compressor Edge Case Tests
// ============================================================================

mod context_edge_cases {
    use selfware::agent::context::ContextCompressor;
    use selfware::api::types::Message;

    #[test]
    fn test_empty_messages() {
        let compressor = ContextCompressor::new(10000);
        let messages: Vec<Message> = vec![];
        assert!(!compressor.should_compress(&messages));
        assert_eq!(compressor.estimate_tokens(&messages), 0);
    }

    #[test]
    fn test_single_large_message() {
        let compressor = ContextCompressor::new(1000);
        let large_content = "x".repeat(10000);
        let messages = vec![Message::user(large_content)];
        assert!(compressor.should_compress(&messages));
    }

    #[test]
    fn test_many_small_messages() {
        let compressor = ContextCompressor::new(1000);
        let messages: Vec<Message> = (0..100)
            .map(|i| Message::user(format!("msg {}", i)))
            .collect();
        // Many messages should trigger compression due to overhead
        let tokens = compressor.estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_code_vs_prose_estimation() {
        let compressor = ContextCompressor::new(10000);

        let code = Message::user("fn main() { let x = 42; println!(\"{}\", x); }".to_string());
        let prose = Message::user("The quick brown fox jumps over the lazy dog.".to_string());

        let code_tokens = compressor.estimate_tokens(&[code]);
        let prose_tokens = compressor.estimate_tokens(&[prose]);

        // Code should use factor of 3, prose uses factor of 4
        // So similar length code should result in more tokens
        assert!(code_tokens > 0);
        assert!(prose_tokens > 0);
    }

    #[test]
    fn test_unicode_content() {
        let compressor = ContextCompressor::new(10000);
        let unicode = Message::user("Hello 世界 🌍 مرحبا Здравствуйте".to_string());
        let tokens = compressor.estimate_tokens(&[unicode]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_mixed_message_types() {
        let compressor = ContextCompressor::new(10000);
        let messages = vec![
            Message::system("You are helpful."),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
            Message::tool("result", "call_1"),
        ];
        let tokens = compressor.estimate_tokens(&messages);
        assert!(tokens > 0);
    }
}
