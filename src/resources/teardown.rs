//! Draining resources: polite stop, deadline, force, confirm.
//!
//! [`teardown_task`] runs when a task reaches a terminal state. It drains
//! every unreleased, non-`keep` resource the task owns in reverse creation
//! order; `keep` resources are re-owned by the session and left running.
//! Each drain: polite stop → poll until gone or the deadline → force stop →
//! poll a short grace → finalize (e.g. remove the stopped container). Only a
//! confirmed-gone resource becomes [`ResourceState::Released`]; everything
//! else becomes [`ResourceState::Leaked`] with the reason in `note`.

use super::context::session_owner;
use super::driver::{Probe, ResourceDriver};
use super::registry::ResourceRegistry;
use super::{Resource, ResourceState};
use std::time::Duration;

/// Timing knobs for a drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeardownPolicy {
    /// How long a polite stop gets before it is forced
    /// (`resources.teardown_deadline_secs`, default 10 s).
    pub deadline: Duration,
    /// How long a forced stop gets to take effect before the resource is
    /// declared leaked.
    pub force_grace: Duration,
    /// Probe interval while waiting.
    pub poll: Duration,
}

impl Default for TeardownPolicy {
    fn default() -> Self {
        Self::with_deadline(Duration::from_secs(10))
    }
}

impl TeardownPolicy {
    pub fn with_deadline(deadline: Duration) -> Self {
        Self {
            deadline,
            force_grace: Duration::from_secs(3),
            poll: Duration::from_millis(100),
        }
    }
}

/// Outcome of draining a set of resources.
#[derive(Debug, Clone, Default)]
pub struct DrainReport {
    /// Confirmed gone.
    pub released: Vec<Resource>,
    /// Could not be confirmed gone (reason in each `note`).
    pub leaked: Vec<Resource>,
    /// `keep` resources handed to the session instead of drained.
    pub kept: Vec<Resource>,
}

impl DrainReport {
    pub fn is_empty(&self) -> bool {
        self.released.is_empty() && self.leaked.is_empty() && self.kept.is_empty()
    }

    /// The run-summary line, or `None` when the task owned nothing.
    pub fn summary_line(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut line = format!(
            "resources: {} released, {} leaked",
            self.released.len(),
            self.leaked.len()
        );
        if !self.leaked.is_empty() {
            let detail: Vec<String> = self
                .leaked
                .iter()
                .map(|r| match &r.note {
                    Some(note) => format!("{}: {note}", r.handle.short()),
                    None => r.handle.short(),
                })
                .collect();
            line.push_str(&format!(" ({})", detail.join("; ")));
        }
        if !self.kept.is_empty() {
            line.push_str(&format!(
                ", {} kept running (see `selfware resources`)",
                self.kept.len()
            ));
        }
        Some(line)
    }
}

/// Drain everything `task` owns (see the module docs).
pub async fn teardown_task(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    task: &str,
    policy: TeardownPolicy,
) -> DrainReport {
    let owned = registry.owned_by(task);
    if owned.is_empty() {
        return DrainReport::default();
    }
    let mut report = DrainReport::default();
    let mut to_drain = Vec::new();
    for resource in owned {
        if resource.keep {
            registry.reown(&resource.id, session_owner());
            report
                .kept
                .push(registry.get(&resource.id).unwrap_or(resource));
        } else {
            to_drain.push(resource);
        }
    }
    to_drain.reverse();
    let drained = drain(registry, driver, to_drain, policy).await;
    report.released = drained.released;
    report.leaked = drained.leaked;
    report
}

/// Drain `resources` in the given order.
pub async fn drain(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    resources: Vec<Resource>,
    policy: TeardownPolicy,
) -> DrainReport {
    let mut report = DrainReport::default();
    for resource in resources {
        let (state, note) = drain_one(registry, driver, &resource, policy).await;
        registry.set_state(&resource.id, state, note.clone());
        let mut updated = registry.get(&resource.id).unwrap_or(resource);
        updated.state = state;
        updated.note = note;
        if state.is_released() {
            report.released.push(updated);
        } else {
            report.leaked.push(updated);
        }
    }
    report
}

async fn drain_one(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    resource: &Resource,
    policy: TeardownPolicy,
) -> (ResourceState, Option<String>) {
    if !resource.kind.is_drainable() {
        return (
            ResourceState::Leaked,
            Some(format!(
                "{} resources are not stopped automatically",
                resource.kind
            )),
        );
    }
    registry.set_state(&resource.id, ResourceState::Draining, None);

    match driver.probe(resource).await {
        Probe::Gone => return finalize(driver, resource).await,
        Probe::Foreign(why) | Probe::Unknown(why) => return (ResourceState::Leaked, Some(why)),
        Probe::Running => {}
    }

    let mut stop_error = driver.polite_stop(resource).await.err();
    match wait_gone(driver, resource, policy.deadline, policy.poll).await {
        Probe::Gone => return finalize(driver, resource).await,
        Probe::Foreign(why) => return (ResourceState::Leaked, Some(why)),
        Probe::Running | Probe::Unknown(_) => {}
    }

    if let Err(e) = driver.force_stop(resource).await {
        stop_error = Some(e);
    }
    match wait_gone(driver, resource, policy.force_grace, policy.poll).await {
        Probe::Gone => finalize(driver, resource).await,
        Probe::Foreign(why) | Probe::Unknown(why) => (ResourceState::Leaked, Some(why)),
        Probe::Running => {
            let mut note = format!(
                "still running after {}s polite stop + force",
                policy.deadline.as_secs_f32()
            );
            if let Some(e) = stop_error {
                note.push_str(&format!(" ({e:#})"));
            }
            (ResourceState::Leaked, Some(note))
        }
    }
}

async fn finalize(
    driver: &dyn ResourceDriver,
    resource: &Resource,
) -> (ResourceState, Option<String>) {
    match driver.finalize(resource).await {
        Ok(()) => (ResourceState::Released, None),
        Err(e) => (ResourceState::Leaked, Some(format!("{e:#}"))),
    }
}

/// Poll until the resource is gone/foreign or `limit` elapses; returns the
/// last probe.
async fn wait_gone(
    driver: &dyn ResourceDriver,
    resource: &Resource,
    limit: Duration,
    poll: Duration,
) -> Probe {
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let probe = driver.probe(resource).await;
        if matches!(probe, Probe::Gone | Probe::Foreign(_)) {
            return probe;
        }
        if tokio::time::Instant::now() >= deadline {
            return probe;
        }
        tokio::time::sleep(poll).await;
    }
}

/// Drain everything this session owns (session end: REPL exit). Includes
/// `keep` resources — the session that kept them is going away.
pub async fn teardown_session(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    policy: TeardownPolicy,
) -> DrainReport {
    let session = registry.session().id.clone();
    let mut owned: Vec<Resource> = registry
        .unreleased()
        .into_iter()
        .filter(|r| r.session == session && r.kind.is_drainable())
        .collect();
    owned.reverse();
    drain(registry, driver, owned, policy).await
}

#[cfg(test)]
#[path = "../../tests/unit/resources/teardown_test.rs"]
mod tests;
