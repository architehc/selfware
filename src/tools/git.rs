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
    fn name(&self) -> &str { "git_checkpoint" }
    
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
        let auto_branch = args.get("auto_branch").and_then(|v| v.as_bool()).unwrap_or(true);
        
        // Check current branch
        let branch_output = tokio::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output().await?;
        let current_branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
        
        // Auto-create agent working branch if on main/master
        let target_branch = if auto_branch && (current_branch == "main" || current_branch == "master") {
            let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            let agent_branch = format!("agent-{}", timestamp);
            
            tokio::process::Command::new("git")
                .args(["checkout", "-b", &agent_branch])
                .output().await?;
            
            info!("Created agent branch: {}", agent_branch);
            agent_branch
        } else {
            current_branch
        };
        
        // Stage all changes
        tokio::process::Command::new("git")
            .args(["add", "-A"])
            .output().await
            .context("Failed to stage changes")?;
        
        // Commit with checkpoint marker
        let full_msg = format!("[AGENT CHECKPOINT] {}", msg);
        let commit_output = tokio::process::Command::new("git")
            .args(["commit", "-m", &full_msg, "--allow-empty"])
            .output().await
            .context("Failed to create checkpoint commit")?;
            
        // Get hash
        let hash_output = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output().await?;
        let hash = String::from_utf8_lossy(&hash_output.stdout).trim().to_string();
        
        // Create or move tag
        if let Some(tag_name) = tag {
            tokio::process::Command::new("git")
                .args(["tag", "-f", tag_name, &hash])
                .output().await?;
        }
        
        // Get status summary
        let status_output = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .output().await?;
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
    fn name(&self) -> &str { "git_status" }
    
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
        let repo_path = args.get("repo_path")
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
            
            if status_bits.is_index_new() || status_bits.is_index_modified() || status_bits.is_index_deleted() {
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
    fn name(&self) -> &str { "git_diff" }
    
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
        let staged = args.get("staged").and_then(|v| v.as_bool()).unwrap_or(false);
        
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
    fn name(&self) -> &str { "git_commit" }
    
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
        let files = args.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        
        // Stage files
        if files.is_empty() {
            tokio::process::Command::new("git")
                .arg("-C").arg(repo_path).arg("add").arg("-A")
                .output().await?;
        } else {
            for file in files {
                if let Some(f) = file.as_str() {
                    tokio::process::Command::new("git")
                        .arg("-C").arg(repo_path).arg("add").arg(f)
                        .output().await?;
                }
            }
        }
        
        // Commit
        let output = tokio::process::Command::new("git")
            .arg("-C").arg(repo_path).arg("commit").arg("-m").arg(message)
            .output().await?;
            
        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        Ok(serde_json::json!({
            "success": success,
            "output": stdout.to_string()
        }))
    }
}
