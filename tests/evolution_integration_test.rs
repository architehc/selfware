//! Integration test for the evolution daemon.
//!
//! Validates the early-exit path of `evolve()` when the SAB runner script
//! doesn't exist — the baseline measurement should fail and the daemon
//! should return immediately with generations_run=0.

#![cfg(feature = "self-improvement")]

use selfware::evolution::daemon::{evolve, EvolutionResult};
use selfware::evolution::{
    EvolutionConfig, FitnessWeights, MutationTargets, SafetyConfig, PROTECTED_PATHS,
};
use std::path::PathBuf;

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

#[test]
fn test_evolve_early_exit_no_sab_runner() {
    // Use a temp directory as "repo root" — SAB runner script won't exist,
    // so baseline measurement fails and evolve() returns immediately.
    let tmp = tempfile::tempdir().expect("Failed to create temp dir");
    let config = test_config(1);

    let result: EvolutionResult = evolve(config, tmp.path());

    assert_eq!(
        result.generations_run, 0,
        "Should exit immediately when baseline fails"
    );
    assert!(
        result.improvements.is_empty(),
        "No improvements when baseline fails"
    );
    assert_eq!(result.final_sab_score, 0.0);
    assert_eq!(result.initial_sab_score, 0.0);
}

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
    use selfware::evolution::is_protected;
    use std::path::Path;

    // All protected paths should be detected
    for protected in PROTECTED_PATHS {
        let test_path = format!("{}test_file.rs", protected);
        assert!(
            is_protected(Path::new(&test_path)),
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

    // Min test count should be high enough to prevent test deletion attacks
    assert!(
        safety.min_test_count >= 1000,
        "Min test count should be >= 1000 to prevent test deletion"
    );

    // Binary size limit should be reasonable
    assert!(
        safety.max_binary_size_mb > 0.0 && safety.max_binary_size_mb <= 200.0,
        "Binary size limit should be between 0 and 200 MB"
    );

    // Rollback on test failure should be enabled by default
    assert!(
        safety.rollback_on_any_test_failure,
        "Rollback on test failure should be true by default"
    );

    // All PROTECTED_PATHS should be in the config
    for path in PROTECTED_PATHS {
        assert!(
            safety.protected_files.contains(&path.to_string()),
            "Protected path '{}' missing from SafetyConfig",
            path
        );
    }
}
