use super::*;
use crate::resources::fake::{Behavior, FakeDriver};
use crate::resources::registry::NewResource;
use crate::resources::{ResourceHandle, ResourceKind};

fn fast() -> TeardownPolicy {
    TeardownPolicy {
        deadline: Duration::from_millis(40),
        force_grace: Duration::from_millis(40),
        poll: Duration::from_millis(5),
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

fn container(id: &str) -> NewResource {
    NewResource::new(
        ResourceKind::Container,
        ResourceHandle::Container {
            runtime: "docker".into(),
            id: id.into(),
            task_label: "t1".into(),
            run_label: None,
        },
        "nginx",
    )
}

fn register(reg: &ResourceRegistry, new: NewResource, task: &str) -> String {
    reg.register_owned(new, task.to_string(), None)
}

#[tokio::test]
async fn drains_in_reverse_creation_order_and_releases() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let a = register(&reg, process(101), "t1");
    let b = register(&reg, container("aaaa"), "t1");
    let c = register(&reg, process(103), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(
        driver.calls_with("polite:"),
        vec!["polite:pid 103", "polite:container aaaa", "polite:pid 101"],
        "last created is stopped first"
    );
    assert_eq!(report.released.len(), 3);
    assert!(report.leaked.is_empty());
    for id in [a, b, c] {
        assert_eq!(reg.get(&id).unwrap().state, ResourceState::Released);
    }
    assert_eq!(
        report.summary_line().as_deref(),
        Some("resources: 3 released, 0 leaked")
    );
}

#[tokio::test]
async fn polite_stop_ignored_is_forced_after_deadline() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    driver.script("pid 7", Behavior::StopsOnForce);
    let id = register(&reg, process(7), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(
        driver.calls(),
        vec!["polite:pid 7", "force:pid 7", "finalize:pid 7"]
    );
    assert_eq!(report.released.len(), 1);
    assert_eq!(reg.get(&id).unwrap().state, ResourceState::Released);
}

#[tokio::test]
async fn still_running_after_deadline_and_force_is_leaked_not_released() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    driver.script("pid 9", Behavior::NeverStops);
    let id = register(&reg, process(9), "t1");

    let started = std::time::Instant::now();
    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    // The deadline was honoured before forcing, and the force grace after.
    assert!(started.elapsed() >= Duration::from_millis(80));
    assert!(report.released.is_empty());
    assert_eq!(report.leaked.len(), 1);
    let entry = reg.get(&id).unwrap();
    assert_eq!(entry.state, ResourceState::Leaked);
    let note = entry.note.unwrap();
    assert!(note.contains("still running"), "{note}");
    assert!(note.contains("kill refused"), "{note}");
    let line = report.summary_line().unwrap();
    assert!(
        line.starts_with("resources: 0 released, 1 leaked (pid 9: still running"),
        "{line}"
    );
    // Never finalized: release was not confirmed.
    assert!(driver.calls_with("finalize:").is_empty());
}

#[tokio::test]
async fn finalize_failure_is_leaked() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    driver.script("container bbbb", Behavior::FinalizeFails);
    let id = register(&reg, container("bbbb"), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(report.leaked.len(), 1);
    assert_eq!(reg.get(&id).unwrap().state, ResourceState::Leaked);
    assert!(reg
        .get(&id)
        .unwrap()
        .note
        .unwrap()
        .contains("stopped but not removed"));
}

#[tokio::test]
async fn foreign_handle_is_never_stopped() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    driver.script("pid 11", Behavior::Foreign);
    let id = register(&reg, process(11), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert!(driver.calls_with("polite:").is_empty());
    assert!(driver.calls_with("force:").is_empty());
    assert_eq!(report.leaked.len(), 1);
    assert_eq!(reg.get(&id).unwrap().state, ResourceState::Leaked);
}

#[tokio::test]
async fn already_gone_is_released_without_signals() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    driver.script("pid 12", Behavior::AlreadyGone);
    register(&reg, process(12), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(driver.calls(), vec!["finalize:pid 12"]);
    assert_eq!(report.released.len(), 1);
}

#[tokio::test]
async fn keep_resources_are_reowned_by_the_session_not_drained() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let kept = register(&reg, process(20).keep(true), "t1");
    let dropped = register(&reg, process(21), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(driver.calls_with("polite:"), vec!["polite:pid 21"]);
    assert_eq!(report.kept.len(), 1);
    let kept = reg.get(&kept).unwrap();
    assert_eq!(kept.state, ResourceState::Live);
    assert_eq!(kept.owner_task, session_owner());
    assert_eq!(reg.get(&dropped).unwrap().state, ResourceState::Released);
    let line = report.summary_line().unwrap();
    assert!(line.contains("1 kept running"), "{line}");
    // A second task end does not touch the kept resource either.
    let again = teardown_task(&reg, &driver, "t1", fast()).await;
    assert!(again.is_empty());
}

#[tokio::test]
async fn only_the_finished_tasks_resources_are_drained() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let other = register(&reg, process(30), "t2");
    register(&reg, process(31), "t1");

    teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(driver.calls_with("polite:"), vec!["polite:pid 31"]);
    assert_eq!(reg.get(&other).unwrap().state, ResourceState::Live);
}

#[tokio::test]
async fn a_task_that_owned_nothing_gets_no_summary_line_and_no_host_calls() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let report = teardown_task(&reg, &driver, "t1", fast()).await;
    assert!(report.summary_line().is_none());
    assert!(driver.calls().is_empty());
}

#[tokio::test]
async fn non_drainable_kinds_are_never_auto_drained() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let wt = register(
        &reg,
        NewResource::new(
            ResourceKind::Worktree,
            ResourceHandle::Path {
                path: "/tmp/wt".into(),
            },
            "wt",
        ),
        "t1",
    );
    let report = teardown_task(&reg, &driver, "t1", fast()).await;
    assert!(driver.calls().is_empty());
    assert_eq!(report.leaked.len(), 1);
    assert_eq!(reg.get(&wt).unwrap().state, ResourceState::Leaked);
}

#[tokio::test]
async fn session_teardown_drains_kept_resources_of_this_session() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let kept = register(&reg, process(40).keep(true), &session_owner());
    let report = teardown_session(&reg, &driver, fast()).await;
    assert_eq!(report.released.len(), 1);
    assert_eq!(reg.get(&kept).unwrap().state, ResourceState::Released);
}

#[tokio::test]
async fn a_drain_is_recorded_event_by_event_and_a_leak_raises_the_alarm() {
    let dir = tempfile::tempdir().unwrap();
    let log = crate::lifecycle::EventLog::at(dir.path().join("events.jsonl"));
    let reg = ResourceRegistry::in_memory().with_event_log(log.clone());
    let driver = FakeDriver::new();
    driver.script("pid 11", Behavior::NeverStops);
    driver.script("container ffff", Behavior::Foreign);
    let stuck = register(&reg, process(11), "t1");
    let foreign = register(&reg, container("ffff"), "t1");
    let fine = register(&reg, process(12), "t1");

    let report = teardown_task(&reg, &driver, "t1", fast()).await;

    assert_eq!(report.counts(), (1, 2, 0));
    let mut alarms = report.leak_alarms.clone();
    alarms.sort();
    let mut expected = vec![stuck.clone(), foreign.clone()];
    expected.sort();
    assert_eq!(
        alarms, expected,
        "every resource that leaked raised LeakAlarm"
    );

    let (records, _) = log.read_all();
    let path = |id: &str| -> Vec<String> {
        records
            .iter()
            .filter(|r| r.id == id)
            .map(|r| format!("{}:{}", r.event.as_deref().unwrap_or("new"), r.to))
            .collect()
    };
    assert_eq!(
        path(&stuck),
        vec!["new:live", "drain:draining", "deadline_passed:leaked"]
    );
    assert_eq!(
        path(&foreign),
        vec!["new:live", "drain:draining", "abandon:leaked"]
    );
    assert_eq!(
        path(&fine),
        vec!["new:live", "drain:draining", "stopped:released"]
    );
    assert!(records.iter().all(|r| r.owner.as_deref() == Some("t1")));
}

#[tokio::test]
async fn a_resource_released_by_its_tool_during_teardown_is_not_touched() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let id = register(&reg, process(13), "t1");
    let snapshot = reg.owned_by("t1");
    assert!(reg.release(&id, "tool: reaped its child"));

    let report = drain(&reg, &driver, snapshot, fast()).await;

    assert_eq!(report.counts(), (1, 0, 0));
    assert!(driver.calls().is_empty(), "nothing signalled");
    assert!(report.leak_alarms.is_empty());
}

#[tokio::test]
async fn session_end_drains_once_and_is_silent_the_second_time() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    register(&reg, process(50), &session_owner());
    register(&reg, process(51).keep(true), &session_owner());
    // A task's resource left behind by a run that never drained it.
    register(&reg, process(52), "task-that-crashed");

    let first = end_session(&reg, &driver, fast()).await;
    assert_eq!(first.as_deref(), Some("resources: 3 released, 0 leaked"));
    // The REPL drained on its way out; the process-exit drain finds nothing.
    assert_eq!(end_session(&reg, &driver, fast()).await, None);
}

#[test]
fn the_session_policy_uses_the_configured_deadline() {
    // The only caller in the test process: `cli::run` sets it in production.
    configure_session_deadline(Duration::from_secs(4));
    assert_eq!(session_policy().deadline, Duration::from_secs(4));
    // First call wins: a later configuration does not move it mid-session.
    configure_session_deadline(Duration::from_secs(9));
    assert_eq!(session_policy().deadline, Duration::from_secs(4));
}

fn pending_container(run: &str) -> NewResource {
    let mut new = NewResource::new(
        ResourceKind::Container,
        ResourceHandle::Container {
            runtime: "docker".into(),
            id: String::new(),
            task_label: "t1".into(),
            run_label: Some(run.into()),
        },
        "alpine",
    );
    new.state = ResourceState::Starting;
    new
}

/// A container_run cancelled before its id was read, whose container was
/// never created: the runtime answers "none", so the entry is released —
/// no stop is attempted.
#[tokio::test]
async fn pending_container_with_no_container_is_released() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let id = register(&reg, pending_container("run1"), "t1");
    let report = teardown_task(&reg, &driver, "t1", fast()).await;
    assert_eq!(report.released.len(), 1);
    assert_eq!(driver.calls(), vec!["lookup:docker:run1"]);
    let r = reg.get(&id).unwrap();
    assert_eq!(r.state, ResourceState::Released);
    assert!(r.note.unwrap().contains("no container carries"));
}

/// The label lookup fails (runtime down): leaked with the reason, never
/// released.
#[tokio::test]
async fn pending_container_whose_lookup_fails_is_leaked() {
    let reg = ResourceRegistry::in_memory();
    let mut driver = FakeDriver::new();
    driver.run_lookup_error = Some("docker daemon not reachable".into());
    let id = register(&reg, pending_container("run2"), "t1");
    let report = teardown_task(&reg, &driver, "t1", fast()).await;
    assert_eq!(report.leaked.len(), 1);
    assert_eq!(report.leak_alarms, vec![id.clone()]);
    let r = reg.get(&id).unwrap();
    assert_eq!(r.state, ResourceState::Leaked);
    assert!(r.note.unwrap().contains("docker daemon not reachable"));
}

// ---------------------------------------------------------------------------
// Bounded session drain (idle-REPL SIGTERM)
// ---------------------------------------------------------------------------

fn slow() -> TeardownPolicy {
    // A configured deadline far longer than the budget: the bound must win.
    TeardownPolicy {
        deadline: Duration::from_secs(10),
        force_grace: Duration::from_secs(3),
        poll: Duration::from_millis(5),
    }
}

#[tokio::test]
async fn bounded_session_drain_with_room_releases_everything() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let a = register(&reg, process(51), &session_owner());
    let b = register(&reg, process(52).keep(true), &session_owner());
    let report = teardown_session_within(&reg, &driver, fast(), Duration::from_secs(5)).await;
    assert_eq!(report.released.len(), 2);
    assert!(report.undrained.is_empty() && report.leaked.is_empty());
    for id in [a, b] {
        assert_eq!(reg.get(&id).unwrap().state, ResourceState::Released);
    }
    assert_eq!(
        report.summary_line().as_deref(),
        Some("resources: 2 released, 0 leaked")
    );
}

#[tokio::test]
async fn bounded_session_drain_stops_at_the_budget_and_reports_the_rest_undrained() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let first = register(&reg, process(61), &session_owner());
    driver.script("pid 62", Behavior::NeverStops);
    let stuck = register(&reg, process(62), &session_owner());

    let budget = Duration::from_millis(300);
    let started = std::time::Instant::now();
    let report = teardown_session_within(&reg, &driver, slow(), budget).await;
    let elapsed = started.elapsed();

    // The 10 s configured deadline did not apply: the budget bounds the drain.
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    // The stuck one (drained first: reverse creation order) is never
    // reported released; the one not reached is undrained and still live.
    assert!(report.released.is_empty(), "{:?}", report.released);
    assert!(!reg.get(&stuck).unwrap().state.is_released());
    assert!(report.undrained.iter().any(|r| r.id == first));
    assert_eq!(reg.get(&first).unwrap().state, ResourceState::Live);
    assert!(driver.calls_with("polite:pid 61").is_empty());
    let line = report.summary_line().expect("a summary line");
    assert!(
        line.contains("not drained before the shutdown deadline"),
        "{line}"
    );
}

#[tokio::test]
async fn bounded_session_drain_with_no_budget_touches_nothing() {
    let reg = ResourceRegistry::in_memory();
    let driver = FakeDriver::new();
    let id = register(&reg, process(71), &session_owner());
    let report = teardown_session_within(&reg, &driver, fast(), Duration::ZERO).await;
    assert!(driver.calls().is_empty());
    assert_eq!(report.undrained.len(), 1);
    assert_eq!(reg.get(&id).unwrap().state, ResourceState::Live);
}

#[test]
fn bounded_policy_fits_polite_stop_and_force_grace_in_what_is_left() {
    let left = Duration::from_secs(8);
    let p = bounded_policy(slow(), left);
    assert!(p.deadline + p.force_grace <= left, "{p:?}");
    assert_eq!(p.force_grace, Duration::from_secs(8) / 3);
    // Plenty of room: the configured policy is unchanged.
    assert_eq!(bounded_policy(fast(), Duration::from_secs(8)), fast());
}

#[test]
fn the_signal_drain_budget_leaves_margin_inside_the_grace() {
    let grace = Duration::from_secs(10);
    let budget = signal_drain_budget(grace);
    assert_eq!(budget, Duration::from_secs(8));
    assert!(budget + SIGNAL_EXIT_MARGIN <= grace);
    assert_eq!(signal_drain_budget(Duration::from_secs(1)), Duration::ZERO);
}
