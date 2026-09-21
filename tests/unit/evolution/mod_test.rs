use super::*;

#[test]
fn test_protected_paths() {
    assert!(is_protected(std::path::Path::new(
        "src/evolution/daemon.rs"
    )));
    assert!(is_protected(std::path::Path::new("src/safety/sandbox.rs")));
    assert!(is_protected(std::path::Path::new(
        "system_tests/projecte2e/easy_calculator/"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/agent/verification.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/agent/verification_scope.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/agent/checkpointing.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/agent/tool_dispatch/mod.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/cognitive/rsi_orchestrator.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/testing/verification.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/cognitive/self_edit.rs"
    )));
    assert!(is_protected(std::path::Path::new(
        "src/cognitive/compilation_manager.rs"
    )));
    assert!(is_protected(std::path::Path::new("Cargo.toml")));
    assert!(is_protected(std::path::Path::new("Cargo.lock")));
    assert!(is_protected(std::path::Path::new(
        ".github/workflows/ci.yml"
    )));
    assert!(is_protected(std::path::Path::new("src/main.rs")));
    assert!(is_protected(std::path::Path::new("tests/unit/mod.rs")));
    assert!(is_protected(std::path::Path::new(
        ".selfware/active_evolution.lock"
    )));

    assert!(!is_protected(std::path::Path::new("src/agent/agent.rs")));
    assert!(!is_protected(std::path::Path::new(
        "src/tools/file_edit.rs"
    )));
    assert!(!is_protected(std::path::Path::new("src/memory.rs")));
}

/// Repository instruction files are part of the shared protected-path policy:
/// the root AGENTS.md and any NESTED AGENTS.md (src/, docs/, any depth) must
/// be immutable from self-modification, and lookalike names must not match.
#[test]
fn test_repository_instruction_files_protected() {
    assert!(is_protected(std::path::Path::new("AGENTS.md")));
    assert!(is_protected(std::path::Path::new("./AGENTS.md")));
    assert!(is_protected(std::path::Path::new("src/AGENTS.md")));
    assert!(is_protected(std::path::Path::new("src/deep/AGENTS.md")));
    assert!(is_protected(std::path::Path::new("docs/AGENTS.md")));
    // Lookalike / suffix-trick names must NOT match.
    assert!(!is_protected(std::path::Path::new("AGENTS.md.bak")));
    assert!(!is_protected(std::path::Path::new("AGENTS_md")));
    assert!(!is_protected(std::path::Path::new("src/AGENTS.md.bak")));
    assert!(!is_protected(std::path::Path::new("my_AGENTS.md")));
}

#[test]
fn test_fitness_weights_default() {
    let w = FitnessWeights::default();
    let total = w.sab_score
        + w.token_efficiency
        + w.latency
        + w.test_pass_rate
        + w.binary_size
        + w.visual_quality;
    assert!(
        (total - 1.0).abs() < f64::EPSILON,
        "Weights must sum to 1.0"
    );
}

#[test]
fn test_composite_score_perfect() {
    let w = FitnessWeights::default();
    let metrics = FitnessMetrics {
        sab_score: 100.0,
        tokens_used: Some(0),
        token_budget: 500_000,
        wall_clock_secs: 0.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 100.0,
        binary_size_mb: 0.0,
        max_binary_size_mb: 50.0,
        tests_passed: 5200,
        tests_total: 5200,
        visual_score: 0.0,
    };
    let score = w.composite(&metrics);
    assert!(
        (score - 1.0).abs() < f64::EPSILON,
        "Perfect metrics should yield 1.0"
    );
}

#[test]
fn test_composite_score_ordering() {
    let w = FitnessWeights::default();
    let good = FitnessMetrics {
        sab_score: 95.0,
        tokens_used: Some(100_000),
        token_budget: 500_000,
        wall_clock_secs: 60.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 85.0,
        binary_size_mb: 10.0,
        max_binary_size_mb: 50.0,
        tests_passed: 5200,
        tests_total: 5200,
        visual_score: 0.0,
    };
    let bad = FitnessMetrics {
        sab_score: 60.0,
        tokens_used: Some(400_000),
        token_budget: 500_000,
        wall_clock_secs: 3000.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 50.0,
        binary_size_mb: 40.0,
        max_binary_size_mb: 50.0,
        tests_passed: 4000,
        tests_total: 5200,
        visual_score: 0.0,
    };
    assert!(w.composite(&good) > w.composite(&bad));
}

#[test]
fn test_generation_rating_display() {
    assert_eq!(format!("{}", GenerationRating::Bloom), "BLOOM 🌸");
    assert_eq!(format!("{}", GenerationRating::Frost), "FROST ❄️");
}

#[test]
fn test_composite_score_zero_budget() {
    let w = FitnessWeights::default();
    let metrics = FitnessMetrics {
        sab_score: 50.0,
        tokens_used: Some(10),
        token_budget: 1, // edge: budget=1, tokens_used > budget
        wall_clock_secs: 100.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 80.0,
        binary_size_mb: 10.0,
        max_binary_size_mb: 50.0,
        tests_passed: 100,
        tests_total: 100,
        visual_score: 0.0,
    };
    let score = w.composite(&metrics);
    // token ratio clamps to 1.0, so normalized_tokens = 0.0
    assert!(score >= 0.0, "Score should be non-negative");
    assert!(score <= 1.0, "Score should be <= 1.0");
}

#[test]
fn test_composite_score_custom_weights() {
    let w = FitnessWeights {
        sab_score: 1.0,
        token_efficiency: 0.0,
        latency: 0.0,
        test_pass_rate: 0.0,
        binary_size: 0.0,
        visual_quality: 0.0,
    };
    let metrics = FitnessMetrics {
        sab_score: 75.0,
        tokens_used: Some(999_999),
        token_budget: 100,
        wall_clock_secs: 99999.0,
        full_evaluation_secs: None,
        timeout_secs: 1.0,
        test_pass_pct: 0.0,
        binary_size_mb: 999.0,
        max_binary_size_mb: 1.0,
        tests_passed: 0,
        tests_total: 100,
        visual_score: 0.0,
    };
    // Only sab_score matters: 1.0 * (75/100) = 0.75
    let score = w.composite(&metrics);
    assert!(
        (score - 0.75).abs() < f64::EPSILON,
        "Score should be 0.75, got {}",
        score
    );
}

#[test]
fn test_is_protected_empty_path() {
    assert!(!is_protected(std::path::Path::new("")));
}

#[test]
fn test_is_protected_partial_match() {
    // "src/evolutionary/" contains "src/evolution" as a substring — should NOT match
    // because PROTECTED_PATHS uses "src/evolution/" with trailing slash
    assert!(!is_protected(std::path::Path::new(
        "src/evolutionary/something.rs"
    )));
    // But "src/evolution/something.rs" should match
    assert!(is_protected(std::path::Path::new(
        "src/evolution/something.rs"
    )));
}

#[test]
fn test_safety_config_default() {
    let cfg = SafetyConfig::default();
    assert_eq!(cfg.min_test_count, 5000);
    assert_eq!(cfg.max_binary_size_mb, 50.0);
    assert!(cfg.rollback_on_any_test_failure);
    assert_eq!(cfg.protected_files.len(), PROTECTED_PATHS.len());
    for p in PROTECTED_PATHS {
        assert!(
            cfg.protected_files.contains(&p.to_string()),
            "Missing protected path: {}",
            p
        );
    }
}

#[test]
fn test_generation_rating_all_variants() {
    assert_eq!(format!("{}", GenerationRating::Grow), "GROW 🌿");
    assert_eq!(format!("{}", GenerationRating::Wilt), "WILT 🥀");
}

#[test]
fn test_composite_score_worst_case() {
    let w = FitnessWeights::default();
    let metrics = FitnessMetrics {
        sab_score: 0.0,
        tokens_used: Some(500_000),
        token_budget: 500_000,
        wall_clock_secs: 3600.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 0.0,
        binary_size_mb: 50.0,
        max_binary_size_mb: 50.0,
        tests_passed: 0,
        tests_total: 5000,
        visual_score: 0.0,
    };
    let score = w.composite(&metrics);
    assert!(
        score.abs() < f64::EPSILON,
        "Worst metrics should yield 0.0, got {}",
        score
    );
}

#[test]
fn test_is_candidate_better_noise_aware() {
    use crate::evolution::daemon::{is_candidate_better, SAB_NOISE_MARGIN};

    let base = FitnessMetrics {
        sab_score: 80.0,
        tokens_used: Some(10_000),
        token_budget: 500_000,
        wall_clock_secs: 100.0,
        full_evaluation_secs: None,
        timeout_secs: 3600.0,
        test_pass_pct: 80.0,
        binary_size_mb: 20.0,
        max_binary_size_mb: 50.0,
        tests_passed: 80,
        tests_total: 100,
        visual_score: 0.0,
    };

    // 1. Candidate with SAB improvement beyond noise margin beats base even with lower composite
    let mut cand_higher_sab = base.clone();
    cand_higher_sab.sab_score = 80.0 + SAB_NOISE_MARGIN + 0.1;
    assert!(is_candidate_better(&cand_higher_sab, 0.70, &base, 0.75));

    // 2. Candidate with SAB regression beyond noise margin is worse even with higher composite
    let mut cand_lower_sab = base.clone();
    cand_lower_sab.sab_score = 80.0 - SAB_NOISE_MARGIN - 0.1;
    assert!(!is_candidate_better(&cand_lower_sab, 0.85, &base, 0.75));

    // 3. Within noise margin, composite score serves as secondary tie-breaker
    let mut cand_tied_sab = base.clone();
    cand_tied_sab.sab_score = 80.0 + 0.2; // within 0.5 noise margin
    assert!(is_candidate_better(&cand_tied_sab, 0.80, &base, 0.75));
    assert!(!is_candidate_better(&cand_tied_sab, 0.70, &base, 0.75));
}
