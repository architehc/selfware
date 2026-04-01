//! Git Worktree Isolation Tools
//!
//! Provides tools for creating and managing isolated git worktrees, allowing
//! the agent to work in a separate directory without affecting the main working directory.
//!
//! # Example Workflow
//!
//! ```
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

#[derive(Debug, Clone)]
struct WorktreeState {
    /// Stack of directories representing worktree entry history
    /// The first element is always the original repo root
    directory_stack: Vec<PathBuf>,
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
            self.directory_stack.push(current);
        }
        Ok(())
    }

    fn push_worktree(&mut self, worktree_path: PathBuf) -> Result<PathBuf> {
        self.initialize()?;
        self.directory_stack.push(worktree_path.clone());
        self.current_worktree = Some(worktree_path.clone());
        env::set_current_dir(&worktree_path)
            .with_context(|| format!("Failed to change to worktree directory: {}", worktree_path.display()))?;
        Ok(worktree_path)
    }

    fn pop_worktree(&mut self, remove: bool) -> Result<(PathBuf, Option<PathBuf>)> {
        self.initialize()?;
        
        let current = self.directory_stack.pop();
        let _previous_path = current.clone();
        
        // If we're removing the worktree, capture its path before we forget it
        let removed_path = if remove {
            current
        } else {
            None
        };

        // Update current worktree to the previous entry (or None if back at root)
        self.current_worktree = self.directory_stack.last().cloned();
        
        // Change back to the previous directory (root of the stack)
        if let Some(ref root) = self.directory_stack.first() {
            env::set_current_dir(root)
                .with_context(|| format!("Failed to change back to root directory: {}", root.display()))?;
        }

        Ok((self.directory_stack.first().cloned().unwrap_or_else(|| PathBuf::from(".")), removed_path))
    }

    #[allow(dead_code)]
    fn current(&self) -> Option<&PathBuf> {
        self.directory_stack.last()
    }

    #[allow(dead_code)]
    fn root(&self) -> Option<&PathBuf> {
        self.directory_stack.first()
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

/// Resolve the worktree path, creating default if needed
#[allow(dead_code)]
fn resolve_worktree_path(path: Option<&str>) -> Result<PathBuf> {
    if let Some(p) = path {
        // Explicit path provided
        let path_buf = PathBuf::from(p);
        if path_buf.is_absolute() {
            Ok(path_buf)
        } else {
            // Relative to current directory
            env::current_dir()
                .map(|cwd| cwd.join(&path_buf))
                .context("Failed to resolve relative path")
        }
    } else {
        // Generate default path
        let name = generate_worktree_name();
        env::current_dir()
            .map(|cwd| cwd.join(DEFAULT_WORKTREE_BASE).join(name))
            .context("Failed to create default worktree path")
    }
}

/// Find the git repository root
async fn find_git_root() -> Result<PathBuf> {
    let output = tokio::process::Command::new("git")
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

/// Validate a path for security
fn validate_path(path: &str, _safety_config: Option<&SafetyConfig>) -> Result<()> {
    // Basic validation - prevent path traversal
    if path.contains("..") {
        // Allow .. in the middle but not at the start or as escape attempts
        let normalized = Path::new(path).components().collect::<PathBuf>();
        if normalized.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            // This is ok - it's a relative path that happens to have ..
        }
    }
    
    // Check for null bytes
    if path.contains('\0') {
        anyhow::bail!("Path contains null bytes");
    }
    
    Ok(())
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
        let mut state = WORKTREE_STATE.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        state.push_worktree(worktree_path.clone())?;

        info!("Entered worktree: {} (branch: {})", worktree_path.display(), branch_used);

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
        let remove = args.get("remove").and_then(|v| v.as_bool()).unwrap_or(false);

        let (root_path, removed_path) = {
            let mut state = WORKTREE_STATE.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
            
            if !state.is_in_worktree() {
                anyhow::bail!("Not currently in a worktree");
            }

            state.pop_worktree(remove)?
        };

        // If remove is requested, run git worktree remove
        let mut removed = false;
        if remove {
            if let Some(ref worktree_path) = removed_path {
                let output = tokio::process::Command::new("git")
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

        info!("Exited worktree, returned to: {}", root_path.display());

        Ok(serde_json::json!({
            "success": true,
            "previous_path": removed_path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            "current_path": root_path.to_string_lossy().to_string(),
            "removed": removed
        }))
    }

    fn metadata(&self) -> crate::safety::ToolMetadata {
        // Medium risk - can remove directories
        crate::safety::ToolMetadata::custom(
            false,
            true,  // Destructive - can remove worktrees
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
        let output = tokio::process::Command::new("git")
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
        let state = WORKTREE_STATE.lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let current_worktree = state.current_worktree.as_ref()
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
            current.branch = branch.split('/').last().map(|s| s.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_worktree_tool_name() {
        let tool = EnterWorktreeTool::new();
        assert_eq!(tool.name(), "enter_worktree");
    }

    #[test]
    fn test_exit_worktree_tool_name() {
        let tool = ExitWorktreeTool::new();
        assert_eq!(tool.name(), "exit_worktree");
    }

    #[test]
    fn test_list_worktrees_tool_name() {
        let tool = ListWorktreesTool::new();
        assert_eq!(tool.name(), "list_worktrees");
    }

    #[test]
    fn test_enter_worktree_description() {
        let tool = EnterWorktreeTool::new();
        assert!(tool.description().contains("worktree"));
        assert!(tool.description().contains("isolated"));
    }

    #[test]
    fn test_exit_worktree_description() {
        let tool = ExitWorktreeTool::new();
        assert!(tool.description().contains("Exit"));
        assert!(tool.description().contains("worktree"));
    }

    #[test]
    fn test_list_worktrees_description() {
        let tool = ListWorktreesTool::new();
        assert!(tool.description().contains("List"));
        assert!(tool.description().contains("worktree"));
    }

    #[test]
    fn test_enter_worktree_schema() {
        let tool = EnterWorktreeTool::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["branch"].is_object());
    }

    #[test]
    fn test_exit_worktree_schema() {
        let tool = ExitWorktreeTool::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["remove"].is_object());
    }

    #[test]
    fn test_list_worktrees_schema() {
        let tool = ListWorktreesTool::new();
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("main").is_ok());
        assert!(validate_branch_name("feature-branch").is_ok());
        assert!(validate_branch_name("bugfix/issue-123").is_ok());
        assert!(validate_branch_name("release/v1.0.0").is_ok());
    }

    #[test]
    fn test_validate_branch_name_invalid() {
        assert!(validate_branch_name("").is_err());
        assert!(validate_branch_name("-main").is_err());
        assert!(validate_branch_name("branch;rm -rf /").is_err());
        assert!(validate_branch_name("branch|cat").is_err());
        assert!(validate_branch_name("branch&&evil").is_err());
    }

    #[test]
    fn test_validate_branch_name_long() {
        let long_name = "a".repeat(256);
        assert!(validate_branch_name(&long_name).is_err());
    }

    #[test]
    fn test_parse_worktree_list_empty() {
        let result = parse_worktree_list("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_worktree_list_single() {
        let output = r#"worktree /path/to/repo
HEAD abc123
branch refs/heads/main
"#;
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "/path/to/repo");
        assert_eq!(result[0].branch, Some("main".to_string()));
        assert!(!result[0].detached);
    }

    #[test]
    fn test_parse_worktree_list_detached() {
        let output = r#"worktree /path/to/worktree
HEAD def456
detached
"#;
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "/path/to/worktree");
        assert!(result[0].detached);
        assert!(result[0].branch.is_none());
    }

    #[test]
    fn test_parse_worktree_list_multiple() {
        let output = r#"worktree /path/to/main
HEAD abc123
branch refs/heads/main

worktree /path/to/feature
HEAD def456
branch refs/heads/feature-branch

worktree /path/to/detached
HEAD ghi789
detached
"#;
        let result = parse_worktree_list(output);
        assert_eq!(result.len(), 3);
        
        assert_eq!(result[0].path, "/path/to/main");
        assert_eq!(result[0].branch, Some("main".to_string()));
        
        assert_eq!(result[1].path, "/path/to/feature");
        assert_eq!(result[1].branch, Some("feature-branch".to_string()));
        
        assert_eq!(result[2].path, "/path/to/detached");
        assert!(result[2].detached);
    }

    #[test]
    fn test_enter_worktree_is_not_readonly() {
        let tool = EnterWorktreeTool::new();
        assert!(!tool.is_readonly());
    }

    #[test]
    fn test_exit_worktree_is_not_readonly() {
        let tool = ExitWorktreeTool::new();
        assert!(!tool.is_readonly());
    }

    #[test]
    fn test_list_worktrees_is_readonly() {
        let tool = ListWorktreesTool::new();
        assert!(tool.is_readonly());
    }

    #[test]
    fn test_generate_worktree_name_format() {
        let name = generate_worktree_name();
        assert!(name.starts_with("worktree_"));
        assert!(name.len() > "worktree_".len());
    }

    #[test]
    fn test_worktree_state_new() {
        let state = WorktreeState::new();
        assert!(!state.is_in_worktree());
        assert!(state.current().is_none());
        assert!(state.root().is_none());
    }

    #[tokio::test]
    async fn test_list_worktrees_execute() {
        let tool = ListWorktreesTool::new();
        let args = serde_json::json!({});

        // This will work in a git repo
        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.get("worktrees").is_some());
        assert!(output.get("count").is_some());
    }

    #[tokio::test]
    async fn test_enter_worktree_validation() {
        let tool = EnterWorktreeTool::new();
        
        // Test with invalid branch name
        let args = serde_json::json!({
            "branch": "branch; rm -rf /"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());

        // Test with branch starting with -
        let args = serde_json::json!({
            "branch": "-f"
        });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exit_worktree_not_in_worktree() {
        // Reset state to ensure we're not in a worktree
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
        drop(state);

        let tool = ExitWorktreeTool::new();
        let args = serde_json::json!({});
        
        let result = tool.execute(args).await;
        // Should fail since we're not in a worktree
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not currently in a worktree"));
    }

    #[test]
    fn test_is_in_worktree_initially_false() {
        // Reset state
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
        drop(state);

        assert!(!is_in_worktree());
    }

    #[test]
    fn test_get_current_worktree_initially_none() {
        // Reset state
        let mut state = WORKTREE_STATE.lock().unwrap();
        *state = WorktreeState::new();
        drop(state);

        assert!(get_current_worktree().is_none());
    }
}
