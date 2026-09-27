//! Who owns a resource being spawned right now.
//!
//! The agent installs an [`Owner`] as a tokio task-local around each task run
//! ([`scope`]), the same way `tools::workspace_root::scope` carries the
//! workspace root, and fills in the checkpoint's task id once it exists
//! ([`set_current_task`]). Spawn sites read [`current_owner`]; outside any
//! scope (REPL commands, the MCP server) resources are owned by the session.

use super::TaskId;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};

/// The owner cell installed for one task run.
#[derive(Debug, Clone, Default)]
pub struct Owner {
    task: Arc<Mutex<Option<TaskId>>>,
    agent: Option<String>,
}

impl Owner {
    /// An owner whose task id is filled in later with [`Owner::set_task`].
    pub fn new() -> Self {
        Self::default()
    }

    /// An owner for a known task id.
    pub fn for_task(task: impl Into<TaskId>) -> Self {
        let owner = Self::new();
        owner.set_task(task);
        owner
    }

    /// Attribute resources to a named agent as well (sub-agents, swarm roles).
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    pub fn set_task(&self, task: impl Into<TaskId>) {
        *self.task.lock().unwrap_or_else(|e| e.into_inner()) = Some(task.into());
    }

    /// The task id, if the run has created its checkpoint yet.
    pub fn task(&self) -> Option<TaskId> {
        self.task.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn agent(&self) -> Option<&str> {
        self.agent.as_deref()
    }
}

tokio::task_local! {
    static OWNER: Owner;
}

/// Run `fut` with `owner` as the owner of every resource it registers.
pub async fn scope<F: Future>(owner: Owner, fut: F) -> F::Output {
    OWNER.scope(owner, fut).await
}

/// Set the task id of the innermost [`scope`], if any. Returns whether a
/// scope was installed.
pub fn set_current_task(task: impl Into<TaskId>) -> bool {
    let task = task.into();
    OWNER.try_with(|owner| owner.set_task(task)).is_ok()
}

/// `(owner task, owner agent)` for a resource registered right now.
pub fn current_owner() -> (TaskId, Option<String>) {
    OWNER
        .try_with(|owner| {
            (
                owner.task().unwrap_or_else(session_owner),
                owner.agent.clone(),
            )
        })
        .unwrap_or_else(|_| (session_owner(), None))
}

/// This process's session id (random, stable for the process lifetime).
pub fn session_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().simple().to_string()[..12].to_string())
}

/// The owner id for session-owned resources of this process.
pub fn session_owner() -> TaskId {
    session_owner_of(session_id())
}

/// The owner id for session-owned resources of `session`.
pub fn session_owner_of(session: &str) -> TaskId {
    format!("session:{session}")
}

/// Whether `owner` names a session rather than a task.
pub fn is_session_owner(owner: &str) -> bool {
    owner.starts_with("session:")
}

/// `--label` arguments identifying the current owner, for container and
/// image creation (`docker run`/`docker build`).
pub fn container_label_args() -> Vec<String> {
    let (task, agent) = current_owner();
    let mut args = vec![
        "--label".to_string(),
        format!("selfware.task={task}"),
        "--label".to_string(),
        format!("selfware.session={}", session_id()),
    ];
    if let Some(agent) = agent {
        args.push("--label".to_string());
        args.push(format!("selfware.agent={agent}"));
    }
    args
}

/// `--label` arguments attributing a built image to the current owner.
/// Uses distinct keys from [`container_label_args`]: image labels are
/// inherited by containers run from the image, and those containers (maybe
/// the user's own) must not carry `selfware.task`.
pub fn image_label_args() -> Vec<String> {
    let (task, _) = current_owner();
    vec![
        "--label".to_string(),
        format!("selfware.built_by_task={task}"),
        "--label".to_string(),
        format!("selfware.built_by_session={}", session_id()),
    ]
}
