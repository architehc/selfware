use super::*;
use crate::bench_harness::task::EvalResult;

fn make_result(task_id: &str, success: bool, latency_ms: u64, tokens: u64) -> StreamResult {
    StreamResult {
        task_id: task_id.into(),
        stream_id: 0,
        success,
        transport_succeeded: success,
        response: "test".into(),
        prompt_tokens: tokens,
        completion_tokens: tokens * 2,
        latency_ms,
        eval: Some(EvalResult {
            score: if success { 1.0 } else { 0.0 },
            passed: success,
            details: vec![],
        }),
        error: if success { None } else { Some("error".into()) },
    }
}

#[test]
fn test_report_from_results() {
    let config = HarnessConfig::default();
    let results = vec![
        make_result("t1", true, 100, 50),
        make_result("t2", true, 200, 60),
        make_result("t3", false, 300, 70),
    ];
    let report = HarnessReport::from_results(&config, results, 10.0);
    assert_eq!(report.tasks_total, 3);
    assert_eq!(report.tasks_passed, 2);
    assert_eq!(report.tasks_failed, 1);
    assert_eq!(report.total_prompt_tokens, 180);
    assert_eq!(report.total_completion_tokens, 360);
    assert!((report.tokens_per_sec - 36.0).abs() < 0.1);
    assert_eq!(report.latency_p50_ms, 200);
}

#[test]
fn test_report_markdown() {
    let config = HarnessConfig::default();
    let results = vec![make_result("t1", true, 100, 50)];
    let report = HarnessReport::from_results(&config, results, 5.0);
    let md = report.to_markdown();
    assert!(md.contains("Benchmark Report"));
    assert!(md.contains("t1"));
    assert!(md.contains("PASS"));
}

#[test]
fn percentiles_exclude_failed_requests() {
    let config = HarnessConfig::default();
    // Two fast successes and one FAILED request with a huge latency.
    let results = vec![
        make_result("ok1", true, 100, 10),
        make_result("ok2", true, 120, 10),
        make_result("boom", false, 100_000, 0),
    ];
    let report = HarnessReport::from_results(&config, results, 1.0);
    // The 100_000ms failure must NOT pollute the latency stats.
    assert_eq!(report.latency_min_ms, 100);
    assert!(
        report.latency_max_ms <= 120,
        "max must be a success latency, got {}",
        report.latency_max_ms
    );
    assert!(
        report.latency_p99_ms <= 120,
        "p99 must exclude the failed request, got {}",
        report.latency_p99_ms
    );
}

#[test]
fn latency_counts_transport_success_even_when_eval_fails() {
    let config = HarnessConfig::default();
    // A timely response that transported fine but whose ANSWER was wrong.
    let results = vec![StreamResult {
        task_id: "t1".into(),
        stream_id: 0,
        success: false,            // eval failed (wrong answer)
        transport_succeeded: true, // but the request/response was fine
        response: "wrong".into(),
        prompt_tokens: 10,
        completion_tokens: 20,
        latency_ms: 150,
        eval: Some(EvalResult {
            score: 0.0,
            passed: false,
            details: vec![],
        }),
        error: None,
    }];
    let report = HarnessReport::from_results(&config, results, 1.0);
    // The latency MUST be counted despite the failed evaluation.
    assert_eq!(report.latency_p50_ms, 150);
    assert_eq!(report.latency_max_ms, 150);
    assert_eq!(report.latency_min_ms, 150);
}

#[test]
fn test_percentile() {
    assert_eq!(percentile(&[10, 20, 30, 40, 50], 50), 30);
    assert_eq!(percentile(&[10, 20, 30, 40, 50], 95), 50);
    assert_eq!(percentile(&[], 50), 0);
}

#[test]
fn test_format_tokens() {
    assert_eq!(format_tokens(1234), "1,234");
    assert_eq!(format_tokens(1234567), "1,234,567");
    assert_eq!(format_tokens(42), "42");
}
