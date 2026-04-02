use guided_scheduler_lab::{
    parse_command, Command, Priority, Recurrence, Scheduler, SchedulerError, Status,
};

#[test]
fn parse_add_command_extracts_metadata() {
    let command = parse_command(
        "add Ship release notes ;p1 ;tags=docs,release ;estimate=45 ;every=weekly:2",
    )
    .expect("parse should succeed");

    let Command::Add(spec) = command else {
        panic!("expected add command");
    };

    assert_eq!(spec.title, "Ship release notes");
    assert_eq!(spec.priority, Priority::P1);
    assert_eq!(spec.tags, vec!["docs", "release"]);
    assert_eq!(spec.estimate_minutes, Some(45));
    assert_eq!(
        spec.recurrence,
        Some(Recurrence::Weekly { interval_weeks: 2 })
    );
}

#[test]
fn block_command_rejects_non_numeric_dependency_ids() {
    let err = parse_command("block 7 on alpha,2").unwrap_err();
    assert!(
        err.to_string().contains("numeric dependency id"),
        "unexpected error: {err}"
    );
}

#[test]
fn scheduler_ready_tasks_unlock_after_dependency_completion() {
    let mut scheduler = Scheduler::new();
    let write_tests = scheduler
        .apply(
            parse_command("add Write more tests ;p0 ;tags=quality")
                .expect("task should parse"),
        )
        .expect("add should succeed")
        .expect("add should return a new id");

    let document_release = scheduler
        .apply(
            parse_command("add Document the release ;p1 ;tags=docs")
                .expect("task should parse"),
        )
        .expect("add should succeed")
        .expect("add should return a new id");

    scheduler
        .apply(Command::Block {
            task_id: document_release,
            dependency_ids: vec![write_tests],
        })
        .expect("block should succeed");

    let ready_titles = scheduler
        .ready_tasks()
        .iter()
        .map(|task| task.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ready_titles, vec!["Write more tests"]);

    scheduler
        .apply(Command::Complete(write_tests))
        .expect("complete should succeed");

    let ready_titles = scheduler
        .ready_tasks()
        .iter()
        .map(|task| task.title.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ready_titles,
        vec!["Document the release"],
        "dependency completion should unblock downstream work"
    );
}

#[test]
fn add_command_priority_is_preserved_in_scheduler() {
    let mut scheduler = Scheduler::new();
    let task_id = scheduler
        .apply(parse_command("add Harden parser ;p0 ;tags=parser").unwrap())
        .expect("add should succeed")
        .expect("id expected");

    let task = scheduler
        .tasks()
        .iter()
        .find(|task| task.id == task_id)
        .expect("task should exist");
    assert_eq!(task.priority, Priority::P0);
}

#[test]
fn snapshot_round_trip_preserves_metadata() {
    let mut scheduler = Scheduler::new();
    let task_id = scheduler
        .apply(
            parse_command("add Refresh weekly summary ;p1 ;tags=ops,report ;estimate=20")
                .unwrap(),
        )
        .expect("add should succeed")
        .expect("id expected");

    scheduler
        .apply(Command::Retag {
            task_id,
            tags: vec!["ops".to_string(), "report".to_string(), "weekly".to_string()],
        })
        .expect("retag should succeed");
    scheduler
        .apply(Command::Complete(task_id))
        .expect("complete should succeed");

    let snapshot = scheduler.snapshot_json().expect("snapshot should serialize");
    let restored = Scheduler::restore_from_snapshot(&snapshot).expect("snapshot should restore");
    let task = restored.tasks().iter().find(|task| task.id == task_id).unwrap();

    assert_eq!(task.priority, Priority::P1);
    assert_eq!(task.tags, vec!["ops", "report", "weekly"]);
    assert_eq!(task.estimate_minutes, Some(20));
    assert_eq!(task.status, Status::Done);
}

#[test]
fn blocking_unknown_dependency_is_an_error() {
    let mut scheduler = Scheduler::new();
    let task_id = scheduler
        .apply(parse_command("add Review incident log").unwrap())
        .expect("add should succeed")
        .expect("id expected");

    let err = scheduler
        .apply(Command::Block {
            task_id,
            dependency_ids: vec![404],
        })
        .unwrap_err();

    match err {
        SchedulerError::MissingDependency {
            task_id: actual_task,
            dependency_id,
        } => {
            assert_eq!(actual_task, task_id);
            assert_eq!(dependency_id, 404);
        }
        other => panic!("expected missing dependency error, got {other:?}"),
    }
}
