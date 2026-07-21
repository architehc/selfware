use super::*;

#[test]
fn test_agent_loop_new() {
    let loop_ctrl = AgentLoop::new(100);
    assert_eq!(loop_ctrl.max_iterations, 100);
    assert_eq!(loop_ctrl.current_step, 0);
    assert_eq!(loop_ctrl.iteration, 0);
}

#[test]
fn test_agent_loop_initial_state_is_planning() {
    let mut loop_ctrl = AgentLoop::new(100);
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Planning)));
}

#[test]
fn test_agent_loop_set_state() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Executing { step: 0 })));
}

#[test]
fn test_agent_loop_increment_step() {
    let mut loop_ctrl = AgentLoop::new(100);
    assert_eq!(loop_ctrl.current_step(), 0);
    loop_ctrl.increment_step().unwrap();
    assert_eq!(loop_ctrl.current_step(), 1);
    loop_ctrl.increment_step().unwrap();
    assert_eq!(loop_ctrl.current_step(), 2);
}

#[test]
fn test_agent_loop_max_iterations_exceeded() {
    let mut loop_ctrl = AgentLoop::new(3);

    // First call is Planning — does not consume an iteration slot.
    assert!(loop_ctrl.next_state().is_some());

    // Transition to Executing so subsequent calls increment.
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();

    // 3 execution iterations should work
    assert!(loop_ctrl.next_state().is_some());
    assert!(loop_ctrl.next_state().is_some());
    assert!(loop_ctrl.next_state().is_some());

    // 4th execution iteration should fail
    let state = loop_ctrl.next_state();
    assert!(
        matches!(state, Some(AgentState::Failed { reason }) if reason == "Max iterations exceeded")
    );
}

#[test]
fn test_agent_state_error_recovery() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl
        .transition_to(AgentState::ErrorRecovery {
            error: "Test error".to_string(),
        })
        .unwrap();

    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::ErrorRecovery { error }) => {
            assert_eq!(error, "Test error");
        }
        _ => panic!("Expected ErrorRecovery state"),
    }
}

#[test]
fn test_agent_state_failed() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl
        .transition_to(AgentState::Failed {
            reason: "Something went wrong".to_string(),
        })
        .unwrap();

    let state = loop_ctrl.next_state();
    match state {
        Some(AgentState::Failed { reason }) => {
            assert_eq!(reason, "Something went wrong");
        }
        _ => panic!("Expected Failed state"),
    }
}

#[test]
fn test_executing_state_tracks_step() {
    let state = AgentState::Executing { step: 5 };
    match state {
        AgentState::Executing { step } => assert_eq!(step, 5),
        _ => panic!("Expected Executing state"),
    }
}

#[test]
fn test_increment_step_updates_state() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl.increment_step().unwrap();

    // After increment, state should be Executing with current step
    match &loop_ctrl.state {
        AgentState::Executing { step } => assert_eq!(*step, 1),
        _ => panic!("Expected Executing state after increment"),
    }
}

#[test]
fn test_reset_for_task() {
    let mut loop_ctrl = AgentLoop::new(10);

    // Simulate iterations: Planning turn + execution turns
    loop_ctrl.next_state(); // Planning — no increment
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    loop_ctrl.next_state(); // iteration 1
    loop_ctrl.next_state(); // iteration 2
    loop_ctrl.next_state(); // iteration 3
    loop_ctrl.increment_step().unwrap();
    loop_ctrl.increment_step().unwrap();

    assert_eq!(loop_ctrl.iteration, 3);
    assert_eq!(loop_ctrl.current_step(), 2);

    // Reset for a new task
    loop_ctrl.reset_for_task();
    assert_eq!(loop_ctrl.iteration, 0);
    assert_eq!(loop_ctrl.current_step(), 0);
    assert!(matches!(loop_ctrl.state, AgentState::Planning));

    // Planning turn + 10 execution turns should all succeed
    loop_ctrl.next_state(); // Planning — no increment
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..10 {
        let state = loop_ctrl.next_state();
        assert!(!matches!(state, Some(AgentState::Failed { .. })));
    }
    // 11th execution turn should fail
    let state = loop_ctrl.next_state();
    assert!(matches!(state, Some(AgentState::Failed { .. })));
}

#[test]
fn test_restore_progress() {
    let mut loop_ctrl = AgentLoop::new(10);
    loop_ctrl.restore_progress(3, 7);

    assert_eq!(loop_ctrl.current_step(), 3);
    assert_eq!(loop_ctrl.current_iteration(), 7);
    assert!(matches!(loop_ctrl.state, AgentState::Executing { step: 3 }));
}

#[test]
fn test_invalid_transition_rejected() {
    let mut loop_ctrl = AgentLoop::new(10);
    let error = loop_ctrl.transition_to(AgentState::Completed).unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid agent state transition from 'planning' to 'completed'"
    );
}

#[test]
fn test_approaching_limit_warning_none_early() {
    let mut loop_ctrl = AgentLoop::new(100);
    // Advance to 50% — no warning. Planning doesn't increment, so
    // transition to Executing first.
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..50 {
        loop_ctrl.next_state();
    }
    assert!(loop_ctrl.approaching_limit_warning().is_none());
}

#[test]
fn test_approaching_limit_warning_at_80_pct() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..80 {
        loop_ctrl.next_state();
    }
    let warning = loop_ctrl.approaching_limit_warning();
    assert!(warning.is_some());
    assert!(warning.unwrap().contains("wrapping up"));
}

#[test]
fn test_approaching_limit_warning_at_90_pct() {
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..90 {
        loop_ctrl.next_state();
    }
    let warning = loop_ctrl.approaching_limit_warning();
    assert!(warning.is_some());
    assert!(warning.unwrap().contains("final answer"));
}

#[test]
fn test_approaching_limit_warning_zero_max() {
    let loop_ctrl = AgentLoop::new(0);
    assert!(loop_ctrl.approaching_limit_warning().is_none());
}
