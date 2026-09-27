//! A scripted, in-memory [`ResourceDriver`] for teardown/reaper tests. It
//! never touches the host: no docker, no signals.

use crate::resources::driver::{LabelledContainer, Probe, ResourceDriver};
use crate::resources::registry::SessionRecord;
use crate::resources::Resource;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// How a scripted resource reacts to stop requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behavior {
    StopsOnPolite,
    StopsOnForce,
    NeverStops,
    Foreign,
    AlreadyGone,
    /// Stops, but removing what remains fails.
    FinalizeFails,
}

#[derive(Default)]
pub struct FakeDriver {
    /// "polite:<key>", "force:<key>", "finalize:<key>" in call order.
    pub log: Mutex<Vec<String>>,
    behaviors: Mutex<HashMap<String, Behavior>>,
    gone: Mutex<HashSet<String>>,
    pub containers: Vec<LabelledContainer>,
    pub dead_sessions: HashSet<String>,
    pub containers_error: Option<String>,
    /// `selfware.run` label -> container id, for `container_by_run_label`.
    pub run_labels: Mutex<HashMap<String, String>>,
    /// When set, `container_by_run_label` fails with this error.
    pub run_lookup_error: Option<String>,
}

/// The key a resource is scripted by: its handle's short form.
pub fn key(resource: &Resource) -> String {
    resource.handle.short()
}

impl FakeDriver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn script(&self, key: impl Into<String>, behavior: Behavior) {
        let key = key.into();
        if behavior == Behavior::AlreadyGone {
            self.gone.lock().unwrap().insert(key.clone());
        }
        self.behaviors.lock().unwrap().insert(key, behavior);
    }

    fn behavior(&self, key: &str) -> Behavior {
        self.behaviors
            .lock()
            .unwrap()
            .get(key)
            .copied()
            .unwrap_or(Behavior::StopsOnPolite)
    }

    pub fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn calls_with(&self, prefix: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.starts_with(prefix))
            .collect()
    }
}

#[async_trait]
impl ResourceDriver for FakeDriver {
    async fn probe(&self, resource: &Resource) -> Probe {
        let key = key(resource);
        match self.behavior(&key) {
            Behavior::Foreign => Probe::Foreign(format!("{key} is not ours")),
            _ if self.gone.lock().unwrap().contains(&key) => Probe::Gone,
            _ => Probe::Running,
        }
    }

    async fn polite_stop(&self, resource: &Resource) -> anyhow::Result<()> {
        let key = key(resource);
        self.log.lock().unwrap().push(format!("polite:{key}"));
        if matches!(
            self.behavior(&key),
            Behavior::StopsOnPolite | Behavior::FinalizeFails
        ) {
            self.gone.lock().unwrap().insert(key);
        }
        Ok(())
    }

    async fn force_stop(&self, resource: &Resource) -> anyhow::Result<()> {
        let key = key(resource);
        self.log.lock().unwrap().push(format!("force:{key}"));
        if self.behavior(&key) == Behavior::NeverStops {
            anyhow::bail!("kill refused");
        }
        self.gone.lock().unwrap().insert(key);
        Ok(())
    }

    async fn finalize(&self, resource: &Resource) -> anyhow::Result<()> {
        let key = key(resource);
        self.log.lock().unwrap().push(format!("finalize:{key}"));
        if self.behavior(&key) == Behavior::FinalizeFails {
            anyhow::bail!("stopped but not removed");
        }
        Ok(())
    }

    async fn labelled_containers(&self) -> anyhow::Result<Vec<LabelledContainer>> {
        if let Some(e) = &self.containers_error {
            anyhow::bail!(e.clone());
        }
        Ok(self.containers.clone())
    }

    async fn container_by_run_label(
        &self,
        runtime: &str,
        run_label: &str,
    ) -> anyhow::Result<Option<String>> {
        self.log
            .lock()
            .unwrap()
            .push(format!("lookup:{runtime}:{run_label}"));
        if let Some(e) = &self.run_lookup_error {
            anyhow::bail!(e.clone());
        }
        Ok(self.run_labels.lock().unwrap().get(run_label).cloned())
    }

    fn session_alive(&self, session: &SessionRecord) -> bool {
        !self.dead_sessions.contains(&session.id)
    }
}
