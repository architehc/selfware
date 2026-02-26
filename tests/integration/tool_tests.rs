//! Integration tests for individual tool execution
//!
//! These tests verify that the agent can successfully call tools
//! and handle responses from the local model.

use super::helpers::*;
use selfware::tools::{Tool, ToolRegistry};
use serde_json::json;
use std::time::Duration;

// Re-import the macros from the test crate root
use crate::{skip_if_no_model, skip_if_slow};

/// Test that file_read tool works correctly
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_file_read_tool_execution() {
    let config = test_config();
    skip_if_no_model!(&config);

    let registry = ToolRegistry::new();
    let tool = registry
        .get("file_read")
        .expect("file_read tool should exist");

    let args = json!({
        "path": "./Cargo.toml"
    });

    let result = tokio::time::timeout(Duration::from_secs(10), tool.execute(args)).await;

    assert!(result.is_ok(), "Tool execution should not timeout");
    let result = result.unwrap();
    assert!(result.is_ok(), "file_read should succeed for existing file");

    let value = result.unwrap();
    let content = value.get("content").and_then(|c| c.as_str()).unwrap_or("");
    assert!(
        content.contains("[package]"),
        "Should contain Cargo.toml content"
    );
    assert!(content.contains("selfware"), "Should contain package name");
}

/// Test that file_read handles missing files gracefully
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_file_read_missing_file() {
    let registry = ToolRegistry::new();
    let tool = registry
        .get("file_read")
        .expect("file_read tool should exist");

    let args = json!({
        "path": "./nonexistent_file_12345.txt"
    });

    let result = tool.execute(args).await;
    assert!(result.is_err(), "file_read should fail for missing file");
}

/// Test shell_exec tool with simple command
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_shell_exec_echo() {
    let registry = ToolRegistry::new();
    let tool = registry
        .get("shell_exec")
        .expect("shell_exec tool should exist");

    let args = json!({
        "command": "echo 'hello integration test'"
    });

    let result = tokio::time::timeout(Duration::from_secs(30), tool.execute(args)).await;

    assert!(result.is_ok(), "Tool execution should not timeout");
    let result = result.unwrap();
    assert!(result.is_ok(), "shell_exec should succeed for echo");

    let value = result.unwrap();
    let stdout = value.get("stdout").and_then(|s| s.as_str()).unwrap_or("");
    assert!(
        stdout.contains("hello integration test"),
        "Should contain echoed text"
    );
}

/// Test directory_tree tool
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_directory_tree_execution() {
    let registry = ToolRegistry::new();
    let tool = registry
        .get("directory_tree")
        .expect("directory_tree tool should exist");

    let args = json!({
        "path": "./src",
        "max_depth": 2
    });

    let result = tokio::time::timeout(Duration::from_secs(10), tool.execute(args)).await;

    assert!(result.is_ok(), "Tool execution should not timeout");
    let result = result.unwrap();
    assert!(result.is_ok(), "directory_tree should succeed");

    let value = result.unwrap();

    // Check that we got entries back
    let entries = value.get("entries").and_then(|e| e.as_array());
    assert!(entries.is_some(), "Should have entries array");
    let entries = entries.unwrap();

    // Check that we found some files
    let has_rs_files = entries.iter().any(|e| {
        e.get("path")
            .and_then(|p| p.as_str())
            .map(|p| p.ends_with(".rs"))
            .unwrap_or(false)
    });
    assert!(has_rs_files, "Should contain .rs source files");
}

/// Test git_status tool (if in git repo)
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_git_status_execution() {
    let registry = ToolRegistry::new();
    let tool = registry
        .get("git_status")
        .expect("git_status tool should exist");

    let args = json!({});

    let result = tokio::time::timeout(Duration::from_secs(10), tool.execute(args)).await;

    assert!(result.is_ok(), "Tool execution should not timeout");
    let result = result.unwrap();
    // git_status might fail if not in a repo, but shouldn't panic
    match result {
        Ok(value) => {
            // If it succeeds, should have status info
            assert!(value.get("status").is_some() || value.get("branch").is_some());
        }
        Err(_) => {
            // Acceptable if not in git repo
        }
    }
}

/// Test cargo_check tool
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_cargo_check_execution() {
    skip_if_slow!();

    let registry = ToolRegistry::new();
    let tool = registry
        .get("cargo_check")
        .expect("cargo_check tool should exist");

    let args = json!({});

    // Cargo operations can be slow
    let result = tokio::time::timeout(Duration::from_secs(120), tool.execute(args)).await;

    assert!(result.is_ok(), "Tool execution should not timeout");
    let result = result.unwrap();
    assert!(
        result.is_ok(),
        "cargo_check should succeed for this project"
    );
}

/// Test that all tools have valid schemas
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_all_tools_have_valid_schemas() {
    let registry = ToolRegistry::new();

    for tool in registry.list() {
        let schema = tool.schema();
        assert!(
            schema.is_object(),
            "Tool {} should have object schema",
            tool.name()
        );
        assert!(
            schema.get("type").is_some(),
            "Tool {} schema should have type",
            tool.name()
        );
    }
}

/// Test file_write and file_read roundtrip
#[tokio::test]
#[cfg(feature = "integration")]
async fn test_file_write_read_roundtrip() {
    let mut write_tool = selfware::tools::file::FileWrite::new();
    let mut read_tool = selfware::tools::file::FileRead::new();

    let cfg = selfware::config::SafetyConfig {
        allowed_paths: vec!["/**".to_string()],
        ..Default::default()
    };
    write_tool.safety_config = Some(cfg.clone());
    read_tool.safety_config = Some(cfg);

    let test_content = "Integration test content: Hello, Selfware!";
    let test_path = std::env::temp_dir()
        .join("selfware_integration_test_file.txt")
        .to_string_lossy()
        .to_string();
    let test_path = test_path.as_str();

    // Write the file
    let write_args = json!({
        "path": test_path,
        "content": test_content
    });

    let write_result = write_tool.execute(write_args).await;
    assert!(write_result.is_ok(), "file_write should succeed");

    // Read the file back
    let read_args = json!({
        "path": test_path
    });

    let read_result = read_tool.execute(read_args).await;
    assert!(read_result.is_ok(), "file_read should succeed");

    let value = read_result.unwrap();
    let content = value.get("content").and_then(|c| c.as_str()).unwrap_or("");
    assert_eq!(content.trim(), test_content, "Content should match");

    // Cleanup
    let _ = std::fs::remove_file(test_path);
}
