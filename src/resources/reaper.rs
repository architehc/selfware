//! Listing, zombie detection and reaping (`selfware resources`).
//!
//! A zombie is an unreleased, non-`keep` resource whose owner can no longer
//! drain it: its task's teardown already ran and could not confirm release
//! (`Leaked`), or its session ended without teardown (crash, kill -9). The
//! reaper only ever acts on resources selfware recorded in the registry or
//! labelled (`selfware.task` on containers); a resource of a session that is
//! still running is never a zombie.

use super::driver::{LabelledContainer, ResourceDriver};
use super::registry::ResourceRegistry;
use super::teardown::{drain, DrainReport, TeardownPolicy};
use super::{Resource, ResourceEvent, ResourceHandle, ResourceKind, ResourceState};
use serde::Serialize;
use std::collections::HashSet;

/// How a listed resource stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "reason", rename_all = "snake_case")]
pub enum EntryStatus {
    /// Owned by a running task of a live session.
    Active,
    /// `keep` / session-owned in a live session.
    Kept,
    /// `keep` resource whose session ended; not reaped unless asked.
    KeptOrphan,
    /// Owner gone; reap candidate.
    Zombie(String),
    /// Confirmed gone (only listed with `--all`).
    Released,
}

impl EntryStatus {
    pub fn label(&self) -> &'static str {
        match self {
            EntryStatus::Active => "active",
            EntryStatus::Kept => "kept",
            EntryStatus::KeptOrphan => "kept (session ended)",
            EntryStatus::Zombie(_) => "zombie",
            EntryStatus::Released => "released",
        }
    }
}

/// One row of `selfware resources`.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub resource: Resource,
    #[serde(flatten)]
    pub status: EntryStatus,
    /// Found by container label, not in the registry.
    pub discovered: bool,
}

/// `selfware resources` output.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Listing {
    pub entries: Vec<Entry>,
    /// Why container reconciliation was skipped or incomplete, if it was.
    pub container_check: Option<String>,
}

impl Listing {
    pub fn zombies(&self) -> impl Iterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|e| matches!(e.status, EntryStatus::Zombie(_)))
    }
}

fn alive_sessions(registry: &ResourceRegistry, driver: &dyn ResourceDriver) -> HashSet<String> {
    let snapshot = registry.snapshot();
    let mut alive: HashSet<String> = snapshot
        .sessions
        .iter()
        .filter(|s| driver.session_alive(s))
        .map(|s| s.id.clone())
        .collect();
    alive.insert(registry.session().id.clone());
    alive
}

/// Classify one registry entry.
pub fn classify(resource: &Resource, alive: &HashSet<String>) -> EntryStatus {
    if resource.state.is_released() {
        return EntryStatus::Released;
    }
    let session_alive = alive.contains(&resource.session);
    let session_owned = super::context::is_session_owner(&resource.owner_task);
    if resource.state == ResourceState::Leaked && !resource.keep {
        return EntryStatus::Zombie(match &resource.note {
            Some(note) => format!("teardown could not confirm release: {note}"),
            None => "teardown could not confirm release".to_string(),
        });
    }
    match (session_alive, resource.keep) {
        (true, true) => EntryStatus::Kept,
        (true, false) if session_owned => EntryStatus::Kept,
        (true, false) => EntryStatus::Active,
        (false, true) => EntryStatus::KeptOrphan,
        (false, false) => EntryStatus::Zombie("owning session ended without teardown".into()),
    }
}

fn discovered_resource(c: &LabelledContainer) -> Resource {
    let now = chrono::Utc::now();
    Resource {
        id: format!("ctr-{}", c.id.chars().take(12).collect::<String>()),
        kind: ResourceKind::Container,
        owner_task: c.task.clone(),
        owner_agent: c.agent.clone(),
        session: c.session.clone(),
        created_at: now,
        updated_at: now,
        state: if c.running {
            ResourceState::Live
        } else {
            ResourceState::Orphaned
        },
        handle: ResourceHandle::Container {
            runtime: c.runtime.clone(),
            id: c.id.clone(),
            task_label: c.task.clone(),
        },
        keep: false,
        label: if c.name.is_empty() {
            c.image.clone()
        } else {
            format!("{} {}", c.image, c.name)
        },
        note: None,
    }
}

fn same_container(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

/// List registry entries (unreleased unless `include_released`), reconciled
/// with labelled containers when `reconcile_containers` is set.
pub async fn list(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    reconcile_containers: bool,
    include_released: bool,
) -> Listing {
    let alive = alive_sessions(registry, driver);
    let snapshot = registry.snapshot();
    let mut listing = Listing::default();
    for resource in &snapshot.resources {
        if resource.state.is_released() && !include_released {
            continue;
        }
        listing.entries.push(Entry {
            status: classify(resource, &alive),
            resource: resource.clone(),
            discovered: false,
        });
    }
    if reconcile_containers {
        match driver.labelled_containers().await {
            Ok(containers) => {
                for c in containers {
                    let known = snapshot.resources.iter().any(|r| match &r.handle {
                        ResourceHandle::Container { id, .. } => same_container(id, &c.id),
                        _ => false,
                    });
                    if known {
                        continue;
                    }
                    let resource = discovered_resource(&c);
                    let status = if alive.contains(&c.session) {
                        EntryStatus::Active
                    } else {
                        EntryStatus::Zombie(
                            "selfware-labelled container not in the registry; its session ended"
                                .into(),
                        )
                    };
                    listing.entries.push(Entry {
                        resource,
                        status,
                        discovered: true,
                    });
                }
            }
            Err(e) => listing.container_check = Some(format!("{e:#}")),
        }
    }
    listing
}

/// What `selfware resources reap` did (or would do).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReapReport {
    /// Candidates selected for draining.
    pub candidates: Vec<Entry>,
    /// Zombies left alone, with why (non-drainable kinds).
    pub skipped: Vec<(Entry, String)>,
    #[serde(skip)]
    pub drained: Option<DrainReport>,
    pub dry_run: bool,
    pub container_check: Option<String>,
}

/// Drain zombies (and, with `include_kept`, kept resources of ended
/// sessions). With `dry_run` nothing is touched.
pub async fn reap(
    registry: &ResourceRegistry,
    driver: &dyn ResourceDriver,
    dry_run: bool,
    include_kept: bool,
    policy: TeardownPolicy,
) -> ReapReport {
    let listing = list(registry, driver, true, false).await;
    let mut report = ReapReport {
        dry_run,
        container_check: listing.container_check.clone(),
        ..Default::default()
    };
    for entry in listing.entries {
        let wanted = match &entry.status {
            EntryStatus::Zombie(_) => true,
            EntryStatus::KeptOrphan => include_kept,
            _ => false,
        };
        if !wanted {
            continue;
        }
        if !entry.resource.kind.is_drainable() {
            let hint = match &entry.resource.handle {
                ResourceHandle::Path { path } if entry.resource.kind == ResourceKind::Worktree => {
                    format!("remove manually: git worktree remove {}", path.display())
                }
                _ => format!(
                    "{} resources are not reaped automatically",
                    entry.resource.kind
                ),
            };
            report.skipped.push((entry, hint));
            continue;
        }
        report.candidates.push(entry);
    }
    if dry_run || report.candidates.is_empty() {
        return report;
    }
    let mut to_drain = Vec::new();
    for entry in &report.candidates {
        if entry.discovered {
            registry.adopt(entry.resource.clone());
        }
        to_drain.push(entry.resource.clone());
    }
    to_drain.reverse();
    report.drained = Some(drain(registry, driver, to_drain, policy).await);
    report
}

/// Cheap startup check (registry only, no container runtime calls): mark
/// live entries of ended sessions `Orphaned` and return one line when any
/// zombies exist. Never stops anything.
pub fn startup_report(registry: &ResourceRegistry, driver: &dyn ResourceDriver) -> Option<String> {
    let alive = alive_sessions(registry, driver);
    let mut kinds: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut total = 0usize;
    for resource in registry.unreleased() {
        let status = classify(&resource, &alive);
        if matches!(status, EntryStatus::Zombie(_) | EntryStatus::KeptOrphan) {
            reconcile_ended_session(registry, &resource);
        }
        if matches!(status, EntryStatus::Zombie(_)) {
            total += 1;
            *kinds.entry(resource.kind.as_str()).or_default() += 1;
        }
    }
    if total == 0 {
        return None;
    }
    let breakdown: Vec<String> = kinds.iter().map(|(k, n)| format!("{n} {k}")).collect();
    Some(format!(
        "resources: {total} zombie resource{} from earlier tasks ({}) — inspect with `selfware resources --zombies`, clean up with `selfware resources reap`",
        if total == 1 { "" } else { "s" },
        breakdown.join(", ")
    ))
}

/// Record what an ended session left behind, as lifecycle events: a live
/// resource lost its owner (`owner_gone` → orphaned), a drain in progress
/// can no longer finish in time (`deadline_passed` → leaked), a start that
/// was never confirmed may have half-happened (`fail` → leaked). Requested
/// (never started) and already settled resources are left as they are.
fn reconcile_ended_session(registry: &ResourceRegistry, resource: &Resource) {
    let (event, note) = match resource.state {
        ResourceState::Live => (
            ResourceEvent::OwnerGone,
            "owning session ended without teardown",
        ),
        ResourceState::Draining => (
            ResourceEvent::DeadlinePassed,
            "owning session ended while draining it",
        ),
        ResourceState::Starting => (
            ResourceEvent::Fail,
            "owning session ended before its start was confirmed",
        ),
        _ => return,
    };
    let _ = registry.transition(
        &resource.id,
        event,
        &format!("startup reconcile: {note}"),
        Some(note.to_string()),
    );
}

/// Text rendering of a listing (`selfware resources`).
pub fn render_listing(listing: &Listing, zombies_only: bool) -> String {
    let rows: Vec<&Entry> = listing
        .entries
        .iter()
        .filter(|e| !zombies_only || matches!(e.status, EntryStatus::Zombie(_)))
        .collect();
    let mut out = Vec::new();
    if rows.is_empty() {
        out.push(if zombies_only {
            "No zombie resources.".to_string()
        } else {
            "No resources recorded.".to_string()
        });
    }
    for e in &rows {
        let r = &e.resource;
        let mut line = format!(
            "{:<10} {:<22} {:<12} {:<9} owner {}  {}",
            r.kind.as_str(),
            e.status.label(),
            r.id,
            r.state.as_str(),
            r.owner_task,
            r.describe()
        );
        if e.discovered {
            line.push_str("  [found by label]");
        }
        if let EntryStatus::Zombie(reason) = &e.status {
            line.push_str(&format!("\n{:>12}{reason}", ""));
        } else if let Some(note) = &r.note {
            line.push_str(&format!("\n{:>12}{note}", ""));
        }
        out.push(line);
    }
    if let Some(why) = &listing.container_check {
        out.push(format!("note: container reconciliation incomplete — {why}"));
    }
    let zombies = listing.zombies().count();
    if zombies > 0 && !zombies_only {
        out.push(format!(
            "{zombies} zombie(s): `selfware resources reap --dry-run` to preview cleanup"
        ));
    }
    out.join("\n")
}

/// Text rendering of a reap (`selfware resources reap`).
pub fn render_reap(report: &ReapReport) -> String {
    let mut out = Vec::new();
    if report.candidates.is_empty() {
        out.push("Nothing to reap.".to_string());
    } else if report.dry_run {
        out.push(format!("Would drain {}:", report.candidates.len()));
        for e in &report.candidates {
            out.push(format!(
                "  {} {} ({})",
                e.resource.kind,
                e.resource.describe(),
                e.resource.id
            ));
        }
    }
    if let Some(drained) = &report.drained {
        for r in &drained.released {
            out.push(format!("  released {} {}", r.kind, r.describe()));
        }
        for r in &drained.leaked {
            out.push(format!(
                "  LEAKED   {} {} — {}",
                r.kind,
                r.describe(),
                r.note.as_deref().unwrap_or("release not confirmed")
            ));
        }
        out.push(format!(
            "reap: {} released, {} leaked",
            drained.released.len(),
            drained.leaked.len()
        ));
    }
    for (e, why) in &report.skipped {
        out.push(format!(
            "  skipped {} {} — {why}",
            e.resource.kind,
            e.resource.describe()
        ));
    }
    if let Some(why) = &report.container_check {
        out.push(format!("note: container reconciliation incomplete — {why}"));
    }
    out.join("\n")
}

#[cfg(test)]
#[path = "../../tests/unit/resources/reaper_test.rs"]
mod tests;
