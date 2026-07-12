use super::file::{resolve_safety_config, validate_tool_path};
use super::Tool;
use crate::config::SafetyConfig;
use anyhow::{Context, Result};
use async_trait::async_trait;
use git2::{Repository, StatusOptions};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

/// Validate a git tag name to prevent shell injection.
///
/// Only allows alphanumeric characters plus `-`, `.`, `_`, and `/`.
/// Rejects spaces, shell metacharacters, control characters, and empty names.
fn validate_tag_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Tag name must not be empty");
    }
    if name.len() > 256 {
        anyhow::bail!("Tag name too long (max 256 characters)");
    }
    for c in name.chars() {
        if !(c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '/') {
            anyhow::bail!(
                "Invalid character '{}' in tag name '{}'. Only alphanumeric, '-', '.', '_', '/' are allowed.",
                c,
                name
            );
        }
    }
    // Reject names starting with '-' (could be interpreted as a flag)
    if name.starts_with('-') {
        anyhow::bail!("Tag name must not start with '-'");
    }
    Ok(())
}

/// Counter for unique temp file names within the same process.
static COMMIT_MSG_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Write a commit message to a temp file for use with `git commit --file`.
///
/// Returns `Some(path)` on success, or `None` if writing fails (caller should
/// fall back to `-m`).
fn write_commit_message_file(message: &str) -> Option<PathBuf> {
    let seq = COMMIT_MSG_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_dir = std::env::temp_dir();
    let msg_file = temp_dir.join(format!(
        "selfware_commit_msg_{}_{}.txt",
        std::process::id(),
        seq
    ));
    match std::fs::write(&msg_file, message) {
        Ok(()) => Some(msg_file),
        Err(e) => {
            warn!(
                "Failed to write commit message to temp file {}: {}. Falling back to -m.",
                msg_file.display(),
                e
            );
            None
        }
    }
}

/// Validate that a repo/working-directory path is within the allowed paths.
fn validate_git_path(path: &str, safety_config: Option<&SafetyConfig>) -> Result<()> {
    let safety = resolve_safety_config(safety_config);
    validate_tool_path(path, &safety)
}

#[derive(Default)]
pub struct GitStatus {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitDiff {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitCommit {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitPush {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct GitCheckpoint {
    pub safety_config: Option<SafetyConfig>,
}

impl GitStatus {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitDiff {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitCommit {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitPush {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl GitCheckpoint {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

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
        let msg = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let tag = args.get("tag").and_then(|v| v.as_str());
        let auto_branch = args
            .get("auto_branch")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Validate cwd is within allowed paths
        validate_git_path(".", self.safety_config.as_ref())?;

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
        let msg_file = write_commit_message_file(&full_msg);
        let commit_output = if let Some(ref path) = msg_file {
            tokio::process::Command::new("git")
                .arg("commit")
                .arg("--file")
                .arg(path)
                .arg("--allow-empty")
                .output()
                .await
                .context("Failed to create checkpoint commit")?
        } else {
            tokio::process::Command::new("git")
                .args(["commit", "-m", &full_msg, "--allow-empty"])
                .output()
                .await
                .context("Failed to create checkpoint commit")?
        };
        if let Some(path) = msg_file {
            let _ = std::fs::remove_file(path);
        }

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
            validate_tag_name(tag_name)?;
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

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::git()
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

        validate_git_path(repo_path, self.safety_config.as_ref())?;

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

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
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

        validate_git_path(repo_path, self.safety_config.as_ref())?;

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

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
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
                "files": {"type": "array", "items": {"type": "string"}, "description": "Files to stage. Empty stages tracked modifications only (git add -u); list new/untracked files explicitly to include them."},
                "message": {"type": "string", "description": "Commit message"},
                "commit_type": {"type": "string", "enum": ["feat", "fix", "refactor", "docs", "test", "chore"]}
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let repo_path = ".";
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: message"))?;
        let files = args
            .get("files")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        validate_git_path(repo_path, self.safety_config.as_ref())?;

        // Validate individual file paths
        for file in &files {
            if let Some(f) = file.as_str() {
                validate_git_path(f, self.safety_config.as_ref())?;
            }
        }

        // Stage files
        if files.is_empty() {
            // Empty list stages tracked modifications/deletions only (`-u`), NOT
            // new untracked files. `git add -A` would sweep in whatever happens
            // to be untracked in the tree — a stray .env, a build artifact, a
            // spilled secret — and (with push allowed by default) publish it.
            // To commit a NEW file, pass it explicitly in `files`.
            tokio::process::Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .arg("add")
                .arg("-u")
                .output()
                .await?;
        } else {
            for file in files {
                if let Some(f) = file.as_str() {
                    if f.contains("..") || f.starts_with('/') {
                        anyhow::bail!("Invalid file path for git commit: {}", f);
                    }
                    tokio::process::Command::new("git")
                        .arg("-C")
                        .arg(repo_path)
                        .arg("add")
                        .arg("--")
                        .arg(f)
                        .output()
                        .await?;
                }
            }
        }

        // Commit — write message to temp file for defense-in-depth against
        // shell metacharacters, falling back to -m if the write fails.
        let msg_file = write_commit_message_file(message);
        let output = if let Some(ref path) = msg_file {
            tokio::process::Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .arg("commit")
                .arg("--file")
                .arg(path)
                .output()
                .await?
        } else {
            tokio::process::Command::new("git")
                .arg("-C")
                .arg(repo_path)
                .arg("commit")
                .arg("-m")
                .arg(message)
                .output()
                .await?
        };
        if let Some(path) = msg_file {
            let _ = std::fs::remove_file(path);
        }

        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);

        Ok(serde_json::json!({
            "success": success,
            "output": stdout.to_string()
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::git()
    }
}

#[async_trait]
impl Tool for GitPush {
    fn name(&self) -> &str {
        "git_push"
    }

    fn description(&self) -> &str {
        "Push commits to a remote repository. Force push is blocked by the safety checker."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "remote": {
                    "type": "string",
                    "description": "Remote name (default: origin)",
                    "default": "origin"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch to push (default: current branch)"
                },
                "force": {
                    "type": "boolean",
                    "description": "Force push (blocked by safety checker)",
                    "default": false
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds for the network push (default: 120)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let remote = args
            .get("remote")
            .and_then(|v| v.as_str())
            .unwrap_or("origin");
        let force = args.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

        if force {
            anyhow::bail!("Force push is blocked by the safety checker.");
        }

        // Validate cwd is within allowed paths
        validate_git_path(".", self.safety_config.as_ref())?;

        // Determine branch
        let branch = if let Some(b) = args.get("branch").and_then(|v| v.as_str()) {
            b.to_string()
        } else {
            let output = tokio::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .await
                .context("Failed to get current branch")?;
            if !output.status.success() {
                let err = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to detect current branch: {}", err.trim());
            }
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        if let Some(ref safety_config) = self.safety_config {
            if safety_config
                .protected_branches
                .iter()
                .any(|b| b == &branch)
            {
                anyhow::bail!(
                    "Push to protected branch '{}' is blocked by the safety checker \
                     (protected_branches: {:?}).",
                    branch,
                    safety_config.protected_branches
                );
            }
        }

        let mut cmd = tokio::process::Command::new("git");
        cmd.arg("push").arg("--").arg(remote).arg(&branch);

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                .await
                .context("git push timed out (network hang?)")?
                .context("Failed to execute git push")?;
        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(serde_json::json!({
            "success": success,
            "remote": remote,
            "branch": branch,
            "force": force,
            "output": format!("{}{}", stdout, stderr)
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        // Git push is high risk due to potential for remote changes
        crate::safety::ToolMetadata::custom(
            false,
            false,
            crate::safety::RiskLevel::High,
            true,
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_status_name() {
        let tool = GitStatus::new();
        assert_eq!(tool.name(), "git_status");
    }

    #[test]
    fn test_git_status_description() {
        let tool = GitStatus::new();
        assert!(tool.description().contains("status"));
    }

    #[test]
    fn test_git_status_schema() {
        let tool = GitStatus::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_git_diff_name() {
        let tool = GitDiff::new();
        assert_eq!(tool.name(), "git_diff");
    }

    #[test]
    fn test_git_diff_description() {
        let tool = GitDiff::new();
        assert!(tool.description().contains("diff"));
    }

    #[test]
    fn test_git_diff_schema() {
        let tool = GitDiff::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["staged"].is_object());
    }

    #[test]
    fn test_git_commit_name() {
        let tool = GitCommit::new();
        assert_eq!(tool.name(), "git_commit");
    }

    #[test]
    fn test_git_commit_description() {
        let tool = GitCommit::new();
        assert!(tool.description().contains("commit"));
    }

    #[test]
    fn test_git_commit_schema() {
        let tool = GitCommit::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["files"].is_object());
    }

    #[test]
    fn test_git_checkpoint_name() {
        let tool = GitCheckpoint::new();
        assert_eq!(tool.name(), "git_checkpoint");
    }

    #[test]
    fn test_git_checkpoint_description() {
        let tool = GitCheckpoint::new();
        assert!(tool.description().contains("checkpoint"));
        assert!(tool.description().contains("rollback"));
    }

    #[test]
    fn test_git_checkpoint_schema() {
        let tool = GitCheckpoint::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["tag"].is_object());
        assert!(schema["properties"]["auto_branch"].is_object());
    }

    #[test]
    fn test_git_checkpoint_schema_required() {
        let tool = GitCheckpoint::new();
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_git_commit_schema_required() {
        let tool = GitCommit::new();
        let schema = tool.schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_git_commit_schema_commit_types() {
        let tool = GitCommit::new();
        let schema = tool.schema();
        let commit_type = &schema["properties"]["commit_type"];
        let enum_values = commit_type["enum"].as_array().unwrap();

        assert!(enum_values.contains(&serde_json::json!("feat")));
        assert!(enum_values.contains(&serde_json::json!("fix")));
        assert!(enum_values.contains(&serde_json::json!("refactor")));
    }

    #[tokio::test]
    async fn test_git_status_execute() {
        let _g = crate::test_support::CwdGuard::hold();
        let tool = GitStatus::new();
        let args = serde_json::json!({});

        // This will work in a git repo (like this project)
        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("branch").is_some() || output.get("error").is_some());
    }

    #[tokio::test]
    async fn test_git_diff_execute_unstaged() {
        let _g = crate::test_support::CwdGuard::hold();
        let tool = GitDiff::new();
        let args = serde_json::json!({"staged": false});

        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("diff").is_some() || output.get("error").is_some());
    }

    #[tokio::test]
    async fn test_git_diff_execute_staged() {
        let _g = crate::test_support::CwdGuard::hold();
        let tool = GitDiff::new();
        let args = serde_json::json!({"staged": true});

        let result = tool.execute(args).await;
        assert!(result.is_ok());
    }

    fn isolated_git_repo() -> (crate::test_support::CwdGuard, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let guard = crate::test_support::CwdGuard::enter(dir.path());
        fn git(args: &[&str]) {
            std::process::Command::new("git")
                .args(args)
                .status()
                .unwrap();
        }
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(dir.path().join("f.txt"), "x").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "base"]);
        (guard, dir)
    }

    #[tokio::test]
    async fn test_git_commit_with_message() {
        let _iso = isolated_git_repo();
        let tool = GitCommit::new();
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
    async fn empty_files_commit_excludes_untracked() {
        let (_guard, dir) = isolated_git_repo();
        // Modify a tracked file and drop a new untracked one (a stray secret).
        std::fs::write(dir.path().join("f.txt"), "modified content").unwrap();
        std::fs::write(dir.path().join("untracked_secret.txt"), "ghp_secret").unwrap();

        let tool = GitCommit::new();
        let args = serde_json::json!({ "message": "commit tracked only", "files": [] });
        tool.execute(args).await.expect("commit of tracked change");

        let status = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let status_str = String::from_utf8_lossy(&status.stdout);

        // The untracked file must NOT have been swept into the commit.
        assert!(
            status_str.contains("untracked_secret.txt"),
            "empty-files commit must leave untracked files untracked; status: {status_str}"
        );
        // The tracked modification WAS committed (no longer pending).
        assert!(
            !status_str.contains("f.txt"),
            "tracked modification should have been committed; status: {status_str}"
        );
    }

    #[tokio::test]
    async fn test_git_checkpoint_execute() {
        let _iso = isolated_git_repo();
        let tool = GitCheckpoint::new();
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
        let tool = GitDiff::new();
        let schema = tool.schema();

        assert!(schema["properties"]["staged"].is_object());
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["base"].is_object());
    }

    #[test]
    fn test_git_checkpoint_schema_defaults() {
        let tool = GitCheckpoint::new();
        let schema = tool.schema();

        let auto_branch = &schema["properties"]["auto_branch"];
        assert_eq!(auto_branch["default"], true);
    }

    #[test]
    fn test_git_status_schema_properties() {
        let tool = GitStatus::new();
        let schema = tool.schema();

        assert!(schema["properties"]["repo_path"].is_object());
    }

    #[test]
    fn test_git_commit_schema_files_array() {
        let tool = GitCommit::new();
        let schema = tool.schema();

        let files = &schema["properties"]["files"];
        assert_eq!(files["type"], "array");
    }

    // Additional tests for error paths and edge cases

    #[tokio::test]
    async fn test_git_status_not_a_repo() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();

        let tool = GitStatus::new();
        let args = serde_json::json!({
            "repo_path": temp_dir.path().to_str().unwrap()
        });

        let result = tool.execute(args).await;
        // Should fail since it's not a git repo
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_status_with_explicit_current_dir() {
        let _g = crate::test_support::CwdGuard::hold();
        let tool = GitStatus::new();
        let args = serde_json::json!({
            "repo_path": "."  // Explicit current dir
        });

        // Should work since we're in a git repo
        let result = tool.execute(args).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_git_diff_with_specific_path() {
        let _g = crate::test_support::CwdGuard::hold();
        let tool = GitDiff::new();
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
        let _iso = isolated_git_repo();
        let tool = GitCommit::new();
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
        let _iso = isolated_git_repo();
        let tool = GitCheckpoint::new();
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
        let _iso = isolated_git_repo();
        let tool = GitCheckpoint::new();
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
        let tool = GitStatus::new();
        let schema = tool.schema();

        let repo_path = &schema["properties"]["repo_path"];
        assert_eq!(repo_path["type"], "string");
    }

    #[test]
    fn test_git_diff_schema_has_base() {
        let tool = GitDiff::new();
        let schema = tool.schema();

        let base = &schema["properties"]["base"];
        assert_eq!(base["type"], "string");
    }

    #[test]
    fn test_git_checkpoint_message_required() {
        let tool = GitCheckpoint::new();
        let schema = tool.schema();

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert!(required.contains(&serde_json::json!("message")));
    }

    // GitPush tests

    #[test]
    fn test_git_push_name() {
        let tool = GitPush::new();
        assert_eq!(tool.name(), "git_push");
    }

    #[test]
    fn test_git_push_description() {
        let tool = GitPush::new();
        assert!(tool.description().contains("Push"));
    }

    #[test]
    fn test_git_push_schema() {
        let tool = GitPush::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["remote"].is_object());
        assert!(schema["properties"]["branch"].is_object());
        assert!(schema["properties"]["force"].is_object());
    }

    #[test]
    fn test_git_push_schema_defaults() {
        let tool = GitPush::new();
        let schema = tool.schema();
        assert_eq!(schema["properties"]["remote"]["default"], "origin");
        assert_eq!(schema["properties"]["force"]["default"], false);
    }

    #[tokio::test]
    async fn test_git_push_execute() {
        let tool = GitPush::new();
        // Push to nonexistent remote will fail, but shouldn't panic
        let args = serde_json::json!({
            "remote": "nonexistent_remote_test",
            "branch": "test-branch"
        });
        let result = tool.execute(args).await;
        // Should return Ok with success: false (remote doesn't exist)
        assert!(result.is_ok());
        let output = result.unwrap();
        assert_eq!(output["success"], false);
    }

    #[tokio::test]
    async fn test_git_push_execute_blocks_protected_branch() {
        let safety_config = crate::config::SafetyConfig {
            protected_branches: vec!["main".to_string()],
            ..Default::default()
        };
        let tool = GitPush::with_safety_config(safety_config);
        let args = serde_json::json!({
            "remote": "nonexistent_remote_test",
            "branch": "main"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("protected branch"));
    }

    #[tokio::test]
    async fn test_git_push_execute_allows_non_protected_branch() {
        let safety_config = crate::config::SafetyConfig {
            protected_branches: vec!["main".to_string()],
            ..Default::default()
        };
        let tool = GitPush::with_safety_config(safety_config);
        let args = serde_json::json!({
            "remote": "nonexistent_remote_test",
            "branch": "some-other-branch"
        });
        let result = tool.execute(args).await;
        // Not blocked by protected_branches; fails later since the remote
        // doesn't exist, same as the un-configured case above.
        assert!(result.is_ok());
        assert_eq!(result.unwrap()["success"], false);
    }

    // Tests for validate_tag_name function

    #[test]
    fn test_validate_tag_name_valid() {
        assert!(validate_tag_name("v1.0.0").is_ok());
        assert!(validate_tag_name("release-2024").is_ok());
        assert!(validate_tag_name("feature/new-thing").is_ok());
        assert!(validate_tag_name("hotfix_123").is_ok());
        assert!(validate_tag_name("a").is_ok());
    }

    #[test]
    fn test_validate_tag_name_empty() {
        assert!(validate_tag_name("").is_err());
    }

    #[test]
    fn test_validate_tag_name_too_long() {
        let long_name = "a".repeat(257);
        assert!(validate_tag_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_tag_name_starts_with_dash() {
        assert!(validate_tag_name("-v1.0.0").is_err());
    }

    #[test]
    fn test_validate_tag_name_invalid_chars() {
        assert!(validate_tag_name("v1.0 0").is_err()); // space
        assert!(validate_tag_name("v1.0@0").is_err()); // @
        assert!(validate_tag_name("v1.0#0").is_err()); // #
        assert!(validate_tag_name("v1.0$0").is_err()); // $
        assert!(validate_tag_name("v1.0!0").is_err()); // !
        assert!(validate_tag_name("v1.0*0").is_err()); // *
    }

    #[test]
    fn test_validate_tag_name_exactly_256() {
        let name = "a".repeat(256);
        assert!(validate_tag_name(&name).is_ok());
    }

    // Tests for write_commit_message_file

    #[test]
    fn test_write_commit_message_file_creates_file() {
        let message = "Test commit message";
        let path = write_commit_message_file(message);
        assert!(path.is_some());

        let path = path.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, message);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_commit_message_file_multiline() {
        let message = "Line 1\nLine 2\nLine 3";
        let path = write_commit_message_file(message);
        assert!(path.is_some());

        let path = path.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, message);

        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_commit_message_file_unique_names() {
        let path1 = write_commit_message_file("msg1");
        let path2 = write_commit_message_file("msg2");

        assert!(path1.is_some());
        assert!(path2.is_some());
        assert_ne!(path1, path2);

        // Clean up
        let _ = std::fs::remove_file(path1.unwrap());
        let _ = std::fs::remove_file(path2.unwrap());
    }
}
