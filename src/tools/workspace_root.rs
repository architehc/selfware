//! Explicit per-agent workspace root.
//!
//! The workspace root is the directory every tool resolves relative paths
//! against, validates paths against (see
//! [`crate::tools::file::validate_tool_path`]) and runs subprocesses in. It
//! used to be the PROCESS current directory, and entering a git worktree
//! called `std::env::set_current_dir` — a single global shared by every Tokio
//! worker thread, every concurrently running agent, background job, LSP server
//! and subprocess. Entering a worktree in one agent therefore silently moved
//! the workspace of everything else in the process.
//!
//! A [`WorkspaceRoot`] is a cheaply clonable handle (`Arc<RwLock<..>>`) owned
//! by an agent's [`crate::tools::ToolRegistry`]. The registry (and the agent's
//! tool dispatch) runs every tool call inside [`scope`], which installs the
//! handle as a Tokio task-local; tools read it through [`current`] /
//! [`current_path`] / [`anchor`] / [`CommandRootExt::in_workspace_root`]
//! without any change to the `Tool::execute(args)` signature. Entering or
//! exiting a worktree pushes/pops a level on the handle — the process cwd is
//! never touched.
//!
//! A root that has not entered a worktree *follows the process cwd*: it
//! resolves to `std::env::current_dir()` at use time. Production code never
//! changes the process cwd after startup, so this equals "the cwd at startup",
//! and behaviour with no worktree entered is byte-identical to the legacy
//! behaviour: relative paths are left relative ([`anchor`] is a no-op) and
//! subprocesses inherit the process cwd ([`CommandRootExt::in_workspace_root`]
//! is a no-op). Only once a root diverges from the process cwd (a worktree was
//! entered, or the root was pinned with [`WorkspaceRoot::fixed`]) do paths get
//! anchored and commands get an explicit `current_dir`.
//!
//! Outside any [`scope`] (unit tests calling a tool directly, startup code)
//! [`current`] falls back to one process-default handle with the same
//! follow-the-cwd semantics.
//!
//! Task-locals do not cross `tokio::spawn` / `spawn_blocking`: a tool that
//! hands work to another task must resolve its paths (or capture
//! [`current`]) before spawning.

use anyhow::{Context, Result};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

#[derive(Debug)]
struct RootState {
    /// `None` = follow the process cwd; `Some` = pinned base directory.
    base: Option<PathBuf>,
    /// Entered worktrees, innermost last. Empty = not in a worktree.
    worktrees: Vec<PathBuf>,
}

/// A shared, explicit workspace root (see the module docs).
#[derive(Clone, Debug)]
pub struct WorkspaceRoot {
    inner: Arc<RwLock<RootState>>,
}

fn process_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

impl Default for WorkspaceRoot {
    fn default() -> Self {
        Self::follow_process_cwd()
    }
}

impl WorkspaceRoot {
    /// A root whose base is the process cwd at use time (see module docs).
    pub fn follow_process_cwd() -> Self {
        Self::with_base(None)
    }

    /// A root pinned to an explicit base directory, independent of the
    /// process cwd.
    pub fn fixed(base: impl Into<PathBuf>) -> Self {
        let base = base.into();
        let base = if base.is_absolute() {
            base
        } else {
            process_cwd().join(base)
        };
        Self::with_base(Some(base))
    }

    fn with_base(base: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RootState {
                base,
                worktrees: Vec::new(),
            })),
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, RootState> {
        self.inner.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, RootState> {
        self.inner.write().unwrap_or_else(|e| e.into_inner())
    }

    /// The directory tools currently resolve against: the innermost entered
    /// worktree, else the base.
    pub fn path(&self) -> PathBuf {
        let state = self.read();
        match state.worktrees.last() {
            Some(wt) => wt.clone(),
            None => state.base.clone().unwrap_or_else(process_cwd),
        }
    }

    /// The base directory (outside every worktree).
    pub fn base(&self) -> PathBuf {
        self.read().base.clone().unwrap_or_else(process_cwd)
    }

    /// The root differs from the process cwd by construction (pinned base or
    /// an entered worktree). When `false`, the OS already resolves relative
    /// paths and spawns subprocesses exactly where this root points.
    pub fn is_explicit(&self) -> bool {
        let state = self.read();
        state.base.is_some() || !state.worktrees.is_empty()
    }

    /// Whether a worktree is currently entered on this root.
    pub fn is_in_worktree(&self) -> bool {
        !self.read().worktrees.is_empty()
    }

    /// The innermost entered worktree, if any.
    pub fn current_worktree(&self) -> Option<PathBuf> {
        self.read().worktrees.last().cloned()
    }

    /// True when both handles share the same underlying root.
    pub fn same_root(&self, other: &WorkspaceRoot) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    /// Resolve `path` against this root: absolute paths are returned as-is,
    /// relative ones are joined onto [`Self::path`].
    pub fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.path().join(path)
        }
    }

    /// Like [`Self::resolve`], but a relative path is left untouched when the
    /// root is not explicit (it follows the process cwd, so the OS resolves
    /// it identically) — keeping tool inputs/outputs byte-identical when no
    /// worktree is entered.
    pub fn anchor_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() || !self.is_explicit() {
            path.to_path_buf()
        } else {
            self.path().join(path)
        }
    }

    /// String form of [`Self::anchor_path`].
    pub fn anchor_str(&self, path: &str) -> String {
        let p = Path::new(path);
        if p.is_absolute() || !self.is_explicit() {
            path.to_string()
        } else {
            self.path().join(p).to_string_lossy().into_owned()
        }
    }

    /// The directory a subprocess must be started in, or `None` when the
    /// root follows the process cwd (the child inherits the right directory).
    pub fn command_dir(&self) -> Option<PathBuf> {
        if self.is_explicit() {
            Some(self.path())
        } else {
            None
        }
    }

    /// Enter `dir` as a worktree level. A relative `dir` is resolved against
    /// the current root. The directory must exist; on failure nothing changes.
    /// Returns the (resolved) directory now in effect. The process cwd is not
    /// touched.
    pub fn enter(&self, dir: &Path) -> Result<PathBuf> {
        let mut state = self.write();
        let current = match state.worktrees.last() {
            Some(wt) => wt.clone(),
            None => state.base.clone().unwrap_or_else(process_cwd),
        };
        let target = if dir.is_absolute() {
            dir.to_path_buf()
        } else {
            current.join(dir)
        };
        if !target.is_dir() {
            anyhow::bail!(
                "Failed to change to worktree directory: {} (not a directory)",
                target.display()
            );
        }
        state.worktrees.push(target.clone());
        Ok(target)
    }

    /// Leave the innermost worktree. Returns `(restored, left)`: the directory
    /// now in effect and the worktree just left. Fails — leaving the state
    /// unchanged — when not in a worktree or when the directory to restore to
    /// no longer exists (an exit must never report success while every later
    /// relative path would resolve against a vanished directory).
    pub fn exit(&self) -> Result<(PathBuf, PathBuf)> {
        let mut state = self.write();
        let len = state.worktrees.len();
        if len == 0 {
            anyhow::bail!("Not currently in a worktree");
        }
        let restore_to = if len >= 2 {
            state.worktrees[len - 2].clone()
        } else {
            state.base.clone().unwrap_or_else(process_cwd)
        };
        if !restore_to.is_dir() {
            return Err(anyhow::anyhow!("directory no longer exists")).with_context(|| {
                format!(
                    "Failed to restore working directory to {}",
                    restore_to.display()
                )
            });
        }
        let left = state
            .worktrees
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Not currently in a worktree"))?;
        Ok((restore_to, left))
    }

    /// Drop every entered worktree level (test helper / hard reset).
    pub fn reset_worktrees(&self) {
        self.write().worktrees.clear();
    }
}

tokio::task_local! {
    static CURRENT: WorkspaceRoot;
}

fn process_default() -> &'static WorkspaceRoot {
    static DEFAULT: OnceLock<WorkspaceRoot> = OnceLock::new();
    DEFAULT.get_or_init(WorkspaceRoot::follow_process_cwd)
}

/// The workspace root of the running tool call: the task-local installed by
/// [`scope`], else the process-default root.
pub fn current() -> WorkspaceRoot {
    CURRENT
        .try_with(Clone::clone)
        .unwrap_or_else(|_| process_default().clone())
}

/// [`WorkspaceRoot::path`] of [`current`].
pub fn current_path() -> PathBuf {
    current().path()
}

/// [`WorkspaceRoot::anchor_str`] of [`current`].
pub fn anchor(path: &str) -> String {
    current().anchor_str(path)
}

/// [`WorkspaceRoot::anchor_path`] of [`current`].
pub fn anchor_path(path: &Path) -> PathBuf {
    current().anchor_path(path)
}

/// Anchor the path-valued keys `keys` of a tool's JSON arguments to the
/// [`current`] root (see [`WorkspaceRoot::anchor_str`]). A string value is
/// anchored; an array is walked — string items are anchored, object items
/// are recursed into with the same keys (e.g. `file_multi_edit`'s
/// `edits[].path`). A no-op when the root follows the process cwd, so tool
/// inputs stay byte-identical when no worktree is entered.
pub fn anchor_json(mut args: serde_json::Value, keys: &[&str]) -> serde_json::Value {
    let root = current();
    if root.is_explicit() {
        anchor_json_in(&root, &mut args, keys);
    }
    args
}

fn anchor_json_in(root: &WorkspaceRoot, value: &mut serde_json::Value, keys: &[&str]) {
    use serde_json::Value;
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    for key in keys {
        match obj.get_mut(*key) {
            Some(Value::String(s)) => *s = root.anchor_str(s),
            Some(Value::Array(items)) => {
                for item in items.iter_mut() {
                    match item {
                        Value::String(s) => *s = root.anchor_str(s),
                        Value::Object(_) => anchor_json_in(root, item, keys),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Run `fut` with `root` installed as the task-local workspace root.
pub async fn scope<F: Future>(root: WorkspaceRoot, fut: F) -> F::Output {
    CURRENT.scope(root, fut).await
}

/// Synchronous counterpart of [`scope`].
pub fn sync_scope<R>(root: WorkspaceRoot, f: impl FnOnce() -> R) -> R {
    CURRENT.sync_scope(root, f)
}

/// `tokio::task::spawn_blocking` that carries the [`current`] root into the
/// blocking closure (task-locals do not cross the spawn on their own), so
/// path validation and anchoring inside it resolve against the caller's
/// workspace root rather than the process default.
pub fn spawn_blocking<F, R>(f: F) -> tokio::task::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let root = current();
    tokio::task::spawn_blocking(move || sync_scope(root, f))
}

/// Start a subprocess in the current workspace root.
pub trait CommandRootExt {
    /// Set the command's `current_dir` to the [`current`] root when it is
    /// explicit (no-op when the root follows the process cwd, which the child
    /// inherits anyway).
    fn in_workspace_root(&mut self) -> &mut Self {
        self.in_root(&current())
    }

    /// Set the command's `current_dir` to `root` when it is explicit.
    fn in_root(&mut self, root: &WorkspaceRoot) -> &mut Self;
}

impl CommandRootExt for std::process::Command {
    fn in_root(&mut self, root: &WorkspaceRoot) -> &mut Self {
        if let Some(dir) = root.command_dir() {
            self.current_dir(dir);
        }
        self
    }
}

impl CommandRootExt for tokio::process::Command {
    fn in_root(&mut self, root: &WorkspaceRoot) -> &mut Self {
        if let Some(dir) = root.command_dir() {
            self.current_dir(dir);
        }
        self
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/workspace_root/workspace_root_test.rs"]
mod tests;
