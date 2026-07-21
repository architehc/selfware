use super::*;

#[test]
fn test_rating_thresholds() {
    let make_result = |score: f64| SabResult {
        aggregate_score: score,
        scenario_scores: vec![],
        total_tokens_used: 0,
        wall_clock: Duration::ZERO,
        rating: match score as u32 {
            85..=100 => GenerationRating::Bloom,
            60..=84 => GenerationRating::Grow,
            30..=59 => GenerationRating::Wilt,
            _ => GenerationRating::Frost,
        },
    };

    assert_eq!(make_result(95.0).rating, GenerationRating::Bloom);
    assert_eq!(make_result(85.0).rating, GenerationRating::Bloom);
    assert_eq!(make_result(70.0).rating, GenerationRating::Grow);
    assert_eq!(make_result(45.0).rating, GenerationRating::Wilt);
    assert_eq!(make_result(20.0).rating, GenerationRating::Frost);
}

#[test]
fn test_difficulty_inference() {
    assert_eq!(infer_difficulty("easy_calculator"), Difficulty::Easy);
    assert_eq!(infer_difficulty("medium_bitset"), Difficulty::Medium);
    assert_eq!(infer_difficulty("testgen_ringbuf"), Difficulty::Medium);
    assert_eq!(infer_difficulty("hard_scheduler"), Difficulty::Hard);
    assert_eq!(infer_difficulty("expert_async_race"), Difficulty::Expert);
}

#[test]
fn test_fitness_delta_positive_improvement() {
    let weights = FitnessWeights::default();
    let baseline = FitnessMetrics {
        sab_score: 90.0,
        tokens_used: 300_000,
        token_budget: 500_000,
        wall_clock_secs: 1800.0,
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
        tokens_used: 200_000,
        ..baseline.clone()
    };
    assert!(fitness_delta(&baseline, &better, &weights) > 0.0);
}

#[test]
fn test_parse_sab_output_json_path() {
    // Output containing a report path — should try to parse as JSON file
    // (which won't exist), then fall back
    let output = "Running SAB...\nreports/sab_2024.json\nDone.";
    let result = parse_sab_output(output, Duration::from_secs(60));
    // The report file doesn't exist, so this returns an error
    assert!(result.is_err());
}

#[test]
fn test_parse_text_output_multiple_scenarios() {
    let output = "\
easy_calculator: 95/100 BLOOM
medium_bitset: 72/100 GROW
hard_scheduler: 45/100 WILT
expert_async_race: 30/100 FROST";
    let scores = parse_text_output(output).unwrap();
    assert_eq!(scores.len(), 4);
    assert_eq!(scores[0].name, "easy_calculator");
    assert_eq!(scores[0].score, 95.0);
    assert_eq!(scores[0].difficulty, Difficulty::Easy);
    assert_eq!(scores[1].name, "medium_bitset");
    assert_eq!(scores[1].difficulty, Difficulty::Medium);
    assert_eq!(scores[2].name, "hard_scheduler");
    assert_eq!(scores[2].difficulty, Difficulty::Hard);
    assert_eq!(scores[3].name, "expert_async_race");
    assert_eq!(scores[3].difficulty, Difficulty::Expert);
}

#[test]
fn test_parse_text_output_empty() {
    let scores = parse_text_output("").unwrap();
    assert!(scores.is_empty());
}

#[test]
fn test_parse_text_output_malformed() {
    let output = "easy_calculator: not_a_number/100\nrandom line\n: /100";
    let scores = parse_text_output(output).unwrap();
    // "not_a_number" can't be parsed as f64, so no score for that line
    assert!(scores.is_empty());
}

#[test]
fn test_rating_boundary_84_is_grow() {
    // 84 is in the Grow range (60..=84)
    let result = parse_sab_output("test_scenario: 84/100 OK", Duration::from_secs(10)).unwrap();
    assert_eq!(result.rating, GenerationRating::Grow);
}

#[test]
fn test_rating_boundary_85_is_bloom() {
    let result = parse_sab_output("test_scenario: 85/100 OK", Duration::from_secs(10)).unwrap();
    assert_eq!(result.rating, GenerationRating::Bloom);
}

#[test]
fn test_rating_boundary_59_is_wilt() {
    let result = parse_sab_output("test_scenario: 59/100 OK", Duration::from_secs(10)).unwrap();
    assert_eq!(result.rating, GenerationRating::Wilt);
}

#[test]
fn test_rating_boundary_29_is_frost() {
    let result = parse_sab_output("test_scenario: 29/100 OK", Duration::from_secs(10)).unwrap();
    assert_eq!(result.rating, GenerationRating::Frost);
}

#[test]
fn test_fitness_delta_negative() {
    let weights = FitnessWeights::default();
    let baseline = FitnessMetrics {
        sab_score: 90.0,
        tokens_used: 200_000,
        token_budget: 500_000,
        wall_clock_secs: 1000.0,
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
        tokens_used: 450_000,
        wall_clock_secs: 3500.0,
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
        tokens_used: 200_000,
        token_budget: 500_000,
        wall_clock_secs: 1800.0,
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
        total_tokens_used: 100_000,
        wall_clock: Duration::from_secs(600),
        rating: GenerationRating::Grow,
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
    assert_eq!(metrics.tokens_used, 100_000);
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
fn test_parse_sab_output_empty_gives_frost() {
    let result = parse_sab_output("", Duration::from_secs(10)).unwrap();
    assert_eq!(result.aggregate_score, 0.0);
    assert_eq!(result.rating, GenerationRating::Frost);
    assert!(result.scenario_scores.is_empty());
}

#[test]
fn test_infer_difficulty_refactor_prefix() {
    assert_eq!(infer_difficulty("refactor_module"), Difficulty::Medium);
}

#[test]
fn test_infer_difficulty_unknown_prefix() {
    assert_eq!(infer_difficulty("custom_scenario"), Difficulty::Hard);
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

#[test]
fn test_scenario_score_derived_fields() {
    let output = "easy_calculator: 95/100 BLOOM";
    let scores = parse_text_output(output).unwrap();
    assert_eq!(scores.len(), 1);
    assert!(scores[0].tests_passed); // 95 >= 70
    assert!(scores[0].broken_tests_fixed); // 95 >= 90
    assert!(scores[0].clean_exit); // 95 >= 10
}

#[test]
fn test_scenario_score_low_score_flags() {
    let output = "bad_scenario: 5/100 FROST";
    let scores = parse_text_output(output).unwrap();
    assert_eq!(scores.len(), 1);
    assert!(!scores[0].tests_passed); // 5 < 70
    assert!(!scores[0].broken_tests_fixed); // 5 < 90
    assert!(!scores[0].clean_exit); // 5 < 10
}
