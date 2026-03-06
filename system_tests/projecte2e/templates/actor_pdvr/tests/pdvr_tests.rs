use actor_pdvr::actor::{Actor, Message, Response};
use actor_pdvr::state::{Phase, PhaseOutcome, StateMachine};

// ── State Machine Transitions ────────────────────────────────────────

#[test]
fn test_initial_phase_is_plan() {
    let sm = StateMachine::new(10);
    assert_eq!(sm.current_phase(), Phase::Plan);
    assert_eq!(sm.iteration(), 0);
}

#[test]
fn test_full_pdvr_cycle() {
    let mut sm = StateMachine::new(10);

    // Plan -> Do
    let next = sm.transition(PhaseOutcome::Success("plan done".into())).unwrap();
    assert_eq!(next, Phase::Do);

    // Do -> Verify
    let next = sm.transition(PhaseOutcome::Success("action done".into())).unwrap();
    assert_eq!(next, Phase::Verify);

    // Verify -> Reflect
    let next = sm.transition(PhaseOutcome::Success("checks pass".into())).unwrap();
    assert_eq!(next, Phase::Reflect);

    // Reflect -> Plan (new cycle)
    let next = sm.transition(PhaseOutcome::Success("lessons learned".into())).unwrap();
    assert_eq!(next, Phase::Plan);

    assert_eq!(sm.iteration(), 1);
}

#[test]
fn test_verify_failure_goes_back_to_do() {
    // BUG 2: Verify -> Do should be valid on failure (retry the action)
    let mut sm = StateMachine::new(10);
    sm.transition(PhaseOutcome::Success("".into())).unwrap(); // Plan -> Do
    sm.transition(PhaseOutcome::Success("".into())).unwrap(); // Do -> Verify

    // Verify fails — should go back to Do
    let result = sm.transition(PhaseOutcome::Failure("tests failed".into()));
    assert!(result.is_ok(), "Verify -> Do on failure should be valid");
    assert_eq!(result.unwrap(), Phase::Do);
}

#[test]
fn test_plan_to_verify_is_invalid() {
    // BUG 1: Plan -> Verify should NOT be allowed (can't skip Do)
    let sm = StateMachine::new(10);
    assert!(
        !sm.is_valid_transition(Phase::Verify),
        "Plan -> Verify should be invalid (can't skip Do phase)"
    );
}

#[test]
fn test_max_iterations_enforced() {
    let mut sm = StateMachine::new(2);

    // Cycle 1
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();

    // Cycle 2
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();

    assert!(sm.is_exhausted());
    let result = sm.transition(PhaseOutcome::Success("".into()));
    assert!(result.is_err(), "should reject transition when exhausted");
}

#[test]
fn test_verify_do_loop_still_counts_toward_max() {
    // BUG 3: A Verify->Do->Verify loop should still count toward max_iterations.
    // Without this, an agent could loop forever retrying verification.
    let mut sm = StateMachine::new(3);

    sm.transition(PhaseOutcome::Success("".into())).unwrap(); // Plan -> Do
    sm.transition(PhaseOutcome::Success("".into())).unwrap(); // Do -> Verify

    // Loop: Verify fails -> Do -> Verify fails -> Do -> ...
    // This should eventually exhaust iterations even without completing a full cycle.
    for _ in 0..20 {
        if sm.is_exhausted() {
            break;
        }
        let _ = sm.transition(PhaseOutcome::Failure("retry".into())); // Verify -> Do
        if sm.is_exhausted() {
            break;
        }
        let _ = sm.transition(PhaseOutcome::Success("".into())); // Do -> Verify
    }

    assert!(
        sm.is_exhausted(),
        "retry loop must eventually exhaust iterations"
    );
}

#[test]
fn test_reset_allows_new_task() {
    // BUG 4: reset() should reset the iteration counter
    let mut sm = StateMachine::new(1);

    // Exhaust with one cycle
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    sm.transition(PhaseOutcome::Success("".into())).unwrap();
    assert!(sm.is_exhausted());

    sm.reset();
    assert!(!sm.is_exhausted(), "reset should allow running a new task");
    assert_eq!(sm.current_phase(), Phase::Plan);
    assert_eq!(sm.iteration(), 0);
}

#[test]
fn test_history_recorded() {
    let mut sm = StateMachine::new(10);
    sm.transition(PhaseOutcome::Success("planned".into())).unwrap();
    sm.transition(PhaseOutcome::Success("executed".into())).unwrap();
    assert_eq!(sm.history().len(), 2);
    assert_eq!(sm.history()[0].phase, Phase::Plan);
    assert_eq!(sm.history()[1].phase, Phase::Do);
}

// ── Actor Message Handling ───────────────────────────────────────────

#[test]
fn test_actor_start_task() {
    // BUG 5 + 6: send() should accept messages when queue has room
    let mut actor = Actor::new(10, 16);
    let result = actor.send(Message::StartTask("my task".into()));
    assert!(result.is_ok(), "send should succeed when queue has room");
    let responses = actor.process();
    assert_eq!(responses.len(), 1);
    assert_eq!(responses[0], Response::TaskStarted(Phase::Plan));
}

#[test]
fn test_actor_full_cycle() {
    let mut actor = Actor::new(10, 16);

    actor.send(Message::StartTask("task".into())).unwrap();
    actor.process();

    // Plan phase
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("planned".into())))
        .unwrap();
    let r = actor.process();
    assert_eq!(r[0], Response::Transitioned(Phase::Do));

    // Do phase
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("done".into())))
        .unwrap();
    let r = actor.process();
    assert_eq!(r[0], Response::Transitioned(Phase::Verify));

    // Verify phase
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("verified".into())))
        .unwrap();
    let r = actor.process();
    assert_eq!(r[0], Response::Transitioned(Phase::Reflect));

    // Reflect phase — should complete the task
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("reflected".into())))
        .unwrap();
    let r = actor.process();
    match &r[0] {
        Response::TaskComplete {
            iterations,
            history_len,
        } => {
            assert_eq!(*iterations, 1, "should have completed 1 iteration");
            assert_eq!(*history_len, 4, "should have 4 phase records");
        }
        other => panic!("expected TaskComplete, got {:?}", other),
    }
}

#[test]
fn test_actor_queue_backpressure() {
    let mut actor = Actor::new(10, 3);

    // Fill the queue
    actor.send(Message::GetStatus).unwrap();
    actor.send(Message::GetStatus).unwrap();
    actor.send(Message::GetStatus).unwrap();

    // Queue should be full now
    let result = actor.send(Message::GetStatus);
    assert!(result.is_err(), "should reject when queue is full");
}

#[test]
fn test_actor_fifo_message_order() {
    // BUG 7: Messages should be processed in FIFO order
    let mut actor = Actor::new(10, 16);

    actor.send(Message::StartTask("task".into())).unwrap();
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("planned".into())))
        .unwrap();

    let responses = actor.process();

    // First response should be TaskStarted (from StartTask message)
    assert_eq!(responses[0], Response::TaskStarted(Phase::Plan));
    // Second response should be Transitioned (from PhaseComplete message)
    assert_eq!(responses[1], Response::Transitioned(Phase::Do));
}

#[test]
fn test_actor_status_shows_task() {
    let mut actor = Actor::new(10, 16);
    actor.send(Message::StartTask("my task".into())).unwrap();
    actor.process();

    actor.send(Message::GetStatus).unwrap();
    let responses = actor.process();

    match &responses[0] {
        Response::Status { task, phase, .. } => {
            assert_eq!(*phase, Phase::Plan);
            assert_eq!(task.as_deref(), Some("my task"));
        }
        other => panic!("expected Status, got {:?}", other),
    }
}

#[test]
fn test_actor_rejects_double_start() {
    let mut actor = Actor::new(10, 16);
    actor.send(Message::StartTask("task 1".into())).unwrap();
    actor.process();

    actor.send(Message::StartTask("task 2".into())).unwrap();
    let responses = actor.process();
    assert!(
        matches!(responses[0], Response::Error(_)),
        "should reject second task while first is running"
    );
}

#[test]
fn test_actor_phase_complete_without_task() {
    let mut actor = Actor::new(10, 16);
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("".into())))
        .unwrap();
    let responses = actor.process();
    assert!(
        matches!(responses[0], Response::Error(_)),
        "should error when no task is in progress"
    );
}

#[test]
fn test_actor_shutdown() {
    let mut actor = Actor::new(10, 16);
    actor.send(Message::Shutdown).unwrap();
    let responses = actor.process();
    assert_eq!(responses[0], Response::ShuttingDown);
    assert!(actor.is_stopped());

    let result = actor.send(Message::GetStatus);
    assert!(result.is_err(), "should reject messages after shutdown");
}

#[test]
fn test_actor_shutdown_reports_pending() {
    // BUG 9: Shutdown should report how many messages were dropped
    let mut actor = Actor::new(10, 16);
    actor.send(Message::StartTask("task".into())).unwrap();
    actor
        .send(Message::PhaseComplete(PhaseOutcome::Success("".into())))
        .unwrap();
    actor.send(Message::Shutdown).unwrap();

    let responses = actor.process();

    // The Shutdown message should be the last thing processed,
    // and all messages before it should have been processed first.
    assert!(
        responses.len() >= 3,
        "all queued messages should be processed before shutdown, got {} responses",
        responses.len()
    );
    assert_eq!(
        *responses.last().unwrap(),
        Response::ShuttingDown,
        "last response should be ShuttingDown"
    );
}

// ── Edge Cases ───────────────────────────────────────────────────────

#[test]
fn test_actor_can_run_multiple_tasks() {
    let mut actor = Actor::new(10, 16);

    // Task 1
    actor.send(Message::StartTask("task 1".into())).unwrap();
    actor.process();
    for _ in 0..4 {
        actor
            .send(Message::PhaseComplete(PhaseOutcome::Success("ok".into())))
            .unwrap();
        actor.process();
    }
    assert!(actor.current_task().is_none(), "task should be complete");

    // Task 2 — should work because state was reset
    actor.send(Message::StartTask("task 2".into())).unwrap();
    let r = actor.process();
    assert_eq!(r[0], Response::TaskStarted(Phase::Plan));
}

#[test]
fn test_state_machine_do_failure_goes_to_plan() {
    let mut sm = StateMachine::new(10);
    sm.transition(PhaseOutcome::Success("planned".into())).unwrap(); // Plan -> Do

    let next = sm.transition(PhaseOutcome::Failure("action failed".into())).unwrap();
    assert_eq!(next, Phase::Plan, "Do failure should go back to Plan");
}
