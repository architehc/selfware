use super::*;

fn live(id: &str, state: TaskState) -> LiveTask {
    LiveTask {
        id: id.into(),
        agent: MAIN_AGENT.into(),
        description: "add max_words to slugify()".into(),
        original_description: None,
        task_type: Some("mutation".into()),
        state,
        state_since: Utc::now(),
        step: 3,
        constraints: TaskConstraints {
            max_turns: 40,
            turns_used: 3,
            token_budget: Some(100_000),
            allowed_paths: vec!["./**".into()],
        },
        usage: Some(RecordedUsage {
            total_tokens: 5_000,
            main_tokens: Some(4_000),
            side_tokens: Some(1_000),
            cost_usd: None,
            cost_complete: false,
        }),
        parent: None,
        pause_pending: false,
        paused: std::time::Duration::ZERO,
    }
}

fn control() -> (TaskControl, Arc<AtomicBool>) {
    let token = Arc::new(AtomicBool::new(false));
    (TaskControl::new(Arc::clone(&token)), token)
}

#[test]
fn requests_need_the_task_running_here() {
    let (c, _) = control();
    assert_eq!(c.request_pause("t1"), Err(ControlError::NoLiveTask));
    c.publish(live("t1", TaskState::Executing));
    assert_eq!(
        c.request_pause("other"),
        Err(ControlError::NotInThisProcess("other".into()))
    );
    c.publish(live("t1", TaskState::Completed));
    assert_eq!(
        c.request_cancel("t1"),
        Err(ControlError::Finished("t1".into()))
    );
}

#[test]
fn pause_is_pending_until_the_agent_reaches_its_safe_point() {
    let (c, _) = control();
    c.publish(live("t1", TaskState::Executing));
    c.request_pause("t1").unwrap();
    assert!(c.pause_requested());
    assert!(c.snapshot().unwrap().pause_pending);
    // The agent pauses: the snapshot says paused, nothing pending.
    c.publish(live("t1", TaskState::Paused));
    assert!(!c.snapshot().unwrap().pause_pending);
    assert!(!c.take_resume(), "still paused");
    c.request_resume("t1").unwrap();
    assert!(c.take_resume());
    assert!(!c.pause_requested());
}

#[test]
fn resume_of_a_running_task_without_a_pause_is_refused() {
    let (c, _) = control();
    c.publish(live("t1", TaskState::Executing));
    assert_eq!(
        c.request_resume("t1"),
        Err(ControlError::NotPaused("t1".into()))
    );
    // A pause not reached yet can be withdrawn.
    c.request_pause("t1").unwrap();
    c.request_resume("t1").unwrap();
    assert!(!c.pause_requested());
}

#[test]
fn cancel_latches_the_agent_token_and_is_remembered() {
    let (c, token) = control();
    c.publish(live("t1", TaskState::Executing));
    c.request_cancel("t1").unwrap();
    assert!(token.load(Ordering::Relaxed));
    assert!(c.cancel_requested());
    c.begin_task();
    assert!(!c.cancel_requested(), "a new task starts clean");
}

#[test]
fn edit_is_validated_and_implies_a_pause() {
    let (c, _) = control();
    let l = live("t1", TaskState::Executing);
    c.publish(l.clone());
    let mut e = TaskEdit::from_live(&l);
    assert_eq!(e.changes(&l), None, "unchanged edit changes nothing");
    e.max_turns = 3;
    assert!(matches!(
        c.submit_edit("t1", e.clone()),
        Err(ControlError::InvalidEdit(m)) if m.contains("3 already used")
    ));
    e.max_turns = 60;
    e.token_budget = Some(4_000);
    assert!(matches!(
        c.submit_edit("t1", e.clone()),
        Err(ControlError::InvalidEdit(m)) if m.contains("5000 tokens")
    ));
    e.token_budget = None;
    e.description = "  ".into();
    assert!(c.submit_edit("t1", e.clone()).is_err());
    e.description = "add max_words and max_len to slugify()".into();
    c.submit_edit("t1", e.clone()).unwrap();
    assert!(c.pause_requested());
    let taken = c.take_edit().unwrap();
    let changes = taken.changes(&l).unwrap();
    assert!(changes.contains("description is now: add max_words and max_len"));
    assert!(changes.contains("max turns 40 → 60"));
    assert!(changes.contains("token budget 100000 → unbounded"));
}

#[test]
fn time_in_state_restarts_only_on_a_state_change() {
    let (c, _) = control();
    let mut a = live("t1", TaskState::Executing);
    a.state_since = Utc::now() - chrono::Duration::seconds(90);
    c.publish(a.clone());
    let mut b = live("t1", TaskState::Executing);
    b.step = 9;
    c.publish(b);
    assert_eq!(c.snapshot().unwrap().state_since, a.state_since);
    c.publish(live("t1", TaskState::Paused));
    assert!(c.snapshot().unwrap().state_since > a.state_since);
}

#[test]
fn a_queued_fork_attaches_only_to_the_matching_task_text() {
    let (c, _) = control();
    c.queue_fork("orig", "do it better");
    assert_eq!(c.take_fork_for("something else"), None);
    assert_eq!(c.take_fork_for("do it better"), Some("orig".into()));
    assert_eq!(c.take_fork_for("do it better"), None, "consumed");
}
