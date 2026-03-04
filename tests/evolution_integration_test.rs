//! Integration tests for the evolution module.
//!
//! These tests validate the evolution module's public API and component
//! interactions without invoking external tools (Docker, SAB runner, etc).

#![cfg(feature = "self-improvement")]

use selfware::evolution::fitness::{self, SabConfig, SabResult};
use selfware::evolution::sandbox::SandboxConfig;
use selfware::evolution::tournament::{Hypothesis, TournamentConfig};
use selfware::evolution::{
    is_protected, EvolutionConfig, FitnessWeights, GenerationRating, MutationTargets, SafetyConfig,
    PROTECTED_PATHS,
};
use std::path::PathBuf;
use std::time::Duration;

fn test_config(generations: usize) -> EvolutionConfig {
    EvolutionConfig {
        generations,
        population_size: 4,
        parallel_eval: 2,
        checkpoint_interval: 5,
        fitness_weights: FitnessWeights::default(),
        mutation_targets: MutationTargets {
            config_keys: vec!["temperature".into(), "token_budget".into()],
            prompt_logic: vec![PathBuf::from("src/agent/prompt.rs")],
            tool_code: vec![PathBuf::from("src/tools/file_edit.rs")],
            cognitive: vec![],
        },
        safety: SafetyConfig::default(),
    }
}

// ── Config & safety integration tests ──

#[test]
fn test_evolution_config_construction() {
    let config = test_config(10);
    assert_eq!(config.generations, 10);
    assert_eq!(config.population_size, 4);
    assert_eq!(config.parallel_eval, 2);
    assert_eq!(config.checkpoint_interval, 5);

    // Verify fitness weights sum to 1.0
    let w = &config.fitness_weights;
    let total = w.sab_score + w.token_efficiency + w.latency + w.test_coverage + w.binary_size;
    assert!((total - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_protected_paths_prevent_self_modification() {
    // All protected paths should be detected
    for protected in PROTECTED_PATHS {
        let test_path = format!("{}test_file.rs", protected);
        assert!(
            is_protected(std::path::Path::new(&test_path)),
            "Path '{}' should be protected",
            test_path
        );
    }

    // Mutation targets should NOT be protected
    let config = test_config(1);
    for target in &config.mutation_targets.prompt_logic {
        assert!(
            !is_protected(target),
            "Mutation target '{}' should not be protected",
            target.display()
        );
    }
    for target in &config.mutation_targets.tool_code {
        assert!(
            !is_protected(target),
            "Mutation target '{}' should not be protected",
            target.display()
        );
    }
}

#[test]
fn test_safety_config_defaults_are_conservative() {
    let safety = SafetyConfig::default();

    assert!(
        safety.min_test_count >= 1000,
        "Min test count should be >= 1000 to prevent test deletion"
    );
    assert!(
        safety.max_binary_size_mb > 0.0 && safety.max_binary_size_mb <= 200.0,
        "Binary size limit should be between 0 and 200 MB"
    );
    assert!(
        safety.rollback_on_any_test_failure,
        "Rollback on test failure should be true by default"
    );
    for path in PROTECTED_PATHS {
        assert!(
            safety.protected_files.contains(&path.to_string()),
            "Protected path '{}' missing from SafetyConfig",
            path
        );
    }
}

// ── Fitness pipeline integration tests ──

#[test]
fn test_fitness_pipeline_end_to_end() {
    // Construct a SabResult → build FitnessMetrics → compute composite → compute delta
    let sab = SabResult {
        aggregate_score: 82.0,
        scenario_scores: vec![],
        total_tokens_used: 200_000,
        wall_clock: Duration::from_secs(1200),
        rating: GenerationRating::Grow,
    };

    let metrics = fitness::build_fitness_metrics(
        &sab,
        500_000,
        3600.0,
        std::path::Path::new("/nonexistent/binary"), // 0.0 MB
        5100,
        5200,
        50.0,
    );

    let weights = FitnessWeights::default();
    let composite = weights.composite(&metrics);

    // Verify composite is in valid range
    assert!(
        composite >= 0.0 && composite <= 1.0,
        "Composite: {}",
        composite
    );

    // Create a "better" candidate and verify delta is positive
    let better_sab = SabResult {
        aggregate_score: 92.0,
        total_tokens_used: 150_000,
        ..sab.clone()
    };
    let better_metrics = fitness::build_fitness_metrics(
        &better_sab,
        500_000,
        3600.0,
        std::path::Path::new("/nonexistent/binary"),
        5200,
        5200,
        50.0,
    );

    let delta = fitness::fitness_delta(&metrics, &better_metrics, &weights);
    assert!(
        delta > 0.0,
        "Better candidate should have positive delta: {}",
        delta
    );
}

#[test]
fn test_rating_lifecycle() {
    // Simulate a generation rating progression: Frost → Wilt → Grow → Bloom
    let scores = [20.0, 45.0, 70.0, 90.0];
    let expected = [
        GenerationRating::Frost,
        GenerationRating::Wilt,
        GenerationRating::Grow,
        GenerationRating::Bloom,
    ];

    for (score, expected_rating) in scores.iter().zip(expected.iter()) {
        let rating = match *score as u32 {
            85..=100 => GenerationRating::Bloom,
            60..=84 => GenerationRating::Grow,
            30..=59 => GenerationRating::Wilt,
            _ => GenerationRating::Frost,
        };
        assert_eq!(
            &rating, expected_rating,
            "Score {} should yield {:?}",
            score, expected_rating
        );
    }
}

// ── Tournament integration tests ──

#[test]
fn test_tournament_empty_hypotheses_returns_empty() {
    let config = TournamentConfig::default();
    let tmp = std::env::temp_dir();
    let results = selfware::evolution::tournament::run_tournament(vec![], &config, &tmp);
    assert!(results.is_empty());
}

#[test]
fn test_hypothesis_safety_filter() {
    // Simulate the safety filter from daemon.rs:
    // hypotheses touching protected files should be rejected
    let hypotheses = vec![
        Hypothesis {
            id: "h1".into(),
            description: "Good mutation".into(),
            patch: String::new(),
            target_files: vec![PathBuf::from("src/agent/prompt.rs")],
            property_test: None,
        },
        Hypothesis {
            id: "h2".into(),
            description: "Bad mutation - touches evolution".into(),
            patch: String::new(),
            target_files: vec![PathBuf::from("src/evolution/fitness.rs")],
            property_test: None,
        },
        Hypothesis {
            id: "h3".into(),
            description: "Bad mutation - touches safety".into(),
            patch: String::new(),
            target_files: vec![PathBuf::from("src/safety/sandbox.rs")],
            property_test: None,
        },
        Hypothesis {
            id: "h4".into(),
            description: "Bad mutation - touches system tests".into(),
            patch: String::new(),
            target_files: vec![PathBuf::from("system_tests/projecte2e/easy_calc/test.sh")],
            property_test: None,
        },
    ];

    let valid: Vec<_> = hypotheses
        .into_iter()
        .filter(|h| !h.target_files.iter().any(|f| is_protected(f)))
        .collect();

    assert_eq!(valid.len(), 1, "Only h1 should pass safety filter");
    assert_eq!(valid[0].id, "h1");
}

// ── Config defaults integration ──

#[test]
fn test_all_configs_have_sane_defaults() {
    let evo_config = test_config(0); // 0 = infinite generations
    assert_eq!(evo_config.generations, 0);

    let sab_config = SabConfig::default();
    assert_eq!(sab_config.max_parallel, 6);
    assert_eq!(sab_config.scenario_timeout, Duration::from_secs(3600));

    let sandbox_config = SandboxConfig::default();
    assert!(
        !sandbox_config.network,
        "Network should be disabled by default"
    );
    assert_eq!(sandbox_config.timeout, Duration::from_secs(3600));

    let tournament_config = TournamentConfig::default();
    assert_eq!(tournament_config.max_parallel, 4);

    // Fitness weights should all be non-negative
    let w = FitnessWeights::default();
    assert!(w.sab_score >= 0.0);
    assert!(w.token_efficiency >= 0.0);
    assert!(w.latency >= 0.0);
    assert!(w.test_coverage >= 0.0);
    assert!(w.binary_size >= 0.0);
}
