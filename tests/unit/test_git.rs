//! Git tool tests
//!
//! Tests for GitStatus, GitDiff, and GitCommit tools
//! using temporary git repositories.

use selfware::tools::{git::{GitStatus, GitDiff, GitCommit}, Tool};
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
    fs::write(dir.path().join("new_file.txt"), "content").unwrap();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let untracked = result.get("untracked").unwrap().as_array().unwrap();
    assert!(!untracked.is_empty());
    assert!(untracked.iter().any(|f| f.as_str().unwrap().contains("new_file")));
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
    assert!(!staged.is_empty());
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
    assert!(!unstaged.is_empty());
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
    // Default branch could be main or master
    assert!(branch == "main" || branch == "master");
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

// ==================== GitDiff Tests ====================

#[tokio::test]
async fn test_git_diff_no_changes() {
    let dir = create_test_repo();

    let tool = GitDiff;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let diff = result.get("diff").unwrap().as_str().unwrap();
    assert!(diff.is_empty() || !diff.contains("diff --git"));
}

#[tokio::test]
async fn test_git_diff_with_changes() {
    let dir = create_test_repo();
    fs::write(dir.path().join("README.md"), "# Modified Content").unwrap();

    let tool = GitDiff;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();
    let diff = result.get("diff").unwrap().as_str().unwrap();
    assert!(diff.contains("Modified Content") || diff.contains("README.md"));
}

#[tokio::test]
async fn test_git_diff_staged() {
    let dir = create_test_repo();
    fs::write(dir.path().join("staged.txt"), "new content").unwrap();

    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    let tool = GitDiff;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap(),
        "staged": true
    });

    let result = tool.execute(args).await.unwrap();
    let diff = result.get("diff").unwrap().as_str().unwrap();
    assert!(diff.contains("staged.txt") || diff.contains("new content"));
}

#[tokio::test]
async fn test_git_diff_specific_path() {
    let dir = create_test_repo();
    fs::write(dir.path().join("README.md"), "# Changed").unwrap();
    fs::write(dir.path().join("other.txt"), "other content").unwrap();

    let tool = GitDiff;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap(),
        "path": "README.md"
    });

    let result = tool.execute(args).await.unwrap();
    let diff = result.get("diff").unwrap().as_str().unwrap();
    // Should only contain README.md changes
    assert!(!diff.contains("other.txt"));
}

// ==================== GitCommit Tests ====================

#[tokio::test]
async fn test_git_commit_success() {
    let dir = create_test_repo();
    fs::write(dir.path().join("new_file.txt"), "content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    let tool = GitCommit;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap(),
        "message": "Add new file"
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
    assert!(result.get("hash").is_some());
}

#[tokio::test]
async fn test_git_commit_empty() {
    let dir = create_test_repo();
    // No changes to commit

    let tool = GitCommit;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap(),
        "message": "Empty commit"
    });

    let result = tool.execute(args).await;
    // Should either fail or indicate no changes
    if let Ok(res) = result {
        let success = res.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
        // Empty commits without --allow-empty should fail
        assert!(!success || res.get("files_changed").is_some());
    }
}

#[tokio::test]
async fn test_git_commit_multiline_message() {
    let dir = create_test_repo();
    fs::write(dir.path().join("feature.txt"), "feature content").unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    let tool = GitCommit;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap(),
        "message": "Add feature\n\nThis is a longer description.\nWith multiple lines."
    });

    let result = tool.execute(args).await.unwrap();
    assert!(result.get("success").unwrap().as_bool().unwrap());
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

#[test]
fn test_git_diff_metadata() {
    let tool = GitDiff;
    assert_eq!(tool.name(), "git_diff");
    assert!(tool.description().contains("diff"));
}

#[test]
fn test_git_commit_metadata() {
    let tool = GitCommit;
    assert_eq!(tool.name(), "git_commit");
    assert!(!tool.description().is_empty());
}

// ==================== Edge Cases ====================

#[tokio::test]
async fn test_git_status_with_multiple_changes() {
    let dir = create_test_repo();

    // Create multiple files in different states
    fs::write(dir.path().join("untracked.txt"), "untracked").unwrap();

    fs::write(dir.path().join("staged.txt"), "staged").unwrap();
    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(dir.path())
        .output()
        .expect("Failed to stage file");

    fs::write(dir.path().join("README.md"), "modified").unwrap();

    let tool = GitStatus;
    let args = json!({
        "repo_path": dir.path().to_str().unwrap()
    });

    let result = tool.execute(args).await.unwrap();

    let staged = result.get("staged").unwrap().as_array().unwrap();
    let unstaged = result.get("unstaged").unwrap().as_array().unwrap();
    let untracked = result.get("untracked").unwrap().as_array().unwrap();

    assert!(!staged.is_empty(), "Should have staged files");
    assert!(!unstaged.is_empty(), "Should have unstaged files");
    assert!(!untracked.is_empty(), "Should have untracked files");
}
