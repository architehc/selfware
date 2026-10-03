//! The persisted resource registry.
//!
//! One JSON file (`~/.selfware/state/resources.json`, overridable with
//! `SELFWARE_RESOURCES_FILE`; `off` keeps it in memory) shared by every
//! selfware process on the machine. Writes are read-merge-write under an
//! advisory file lock and land via temp file + rename, so a crash never
//! leaves a torn file and two sessions never drop each other's entries: an
//! entry is written back from memory only if this process created or changed
//! it, otherwise the on-disk version wins.
//!
//! Every state change is a [`ResourceEvent`] checked by
//! [`ResourceMachine`] ([`ResourceRegistry::transition`]): an accepted one is
//! appended to the lifecycle event log (`entity: resource`, owner = the task,
//! with the cause), a refused one is a typed [`TransitionError`] that is
//! logged and changes nothing. Entering `leaked` raises
//! [`Effect::LeakAlarm`], surfaced here as a warning.

use super::context::{current_owner, session_id};
use super::{Resource, ResourceHandle, ResourceKind, ResourceState, TaskId};
use crate::lifecycle::{
    Effect, EventLog, InvalidTransition, ResourceEvent, ResourceMachine, Tracked,
};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Released entries older than this are pruned on write.
const RELEASED_RETENTION_DAYS: i64 = 7;

/// A selfware process that owned resources. Liveness is checked against the
/// OS start time so a reused pid never makes a dead session look alive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub pid: u32,
    #[serde(default)]
    pub start_time: Option<u64>,
    pub started_at: DateTime<Utc>,
}

impl SessionRecord {
    /// The record for this process.
    pub fn current() -> Self {
        let pid = std::process::id();
        Self {
            id: session_id().to_string(),
            pid,
            start_time: super::driver::process_start_time(pid),
            started_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    #[serde(default)]
    pub resources: Vec<Resource>,
}

/// A registry state change that did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransitionError {
    /// No resource with this id is in the registry.
    #[error("resource {0} is not in the registry")]
    NotFound(String),
    /// The resource lifecycle refuses the event in the current state.
    #[error("resource {id}: {source}")]
    Invalid {
        /// The resource.
        id: String,
        /// The refusal.
        #[source]
        source: InvalidTransition,
    },
}

/// What a spawn site knows about the resource it just started.
#[derive(Debug, Clone)]
pub struct NewResource {
    pub kind: ResourceKind,
    pub handle: ResourceHandle,
    pub label: String,
    pub keep: bool,
    pub state: ResourceState,
}

impl NewResource {
    pub fn new(kind: ResourceKind, handle: ResourceHandle, label: impl Into<String>) -> Self {
        Self {
            kind,
            handle,
            label: label.into(),
            keep: false,
            state: ResourceState::Live,
        }
    }

    pub fn keep(mut self, keep: bool) -> Self {
        self.keep = keep;
        self
    }
}

#[derive(Default)]
struct Inner {
    loaded: bool,
    file: RegistryFile,
    /// Resource ids this process created or changed.
    touched: HashSet<String>,
    session_recorded: bool,
}

/// The resource registry (see the module docs).
pub struct ResourceRegistry {
    path: Option<PathBuf>,
    session: SessionRecord,
    events: EventLog,
    inner: Mutex<Inner>,
}

impl ResourceRegistry {
    /// A registry that is never written to disk.
    pub fn in_memory() -> Self {
        Self::build(None, SessionRecord::current())
    }

    /// A registry persisted at `path`.
    pub fn at_path(path: impl Into<PathBuf>) -> Self {
        Self::build(Some(path.into()), SessionRecord::current())
    }

    /// A registry persisted at `path` acting as `session` (tests simulate
    /// several sessions sharing one file this way).
    pub fn at_path_as(path: impl Into<PathBuf>, session: SessionRecord) -> Self {
        Self::build(Some(path.into()), session)
    }

    /// In-memory registry acting as `session`.
    pub fn in_memory_as(session: SessionRecord) -> Self {
        Self::build(None, session)
    }

    fn build(path: Option<PathBuf>, session: SessionRecord) -> Self {
        Self {
            path,
            session,
            events: EventLog::disabled(),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// Record every transition to `log` (the constructors above record
    /// nothing; [`ResourceRegistry::global`] uses the default event log).
    pub fn with_event_log(mut self, log: EventLog) -> Self {
        self.events = log;
        self
    }

    /// The event log transitions are recorded to.
    pub fn event_log(&self) -> &EventLog {
        &self.events
    }

    /// The process-wide registry, recording to the default lifecycle event
    /// log. Unit tests get an in-memory one so they never write under the
    /// real home directory (their event log is a per-process temp file).
    pub fn global() -> &'static ResourceRegistry {
        static GLOBAL: OnceLock<ResourceRegistry> = OnceLock::new();
        GLOBAL.get_or_init(|| {
            match default_path() {
                Some(path) if !cfg!(test) => ResourceRegistry::at_path(path),
                _ => ResourceRegistry::in_memory(),
            }
            .with_event_log(EventLog::default_location())
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn session(&self) -> &SessionRecord {
        &self.session
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.loaded {
            inner.loaded = true;
            if let Some(path) = &self.path {
                inner.file = read_file(path);
            }
        }
        inner
    }

    /// Register a resource owned by the current task (see
    /// [`super::context::current_owner`]). Returns its registry id.
    pub fn register(&self, new: NewResource) -> String {
        let (task, agent) = current_owner();
        self.register_owned(new, task, agent)
    }

    /// Register a resource with an explicit owner.
    pub fn register_owned(&self, new: NewResource, task: TaskId, agent: Option<String>) -> String {
        let now = Utc::now();
        let id = format!("res-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
        let resource = Resource {
            id: id.clone(),
            kind: new.kind,
            owner_task: task,
            owner_agent: agent,
            session: self.session.id.clone(),
            created_at: now,
            updated_at: now,
            state: new.state,
            handle: new.handle,
            keep: new.keep,
            label: new.label,
            note: None,
        };
        Tracked::<ResourceMachine>::new(&id, resource.state, self.events.clone())
            .with_owner(&resource.owner_task)
            .record_created(&format!(
                "registered: {} {}",
                resource.kind,
                resource.describe()
            ));
        let mut inner = self.lock();
        if !inner.session_recorded {
            inner.session_recorded = true;
            let session = self.session.clone();
            inner.file.sessions.retain(|s| s.id != session.id);
            inner.file.sessions.push(session);
        }
        inner.touched.insert(id.clone());
        inner.file.resources.push(resource);
        self.persist(&mut inner);
        id
    }

    /// Adopt an externally discovered resource (e.g. a labelled container
    /// found by the reaper) so its drain outcome is recorded.
    pub fn adopt(&self, resource: Resource) {
        Tracked::<ResourceMachine>::new(&resource.id, resource.state, self.events.clone())
            .with_owner(&resource.owner_task)
            .record_created(&format!(
                "adopted by the reaper: {} {} found by label",
                resource.kind,
                resource.describe()
            ));
        let mut inner = self.lock();
        inner.touched.insert(resource.id.clone());
        inner.file.resources.retain(|r| r.id != resource.id);
        inner.file.resources.push(resource);
        self.persist(&mut inner);
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut Resource)) -> bool {
        let mut inner = self.lock();
        let Some(resource) = inner.file.resources.iter_mut().find(|r| r.id == id) else {
            return false;
        };
        f(resource);
        resource.updated_at = Utc::now();
        inner.touched.insert(id.to_string());
        self.persist(&mut inner);
        true
    }

    /// Apply `event` to resource `id` through [`ResourceMachine`]. On success
    /// the state changes, `note` replaces the resource's note, the transition
    /// is appended to the event log with `cause`, and the effects of
    /// entering the new state are returned (a `leaked` resource raises
    /// [`Effect::LeakAlarm`], logged here as a warning). A refused event is a
    /// typed error, logged, and changes nothing.
    pub fn transition(
        &self,
        id: &str,
        event: ResourceEvent,
        cause: &str,
        note: Option<String>,
    ) -> Result<Vec<Effect>, TransitionError> {
        let mut inner = self.lock();
        let Some(resource) = inner.file.resources.iter_mut().find(|r| r.id == id) else {
            tracing::warn!("resource registry: {event:?} for unknown resource {id} ignored");
            return Err(TransitionError::NotFound(id.to_string()));
        };
        let mut tracked = Tracked::<ResourceMachine>::new(id, resource.state, self.events.clone())
            .with_owner(&resource.owner_task);
        let effects = match tracked.apply(event, cause) {
            Ok(effects) => effects,
            Err(source) => {
                let err = TransitionError::Invalid {
                    id: id.to_string(),
                    source,
                };
                tracing::warn!("resource registry: {err} ({cause})");
                return Err(err);
            }
        };
        resource.state = *tracked.state();
        resource.note = note;
        resource.updated_at = Utc::now();
        if effects.contains(&Effect::LeakAlarm) {
            tracing::warn!(
                "resource LEAKED: {} {} (owner {}): {} — see `selfware resources`, clean up with `selfware resources reap`",
                resource.kind,
                resource.describe(),
                resource.owner_task,
                resource.note.as_deref().unwrap_or(cause)
            );
        }
        inner.touched.insert(id.to_string());
        self.persist(&mut inner);
        Ok(effects)
    }

    pub fn set_handle(&self, id: &str, handle: ResourceHandle) -> bool {
        self.update(id, |r| r.handle = handle)
    }

    /// Hand a resource to a new owner (keep → session at task end).
    pub fn reown(&self, id: &str, owner: TaskId) -> bool {
        self.update(id, |r| r.owner_task = owner)
    }

    /// Mark confirmed-gone (`stopped`; `drain` for a resource that never
    /// started), recording `cause`. Callers must only use this after they
    /// observed the resource exit (a reaped child, a successful remove).
    /// Returns whether the resource is now released.
    pub fn release(&self, id: &str, cause: &str) -> bool {
        let Some(state) = self.get(id).map(|r| r.state) else {
            return false;
        };
        let event = match state {
            ResourceState::Released => return true,
            ResourceState::Requested => ResourceEvent::Drain,
            _ => ResourceEvent::Stopped,
        };
        self.transition(id, event, cause, None).is_ok()
    }

    /// Release every unreleased resource matching `pred` (e.g. the `pty`
    /// entry for a closed session), recording `cause`. Returns how many were
    /// released.
    pub fn release_where(&self, cause: &str, pred: impl Fn(&Resource) -> bool) -> usize {
        let ids: Vec<String> = self
            .unreleased()
            .into_iter()
            .filter(|r| pred(r))
            .map(|r| r.id)
            .collect();
        ids.iter().filter(|id| self.release(id, cause)).count()
    }

    pub fn get(&self, id: &str) -> Option<Resource> {
        self.lock()
            .file
            .resources
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }

    /// Every resource not yet released, in creation order.
    pub fn unreleased(&self) -> Vec<Resource> {
        self.lock()
            .file
            .resources
            .iter()
            .filter(|r| !r.state.is_released())
            .cloned()
            .collect()
    }

    /// Unreleased resources owned by `task`, in creation order.
    pub fn owned_by(&self, task: &str) -> Vec<Resource> {
        self.unreleased()
            .into_iter()
            .filter(|r| r.owner_task == task)
            .collect()
    }

    /// Re-read the file (other sessions' changes) and return a snapshot.
    pub fn snapshot(&self) -> RegistryFile {
        let mut inner = self.lock();
        if let Some(path) = &self.path {
            let disk = read_file(path);
            merge_into(&mut inner, disk);
        }
        inner.file.clone()
    }

    fn persist(&self, inner: &mut Inner) {
        let Some(path) = &self.path else {
            return;
        };
        if let Err(e) = self.persist_to(path, inner) {
            tracing::warn!(
                "resource registry: could not write {}: {e:#}",
                path.display()
            );
        }
    }

    fn persist_to(&self, path: &Path, inner: &mut Inner) -> Result<()> {
        let dir = path.parent().context("registry path has no parent")?;
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let lock_path = path.with_extension("json.lock");
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("open {}", lock_path.display()))?;
        lock.lock().context("lock resource registry")?;
        let disk = read_file(path);
        merge_into(inner, disk);
        prune(&mut inner.file, &self.session.id);
        inner.file.version = 1;
        let json = serde_json::to_string_pretty(&inner.file)?;
        crate::session::checkpoint::write_bytes_atomically(path, json.as_bytes(), 0o600)
            .with_context(|| format!("atomically write {}", path.display()))?;
        let _ = lock.unlock();
        Ok(())
    }
}

/// Merge the on-disk state into memory: our touched entries win, everything
/// else is taken from disk (another session may have changed or pruned it).
fn merge_into(inner: &mut Inner, disk: RegistryFile) {
    let mut resources: Vec<Resource> = inner
        .file
        .resources
        .iter()
        .filter(|r| inner.touched.contains(&r.id))
        .cloned()
        .collect();
    for r in disk.resources {
        if !inner.touched.contains(&r.id) {
            resources.push(r);
        }
    }
    resources.sort_by_key(|r| r.created_at);
    inner.file.resources = resources;

    let mut sessions = disk.sessions;
    for s in &inner.file.sessions {
        if !sessions.iter().any(|d| d.id == s.id) {
            sessions.push(s.clone());
        }
    }
    inner.file.sessions = sessions;
}

fn prune(file: &mut RegistryFile, current: &str) {
    let cutoff = Utc::now() - chrono::Duration::days(RELEASED_RETENTION_DAYS);
    file.resources
        .retain(|r| !(r.state.is_released() && r.updated_at < cutoff));
    let referenced: HashSet<&str> = file.resources.iter().map(|r| r.session.as_str()).collect();
    file.sessions
        .retain(|s| s.id == current || referenced.contains(s.id.as_str()));
}

fn read_file(path: &Path) -> RegistryFile {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            tracing::warn!(
                "resource registry: ignoring unreadable {}: {e}",
                path.display()
            );
            RegistryFile::default()
        }),
        Err(_) => RegistryFile::default(),
    }
}

/// `SELFWARE_RESOURCES_FILE` (`off`/empty = in memory), else
/// `~/.selfware/state/resources.json`.
pub fn default_path() -> Option<PathBuf> {
    match std::env::var("SELFWARE_RESOURCES_FILE") {
        Ok(v) if v.is_empty() || v.eq_ignore_ascii_case("off") => None,
        Ok(v) => Some(PathBuf::from(v)),
        Err(_) => {
            dirs::home_dir().map(|h| h.join(".selfware").join("state").join("resources.json"))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/resources/registry_test.rs"]
mod tests;
