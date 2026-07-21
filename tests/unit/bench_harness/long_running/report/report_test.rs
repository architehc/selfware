use super::*;

#[test]
fn test_empty_report() {
    let report = LongRunningReport::new();
    assert_eq!(report.results.len(), 0);
    assert_eq!(report.success_rate(), 0.0);
}

#[test]
fn test_report_with_results() {
    let mut report = LongRunningReport::new();
    report.add_result(ProjectResult {
        name: "test1".into(),
        status: ProjectStatus::Green,
        duration_secs: 100,
        steps: 50,
        src_lines: 200,
        compiles: true,
        tests_passed: 10,
        tests_failed: 0,
        outcome_label: "completed".into(),
    });
    report.add_result(ProjectResult {
        name: "test2".into(),
        status: ProjectStatus::Fail,
        duration_secs: 50,
        steps: 20,
        src_lines: 0,
        compiles: false,
        tests_passed: 0,
        tests_failed: 0,
        outcome_label: "failed".into(),
    });

    assert_eq!(report.results.len(), 2);
    assert_eq!(report.green_rate(), 0.5);
}

#[test]
fn test_markdown_generation() {
    let mut report = LongRunningReport::new();
    report.set_duration(3600);
    report.add_result(ProjectResult {
        name: "test1".into(),
        status: ProjectStatus::Green,
        duration_secs: 100,
        steps: 50,
        src_lines: 200,
        compiles: true,
        tests_passed: 10,
        tests_failed: 0,
        outcome_label: "completed".into(),
    });

    let md = report.to_markdown();
    assert!(md.contains("Long-Running Test Report"));
    assert!(md.contains("test1"));
    assert!(md.contains("GREEN"));
}
