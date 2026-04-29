//! Real subagent runner with worktree isolation.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::agent::Agent;
use crate::config::Config;

/// Maximum concurrent subagents across the entire process.
const DEFAULT_MAX_CONCURRENT_SUBAGENTS: usize = 3;
/// Default turn budget for each subagent.
const DEFAULT_MAX_TURNS: usize = 30;

/// Role of a subagent in the SWE workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentRole {
    /// Finds relevant files for an issue.
    Localizer,
    /// Makes the actual code changes.
    Patcher,
    /// Runs tests and verification.
    Verifier,
}

impl SubagentRole {
    /// Human-readable name for the role.
    pub fn name(&self) -> &'static str {
        match self {
            SubagentRole::Localizer => "localizer",
            SubagentRole::Patcher => "patcher",
            SubagentRole::Verifier => "verifier",
        }
    }

    /// Role-specific task prompt prefix.
    pub fn prompt_prefix(&self) -> &'static str {
        match self {
            SubagentRole::Localizer => {
                "You are a code localizer. Your job is to find the files relevant to this issue. \
                 Search the codebase, identify the root cause, and list the files that need to change. \
                 Do NOT make any edits — only investigate and report."
            }
            SubagentRole::Patcher => {
                "You are a code patcher. Your job is to fix the issue by editing the relevant files. \
                 Make minimal, targeted changes. After editing, run cargo check and cargo test to verify."
            }
            SubagentRole::Verifier => {
                "You are a code verifier. Your job is to run the test suite and confirm the fix works. \
                 Run cargo test and report which tests pass or fail. Do NOT make edits unless tests reveal a remaining bug."
            }
        }
    }
}

/// Result of a subagent execution.
#[derive(Debug, Clone)]
pub struct SubagentResult {
    /// Git diff patch produced by the subagent (if any).
    pub patch: Option<String>,
    /// Paths to artifacts produced in the worktree.
    pub artifacts: Vec<String>,
    /// Execution logs.
    pub logs: Vec<String>,
    /// Whether the subagent completed successfully.
    pub success: bool,
    /// Error message if the subagent failed.
    pub error: Option<String>,
}

/// A real subagent that runs an [`Agent`] inside an isolated worktree.
pub struct Subagent {
    id: String,
    workdir: PathBuf,
    role: SubagentRole,
    config: Config,
    max_turns: usize,
}

// Global semaphore to limit concurrent subagents.
static SUBAGENT_SEMAPHORE: once_cell::sync::Lazy<Semaphore> =
    once_cell::sync::Lazy::new(|| Semaphore::new(DEFAULT_MAX_CONCURRENT_SUBAGENTS));

// Global mutex to serialize process-wide directory changes.
static WORKDIR_MUTEX: once_cell::sync::Lazy<tokio::sync::Mutex<()>> =
    once_cell::sync::Lazy::new(|| tokio::sync::Mutex::new(()));

impl Subagent {
    /// Create a new subagent.
    pub fn new(
        id: impl Into<String>,
        workdir: PathBuf,
        role: SubagentRole,
        config: Config,
    ) -> Self {
        Self {
            id: id.into(),
            workdir,
            role,
            config,
            max_turns: DEFAULT_MAX_TURNS,
        }
    }

    /// Set a custom turn budget (default is 30).
    pub fn with_max_turns(mut self, turns: usize) -> Self {
        self.max_turns = turns;
        self
    }

    /// Run the subagent on the given task.
    ///
    /// This method acquires a process-wide semaphore permit and a directory
    /// change mutex before creating and running the agent in the worktree.
    /// Directory changes are process-global, so subagents serialize on the
    /// mutex for safety.
    pub async fn run(&self, task: &str) -> Result<SubagentResult> {
        // Acquire a permit from the global semaphore.
        let _permit = SUBAGENT_SEMAPHORE
            .acquire()
            .await
            .context("Failed to acquire subagent semaphore permit")?;

        // Acquire the directory-change mutex.
        let _dir_guard = WORKDIR_MUTEX.lock().await;

        info!(
            subagent_id = %self.id,
            role = %self.role.name(),
            workdir = %self.workdir.display(),
            "Starting subagent execution"
        );

        // Remember original directory and switch to worktree.
        let original_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        std::env::set_current_dir(&self.workdir).with_context(|| {
            format!(
                "Failed to change to worktree directory: {}",
                self.workdir.display()
            )
        })?;

        // Prepare config with limited turns.
        let mut config = self.config.clone();
        config.agent.max_iterations = self.max_turns;

        // Run the agent.
        let full_task = format!("{}\n\nTask: {}", self.role.prompt_prefix(), task);
        let run_result = async {
            let mut agent = Agent::new(config).await?;
            agent.run_task(&full_task).await
        }
        .await;

        // Restore original directory.
        if let Err(e) = std::env::set_current_dir(&original_dir) {
            warn!(error = %e, "Failed to restore original directory");
        }

        // Collect results.
        let success = run_result.is_ok();
        let error = run_result.err().map(|e| e.to_string());

        let patch = self.extract_patch().await.ok();
        let artifacts = self.collect_artifacts().await.unwrap_or_default();

        Ok(SubagentResult {
            patch,
            artifacts,
            logs: vec![], // Logs could be captured via tracing subscriber in future.
            success,
            error,
        })
    }

    /// Extract a git diff patch from the worktree.
    async fn extract_patch(&self) -> Result<String> {
        let output = tokio::process::Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&self.workdir)
            .output()
            .await
            .context("Failed to run git diff")?;

        if !output.status.success() {
            anyhow::bail!(
                "git diff failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if diff.trim().is_empty() {
            anyhow::bail!("No changes in worktree");
        }
        Ok(diff)
    }

    /// Collect artifact paths from the worktree (modified files).
    async fn collect_artifacts(&self) -> Result<Vec<String>> {
        let output = tokio::process::Command::new("git")
            .args(["status", "--short"])
            .current_dir(&self.workdir)
            .output()
            .await
            .context("Failed to run git status")?;

        let status = String::from_utf8_lossy(&output.stdout);
        let artifacts: Vec<String> = status
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.last().map(|s| s.to_string())
            })
            .collect();
        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_temp_git_repo() -> tempfile::TempDir {
        let tmp = tempfile::TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .expect("git init failed");
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        std::fs::write(tmp.path().join("README.md"), "# test").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        tmp
    }

    #[tokio::test]
    async fn test_subagent_runs_in_isolated_directory() {
        let repo = init_temp_git_repo();
        let base = repo.path().join("worktrees");
        let manager = crate::agent::worktree::WorktreeManager::new(&base);
        let workdir = manager.create_worktree("sub-1", repo.path()).await.unwrap();

        // Verify the worktree is isolated by writing a file in it.
        std::fs::write(workdir.join("isolated.txt"), "hello").unwrap();
        assert!(workdir.join("isolated.txt").exists());
        assert!(!repo.path().join("isolated.txt").exists());

        manager.remove_worktree("sub-1").await.unwrap();
    }

    #[test]
    fn test_subagent_role_names() {
        assert_eq!(SubagentRole::Localizer.name(), "localizer");
        assert_eq!(SubagentRole::Patcher.name(), "patcher");
        assert_eq!(SubagentRole::Verifier.name(), "verifier");
    }

    #[test]
    fn test_subagent_result_defaults() {
        let result = SubagentResult {
            patch: None,
            artifacts: vec![],
            logs: vec![],
            success: false,
            error: None,
        };
        assert!(!result.success);
        assert!(result.patch.is_none());
    }
}
