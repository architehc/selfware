//! Worktree lifecycle manager for isolated subagent execution.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Manages git worktrees for isolated agent execution.
///
/// Each worktree is an independent checkout of the repository at the current
/// HEAD, allowing subagents to work in complete isolation.
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    base_dir: PathBuf,
}

impl WorktreeManager {
    /// Create a new worktree manager with the given base directory.
    pub fn new(base_dir: &Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }

    /// Create a new detached worktree for the given id, using the repo at `source`.
    pub async fn create_worktree(&self, id: &str, source: &Path) -> Result<PathBuf> {
        let worktree_path = self.base_dir.join(id);
        tokio::fs::create_dir_all(&self.base_dir)
            .await
            .with_context(|| format!("Failed to create base dir: {}", self.base_dir.display()))?;

        let git_root = Self::find_git_root(source).await?;
        let output = tokio::process::Command::new("git")
            .args([
                "worktree",
                "add",
                "--detach",
                &worktree_path.to_string_lossy(),
            ])
            .current_dir(&git_root)
            .output()
            .await
            .context("Failed to execute git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {}", stderr);
        }

        info!(
            worktree_id = %id,
            path = %worktree_path.display(),
            "Created worktree"
        );
        Ok(worktree_path)
    }

    /// Remove a worktree by id.
    ///
    /// First attempts `git worktree remove`. If that fails (e.g. because the
    /// worktree is not recognised), falls back to a manual directory removal.
    pub async fn remove_worktree(&self, id: &str) -> Result<()> {
        let worktree_path = self.base_dir.join(id);
        if !tokio::fs::try_exists(&worktree_path).await.unwrap_or(false) {
            warn!(worktree_id = %id, "Worktree does not exist, skipping removal");
            return Ok(());
        }

        // Try `git worktree remove` from inside the worktree so git knows
        // which repository it belongs to.
        let git_result = tokio::process::Command::new("git")
            .args(["worktree", "remove", "--force"])
            .current_dir(&worktree_path)
            .output()
            .await;

        match git_result {
            Ok(output) if output.status.success() => {
                info!(worktree_id = %id, "Removed worktree via git");
                return Ok(());
            }
            Ok(output) => {
                warn!(
                    worktree_id = %id,
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "git worktree remove failed, falling back to manual removal"
                );
            }
            Err(e) => {
                warn!(worktree_id = %id, error = %e, "Failed to run git worktree remove");
            }
        }

        // Fallback: manual removal.
        tokio::fs::remove_dir_all(&worktree_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to remove worktree directory: {}",
                    worktree_path.display()
                )
            })?;

        info!(worktree_id = %id, "Removed worktree manually");
        Ok(())
    }

    /// List all managed worktree paths.
    pub async fn list_worktrees(&self) -> Vec<PathBuf> {
        match tokio::fs::read_dir(&self.base_dir).await {
            Ok(mut rd) => {
                let mut paths = Vec::new();
                while let Ok(Some(entry)) = rd.next_entry().await {
                    let path = entry.path();
                    if tokio::fs::metadata(&path)
                        .await
                        .map(|m| m.is_dir())
                        .unwrap_or(false)
                    {
                        paths.push(path);
                    }
                }
                paths
            }
            Err(_) => Vec::new(),
        }
    }

    pub async fn find_git_root(path: &Path) -> Result<PathBuf> {
        let output = tokio::process::Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "rev-parse",
                "--show-toplevel",
            ])
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
        // Create an initial commit so HEAD exists.
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
    async fn test_create_and_remove_worktree() {
        let repo = init_temp_git_repo();
        let base = repo.path().join("worktrees");
        let manager = WorktreeManager::new(&base);

        let wt = manager.create_worktree("wt-1", repo.path()).await.unwrap();
        assert!(wt.exists());
        assert!(wt.join(".git").exists() || wt.join(".git").is_symlink());
        assert!(wt.join("README.md").exists());

        let listed = manager.list_worktrees().await;
        assert_eq!(listed.len(), 1);

        manager.remove_worktree("wt-1").await.unwrap();
        assert!(!wt.exists());
        assert!(manager.list_worktrees().await.is_empty());
    }

    #[tokio::test]
    async fn test_worktree_isolation() {
        let repo = init_temp_git_repo();
        let base = repo.path().join("worktrees");
        let manager = WorktreeManager::new(&base);

        let wt = manager.create_worktree("iso", repo.path()).await.unwrap();
        // Modify file in worktree without affecting main repo.
        std::fs::write(wt.join("README.md"), "modified").unwrap();

        let main_content = std::fs::read_to_string(repo.path().join("README.md")).unwrap();
        assert_eq!(main_content, "# test");

        let wt_content = std::fs::read_to_string(wt.join("README.md")).unwrap();
        assert_eq!(wt_content, "modified");

        manager.remove_worktree("iso").await.unwrap();
    }
}
