//! Task-owned resource registry, teardown and reaper.
//!
//! Everything selfware starts that can outlive the tool call that started it
//! — detached containers, compose projects, background processes, PTY
//! sessions, headless browsers, bound server ports, git worktrees — is
//! recorded here as a [`Resource`] owned by the task that was running when it
//! was spawned (see [`context`]). When the task reaches a terminal state the
//! agent drains everything the task owns in reverse creation order
//! ([`teardown`]); whatever cannot be confirmed gone is reported as
//! [`ResourceState::Leaked`], never as released.
//!
//! The registry is persisted to `~/.selfware/state/resources.json`
//! ([`registry`]) so a crashed session's resources are still visible:
//! `selfware resources` lists them and `selfware resources reap` drains the
//! ones whose owner is gone ([`reaper`]). Only resources selfware recorded or
//! labelled (`selfware.task=<id>` on containers) are ever touched, and a raw
//! pid is only signalled when its OS start time still matches the recorded one.
//!
//! The state enum here is deliberately self-contained so the typed lifecycle
//! state machines (`src/lifecycle/`) can re-export it without a migration.

pub mod context;
pub mod driver;
pub mod reaper;
pub mod registry;
pub mod teardown;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use context::{session_id, Owner};
pub use driver::{Probe, ResourceDriver, SystemDriver};
pub use registry::{NewResource, ResourceRegistry, SessionRecord};
pub use teardown::{DrainReport, TeardownPolicy};

/// Identifier of the task that owns a resource: the agent checkpoint's
/// `task_id`, or `session:<session id>` for resources owned by the session
/// itself (spawned outside any task, or kept past their task with `keep`).
pub type TaskId = String;

/// What kind of thing a [`Resource`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Container,
    Process,
    Pty,
    Browser,
    ServerPort,
    Worktree,
    TempDir,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceKind::Container => "container",
            ResourceKind::Process => "process",
            ResourceKind::Pty => "pty",
            ResourceKind::Browser => "browser",
            ResourceKind::ServerPort => "server_port",
            ResourceKind::Worktree => "worktree",
            ResourceKind::TempDir => "tempdir",
        }
    }

    /// Kinds selfware can stop on its own. Worktrees, temp dirs and bound
    /// ports hold user work or die with the process; they are listed but
    /// never auto-drained.
    pub fn is_drainable(self) -> bool {
        matches!(
            self,
            ResourceKind::Container
                | ResourceKind::Process
                | ResourceKind::Pty
                | ResourceKind::Browser
        )
    }
}

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle of a registered resource.
///
/// `Released` is the only state meaning "confirmed gone". `Leaked` means a
/// drain was attempted and release could not be confirmed; `Orphaned` means
/// the owning session ended without draining it. Both still hold something
/// on the host and are reap candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    Requested,
    Starting,
    Live,
    Draining,
    Released,
    Leaked,
    Orphaned,
}

impl ResourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            ResourceState::Requested => "requested",
            ResourceState::Starting => "starting",
            ResourceState::Live => "live",
            ResourceState::Draining => "draining",
            ResourceState::Released => "released",
            ResourceState::Leaked => "leaked",
            ResourceState::Orphaned => "orphaned",
        }
    }

    /// Confirmed gone; nothing left on the host.
    pub fn is_released(self) -> bool {
        matches!(self, ResourceState::Released)
    }
}

impl std::fmt::Display for ResourceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How to reach a resource on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResourceHandle {
    /// A container started with `--label selfware.task=<task_label>`.
    Container {
        runtime: String,
        id: String,
        /// The `selfware.task` label the container was started with; a drain
        /// only acts on a container that still carries it.
        task_label: String,
    },
    /// A compose project started with `compose up` in `dir`.
    Compose {
        runtime: String,
        dir: PathBuf,
        file: Option<String>,
    },
    /// An OS process. `start_time` (seconds since the epoch, as reported by
    /// the OS) guards against pid reuse: without a match the pid is never
    /// signalled. `managed_id` is the process-manager id for processes
    /// started through the `process_start` tool.
    Process {
        pid: u32,
        pgid: Option<u32>,
        start_time: Option<u64>,
        managed_id: Option<String>,
    },
    /// A `pty_shell` session.
    Pty {
        session_id: String,
        pid: u32,
        pgid: Option<u32>,
        start_time: Option<u64>,
    },
    /// A bound TCP port.
    Port { port: u16 },
    /// A filesystem path (worktree, temp dir).
    Path { path: PathBuf },
}

impl ResourceHandle {
    /// Short human description (`container 3f2a1b9c0d11`, `pid 4242`).
    pub fn short(&self) -> String {
        match self {
            ResourceHandle::Container { id, .. } => {
                format!("container {}", id.chars().take(12).collect::<String>())
            }
            ResourceHandle::Compose { dir, .. } => format!("compose {}", dir.display()),
            ResourceHandle::Process { pid, .. } => format!("pid {pid}"),
            ResourceHandle::Pty {
                session_id, pid, ..
            } => {
                format!(
                    "pty {} (pid {pid})",
                    session_id.chars().take(8).collect::<String>()
                )
            }
            ResourceHandle::Port { port } => format!("port {port}"),
            ResourceHandle::Path { path } => path.display().to_string(),
        }
    }
}

/// A registered resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub kind: ResourceKind,
    pub owner_task: TaskId,
    #[serde(default)]
    pub owner_agent: Option<String>,
    pub session: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state: ResourceState,
    pub handle: ResourceHandle,
    /// Deliberately long-lived: re-owned by the session at task end and
    /// never auto-drained by task teardown.
    #[serde(default)]
    pub keep: bool,
    /// What it is, for listings (`npm run dev`, `nginx:latest`).
    #[serde(default)]
    pub label: String,
    /// Why the last drain did not confirm release, or other context.
    #[serde(default)]
    pub note: Option<String>,
}

impl Resource {
    /// One-line description used in summaries and listings.
    pub fn describe(&self) -> String {
        if self.label.is_empty() {
            self.handle.short()
        } else {
            format!("{} ({})", self.handle.short(), self.label)
        }
    }
}

/// Record a git worktree selfware created. Worktrees hold user work, so they
/// are `keep`: listed by `selfware resources`, never removed automatically.
pub fn record_worktree(path: &std::path::Path, label: impl Into<String>) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    ResourceRegistry::global().register(
        NewResource::new(ResourceKind::Worktree, ResourceHandle::Path { path }, label).keep(true),
    );
}

/// Mark this session's worktree entry for `path` released after a
/// successful `git worktree remove`. Pass the path canonicalized while it
/// still existed (or as registered).
pub fn release_worktree(path: &std::path::Path) {
    let session = session_id();
    ResourceRegistry::global().release_where(|r| {
        r.session == session
            && r.kind == ResourceKind::Worktree
            && matches!(&r.handle, ResourceHandle::Path { path: p } if p == path)
    });
}

#[cfg(test)]
#[path = "../../tests/unit/resources/fake.rs"]
pub(crate) mod fake;
