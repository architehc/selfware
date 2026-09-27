//! Draining resources: polite stop, deadline, force, confirm.
//!
//! [`teardown_task`] runs when a task reaches a terminal state. It drains
//! every unreleased, non-`keep` resource the task owns in reverse creation
//! order; `keep` resources are re-owned by the session and left running.
//! Each drain: polite stop → poll until gone or the deadline → force stop →
//! poll a short grace → finalize (e.g. remove the stopped container). Only a
//! confirmed-gone resource becomes [`ResourceState::Released`]; everything
//! else becomes [`ResourceState::Leaked`] with the reason in `note`.
//!
//! Each step is a lifecycle event applied through the registry
//! (`drain`/`reap` into `draining`, then `stopped`, `deadline_passed` or
//! `abandon`), so the drain is recorded in the event log and cannot take a
//! transition the proved resource table refuses.

use super::context::session_owner;
use super::driver::{Probe, ResourceDriver};
use super::registry::ResourceRegistry;
use super::{Resource, ResourceEvent, ResourceState};
use crate::lifecycle::Effect;
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
    /// Ids of the resources whose drain raised [`Effect::LeakAlarm`] (they
    /// entered `leaked` during this drain).
    pub leak_alarms: Vec<String>,
}

impl DrainReport {
    pub fn is_empty(&self) -> bool {
        self.released.is_empty() && self.leaked.is_empty() && self.kept.is_empty()
    }

    /// `(released, leaked, kept)` counts.
    pub fn counts(&self) -> (usize, usize, usize) {
        (self.released.len(), self.leaked.len(), self.kept.len())
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

/// A task's teardown outcome as reported outside the process (the headless
/// JSON / stream-json result): counts plus the run-summary line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TeardownOutcome {
    /// Confirmed gone.
    pub released: usize,
    /// Not confirmed gone (a `LeakAlarm` each).
    pub leaked: usize,
    /// `keep` resources handed to the session, still running.
    pub kept: usize,
    /// The run-summary line (`resources: N released, M leaked (…)`).
    pub summary: String,
}

impl DrainReport {
    /// The outcome to report, or `None` when the task owned nothing.
    pub fn outcome(&self) -> Option<TeardownOutcome> {
        let summary = self.summary_line()?;
        let (released, leaked, kept) = self.counts();
        Some(TeardownOutcome {
            released,
            leaked,
            kept,
            summary,
        })
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
    report.leak_alarms = drained.leak_alarms;
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
        let alarm = drain_tracked(registry, driver, &resource, policy).await;
        let mut updated = registry.get(&resource.id).unwrap_or(resource);
        if alarm {
            report.leak_alarms.push(updated.id.clone());
        }
        if updated.state.is_released() {
            report.released.push(updated);
        } else {
            if updated.state != ResourceState::Leaked && updated.note.is_none() {
                updated.note = Some(format!("release not confirmed (state {})", updated.state));
            }
            report.leaked.push(updated);
        }
    }
    report
}

/// Take one resource through `draining` to a settled state. Returns whether
/// entering `leaked` raised [`Effect::LeakAlarm`].
async fn drain_tracked(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    resource: &Resource,
    policy: TeardownPolicy,
) -> bool {
    let id = resource.id.as_str();
    // The registry's current state, not the caller's snapshot: a tool may
    // have released it in the meantime.
    let state = registry.get(id).map_or(resource.state, |r| r.state);
    let enter = match state {
        ResourceState::Released => return false,
        // Never started: nothing to stop (R4 allows exactly this release).
        ResourceState::Requested => {
            let _ = registry.transition(id, ResourceEvent::Drain, "drain: never started", None);
            return false;
        }
        ResourceState::Draining => None,
        ResourceState::Orphaned | ResourceState::Leaked => Some(ResourceEvent::Reap),
        ResourceState::Starting | ResourceState::Live => Some(ResourceEvent::Drain),
    };
    if let Some(event) = enter {
        let cause = match event {
            ResourceEvent::Reap => "reaper: draining",
            _ => "teardown: draining",
        };
        if registry.transition(id, event, cause, None).is_err() {
            // Refused (logged by the registry): the state is unchanged and
            // the caller reports it as not released.
            return false;
        }
    }
    let (event, note) = drain_one(driver, resource, policy).await;
    let cause = match (&event, &note) {
        (ResourceEvent::Stopped, _) => "teardown: confirmed gone".to_string(),
        (_, Some(note)) => format!("teardown: {note}"),
        (_, None) => "teardown: release not confirmed".to_string(),
    };
    registry
        .transition(id, event, &cause, note)
        .is_ok_and(|effects| effects.contains(&Effect::LeakAlarm))
}

/// Drive the host side of a drain for a resource already in `draining` and
/// return the settling event: `stopped` (confirmed gone), `deadline_passed`
/// (still running after polite stop + force) or `abandon` (gave up: foreign
/// or unknown handle, kind not stopped automatically, finalize failed).
async fn drain_one(
    driver: &dyn ResourceDriver,
    resource: &Resource,
    policy: TeardownPolicy,
) -> (ResourceEvent, Option<String>) {
    if !resource.kind.is_drainable() {
        return (
            ResourceEvent::Abandon,
            Some(format!(
                "{} resources are not stopped automatically",
                resource.kind
            )),
        );
    }

    match driver.probe(resource).await {
        Probe::Gone => return finalize(driver, resource).await,
        Probe::Foreign(why) | Probe::Unknown(why) => return (ResourceEvent::Abandon, Some(why)),
        Probe::Running => {}
    }

    let mut stop_error = driver.polite_stop(resource).await.err();
    match wait_gone(driver, resource, policy.deadline, policy.poll).await {
        Probe::Gone => return finalize(driver, resource).await,
        Probe::Foreign(why) => return (ResourceEvent::Abandon, Some(why)),
        Probe::Running | Probe::Unknown(_) => {}
    }

    if let Err(e) = driver.force_stop(resource).await {
        stop_error = Some(e);
    }
    match wait_gone(driver, resource, policy.force_grace, policy.poll).await {
        Probe::Gone => finalize(driver, resource).await,
        Probe::Foreign(why) => (ResourceEvent::Abandon, Some(why)),
        Probe::Unknown(why) => (ResourceEvent::DeadlinePassed, Some(why)),
        Probe::Running => {
            let mut note = format!(
                "still running after {}s polite stop + force",
                policy.deadline.as_secs_f32()
            );
            if let Some(e) = stop_error {
                note.push_str(&format!(" ({e:#})"));
            }
            (ResourceEvent::DeadlinePassed, Some(note))
        }
    }
}

async fn finalize(
    driver: &dyn ResourceDriver,
    resource: &Resource,
) -> (ResourceEvent, Option<String>) {
    match driver.finalize(resource).await {
        Ok(()) => (ResourceEvent::Stopped, None),
        Err(e) => (ResourceEvent::Abandon, Some(format!("{e:#}"))),
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

static SESSION_TEARDOWN_DEADLINE: std::sync::OnceLock<Duration> = std::sync::OnceLock::new();

/// Record the configured teardown deadline (`resources.teardown_deadline_secs`)
/// for the process-exit drain, which runs after the configuration is gone.
/// The first call wins.
pub fn configure_session_deadline(deadline: Duration) {
    let _ = SESSION_TEARDOWN_DEADLINE.set(deadline);
}

/// The policy for the session-end drain: the configured deadline, or the
/// default when none was configured.
pub fn session_policy() -> TeardownPolicy {
    SESSION_TEARDOWN_DEADLINE
        .get()
        .map_or_else(TeardownPolicy::default, |d| {
            TeardownPolicy::with_deadline(*d)
        })
}

/// Session end: drain everything this session still owns and return the
/// summary line, `None` when nothing was left. Idempotent — the REPL/TUI
/// drain on their own exit and the process-exit drain in `main` then finds
/// nothing, so a summary is printed once.
pub async fn end_session(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    policy: TeardownPolicy,
) -> Option<String> {
    teardown_session(registry, driver, policy)
        .await
        .summary_line()
}

/// [`end_session`] on the process-wide registry with the system driver and
/// the configured deadline. Called on every non-forced process exit
/// (headless runs, subcommands, REPL/TUI errors).
pub async fn end_process_session() -> Option<String> {
    end_session(
        ResourceRegistry::global(),
        &super::SystemDriver::default(),
        session_policy(),
    )
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/resources/teardown_test.rs"]
mod tests;
