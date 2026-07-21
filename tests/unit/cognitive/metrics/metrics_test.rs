use super::*;

#[test]
fn test_performance_snapshot_from_checkpoint() {
    let snapshot = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true);
    assert_eq!(snapshot.task_success_rate, 1.0);
    assert_eq!(snapshot.avg_iterations, 5.0);
    assert_eq!(snapshot.avg_tool_calls, 10.0);
    assert_eq!(snapshot.error_recovery_rate, 0.5);
    assert_eq!(snapshot.first_try_verification_rate, 1.0);
}

#[test]
fn test_effectiveness_delta() {
    let before = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 2, false, 10000, false);
    let after = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 2, true, 5000, true);
    let delta = after.effectiveness_delta(&before);
    assert!(delta > 0.0, "Improvement should be positive: {}", delta);
}

#[test]
fn test_performance_snapshot_with_label() {
    let snapshot = PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true)
        .with_label("pre-improve-42");
    assert_eq!(snapshot.label, Some("pre-improve-42".to_string()));
}

#[test]
fn test_performance_snapshot_failed_task() {
    let snapshot = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 0, false, 8000, false);
    assert_eq!(snapshot.task_success_rate, 0.0);
    assert_eq!(snapshot.first_try_verification_rate, 0.0);
    assert_eq!(snapshot.error_recovery_rate, 0.0);
    assert_eq!(snapshot.compilation_errors_per_task, 5.0);
}

#[test]
fn test_performance_snapshot_no_errors() {
    let snapshot = PerformanceSnapshot::from_checkpoint_data(3, 5, 0, 0, true, 2000, true);
    // No errors means recovery rate defaults to 1.0
    assert_eq!(snapshot.error_recovery_rate, 1.0);
    assert_eq!(snapshot.compilation_errors_per_task, 0.0);
}

#[test]
fn test_effectiveness_delta_regression() {
    // After is worse than before
    let before = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 3000, true);
    let after = PerformanceSnapshot::from_checkpoint_data(10, 20, 5, 0, false, 10000, false);
    let delta = after.effectiveness_delta(&before);
    assert!(delta < 0.0, "Regression should be negative: {}", delta);
}

#[test]
fn test_effectiveness_delta_identical() {
    let snap = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 5000, true);
    let delta = snap.effectiveness_delta(&snap);
    assert!(
        delta.abs() < 0.001,
        "Identical snapshots should have ~0 delta: {}",
        delta
    );
}

#[test]
fn test_performance_snapshot_serialization_roundtrip() {
    let snapshot =
        PerformanceSnapshot::from_checkpoint_data(5, 10, 2, 1, true, 5000, true).with_label("test");
    let json = serde_json::to_string(&snapshot).unwrap();
    let deserialized: PerformanceSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.avg_iterations, 5.0);
    assert_eq!(deserialized.label, Some("test".to_string()));
}

#[test]
fn test_metrics_store_roundtrip() {
    let tmp = std::env::temp_dir().join("selfware_test_metrics.jsonl");
    // Clean up from any previous run
    std::fs::remove_file(&tmp).ok();

    let store = MetricsStore::with_path(tmp.clone());

    let s1 = PerformanceSnapshot::from_checkpoint_data(5, 10, 1, 1, true, 5000, true);
    let s2 = PerformanceSnapshot::from_checkpoint_data(3, 8, 0, 0, true, 3000, true);
    store.record(&s1).unwrap();
    store.record(&s2).unwrap();

    let latest = store.latest().unwrap().unwrap();
    assert_eq!(latest.avg_iterations, 3.0);

    let trend = store.trend(10).unwrap();
    assert_eq!(trend.len(), 2);

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_metrics_store_empty() {
    let tmp = std::env::temp_dir().join("selfware_test_metrics_empty.jsonl");
    std::fs::remove_file(&tmp).ok();

    let store = MetricsStore::with_path(tmp.clone());
    assert!(store.latest().unwrap().is_none());
    assert!(store.trend(10).unwrap().is_empty());
    assert!(store.running_average(10).unwrap().is_none());
}

#[test]
fn test_metrics_store_running_average() {
    let tmp = std::env::temp_dir().join("selfware_test_metrics_avg.jsonl");
    std::fs::remove_file(&tmp).ok();

    let store = MetricsStore::with_path(tmp.clone());

    let s1 = PerformanceSnapshot::from_checkpoint_data(10, 20, 2, 1, false, 10000, true);
    let s2 = PerformanceSnapshot::from_checkpoint_data(6, 12, 0, 0, true, 6000, true);
    let s3 = PerformanceSnapshot::from_checkpoint_data(2, 4, 0, 0, true, 2000, true);
    store.record(&s1).unwrap();
    store.record(&s2).unwrap();
    store.record(&s3).unwrap();

    let avg = store.running_average(3).unwrap().unwrap();
    assert!((avg.avg_iterations - 6.0).abs() < 0.001); // (10+6+2)/3
    assert!((avg.avg_tool_calls - 12.0).abs() < 0.001); // (20+12+4)/3
    assert!((avg.avg_tokens - 6000.0).abs() < 0.001); // (10000+6000+2000)/3
    assert!(avg.label.unwrap().contains("avg_of_3"));

    // Running average of last 2 only
    let avg2 = store.running_average(2).unwrap().unwrap();
    assert!((avg2.avg_iterations - 4.0).abs() < 0.001); // (6+2)/2

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_metrics_store_trend_limited() {
    let tmp = std::env::temp_dir().join("selfware_test_metrics_trend.jsonl");
    std::fs::remove_file(&tmp).ok();

    let store = MetricsStore::with_path(tmp.clone());
    for i in 0..5 {
        let s = PerformanceSnapshot::from_checkpoint_data(i, i * 2, 0, 0, true, 1000, true);
        store.record(&s).unwrap();
    }

    // Request last 3 out of 5
    let trend = store.trend(3).unwrap();
    assert_eq!(trend.len(), 3);
    assert_eq!(trend[0].avg_iterations, 2.0);
    assert_eq!(trend[2].avg_iterations, 4.0);

    // Request more than available
    let trend_all = store.trend(100).unwrap();
    assert_eq!(trend_all.len(), 5);

    std::fs::remove_file(&tmp).ok();
}

#[test]
fn test_metrics_store_append_only() {
    let tmp = std::env::temp_dir().join("selfware_test_metrics_append.jsonl");
    std::fs::remove_file(&tmp).ok();

    let store = MetricsStore::with_path(tmp.clone());
    store
        .record(&PerformanceSnapshot::from_checkpoint_data(
            1, 1, 0, 0, true, 100, true,
        ))
        .unwrap();

    // Create a new store instance pointing to same file — should see previous data
    let store2 = MetricsStore::with_path(tmp.clone());
    store2
        .record(&PerformanceSnapshot::from_checkpoint_data(
            2, 2, 0, 0, true, 200, true,
        ))
        .unwrap();

    let trend = store2.trend(10).unwrap();
    assert_eq!(trend.len(), 2);
    assert_eq!(trend[0].avg_iterations, 1.0);
    assert_eq!(trend[1].avg_iterations, 2.0);

    std::fs::remove_file(&tmp).ok();
}
