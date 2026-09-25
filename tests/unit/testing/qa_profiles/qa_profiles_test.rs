use super::*;

#[test]
fn test_quality_grade_from_score() {
    assert_eq!(QualityGrade::from_score(100.0), QualityGrade::S);
    assert_eq!(QualityGrade::from_score(95.0), QualityGrade::S);
    assert_eq!(QualityGrade::from_score(94.9), QualityGrade::A);
    assert_eq!(QualityGrade::from_score(90.0), QualityGrade::A);
    assert_eq!(QualityGrade::from_score(80.0), QualityGrade::B);
    assert_eq!(QualityGrade::from_score(70.0), QualityGrade::C);
    assert_eq!(QualityGrade::from_score(60.0), QualityGrade::D);
    assert_eq!(QualityGrade::from_score(59.9), QualityGrade::F);
}

#[test]
fn test_compute_score_all_pass() {
    let stages = vec![
        QaStageResult {
            stage: QaStage::Syntax,
            passed: true,
            duration_ms: 100,
            output: String::new(),
            error_count: 0,
            warning_count: 0,
            not_run: None,
        },
        QaStageResult {
            stage: QaStage::Test,
            passed: true,
            duration_ms: 5000,
            output: String::new(),
            error_count: 0,
            warning_count: 0,
            not_run: None,
        },
    ];
    let weights = QaWeights::standard();
    let score = compute_score(&stages, &weights);
    // Only syntax (10) + test (30) = 40 out of 80 total weight
    assert!(score > 0.0);
}

#[test]
fn test_compute_score_all_fail() {
    let stages = vec![QaStageResult {
        stage: QaStage::Syntax,
        passed: false,
        duration_ms: 100,
        output: "error".to_string(),
        error_count: 1,
        warning_count: 0,
        not_run: None,
    }];
    let weights = QaWeights::standard();
    let score = compute_score(&stages, &weights);
    assert_eq!(score, 0.0);
}

#[test]
fn test_qa_profile_default() {
    let config = QaConfig::default();
    assert_eq!(config.profile, QaProfile::Standard);
    assert_eq!(config.auto_fix_iterations, 3);
}

/// A stage that did not run earns no credit, even with zero errors.
#[test]
fn test_compute_score_not_run_earns_no_credit() {
    let weights = QaWeights::standard();
    let not_run = vec![QaStageResult::not_run(QaStage::Lint, "no ESLint config")];
    assert_eq!(compute_score(&not_run, &weights), 0.0);
    assert!(!not_run[0].failed(), "a not-run stage is not a failure");
}
