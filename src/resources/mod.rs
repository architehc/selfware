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
//! There is one resource state model: [`ResourceState`], [`ResourceKind`] and
//! [`ResourceEvent`] are the typed lifecycle's (`crate::lifecycle`), and every
//! registry state change is an event checked by `ResourceMachine` (the table
//! proved in `formal/ResourceFsm.lean`) and appended to the lifecycle event
//! log (`entity: resource`, with a cause). A transition the table refuses is
//! a typed [`TransitionError`], logged, never a panic; entering `leaked`
//! raises `Effect::LeakAlarm`, which the registry surfaces as a warning.

pub mod context;
pub mod driver;
pub mod reaper;
pub mod registry;
pub mod teardown;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use crate::lifecycle::{ResourceEvent, ResourceKind, ResourceState};
pub use context::{session_id, Owner};
pub use driver::{Probe, ResourceDriver, SystemDriver};
pub use registry::{NewResource, ResourceRegistry, SessionRecord, TransitionError};
pub use teardown::{DrainReport, TeardownPolicy};

/// Identifier of the task that owns a resource: the agent checkpoint's
/// `task_id`, or `session:<session id>` for resources owned by the session
/// itself (spawned outside any task, or kept past their task with `keep`).
pub type TaskId = String;

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
        /// The unique `selfware.run=<value>` label `container_run` starts the
        /// container with. The entry is registered BEFORE the runtime is
        /// spawned, with an empty `id`: a run cancelled or timed out before
        /// its id was read is still recorded, and teardown resolves the id
        /// from this label ([`crate::resources::ResourceDriver::container_by_run_label`]).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_label: Option<String>,
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
            ResourceHandle::Container {
                id,
                run_label: Some(run),
                ..
            } if id.is_empty() => {
                format!(
                    "container run {} (id not yet known)",
                    run.chars().take(12).collect::<String>()
                )
            }
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
    ResourceRegistry::global().release_where("git worktree removed", |r| {
        r.session == session
            && r.kind == ResourceKind::Worktree
            && matches!(&r.handle, ResourceHandle::Path { path: p } if p == path)
    });
}

/// Register a process the *session* owns — not whichever task happens to be
/// running when it is spawned. MCP stdio servers are the case: they are
/// started once per agent and serve every later task, so a task teardown
/// must not stop them, but session end must (and after a crash the reaper
/// finds them because their session is gone). The OS start time is recorded
/// with the pid, so a drain or reap never signals a reused pid. `pgid` is the
/// process group the pid leads, if it was spawned into its own.
pub fn register_session_process(
    registry: &ResourceRegistry,
    kind: ResourceKind,
    pid: u32,
    pgid: Option<u32>,
    label: impl Into<String>,
) -> String {
    registry.register_owned(
        NewResource::new(
            kind,
            ResourceHandle::Process {
                pid,
                pgid,
                start_time: driver::process_start_time(pid),
                managed_id: None,
            },
            label,
        ),
        context::session_owner_of(&registry.session().id),
        None,
    )
}

/// This session's unreleased registry entry for managed process
/// `managed_id` (started through the `process_start` tool), if any.
pub fn managed_process_entry(registry: &ResourceRegistry, managed_id: &str) -> Option<Resource> {
    let session = registry.session().id.clone();
    registry.unreleased().into_iter().find(|r| {
        r.session == session
            && matches!(&r.handle, ResourceHandle::Process { managed_id: Some(m), .. } if m == managed_id)
    })
}

/// Managed process `managed_id` was restarted (automatically after a crash,
/// or by `process_restart`) and now runs as `pid`: point its registry entry
/// at the new pid, process group and OS start time, so task teardown and a
/// post-crash `selfware resources reap` find the running process instead of
/// the dead one. Returns whether an entry was updated.
pub fn refresh_managed_process(
    registry: &ResourceRegistry,
    managed_id: &str,
    pid: u32,
    start_time: Option<u64>,
) -> bool {
    let Some(entry) = managed_process_entry(registry, managed_id) else {
        return false;
    };
    let handle = ResourceHandle::Process {
        pid,
        pgid: cfg!(unix).then_some(pid),
        start_time,
        managed_id: Some(managed_id.to_string()),
    };
    if entry.handle == handle {
        return true;
    }
    tracing::info!(
        "resource registry: managed process {managed_id} restarted as pid {pid}; entry {} updated",
        entry.id
    );
    registry.set_handle(&entry.id, handle)
}

#[cfg(test)]
#[path = "../../tests/unit/resources/fake.rs"]
pub(crate) mod fake;
