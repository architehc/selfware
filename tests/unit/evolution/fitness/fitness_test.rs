use super::*;

#[test]
fn test_rating_thresholds() {
    let make_result = |score: f64| SabResult {
        aggregate_score: score,
        scenario_scores: vec![],
        total_tokens_used: Some(0),
        wall_clock: Duration::ZERO,
        rating: match score as u32 {
            85..=100 => GenerationRating::Bloom,
            60..=84 => GenerationRating::Grow,
            30..=59 => GenerationRating::Wilt,
            _ => GenerationRating::Frost,
        },
        binary_sha256: "test".to_string(),
        run_id: "test".to_string(),
        report_path: PathBuf::from("reports/sab-test/sab_report.json"),
    };

    assert_eq!(make_result(95.0).rating, GenerationRating::Bloom);
    assert_eq!(make_result(85.0).rating, GenerationRating::Bloom);
    assert_eq!(make_result(70.0).rating, GenerationRating::Grow);
    assert_eq!(make_result(45.0).rating, GenerationRating::Wilt);
    assert_eq!(make_result(20.0).rating, GenerationRating::Frost);
}

#[test]
fn test_fitness_delta_positive_improvement() {
    let weights = FitnessWeights::default();
    let baseline = FitnessMetrics {
        sab_score: 90.0,
        tokens_used: Some(300_000),
        token_budget: 500_000,
        wall_clock_secs: 1800.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_coverage_pct: 82.0,
        binary_size_mb: 15.0,
        max_binary_size_mb: 50.0,
        tests_passed: 5200,
        tests_total: 5200,
        visual_score: 0.0,
    };
    let better = FitnessMetrics {
        sab_score: 95.0,
        tokens_used: Some(200_000),
        ..baseline.clone()
    };
    assert!(fitness_delta(&baseline, &better, &weights) > 0.0);
}

#[test]
fn test_rating_boundary_84_is_grow() {
    // 84 is in the Grow range (60..=84)
    let fx = report_fixture(84.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(result.rating, GenerationRating::Grow);
}

#[test]
fn test_rating_boundary_85_is_bloom() {
    let fx = report_fixture(85.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(result.rating, GenerationRating::Bloom);
}

#[test]
fn test_rating_boundary_59_is_wilt() {
    let fx = report_fixture(59.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(result.rating, GenerationRating::Wilt);
}

#[test]
fn test_rating_boundary_29_is_frost() {
    let fx = report_fixture(29.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(result.rating, GenerationRating::Frost);
}

#[test]
fn test_fitness_delta_negative() {
    let weights = FitnessWeights::default();
    let baseline = FitnessMetrics {
        sab_score: 90.0,
        tokens_used: Some(200_000),
        token_budget: 500_000,
        wall_clock_secs: 1000.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_coverage_pct: 85.0,
        binary_size_mb: 10.0,
        max_binary_size_mb: 50.0,
        tests_passed: 5200,
        tests_total: 5200,
        visual_score: 0.0,
    };
    let worse = FitnessMetrics {
        sab_score: 60.0,
        tokens_used: Some(450_000),
        wall_clock_secs: 3500.0,
        full_evaluation_secs: None,
        test_coverage_pct: 50.0,
        binary_size_mb: 45.0,
        ..baseline.clone()
    };
    let delta = fitness_delta(&baseline, &worse, &weights);
    assert!(delta < 0.0, "Delta should be negative for worse candidate");
}

#[test]
fn test_fitness_delta_equal() {
    let weights = FitnessWeights::default();
    let metrics = FitnessMetrics {
        sab_score: 80.0,
        tokens_used: Some(200_000),
        token_budget: 500_000,
        wall_clock_secs: 1800.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_coverage_pct: 82.0,
        binary_size_mb: 15.0,
        max_binary_size_mb: 50.0,
        tests_passed: 5200,
        tests_total: 5200,
        visual_score: 0.0,
    };
    let delta = fitness_delta(&metrics, &metrics, &weights);
    assert!(
        delta.abs() < f64::EPSILON,
        "Delta should be 0 for identical metrics"
    );
}

#[test]
fn test_build_fitness_metrics_missing_binary() {
    let sab = SabResult {
        aggregate_score: 75.0,
        scenario_scores: vec![],
        total_tokens_used: Some(100_000),
        wall_clock: Duration::from_secs(600),
        rating: GenerationRating::Grow,
        binary_sha256: "test".to_string(),
        run_id: "test".to_string(),
        report_path: PathBuf::from("reports/sab-test/sab_report.json"),
    };
    let metrics = build_fitness_metrics(
        &sab,
        500_000,
        3600.0,
        std::path::Path::new("/nonexistent/binary"),
        5000,
        5200,
        50.0,
    );
    assert_eq!(metrics.binary_size_mb, 0.0); // File doesn't exist → 0.0
    assert_eq!(metrics.sab_score, 75.0);
    assert_eq!(metrics.tokens_used, Some(100_000));
    assert_eq!(metrics.tests_passed, 5000);
    assert_eq!(metrics.tests_total, 5200);
}

#[test]
fn test_sab_config_default() {
    let cfg = SabConfig::default();
    assert!(cfg.runner_script.to_str().unwrap().contains("run_full_sab"));
    assert_eq!(cfg.model, "Qwen/Qwen3-Coder-Next-FP8");
    assert_eq!(cfg.max_parallel, 6);
    assert_eq!(cfg.scenario_timeout, Duration::from_secs(3600));
    assert!(cfg.scenario_filter.is_none());
}

#[test]
fn test_parse_sab_output_empty_is_an_error_not_a_frost_score() {
    // This used to produce a Frost RATING from empty output, because the text
    // fallback returned zero scenarios and zero averaged to 0.0. A run that
    // reported nothing is not a run that scored badly.
    let fx = report_fixture(50.0);
    assert!(parse_sab_output("", Duration::from_secs(10), fx.binary()).is_err());
    // A real report still parses normally.
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(result.aggregate_score, 50.0);
    assert_eq!(result.rating, GenerationRating::Wilt);
    assert_eq!(result.scenario_scores.len(), 1);
}

#[test]
fn test_fitness_error_display() {
    let e1 = FitnessError::SabRunFailed("timeout".to_string());
    assert!(format!("{}", e1).contains("timeout"));

    let e2 = FitnessError::ReportParseFailed("bad json".to_string());
    assert!(format!("{}", e2).contains("bad json"));

    let e3 = FitnessError::BinaryNotFound(PathBuf::from("/tmp/missing"));
    assert!(format!("{}", e3).contains("/tmp/missing"));
}

// ── SAB report contract ────────────────────────────────────────────────────
//
// The runner ignored `SELFWARE_BINARY` and always built and ran its OWN
// repository's release binary, so every candidate worktree was scored using the
// baseline build and fitness could not tell the two apart. These tests use
// DISTINGUISHABLE fixture executables so scoring the wrong one necessarily
// fails, rather than silently producing a plausible number.

struct ReportFixture {
    _dir: tempfile::TempDir,
    binary: std::path::PathBuf,
    report: std::path::PathBuf,
}

impl ReportFixture {
    fn binary(&self) -> &Path {
        &self.binary
    }
    fn stdout(&self) -> String {
        format!(
            "Running SAB...\nSAB_REPORT_JSON={}\n",
            self.report.display()
        )
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

/// A fixture whose report honestly describes the fixture's own binary.
fn report_fixture(score: f64) -> ReportFixture {
    report_fixture_for(score, None)
}

/// `claimed_sha` overrides what the report SAYS it evaluated, to simulate a
/// runner that scored a different build than the caller asked for.
fn report_fixture_for(score: f64, claimed_sha: Option<&str>) -> ReportFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let binary = dir.path().join("selfware-candidate");
    let contents = format!("#!/bin/sh\necho candidate-{score}\n");
    std::fs::write(&binary, &contents).unwrap();
    let real_sha = sha256_hex(contents.as_bytes());
    let report = dir.path().join("report.json");
    let body = serde_json::json!({
        "schema": "sab-report/1",
        "run_id": "fixture-run",
        "binary": binary.to_string_lossy(),
        "binary_sha256": claimed_sha.unwrap_or(&real_sha),
        "binary_source": "SELFWARE_BINARY",
        "scenarios_expected": 1,
        "scenarios_completed": 1,
        "scenarios": [{
            "name": "test_scenario",
            "difficulty": "easy",
            "score": score,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 10,
            "tokens_used": serde_json::Value::Null,
        }],
    });
    std::fs::write(&report, serde_json::to_string(&body).unwrap()).unwrap();
    ReportFixture {
        _dir: dir,
        binary,
        report,
    }
}

#[test]
fn scoring_a_different_binary_than_requested_is_rejected() {
    // The exact defect: the runner evaluated some other build. A score that
    // describes a different executable must fail loudly, not be returned.
    let fx = report_fixture_for(95.0, Some(&sha256_hex(b"a completely different build")));
    let err = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary())
        .expect_err("a report about another binary must not yield a score");
    assert!(
        matches!(err, FitnessError::WrongBinaryEvaluated { .. }),
        "expected WrongBinaryEvaluated, got {err:?}"
    );
}

#[test]
fn a_report_describing_the_requested_binary_is_accepted() {
    let fx = report_fixture(88.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary())
        .expect("matching binary should parse");
    assert_eq!(result.aggregate_score, 88.0);
    assert_eq!(result.run_id, "fixture-run");
}

#[test]
fn output_without_a_structured_report_is_an_error() {
    // Previously this fell back to scraping `name: NN/100` out of the text and
    // inventing tests_passed from a threshold.
    let fx = report_fixture(90.0);
    let err = parse_sab_output(
        "easy_calculator: 95/100 BLOOM\nmedium_bitset: 72/100 GROW",
        Duration::from_secs(10),
        fx.binary(),
    )
    .expect_err("scores must come from the report, never from scraped text");
    assert!(matches!(err, FitnessError::ReportParseFailed(_)), "{err:?}");
}

#[test]
fn a_partial_suite_is_not_a_score() {
    // Averaging over the scenarios that finished let a candidate that crashed
    // most of the suite outscore one that completed it.
    let fx = report_fixture(100.0);
    let mut body: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fx.report).unwrap()).unwrap();
    body["scenarios_expected"] = serde_json::json!(12);
    std::fs::write(&fx.report, serde_json::to_string(&body).unwrap()).unwrap();
    let err = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary())
        .expect_err("1 of 12 scenarios is not a suite result");
    assert!(matches!(err, FitnessError::IncompleteReport(_)), "{err:?}");
}

#[test]
fn a_missing_outcome_field_is_rejected_rather_than_defaulted() {
    let fx = report_fixture(90.0);
    let mut body: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fx.report).unwrap()).unwrap();
    body["scenarios"][0]
        .as_object_mut()
        .unwrap()
        .remove("tests_passed");
    std::fs::write(&fx.report, serde_json::to_string(&body).unwrap()).unwrap();
    let err = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary())
        .expect_err("a missing outcome used to default to false");
    assert!(matches!(err, FitnessError::IncompleteReport(_)), "{err:?}");
}

#[test]
fn unobserved_tokens_stay_unknown_and_do_not_score_as_perfect_efficiency() {
    let fx = report_fixture(80.0);
    let result = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary()).unwrap();
    assert_eq!(
        result.total_tokens_used, None,
        "the runner does not observe tokens; absent is not zero"
    );

    // And the composite must not award full marks for the term it never saw.
    let weights = FitnessWeights::default();
    let unmeasured = FitnessMetrics {
        sab_score: 80.0,
        tokens_used: None,
        token_budget: 500_000,
        wall_clock_secs: 10.0,
        timeout_secs: 3600.0,
        full_evaluation_secs: None,
        test_coverage_pct: 80.0,
        binary_size_mb: 15.0,
        max_binary_size_mb: 50.0,
        tests_passed: 8,
        tests_total: 10,
        visual_score: 0.0,
    };
    let spent_everything = FitnessMetrics {
        tokens_used: Some(500_000),
        ..unmeasured.clone()
    };
    assert!(
        weights.composite(&unmeasured) < 1.0,
        "an unmeasured run must not score as if it spent nothing"
    );
    assert!(
        weights.composite(&unmeasured) > weights.composite(&spent_everything),
        "and a run that burned the whole budget should still rank below it"
    );
    assert!(!unmeasured.is_complete());
}

#[test]
fn test_darwinx_non_regression_passes_when_all_baseline_passed_scenarios_pass() {
    let make_scenario = |name: &str, passed: bool| ScenarioScore {
        name: name.to_string(),
        difficulty: Difficulty::Medium,
        score: if passed { 100.0 } else { 0.0 },
        tests_passed: passed,
        broken_tests_fixed: passed,
        clean_exit: true,
        tokens_used: Some(1000),
        duration: Duration::from_secs(1),
    };

    let baseline = SabResult {
        aggregate_score: 50.0,
        scenario_scores: vec![make_scenario("sc1", true), make_scenario("sc2", false)],
        total_tokens_used: Some(2000),
        wall_clock: Duration::from_secs(2),
        rating: GenerationRating::Wilt,
        binary_sha256: "hash".to_string(),
        run_id: "r1".to_string(),
        report_path: PathBuf::from("reports/sab-r1/sab_report.json"),
    };

    // Candidate solves sc2 as well, keeps sc1 passing
    let candidate_better = SabResult {
        aggregate_score: 100.0,
        scenario_scores: vec![make_scenario("sc1", true), make_scenario("sc2", true)],
        total_tokens_used: Some(2000),
        wall_clock: Duration::from_secs(2),
        rating: GenerationRating::Bloom,
        binary_sha256: "hash".to_string(),
        run_id: "r2".to_string(),
        report_path: PathBuf::from("reports/sab-r2/sab_report.json"),
    };

    assert!(baseline
        .check_darwinx_non_regression(&candidate_better)
        .is_ok());
}

#[test]
fn test_darwinx_non_regression_rejects_candidate_when_previously_passing_scenario_regresses() {
    let make_scenario = |name: &str, passed: bool| ScenarioScore {
        name: name.to_string(),
        difficulty: Difficulty::Medium,
        score: if passed { 100.0 } else { 0.0 },
        tests_passed: passed,
        broken_tests_fixed: passed,
        clean_exit: true,
        tokens_used: Some(1000),
        duration: Duration::from_secs(1),
    };

    let baseline = SabResult {
        aggregate_score: 50.0,
        scenario_scores: vec![make_scenario("sc1", true), make_scenario("sc2", false)],
        total_tokens_used: Some(2000),
        wall_clock: Duration::from_secs(2),
        rating: GenerationRating::Wilt,
        binary_sha256: "hash".to_string(),
        run_id: "r1".to_string(),
        report_path: PathBuf::from("reports/sab-r1/sab_report.json"),
    };

    // Candidate has higher score on sc2, but broke sc1! Even if aggregate score was identical or higher!
    let candidate_regressed = SabResult {
        aggregate_score: 50.0,
        scenario_scores: vec![make_scenario("sc1", false), make_scenario("sc2", true)],
        total_tokens_used: Some(2000),
        wall_clock: Duration::from_secs(2),
        rating: GenerationRating::Wilt,
        binary_sha256: "hash".to_string(),
        run_id: "r2".to_string(),
        report_path: PathBuf::from("reports/sab-r2/sab_report.json"),
    };

    let err = baseline
        .check_darwinx_non_regression(&candidate_regressed)
        .expect_err("regression on sc1 must fail DarwinX check");
    assert_eq!(
        err,
        DarwinXViolation::Regression {
            regressed_scenarios: vec!["sc1".to_string()]
        }
    );
}

#[test]
fn test_darwinx_non_regression_rejects_candidate_when_baseline_passed_scenario_missing_from_candidate(
) {
    let make_scenario = |name: &str, passed: bool| ScenarioScore {
        name: name.to_string(),
        difficulty: Difficulty::Medium,
        score: if passed { 100.0 } else { 0.0 },
        tests_passed: passed,
        broken_tests_fixed: passed,
        clean_exit: true,
        tokens_used: Some(1000),
        duration: Duration::from_secs(1),
    };

    let baseline = SabResult {
        aggregate_score: 100.0,
        scenario_scores: vec![make_scenario("sc1", true), make_scenario("sc2", true)],
        total_tokens_used: Some(2000),
        wall_clock: Duration::from_secs(2),
        rating: GenerationRating::Bloom,
        binary_sha256: "hash".to_string(),
        run_id: "r1".to_string(),
        report_path: PathBuf::from("reports/sab-r1/sab_report.json"),
    };

    // Candidate omits sc2 entirely (e.g. silently dropped test)
    let candidate_missing = SabResult {
        aggregate_score: 100.0,
        scenario_scores: vec![make_scenario("sc1", true)],
        total_tokens_used: Some(1000),
        wall_clock: Duration::from_secs(1),
        rating: GenerationRating::Bloom,
        binary_sha256: "hash".to_string(),
        run_id: "r2".to_string(),
        report_path: PathBuf::from("reports/sab-r2/sab_report.json"),
    };

    let err = baseline
        .check_darwinx_non_regression(&candidate_missing)
        .expect_err("missing baseline-passed scenario must fail DarwinX check");
    assert_eq!(
        err,
        DarwinXViolation::SuiteMismatch {
            missing_in_candidate: vec!["sc2".to_string()],
            unexpected_in_candidate: vec![],
        }
    );
}

#[test]
fn duplicate_scenario_names_in_sab_report_are_rejected() {
    let fx = report_fixture(90.0);
    let mut body: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fx.report).unwrap()).unwrap();
    let dup_scenario = body["scenarios"][0].clone();
    body["scenarios"].as_array_mut().unwrap().push(dup_scenario);
    body["scenarios_expected"] = serde_json::json!(body["scenarios"].as_array().unwrap().len());
    std::fs::write(&fx.report, serde_json::to_string(&body).unwrap()).unwrap();

    let err = parse_sab_output(&fx.stdout(), Duration::from_secs(10), fx.binary())
        .expect_err("duplicate scenario names must be rejected");
    assert!(
        matches!(err, FitnessError::ReportParseFailed(ref msg) if msg.contains("duplicate scenario name")),
        "unexpected error: {err:?}"
    );
}
