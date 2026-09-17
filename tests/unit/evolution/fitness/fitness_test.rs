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
        model: None,
        endpoint: None,
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
        test_pass_pct: 82.0,
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
        test_pass_pct: 85.0,
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
        test_pass_pct: 50.0,
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
        test_pass_pct: 82.0,
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
        model: None,
        endpoint: None,
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
fn test_sab_config_with_filter() {
    let cfg = SabConfig {
        scenario_filter: Some(vec!["easy_calculator".to_string()]),
        ..Default::default()
    };
    assert_eq!(
        cfg.scenario_filter.as_deref(),
        Some(&["easy_calculator".to_string()][..])
    );
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
        test_pass_pct: 80.0,
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
        model: Some("qwen".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
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
fn test_darwinx_non_regression_rejects_model_or_endpoint_mismatch() {
    let make_scenario = |name: &str| ScenarioScore {
        name: name.to_string(),
        difficulty: Difficulty::Medium,
        score: 100.0,
        tests_passed: true,
        broken_tests_fixed: false,
        clean_exit: true,
        tokens_used: Some(1000),
        duration: Duration::from_secs(1),
    };

    let baseline = SabResult {
        aggregate_score: 100.0,
        scenario_scores: vec![make_scenario("sc1")],
        total_tokens_used: Some(1000),
        wall_clock: Duration::from_secs(1),
        rating: GenerationRating::Bloom,
        binary_sha256: "hash".to_string(),
        model: Some("qwen-2.5-coder-32b".to_string()),
        endpoint: Some("http://localhost:11434/v1".to_string()),
        run_id: "r1".to_string(),
        report_path: PathBuf::from("reports/sab-r1/sab_report.json"),
    };

    // Candidate evaluated against a different model
    let mut candidate_diff_model = baseline.clone();
    candidate_diff_model.model = Some("claude-3-5-sonnet".to_string());
    let err_model = baseline
        .check_darwinx_non_regression(&candidate_diff_model)
        .expect_err("model mismatch must violate DarwinX identity");
    assert_eq!(
        err_model,
        DarwinXViolation::IdentityMismatch {
            baseline_model: Some("qwen-2.5-coder-32b".to_string()),
            candidate_model: Some("claude-3-5-sonnet".to_string()),
            baseline_endpoint: Some("http://localhost:11434/v1".to_string()),
            candidate_endpoint: Some("http://localhost:11434/v1".to_string()),
        }
    );

    // Candidate evaluated against a different endpoint
    let mut candidate_diff_endpoint = baseline.clone();
    candidate_diff_endpoint.endpoint = Some("https://api.openai.com/v1".to_string());
    let err_endpoint = baseline
        .check_darwinx_non_regression(&candidate_diff_endpoint)
        .expect_err("endpoint mismatch must violate DarwinX identity");
    assert_eq!(
        err_endpoint,
        DarwinXViolation::IdentityMismatch {
            baseline_model: Some("qwen-2.5-coder-32b".to_string()),
            candidate_model: Some("qwen-2.5-coder-32b".to_string()),
            baseline_endpoint: Some("http://localhost:11434/v1".to_string()),
            candidate_endpoint: Some("https://api.openai.com/v1".to_string()),
        }
    );
}

#[test]
fn test_darwinx_non_regression_rejects_missing_or_empty_identity() {
    let make_scenario = |name: &str| ScenarioScore {
        name: name.to_string(),
        difficulty: Difficulty::Medium,
        score: 100.0,
        tests_passed: true,
        broken_tests_fixed: true,
        clean_exit: true,
        tokens_used: Some(1000),
        duration: Duration::from_secs(1),
    };

    let base = SabResult {
        aggregate_score: 100.0,
        scenario_scores: vec![make_scenario("sc1")],
        total_tokens_used: Some(1000),
        wall_clock: Duration::from_secs(1),
        rating: GenerationRating::Bloom,
        binary_sha256: "hash".to_string(),
        model: Some("qwen-2.5-coder-32b".to_string()),
        endpoint: Some("http://localhost:11434/v1".to_string()),
        run_id: "r1".to_string(),
        report_path: PathBuf::from("reports/sab-r1/sab_report.json"),
    };

    // Both baseline and candidate having None for model must fail (finding 3: None == None bug)
    let mut no_model_base = base.clone();
    no_model_base.model = None;
    let mut no_model_cand = base.clone();
    no_model_cand.model = None;
    assert!(matches!(
        no_model_base.check_darwinx_non_regression(&no_model_cand),
        Err(DarwinXViolation::IdentityMismatch { .. })
    ));

    // Both having None for endpoint must fail
    let mut no_ep_base = base.clone();
    no_ep_base.endpoint = None;
    let mut no_ep_cand = base.clone();
    no_ep_cand.endpoint = None;
    assert!(matches!(
        no_ep_base.check_darwinx_non_regression(&no_ep_cand),
        Err(DarwinXViolation::IdentityMismatch { .. })
    ));

    // Empty model string
    let mut empty_model_cand = base.clone();
    empty_model_cand.model = Some("   ".to_string());
    assert!(matches!(
        base.check_darwinx_non_regression(&empty_model_cand),
        Err(DarwinXViolation::IdentityMismatch { .. })
    ));

    // Empty endpoint string
    let mut empty_ep_cand = base.clone();
    empty_ep_cand.endpoint = Some("".to_string());
    assert!(matches!(
        base.check_darwinx_non_regression(&empty_ep_cand),
        Err(DarwinXViolation::IdentityMismatch { .. })
    ));
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

#[test]
fn test_prune_report_dirs_retention_and_protection() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // 1. Create 12 completed prefixed directories with stepped mtimes
    for i in 1..=12 {
        let dir = reports_dir.join(format!("sab-run-{:02}", i));
        fs::create_dir(&dir).unwrap();
        File::create(dir.join(".completed")).unwrap();
        #[cfg(unix)]
        {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: (1_000_000 + i * 60) as i64,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: (1_000_000 + i * 60) as i64,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    // 2. Create non-prefixed directory (must NEVER be pruned)
    let non_prefixed = reports_dir.join("other-benchmark");
    fs::create_dir(&non_prefixed).unwrap();
    File::create(non_prefixed.join("summary.txt")).unwrap();

    // 3. Create an exempt directory via file-shaped report path (e.g. winner from older generation)
    let exempt_dir = reports_dir.join("sab-run-02"); // older, normally would be pruned
    let exempt_report_file = exempt_dir.join("sab_report.json");
    File::create(&exempt_report_file).unwrap();
    let exempt_paths = vec![exempt_report_file];

    // 4. Create an actively leased directory (in-flight run)
    // Make this non-vacuous: write .completed and set mtime to >2 hours ago,
    // so it would be pruned if not for the exclusive flock on .lease.
    let leased_dir = reports_dir.join("sab-run-in-flight");
    fs::create_dir(&leased_dir).unwrap();
    File::create(leased_dir.join(".completed")).unwrap();
    let lease_file = leased_dir.join(".lease");
    let lease_handle = File::create(&lease_file).unwrap();
    #[cfg(unix)]
    {
        let cname = std::ffi::CString::new(leased_dir.to_str().unwrap()).unwrap();
        let times = [
            nix::libc::timespec {
                tv_sec: 1_000_000,
                tv_nsec: 0,
            },
            nix::libc::timespec {
                tv_sec: 1_000_000,
                tv_nsec: 0,
            },
        ];
        unsafe {
            nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
        }
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        unsafe {
            nix::libc::flock(
                lease_handle.as_raw_fd(),
                nix::libc::LOCK_EX | nix::libc::LOCK_NB,
            );
        }
    }

    // 5. Create a symlink in reports_dir (must not crash GC)
    #[cfg(unix)]
    {
        let symlink_path = reports_dir.join("sab-symlink");
        let _ = std::os::unix::fs::symlink(&non_prefixed, &symlink_path);
    }

    // Prune keeping at most 5 newest completed unleased runs
    prune_report_dirs(reports_dir, "sab-", 5, &exempt_paths);

    // Release lease
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        unsafe {
            nix::libc::flock(lease_handle.as_raw_fd(), nix::libc::LOCK_UN);
        }
    }

    // Assertions:
    // Non-prefixed survived
    assert!(
        non_prefixed.exists(),
        "non-prefixed dir must never be pruned"
    );

    // Exempt directory survived even though it's older than retention cutoff
    assert!(
        exempt_dir.exists(),
        "exempt dir must be protected from pruning"
    );

    // Actively leased directory survived
    assert!(leased_dir.exists(), "actively leased dir must be protected");

    // Top 5 newest (sab-run-12 down to sab-run-08) must survive
    for i in 8..=12 {
        let dir = reports_dir.join(format!("sab-run-{:02}", i));
        assert!(dir.exists(), "newest dir sab-run-{:02} should survive", i);
    }

    // Oldest eligible directories (sab-run-01, sab-run-03..=sab-run-07) must be pruned
    assert!(
        !reports_dir.join("sab-run-01").exists(),
        "sab-run-01 should be pruned"
    );
    for i in 3..=7 {
        let dir = reports_dir.join(format!("sab-run-{:02}", i));
        assert!(!dir.exists(), "older dir sab-run-{:02} should be pruned", i);
    }
}

#[test]
fn test_prune_report_dirs_dotted_dir_does_not_exempt_siblings() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // Create dotted exempt directory (e.g. sab-run.v2)
    let dotted_dir = reports_dir.join("sab-run.v2");
    fs::create_dir(&dotted_dir).unwrap();
    File::create(dotted_dir.join(".completed")).unwrap();

    // Create 2 older completed directories and 1 newer
    let old_1 = reports_dir.join("sab-run-old-1");
    fs::create_dir(&old_1).unwrap();
    File::create(old_1.join(".completed")).unwrap();

    let old_2 = reports_dir.join("sab-run-old-2");
    fs::create_dir(&old_2).unwrap();
    File::create(old_2.join(".completed")).unwrap();

    let newest = reports_dir.join("sab-run-newest");
    fs::create_dir(&newest).unwrap();
    File::create(newest.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        for (dir, ts) in [
            (&old_1, 1_000_000),
            (&old_2, 1_000_100),
            (&dotted_dir, 1_000_200),
            (&newest, 1_000_300),
        ] {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    // Exempt the dotted dir: exempt_paths = [reports/sab-run.v2]
    // keep_count = 1: only 1 newest non-exempt directory should survive
    let exempt_paths = vec![dotted_dir.clone()];
    prune_report_dirs(reports_dir, "sab-", 1, &exempt_paths);

    // Dotted dir must survive because it is explicitly exempt
    assert!(dotted_dir.exists(), "dotted exempt dir must survive");
    // Newest non-exempt must survive (keep_count = 1)
    assert!(newest.exists(), "newest dir must survive");
    // Old sibling directories must BE PRUNED (and NOT protected by a broken parent heuristic)
    assert!(
        !old_1.exists(),
        "old_1 must be pruned; dotted sibling dir must not exempt entire parent reports_dir"
    );
    assert!(
        !old_2.exists(),
        "old_2 must be pruned; dotted sibling dir must not exempt entire parent reports_dir"
    );
}

#[test]
fn test_prune_report_dirs_stale_unheld_lease_is_pruned() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // 1. Create an old directory with an unheld .lease file and .completed marker
    let stale_dir = reports_dir.join("sab-stale-lease");
    fs::create_dir(&stale_dir).unwrap();
    File::create(stale_dir.join(".completed")).unwrap();
    File::create(stale_dir.join(".lease")).unwrap(); // created and immediately closed (unheld)

    // 2. Create a newer completed directory
    let fresh_dir = reports_dir.join("sab-fresh");
    fs::create_dir(&fresh_dir).unwrap();
    File::create(fresh_dir.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        for (dir, ts) in [(&stale_dir, 1_000_000), (&fresh_dir, 2_000_000)] {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    // Prune with keep_count = 1: the newer one survives, stale unheld lease directory is pruned
    prune_report_dirs(reports_dir, "sab-", 1, &[]);

    assert!(fresh_dir.exists(), "fresh dir must survive");
    assert!(
        !stale_dir.exists(),
        "stale directory with unheld .lease must be pruned"
    );
}

#[test]
fn test_prune_report_dirs_root_json_file_does_not_exempt_sibling_dirs() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // Root-level JSON file in reports_dir (e.g. summary.json)
    let root_json = reports_dir.join("summary.json");
    File::create(&root_json).unwrap();

    let old_dir = reports_dir.join("sab-old");
    fs::create_dir(&old_dir).unwrap();
    File::create(old_dir.join(".completed")).unwrap();

    let new_dir = reports_dir.join("sab-new");
    fs::create_dir(&new_dir).unwrap();
    File::create(new_dir.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        for (dir, ts) in [(&old_dir, 1_000_000), (&new_dir, 2_000_000)] {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    // Exempt the root-level JSON file. keep_count = 1.
    prune_report_dirs(reports_dir, "sab-", 1, std::slice::from_ref(&root_json));

    assert!(root_json.exists(), "root-level JSON file must survive");
    assert!(new_dir.exists(), "newest dir must survive");
    assert!(
        !old_dir.exists(),
        "old sibling dir must be pruned; root JSON file must not exempt entire reports_dir"
    );
}

#[test]
fn test_prune_report_dirs_stale_orphan_lease_pid_is_cleaned_and_pruned() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    let orphan_dir = reports_dir.join("sab-orphan");
    fs::create_dir(&orphan_dir).unwrap();
    File::create(orphan_dir.join(".completed")).unwrap();
    File::create(orphan_dir.join(".lease")).unwrap();
    // Use an unlikely PID (99999999) that does not exist
    fs::write(orphan_dir.join(".lease_pid"), "99999999\n").unwrap();

    let fresh_dir = reports_dir.join("sab-fresh");
    fs::create_dir(&fresh_dir).unwrap();
    File::create(fresh_dir.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        for (dir, ts) in [(&orphan_dir, 1_000_000), (&fresh_dir, 2_000_000)] {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    prune_report_dirs(reports_dir, "sab-", 1, &[]);

    assert!(fresh_dir.exists(), "fresh dir must survive");
    assert!(
        !orphan_dir.exists(),
        "directory with stale orphan .lease_pid must be pruned"
    );
}

#[test]
fn test_prune_completed_run_with_alive_parent_pid_and_locked_run() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // 1. Completed run whose parent daemon PID is still alive (Finding 2)
    // Must NOT escape retention just because the daemon is alive.
    let completed_dir = reports_dir.join("sab-completed-parent-alive");
    fs::create_dir(&completed_dir).unwrap();
    File::create(completed_dir.join(".completed")).unwrap();
    File::create(completed_dir.join(".lease")).unwrap(); // unheld
    fs::write(
        completed_dir.join(".lease_pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    // 2. Actively locked run (in flight) - must be protected
    let locked_dir = reports_dir.join("sab-actively-locked");
    fs::create_dir(&locked_dir).unwrap();
    let locked_file = File::create(locked_dir.join(".lease")).unwrap();
    fs::write(
        locked_dir.join(".lease_pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    #[cfg(unix)]
    unsafe {
        use std::os::fd::AsRawFd;
        let rc = nix::libc::flock(
            locked_file.as_raw_fd(),
            nix::libc::LOCK_EX | nix::libc::LOCK_NB,
        );
        assert_eq!(rc, 0, "must acquire exclusive test lock");
    }

    // 3. Newer completed run (survives under keep_count = 1)
    let fresh_dir = reports_dir.join("sab-fresh-new");
    fs::create_dir(&fresh_dir).unwrap();
    File::create(fresh_dir.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        for (dir, ts) in [
            (&completed_dir, 1_000_000),
            (&locked_dir, 1_500_000),
            (&fresh_dir, 2_000_000),
        ] {
            let cname = std::ffi::CString::new(dir.to_str().unwrap()).unwrap();
            let times = [
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
                nix::libc::timespec {
                    tv_sec: ts,
                    tv_nsec: 0,
                },
            ];
            unsafe {
                nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
            }
        }
    }

    // Prune with keep_count = 1
    prune_report_dirs(reports_dir, "sab-", 1, &[]);

    assert!(fresh_dir.exists(), "fresh completed dir must survive");
    assert!(
        locked_dir.exists(),
        "actively locked in-flight run must be protected"
    );
    assert!(
        !completed_dir.exists(),
        "completed run must be pruned even if recorded parent daemon PID remains alive"
    );

    // Release lock on cleanup
    #[cfg(unix)]
    unsafe {
        use std::os::fd::AsRawFd;
        nix::libc::flock(locked_file.as_raw_fd(), nix::libc::LOCK_UN);
    }
}

#[test]
fn test_failed_run_becomes_prune_eligible_past_age_backstop() {
    use std::fs::{self, File};
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let reports_dir = temp.path();

    // 1. Failed uncompleted run (no .completed marker) older than 2 hours whose recorded
    // daemon PID is still alive. The age backstop must prevent this from escaping retention.
    let failed_old_dir = reports_dir.join("sab-failed-old");
    fs::create_dir(&failed_old_dir).unwrap();
    File::create(failed_old_dir.join(".lease")).unwrap(); // unheld
    fs::write(
        failed_old_dir.join(".lease_pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    // 2. Active in-flight run (< 2h old) with live recorded PID: must be protected
    let inflight_recent_dir = reports_dir.join("sab-inflight-recent");
    fs::create_dir(&inflight_recent_dir).unwrap();
    File::create(inflight_recent_dir.join(".lease")).unwrap(); // unheld lock, but recent & live PID
    fs::write(
        inflight_recent_dir.join(".lease_pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    // 3. Fresh completed run (survives under keep_count = 1)
    let fresh_dir = reports_dir.join("sab-fresh-completed");
    fs::create_dir(&fresh_dir).unwrap();
    File::create(fresh_dir.join(".completed")).unwrap();

    #[cfg(unix)]
    {
        // Backdate failed_old_dir to > 2h ago
        let cname = std::ffi::CString::new(failed_old_dir.to_str().unwrap()).unwrap();
        let times = [
            nix::libc::timespec {
                tv_sec: 1_000_000,
                tv_nsec: 0,
            },
            nix::libc::timespec {
                tv_sec: 1_000_000,
                tv_nsec: 0,
            },
        ];
        unsafe {
            nix::libc::utimensat(nix::libc::AT_FDCWD, cname.as_ptr(), times.as_ptr(), 0);
        }
    }

    // Prune with keep_count = 1
    prune_report_dirs(reports_dir, "sab-", 1, &[]);

    assert!(fresh_dir.exists(), "fresh completed dir must survive");
    assert!(
        inflight_recent_dir.exists(),
        "recent in-flight run (<2h) must be protected"
    );
    assert!(
        !failed_old_dir.exists(),
        "failed run older than 2h must be pruned despite still-alive daemon PID"
    );
}
