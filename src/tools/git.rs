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
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let branch_output = cmd
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

                let mut cmd = tokio::process::Command::new("git");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                cmd.args(["checkout", "-b", &agent_branch]).output().await?;

                info!("Created agent branch: {}", agent_branch);
                agent_branch
            } else {
                current_branch
            };

        // Stage all changes
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        cmd.args(["add", "-A"])
            .output()
            .await
            .context("Failed to stage changes")?;

        // Commit with checkpoint marker
        let full_msg = format!("[AGENT CHECKPOINT] {}", msg);
        let msg_file = write_commit_message_file(&full_msg);
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let commit_output = if let Some(ref path) = msg_file {
            cmd.arg("commit")
                .arg("--file")
                .arg(path)
                .arg("--allow-empty")
                .output()
                .await
                .context("Failed to create checkpoint commit")?
        } else {
            cmd.args(["commit", "-m", &full_msg, "--allow-empty"])
                .output()
                .await
                .context("Failed to create checkpoint commit")?
        };
        if let Some(path) = msg_file {
            let _ = std::fs::remove_file(path);
        }

        // Get hash
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let hash_output = cmd.args(["rev-parse", "HEAD"]).output().await?;
        let hash = String::from_utf8_lossy(&hash_output.stdout)
            .trim()
            .to_string();

        // Create or move tag
        if let Some(tag_name) = tag {
            validate_tag_name(tag_name)?;
            let mut cmd = tokio::process::Command::new("git");
            crate::safety::process_env::sanitize_command_env(&mut cmd);
            cmd.args(["tag", "-f", tag_name, &hash]).output().await?;
        }

        // Get status summary
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let status_output = cmd.args(["status", "--short"]).output().await?;
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
        let base = args.get("base").and_then(|v| v.as_str());

        validate_git_path(repo_path, self.safety_config.as_ref())?;

        // `base` is passed as a single argv entry; reject leading dashes so it
        // can't smuggle in a flag (e.g. `--output=/path`).
        if let Some(base) = base {
            if base.starts_with('-') {
                anyhow::bail!("Invalid base for git diff: {}", base);
            }
        }

        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        cmd.arg("-C").arg(repo_path).arg("diff");
        if staged {
            cmd.arg("--cached");
        }
        if let Some(base) = base {
            cmd.arg(base);
        }

        let output = cmd.output().await?;
        // A non-zero exit (bad revision, not a repo, ...) used to be reported
        // as `has_changes: false` with the error text silently dropped.
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git diff failed: {}", stderr.trim());
        }
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
            let mut cmd = tokio::process::Command::new("git");
            crate::safety::process_env::sanitize_command_env(&mut cmd);
            let add_output = cmd
                .arg("-C")
                .arg(repo_path)
                .arg("add")
                .arg("-u")
                .output()
                .await?;
            // A failed add must not silently become a partial commit below.
            if !add_output.status.success() {
                let stderr = String::from_utf8_lossy(&add_output.stderr);
                anyhow::bail!("git add -u failed: {}", stderr.trim());
            }
        } else {
            for file in files {
                if let Some(f) = file.as_str() {
                    if f.contains("..") || f.starts_with('/') {
                        anyhow::bail!("Invalid file path for git commit: {}", f);
                    }
                    let mut cmd = tokio::process::Command::new("git");
                    crate::safety::process_env::sanitize_command_env(&mut cmd);
                    let add_output = cmd
                        .arg("-C")
                        .arg(repo_path)
                        .arg("add")
                        .arg("--")
                        .arg(f)
                        .output()
                        .await?;
                    // A typo'd/unmatched path makes `git add` exit non-zero;
                    // ignoring it would produce a partial commit reported as
                    // success. Surface the real error instead.
                    if !add_output.status.success() {
                        let stderr = String::from_utf8_lossy(&add_output.stderr);
                        anyhow::bail!("git add failed for '{}': {}", f, stderr.trim());
                    }
                }
            }
        }

        // Commit — write message to temp file for defense-in-depth against
        // shell metacharacters, falling back to -m if the write fails.
        let msg_file = write_commit_message_file(message);
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let output = if let Some(ref path) = msg_file {
            cmd.arg("-C")
                .arg(repo_path)
                .arg("commit")
                .arg("--file")
                .arg(path)
                .output()
                .await?
        } else {
            cmd.arg("-C")
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        // On failure git explains itself on stderr (hooks, identity, empty
        // staging area) — surface it instead of reporting a bare failure.
        let combined = if success {
            stdout.to_string()
        } else {
            format!("{}{}", stdout, stderr)
        };

        Ok(serde_json::json!({
            "success": success,
            "output": combined
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
            let mut cmd = tokio::process::Command::new("git");
            crate::safety::process_env::sanitize_command_env(&mut cmd);
            let output = cmd
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
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        // Push goes over the network and may need the user's SSH agent;
        // re-add it explicitly after the env clear.
        if let Ok(v) = std::env::var("SSH_AUTH_SOCK") {
            cmd.env("SSH_AUTH_SOCK", v);
        }
        cmd.arg("push").arg("--").arg(remote).arg(&branch);
        // Kill the child if the timeout below drops the output future —
        // a "timed-out" push must not keep running and still land on the
        // remote after we've reported the timeout.
        cmd.kill_on_drop(true);

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120);
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), cmd.output())
                .await
                .context("git push timed out (network hang?) — the push process was killed")?
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
#[path = "../../tests/unit/tools/git/git_test.rs"]
mod tests;
