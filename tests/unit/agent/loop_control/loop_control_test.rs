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

/// e2e lowcap: max_iterations 12 + two +3 extensions = cap 18; the run
/// executed 18 iterations and the summary printed "iterations: 19/18". The
/// Nth iteration is the last one executed, and the stop never counts the
/// refused slot — the reported count can never exceed the cap.
#[test]
fn cap_stop_never_reports_more_iterations_than_the_cap() {
    let mut loop_ctrl = AgentLoop::new(12);
    loop_ctrl.next_state(); // Planning
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    let mut executed = 0;
    loop {
        match loop_ctrl.next_state() {
            Some(AgentState::Failed { .. }) => {
                if loop_ctrl.max_iterations() < 18 && loop_ctrl.extend_budget_once().is_some() {
                    loop_ctrl.resume_after_extension();
                    executed += 1; // the resumed turn runs
                    continue;
                }
                break;
            }
            Some(_) => executed += 1,
            None => break,
        }
        assert!(loop_ctrl.current_iteration() <= loop_ctrl.max_iterations());
    }
    // Two grants of +3 (12 → 15 → 18), then the run stops at the cap.
    assert_eq!(loop_ctrl.max_iterations(), 18);
    assert_eq!(executed, 18, "exactly the capped number of iterations ran");
    assert_eq!(
        loop_ctrl.current_iteration(),
        18,
        "summary must read 18/18, never 19/18"
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
        // The refused turn resumes and takes its slot (cap - 1 → cap - 1 + 1).
        loop_ctrl.resume_after_extension();
        assert_eq!(loop_ctrl.current_iteration(), expected_cap - 1);
        assert!(matches!(
            loop_ctrl.next_state(), // fits within the new cap
            Some(AgentState::Executing { .. })
        ));
        assert_eq!(loop_ctrl.current_iteration(), expected_cap);
        let tripped = loop_ctrl.next_state(); // past the new cap
        assert!(matches!(tripped, Some(AgentState::Failed { .. })));
        assert_eq!(
            loop_ctrl.current_iteration(),
            expected_cap,
            "a refused slot is never counted"
        );
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

// ---------------------------------------------------------------------------
// Productive-streak duplicate rule (2026-09-22 long-horizon finding):
// re-running verification after an intervening successful mutation is the
// edit→test rhythm of a long task, not a stall; only a repeat with NO
// intervening mutation breaks the streak.
// ---------------------------------------------------------------------------

#[test]
fn verification_rerun_after_each_mutation_earns_streak() {
    // The exact long-task pattern the old rule killed: edit, test, edit,
    // test, edit — cargo_test repeats, but every repeat follows a fresh
    // mutation.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_edit", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 2)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 3)]),
    ]);
    assert!(
        productive_streak(&turns, 5),
        "verification re-run after an intervening mutation must earn the grant"
    );
}

#[test]
fn batched_edit_and_verify_turns_earn_streak() {
    // Same rhythm with the edit and the verification in ONE batched turn:
    // the mutation shares the repeat's turn, which still counts.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_edit", 1), ("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 2), ("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 3), ("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 4), ("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 5), ("cargo_test", 9)]),
    ]);
    assert!(productive_streak(&turns, 5));
}

#[test]
fn identical_verification_loop_without_mutation_still_breaks() {
    // The true stall: the same verification call five times with nothing
    // changing in between must keep breaking the streak (fail-closed).
    let turns = streak_of(
        (0..5)
            .map(|_| progress_turn(true, &[("cargo_test", 9)]))
            .collect(),
    );
    assert!(
        !productive_streak(&turns, 5),
        "5x the identical call with no intervening mutation is a stall"
    );
}

#[test]
fn same_mutation_reapplied_is_still_a_stall() {
    // The repeated call may not be its OWN witness: re-applying the identical
    // edit args changes nothing, so test/edit cycles with the SAME edit are
    // a retry loop, not progress.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_edit", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 1)]),
    ]);
    assert!(
        !productive_streak(&turns, 5),
        "the identical edit re-applied is not new work"
    );
}

#[test]
fn reread_after_mutation_is_progress() {
    // Re-reading a file after an edit (formatter ran, own edit to review) is
    // follow-up work on changed state, not a probe loop.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("file_edit", 2)]),
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("file_edit", 3)]),
        progress_turn(true, &[("file_read", 1)]),
    ]);
    assert!(productive_streak(&turns, 5));
}

#[test]
fn adjacent_repeat_after_mutation_turn_still_breaks() {
    // One edit, then the same verification twice in a row: nothing changed
    // between the two runs, so the second run is a bare retry.
    let turns = streak_of(vec![
        progress_turn(true, &[("file_edit", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_edit", 2)]),
        progress_turn(true, &[("cargo_test", 9)]),
    ]);
    assert!(
        !productive_streak(&turns, 5),
        "a back-to-back identical pair with no mutation between is a stall"
    );
}

#[test]
fn alternating_two_probes_without_mutation_still_breaks() {
    // test/read alternation with no mutation anywhere in the window: the
    // two calls are not each other's progress.
    let turns = streak_of(vec![
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
        progress_turn(true, &[("file_read", 1)]),
        progress_turn(true, &[("cargo_test", 9)]),
    ]);
    assert!(!productive_streak(&turns, 5));
}

#[test]
fn shell_probe_repeat_is_fail_closed_without_args() {
    // The progress window stores args HASHES, so shell commands cannot be
    // classified as mutating or observational after the fact. A repeated
    // shell_exec signature therefore never excuses itself — fail-closed.
    let turns = streak_of(
        (0..5)
            .map(|_| progress_turn(true, &[("shell_exec", 7)]))
            .collect(),
    );
    assert!(
        !productive_streak(&turns, 5),
        "shell_exec repeats cannot prove an intervening mutation"
    );
    // But a shell probe interleaved with REAL file-tool mutations still
    // earns the streak — the mutation witness is what matters.
    let turns = streak_of(vec![
        progress_turn(true, &[("shell_exec", 7)]),
        progress_turn(true, &[("file_write", 1)]),
        progress_turn(true, &[("shell_exec", 7)]),
        progress_turn(true, &[("file_write", 2)]),
        progress_turn(true, &[("shell_exec", 7)]),
    ]);
    assert!(productive_streak(&turns, 5));
}

// ---------------------------------------------------------------------------
// Adaptive-budget persistence across resume (2026-09-22 finding: the
// extended cap was rebuilt at the configured value on resume, silently
// dropping earned grants).
// ---------------------------------------------------------------------------

#[test]
fn restore_budget_extension_restores_cap_and_grants() {
    let mut loop_ctrl = AgentLoop::new(12);
    // Checkpoint persisted cap 15 with one grant consumed (12 + 12/4).
    loop_ctrl.restore_budget_extension(15, 1);
    assert_eq!(loop_ctrl.max_iterations(), 15);
    assert_eq!(loop_ctrl.extensions_granted(), 1);
    assert!(!loop_ctrl.extension_ceiling_reached());
    // The remaining grants keep the +25%-of-original step and the ceiling.
    assert_eq!(loop_ctrl.extend_budget_once(), Some(3));
    assert_eq!(loop_ctrl.max_iterations(), 18);
    assert_eq!(loop_ctrl.extend_budget_once(), Some(3));
    assert_eq!(loop_ctrl.extend_budget_once(), Some(3));
    assert_eq!(loop_ctrl.max_iterations(), 24);
    assert_eq!(
        loop_ctrl.extend_budget_once(),
        None,
        "ceiling includes restored grants"
    );
}

#[test]
fn restore_budget_extension_never_shrinks_below_configured_cap() {
    // Operator re-passed a LARGER --max-turns on resume: the persisted cap
    // must not shrink the configured one (extensions only ever grow a cap).
    let mut loop_ctrl = AgentLoop::new(20);
    loop_ctrl.restore_budget_extension(15, 2);
    assert_eq!(loop_ctrl.max_iterations(), 20);
    assert_eq!(
        loop_ctrl.extensions_granted(),
        2,
        "spent grants still count against the ceiling"
    );
}

#[test]
fn restore_budget_extension_clamps_grants_to_the_ceiling() {
    // A hand-edited/corrupt checkpoint must not mint grants beyond the
    // +100% ceiling, nor a huge grant count panic the accounting.
    let mut loop_ctrl = AgentLoop::new(12);
    loop_ctrl.restore_budget_extension(24, usize::MAX);
    assert!(loop_ctrl.extension_ceiling_reached());
    assert_eq!(loop_ctrl.extend_budget_once(), None);
}

// ---------------------------------------------------------------------------
// Chain-wide iteration total (resume segments accumulate; the per-segment
// counter resets for budget fairness).
// ---------------------------------------------------------------------------

#[test]
fn accumulated_iterations_fold_across_segments_and_reset_per_task() {
    let mut loop_ctrl = AgentLoop::new(10);
    loop_ctrl.next_state(); // Planning — consumes no iteration
    loop_ctrl
        .transition_to(AgentState::Executing { step: 0 })
        .unwrap();
    for _ in 0..3 {
        loop_ctrl.next_state();
    }
    assert_eq!(loop_ctrl.current_iteration(), 3);
    assert_eq!(loop_ctrl.accumulated_iterations(), 3);

    // The resume/chain boundary: per-segment counter resets, total folds.
    loop_ctrl.set_prior_iterations(13); // what Agent::resume restores
    loop_ctrl.reset_budget_for_resume();
    assert_eq!(loop_ctrl.current_iteration(), 0);
    assert_eq!(
        loop_ctrl.accumulated_iterations(),
        16,
        "the closing segment folds into the chain-wide total"
    );

    loop_ctrl.next_state();
    loop_ctrl.next_state();
    assert_eq!(loop_ctrl.accumulated_iterations(), 18);

    // A genuinely new task starts the chain total over.
    loop_ctrl.reset_for_task();
    assert_eq!(loop_ctrl.accumulated_iterations(), 0);
}

// ---------------------------------------------------------------------------
// Auto-checkpoint-and-continue (long-task caps, USER-APPROVED policy)
// ---------------------------------------------------------------------------

#[test]
fn auto_continue_count_lifecycle_is_bounded_and_per_task() {
    let mut loop_ctrl = AgentLoop::new(10);
    assert_eq!(loop_ctrl.auto_continue_count(), 0);

    // Registering returns the new count; the chain bound is 3 by policy.
    assert_eq!(loop_ctrl.register_auto_continue(), 1);
    assert_eq!(loop_ctrl.register_auto_continue(), 2);
    assert_eq!(loop_ctrl.register_auto_continue(), 3);
    assert_eq!(loop_ctrl.auto_continue_count(), 3);
    assert_eq!(loop_ctrl.auto_continue_count(), MAX_AUTO_CONTINUES);

    // A new task (run_task's reset_for_task) gets a fresh chain budget.
    loop_ctrl.reset_for_task();
    assert_eq!(loop_ctrl.auto_continue_count(), 0);
}

#[test]
fn extension_ceiling_reached_after_four_grants() {
    let mut loop_ctrl = AgentLoop::new(20);
    assert!(!loop_ctrl.extension_ceiling_reached());
    for _ in 0..4 {
        assert!(loop_ctrl.extend_budget_once().is_some());
    }
    assert!(loop_ctrl.extension_ceiling_reached());
    assert_eq!(loop_ctrl.extend_budget_once(), None);
}

#[test]
fn reset_budget_for_resume_mirrors_manual_resume() {
    // A run partway through a segment: step 5, iteration 150, cap extended
    // twice (+25% of the original 100 → 150), state Executing.
    let mut loop_ctrl = AgentLoop::new(100);
    loop_ctrl.restore_progress(5, 150);
    loop_ctrl.extend_budget_once();
    loop_ctrl.extend_budget_once();
    assert_eq!(loop_ctrl.max_iterations(), 150);
    loop_ctrl.register_auto_continue();

    // Exactly what `Agent::resume` re-creates: fresh iteration + extension
    // budget at the ORIGINAL cap, while the step counter keeps counting and
    // the chain counter is NOT reset (it bounds the whole task).
    loop_ctrl.reset_budget_for_resume();
    assert_eq!(loop_ctrl.current_iteration(), 0);
    assert_eq!(
        loop_ctrl.current_step(),
        5,
        "steps keep counting across the chain"
    );
    assert_eq!(loop_ctrl.max_iterations(), 100);
    assert!(!loop_ctrl.extension_ceiling_reached());
    assert_eq!(
        loop_ctrl.auto_continue_count(),
        1,
        "the chain counter bounds the whole task, not one segment"
    );
    assert!(matches!(loop_ctrl.current_state_label(), "executing"));

    // The fresh segment runs: iteration 1 fits inside the restored cap.
    assert!(matches!(
        loop_ctrl.next_state(),
        Some(AgentState::Executing { .. })
    ));
}

/// `turns_run` (the headless `num_turns`) counts the whole task: an
/// in-process continuation resets the per-segment iteration budget but not
/// the turn count; a new task starts from zero.
#[test]
fn turns_run_survives_continuation_and_resets_per_task() {
    let mut loop_ctrl = AgentLoop::new(10);
    loop_ctrl.record_turn();
    loop_ctrl.record_turn();
    assert_eq!(loop_ctrl.turns_run(), 2);
    loop_ctrl.reset_budget_for_resume();
    loop_ctrl.record_turn();
    assert_eq!(loop_ctrl.turns_run(), 3);
    loop_ctrl.reset_for_task();
    assert_eq!(loop_ctrl.turns_run(), 0);
}

// ---- Formal model conformance (formal/HarnessLoopBounds.lean) ----

/// The Lean model's exported table (`formal/agent_state_table.json`,
/// regenerated by `scripts/check_formal.sh --write`) equals
/// [`agent_state_table`] for every (state, event) pair, refusals included.
#[test]
fn rust_agent_state_table_equals_the_lean_model_for_every_pair() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/formal/agent_state_table.json"
    ));
    let rows: Vec<[String; 3]> =
        serde_json::from_str(text).expect("agent_state_table.json is JSON");
    let lean: std::collections::HashMap<(String, String), String> =
        rows.into_iter().map(|[s, e, t]| ((s, e), t)).collect();
    let mut accepted = 0;
    for (s, e, next) in agent_state_table() {
        let key = (s.label().to_string(), e.label().to_string());
        assert_eq!(
            next.map(|t| t.label().to_string()),
            lean.get(&key).cloned(),
            "({}, {}): Rust and the Lean model disagree",
            s.label(),
            e.label()
        );
        accepted += usize::from(next.is_some());
    }
    assert_eq!(accepted, lean.len(), "every Lean row is a Rust row");
    assert_eq!(accepted, 26);
}

fn representative(state: ModelState) -> AgentState {
    match state {
        ModelState::Planning => AgentState::Planning,
        ModelState::Executing => AgentState::Executing { step: 0 },
        ModelState::ErrorRecovery => AgentState::ErrorRecovery {
            error: "e".to_string(),
        },
        ModelState::Completed => AgentState::Completed,
        ModelState::Failed => AgentState::Failed {
            reason: "tool crashed".to_string(),
        },
        ModelState::Capped => AgentState::Failed {
            reason: MAX_ITERATIONS_STOP_REASON.to_string(),
        },
    }
}

/// `is_valid_transition` (what `transition_to` accepts) is exactly the
/// checked part of the model: a pair is accepted iff one checked event
/// leads from one to the other.
#[test]
fn transition_to_accepts_exactly_the_checked_model_events() {
    for from in ModelState::ALL {
        for to in ModelState::ALL {
            let rust = AgentLoop::is_valid_transition(&representative(from), &representative(to));
            let model = LoopEvent::ALL
                .into_iter()
                .any(|e| e.is_checked() && model_step(from, e) == Some(to));
            assert_eq!(
                rust,
                model,
                "({} -> {}): transition_to {rust}, model {model}",
                from.label(),
                to.label()
            );
        }
    }
}

/// A loop parked in `state` (built through the real methods only).
fn loop_in(state: ModelState, cap: usize) -> AgentLoop {
    let mut l = AgentLoop::new(cap);
    match state {
        ModelState::Planning => {}
        ModelState::Executing => l.transition_to(AgentState::Executing { step: 0 }).unwrap(),
        ModelState::ErrorRecovery => l
            .transition_to(AgentState::ErrorRecovery {
                error: "e".to_string(),
            })
            .unwrap(),
        ModelState::Completed => {
            l.transition_to(AgentState::Executing { step: 0 }).unwrap();
            l.transition_to(AgentState::Completed).unwrap();
        }
        ModelState::Failed => l
            .transition_to(AgentState::Failed {
                reason: "tool crashed".to_string(),
            })
            .unwrap(),
        ModelState::Capped => {
            l.transition_to(AgentState::Executing { step: 0 }).unwrap();
            while !matches!(l.next_state(), Some(AgentState::Failed { .. })) {}
        }
    }
    assert_eq!(ModelState::of(&l.state), state);
    l
}

/// Every direct write lands where the model says, from every state the
/// model allows it in.
#[test]
fn direct_state_writes_follow_the_model() {
    for s in ModelState::ALL {
        // task_reset: from every state.
        let mut l = loop_in(s, 3);
        l.reset_for_task();
        assert_eq!(
            Some(ModelState::of(&l.state)),
            model_step(s, LoopEvent::TaskReset)
        );

        // cap_trip: next_state past the cap; a stopped state stays put.
        let mut l = loop_in(s, 0);
        l.iteration = 1;
        let before = ModelState::of(&l.state);
        l.next_state();
        let after = ModelState::of(&l.state);
        match model_step(s, LoopEvent::CapTrip) {
            Some(t) => assert_eq!(after, t, "{} cap_trip", s.label()),
            None => assert_eq!(after, before, "{} is stopped: unchanged", s.label()),
        }
    }
    // extension_resume: from capped only; chain_resume: from capped (the
    // runner) or an active loop, never from a finished one.
    let mut l = loop_in(ModelState::Capped, 2);
    assert!(l.extend_budget_once().is_some());
    l.resume_after_extension();
    assert_eq!(ModelState::of(&l.state), ModelState::Executing);
    let mut l = loop_in(ModelState::Capped, 2);
    l.register_auto_continue();
    l.reset_budget_for_resume();
    assert_eq!(ModelState::of(&l.state), ModelState::Executing);
    // restore_progress: a freshly built loop.
    let mut l = AgentLoop::new(5);
    l.restore_progress(3, 0);
    assert_eq!(ModelState::of(&l.state), ModelState::Executing);
}

/// L1b: a real failure is never resumed — the oracle stops it in tests.
#[test]
#[should_panic(expected = "agent loop oracle")]
fn resuming_a_real_failure_trips_the_oracle() {
    let mut l = loop_in(ModelState::Failed, 5);
    l.register_auto_continue();
    l.reset_budget_for_resume();
}

/// L1b: `transition_to` never produces the resumable budget stop.
#[test]
fn transition_to_refuses_the_budget_stop() {
    let mut l = loop_in(ModelState::Executing, 5);
    assert!(l
        .transition_to(AgentState::Failed {
            reason: MAX_ITERATIONS_STOP_REASON.to_string(),
        })
        .is_err());
    assert_eq!(ModelState::of(&l.state), ModelState::Executing);
}

/// L1 / L6: a stopped loop is left unchanged by `next_state` — a real
/// failure is not rewritten into a (resumable) budget stop.
#[test]
fn next_state_leaves_a_stopped_loop_unchanged() {
    for s in [
        ModelState::Completed,
        ModelState::Failed,
        ModelState::Capped,
    ] {
        let mut l = loop_in(s, 1);
        let iteration = l.current_iteration();
        for _ in 0..5 {
            l.next_state();
        }
        assert_eq!(ModelState::of(&l.state), s);
        assert_eq!(l.current_iteration(), iteration);
    }
}

/// L4: the segment cap is ≤ max(2·original, original + 4) for every
/// original cap, and ≤ 2·original once original ≥ 4. The grant is at least
/// one iteration, so below 4 the 2·original bound does NOT hold (a 1-turn
/// cap extends to 5).
#[test]
fn adaptive_budget_ceiling_matches_lean_bounds() {
    for original in 0..=64 {
        let mut loop_ctrl = AgentLoop::new(original);
        for _ in 0..10 {
            let _ = loop_ctrl.extend_budget_once();
        }
        let cap = loop_ctrl.max_iterations();
        assert!(loop_ctrl.extension_ceiling_reached());
        assert!(
            cap <= (2 * original).max(original + 4),
            "{original} -> {cap}"
        );
        if original >= 4 {
            assert!(cap <= 2 * original, "{original} -> {cap}");
        }
        assert_eq!(cap, original + 4 * (original / 4).max(1));
    }
    let mut one = AgentLoop::new(1);
    while one.extend_budget_once().is_some() {}
    assert_eq!(one.max_iterations(), 5);
}

/// Run one segment to its end the way the task runner does: consume slots
/// until the cap trips, take every adaptive grant (resuming each time),
/// and stop at the final trip. Returns the segment's iterations.
fn run_segment(l: &mut AgentLoop) -> usize {
    let start = l.accumulated_iterations();
    loop {
        match l.next_state() {
            Some(AgentState::Failed { .. }) => {
                if l.extend_budget_once().is_none() {
                    break;
                }
                l.resume_after_extension();
            }
            Some(_) => {}
            None => unreachable!(),
        }
    }
    l.accumulated_iterations() - start
}

/// L5 + L6: a chain of 1 + MAX_AUTO_CONTINUES segments (the runner's
/// `auto_continue_count() >= MAX_AUTO_CONTINUES` stop) consumes exactly
/// 4 × the fully extended segment cap, ≤ 4 × max(2·original, original + 4).
#[test]
fn task_chain_cumulative_iterations_matches_lean_bounds() {
    for original in [1usize, 3, 4, 10, 100] {
        let mut l = AgentLoop::new(original);
        l.transition_to(AgentState::Executing { step: 0 }).unwrap();
        let segment_cap = original + 4 * (original / 4).max(1);
        let mut segments = 0;
        loop {
            let used = run_segment(&mut l);
            assert_eq!(used, segment_cap, "segment of a {original}-turn cap");
            segments += 1;
            if l.auto_continue_count() >= MAX_AUTO_CONTINUES {
                break;
            }
            l.register_auto_continue();
            l.reset_budget_for_resume();
        }
        assert_eq!(segments, 1 + MAX_AUTO_CONTINUES);
        let total = l.accumulated_iterations();
        assert_eq!(total, 4 * segment_cap);
        assert!(total <= 4 * (2 * original).max(original + 4));
    }
}

/// L6: from `executing` at iteration i ≤ cap, `next_state` trips after
/// exactly cap − i + 1 calls and never passes the cap; planning takes no
/// slot.
#[test]
fn next_state_trips_after_exactly_the_remaining_slots() {
    for cap in 0..=12usize {
        for start in 0..=cap {
            let mut l = AgentLoop::new(cap);
            l.transition_to(AgentState::Executing { step: 0 }).unwrap();
            l.iteration = start;
            let mut calls = 0;
            loop {
                calls += 1;
                let s = l.next_state().unwrap();
                assert!(l.current_iteration() <= cap);
                if ModelState::of(&s) == ModelState::Capped {
                    break;
                }
            }
            assert_eq!(calls, cap - start + 1, "cap {cap}, start {start}");
        }
    }
    let mut planning = AgentLoop::new(3);
    planning.next_state();
    assert_eq!(planning.current_iteration(), 0);
}
