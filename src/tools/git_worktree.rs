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
//!
//! # Workspace root, not process cwd
//!
//! Entering a worktree moves the calling agent's
//! [`WorkspaceRoot`] — the root
//! its tool calls validate paths against, resolve relative paths against and
//! start subprocesses in. It does NOT call `std::env::set_current_dir`: the
//! process cwd is one global shared by every Tokio worker, every concurrently
//! running agent, background job, LSP server and subprocess, so the old
//! cwd switch silently moved the workspace of everything else in the process.
//! The worktree stack lives on the root handle itself, so two agents in one
//! process have independent worktree state.

use super::Tool;
use crate::config::SafetyConfig;
use crate::tools::file::{resolve_safety_config, validate_tool_path};
use crate::tools::workspace_root::{self, CommandRootExt, WorkspaceRoot};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Default worktree base directory within .selfware/
const DEFAULT_WORKTREE_BASE: &str = ".selfware/worktrees";

/// Generate a timestamp-based worktree name
fn generate_worktree_name() -> String {
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    format!("worktree_{}", timestamp)
}

/// Find the git repository root containing the workspace root `root`.
async fn find_git_root(root: &WorkspaceRoot) -> Result<PathBuf> {
    let mut cmd = tokio::process::Command::new("git");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    let output = cmd
        .in_root(root)
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

/// Upper bound on each `git worktree list/prune` housekeeping call. Pruning
/// is metadata-only and never worth stalling a tool call for.
const PRUNE_GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Ledger (inside the git common dir) of worktree paths selfware created
/// outside the default base, so their stale records can be recognised as
/// selfware's after the directory is gone.
const CREATED_LEDGER: &str = "selfware-created-worktrees";

/// Run a housekeeping git command in `dir` with a sanitized environment and a
/// bounded timeout (the child is killed if the timeout fires).
async fn run_git_bounded(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut cmd = tokio::process::Command::new("git");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.current_dir(dir).args(args).kill_on_drop(true);
    match tokio::time::timeout(PRUNE_GIT_TIMEOUT, cmd.output()).await {
        Ok(out) => out.with_context(|| format!("Failed to execute git {}", args.join(" "))),
        Err(_) => anyhow::bail!(
            "git {} timed out after {}s",
            args.join(" "),
            PRUNE_GIT_TIMEOUT.as_secs()
        ),
    }
}

/// The repository's common git dir (shared by all worktrees), absolute.
async fn git_common_dir(dir: &Path) -> Result<PathBuf> {
    let out = run_git_bounded(dir, &["rev-parse", "--git-common-dir"]).await?;
    if !out.status.success() {
        anyhow::bail!(
            "Not a git repository: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let raw = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    Ok(if raw.is_absolute() {
        raw
    } else {
        dir.join(raw)
    })
}

/// Remember that selfware created the worktree at `path`.
async fn record_created_worktree(repo_dir: &Path, path: &Path) {
    let Ok(common) = git_common_dir(repo_dir).await else {
        return;
    };
    let ledger = common.join(CREATED_LEDGER);
    // git lists worktrees by their real path (e.g. /private/var on macOS),
    // so record the canonical form while the directory still exists.
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let line = format!("{}\n", path.to_string_lossy());
    let appended = async {
        use tokio::io::AsyncWriteExt;
        let mut f = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&ledger)
            .await?;
        f.write_all(line.as_bytes()).await
    }
    .await;
    if let Err(e) = appended {
        warn!(
            "Could not record created worktree in {}: {}",
            ledger.display(),
            e
        );
    }
}

/// Whether selfware created the worktree recorded at `path`: it lives under
/// the default `.selfware/worktrees` base, or it is in the created ledger.
fn is_selfware_worktree(path: &Path, ledger: &[PathBuf]) -> bool {
    let components: Vec<_> = path.components().map(|c| c.as_os_str()).collect();
    let under_default_base = components
        .windows(2)
        .any(|w| w[0] == ".selfware" && w[1] == "worktrees");
    under_default_base || ledger.iter().any(|p| p == path)
}

/// Result of a stale-worktree prune attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneOutcome {
    /// No selfware-created worktree record points at a missing directory.
    NothingStale,
    /// `git worktree prune` ran; these selfware records were stale.
    Pruned(Vec<PathBuf>),
    /// Stale selfware records exist, but so do stale records selfware did
    /// NOT create. `git worktree prune` cannot be scoped to a subset, so it
    /// was not run: the user's own records are never touched.
    SkippedForeign { selfware: usize, foreign: usize },
}

/// Prune stale worktree records that selfware created.
///
/// After an unexpected termination the directory of a selfware worktree can
/// be gone while git still lists it. This runs `git worktree prune` — which
/// only drops ADMIN RECORDS whose directory no longer exists and never removes
/// a directory — but only when every prunable record belongs to selfware.
pub async fn prune_stale_selfware_worktrees(repo_dir: &Path) -> Result<PruneOutcome> {
    let out = run_git_bounded(repo_dir, &["worktree", "list", "--porcelain"]).await?;
    if !out.status.success() {
        anyhow::bail!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let entries = parse_worktree_list(&String::from_utf8_lossy(&out.stdout));
    let ledger: Vec<PathBuf> = match git_common_dir(repo_dir).await {
        Ok(common) => tokio::fs::read_to_string(common.join(CREATED_LEDGER))
            .await
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(PathBuf::from)
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut ours = Vec::new();
    let mut foreign = 0usize;
    for entry in entries.iter().filter(|e| e.prunable) {
        let path = PathBuf::from(&entry.path);
        // Belt and braces: git said prunable; only a record whose directory
        // is really gone counts.
        if path.exists() {
            continue;
        }
        if is_selfware_worktree(&path, &ledger) {
            ours.push(path);
        } else {
            foreign += 1;
        }
    }
    if ours.is_empty() {
        return Ok(PruneOutcome::NothingStale);
    }
    if foreign > 0 {
        warn!(
            "Not pruning {} stale selfware worktree record(s): {} stale record(s) not \
             created by selfware would also be pruned (run `git worktree prune` yourself)",
            ours.len(),
            foreign
        );
        return Ok(PruneOutcome::SkippedForeign {
            selfware: ours.len(),
            foreign,
        });
    }
    let out = run_git_bounded(repo_dir, &["worktree", "prune"]).await?;
    if !out.status.success() {
        anyhow::bail!(
            "git worktree prune failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    info!("Pruned {} stale selfware worktree record(s)", ours.len());
    Ok(PruneOutcome::Pruned(ours))
}

/// Best-effort prune for the enter/exit paths: failures are logged, never
/// fatal. Returns the pruned paths for the tool's JSON output.
async fn prune_best_effort(repo_dir: &Path) -> Vec<String> {
    match prune_stale_selfware_worktrees(repo_dir).await {
        Ok(PruneOutcome::Pruned(paths)) => paths
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        Ok(_) => Vec::new(),
        Err(e) => {
            warn!("Stale worktree prune skipped: {}", e);
            Vec::new()
        }
    }
}

/// Repositories already pruned at tool startup in this process.
static STARTUP_PRUNED: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(Default::default);

/// Tool-startup hook: when the worktree tools are registered for a workspace
/// that has a `.selfware/worktrees` base (selfware has created worktrees
/// here), prune stale selfware records once per process, in the background.
/// A no-op without a Tokio runtime, outside such a workspace, and in unit
/// tests (which must not touch the developer's repository).
pub fn startup_prune(workspace: &Path) {
    if cfg!(test) || !workspace.join(DEFAULT_WORKTREE_BASE).is_dir() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    {
        let mut seen = STARTUP_PRUNED.lock().unwrap_or_else(|e| e.into_inner());
        if !seen.insert(workspace.to_path_buf()) {
            return;
        }
    }
    let dir = workspace.to_path_buf();
    handle.spawn(async move {
        let _ = prune_best_effort(&dir).await;
    });
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
/// directory (`git worktree add` creates it) and the agent's workspace root
/// moves into it, so the path obeys the same workspace path policy as every
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
    /// git reports this record as prunable (its directory is gone).
    #[serde(default)]
    pub prunable: bool,
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
        "Create and enter a git worktree for isolated development. Moves this agent's workspace root to the new worktree \
         (relative paths, path checks and commands then resolve there). \
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

        // The calling agent's workspace root (task-local installed by the
        // registry / agent dispatch).
        let root = workspace_root::current();

        // Find git root
        let git_root = find_git_root(&root).await?;
        let original_dir = root.path();

        // Drop stale records of selfware worktrees whose directories vanished
        // (e.g. after an unexpected termination) before adding a new one.
        let pruned = prune_best_effort(&git_root).await;

        // Resolve worktree path. A relative path is resolved against the
        // workspace root — the same directory it was validated against —
        // so `git worktree add` (run in git_root) and the enter below agree
        // on one absolute directory.
        let worktree_path = if let Some(p) = path_arg {
            root.resolve(std::path::Path::new(p))
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

        record_created_worktree(&git_root, &worktree_path).await;

        let worktree_path_str = worktree_path.to_string_lossy().to_string();
        let branch_used = branch_arg.unwrap_or("(detached)").to_string();

        // Move this agent's workspace root into the worktree. The process
        // cwd is untouched, so concurrent agents/tasks are unaffected.
        root.enter(&worktree_path)?;

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
            "git_root": git_root.to_string_lossy().to_string(),
            "pruned_stale_worktrees": pruned
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

        // `restored_path` is the directory the workspace root was restored to
        // (the level below the one exited); for a single-level entry that is
        // the base workspace root. The process cwd is never touched.
        let (restored_path, left_path) = workspace_root::current().exit()?;
        let removed_path = if remove { Some(left_path) } else { None };

        // If remove is requested, run git worktree remove
        let mut removed = false;
        if remove {
            if let Some(ref worktree_path) = removed_path {
                let mut cmd = tokio::process::Command::new("git");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                let output = cmd
                    .current_dir(&restored_path)
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
                    // Don't fail - the workspace root is already restored
                }
            }
        }

        info!("Exited worktree, returned to: {}", restored_path.display());

        let pruned = prune_best_effort(&restored_path).await;

        Ok(serde_json::json!({
            "success": true,
            "previous_path": removed_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            "current_path": restored_path.to_string_lossy().to_string(),
            "removed": removed,
            "pruned_stale_worktrees": pruned
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
        let root = workspace_root::current();
        let mut cmd = tokio::process::Command::new("git");
        crate::safety::process_env::sanitize_command_env(&mut cmd);
        let output = cmd
            .in_root(&root)
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

        // Check if this agent's workspace root is currently in a worktree
        let current_worktree = root
            .current_worktree()
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
        prunable: false,
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
                    prunable: false,
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
        } else if line == "prunable" || line.starts_with("prunable ") {
            current.prunable = true;
        }
        // Ignore other fields (HEAD, locked)
    }

    // Don't forget the last entry
    if !current.path.is_empty() {
        worktrees.push(current);
    }

    worktrees
}

/// Get the current worktree path of the calling task's workspace root.
pub fn get_current_worktree() -> Option<PathBuf> {
    workspace_root::current().current_worktree()
}

/// Check if the calling task's workspace root is inside a worktree.
pub fn is_in_worktree() -> bool {
    workspace_root::current().is_in_worktree()
}

/// Enter `worktree_path` on `root`: push a worktree level so every tool call
/// dispatched with `root` resolves against the worktree. Shared by the
/// `enter_worktree` tool and the TUI `/worktree enter` handler so both observe
/// one consistent worktree state. The process cwd is never changed. Returns
/// the directory now in effect.
///
/// The path is validated against the workspace path policy FIRST, with the
/// process-global safety config (`validate_path`) resolved against `root`.
/// The `enter_worktree` tool validates with its own per-instance config on
/// the execute path; this entry point is what the TUI `/worktree enter`
/// handler goes through, and without the same check it could enter an
/// out-of-workspace target (2026-09-21 review: the tool path was hardened,
/// the TUI path was not). An in-workspace target passes unchanged.
pub fn enter_worktree_dir(root: &WorkspaceRoot, worktree_path: PathBuf) -> Result<PathBuf> {
    let path_str = worktree_path.to_string_lossy().into_owned();
    workspace_root::sync_scope(root.clone(), || validate_path(&path_str, None)).with_context(
        || {
            format!(
                "Refused to enter worktree outside the workspace: {} (the \
                 worktree was created, but the workspace root was not changed)",
                path_str
            )
        },
    )?;
    root.enter(&worktree_path)
}

/// Leave the current worktree on `root`, restoring the directory that was in
/// effect before it was entered. Returns the restored directory and the
/// worktree just left (useful for a follow-up `git worktree remove`).
pub fn exit_worktree_dir(root: &WorkspaceRoot) -> Result<(PathBuf, Option<PathBuf>)> {
    let (restored, left) = root.exit()?;
    Ok((restored, Some(left)))
}

#[cfg(test)]
#[path = "../../tests/unit/tools/git_worktree/git_worktree_test.rs"]
mod tests;
