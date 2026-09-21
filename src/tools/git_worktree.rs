//! Git Worktree Isolation Tools
//!
//! Provides tools for creating and managing isolated git worktrees, allowing
//! the agent to work in a separate directory without affecting the main working directory.
//!
//! # Example Workflow
//!
//! ```text
//! // Enter a new worktree for isolated development
//! EnterWorktreeTool::execute({"path": "feature-branch", "branch": "main"})
//!
//! // ... do work in isolation ...
//!
//! // Exit and optionally remove the worktree
//! ExitWorktreeTool::execute({"path": "feature-branch", "remove": true})
//! ```

use super::Tool;
use crate::config::SafetyConfig;
use crate::tools::file::{resolve_safety_config, validate_tool_path};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Global state to track the current worktree context
/// This is used to remember the original directory when entering a worktree
use std::sync::Mutex;

static WORKTREE_STATE: Mutex<WorktreeState> = Mutex::new(WorktreeState::new());

/// RAII guard that restores the process current directory to the directory
/// that was current before [`CwdRestoreGuard::enter`] changed it.
///
/// The guard is stored in the worktree stack level it was created for. The
/// normal exit path restores the cwd explicitly and propagates a failure (see
/// [`WorktreeState::pop_worktree`]); this guard additionally re-applies the
/// same restore when the level is dropped, covering panics and any early
/// return between the explicit restore and the end of the exit path. The
/// process cwd can therefore never be silently stranded inside a worktree
/// while [`WorktreeState`] claims it is not there (or claimed "restored" while
/// the cwd still points into the worktree).
struct CwdRestoreGuard {
    previous: PathBuf,
}

impl CwdRestoreGuard {
    /// Record the current cwd as the restore target without changing it.
    fn record() -> Result<Self> {
        let previous = env::current_dir().context("Failed to get current directory")?;
        Ok(Self { previous })
    }

    /// Change the process cwd into `target`, remembering where we came from.
    /// On failure the cwd is left untouched and no guard is created.
    fn enter(target: &Path) -> Result<Self> {
        let guard = Self::record()?;
        env::set_current_dir(target).with_context(|| {
            format!(
                "Failed to change to worktree directory: {}",
                target.display()
            )
        })?;
        Ok(guard)
    }

    /// The directory this guard will restore the cwd to.
    fn previous(&self) -> &Path {
        &self.previous
    }
}

impl std::fmt::Debug for CwdRestoreGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CwdRestoreGuard")
            .field("previous", &self.previous)
            .finish()
    }
}

impl Drop for CwdRestoreGuard {
    fn drop(&mut self) {
        // Restoring the process cwd can fail at the OS level; surface it
        // loudly instead of silently leaving every later relative path
        // resolved against the wrong directory.
        if let Err(e) = env::set_current_dir(&self.previous) {
            warn!(
                "Failed to restore working directory to {}: {}",
                self.previous.display(),
                e
            );
        }
    }
}

/// One level of worktree nesting. The level's guard restores the process cwd
/// to the directory that was current before this level was entered. The exit
/// path reads `cwd.previous()` to restore explicitly (propagating failure);
/// the guard's `Drop` then re-applies the same restore as a panic/early-return
/// fallback.
#[derive(Debug)]
struct WorktreeLevel {
    path: PathBuf,
    cwd: CwdRestoreGuard,
}

#[derive(Debug)]
struct WorktreeState {
    /// Stack of directory levels representing worktree entry history
    /// The first element is always the original repo root
    directory_stack: Vec<WorktreeLevel>,
    /// Currently active worktree path (if any)
    current_worktree: Option<PathBuf>,
}

impl WorktreeState {
    const fn new() -> Self {
        Self {
            directory_stack: Vec::new(),
            current_worktree: None,
        }
    }

    fn initialize(&mut self) -> Result<()> {
        if self.directory_stack.is_empty() {
            let current = env::current_dir().context("Failed to get current directory")?;
            let cwd = CwdRestoreGuard::record()?;
            self.directory_stack
                .push(WorktreeLevel { path: current, cwd });
        }
        Ok(())
    }

    fn push_worktree(&mut self, worktree_path: PathBuf) -> Result<PathBuf> {
        self.initialize()?;
        // Change the process cwd FIRST. `CwdRestoreGuard::enter` remembers the
        // directory we leave and only mutates the cwd on success; if it fails,
        // neither the cwd nor the stack was changed, so an early return here
        // cannot leave the process stranded in (or out of) a worktree that is
        // inconsistent with this state.
        let cwd = CwdRestoreGuard::enter(&worktree_path)?;
        self.directory_stack.push(WorktreeLevel {
            path: worktree_path.clone(),
            cwd,
        });
        self.current_worktree = Some(worktree_path.clone());
        Ok(worktree_path)
    }

    fn pop_worktree(&mut self, remove: bool) -> Result<(PathBuf, Option<PathBuf>)> {
        self.initialize()?;

        // The top level is the one being exited. Callers guard with
        // `is_in_worktree()`; the check here is defensive.
        let level = self
            .directory_stack
            .last()
            .ok_or_else(|| anyhow::anyhow!("Not currently in a worktree"))?;
        let restore_to = level.cwd.previous().to_path_buf();

        // Restore the process cwd EXPLICITLY and BEFORE committing the state
        // change. On failure the stack still says we are inside the worktree —
        // consistent with the cwd — and the error reaches the caller, so an
        // exit can never report success while every later relative path
        // silently resolves against the wrong directory.
        env::set_current_dir(&restore_to).with_context(|| {
            format!(
                "Failed to restore working directory to {}",
                restore_to.display()
            )
        })?;

        // Commit the state change now that the restore succeeded.
        let popped = self
            .directory_stack
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Not currently in a worktree"))?;
        self.current_worktree = self.directory_stack.last().map(|l| l.path.clone());

        // If we're removing the worktree, capture its path before we forget it
        let removed_path = if remove { Some(popped.path) } else { None };

        Ok((restore_to, removed_path))
    }

    #[allow(dead_code)]
    fn current(&self) -> Option<&PathBuf> {
        self.directory_stack.last().map(|l| &l.path)
    }

    #[allow(dead_code)]
    fn root(&self) -> Option<&PathBuf> {
        self.directory_stack.first().map(|l| &l.path)
    }

    fn is_in_worktree(&self) -> bool {
        self.directory_stack.len() > 1
    }
}

/// Default worktree base directory within .selfware/
const DEFAULT_WORKTREE_BASE: &str = ".selfware/worktrees";

/// Generate a timestamp-based worktree name
fn generate_worktree_name() -> String {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    format!("worktree_{}", timestamp)
}

/// Find the git repository root
async fn find_git_root() -> Result<PathBuf> {
    let mut cmd = tokio::process::Command::new("git");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    let output = cmd
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .await
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Not a git repository: {}", stderr);
    }

    let root = String::from_utf8_lossy(&output.stdout);
    Ok(PathBuf::from(root.trim()))
}

/// Validate a branch name to prevent shell injection
fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("Branch name must not be empty");
    }
    if name.len() > 255 {
        anyhow::bail!("Branch name too long (max 255 characters)");
    }

    // Check for dangerous characters that could cause shell injection
    for c in name.chars() {
        if c.is_control() || matches!(c, ';' | '&' | '|' | '$' | '`' | '<' | '>') {
            anyhow::bail!("Invalid character '{}' in branch name", c);
        }
    }

    // Branch name cannot start with '-' (could be interpreted as a flag)
    if name.starts_with('-') {
        anyhow::bail!("Branch name must not start with '-'");
    }

    Ok(())
}

/// Validate a path for security: the `enter_worktree` path becomes a NEW
/// directory (`git worktree add` creates it) and the process cwd moves
/// into it, so the path obeys the same workspace path policy as every
/// other tool path — workspace containment or allowed list, `..` escapes,
/// symlink escapes, protected system paths, null bytes, and denied
/// patterns (2026-09-21 review: this was a stub that ignored the safety
/// config and only rejected literal null bytes, so a worktree could be
/// created and entered outside the workspace).
fn validate_path(path: &str, safety_config: Option<&SafetyConfig>) -> Result<()> {
    let safety = resolve_safety_config(safety_config);
    validate_tool_path(path, &safety)
}

/// Output from git worktree list --porcelain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
}

#[derive(Default)]
pub struct EnterWorktreeTool {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct ExitWorktreeTool {
    pub safety_config: Option<SafetyConfig>,
}

#[derive(Default)]
pub struct ListWorktreesTool {
    pub safety_config: Option<SafetyConfig>,
}

impl EnterWorktreeTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl ExitWorktreeTool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_safety_config(config: SafetyConfig) -> Self {
        Self {
            safety_config: Some(config),
        }
    }
}

impl ListWorktreesTool {
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
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "enter_worktree"
    }

    fn description(&self) -> &str {
        "Create and enter a git worktree for isolated development. Changes working directory to the new worktree. \
         If no path is provided, creates worktree at .selfware/worktrees/{timestamp}/. \
         If no branch is provided, creates a detached worktree."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path for the new worktree (default: .selfware/worktrees/{timestamp}/)"
                },
                "branch": {
                    "type": "string",
                    "description": "Branch to checkout (default: detached HEAD)"
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path_arg = args.get("path").and_then(|v| v.as_str());
        let branch_arg = args.get("branch").and_then(|v| v.as_str());

        // Validate inputs
        if let Some(p) = path_arg {
            validate_path(p, self.safety_config.as_ref())?;
        }
        if let Some(b) = branch_arg {
            validate_branch_name(b)?;
        }

        // Find git root
        let git_root = find_git_root().await?;
        let original_dir = env::current_dir().context("Failed to get current directory")?;

        // Resolve worktree path
        let worktree_path = if let Some(p) = path_arg {
            PathBuf::from(p)
        } else {
            let name = generate_worktree_name();
            git_root.join(DEFAULT_WORKTREE_BASE).join(name)
        };

        // Ensure parent directory exists
        if let Some(parent) = worktree_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        info!("Creating worktree at: {}", worktree_path.display());

        // Build git worktree add command
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        cmd.arg("worktree").arg("add");

        if branch_arg.is_none() {
            // Create detached worktree
            cmd.arg("--detach");
        }

        cmd.arg(&worktree_path);

        if let Some(branch) = branch_arg {
            cmd.arg(branch);
        }

        let output = cmd
            .current_dir(&git_root)
            .output()
            .await
            .context("Failed to execute git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create worktree: {}", stderr);
        }

        // Change to the worktree directory
        let worktree_path_str = worktree_path.to_string_lossy().to_string();
        let branch_used = branch_arg.unwrap_or("(detached)").to_string();

        // Update the global state and change directory
        let mut state = WORKTREE_STATE
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        state.push_worktree(worktree_path.clone())?;

        info!(
            "Entered worktree: {} (branch: {})",
            worktree_path.display(),
            branch_used
        );

        Ok(serde_json::json!({
            "success": true,
            "worktree_path": worktree_path_str,
            "branch": branch_used,
            "previous_path": original_dir.to_string_lossy().to_string(),
            "git_root": git_root.to_string_lossy().to_string()
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        // Medium risk - creates directories and changes working directory
        crate::safety::ToolMetadata::custom(
            false,
            false,
            crate::safety::RiskLevel::Medium,
            false,
            false,
        )
    }
}

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "exit_worktree"
    }

    fn description(&self) -> &str {
        "Exit the current git worktree and return to the main repository. \
         Optionally remove the worktree directory."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path of worktree to exit (default: current worktree)"
                },
                "remove": {
                    "type": "boolean",
                    "description": "Remove the worktree after exiting",
                    "default": false
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let _path_arg = args.get("path").and_then(|v| v.as_str());
        let remove = args
            .get("remove")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // `restored_path` is the directory the process cwd was actually
        // restored to (see `pop_worktree`); for a single-level entry that is
        // the repository root.
        let (restored_path, removed_path) = {
            let mut state = WORKTREE_STATE
                .lock()
                .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;

            if !state.is_in_worktree() {
                anyhow::bail!("Not currently in a worktree");
            }

            state.pop_worktree(remove)?
        };

        // If remove is requested, run git worktree remove
        let mut removed = false;
        if remove {
            if let Some(ref worktree_path) = removed_path {
                let mut cmd = tokio::process::Command::new("git");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                let output = cmd
                    .args(["worktree", "remove", &worktree_path.to_string_lossy()])
                    .output()
                    .await
                    .context("Failed to execute git worktree remove")?;

                if output.status.success() {
                    removed = true;
                    info!("Removed worktree: {}", worktree_path.display());
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    warn!("Failed to remove worktree: {}", stderr);
                    // Don't fail - we've already changed directories back
                }
            }
        }

        info!("Exited worktree, returned to: {}", restored_path.display());

        Ok(serde_json::json!({
            "success": true,
            "previous_path": removed_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            "current_path": restored_path.to_string_lossy().to_string(),
            "removed": removed
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        // Medium risk - can remove directories
        crate::safety::ToolMetadata::custom(
            false,
            true, // Destructive - can remove worktrees
            crate::safety::RiskLevel::Medium,
            false,
            false,
        )
    }
}

#[async_trait]
impl Tool for ListWorktreesTool {
    fn name(&self) -> &str {
        "list_worktrees"
    }

    fn description(&self) -> &str {
        "List all git worktrees with their paths and branches."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let output = cmd
            .args(["worktree", "list", "--porcelain"])
            .output()
            .await
            .context("Failed to execute git worktree list")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to list worktrees: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let worktrees = parse_worktree_list(&stdout);

        // Check if we're currently in a worktree
        let state = WORKTREE_STATE
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let current_worktree = state
            .current_worktree
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());

        Ok(serde_json::json!({
            "worktrees": worktrees,
            "count": worktrees.len(),
            "current_worktree": current_worktree
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        crate::safety::ToolMetadata::read_only()
    }
}

/// Parse git worktree list --porcelain output
fn parse_worktree_list(output: &str) -> Vec<WorktreeEntry> {
    let mut worktrees = Vec::new();
    let mut current = WorktreeEntry {
        path: String::new(),
        branch: None,
        detached: false,
        bare: false,
    };

    for line in output.lines() {
        if line.is_empty() {
            // End of worktree entry
            if !current.path.is_empty() {
                worktrees.push(current);
                current = WorktreeEntry {
                    path: String::new(),
                    branch: None,
                    detached: false,
                    bare: false,
                };
            }
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            current.path = path.to_string();
        } else if let Some(branch) = line.strip_prefix("branch ") {
            // Extract branch name from ref (refs/heads/branch-name)
            current.branch = branch.split('/').next_back().map(|s| s.to_string());
        } else if line == "detached" {
            current.detached = true;
        } else if line == "bare" {
            current.bare = true;
        }
        // Ignore other fields (HEAD, locked, prunable)
    }

    // Don't forget the last entry
    if !current.path.is_empty() {
        worktrees.push(current);
    }

    worktrees
}

/// Get the current worktree path if we're in one
pub fn get_current_worktree() -> Option<PathBuf> {
    WORKTREE_STATE
        .lock()
        .ok()
        .and_then(|state| state.current_worktree.clone())
}

/// Check if currently in a worktree
pub fn is_in_worktree() -> bool {
    WORKTREE_STATE
        .lock()
        .map(|state| state.is_in_worktree())
        .unwrap_or(false)
}

/// Enter `worktree_path`: remember the directory that is currently current and
/// change the process cwd into the worktree. Shared by the `enter_worktree`
/// tool and the TUI `/worktree enter` handler so both observe one consistent
/// worktree state — the cwd is restored again by [`exit_worktree_dir`] on
/// every exit path (success, error, early return). Returns the directory now
/// in effect.
pub fn enter_worktree_dir(worktree_path: PathBuf) -> Result<PathBuf> {
    let mut state = WORKTREE_STATE
        .lock()
        .map_err(|e| anyhow::anyhow!("Worktree state lock poisoned: {}", e))?;
    state.push_worktree(worktree_path)
}

/// Leave the current worktree, restoring the process cwd to the directory that
/// was current before this worktree was entered — on success, on error inside
/// the exit path, and on early return alike. Returns the restored directory
/// and the worktree just left (useful for a follow-up `git worktree remove`).
pub fn exit_worktree_dir() -> Result<(PathBuf, Option<PathBuf>)> {
    let mut state = WORKTREE_STATE
        .lock()
        .map_err(|e| anyhow::anyhow!("Worktree state lock poisoned: {}", e))?;
    if !state.is_in_worktree() {
        anyhow::bail!("Not currently in a worktree");
    }
    // Capture the worktree we are about to leave before popping it; the pop
    // itself restores the process cwd via the level's guard.
    let previous_worktree = state.current_worktree.clone();
    let (restored, _removed) = state.pop_worktree(false)?;
    Ok((restored, previous_worktree))
}

#[cfg(test)]
#[path = "../../tests/unit/tools/git_worktree/git_worktree_test.rs"]
mod tests;
