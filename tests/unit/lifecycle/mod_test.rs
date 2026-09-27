use super::*;

fn temp_log() -> (tempfile::TempDir, EventLog) {
    let dir = tempfile::tempdir().unwrap();
    let log = EventLog::at(dir.path().join("state").join("events.jsonl"));
    (dir, log)
}

// ---- agent machine --------------------------------------------------------

#[test]
fn agent_works_one_task_at_a_time() {
    let working =
        AgentMachine::next(&AgentState::Idle, &AgentEvent::Assign { task: "t1".into() }).unwrap();
    assert_eq!(working.task(), Some("t1"));
    // A second assignment while working is refused.
    assert!(AgentMachine::next(&working, &AgentEvent::Assign { task: "t2".into() }).is_err());
    let blocked = AgentMachine::next(
        &working,
        &AgentEvent::Block {
            reason: "approval".into(),
        },
    )
    .unwrap();
    assert_eq!(blocked.task(), Some("t1"));
    assert_eq!(
        AgentMachine::next(&blocked, &AgentEvent::Unblock).unwrap(),
        working
    );
    assert_eq!(
        AgentMachine::next(&blocked, &AgentEvent::Done).unwrap(),
        AgentState::Idle
    );
}

#[test]
fn agent_crash_restart_and_sticky_stop() {
    let crashed = AgentMachine::next(&AgentState::Idle, &AgentEvent::Crash).unwrap();
    assert_eq!(crashed, AgentState::Crashed);
    assert!(AgentMachine::next(&crashed, &AgentEvent::Crash).is_err());
    assert_eq!(
        AgentMachine::next(&crashed, &AgentEvent::Restart).unwrap(),
        AgentState::Idle
    );
    let all_events = [
        AgentEvent::Assign { task: "t".into() },
        AgentEvent::Block { reason: "r".into() },
        AgentEvent::Unblock,
        AgentEvent::Done,
        AgentEvent::Crash,
        AgentEvent::Restart,
        AgentEvent::Stop,
    ];
    for e in &all_events {
        assert!(
            AgentMachine::next(&AgentState::Stopped, e).is_err(),
            "stopped accepted {}",
            e.label()
        );
    }
    // Every live state can be stopped, and every accepted step passes the oracle.
    let states = [
        AgentState::Idle,
        AgentState::Working { task: "t".into() },
        AgentState::Blocked {
            task: "t".into(),
            reason: "r".into(),
        },
        AgentState::Crashed,
    ];
    for s in &states {
        assert_eq!(
            AgentMachine::next(s, &AgentEvent::Stop).unwrap(),
            AgentState::Stopped
        );
        for e in &all_events {
            if let Ok(t) = AgentMachine::next(s, e) {
                assert_eq!(AgentMachine::check_step(s, e, &t), Ok(()));
            }
        }
    }
}

// ---- resource machine -----------------------------------------------------

#[test]
fn resource_happy_path_and_teardown() {
    use ResourceEvent as E;
    use ResourceState as S;
    let mut s = S::Requested;
    for (e, want) in [
        (E::Start, S::Starting),
        (E::Ready, S::Live),
        (E::Drain, S::Draining),
        (E::Stopped, S::Released),
    ] {
        s = ResourceMachine::next(&s, &e).unwrap();
        assert_eq!(s, want);
    }
    // released is sticky
    for e in ResourceEvent::ALL {
        assert!(ResourceMachine::next(&S::Released, &e).is_err());
    }
}

#[test]
fn resource_deadline_leak_orphan_and_reap() {
    use ResourceEvent as E;
    use ResourceState as S;
    // Every draining resource has a deadline exit that settles it (P4 table half).
    let leaked = ResourceMachine::next(&S::Draining, &E::DeadlinePassed).unwrap();
    assert_eq!(leaked, S::Leaked);
    assert_eq!(ResourceMachine::on_enter(&leaked), vec![Effect::LeakAlarm]);
    // The reaper may retry a leak, or find it gone.
    assert_eq!(
        ResourceMachine::next(&S::Leaked, &E::Reap).unwrap(),
        S::Draining
    );
    assert_eq!(
        ResourceMachine::next(&S::Leaked, &E::Stopped).unwrap(),
        S::Released
    );
    assert!(ResourceMachine::next(&S::Leaked, &E::Ready).is_err());
    // P5: an orphan is drained by the reaper.
    let orphan = ResourceMachine::next(&S::Live, &E::OwnerGone).unwrap();
    assert_eq!(orphan, S::Orphaned);
    assert_eq!(
        ResourceMachine::next(&orphan, &E::Reap).unwrap(),
        S::Draining
    );
    // A resource never started needs no stop.
    assert_eq!(
        ResourceMachine::next(&S::Requested, &E::Drain).unwrap(),
        S::Released
    );
    // A failed start is an alarm, not a silent release.
    assert_eq!(
        ResourceMachine::next(&S::Starting, &E::Fail).unwrap(),
        S::Leaked
    );
}

#[test]
fn resource_oracle_accepts_the_table_and_flags_violations() {
    for s in ResourceState::ALL {
        for e in ResourceEvent::ALL {
            if let Ok(t) = ResourceMachine::next(&s, &e) {
                assert_eq!(
                    ResourceMachine::check_step(&s, &e, &t),
                    Ok(()),
                    "{s:?} {e:?}"
                );
            }
        }
    }
    assert!(ResourceMachine::check_step(
        &ResourceState::Released,
        &ResourceEvent::Start,
        &ResourceState::Starting
    )
    .is_err());
    assert!(ResourceMachine::check_step(
        &ResourceState::Leaked,
        &ResourceEvent::Ready,
        &ResourceState::Live
    )
    .is_err());
}

// ---- Tracked + log --------------------------------------------------------

#[test]
fn tracked_records_one_line_per_transition() {
    let (_dir, log) = temp_log();
    let t: Tracked<TaskMachine> = Tracked::new("task-a", TaskState::Queued, log.clone())
        .with_task_type("bug_fix")
        .with_owner("main");
    t.record_created("task created");
    let mut t = t;
    assert!(t.apply(TaskEvent::Start, "run started").unwrap().is_empty());
    assert!(t.apply(TaskEvent::Planned, "planned").unwrap().is_empty());
    let effects = t.apply(TaskEvent::Succeed, "outcome completed").unwrap();
    assert_eq!(effects, vec![Effect::TeardownOwned]);
    assert!(t.is_terminal());

    let (records, skipped) = log.read_all();
    assert_eq!(skipped, 0);
    let steps: Vec<(Option<&str>, &str, Option<&str>)> = records
        .iter()
        .map(|r| (r.from.as_deref(), r.to.as_str(), r.event.as_deref()))
        .collect();
    assert_eq!(
        steps,
        vec![
            (None, "queued", None),
            (Some("queued"), "planning", Some("start")),
            (Some("planning"), "executing", Some("planned")),
            (Some("executing"), "completed", Some("succeed")),
        ]
    );
    for r in &records {
        assert_eq!(r.entity, Entity::Task);
        assert_eq!(r.id, "task-a");
        assert_eq!(r.task_type.as_deref(), Some("bug_fix"));
        assert_eq!(r.owner.as_deref(), Some("main"));
        assert_eq!(r.pid, Some(std::process::id()));
    }
    assert_eq!(
        log.last_state(Entity::Task, "task-a").as_deref(),
        Some("completed")
    );
    assert_eq!(log.last_state(Entity::Task, "other"), None);
}

#[test]
fn tracked_resource_records_its_owner() {
    let (_dir, log) = temp_log();
    let mut r: Tracked<ResourceMachine> =
        Tracked::new("container:8f2c", ResourceState::Requested, log.clone()).with_owner("task-a");
    r.record_created("docker run");
    r.apply(ResourceEvent::Start, "spawn").unwrap();
    let (records, _) = log.read_all();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|x| x.entity == Entity::Resource));
    assert!(records.iter().all(|x| x.owner.as_deref() == Some("task-a")));
}
