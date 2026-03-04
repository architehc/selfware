//! Integration tests for the evolution module.
//!
//! These tests validate the evolution module's public API and component
//! interactions without invoking external tools (Docker, SAB runner, etc).

#![cfg(feature = "self-improvement")]

use selfware::evolution::daemon;
use selfware::evolution::fitness::{self, SabConfig, SabResult};
use selfware::evolution::sandbox::SandboxConfig;
use selfware::evolution::tournament::{Hypothesis, TournamentConfig};
use selfware::evolution::{
    is_protected, EvolutionConfig, FitnessWeights, GenerationRating, LlmConfig, MutationTargets,
    SafetyConfig, PROTECTED_PATHS,
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
        llm: LlmConfig::default(),
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
    assert!((0.0..=1.0).contains(&composite), "Composite: {}", composite);

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

// ══════════════════════════════════════════════════════════════
// End-to-end pipeline integration tests
// ══════════════════════════════════════════════════════════════

/// Helper: create a temp git repo with Rust source files for testing
fn setup_test_repo(name: &str) -> PathBuf {
    let tmp = std::env::temp_dir().join(format!("selfware-e2e-{}", name));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(tmp.join("src")).unwrap();

    // Init git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(&tmp)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(&tmp)
        .output()
        .unwrap();

    // Create source files
    std::fs::write(
        tmp.join("src/small.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    a - b\n}\n",
    )
    .unwrap();

    std::fs::write(
        tmp.join("src/medium.rs"),
        format!(
            "pub fn process(data: &[u8]) -> Vec<u8> {{\n    let mut result = Vec::new();\n    for &byte in data {{\n        result.push(byte.wrapping_add(1));\n    }}\n    result\n}}\n\n{}\n",
            (1..50).map(|i| format!("// padding line {}\n", i)).collect::<String>()
        ),
    )
    .unwrap();

    // Commit
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output()
        .unwrap();

    tmp
}

fn cleanup_test_repo(path: &PathBuf) {
    let _ = std::fs::remove_dir_all(path);
}

// ── E2E: read_mutation_targets → build_prompt → parse_response → apply_edits ──

#[test]
fn test_e2e_full_pipeline_with_mock_llm_response() {
    let repo = setup_test_repo("full-pipeline");

    // Step 1: Read mutation targets
    let targets = MutationTargets {
        prompt_logic: vec![PathBuf::from("src/small.rs")],
        tool_code: vec![PathBuf::from("src/medium.rs")],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = daemon::read_mutation_targets(&targets, &repo);
    assert!(!context.is_empty());
    assert!(context.contains("src/small.rs"));
    assert!(context.contains("src/medium.rs"));
    assert!(context.contains("1| pub fn add"));

    // Step 2: Build prompts
    let system_prompt = daemon::build_system_prompt(2);
    assert!(system_prompt.contains("exactly 2"));
    assert!(system_prompt.contains("search"));

    let user_prompt = daemon::build_user_prompt("", "", &context);
    assert!(user_prompt.contains("## Source Code"));
    assert!(user_prompt.contains("pub fn add"));

    // Step 3: Simulate LLM response (mock — no actual HTTP call)
    let mock_response = r#"[
        {
            "description": "Optimize add function to use wrapping_add for safety",
            "edits": [
                {
                    "file": "src/small.rs",
                    "search": "    a + b",
                    "replace": "    a.wrapping_add(b)"
                }
            ],
            "target_files": ["src/small.rs"],
            "property_test": null
        },
        {
            "description": "Use with_capacity in process function",
            "edits": [
                {
                    "file": "src/medium.rs",
                    "search": "    let mut result = Vec::new();",
                    "replace": "    let mut result = Vec::with_capacity(data.len());"
                }
            ],
            "target_files": ["src/medium.rs"],
            "property_test": null
        }
    ]"#;

    let hypotheses = daemon::parse_hypotheses_response(mock_response);
    assert_eq!(hypotheses.len(), 2);
    assert_eq!(
        hypotheses[0].target_files,
        vec![PathBuf::from("src/small.rs")]
    );
    assert_eq!(
        hypotheses[1].target_files,
        vec![PathBuf::from("src/medium.rs")]
    );

    // Step 4: Apply edits to the repo
    let applied = daemon::apply_edits(&repo, &hypotheses[0].patch);
    assert!(applied, "Edit should apply successfully");

    // Verify the file was actually changed
    let content = std::fs::read_to_string(repo.join("src/small.rs")).unwrap();
    assert!(
        content.contains("wrapping_add"),
        "File should contain the replacement: {}",
        content
    );
    assert!(
        !content.contains("a + b"),
        "File should not contain old code: {}",
        content
    );

    // Step 5: Apply second edit
    let applied2 = daemon::apply_edits(&repo, &hypotheses[1].patch);
    assert!(applied2, "Second edit should apply successfully");

    let content2 = std::fs::read_to_string(repo.join("src/medium.rs")).unwrap();
    assert!(content2.contains("Vec::with_capacity(data.len())"));

    cleanup_test_repo(&repo);
}

#[test]
fn test_e2e_pipeline_with_fuzzy_whitespace_matching() {
    let repo = setup_test_repo("fuzzy-ws");

    // File with 8-space indentation
    std::fs::write(
        repo.join("src/small.rs"),
        "fn outer() {\n        fn inner() {\n                old_call();\n        }\n}\n",
    )
    .unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "deep indent"])
        .current_dir(&repo)
        .output()
        .unwrap();

    // LLM returns search with 4-space indentation (common LLM mistake)
    let mock_response = r#"[{
        "description": "Replace old_call with new_call",
        "edits": [{
            "file": "src/small.rs",
            "search": "    fn inner() {\n            old_call();\n    }",
            "replace": "    fn inner() {\n            new_call();\n    }"
        }],
        "target_files": ["src/small.rs"],
        "property_test": null
    }]"#;

    let hypotheses = daemon::parse_hypotheses_response(mock_response);
    assert_eq!(hypotheses.len(), 1);

    // Fuzzy matching should handle the indentation mismatch
    let applied = daemon::apply_edits(&repo, &hypotheses[0].patch);
    assert!(applied, "Fuzzy whitespace matching should apply the edit");

    let content = std::fs::read_to_string(repo.join("src/small.rs")).unwrap();
    assert!(
        content.contains("new_call()"),
        "Should contain the replacement: {}",
        content
    );

    cleanup_test_repo(&repo);
}

#[test]
fn test_e2e_pipeline_safety_filter_blocks_protected_files() {
    // Simulate the safety filter that runs in the evolve() loop
    let mock_response = r#"[
        {
            "description": "Good mutation",
            "edits": [{"file": "src/tools/file.rs", "search": "x", "replace": "y"}],
            "target_files": ["src/tools/file.rs"],
            "property_test": null
        },
        {
            "description": "Bad mutation - touches evolution",
            "edits": [{"file": "src/evolution/daemon.rs", "search": "x", "replace": "y"}],
            "target_files": ["src/evolution/daemon.rs"],
            "property_test": null
        },
        {
            "description": "Bad mutation - touches safety",
            "edits": [{"file": "src/safety/sandbox.rs", "search": "x", "replace": "y"}],
            "target_files": ["src/safety/sandbox.rs"],
            "property_test": null
        }
    ]"#;

    let hypotheses = daemon::parse_hypotheses_response(mock_response);
    assert_eq!(hypotheses.len(), 3);

    // Apply safety filter (same logic as evolve())
    let valid: Vec<_> = hypotheses
        .into_iter()
        .filter(|h| !h.target_files.iter().any(|f| is_protected(f)))
        .collect();

    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].description, "Good mutation");
}

#[test]
fn test_e2e_pipeline_multifile_edit() {
    let repo = setup_test_repo("multifile");

    // Mock LLM response that edits multiple files in one hypothesis
    let mock_response = r#"[{
        "description": "Rename add to sum across files",
        "edits": [
            {"file": "src/small.rs", "search": "pub fn add(a: i32, b: i32) -> i32 {", "replace": "pub fn sum(a: i32, b: i32) -> i32 {"},
            {"file": "src/small.rs", "search": "pub fn sub(a: i32, b: i32) -> i32 {", "replace": "pub fn difference(a: i32, b: i32) -> i32 {"}
        ],
        "target_files": ["src/small.rs"],
        "property_test": null
    }]"#;

    let hypotheses = daemon::parse_hypotheses_response(mock_response);
    assert_eq!(hypotheses.len(), 1);

    let applied = daemon::apply_edits(&repo, &hypotheses[0].patch);
    assert!(applied, "Multi-edit should apply");

    let content = std::fs::read_to_string(repo.join("src/small.rs")).unwrap();
    assert!(content.contains("pub fn sum("));
    assert!(content.contains("pub fn difference("));
    assert!(!content.contains("pub fn add("));
    assert!(!content.contains("pub fn sub("));

    cleanup_test_repo(&repo);
}

#[test]
fn test_e2e_evolve_with_unreachable_endpoint() {
    // Run evolve() with an unreachable endpoint — should complete gracefully
    // with 0 improvements (LLM call fails, but no panic)
    let repo = setup_test_repo("evolve-nollm");

    let config = EvolutionConfig {
        generations: 1,
        population_size: 2,
        parallel_eval: 1,
        checkpoint_interval: 5,
        fitness_weights: FitnessWeights::default(),
        mutation_targets: MutationTargets {
            prompt_logic: vec![PathBuf::from("src/small.rs")],
            tool_code: vec![],
            cognitive: vec![],
            config_keys: vec![],
        },
        safety: SafetyConfig::default(),
        llm: LlmConfig {
            endpoint: "http://127.0.0.1:1".to_string(), // unreachable
            model: "test-model".to_string(),
            api_key: None,
            max_tokens: 1024,
            temperature: 0.0,
        },
    };

    let result = daemon::evolve(config, &repo);
    // Should complete without panicking
    assert_eq!(result.improvements.len(), 0);
    assert_eq!(result.initial_sab_score, 50.0); // synthetic baseline
    assert_eq!(result.final_sab_score, 50.0); // no improvement

    // Verify JSONL log was created
    let log_path = repo.join(".evolution-log.jsonl");
    assert!(log_path.exists(), "Should create evolution log");
    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(log_content.contains("\"event\":\"start\""));
    assert!(log_content.contains("\"event\":\"generation_start\""));

    cleanup_test_repo(&repo);
}

#[test]
fn test_e2e_evolution_history_prompt_builds_correctly() {
    let winners = vec![
        daemon::GenerationWinner {
            generation: 1,
            description: "Optimized token counting".into(),
            composite_score: 0.85,
            sab_delta: 3.0,
            token_delta: -5000.0,
            patch: String::new(),
            git_tag: None,
        },
        daemon::GenerationWinner {
            generation: 2,
            description: "Reduced allocations".into(),
            composite_score: 0.90,
            sab_delta: 5.0,
            token_delta: -3000.0,
            patch: String::new(),
            git_tag: Some("evolve-gen-2".into()),
        },
    ];

    let history = daemon::format_evolution_history(&winners);
    assert!(history.contains("Gen 2")); // Most recent first
    assert!(history.contains("Gen 1"));
    assert!(history.contains("Reduced allocations"));
    assert!(history.contains("Optimized token counting"));

    // Build a full user prompt with history
    let prompt = daemon::build_user_prompt("cpu: 50%", &history, "fn main() {}");
    assert!(prompt.contains("## Current Telemetry"));
    assert!(prompt.contains("cpu: 50%"));
    assert!(prompt.contains("Evolution History"));
    assert!(prompt.contains("## Source Code"));
}

#[test]
fn test_e2e_unified_diff_fallback() {
    let repo = setup_test_repo("diff-fallback");

    // Response with legacy unified diff format (not search-and-replace)
    let mock_response = r#"[{
        "description": "Legacy diff format test",
        "patch": "--- a/src/small.rs\n+++ b/src/small.rs\n@@ -1,3 +1,3 @@\n-pub fn add(a: i32, b: i32) -> i32 {\n+pub fn add_numbers(a: i32, b: i32) -> i32 {\n     a + b\n }\n",
        "target_files": ["src/small.rs"],
        "property_test": null
    }]"#;

    let hypotheses = daemon::parse_hypotheses_response(mock_response);
    assert_eq!(hypotheses.len(), 1);

    // The patch field is a unified diff string — apply_edits should dispatch to apply_unified_diff
    let applied = daemon::apply_edits(&repo, &hypotheses[0].patch);
    assert!(applied, "Unified diff fallback should work");

    let content = std::fs::read_to_string(repo.join("src/small.rs")).unwrap();
    assert!(content.contains("pub fn add_numbers("));

    cleanup_test_repo(&repo);
}
