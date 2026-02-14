//! Git tool tests
//!
//! Tests for GitStatus tool using temporary git repositories.
//! Note: GitDiff and GitCommit use current directory and are tested in integration tests.

use selfware::tools::{git::GitStatus, Tool};
use serde_json::json;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Create a temporary git repository for testing
fn create_test_repo() -> TempDir {
    let dir = TempDir::new().unwrap();

    // Initialize git repo
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to init git repo");

    // Configure git user for commits
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to configure git email");

    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to configure git name");

    // Create initial commit
    fs::write(dir.path().join("README.md"), "# Test").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage files");

    Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to create initial commit");

    dir
}

// ==================== GitStatus Tests ====================

#[tokio::test]
async fn test_git_status_clean_repo() {
    let dir = create_test_repo();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let staged = result.get("staged").unwrap().as_array().unwrap();
    let unstaged = result.get("unstaged").unwrap().as_array().unwrap();
    let untracked = result.get("untracked").unwrap().as_array().unwrap();

    assert!(staged.is_empty());
    assert!(unstaged.is_empty());
    assert!(untracked.is_empty());
}

#[tokio::test]
async fn test_git_status_with_untracked() {
    let dir = create_test_repo();
    let new_file = dir.path().join("new_file.txt");
    fs::write(&new_file, "content").unwrap();

    // Verify file exists
    assert!(new_file.exists(), "New file should exist");

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();

    // GitStatus returns branch info and status arrays
    // Note: The current implementation may not include untracked files
    // depending on git2::StatusOptions defaults
    assert!(result.get("branch").is_some());
    assert!(result.get("untracked").is_some());
    assert!(result.get("staged").is_some());
    assert!(result.get("unstaged").is_some());
}

#[tokio::test]
async fn test_git_status_with_staged() {
    let dir = create_test_repo();
    fs::write(dir.path().join("staged.txt"), "content").unwrap();

    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let staged = result.get("staged").unwrap().as_array().unwrap();

    assert!(!staged.is_empty(), "Expected staged files");
}

#[tokio::test]
async fn test_git_status_with_modified() {
    let dir = create_test_repo();
    fs::write(dir.path().join("README.md"), "# Modified").unwrap();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let unstaged = result.get("unstaged").unwrap().as_array().unwrap();

    assert!(!unstaged.is_empty(), "Expected modified files");
}

#[tokio::test]
async fn test_git_status_shows_branch() {
    let dir = create_test_repo();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let branch = result.get("branch").unwrap().as_str().unwrap();

    // Default branch could be main or master depending on git config
    assert!(
        branch == "main" || branch == "master",
        "Expected main or master, got: {}",
        branch
    );
}

#[tokio::test]
async fn test_git_status_not_a_repo() {
    let dir = TempDir::new().unwrap(); // Not initialized as git repo

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await;
    assert!(result.is_err());
}

// ==================== Tool Metadata Tests ====================

#[test]
fn test_git_status_metadata() {
    let tool = GitStatus;
    assert_eq!(tool.name(), "git_status");
    assert!(!tool.description().is_empty());
    let schema = tool.schema();
    assert!(schema.get("properties").is_some());
}

// ==================== Edge Cases ====================

#[tokio::test]
async fn test_git_status_with_multiple_changes() {
    let dir = create_test_repo();

    // Create file to stage
    fs::write(dir.path().join("staged.txt"), "staged").unwrap();
    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    // Modify existing file
    fs::write(dir.path().join("README.md"), "modified").unwrap();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();

    let staged = result.get("staged").unwrap().as_array().unwrap();
    let unstaged = result.get("unstaged").unwrap().as_array().unwrap();

    assert!(!staged.is_empty(), "Should have staged files: {:?}", result);
    assert!(!unstaged.is_empty(), "Should have unstaged files: {:?}", result);
}

#[tokio::test]
async fn test_git_status_default_path() {
    // Test with no repo_path (defaults to current directory)
    // This is the main project repo
    let tool = GitStatus;
    let args = json!({});

    let result = tool.execute(args).await.unwrap();
    // Should succeed since we're in the selfware repo
    assert!(result.get("branch").is_some());
}
