use super::*;
use crate::resources::fake::{Behavior, FakeDriver};
use crate::resources::registry::{NewResource, SessionRecord};
use std::time::Duration;

fn fast() -> TeardownPolicy {
    TeardownPolicy {
        deadline: Duration::from_millis(40),
        force_grace: Duration::from_millis(40),
        poll: Duration::from_millis(5),
    }
}

fn session(id: &str) -> SessionRecord {
    SessionRecord {
        id: id.into(),
        pid: 2_147_483_646,
        start_time: Some(1),
        started_at: chrono::Utc::now(),
    }
}

fn process(pid: u32) -> NewResource {
    NewResource::new(
        ResourceKind::Process,
        ResourceHandle::Process {
            pid,
            pgid: Some(pid),
            start_time: Some(1),
            managed_id: None,
        },
        format!("proc {pid}"),
    )
}

fn labelled(id: &str, task: &str, session: &str) -> LabelledContainer {
    LabelledContainer {
        runtime: "docker".into(),
        id: id.into(),
        task: task.into(),
        session: session.into(),
        agent: None,
        running: true,
        name: "web".into(),
        image: "nginx".into(),
    }
}

/// Two sessions sharing one registry file: `dead` crashed, `me` is running.
fn shared(dir: &tempfile::TempDir) -> (ResourceRegistry, ResourceRegistry, FakeDriver) {
    let path = dir.path().join("resources.json");
    let dead = ResourceRegistry::at_path_as(&path, session("dead"));
    let me = ResourceRegistry::at_path_as(&path, session("me"));
    let mut driver = FakeDriver::new();
    driver.dead_sessions.insert("dead".into());
    (dead, me, driver)
}

#[tokio::test]
async fn resources_of_a_crashed_session_are_zombies_and_get_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    let id = dead.register_owned(process(50), "old-task".into(), None);

    let listing = list(&me, &driver, true, false).await;
    assert_eq!(listing.zombies().count(), 1);
    assert_eq!(listing.entries[0].resource.id, id);

    let report = reap(&me, &driver, false, false, fast()).await;
    let drained = report.drained.unwrap();
    assert_eq!(drained.released.len(), 1);
    assert_eq!(driver.calls_with("polite:"), vec!["polite:pid 50"]);
    // Persisted: a third reader sees it released.
    let reader = ResourceRegistry::at_path_as(dir.path().join("resources.json"), session("x"));
    assert!(reader.get(&id).unwrap().state.is_released());
}

#[tokio::test]
async fn a_live_sessions_resources_are_never_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("resources.json");
    let other = ResourceRegistry::at_path_as(&path, session("other-live"));
    let me = ResourceRegistry::at_path_as(&path, session("me"));
    let driver = FakeDriver::new(); // every session alive
    let id = other.register_owned(process(60), "running-task".into(), None);

    let listing = list(&me, &driver, true, false).await;
    assert_eq!(listing.entries[0].status, EntryStatus::Active);
    let report = reap(&me, &driver, false, true, fast()).await;
    assert!(report.candidates.is_empty());
    assert!(driver.calls().is_empty());
    assert_eq!(me.get(&id).unwrap().state, ResourceState::Live);
}

#[tokio::test]
async fn leaked_resources_of_a_live_session_are_zombies() {
    let reg = ResourceRegistry::in_memory_as(session("me"));
    let driver = FakeDriver::new();
    let id = reg.register_owned(process(61), "t".into(), None);
    reg.set_state(&id, ResourceState::Leaked, Some("still running".into()));
    let listing = list(&reg, &driver, false, false).await;
    assert!(
        matches!(listing.entries[0].status, EntryStatus::Zombie(ref r) if r.contains("still running"))
    );
}

#[tokio::test]
async fn labelled_container_of_an_ended_session_is_reaped_and_adopted() {
    let dir = tempfile::tempdir().unwrap();
    let (_dead, me, mut driver) = shared(&dir);
    driver.containers = vec![
        labelled("c0ffee000001", "old-task", "dead"),
        labelled("c0ffee000002", "live-task", "me"),
    ];

    let listing = list(&me, &driver, true, false).await;
    let statuses: Vec<_> = listing.entries.iter().map(|e| e.status.clone()).collect();
    assert!(matches!(statuses[0], EntryStatus::Zombie(_)));
    assert_eq!(statuses[1], EntryStatus::Active);
    assert!(listing.entries.iter().all(|e| e.discovered));

    let report = reap(&me, &driver, false, false, fast()).await;
    assert_eq!(
        driver.calls_with("polite:"),
        vec!["polite:container c0ffee000001"],
        "the live session's container is untouched"
    );
    let released = &report.drained.unwrap().released;
    assert_eq!(released.len(), 1);
    // Adopted into the registry with its outcome recorded.
    assert!(me.get(&released[0].id).unwrap().state.is_released());
}

#[tokio::test]
async fn a_registered_container_is_not_double_listed_from_labels() {
    let reg = ResourceRegistry::in_memory_as(session("me"));
    let mut driver = FakeDriver::new();
    reg.register_owned(
        NewResource::new(
            ResourceKind::Container,
            ResourceHandle::Container {
                runtime: "docker".into(),
                id: "abcdef0123456789".into(),
                task_label: "t".into(),
            },
            "nginx",
        ),
        "t".into(),
        None,
    );
    driver.containers = vec![labelled("abcdef0123456789", "t", "me")];
    let listing = list(&reg, &driver, true, false).await;
    assert_eq!(listing.entries.len(), 1);
    assert!(!listing.entries[0].discovered);
}

#[tokio::test]
async fn foreign_resources_are_not_touched_by_reap() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    // The recorded pid now belongs to someone else.
    driver.script("pid 70", Behavior::Foreign);
    let id = dead.register_owned(process(70), "old".into(), None);

    let report = reap(&me, &driver, false, false, fast()).await;
    assert!(driver.calls_with("polite:").is_empty());
    assert!(driver.calls_with("force:").is_empty());
    assert_eq!(report.drained.unwrap().leaked.len(), 1);
    assert_eq!(me.get(&id).unwrap().state, ResourceState::Leaked);
}

#[tokio::test]
async fn dry_run_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    let id = dead.register_owned(process(80), "old".into(), None);
    let report = reap(&me, &driver, true, false, fast()).await;
    assert_eq!(report.candidates.len(), 1);
    assert!(report.drained.is_none());
    assert!(driver.calls().is_empty());
    assert_eq!(me.get(&id).unwrap().state, ResourceState::Live);
    assert!(render_reap(&report).contains("Would drain 1"));
}

#[tokio::test]
async fn kept_resources_of_an_ended_session_need_include_kept() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    dead.register_owned(process(90).keep(true), "session:dead".into(), None);

    let listing = list(&me, &driver, false, false).await;
    assert_eq!(listing.entries[0].status, EntryStatus::KeptOrphan);
    assert!(reap(&me, &driver, false, false, fast())
        .await
        .candidates
        .is_empty());
    let report = reap(&me, &driver, false, true, fast()).await;
    assert_eq!(report.drained.unwrap().released.len(), 1);
}

#[tokio::test]
async fn worktree_zombies_are_skipped_with_a_manual_hint() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    dead.register_owned(
        NewResource::new(
            ResourceKind::Worktree,
            ResourceHandle::Path {
                path: "/repo/.selfware/worktrees/x".into(),
            },
            "wt",
        ),
        "old".into(),
        None,
    );
    let report = reap(&me, &driver, false, false, fast()).await;
    assert!(report.candidates.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert!(report.skipped[0].1.contains("git worktree remove"));
    assert!(driver.calls().is_empty());
}

#[tokio::test]
async fn container_runtime_errors_are_reported_not_hidden() {
    let reg = ResourceRegistry::in_memory_as(session("me"));
    let mut driver = FakeDriver::new();
    driver.containers_error = Some("docker daemon not running".into());
    let listing = list(&reg, &driver, true, false).await;
    assert!(listing
        .container_check
        .as_deref()
        .unwrap()
        .contains("docker daemon"));
    assert!(render_listing(&listing, false).contains("reconciliation incomplete"));
}

#[test]
fn startup_report_marks_orphans_and_reports_one_line_without_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let (dead, me, driver) = shared(&dir);
    let a = dead.register_owned(process(100), "old".into(), None);
    dead.register_owned(process(101).keep(true), "session:dead".into(), None);

    let line = startup_report(&me, &driver).unwrap();
    assert!(
        line.starts_with("resources: 1 zombie resource from earlier tasks (1 process)"),
        "{line}"
    );
    assert!(driver.calls().is_empty(), "startup never stops anything");
    assert_eq!(me.get(&a).unwrap().state, ResourceState::Orphaned);
}

#[test]
fn startup_report_is_silent_when_clean() {
    let reg = ResourceRegistry::in_memory_as(session("me"));
    let driver = FakeDriver::new();
    reg.register_owned(process(110), "running".into(), None);
    assert!(startup_report(&reg, &driver).is_none());
}
