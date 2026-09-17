use super::*;

#[test]
fn contained_path_contains_and_rejects_escapes() {
    let base = Path::new("/repo");
    // Normal in-repo paths resolve inside base.
    assert_eq!(
        contained_path(base, Path::new("src/main.rs")),
        Some(PathBuf::from("/repo/src/main.rs"))
    );
    assert_eq!(
        contained_path(base, Path::new("a/../b")),
        Some(PathBuf::from("/repo/b"))
    );
    // Escapes are rejected: absolute, parent traversal, root.
    assert_eq!(contained_path(base, Path::new("/etc/passwd")), None);
    assert_eq!(contained_path(base, Path::new("../secret")), None);
    assert_eq!(contained_path(base, Path::new("../../etc/passwd")), None);
    assert_eq!(contained_path(base, Path::new("a/../../b")), None);
}

#[test]
fn test_format_empty_history() {
    let history = format_evolution_history(&[]);
    assert!(history.contains("generation 1"));
}

#[test]
fn test_format_history_with_entries() {
    let winners = vec![
        GenerationWinner {
            generation: 1,
            description: "Optimized token counting".into(),
            composite_score: 0.85,
            sab_delta: 3.0,
            token_delta: Some(-50000.0),
            patch: String::new(),
            git_tag: None,
            run_id: None,
            binary_sha256: None,
            report_path: None,
        },
        GenerationWinner {
            generation: 5,
            description: "Rewrote XML parser".into(),
            composite_score: 0.91,
            sab_delta: 5.0,
            token_delta: Some(-30000.0),
            patch: String::new(),
            git_tag: Some("evolve-gen-5".into()),
            run_id: None,
            binary_sha256: None,
            report_path: None,
        },
    ];
    let history = format_evolution_history(&winners);
    assert!(history.contains("Rewrote XML parser")); // Most recent first
    assert!(history.contains("Gen 5"));
}

#[test]
fn test_semantic_summary_grep_search() {
    // Test that grep_search can find public functions in the codebase
    // Scope the search to this module tree so the test stays fast even in
    // large worktrees with populated `target/` directories.
    #[allow(unused_imports)]
    use crate::tools::search::grep_search;
    let results = grep_search(
        "pub fn format_evolution_history",
        "src/evolution",
        true,
        10,
        0,
    );
    assert!(
        results.total_matches > 0,
        "Should find at least one match for pub fn format_evolution_history"
    );
    let match_found = results
        .matches
        .iter()
        .any(|m| m.content.contains("format_evolution_history"));
    assert!(
        match_found,
        "Should find the format_evolution_history function definition"
    );
}

#[test]
fn test_parse_test_summary_sums_results() {
    let output = "\n\
            test result: ok. 10 passed; 0 failed; 0 ignored\n\
            some other line\n\
            test result: ok. 3 passed; 1 failed; 0 ignored\n";
    let (passed, total) = parse_test_summary(output);
    assert_eq!(passed, 13);
    assert_eq!(total, 14);
}

#[test]
fn test_parse_test_summary_no_results() {
    let (passed, total) = parse_test_summary("no test output here");
    assert_eq!(passed, 0);
    assert_eq!(total, 0);
}

#[test]
fn test_metrics_from_sab_result() {
    let sab = SabResult {
        aggregate_score: 88.5,
        scenario_scores: vec![],
        total_tokens_used: Some(250_000),
        wall_clock: std::time::Duration::from_secs(1200),
        rating: GenerationRating::Bloom,
        binary_sha256: "test".to_string(),
        model: None,
        endpoint: None,
        run_id: "test".to_string(),
        report_path: PathBuf::from("reports/sab-test/sab_report.json"),
    };
    let metrics = metrics_from_sab_result(&sab, Path::new("target/release/selfware"), 50.0);
    assert_eq!(metrics.sab_score, 88.5);
    assert_eq!(metrics.tokens_used, Some(250_000));
    assert_eq!(metrics.token_budget, DEFAULT_TOKEN_BUDGET);
    assert!((metrics.wall_clock_secs - 1200.0).abs() < 0.01);
    assert_eq!(metrics.max_binary_size_mb, 50.0);
}

#[test]
fn test_format_history_caps_at_10() {
    let winners: Vec<GenerationWinner> = (1..=15)
        .map(|i| GenerationWinner {
            generation: i,
            description: format!("Mutation {}", i),
            composite_score: 0.80 + i as f64 * 0.01,
            sab_delta: 1.0,
            token_delta: Some(-1000.0),
            patch: String::new(),
            git_tag: None,
            run_id: None,
            binary_sha256: None,
            report_path: None,
        })
        .collect();
    let history = format_evolution_history(&winners);
    // Should contain gen 15 (most recent) but not gen 1 (oldest, beyond top 10)
    assert!(history.contains("Gen 15"));
    assert!(history.contains("Gen 6")); // 15..=6 is the top 10 reversed
                                        // Gen 5 should NOT appear (it's the 11th from the end)
    assert!(!history.contains("Gen 5"));
}

#[test]
fn test_format_recent_failure_history_extracts_recent_failures() {
    let tmp = tempfile::tempdir().unwrap();
    let file_path = tmp.path().join("attempts.jsonl");

    let n1 = AttemptNode {
        id: "att-1".to_string(),
        parent_id: None,
        generation: 1,
        branch_id: "b1".to_string(),
        hypothesis_id: "h1".to_string(),
        description: "Patch calculate_complexity in code_metrics.rs".to_string(),
        diff_sha256: "sha1".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 100,
        status: AttemptStatus::TestFailed,
        failure_class: Some(FailureClass::RepairableTestFailure),
        failure_reason: Some("cargo test failed".to_string()),
        output_tail: None,
        binary_sha256: None,
        created_at: "now".to_string(),
    };
    let n2 = AttemptNode {
        id: "att-2".to_string(),
        parent_id: None,
        generation: 1,
        branch_id: "b2".to_string(),
        hypothesis_id: "h2".to_string(),
        description: "Evaluated mutation".to_string(),
        diff_sha256: "sha2".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 100,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        created_at: "now".to_string(),
    };

    let content = format!(
        "{}\n{}\n",
        serde_json::to_string(&n1).unwrap(),
        serde_json::to_string(&n2).unwrap()
    );
    std::fs::write(&file_path, content).unwrap();

    let history = format_recent_failure_history(&file_path, 5);
    assert!(history.contains("Previous Failed Hypotheses"));
    assert!(history.contains("calculate_complexity"));
    assert!(!history.contains("Evaluated mutation"));
}

#[test]
fn test_apply_patch_to_worktree_nonexistent_dir() {
    let result = apply_patch_to_worktree(Path::new("/nonexistent/dir/12345"), "some patch");
    assert!(!result, "Should fail gracefully for nonexistent directory");
}

#[test]
fn test_apply_patch_to_repo_bad_patch() {
    let tmp = std::env::temp_dir().join("selfware-test-bad-patch");
    let _ = std::fs::create_dir_all(&tmp);
    // Initialize a git repo so `git apply` can run
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    let result = apply_patch_to_repo(&tmp, "this is not a valid patch format");
    assert!(!result, "Should fail gracefully for bad patch content");
    let _ = std::fs::remove_dir_all(&tmp);
}

// ─── Protected-path gate on ACTUAL patch paths (Evolution P1) ───

fn make_hypothesis(patch: &str, target_files: &[&str]) -> Hypothesis {
    Hypothesis {
        id: "hyp-test".to_string(),
        description: "test".to_string(),
        patch: patch.to_string(),
        target_files: target_files.iter().map(PathBuf::from).collect(),
        property_test: None,
    }
}

#[test]
fn test_patch_edited_paths_unified_diff() {
    let diff = "diff --git a/src/tools/file.rs b/src/tools/file.rs\n\
                    --- a/src/tools/file.rs\n\
                    +++ b/src/tools/file.rs\n\
                    @@ -1 +1 @@\n-old\n+new\n";
    assert_eq!(
        patch_edited_paths(diff),
        vec![PathBuf::from("src/tools/file.rs")]
    );
    // New file (--- is /dev/null): only the +++ path is extracted.
    let new_file = "--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1 @@\n+x\n";
    assert_eq!(
        patch_edited_paths(new_file),
        vec![PathBuf::from("src/new.rs")]
    );
    // Deletion (+++ is /dev/null): the --- path is still caught.
    let deleted = "--- a/src/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
    assert_eq!(
        patch_edited_paths(deleted),
        vec![PathBuf::from("src/old.rs")]
    );
    // Garbage yields nothing.
    assert!(patch_edited_paths("not a patch").is_empty());
}

#[test]
fn test_patch_edited_paths_search_replace_json() {
    let edits = r#"[{"file":"src/a.rs","search":"x","replace":"y"},
                        {"file":"src/b.rs","search":"p","replace":"q"}]"#;
    assert_eq!(
        patch_edited_paths(edits),
        vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]
    );
}

#[test]
fn test_hypothesis_gate_rejects_protected_diff_with_empty_targets() {
    // The review's bypass: declared target_files is EMPTY, but the diff
    // edits src/safety/ — must be refused.
    let h = make_hypothesis(
        "--- a/src/safety/checker.rs\n+++ b/src/safety/checker.rs\n@@ -1 +1 @@\n-a\n+b\n",
        &[],
    );
    assert!(hypothesis_touches_protected(&h));
}

#[test]
fn test_hypothesis_gate_rejects_protected_diff_with_benign_targets() {
    // Declared metadata says src/tools/, the patch edits src/evolution/.
    let h = make_hypothesis(
        "--- a/src/evolution/daemon.rs\n+++ b/src/evolution/daemon.rs\n@@ -1 +1 @@\n-a\n+b\n",
        &["src/tools/file.rs"],
    );
    assert!(hypothesis_touches_protected(&h));
}

#[test]
fn test_hypothesis_gate_rejects_protected_search_replace_edits() {
    let h = make_hypothesis(
        r#"[{"file":"src/safety/mod.rs","search":"x","replace":"y"}]"#,
        &[],
    );
    assert!(hypothesis_touches_protected(&h));
}

#[test]
fn test_hypothesis_gate_rejects_protected_deletion() {
    let h = make_hypothesis(
        "--- a/src/safety/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n",
        &[],
    );
    assert!(hypothesis_touches_protected(&h));
}

#[test]
fn test_hypothesis_gate_allows_benign_patch() {
    let h = make_hypothesis(
        "--- a/src/tools/file.rs\n+++ b/src/tools/file.rs\n@@ -1 +1 @@\n-a\n+b\n",
        &["src/tools/file.rs"],
    );
    assert!(!hypothesis_touches_protected(&h));
    // Declared-metadata signal still works on its own.
    let h2 = make_hypothesis(
        "--- a/src/tools/file.rs\n+++ b/src/tools/file.rs\n@@ -1 +1 @@\n-a\n+b\n",
        &["src/safety/checker.rs"],
    );
    assert!(hypothesis_touches_protected(&h2));
}

#[test]
fn test_apply_patch_to_repo_refuses_protected_patch() {
    // Final gate at the highest-blast-radius point: a patch editing
    // protected files is refused before touching the repo (no fs setup
    // needed — the refusal happens before any apply attempt).
    let diff = "--- a/src/safety/checker.rs\n+++ b/src/safety/checker.rs\n@@ -1 +1 @@\n-a\n+b\n";
    assert!(!apply_patch_to_repo(Path::new("/nonexistent"), diff));
    let edits = r#"[{"file":"src/evolution/daemon.rs","search":"x","replace":"y"}]"#;
    assert!(!apply_patch_to_repo(Path::new("/nonexistent"), edits));
}

#[test]
fn test_generation_winner_fields() {
    let winner = GenerationWinner {
        generation: 42,
        description: "Cache optimization".to_string(),
        composite_score: 0.92,
        sab_delta: 7.5,
        token_delta: Some(-25000.0),
        patch: "--- a/src/cache.rs\n+++ b/src/cache.rs".to_string(),
        git_tag: Some("evolve-gen-42".to_string()),
        run_id: Some("rsi-42".to_string()),
        binary_sha256: Some("sha256-42".to_string()),
        report_path: Some(PathBuf::from("reports/sab-42/sab_report.json")),
    };
    assert_eq!(winner.generation, 42);
    assert!(winner.sab_delta > 0.0);
    assert!(winner.token_delta.is_some_and(|d| d < 0.0));
    assert!(winner.git_tag.as_ref().unwrap().contains("42"));
}

#[test]
fn test_evolution_result_fields() {
    let result = EvolutionResult {
        aborted: None,
        generations_run: 0,
        improvements: vec![],
        final_sab_score: 0.0,
        initial_sab_score: 0.0,
        total_duration: std::time::Duration::from_secs(1),
    };
    assert_eq!(result.generations_run, 0);
    assert!(result.improvements.is_empty());
}

// ─── Winner test-count gate (evolution daemon hardening) ───

fn make_metrics(tests_passed: usize, tests_total: usize) -> FitnessMetrics {
    FitnessMetrics {
        sab_score: 100.0,
        tokens_used: Some(0),
        token_budget: DEFAULT_TOKEN_BUDGET,
        wall_clock_secs: 1.0,
        full_evaluation_secs: None,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        test_pass_pct: 100.0,
        binary_size_mb: 10.0,
        max_binary_size_mb: 50.0,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    }
}

#[test]
fn test_winner_gate_rejects_test_count_regression() {
    let baseline = make_metrics(100, 100);
    let winner = make_metrics(50, 50); // deleted half the tests
    let err = winner_test_count_gate(&baseline, &winner).unwrap_err();
    assert!(
        err.contains("winner rejected: test count regressed 100→50"),
        "unexpected rejection message: {}",
        err
    );
}

#[test]
fn test_winner_gate_allows_equal_or_more_tests() {
    let baseline = make_metrics(100, 100);
    // Same count passes.
    assert!(winner_test_count_gate(&baseline, &make_metrics(95, 100)).is_ok());
    // More tests passes.
    assert!(winner_test_count_gate(&baseline, &make_metrics(101, 101)).is_ok());
    // Zero-baseline (synthetic) never blocks the first generation.
    let synthetic = make_metrics(0, 0);
    assert!(winner_test_count_gate(&synthetic, &make_metrics(1, 1)).is_ok());
}

fn make_sab(scores: Vec<(&str, f64, bool)>) -> crate::evolution::fitness::SabResult {
    use crate::evolution::fitness::{Difficulty, SabResult, ScenarioScore};
    use std::time::Duration;

    SabResult {
        aggregate_score: 80.0,
        scenario_scores: scores
            .into_iter()
            .map(|(name, score, passed)| ScenarioScore {
                name: name.to_string(),
                difficulty: Difficulty::Medium,
                score,
                tests_passed: passed,
                broken_tests_fixed: false,
                clean_exit: true,
                tokens_used: None,
                duration: Duration::from_secs(1),
            })
            .collect(),
        total_tokens_used: None,
        wall_clock: Duration::from_secs(1),
        rating: GenerationRating::Grow,
        binary_sha256: "dummy".to_string(),
        model: Some("qwen-test".to_string()),
        endpoint: Some("http://localhost:11434".to_string()),
        run_id: "test".to_string(),
        report_path: std::path::PathBuf::from("reports/sab-test/sab_report.json"),
    }
}

#[test]
fn test_winner_darwinx_gate_enforcement() {
    let base = make_sab(vec![("sc1", 90.0, true), ("sc2", 80.0, true)]);

    // 1. Regressed candidate (pass regression on sc1: true -> false) must be rejected
    let cand_regressed = make_sab(vec![("sc1", 70.0, false), ("sc2", 85.0, true)]);
    let err = winner_darwinx_gate(Some(&base), Some(&cand_regressed)).unwrap_err();
    assert!(err.contains("DarwinX non-regression check failed"));
    assert!(err.contains("sc1"));

    // 2. Candidate missing a scenario must be rejected
    let cand_missing = make_sab(vec![("sc1", 95.0, true)]);
    let err = winner_darwinx_gate(Some(&base), Some(&cand_missing)).unwrap_err();
    assert!(err.contains("DarwinX non-regression check failed"));
    assert!(err.contains("sc2"));

    // 3. Candidate with equal or better scores must pass
    let cand_better = make_sab(vec![("sc1", 90.0, true), ("sc2", 95.0, true)]);
    assert!(winner_darwinx_gate(Some(&base), Some(&cand_better)).is_ok());

    // 4. SAB evidence contract:
    // Both absent is allowed (compile-only mode).
    assert!(winner_darwinx_gate(None, None).is_ok());

    // Exactly one absent is rejected fail-closed.
    let err_cand_none = winner_darwinx_gate(Some(&base), None).unwrap_err();
    assert!(err_cand_none.contains("baseline has SAB benchmark evidence but candidate has none"));

    let err_base_none = winner_darwinx_gate(None, Some(&cand_better)).unwrap_err();
    assert!(err_base_none.contains("candidate has SAB benchmark evidence but baseline has none"));
}

#[test]
fn test_evaluate_candidate_promotion_gates() {
    let base_metrics = make_metrics(100, 100);
    let cand_metrics_ok = make_metrics(100, 100);
    let cand_metrics_fewer_tests = make_metrics(50, 50);

    let base_sab = make_sab(vec![("sc1", 90.0, true), ("sc2", 80.0, true)]);
    let cand_sab_ok = make_sab(vec![("sc1", 90.0, true), ("sc2", 85.0, true)]);
    let cand_sab_regressed = make_sab(vec![("sc1", 70.0, false), ("sc2", 85.0, true)]);

    // 1. Score does not exceed baseline -> Reject
    let decision = evaluate_candidate_promotion(
        0.8,
        0.75,
        Some(&base_sab),
        Some(&cand_sab_ok),
        &base_metrics,
        &cand_metrics_ok,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("does not exceed baseline")
    ));

    // 2. DarwinX regression -> Reject
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        Some(&base_sab),
        Some(&cand_sab_regressed),
        &base_metrics,
        &cand_metrics_ok,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("DarwinX non-regression check failed")
    ));

    // 3. Test count regression -> Reject
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        Some(&base_sab),
        Some(&cand_sab_ok),
        &base_metrics,
        &cand_metrics_fewer_tests,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("test count regressed")
    ));

    // 4. Valid winner with capability gain -> Promote
    let mut cand_metrics_capability = cand_metrics_ok.clone();
    cand_metrics_capability.sab_score = 102.0; // capability gain exceeds SAB_NOISE_MARGIN
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        Some(&base_sab),
        Some(&cand_sab_ok),
        &base_metrics,
        &cand_metrics_capability,
    );
    assert_eq!(decision, PromotionDecision::Promote);

    // 5a. Tied SAB and no token improvement -> Reject (noise-aware: latency cannot drive promotion)
    let cand_metrics_tied_sab = make_metrics(100, 100);
    let decision =
        evaluate_candidate_promotion(0.8, 0.85, None, None, &base_metrics, &cand_metrics_tied_sab);
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("latency and binary size are tie-breakers")
    ));

    // 5b. Tied SAB with measured token efficiency gain -> Promote
    let mut cand_metrics_token_gain = make_metrics(100, 100);
    cand_metrics_token_gain.tokens_used = Some(800);
    let mut base_metrics_tokens = make_metrics(100, 100);
    base_metrics_tokens.tokens_used = Some(1000);
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        None,
        None,
        &base_metrics_tokens,
        &cand_metrics_token_gain,
    );
    assert_eq!(decision, PromotionDecision::Promote);

    // 6. Asymmetric SAB evidence -> Reject
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        Some(&base_sab),
        None,
        &base_metrics,
        &cand_metrics_ok,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("baseline has SAB benchmark evidence but candidate has none")
    ));

    // 7. SAB regression beyond noise margin -> Reject even if composite score is higher
    let mut cand_metrics_sab_regressed = cand_metrics_ok.clone();
    cand_metrics_sab_regressed.sab_score = 99.0; // regressed from 100 by 1.0 (> 0.5 margin)
    let decision = evaluate_candidate_promotion(
        0.80,
        0.95, // higher composite
        None,
        None,
        &base_metrics,
        &cand_metrics_sab_regressed,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("regressed below baseline")
    ));

    // 8. SAB improvement beyond noise margin -> Promote even if composite score is slightly lower
    let mut cand_metrics_sab_improved = cand_metrics_ok.clone();
    cand_metrics_sab_improved.sab_score = 105.0; // improved by 5.0 (> 0.5 margin)
    let decision = evaluate_candidate_promotion(
        0.85,
        0.80, // slightly lower composite due to running more tests
        None,
        None,
        &base_metrics,
        &cand_metrics_sab_improved,
    );
    assert_eq!(decision, PromotionDecision::Promote);
}

#[test]
fn test_parse_hypotheses_valid_json() {
    let json = r#"[
            {
                "description": "Cache token count lookups",
                "patch": "--- a/src/token.rs\n+++ b/src/token.rs\n@@ -1,3 +1,4 @@\n+use std::collections::HashMap;\n fn count() {}",
                "target_files": ["src/token.rs"],
                "property_test": null
            },
            {
                "description": "Optimize string allocation",
                "patch": "--- a/src/alloc.rs\n+++ b/src/alloc.rs\n@@ -1 +1 @@\n-let s = String::new();\n+let s = String::with_capacity(64);",
                "target_files": ["src/alloc.rs"],
                "property_test": "assert!(true)"
            }
        ]"#;
    let hypotheses = parse_hypotheses_response(json);
    assert_eq!(hypotheses.len(), 2);
    assert_eq!(hypotheses[0].description, "Cache token count lookups");
    assert_eq!(hypotheses[0].id, "hyp-0");
    assert_eq!(
        hypotheses[0].target_files,
        vec![PathBuf::from("src/token.rs")]
    );
    assert!(hypotheses[0].property_test.is_none());
    assert_eq!(hypotheses[1].description, "Optimize string allocation");
    assert_eq!(hypotheses[1].id, "hyp-1");
    assert_eq!(
        hypotheses[1].property_test.as_deref(),
        Some("assert!(true)")
    );
}

#[test]
fn test_parse_hypotheses_markdown_fences() {
    let response = r#"Here are my suggestions:

```json
[
    {
        "description": "Use Vec::with_capacity",
        "patch": "--- a/src/lib.rs\n+++ b/src/lib.rs",
        "target_files": ["src/lib.rs"],
        "property_test": null
    }
]
```

These changes should improve performance."#;
    let hypotheses = parse_hypotheses_response(response);
    assert_eq!(hypotheses.len(), 1);
    assert_eq!(hypotheses[0].description, "Use Vec::with_capacity");
}

#[test]
fn test_parse_hypotheses_malformed() {
    let malformed = "This is not JSON at all, just some text.";
    let hypotheses = parse_hypotheses_response(malformed);
    assert!(hypotheses.is_empty());
}

#[test]
fn test_parse_hypotheses_partial_objects() {
    // Missing required fields — should be filtered out
    let json = r#"[
            {"description": "Good one", "patch": "diff", "target_files": ["a.rs"], "property_test": null},
            {"description": "Missing patch"},
            {"patch": "diff but no desc"}
        ]"#;
    let hypotheses = parse_hypotheses_response(json);
    assert_eq!(hypotheses.len(), 1);
    assert_eq!(hypotheses[0].description, "Good one");
}

#[test]
fn test_build_system_prompt_contains_population() {
    let prompt = build_system_prompt(5);
    assert!(prompt.contains("exactly 5"));
}

#[test]
fn test_build_user_prompt_shape() {
    let prompt = build_user_prompt(
        "cpu: 80%",
        "Gen 1: improved X",
        "```rust\nfn main() {}\n```",
    );
    assert!(prompt.contains("## Current Telemetry"));
    assert!(prompt.contains("cpu: 80%"));
    assert!(prompt.contains("Gen 1: improved X"));
    assert!(prompt.contains("## Source Code"));
    assert!(prompt.contains("fn main()"));
}

#[test]
fn test_build_user_prompt_empty_telemetry() {
    let prompt = build_user_prompt("", "some history", "source");
    assert!(!prompt.contains("## Current Telemetry"));
    assert!(prompt.contains("some history"));
}

#[test]
fn test_llm_config_default() {
    let cfg = LlmConfig::default();
    assert_eq!(cfg.max_tokens, 16384);
    assert!((cfg.temperature - 0.7).abs() < f32::EPSILON);
    assert!(cfg.api_key.is_none());
    assert!(!cfg.endpoint.is_empty());
    assert!(!cfg.model.is_empty());
}

#[test]
fn test_extract_json_array_plain() {
    let input = r#"[{"a": 1}]"#;
    let result = extract_json_array(input);
    assert_eq!(result.unwrap(), r#"[{"a": 1}]"#);
}

#[test]
fn test_extract_json_array_with_preamble() {
    let input = "Here is the result:\n[{\"x\": 1}]";
    let result = extract_json_array(input);
    assert_eq!(result.unwrap(), r#"[{"x": 1}]"#);
}

#[test]
fn test_extract_json_array_nested() {
    let input = r#"[{"a": [1, 2]}, {"b": 3}]"#;
    let result = extract_json_array(input);
    assert_eq!(result.unwrap(), input);
}

#[test]
fn test_extract_json_array_none() {
    assert!(extract_json_array("no array here").is_none());
}

#[test]
fn test_add_line_numbers() {
    let src = "fn main() {\n    println!(\"hello\");\n}\n";
    let numbered = add_line_numbers(src);
    assert!(numbered.contains("1| fn main() {"));
    assert!(numbered.contains("2|     println!(\"hello\");"));
    assert!(numbered.contains("3| }"));
}

#[test]
fn test_add_line_numbers_width() {
    // 100+ lines should get 3-digit width
    let src: String = (1..=150).map(|i| format!("line {}\n", i)).collect();
    let numbered = add_line_numbers(&src);
    assert!(numbered.contains("  1| line 1"));
    assert!(numbered.contains("150| line 150"));
}

#[test]
fn test_truncate_to_line_boundary() {
    let text = "line one\nline two\nline three\nline four\n";
    let trunc = truncate_to_line_boundary(text, 20);
    assert_eq!(trunc, "line one\nline two");
}

#[test]
fn test_truncate_to_line_boundary_fits() {
    let text = "short";
    assert_eq!(truncate_to_line_boundary(text, 100), "short");
}

#[test]
fn test_parse_hypotheses_edits_format() {
    let json = r#"[
            {
                "description": "Optimize token counting",
                "edits": [
                    {"file": "src/token.rs", "search": "old_code()", "replace": "new_code()"}
                ],
                "target_files": ["src/token.rs"],
                "property_test": null
            }
        ]"#;
    let hypotheses = parse_hypotheses_response(json);
    assert_eq!(hypotheses.len(), 1);
    assert_eq!(hypotheses[0].description, "Optimize token counting");
    // patch should contain the serialized edits JSON
    assert!(hypotheses[0].patch.contains("old_code()"));
    assert!(hypotheses[0].patch.contains("new_code()"));
}

#[test]
fn test_apply_search_replace_basic() {
    let tmp = std::env::temp_dir().join("selfware-test-sr");
    let _ = std::fs::create_dir_all(&tmp);
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old_func() {\n    println!(\"hello\");\n}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "test.rs",
        "search": "fn old_func()",
        "replace": "fn new_func()"
    })];

    let result = apply_search_replace(&tmp, &edits);
    assert!(result);

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new_func()"));
    assert!(!content.contains("fn old_func()"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_search_replace_not_found() {
    let tmp = std::env::temp_dir().join("selfware-test-sr-notfound");
    let _ = std::fs::create_dir_all(&tmp);
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn foo() {}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "test.rs",
        "search": "fn nonexistent()",
        "replace": "fn bar()"
    })];

    let result = apply_search_replace(&tmp, &edits);
    assert!(!result);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_fuzzy_find_and_replace_exact() {
    let content = "fn foo() {\n    old_code();\n}\n";
    let result = fuzzy_find_and_replace(content, "    old_code();", "    new_code();");
    assert!(result.is_some());
    assert!(result.unwrap().contains("new_code()"));
}

#[test]
fn test_fuzzy_find_and_replace_indent_mismatch() {
    // File has 8-space indent, search has 4-space indent
    let content = "fn foo() {\n        old_code();\n}\n";
    let result = fuzzy_find_and_replace(content, "    old_code();", "    new_code();");
    assert!(result.is_some());
    let r = result.unwrap();
    // Should preserve the file's 8-space indent
    assert!(r.contains("        new_code();"), "got: {}", r);
}

#[test]
fn test_fuzzy_find_and_replace_multiline() {
    let content = "    fn foo() {\n        let x = 1;\n        let y = 2;\n    }\n";
    let search = "let x = 1;\n  let y = 2;";
    let replace = "let x = 10;\n  let y = 20;";
    let result = fuzzy_find_and_replace(content, search, replace);
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.contains("let x = 10;"), "got: {}", r);
    assert!(r.contains("let y = 20;"), "got: {}", r);
}

#[test]
fn test_build_system_prompt_mentions_line_numbers() {
    let prompt = build_system_prompt(4);
    assert!(prompt.contains("line numbers"));
    assert!(prompt.contains("exactly 4"));
    assert!(prompt.contains("search"));
    assert!(prompt.contains("replace"));
}

#[test]
fn test_sanitize_patch_strips_line_numbers() {
    let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -10,3 +10,3 @@
  10| fn foo() {
- 11|     old_code();
+ 11|     new_code();
  12| }
";
    let clean = sanitize_patch(patch);
    assert!(clean.contains(" fn foo() {\n"));
    assert!(clean.contains("-    old_code();\n"));
    assert!(clean.contains("+    new_code();\n"));
    assert!(clean.contains(" }\n"));
    assert!(!clean.contains("10|"));
}

#[test]
fn test_sanitize_patch_preserves_clean_patch() {
    let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -10,3 +10,3 @@
 fn foo() {
-    old_code();
+    new_code();
 }
";
    let clean = sanitize_patch(patch);
    assert_eq!(clean, patch);
}

#[test]
fn test_sanitize_patch_handles_pipes_in_code() {
    // Pipe in code (e.g. match arms, closures) should NOT be stripped
    let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,3 @@
 match x {
-    Some(v) | None => {}
+    Some(v) | None => { v }
 }
";
    let clean = sanitize_patch(patch);
    assert!(clean.contains("    Some(v) | None => {}"));
}

// ── leading_whitespace tests ──

#[test]
fn test_leading_whitespace_spaces() {
    assert_eq!(leading_whitespace("    code"), "    ");
}

#[test]
fn test_leading_whitespace_tabs() {
    assert_eq!(leading_whitespace("\t\tcode"), "\t\t");
}

#[test]
fn test_leading_whitespace_none() {
    assert_eq!(leading_whitespace("code"), "");
}

#[test]
fn test_leading_whitespace_all_spaces() {
    assert_eq!(leading_whitespace("    "), "    ");
}

// ── apply_edits dispatch tests ──

#[test]
fn test_apply_edits_dispatches_to_search_replace() {
    let tmp = std::env::temp_dir().join("selfware-test-dispatch-sr");
    let _ = std::fs::create_dir_all(&tmp);
    // Init git repo for the function
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output();

    // JSON array with search/replace → should dispatch to apply_search_replace
    let edits_json = serde_json::json!([
        {"file": "test.rs", "search": "fn old() {}", "replace": "fn new() {}"}
    ]);
    let patch = serde_json::to_string(&edits_json).unwrap();
    assert!(apply_edits(&tmp, &patch));

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new()"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_edits_dispatches_to_unified_diff() {
    let tmp = std::env::temp_dir().join("selfware-test-dispatch-ud");
    let _ = std::fs::create_dir_all(&tmp);
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output();

    // A plain unified diff string → should dispatch to apply_unified_diff
    let patch = "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
    assert!(apply_edits(&tmp, patch));

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new()"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_edits_bad_json_falls_to_diff() {
    // Not valid JSON → falls through to unified diff (which will also fail for gibberish)
    let tmp = std::env::temp_dir().join("selfware-test-dispatch-bad");
    let _ = std::fs::create_dir_all(&tmp);
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    std::fs::write(tmp.join("x.rs"), "code\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output();

    assert!(!apply_edits(&tmp, "not json and not a patch"));

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── apply_search_replace edge cases ──

#[test]
fn test_apply_search_replace_ambiguous() {
    let tmp = std::env::temp_dir().join("selfware-test-sr-ambig");
    let _ = std::fs::create_dir_all(&tmp);
    // File with duplicate pattern
    std::fs::write(tmp.join("dup.rs"), "fn foo() {}\nfn foo() {}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "dup.rs",
        "search": "fn foo() {}",
        "replace": "fn bar() {}"
    })];
    // Should reject because search is ambiguous (2 matches)
    assert!(!apply_search_replace(&tmp, &edits));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_search_replace_multiple_edits_same_file() {
    let tmp = std::env::temp_dir().join("selfware-test-sr-multi");
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(
        tmp.join("multi.rs"),
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
    )
    .unwrap();

    let edits = vec![
        serde_json::json!({"file": "multi.rs", "search": "fn alpha() {}", "replace": "fn alpha_v2() {}"}),
        serde_json::json!({"file": "multi.rs", "search": "fn gamma() {}", "replace": "fn gamma_v2() {}"}),
    ];
    assert!(apply_search_replace(&tmp, &edits));

    let content = std::fs::read_to_string(tmp.join("multi.rs")).unwrap();
    assert!(content.contains("fn alpha_v2()"));
    assert!(content.contains("fn beta()")); // unchanged
    assert!(content.contains("fn gamma_v2()"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_search_replace_missing_file() {
    let tmp = std::env::temp_dir().join("selfware-test-sr-nofile");
    let _ = std::fs::create_dir_all(&tmp);

    let edits = vec![serde_json::json!({
        "file": "nonexistent.rs",
        "search": "a",
        "replace": "b"
    })];
    assert!(!apply_search_replace(&tmp, &edits));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_search_replace_noop_rejected() {
    let tmp = std::env::temp_dir().join("selfware-test-sr-noop");
    let _ = std::fs::create_dir_all(&tmp);
    std::fs::write(tmp.join("noop.rs"), "fn foo() {}\n").unwrap();

    // search == replace → no change → should be rejected
    let edits = vec![serde_json::json!({
        "file": "noop.rs",
        "search": "fn foo() {}",
        "replace": "fn foo() {}"
    })];
    assert!(!apply_search_replace(&tmp, &edits));

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── fuzzy_find_and_replace edge cases ──

#[test]
fn test_fuzzy_find_and_replace_no_match() {
    let content = "fn foo() {\n    code();\n}\n";
    let result = fuzzy_find_and_replace(content, "fn bar() {", "fn baz() {");
    assert!(result.is_none());
}

#[test]
fn test_fuzzy_find_and_replace_empty_search() {
    let content = "fn foo() {}\n";
    let result = fuzzy_find_and_replace(content, "", "something");
    assert!(result.is_none());
}

#[test]
fn test_fuzzy_find_and_replace_preserves_trailing_newline() {
    let content = "fn foo() {\n    old();\n}\n";
    let result = fuzzy_find_and_replace(content, "    old();", "    new();");
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(
        r.ends_with('\n'),
        "Should preserve trailing newline: {:?}",
        r
    );
}

#[test]
fn test_fuzzy_find_and_replace_at_end_of_file() {
    let content = "line1\nline2\ntarget_line\n";
    let result = fuzzy_find_and_replace(content, "target_line", "replaced_line");
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.contains("replaced_line"));
    assert!(r.contains("line1"));
    assert!(r.contains("line2"));
}

#[test]
fn test_fuzzy_find_and_replace_at_start_of_file() {
    let content = "target_line\nline2\nline3\n";
    let result = fuzzy_find_and_replace(content, "target_line", "replaced_line");
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.starts_with("replaced_line"));
}

// ── read_mutation_targets tests ──

#[test]
fn test_read_mutation_targets_sorts_by_size() {
    let tmp = std::env::temp_dir().join("selfware-test-rmt");
    let _ = std::fs::create_dir_all(tmp.join("src"));
    // Create files of different sizes
    std::fs::write(tmp.join("src/big.rs"), "x".repeat(5000)).unwrap();
    std::fs::write(tmp.join("src/small.rs"), "y".repeat(100)).unwrap();
    std::fs::write(tmp.join("src/medium.rs"), "z".repeat(1000)).unwrap();

    let targets = super::super::MutationTargets {
        prompt_logic: vec![
            PathBuf::from("src/big.rs"),
            PathBuf::from("src/small.rs"),
            PathBuf::from("src/medium.rs"),
        ],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, &tmp);
    // small.rs should appear before big.rs (sorted by size ascending)
    let small_pos = context.find("src/small.rs").unwrap();
    let medium_pos = context.find("src/medium.rs").unwrap();
    let big_pos = context.find("src/big.rs").unwrap();
    assert!(small_pos < medium_pos, "small should come before medium");
    assert!(medium_pos < big_pos, "medium should come before big");

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_read_mutation_targets_includes_line_numbers() {
    let tmp = std::env::temp_dir().join("selfware-test-rmt-ln");
    let _ = std::fs::create_dir_all(tmp.join("src"));
    std::fs::write(
        tmp.join("src/test.rs"),
        "fn main() {\n    println!(\"hi\");\n}\n",
    )
    .unwrap();

    let targets = super::super::MutationTargets {
        prompt_logic: vec![PathBuf::from("src/test.rs")],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, &tmp);
    assert!(
        context.contains("1| fn main()"),
        "Should contain line numbers: {}",
        truncate_char_boundary(&context, 200)
    );
    assert!(context.contains("2|     println!"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_read_mutation_targets_empty() {
    let tmp = std::env::temp_dir().join("selfware-test-rmt-empty");
    let _ = std::fs::create_dir_all(&tmp);

    let targets = super::super::MutationTargets {
        prompt_logic: vec![],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, &tmp);
    assert!(context.is_empty());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_read_mutation_targets_missing_file() {
    let tmp = std::env::temp_dir().join("selfware-test-rmt-missing");
    let _ = std::fs::create_dir_all(&tmp);

    let targets = super::super::MutationTargets {
        prompt_logic: vec![PathBuf::from("nonexistent.rs")],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, &tmp);
    // Should gracefully skip missing files
    assert!(context.is_empty() || !context.contains("```rust"));

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── log_event tests ──

#[test]
fn test_log_event_writes_jsonl() {
    let tmp = std::env::temp_dir().join("selfware-test-logevent");
    let _ = std::fs::create_dir_all(&tmp);

    let event = serde_json::json!({"event": "test", "value": 42});
    log_event(&tmp, &event);
    log_event(&tmp, &serde_json::json!({"event": "second"}));

    let log_path = tmp.join(".evolution-log.jsonl");
    let content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["event"], "test");
    assert_eq!(parsed["value"], 42);

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── apply_unified_diff tests ──

#[test]
fn test_apply_unified_diff_valid_patch() {
    let tmp = std::env::temp_dir().join("selfware-test-ud-valid");
    let _ = std::fs::create_dir_all(&tmp);
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    std::fs::write(tmp.join("file.rs"), "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output();

    let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
    assert!(apply_unified_diff(&tmp, patch));

    let content = std::fs::read_to_string(tmp.join("file.rs")).unwrap();
    assert!(content.contains("fn new()"));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn test_apply_unified_diff_invalid_patch() {
    let tmp = std::env::temp_dir().join("selfware-test-ud-invalid");
    let _ = std::fs::create_dir_all(&tmp);
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(&tmp)
        .output();
    std::fs::write(tmp.join("file.rs"), "fn foo() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(&tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(&tmp)
        .output();

    assert!(!apply_unified_diff(&tmp, "garbage patch content"));

    let _ = std::fs::remove_dir_all(&tmp);
}

// ── parse_hypotheses edge cases ──

#[test]
fn test_parse_hypotheses_mixed_formats() {
    // One with edits, one with patch (legacy), one missing both → 2 parsed
    let json = r#"[
            {
                "description": "Edit format",
                "edits": [{"file": "a.rs", "search": "old", "replace": "new"}],
                "target_files": ["a.rs"],
                "property_test": null
            },
            {
                "description": "Patch format",
                "patch": "--- a/b.rs\n+++ b/b.rs",
                "target_files": ["b.rs"],
                "property_test": null
            },
            {
                "description": "Missing both",
                "target_files": ["c.rs"],
                "property_test": null
            }
        ]"#;
    let hypotheses = parse_hypotheses_response(json);
    assert_eq!(hypotheses.len(), 2);
    assert_eq!(hypotheses[0].description, "Edit format");
    assert!(hypotheses[0].patch.contains("old")); // serialized edits JSON
    assert_eq!(hypotheses[1].description, "Patch format");
}

#[test]
fn test_parse_hypotheses_empty_edits_rejected() {
    let json = r#"[{
            "description": "Empty edits",
            "edits": [],
            "target_files": ["a.rs"],
            "property_test": null
        }]"#;
    let hypotheses = parse_hypotheses_response(json);
    // Empty edits serializes to "[]" which is not empty string, but apply_edits
    // would reject it. parse should still create the hypothesis.
    assert_eq!(hypotheses.len(), 1);
}

// ── chrono_now test ──

#[test]
fn test_chrono_now_format() {
    let ts = chrono_now();
    // Should be "seconds.milliseconds" format
    assert!(ts.contains('.'), "Timestamp should contain '.': {}", ts);
    let parts: Vec<&str> = ts.split('.').collect();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].parse::<u64>().is_ok());
    assert!(parts[1].parse::<u64>().is_ok());
}

// ── UTF-8 char-boundary truncation (whole-repo review, Evolution P1) ──

#[test]
fn test_truncate_char_boundary_never_splits_multibyte() {
    // 199 ASCII bytes + a 3-byte char (€) straddling byte 200.
    let mut s = "a".repeat(199);
    s.push('€'); // bytes 199..202
    s.push_str("tail");
    let out = truncate_char_boundary(&s, 200);
    assert_eq!(out.len(), 199, "must back off to the char boundary");

    // Exactly at a boundary: no backoff.
    let s2 = format!("{}€", "b".repeat(197)); // 197 + 3 = exactly 200
    assert_eq!(truncate_char_boundary(&s2, 200).len(), 200);

    // Shorter than the limit: untouched.
    assert_eq!(truncate_char_boundary("short", 200), "short");
}

#[test]
fn test_truncate_to_line_boundary_multibyte_does_not_panic() {
    // Regression: `s[..max_chars]` panicked when max_chars landed inside
    // a multibyte char, aborting the whole evolve run before any LLM call.
    // CJK chars are 3 bytes; byte 20 lands mid-char (18 is the boundary).
    let text = format!("{}\nsecond line\n", "日".repeat(10));
    let trunc = truncate_to_line_boundary(&text, 20);
    assert_eq!(trunc, "日".repeat(6), "backs off to the char boundary");

    // Newline right after a multibyte run: cut lands on the newline.
    let text2 = format!("{}\nsecond line\n", "日".repeat(6)); // 18 bytes + \n
    let trunc2 = truncate_to_line_boundary(&text2, 20);
    assert_eq!(trunc2, "日".repeat(6));
}

// ── Winner-commit helpers (whole-repo review, Evolution P1) ──

/// Run git in `dir`, asserting success.
fn git_ok(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A throwaway repo with one committed source file, an unrelated dirty
/// edit, and an untracked `.env` — the exact cocktail `git add -A` used
/// to sweep into the BLOOM commit.
fn setup_winner_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn f() -> usize { 1 }\n").unwrap();
    std::fs::write(root.join("notes.txt"), "clean notes\n").unwrap();
    git_ok(root, &["init"]);
    git_ok(root, &["config", "user.email", "evo@test"]);
    git_ok(root, &["config", "user.name", "Evo Test"]);
    git_ok(root, &["add", "."]);
    git_ok(root, &["commit", "-m", "initial"]);
    // Unrelated dirty edit + untracked secret — must NEVER be committed
    // by the evolution daemon.
    std::fs::write(root.join("notes.txt"), "user work in progress\n").unwrap();
    std::fs::write(root.join(".env"), "SECRET=hunter2\n").unwrap();
    dir
}

#[test]
fn test_capture_tested_diff_includes_new_files_and_edits() {
    let dir = setup_winner_repo();
    let root = dir.path();
    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    // Simulate the winner patch + a cargo-fmt auto-fix in the worktree.
    std::fs::write(worktree.join("src/lib.rs"), "pub fn f() -> usize { 2 }\n").unwrap();
    std::fs::write(worktree.join("src/new.rs"), "pub fn g() {}\n").unwrap();

    let diff = capture_tested_diff(&worktree).expect("diff should capture");
    assert!(diff.contains("src/lib.rs"), "edit captured: {}", diff);
    assert!(diff.contains("pub fn f() -> usize { 2 }"));
    assert!(diff.contains("src/new.rs"), "new file captured: {}", diff);
    assert_eq!(patch_edited_paths(&diff).len(), 2);

    ast_tools::cleanup_worktree(root, &worktree).unwrap();
}

#[test]
fn test_worktree_guard_cleans_up_on_drop() {
    let dir = setup_winner_repo();
    let root = dir.path();
    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    assert!(worktree.exists());
    {
        let _guard = WorktreeGuard::new(root, worktree.clone());
    } // guard dropped here — including on the panic path
    assert!(
        !worktree.exists(),
        "guard drop must remove the shadow worktree"
    );
}

#[test]
fn test_commit_scoped_paths_excludes_unrelated_dirty_and_env() {
    let dir = setup_winner_repo();
    let root = dir.path();
    // Winner change to src/lib.rs (as if applied from the tested diff).
    std::fs::write(root.join("src/lib.rs"), "pub fn f() -> usize { 2 }\n").unwrap();

    let paths = vec![PathBuf::from("src/lib.rs")];
    assert!(commit_scoped_paths(
        root,
        &paths,
        "🧬 Gen 1 BLOOM: 50 → 60 | test"
    ));

    // The commit contains ONLY src/lib.rs.
    let names = git_stdout(root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(names.contains("src/lib.rs"));
    assert!(!names.contains("notes.txt"), "unrelated edit not committed");
    assert!(!names.contains(".env"), "secret not committed");

    // Unrelated dirty edit and .env are still there, uncommitted.
    let status = git_stdout(root, &["status", "--porcelain"]);
    assert!(
        status.contains("?? .env"),
        "env stays untracked: {}",
        status
    );
    assert!(
        status.contains(" M notes.txt"),
        "unrelated edit stays dirty: {}",
        status
    );
    let notes = std::fs::read_to_string(root.join("notes.txt")).unwrap();
    assert_eq!(notes, "user work in progress\n");
}

#[test]
fn test_commit_scoped_paths_handles_new_and_deleted_files() {
    let dir = setup_winner_repo();
    let root = dir.path();
    std::fs::write(root.join("src/new.rs"), "pub fn g() {}\n").unwrap();
    std::fs::remove_file(root.join("notes.txt")).unwrap();
    // notes.txt is BOTH the deleted winner path and was dirty — reset it
    // to committed state first so the deletion is the only change.
    git_ok(root, &["checkout", "--", "notes.txt"]);
    std::fs::remove_file(root.join("notes.txt")).unwrap();

    let paths = vec![PathBuf::from("src/new.rs"), PathBuf::from("notes.txt")];
    assert!(commit_scoped_paths(root, &paths, "🧬 Gen 2 BLOOM"));

    let names = git_stdout(root, &["show", "--name-status", "--format=", "HEAD"]);
    assert!(names.contains("A\tsrc/new.rs"), "new file added: {}", names);
    assert!(
        names.contains("D\tnotes.txt"),
        "deletion committed: {}",
        names
    );
    assert!(!names.contains(".env"));
}

#[test]
fn test_commit_winner_to_repo_applies_tested_diff_exactly() {
    let dir = setup_winner_repo();
    let root = dir.path();

    // Build a tested diff in a shadow worktree (patch + "fmt fix").
    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    std::fs::write(
        worktree.join("src/lib.rs"),
        "pub fn f() -> usize {\n    2 // fmt: reformatted\n}\n",
    )
    .unwrap();
    let tested_diff = capture_tested_diff(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    assert!(commit_winner_to_repo(root, &tested_diff, "🧬 Gen 3 BLOOM"));
    // The committed content is byte-identical to the TESTED worktree
    // content (fmt fix included), not the raw LLM patch.
    let content = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert_eq!(
        content,
        "pub fn f() -> usize {\n    2 // fmt: reformatted\n}\n"
    );
    let committed = git_stdout(root, &["show", "HEAD:src/lib.rs"]);
    assert_eq!(committed, content);

    // Unrelated files untouched and uncommitted.
    let status = git_stdout(root, &["status", "--porcelain"]);
    assert!(status.contains("?? .env"));
    assert!(status.contains(" M notes.txt"));
}

#[test]
fn test_apply_tested_diff_refuses_protected_paths() {
    let dir = setup_winner_repo();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/evolution")).unwrap();
    let diff =
        "--- a/src/evolution/daemon.rs\n+++ b/src/evolution/daemon.rs\n@@ -1 +1 @@\n-old\n+new\n";
    assert!(!apply_tested_diff_to_repo(root, diff));
    assert!(!root.join("src/evolution/daemon.rs").exists());
}

#[test]
fn test_log_and_append_attempt_failure_aborts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path();

    // Create a directory where the attempts file should be, causing File::open to fail
    let attempts_file = repo_root.join("unwritable_attempts_dir");
    std::fs::create_dir_all(&attempts_file).unwrap();

    let node = AttemptNode {
        id: "att-test-fail".to_string(),
        parent_id: None,
        generation: 1,
        branch_id: "test-branch".to_string(),
        hypothesis_id: "hyp-fail".to_string(),
        description: "Test attempt logging failure".to_string(),
        diff_sha256: "deadbeef".to_string(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 10,
        status: AttemptStatus::InternalError,
        failure_class: Some(FailureClass::EnvironmentError),
        failure_reason: Some("test".into()),
        output_tail: None,
        binary_sha256: None,
        created_at: "2026-09-17T00:00:00Z".to_string(),
    };

    let result = log_and_append_attempt(
        &attempts_file,
        &node,
        repo_root,
        1,
        std::time::Instant::now(),
    );

    assert!(
        result.is_err(),
        "log_and_append_attempt must return Err on I/O failure"
    );

    // Verify aborted event was written to repo_root/.evolution-log.jsonl
    let events_file = repo_root.join(".evolution-log.jsonl");
    assert!(events_file.exists(), "Event log must be created");
    let events_content = std::fs::read_to_string(&events_file).unwrap();
    assert!(events_content.contains("\"outcome\":\"aborted\""));
    assert!(events_content.contains("attempt logging failed"));
}

#[test]
fn test_control_failure_with_unwritable_attempts_file_aborts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path();

    // Create a directory where the attempts file should be, causing log_and_append_attempt to fail
    let attempts_file = repo_root.join("unwritable_attempts_dir");
    std::fs::create_dir_all(&attempts_file).unwrap();

    let node = AttemptNode {
        id: "att-g1-control".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "control".to_string(),
        hypothesis_id: "control".to_string(),
        description: "Unpatched control anchor".to_string(),
        diff_sha256: compute_sha256(b""),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 50,
        status: AttemptStatus::InternalError,
        failure_class: Some(FailureClass::EnvironmentError),
        failure_reason: Some("Control worktree failed".to_string()),
        output_tail: None,
        binary_sha256: None,
        created_at: chrono_now(),
    };

    let result = log_and_append_attempt(
        &attempts_file,
        &node,
        repo_root,
        1,
        std::time::Instant::now(),
    );

    assert!(
        result.is_err(),
        "log_and_append_attempt must return Err when attempt file is unwritable"
    );

    // Verify aborted event was written to .evolution-log.jsonl
    let events_file = repo_root.join(".evolution-log.jsonl");
    assert!(events_file.exists());
    let events_content = std::fs::read_to_string(&events_file).unwrap();
    assert!(events_content.contains("\"outcome\":\"aborted\""));
    assert!(events_content.contains("attempt logging failed"));
}

#[test]
fn test_ranked_candidate_promotion_runner_up_qualifies() {
    let base_metrics = make_metrics(100, 100);
    let base_sab = make_sab(vec![("sc1", 90.0, true), ("sc2", 80.0, true)]);
    let baseline_composite = 0.80;

    // Candidate 1: High composite, but regresses scenario 1 in DarwinX
    let mut cand1_metrics = make_metrics(100, 100);
    cand1_metrics.sab_score = 110.0;
    let cand1_sab = make_sab(vec![("sc1", 70.0, false), ("sc2", 95.0, true)]);
    let cand1_composite = 0.95;

    // Candidate 2 (Runner-up): Lower composite than candidate 1, but strictly improves and passes DarwinX
    let mut cand2_metrics = make_metrics(100, 100);
    cand2_metrics.sab_score = 105.0; // capability gain > SAB_NOISE_MARGIN
    let cand2_sab = make_sab(vec![("sc1", 92.0, true), ("sc2", 88.0, true)]);
    let cand2_composite = 0.88;

    let mut evaluated_candidates = vec![
        EvaluatedCandidate {
            hypothesis: Hypothesis {
                id: "hyp-1".into(),
                description: "Aggressive optimization with regression".into(),
                target_files: vec![],
                patch: "patch1".into(),
                property_test: None,
            },
            metrics: cand1_metrics.clone(),
            sab_result: Some(cand1_sab.clone()),
            tested_diff: "diff1".into(),
            composite: cand1_composite,
            attempt_id: "att-1".into(),
            branch_id: "branch-1".into(),
        },
        EvaluatedCandidate {
            hypothesis: Hypothesis {
                id: "hyp-2".into(),
                description: "Clean verified improvement".into(),
                target_files: vec![],
                patch: "patch2".into(),
                property_test: None,
            },
            metrics: cand2_metrics.clone(),
            sab_result: Some(cand2_sab.clone()),
            tested_diff: "diff2".into(),
            composite: cand2_composite,
            attempt_id: "att-2".into(),
            branch_id: "branch-2".into(),
        },
    ];

    // Sort descending by composite (best-first ranking)
    evaluated_candidates.sort_by(|a, b| {
        b.composite
            .partial_cmp(&a.composite)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    assert_eq!(evaluated_candidates[0].attempt_id, "att-1");
    assert_eq!(evaluated_candidates[1].attempt_id, "att-2");

    // Under single-winner selection (the old buggy pattern):
    // only the highest-scoring candidate is evaluated against promotion gates.
    let single_winner_decision = evaluate_candidate_promotion(
        baseline_composite,
        evaluated_candidates[0].composite,
        Some(&base_sab),
        evaluated_candidates[0].sab_result.as_ref(),
        &base_metrics,
        &evaluated_candidates[0].metrics,
    );
    assert!(matches!(
        single_winner_decision,
        PromotionDecision::Reject(r) if r.contains("DarwinX non-regression check failed")
    ));

    // Under ranked promotion (the fix):
    // We iterate through ranked candidates. If candidate #1 fails, candidate #2 is evaluated.
    let mut promoted_winner = None;
    for (rank_idx, candidate) in evaluated_candidates.into_iter().enumerate() {
        let rank = rank_idx + 1;
        match evaluate_candidate_promotion(
            baseline_composite,
            candidate.composite,
            Some(&base_sab),
            candidate.sab_result.as_ref(),
            &base_metrics,
            &candidate.metrics,
        ) {
            PromotionDecision::Promote => {
                promoted_winner = Some((rank, candidate));
                break;
            }
            PromotionDecision::Reject(_) => {}
        }
    }

    assert!(
        promoted_winner.is_some(),
        "Ranked promotion must promote the qualifying runner-up"
    );
    let (promoted_rank, promoted_candidate) = promoted_winner.unwrap();
    assert_eq!(promoted_rank, 2);
    assert_eq!(promoted_candidate.attempt_id, "att-2");
    assert_eq!(
        promoted_candidate.hypothesis.description,
        "Clean verified improvement"
    );
}

#[test]
fn test_candidate_ranking_transitivity_all_permutations() {
    // Candidates from review finding:
    // A: SAB 90.0, composite 0.90
    // B: SAB 90.4, composite 0.80
    // C: SAB 90.8, composite 0.70
    //
    // Under transitive ranking (SAB tiers of 0.5):
    // C has SAB 90.8 -> tier 181
    // A has SAB 90.0 -> tier 180, composite 0.90
    // B has SAB 90.4 -> tier 180, composite 0.80
    //
    // Therefore C > A > B consistently across every permutation.

    struct Cand {
        id: &'static str,
        sab: f64,
        composite: f64,
    }

    let a = Cand {
        id: "A",
        sab: 90.0,
        composite: 0.90,
    };
    let b = Cand {
        id: "B",
        sab: 90.4,
        composite: 0.80,
    };
    let c = Cand {
        id: "C",
        sab: 90.8,
        composite: 0.70,
    };

    let permutations = [
        vec![&a, &b, &c],
        vec![&a, &c, &b],
        vec![&b, &a, &c],
        vec![&b, &c, &a],
        vec![&c, &a, &b],
        vec![&c, &b, &a],
    ];

    for perm in permutations {
        let mut list = perm;
        list.sort_by(|x, y| {
            candidate_rank_cmp(y.sab, y.composite, x.sab, x.composite).then_with(|| x.id.cmp(y.id))
        });
        let ordered_ids: Vec<&str> = list.iter().map(|cand| cand.id).collect();
        assert_eq!(
            ordered_ids,
            vec!["C", "A", "B"],
            "Sorting must produce consistent, cycle-free ranking regardless of input permutation"
        );
    }
}
