//! Git state and guarded branch creation for graph actions.

use anyhow::{bail, Context, Result};
use git2::{Repository, Status, StatusOptions};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileState {
    pub path: String,
    pub index: String,
    pub worktree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusReport {
    pub branch: Option<String>,
    pub head: String,
    pub dirty: bool,
    pub files: Vec<GitFileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchResult {
    pub name: String,
    pub head: String,
    pub created: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitEngine {
    project_root: PathBuf,
}

impl GitEngine {
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        Self {
            project_root: project_root.as_ref().to_path_buf(),
        }
    }

    pub fn status(&self) -> Result<GitStatusReport> {
        let repo =
            Repository::discover(&self.project_root).context("not inside a git repository")?;
        let head_ref = repo.head().context("repository has no HEAD")?;
        let head = head_ref
            .target()
            .context("HEAD does not point to a commit")?
            .to_string();
        let branch = head_ref.shorthand().ok().map(str::to_string);

        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = repo.statuses(Some(&mut options))?;
        let mut files = statuses
            .iter()
            .filter_map(|entry| {
                let path = entry.path().ok()?.to_string();
                let status = entry.status();
                Some(GitFileState {
                    path,
                    index: index_state(status).to_string(),
                    worktree: worktree_state(status).to_string(),
                })
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(GitStatusReport {
            branch,
            head,
            dirty: !files.is_empty(),
            files,
        })
    }

    /// Preview or create a branch from an exact HEAD.
    ///
    /// Creation is rejected for dirty worktrees so a graph action cannot
    /// silently carry unrelated changes onto its action branch.
    pub fn create_branch(
        &self,
        name: &str,
        expected_head: &str,
        confirm: bool,
    ) -> Result<BranchResult> {
        validate_branch_name(name)?;
        let status = self.status()?;
        let mut blockers = Vec::new();
        if status.head != expected_head {
            blockers.push(format!(
                "HEAD changed: expected {expected_head}, found {}",
                status.head
            ));
        }
        if status.dirty {
            blockers.push(format!(
                "working tree has {} uncommitted path(s)",
                status.files.len()
            ));
        }

        let repo = Repository::discover(&self.project_root)?;
        if repo.state() != git2::RepositoryState::Clean {
            blockers.push(format!(
                "repository operation in progress: {:?}",
                repo.state()
            ));
        }
        if repo.find_branch(name, git2::BranchType::Local).is_ok() {
            blockers.push(format!("branch already exists: {name}"));
        }

        if !confirm || !blockers.is_empty() {
            return Ok(BranchResult {
                name: name.to_string(),
                head: status.head,
                created: false,
                blockers,
            });
        }

        // Lock every ref involved before the final validation. The API mutex
        // only serializes self-evolve requests; Git's ref locks exclude other
        // clients that could otherwise move the current branch between the
        // validation and the HEAD switch.
        let branch_ref = format!("refs/heads/{name}");
        let current_head_ref = repo.head()?.name()?.to_string();
        let mut transaction = repo.transaction()?;
        let mut locked_refs = vec!["HEAD".to_string(), branch_ref.clone(), current_head_ref];
        locked_refs.sort();
        locked_refs.dedup();
        for reference in &locked_refs {
            transaction.lock_ref(reference)?;
        }

        // Re-read externally mutable state only after the relevant refs are
        // locked. Filesystem edits can still make the worktree dirty, so that
        // condition is checked at the same final boundary as HEAD.
        let current = self.status()?;
        if current.head != expected_head || current.dirty {
            return Ok(BranchResult {
                name: name.to_string(),
                head: current.head.clone(),
                created: false,
                blockers: vec![format!(
                    "repository changed after preview: HEAD {}, {} uncommitted path(s)",
                    current.head,
                    current.files.len()
                )],
            });
        }
        if repo.find_reference(&branch_ref).is_ok() {
            return Ok(BranchResult {
                name: name.to_string(),
                head: current.head,
                created: false,
                blockers: vec![format!("branch appeared after preview: {name}")],
            });
        }
        if repo.state() != git2::RepositoryState::Clean {
            return Ok(BranchResult {
                name: name.to_string(),
                head: current.head,
                created: false,
                blockers: vec![format!(
                    "repository operation in progress: {:?}",
                    repo.state()
                )],
            });
        }

        let oid = git2::Oid::from_str(expected_head).context("invalid expected HEAD")?;
        let commit = repo
            .find_commit(oid)
            .context("expected HEAD commit not found")?;
        drop(commit);
        transaction.set_target(
            &branch_ref,
            oid,
            None,
            "self-evolve: create guarded action branch",
        )?;
        transaction.set_symbolic_target(
            "HEAD",
            &branch_ref,
            None,
            "self-evolve: switch to guarded action branch",
        )?;
        if let Err(error) = transaction.commit() {
            if let Ok(mut branch) = repo.find_branch(name, git2::BranchType::Local) {
                let _ = branch.delete();
            }
            return Err(error.into());
        }

        Ok(BranchResult {
            name: name.to_string(),
            head: expected_head.to_string(),
            created: true,
            blockers,
        })
    }
}

fn validate_branch_name(name: &str) -> Result<()> {
    let full = format!("refs/heads/{name}");
    if name.trim().is_empty()
        || name.starts_with('-')
        || name.contains("..")
        || name.ends_with('.')
        || name.ends_with('/')
        || !git2::Reference::is_valid_name(&full)
    {
        bail!("invalid branch name");
    }
    Ok(())
}

fn index_state(status: Status) -> &'static str {
    if status.contains(Status::INDEX_NEW) {
        "added"
    } else if status.contains(Status::INDEX_MODIFIED) {
        "modified"
    } else if status.contains(Status::INDEX_DELETED) {
        "deleted"
    } else if status.contains(Status::INDEX_RENAMED) {
        "renamed"
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        "typechange"
    } else {
        "clean"
    }
}

fn worktree_state(status: Status) -> &'static str {
    if status.contains(Status::WT_NEW) {
        "untracked"
    } else if status.contains(Status::WT_MODIFIED) {
        "modified"
    } else if status.contains(Status::WT_DELETED) {
        "deleted"
    } else if status.contains(Status::WT_RENAMED) {
        "renamed"
    } else if status.contains(Status::WT_TYPECHANGE) {
        "typechange"
    } else if status.contains(Status::CONFLICTED) {
        "conflicted"
    } else {
        "clean"
    }
}
