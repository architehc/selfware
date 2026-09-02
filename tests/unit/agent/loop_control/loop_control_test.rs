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

// ---------------------------------------------------------------------------
// Adaptive iteration budget (loop 13)
// ---------------------------------------------------------------------------

fn progress_turn(had_success: bool, sigs: &[(&str, u64)]) -> TurnProgress {
    TurnProgress {
        had_success,
        signatures: sigs
            .iter()
            .map(|(name, hash)| (name.to_string(), *hash))
            .collect(),
    }
}

fn streak_of(turns: Vec<TurnProgress>) -> std::collections::VecDeque<TurnProgress> {
    turns.into_iter().collect()
}

#[test]
fn productive_streak_extends_when_all_turns_productive() {
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("grep_search", 2)]),
        progress_turn(true, &[("file_read", 3)]),
        progress_turn(true, &[("symbol_search", 4)]),
        progress_turn(true, &[("file_read", 5)]),
    ]);
    assert!(productive_streak(&turns, 5));
    // A longer history is fine — only the last `window` turns are judged.
    let mut longer = streak_of(vec![progress_turn(false, &[("file_read", 1)])]);
    longer.extend(turns);
    assert!(productive_streak(&longer, 5));
}

#[test]
fn productive_streak_needs_full_window_of_evidence() {
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("file_read", 2)]),
    ]);
    assert!(!productive_streak(&turns, 5));
}

#[test]
fn repeated_identical_call_is_not_progress() {
    // The same tool+args repeats across the window (turn 1 and turn 5).
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("grep_search", 2)]),
        progress_turn(true, &[("file_read", 3)]),
        progress_turn(true, &[("symbol_search", 4)]),
        progress_turn(true, &[("file_read", 1)]),
    ]);
    assert!(!productive_streak(&turns, 5));

    // A duplicated call inside a single turn also disqualifies it.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1), ("file_read", 1)]),
        progress_turn(true, &[("grep_search", 2)]),
        progress_turn(true, &[("file_read", 3)]),
        progress_turn(true, &[("symbol_search", 4)]),
        progress_turn(true, &[("file_read", 5)]),
    ]);
    assert!(!productive_streak(&turns, 5));
}

#[test]
fn error_only_streak_is_not_progress() {
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("grep_search", 2)]),
        progress_turn(false, &[("file_read", 3)]),
        progress_turn(true, &[("symbol_search", 4)]),
        progress_turn(true, &[("file_read", 5)]),
    ]);
    assert!(!productive_streak(&turns, 5));

    // A tool-less turn (no calls at all) is no evidence of progress.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[]),
        progress_turn(true, &[("file_read", 3)]),
        progress_turn(true, &[("symbol_search", 4)]),
        progress_turn(true, &[("file_read", 5)]),
    ]);
    assert!(!productive_streak(&turns, 5));
}

#[test]
fn extension_is_quarter_of_original_and_grants_up_to_four() {
    let mut loop_ctrl = AgentLoop::new(30);
    // Multi-fire policy (TB4: the one-shot +50% still left productive tasks
    // dead at the cap): each grant is +25% of the ORIGINAL cap, at most 4
    // grants (+100% total).
    assert_eq!(loop_ctrl.extend_budget_once(), Some(7));
    assert_eq!(loop_ctrl.extend_budget_once(), Some(7));
    assert_eq!(loop_ctrl.extend_budget_once(), Some(7));
    assert_eq!(loop_ctrl.extend_budget_once(), Some(7));
    assert_eq!(loop_ctrl.max_iterations(), 58);
    assert_eq!(
        loop_ctrl.extend_budget_once(),
        None,
        "total extension is capped at +100% of the original cap"
    );
    assert_eq!(loop_ctrl.max_iterations(), 58);
}

#[test]
fn extension_of_tiny_cap_extends_by_at_least_one() {
    let mut loop_ctrl = AgentLoop::new(1);
    assert_eq!(loop_ctrl.extend_budget_once(), Some(1));
    assert_eq!(loop_ctrl.max_iterations(), 2);
}

#[test]
fn extension_lets_the_loop_run_past_the_original_cap() {
    let mut loop_ctrl = AgentLoop::new(8);
    loop_ctrl.next_state(); // Planning
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..8 {
        loop_ctrl.next_state(); // iterations 1-8
    }
    let capped = loop_ctrl.next_state(); // 9 > 8 — cap tripped
    assert!(matches!(capped, Some(AgentState::Failed { .. })));

    // Each grant adds 8/4 = 2; sustained productivity re-earns budget up to
    // the +100% ceiling (4 grants: cap 8 → 16).
    for expected_cap in [10, 12, 14, 16] {
        assert_eq!(loop_ctrl.extend_budget_once(), Some(2));
        assert_eq!(loop_ctrl.max_iterations(), expected_cap);
        let step = loop_ctrl.current_step();
        let iteration = loop_ctrl.current_iteration();
        loop_ctrl.restore_progress(step, iteration);
        assert!(matches!(
            loop_ctrl.next_state(), // fits within the new cap
            Some(AgentState::Executing { .. })
        ));
        let tripped = loop_ctrl.next_state(); // past the new cap
        assert!(matches!(tripped, Some(AgentState::Failed { .. })));
    }
    assert_eq!(
        loop_ctrl.extend_budget_once(),
        None,
        "after four grants the extension ceiling is reached"
    );
}

#[test]
fn reset_for_task_restores_original_budget_and_extension() {
    let mut loop_ctrl = AgentLoop::new(10);
    assert_eq!(loop_ctrl.extend_budget_once(), Some(2));
    assert_eq!(loop_ctrl.max_iterations(), 12);
    loop_ctrl.reset_for_task();
    assert_eq!(loop_ctrl.max_iterations(), 10);
    assert_eq!(
        loop_ctrl.extend_budget_once(),
        Some(2),
        "a new task gets its own extension budget"
    );
}
