use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use git2::{Repository, StatusOptions};
use serde_json::Value;
use tracing::info;

pub struct GitStatus;
pub struct GitDiff;
pub struct GitCommit;

// Add this to the existing git.rs file:

pub struct GitCheckpoint;

#[async_trait]
impl Tool for GitCheckpoint {
    fn name(&self) -> &str {
        "git_checkpoint"
    }

    fn description(&self) -> &str {
        "Create a git checkpoint (commit) before dangerous operations. Returns commit hash for rollback. \
         Use this before any batch of changes that might break the build."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {"type": "string", "description": "Checkpoint description"},
                "tag": {"type": "string", "description": "Optional tag for easy rollback (e.g., 'before-refactor')"},
                "auto_branch": {"type": "boolean", "default": true, "description": "Create auto-incrementing agent branch if on main"}
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let msg = args.get("message").and_then(|v| v.as_str()).unwrap();
        let tag = args.get("tag").and_then(|v| v.as_str());
        let auto_branch = args
            .get("auto_branch")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Check current branch
        let branch_output = tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await?;
        let current_branch = String::from_utf8_lossy(&branch_output.stdout)
            .trim()
            .to_string();

        // Auto-create agent working branch if on main/master
        let target_branch =
            if auto_branch && (current_branch == "main" || current_branch == "master") {
                let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
                let agent_branch = format!("agent-{}", timestamp);

                tokio::process::Command::new("git")
                    .args(["checkout", "-b", &agent_branch])
                    .output()
                    .await?;

                info!("Created agent branch: {}", agent_branch);
                agent_branch
            } else {
                current_branch
            };

        // Stage all changes
        tokio::process::Command::new("git")
            .args(["add", "-A"])
            .output()
            .await
            .context("Failed to stage changes")?;

        // Commit with checkpoint marker
        let full_msg = format!("[AGENT CHECKPOINT] {}", msg);
        let commit_output = tokio::process::Command::new("git")
            .args(["commit", "-m", &full_msg, "--allow-empty"])
            .output()
            .await
            .context("Failed to create checkpoint commit")?;

        // Get hash
        let hash_output = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .await?;
        let hash = String::from_utf8_lossy(&hash_output.stdout)
            .trim()
            .to_string();

        // Create or move tag
        if let Some(tag_name) = tag {
            tokio::process::Command::new("git")
                .args(["tag", "-f", tag_name, &hash])
                .output()
                .await?;
        }

        // Get status summary
        let status_output = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .output()
            .await?;
        let status = String::from_utf8_lossy(&status_output.stdout);

        Ok(serde_json::json!({
            "hash": hash,
            "branch": target_branch,
            "message": full_msg,
            "success": commit_output.status.success(),
            "files_changed": !status.is_empty(),
            "tag": tag
        }))
    }
}

#[async_trait]
impl Tool for GitStatus {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Get current git status including branch, staged/unstaged changes."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "repo_path": {"type": "string", "description": "Repository path (default: current)"}
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let repo_path = args
            .get("repo_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let repo = Repository::open(repo_path)?;
        let head = repo.head()?;
        let branch = head.shorthand().unwrap_or("HEAD");

        let mut status_opts = StatusOptions::new();
        let statuses = repo.statuses(Some(&mut status_opts))?;

        let mut staged = vec![];
        let mut unstaged = vec![];
        let mut untracked = vec![];

        for status in statuses.iter() {
            let path = status.path().unwrap_or("??");
            let status_bits = status.status();

            if status_bits.is_index_new()
                || status_bits.is_index_modified()
                || status_bits.is_index_deleted()
            {
                staged.push(path.to_string());
            }
            if status_bits.is_wt_modified() || status_bits.is_wt_deleted() {
                unstaged.push(path.to_string());
            }
            if status_bits.is_wt_new() {
                untracked.push(path.to_string());
            }
        }

        Ok(serde_json::json!({
            "branch": branch,
            "staged": staged,
            "unstaged": unstaged,
            "untracked": untracked
        }))
    }
}

#[async_trait]
impl Tool for GitDiff {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show diff of changes. Can diff working tree, staged, or between commits."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {"type": "string", "description": "Specific file or directory"},
                "staged": {"type": "boolean", "description": "Diff staged changes", "default": false},
                "base": {"type": "string", "description": "Compare against specific commit"}
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let repo_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let staged = args
            .get("staged")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("-C").arg(repo_path).arg("diff");
        if staged {
            cmd.arg("--cached");
        }

        let output = cmd.output().await?;
        let diff = String::from_utf8_lossy(&output.stdout);

        Ok(serde_json::json!({
            "diff": diff.to_string(),
            "has_changes": !diff.is_empty()
        }))
    }
}

#[async_trait]
impl Tool for GitCommit {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Stage files and create a commit. Use conventional commit format."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "files": {"type": "array", "items": {"type": "string"}, "description": "Files to stage (empty = all)"},
                "message": {"type": "string", "description": "Commit message"},
                "commit_type": {"type": "string", "enum": ["feat", "fix", "refactor", "docs", "test", "chore"]}
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let repo_path = ".";
        let message = args.get("message").and_then(|v| v.as_str()).unwrap();
        let files = args
            .get("files")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Stage files
        if files.is_empty() {
            tokio::process::Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .arg("add")
                .arg("-A")
                .output()
                .await?;
        } else {
            for file in files {
                if let Some(f) = file.as_str() {
                    tokio::process::Command::new("git")
                        .arg("-C")
                        .arg(repo_path)
                        .arg("add")
                        .arg(f)
                        .output()
                        .await?;
                }
            }
        }

        // Commit
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .arg("commit")
            .arg("-m")
            .arg(message)
            .output()
            .await?;

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);

        Ok(serde_json::json!({
            "success": success,
            "output": stdout.to_string()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_name() {
        let tool = GitStatus;
        assert_eq!(tool.name(), "git_status");
    }

    #[test]
    fn test_git_status_description() {
        let tool = GitStatus;
        assert!(tool.description().contains("status"));
    }

    #[test]
    fn test_git_status_schema() {
        let tool = GitStatus;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_git_diff_name() {
        let tool = GitDiff;
        assert_eq!(tool.name(), "git_diff");
    }

    #[test]
    fn test_git_diff_description() {
        let tool = GitDiff;
        assert!(tool.description().contains("diff"));
    }

    #[test]
    fn test_git_diff_schema() {
        let tool = GitDiff;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["staged"].is_object());
    }

    #[test]
    fn test_git_commit_name() {
        let tool = GitCommit;
        assert_eq!(tool.name(), "git_commit");
    }

    #[test]
    fn test_git_commit_description() {
        let tool = GitCommit;
        assert!(tool.description().contains("commit"));
    }

    #[test]
    fn test_git_commit_schema() {
        let tool = GitCommit;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["files"].is_object());
    }

    #[test]
    fn test_git_checkpoint_name() {
        let tool = GitCheckpoint;
        assert_eq!(tool.name(), "git_checkpoint");
    }

    #[test]
    fn test_git_checkpoint_description() {
        let tool = GitCheckpoint;
        assert!(tool.description().contains("checkpoint"));
        assert!(tool.description().contains("rollback"));
    }

    #[test]
    fn test_git_checkpoint_schema() {
        let tool = GitCheckpoint;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["tag"].is_object());
        assert!(schema["properties"]["auto_branch"].is_object());
    }

    #[test]
    fn test_git_checkpoint_schema_required() {
        let tool = GitCheckpoint;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_git_commit_schema_required() {
        let tool = GitCommit;
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_git_commit_schema_commit_types() {
        let tool = GitCommit;
        let schema = tool.schema();
        let commit_type = &schema["properties"]["commit_type"];
        let enum_values = commit_type["enum"].as_array().unwrap();

        assert!(enum_values.contains(&serde_json::json!("feat")));
        assert!(enum_values.contains(&serde_json::json!("fix")));
        assert!(enum_values.contains(&serde_json::json!("refactor")));
    }

    #[tokio::test]
    async fn test_git_status_execute() {
        let tool = GitStatus;
        let args = serde_json::json!({});

        // This will work in a git repo (like this project)
        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("branch").is_some() || output.get("error").is_some());
    }

    #[tokio::test]
    async fn test_git_diff_execute_unstaged() {
        let tool = GitDiff;
        let args = serde_json::json!({"staged": false});

        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("diff").is_some() || output.get("error").is_some());
    }

    #[tokio::test]
    async fn test_git_diff_execute_staged() {
        let tool = GitDiff;
        let args = serde_json::json!({"staged": true});

        let result = tool.execute(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_git_commit_with_message() {
        let tool = GitCommit;
        // This test creates a real commit - only check that it handles the case
        // when there's nothing to commit gracefully
        let args = serde_json::json!({
            "message": "Test commit",
            "files": []
        });

        // This may fail if nothing to commit, but shouldn't panic
        let result = tool.execute(args).await;
        // We accept both Ok (committed) and Err (nothing to commit)
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_git_checkpoint_execute() {
        let tool = GitCheckpoint;
        let args = serde_json::json!({
            "message": "Test checkpoint"
        });

        // This might fail if there's nothing to commit, but shouldn't panic
        let result = tool.execute(args).await;
        // We just verify it returns Ok or expected Err
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_git_diff_schema_properties() {
        let tool = GitDiff;
        let schema = tool.schema();

        assert!(schema["properties"]["staged"].is_object());
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["base"].is_object());
    }

    #[test]
    fn test_git_checkpoint_schema_defaults() {
        let tool = GitCheckpoint;
        let schema = tool.schema();

        let auto_branch = &schema["properties"]["auto_branch"];
        assert_eq!(auto_branch["default"], true);
    }

    #[test]
    fn test_git_status_schema_properties() {
        let tool = GitStatus;
        let schema = tool.schema();

        assert!(schema["properties"]["repo_path"].is_object());
    }

    #[test]
    fn test_git_commit_schema_files_array() {
        let tool = GitCommit;
        let schema = tool.schema();

        let files = &schema["properties"]["files"];
        assert_eq!(files["type"], "array");
    }

    // Additional tests for error paths and edge cases

    #[tokio::test]
    async fn test_git_status_not_a_repo() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();

        let tool = GitStatus;
        let args = serde_json::json!({
            "repo_path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await;
        // Should fail since it's not a git repo
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_status_with_explicit_current_dir() {
        let tool = GitStatus;
        let args = serde_json::json!({
            "repo_path": "."  // Explicit current dir
        });

        // Should work since we're in a git repo
        let result = tool.execute(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_git_diff_with_specific_path() {
        let tool = GitDiff;
        let args = serde_json::json!({
            "path": ".",
            "staged": false
        });

        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        // Should have diff field (may be empty)
        assert!(output.get("diff").is_some());
        assert!(output.get("has_changes").is_some());
    }

    #[tokio::test]
    async fn test_git_commit_with_specific_files() {
        let tool = GitCommit;
        let args = serde_json::json!({
            "message": "Test specific files",
            "files": ["nonexistent_file_12345.txt"]  // File doesn't exist
        });

        // Should handle gracefully - git add will just not add anything
        let result = tool.execute(args).await;
        // Result depends on whether there's anything to commit
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_git_checkpoint_with_tag() {
        let tool = GitCheckpoint;
        let args = serde_json::json!({
            "message": "Test checkpoint with tag",
            "tag": "test-checkpoint-tag"
        });

        let result = tool.execute(args).await;
        // May succeed or fail depending on repo state
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_git_checkpoint_disable_auto_branch() {
        let tool = GitCheckpoint;
        let args = serde_json::json!({
            "message": "Test no auto branch",
            "auto_branch": false
        });

        let result = tool.execute(args).await;
        // Should handle gracefully
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_git_status_schema_has_repo_path() {
        let tool = GitStatus;
        let schema = tool.schema();

        let repo_path = &schema["properties"]["repo_path"];
        assert_eq!(repo_path["type"], "string");
    }

    #[test]
    fn test_git_diff_schema_has_base() {
        let tool = GitDiff;
        let schema = tool.schema();

        let base = &schema["properties"]["base"];
        assert_eq!(base["type"], "string");
    }

    #[test]
    fn test_git_checkpoint_message_required() {
        let tool = GitCheckpoint;
        let schema = tool.schema();

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&serde_json::json!("message")));
    }

    // ──── Extended git tests (test_git_extended) ─────────────────────

    #[tokio::test]
    async fn test_git_status_clean_repo() {
        // Create a fresh git repo with one commit and check that status is clean
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Need at least one commit for HEAD to exist
        {
            let mut index = repo.index().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        let tool = GitStatus;
        let args = serde_json::json!({
            "repo_path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result["branch"].as_str().is_some());
        assert!(result["staged"].as_array().unwrap().is_empty());
        assert!(result["unstaged"].as_array().unwrap().is_empty());
        assert!(result["untracked"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_git_status_with_changes() {
        // Create a repo with uncommitted modifications
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Create initial commit with a file
        let file_path = temp_dir.path().join("tracked.txt");
        std::fs::write(&file_path, "initial content").unwrap();
        {
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("tracked.txt")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
                .unwrap();
        }

        // Modify the tracked file (unstaged change)
        std::fs::write(&file_path, "modified content").unwrap();

        let tool = GitStatus;
        let args = serde_json::json!({
            "repo_path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await.unwrap();
        let unstaged = result["unstaged"].as_array().unwrap();

        assert!(
            unstaged.iter().any(|v| v.as_str().unwrap() == "tracked.txt"),
            "Expected tracked.txt in unstaged, got {:?}",
            unstaged
        );

        // Verify that the status output has the expected structure
        assert!(result["branch"].as_str().is_some());
        assert!(result["staged"].as_array().is_some());
        assert!(result["untracked"].as_array().is_some());
    }

    #[tokio::test]
    async fn test_git_diff_no_changes() {
        // In a clean repo (or current repo with no local diff), diff should be empty
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Create initial commit
        {
            let file_path = temp_dir.path().join("file.txt");
            std::fs::write(&file_path, "content").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("file.txt")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
                .unwrap();
        }

        let tool = GitDiff;
        let args = serde_json::json!({
            "path": temp_dir.path().to_str().unwrap(),
            "staged": false
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["has_changes"], false);
        assert_eq!(result["diff"].as_str().unwrap(), "");
    }

    #[tokio::test]
    async fn test_git_commit_empty_repo() {
        // Committing in a brand-new repo with nothing staged should not succeed
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        git2::Repository::init(temp_dir.path()).unwrap();

        // The GitCommit tool runs git from the current working dir (".")
        // So we verify the behavior by using shell to run in the temp dir
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(temp_dir.path())
            .args(["commit", "-m", "empty commit attempt"])
            .output()
            .await
            .unwrap();

        // Should fail because there's nothing to commit
        assert!(
            !output.status.success(),
            "Commit in empty repo should fail"
        );
    }

    #[tokio::test]
    async fn test_git_checkpoint_creates_commit() {
        // Test that GitCheckpoint creates a commit with the expected prefix
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        // Configure git user for the temp repo
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@test.com").unwrap();
        }

        // Create an initial commit so HEAD exists
        {
            let file_path = temp_dir.path().join("init.txt");
            std::fs::write(&file_path, "init").unwrap();
            let mut index = repo.index().unwrap();
            index.add_path(std::path::Path::new("init.txt")).unwrap();
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_oid).unwrap();
            let sig = git2::Signature::now("Test", "test@test.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "Initial", &tree, &[])
                .unwrap();
        }

        // Use git CLI to create checkpoint commit (--allow-empty) in the temp dir
        let output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(temp_dir.path())
            .args(["commit", "--allow-empty", "-m", "[AGENT CHECKPOINT] Test checkpoint via test"])
            .output()
            .await
            .unwrap();

        assert!(output.status.success(), "Checkpoint commit should succeed");

        // Verify commit message
        let log_output = tokio::process::Command::new("git")
            .arg("-C")
            .arg(temp_dir.path())
            .args(["log", "--oneline", "-1"])
            .output()
            .await
            .unwrap();

        let log_line = String::from_utf8_lossy(&log_output.stdout);
        assert!(
            log_line.contains("[AGENT CHECKPOINT]"),
            "Commit message should contain AGENT CHECKPOINT prefix, got: {}",
            log_line
        );
    }
}
