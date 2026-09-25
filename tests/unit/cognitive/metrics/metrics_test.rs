use super::*;

/// One finished run. `verified` is the (first, final) verification result;
/// `None` means no verification ran.
fn run(
    outcome: TerminalOutcome,
    turns: usize,
    tool_calls: usize,
    errors_total: usize,
    errors_recovered: usize,
    verified: Option<(bool, bool)>,
    tokens: u64,
) -> PerformanceSnapshot {
    PerformanceSnapshot::from_terminal_run(&TerminalRunStats {
        outcome,
        failure_mode: None,
        loop_turns: turns,
        tool_calls,
        errors_total,
        errors_recovered,
        first_verification_passed: verified.map(|(first, _)| first),
        final_verification_passed: verified.map(|(_, last)| last),
        llm_total_tokens: tokens,
    })
}

fn completed(turns: usize, tool_calls: usize, tokens: u64) -> PerformanceSnapshot {
    run(
        TerminalOutcome::Completed,
        turns,
        tool_calls,
        0,
        0,
        Some((true, true)),
        tokens,
    )
}

#[test]
fn test_performance_snapshot_from_terminal_run() {
    let snapshot = run(
        TerminalOutcome::Completed,
        5,
        10,
        2,
        1,
        Some((true, true)),
        5000,
    );
    assert_eq!(snapshot.task_success_rate, 1.0);
    assert_eq!(snapshot.avg_loop_turns, 5.0);
    assert_eq!(snapshot.avg_tool_calls, 10.0);
    assert_eq!(snapshot.error_recovery_rate, 0.5);
    assert_eq!(snapshot.first_verification_pass_rate, Some(1.0));
    assert_eq!(snapshot.final_verification_pass_rate, Some(1.0));
    assert_eq!(snapshot.verification_not_run_rate, 0.0);
    assert_eq!(snapshot.avg_llm_total_tokens, 5000.0);
    assert_eq!(snapshot.runs, 1);
    assert_eq!(snapshot.schema_version, PERFORMANCE_SNAPSHOT_SCHEMA);
}

#[test]
fn test_effectiveness_delta() {
    let before = run(
        TerminalOutcome::Failed,
        10,
        20,
        5,
        2,
        Some((false, false)),
        10000,
    );
    let after = run(
        TerminalOutcome::Completed,
        5,
        10,
        2,
        2,
        Some((true, true)),
        5000,
    );
    let delta = after.effectiveness_delta(&before);
    assert!(delta > 0.0, "Improvement should be positive: {}", delta);
}

#[test]
fn test_performance_snapshot_with_label() {
    let snapshot = completed(5, 10, 5000).with_label("pre-improve-42");
    assert_eq!(snapshot.label, Some("pre-improve-42".to_string()));
}

#[test]
fn test_performance_snapshot_failed_task() {
    let snapshot = run(
        TerminalOutcome::Failed,
        10,
        20,
        5,
        0,
        Some((false, false)),
        8000,
    );
    assert_eq!(snapshot.task_success_rate, 0.0);
    assert_eq!(snapshot.first_verification_pass_rate, Some(0.0));
    assert_eq!(snapshot.error_recovery_rate, 0.0);
    assert_eq!(snapshot.unrecovered_errors_per_run, 5.0);
}

#[test]
fn test_performance_snapshot_no_errors() {
    let snapshot = completed(3, 5, 2000);
    // No errors means recovery rate defaults to 1.0
    assert_eq!(snapshot.error_recovery_rate, 1.0);
    assert_eq!(snapshot.unrecovered_errors_per_run, 0.0);
}

/// Audit item 2 (ts_notsc: "verification: not performed", snapshot said
/// test_pass_rate=1.0): a completed run whose checks never ran records the
/// check as not run — no pass rate at all, never 1.0.
#[test]
fn not_run_verification_is_recorded_as_not_run_never_as_a_pass() {
    let snapshot = run(TerminalOutcome::Completed, 9, 10, 0, 0, None, 246);
    assert_eq!(snapshot.task_success_rate, 1.0);
    assert_eq!(snapshot.first_verification_pass_rate, None);
    assert_eq!(snapshot.final_verification_pass_rate, None);
    assert_eq!(snapshot.verification_not_run_rate, 1.0);
    let json = serde_json::to_value(&snapshot).unwrap();
    assert!(json["final_verification_pass_rate"].is_null());
    assert!(
        json.get("test_pass_rate").is_none(),
        "the old success-derived field must not be written"
    );
}

/// Audit item 1: every terminal outcome other than a completion is a
/// failure in the success rate — the denominator includes them.
#[test]
fn every_non_completed_outcome_counts_against_the_success_rate() {
    for outcome in [
        TerminalOutcome::Failed,
        TerminalOutcome::Timeout,
        TerminalOutcome::BudgetStop,
        TerminalOutcome::Interrupted,
    ] {
        let snapshot = run(outcome, 4, 4, 0, 0, None, 100);
        assert_eq!(snapshot.task_success_rate, 0.0, "{outcome:?}");
        assert_eq!(snapshot.outcome, Some(outcome));
    }
    // 8 completions + 5 stops (the val083 batch shape) average to 8/13.
    let mut all: Vec<PerformanceSnapshot> = (0..8).map(|_| completed(10, 10, 100)).collect();
    for outcome in [
        TerminalOutcome::Timeout,
        TerminalOutcome::Timeout,
        TerminalOutcome::Timeout,
        TerminalOutcome::Timeout,
        TerminalOutcome::Interrupted,
    ] {
        all.push(run(outcome, 10, 10, 0, 0, None, 100));
    }
    let avg = PerformanceSnapshot::average(&all).unwrap();
    assert!((avg.task_success_rate - 8.0 / 13.0).abs() < 1e-9);
    assert_eq!(avg.runs, 13);
}

/// Pass rates average over the runs where the check RAN; the not-run share
/// is reported separately instead of diluting or inflating the rate.
#[test]
fn average_pass_rate_excludes_runs_whose_checks_did_not_run() {
    let snapshots = vec![
        run(
            TerminalOutcome::Completed,
            1,
            1,
            0,
            0,
            Some((false, true)),
            1,
        ),
        run(TerminalOutcome::Completed, 1, 1, 0, 0, None, 1),
        run(TerminalOutcome::Completed, 1, 1, 0, 0, None, 1),
        run(TerminalOutcome::Failed, 1, 1, 0, 0, Some((false, false)), 1),
    ];
    let avg = PerformanceSnapshot::average(&snapshots).unwrap();
    assert_eq!(avg.final_verification_pass_rate, Some(0.5));
    assert_eq!(avg.first_verification_pass_rate, Some(0.0));
    assert_eq!(avg.verification_not_run_rate, 0.5);

    let none_ran = vec![run(TerminalOutcome::Completed, 1, 1, 0, 0, None, 1)];
    let avg = PerformanceSnapshot::average(&none_ran).unwrap();
    assert_eq!(avg.final_verification_pass_rate, None);
}

#[test]
fn test_effectiveness_delta_regression() {
    // After is worse than before
    let before = run(
        TerminalOutcome::Completed,
        5,
        10,
        1,
        1,
        Some((true, true)),
        3000,
    );
    let after = run(
        TerminalOutcome::Failed,
        10,
        20,
        5,
        0,
        Some((false, false)),
        10000,
    );
    let delta = after.effectiveness_delta(&before);
    assert!(delta < 0.0, "Regression should be negative: {}", delta);
}

#[test]
fn test_effectiveness_delta_identical() {
    let snap = run(
        TerminalOutcome::Completed,
        5,
        10,
        1,
        1,
        Some((true, true)),
        5000,
    );
    let delta = snap.effectiveness_delta(&snap);
    assert!(
        delta.abs() < 0.001,
        "Identical snapshots should have ~0 delta: {}",
        delta
    );
}

#[test]
fn effectiveness_delta_ignores_an_unmeasured_verification_rate() {
    let measured = run(
        TerminalOutcome::Completed,
        5,
        10,
        0,
        0,
        Some((false, false)),
        5000,
    );
    let unmeasured = run(TerminalOutcome::Completed, 5, 10, 0, 0, None, 5000);
    assert!(unmeasured.effectiveness_delta(&measured).abs() < 1e-9);
    assert!(measured.effectiveness_delta(&unmeasured).abs() < 1e-9);
}

#[test]
fn test_performance_snapshot_serialization_roundtrip() {
    let snapshot = completed(5, 10, 5000).with_label("test");
    let json = serde_json::to_string(&snapshot).unwrap();
    let deserialized: PerformanceSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.avg_loop_turns, 5.0);
    assert_eq!(deserialized.outcome, Some(TerminalOutcome::Completed));
    assert_eq!(deserialized.label, Some("test".to_string()));
}

#[test]
fn test_metrics_store_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::with_path(dir.path().join("snapshots.jsonl"));

    store.record(&completed(5, 10, 5000)).unwrap();
    store.record(&completed(3, 8, 3000)).unwrap();

    let latest = store.latest().unwrap().unwrap();
    assert_eq!(latest.avg_loop_turns, 3.0);

    let trend = store.trend(10).unwrap();
    assert_eq!(trend.len(), 2);
}

#[test]
fn test_metrics_store_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::with_path(dir.path().join("snapshots.jsonl"));
    assert!(store.latest().unwrap().is_none());
    assert!(store.trend(10).unwrap().is_empty());
    assert!(store.running_average(10).unwrap().is_none());
}

#[test]
fn test_metrics_store_running_average() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::with_path(dir.path().join("snapshots.jsonl"));

    store
        .record(&run(
            TerminalOutcome::Completed,
            10,
            20,
            2,
            1,
            Some((false, true)),
            10000,
        ))
        .unwrap();
    store.record(&completed(6, 12, 6000)).unwrap();
    store.record(&completed(2, 4, 2000)).unwrap();

    let avg = store.running_average(3).unwrap().unwrap();
    assert!((avg.avg_loop_turns - 6.0).abs() < 0.001); // (10+6+2)/3
    assert!((avg.avg_tool_calls - 12.0).abs() < 0.001); // (20+12+4)/3
    assert!((avg.avg_llm_total_tokens - 6000.0).abs() < 0.001); // (10000+6000+2000)/3
    assert!(avg.label.unwrap().contains("avg_of_3"));

    // Running average of last 2 only
    let avg2 = store.running_average(2).unwrap().unwrap();
    assert!((avg2.avg_loop_turns - 4.0).abs() < 0.001); // (6+2)/2
}

#[test]
fn test_metrics_store_trend_limited() {
    let dir = tempfile::tempdir().unwrap();
    let store = MetricsStore::with_path(dir.path().join("snapshots.jsonl"));
    for i in 0..5 {
        store.record(&completed(i, i * 2, 1000)).unwrap();
    }

    // Request last 3 out of 5
    let trend = store.trend(3).unwrap();
    assert_eq!(trend.len(), 3);
    assert_eq!(trend[0].avg_loop_turns, 2.0);
    assert_eq!(trend[2].avg_loop_turns, 4.0);

    // Request more than available
    let trend_all = store.trend(100).unwrap();
    assert_eq!(trend_all.len(), 5);
}

#[test]
fn test_metrics_store_append_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshots.jsonl");
    let store = MetricsStore::with_path(path.clone());
    store.record(&completed(1, 1, 100)).unwrap();

    // Create a new store instance pointing to same file — should see previous data
    let store2 = MetricsStore::with_path(path);
    store2.record(&completed(2, 2, 200)).unwrap();

    let trend = store2.trend(10).unwrap();
    assert_eq!(trend.len(), 2);
    assert_eq!(trend[0].avg_loop_turns, 1.0);
    assert_eq!(trend[1].avg_loop_turns, 2.0);
}

/// Legacy lines (pre-fix, success-only, verbatim from the val083 audit's
/// long_review and ts_notsc snapshots) are not loaded: they are survivorship
/// samples and would re-inflate the success rate.
#[test]
fn legacy_success_only_lines_are_not_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshots.jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"timestamp":1790352541,"task_success_rate":1.0,"avg_iterations":127.0,"avg_tool_calls":171.0,"error_recovery_rate":1.0,"first_try_verification_rate":1.0,"avg_tokens":435.0,"test_pass_rate":1.0,"compilation_errors_per_task":0.0}"#,
            "\n",
            r#"{"timestamp":1790350188,"task_success_rate":1.0,"avg_iterations":9.0,"avg_tool_calls":10.0,"error_recovery_rate":1.0,"first_try_verification_rate":1.0,"avg_tokens":246.0,"test_pass_rate":1.0,"compilation_errors_per_task":0.0}"#,
            "\n",
        ),
    )
    .unwrap();
    let store = MetricsStore::with_path(path.clone());
    assert!(store.trend(10).unwrap().is_empty());

    store
        .record(&run(TerminalOutcome::Timeout, 40, 50, 0, 0, None, 900_000))
        .unwrap();
    let trend = store.trend(10).unwrap();
    assert_eq!(trend.len(), 1, "only the terminal-outcome snapshot loads");
    assert_eq!(trend[0].task_success_rate, 0.0);
    let raw = std::fs::read_to_string(&path).unwrap();
    assert_eq!(raw.lines().count(), 3, "legacy lines are kept on disk");
}
