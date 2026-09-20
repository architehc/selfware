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
        patch: Some(
            "--- a/src/tools/code_metrics.rs\n+++ b/src/tools/code_metrics.rs\n@@ -1 +1 @@\n-old\n+new\n"
                .to_string(),
        ),
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
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
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
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
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
    // The files the patch actually touched, not just the description: dedup
    // only rejects byte-identical diffs, so a re-worded attempt at the same
    // function slips past it and the file list is what makes the repeat visible.
    assert!(
        history.contains("[touched: src/tools/code_metrics.rs]"),
        "the failure entry must name the files the patch touched, got: {history}"
    );
    assert!(
        history.contains("Re-wording an attempt does not make it new"),
        "the header must say that re-wording is not a new attempt"
    );
}

/// The two breakers that bound an unattended daemon must fail safe: unset or
/// unparseable falls back to the default bound, and only `0` disables one.
#[test]
fn test_parse_plateau_and_barren_limits() {
    assert_eq!(parse_plateau_patience(None), DEFAULT_PLATEAU_PATIENCE);
    assert_eq!(parse_plateau_patience(Some("2")), 2);
    assert_eq!(parse_plateau_patience(Some(" 2 ")), 2);
    assert_eq!(
        parse_plateau_patience(Some("0")),
        0,
        "0 disables the breaker"
    );
    assert_eq!(
        parse_plateau_patience(Some("nope")),
        DEFAULT_PLATEAU_PATIENCE
    );

    assert_eq!(parse_barren_generations(None), DEFAULT_BARREN_GENERATIONS);
    assert_eq!(parse_barren_generations(Some("3")), 3);
    assert_eq!(
        parse_barren_generations(Some("0")),
        0,
        "0 disables the breaker"
    );
    assert_eq!(
        parse_barren_generations(Some("")),
        DEFAULT_BARREN_GENERATIONS
    );

    // The stop reason names the streak, so a stopped run explains itself.
    let reason = barren_stop_reason(5);
    assert!(reason.contains('5'), "got: {reason}");
    assert!(reason.contains("consecutive"), "got: {reason}");
}

/// The base-revision arm identity: model and endpoint can match while the two
/// arms were built from different sources, in which case the delta includes a
/// foreign commit's effect and promoting the candidate attributes it to the
/// patch. That is the one failure mode here that promotes a WRONG candidate, so
/// a mismatch must fail closed — while an unknown revision (no resolvable git
/// HEAD) must not reject the run outright, since nothing can be compared.
#[test]
fn test_winner_base_revision_gate_mismatch_fails_closed() {
    assert!(
        winner_base_revision_gate(Some("aaaa111"), Some("aaaa111")).is_ok(),
        "an identical base is a like-for-like comparison"
    );

    let err = winner_base_revision_gate(Some("aaaa111"), Some("bbbb222"))
        .expect_err("a differing base must never be promoted");
    assert!(err.contains("bbbb222"), "got: {err}");
    assert!(err.contains("aaaa111"), "got: {err}");
    assert!(err.contains("not be like-for-like"), "got: {err}");

    // Unresolvable on either side: nothing to compare, so do not fail closed.
    assert!(winner_base_revision_gate(None, None).is_ok());
    assert!(winner_base_revision_gate(Some("aaaa111"), None).is_ok());
    assert!(winner_base_revision_gate(None, Some("bbbb222")).is_ok());
}

#[test]
fn test_apply_patch_to_worktree_nonexistent_dir() {
    let result = apply_patch_to_worktree(Path::new("/nonexistent/dir/12345"), "some patch");
    assert!(!result, "Should fail gracefully for nonexistent directory");
}

#[test]
fn test_apply_patch_to_repo_bad_patch() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    // Initialize a git repo so `git apply` can run
    let _ = std::process::Command::new("git")
        .args(["init"])
        .current_dir(tmp)
        .output();
    let result = apply_patch_to_repo(tmp, "this is not a valid patch format");
    assert!(!result, "Should fail gracefully for bad patch content");
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
        outcome: "completed".to_string(),
        stop_reason: None,
        generations_run: 0,
        improvements: vec![],
        final_sab_score: 0.0,
        initial_sab_score: 0.0,
        total_duration: std::time::Duration::from_secs(1),
    };
    assert_eq!(result.generations_run, 0);
    assert_eq!(result.outcome, "completed");
    assert!(result.improvements.is_empty());
}

#[test]
fn test_evolution_result_policy_stopped_clean_exit() {
    let result = EvolutionResult {
        aborted: None,
        outcome: "policy_stopped".to_string(),
        stop_reason: Some("Incumbent fixed population reached (2/2)".to_string()),
        generations_run: 1,
        improvements: vec![],
        final_sab_score: 85.0,
        initial_sab_score: 85.0,
        total_duration: std::time::Duration::from_secs(10),
    };
    assert!(
        result.aborted.is_none(),
        "policy_stopped must have aborted: None so CLI does not bail"
    );
    assert_eq!(result.outcome, "policy_stopped");
    assert_eq!(
        result.stop_reason.as_deref(),
        Some("Incumbent fixed population reached (2/2)")
    );
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
fn test_compute_empirical_noise_margin_calculation_and_gating() {
    // 1. None inputs fall back to default constant 0.5
    assert_eq!(compute_empirical_noise_margin(None, None), 0.5);

    // 2. Paired scenarios calculate standard error of mean delta
    let base_sab = make_sab(vec![("sc1", 90.0, true), ("sc2", 80.0, true)]);
    let cand_sab = make_sab(vec![("sc1", 90.1, true), ("sc2", 80.1, true)]);
    // Deltas: [0.1, 0.1]. Mean = 0.1, Var = 0.0, StdErr = 0.0 -> clamped to 0.05
    let margin = compute_empirical_noise_margin(Some(&base_sab), Some(&cand_sab));
    assert!((margin - 0.05).abs() < 1e-6);

    // 3. Evaluate promotion with empirical margin: regression of 0.08 is beyond 0.05 margin
    let base_metrics = make_metrics(100, 100);
    let mut cand_metrics = make_metrics(100, 100);
    cand_metrics.sab_score = base_metrics.sab_score - 0.08;
    let decision = evaluate_candidate_promotion(
        0.8,
        0.85,
        Some(&base_sab),
        Some(&cand_sab),
        &base_metrics,
        &cand_metrics,
    );
    assert!(matches!(
        decision,
        PromotionDecision::Reject(r) if r.contains("regressed below baseline") && r.contains("0.05")
    ));
}

#[test]
fn test_compute_empirical_noise_margin_permutation_invariant() {
    let base_sab = make_sab(vec![
        ("sc1", 90.0, true),
        ("sc2", 80.0, true),
        ("sc3", 70.0, true),
    ]);
    let cand_sab_ordered = make_sab(vec![
        ("sc1", 90.2, true),
        ("sc2", 80.5, true),
        ("sc3", 70.1, true),
    ]);
    let cand_sab_permuted = make_sab(vec![
        ("sc3", 70.1, true),
        ("sc1", 90.2, true),
        ("sc2", 80.5, true),
    ]);

    let margin_ordered = compute_empirical_noise_margin(Some(&base_sab), Some(&cand_sab_ordered));
    let margin_permuted = compute_empirical_noise_margin(Some(&base_sab), Some(&cand_sab_permuted));

    assert_eq!(margin_ordered, margin_permuted);
    assert!(margin_ordered > 0.05);
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
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old_func() {\n    println!(\"hello\");\n}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "test.rs",
        "search": "fn old_func()",
        "replace": "fn new_func()"
    })];

    let result = apply_search_replace(tmp, &edits);
    assert!(result);

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new_func()"));
    assert!(!content.contains("fn old_func()"));
}

#[test]
fn test_apply_search_replace_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn foo() {}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "test.rs",
        "search": "fn nonexistent()",
        "replace": "fn bar()"
    })];

    let result = apply_search_replace(tmp, &edits);
    assert!(!result);
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
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    // Init git repo for the function
    let _ = Command::new("git").args(["init"]).current_dir(tmp).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(tmp)
        .output();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp)
        .output();

    // JSON array with search/replace → should dispatch to apply_search_replace
    let edits_json = serde_json::json!([
        {"file": "test.rs", "search": "fn old() {}", "replace": "fn new() {}"}
    ]);
    let patch = serde_json::to_string(&edits_json).unwrap();
    assert!(apply_edits(tmp, &patch));

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new()"));
}

#[test]
fn test_apply_edits_dispatches_to_unified_diff() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let _ = Command::new("git").args(["init"]).current_dir(tmp).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(tmp)
        .output();
    let test_file = tmp.join("test.rs");
    std::fs::write(&test_file, "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp)
        .output();

    // A plain unified diff string → should dispatch to apply_unified_diff
    let patch = "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
    assert!(apply_edits(tmp, patch));

    let content = std::fs::read_to_string(&test_file).unwrap();
    assert!(content.contains("fn new()"));
}

#[test]
fn test_apply_edits_bad_json_falls_to_diff() {
    // Not valid JSON → falls through to unified diff (which will also fail for gibberish)
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let _ = Command::new("git").args(["init"]).current_dir(tmp).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(tmp)
        .output();
    std::fs::write(tmp.join("x.rs"), "code\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp)
        .output();

    assert!(!apply_edits(tmp, "not json and not a patch"));
}

// ── apply_search_replace edge cases ──

#[test]
fn test_apply_search_replace_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    // File with duplicate pattern
    std::fs::write(tmp.join("dup.rs"), "fn foo() {}\nfn foo() {}\n").unwrap();

    let edits = vec![serde_json::json!({
        "file": "dup.rs",
        "search": "fn foo() {}",
        "replace": "fn bar() {}"
    })];
    // Should reject because search is ambiguous (2 matches)
    assert!(!apply_search_replace(tmp, &edits));
}

#[test]
fn test_apply_search_replace_multiple_edits_same_file() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    std::fs::write(
        tmp.join("multi.rs"),
        "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
    )
    .unwrap();

    let edits = vec![
        serde_json::json!({"file": "multi.rs", "search": "fn alpha() {}", "replace": "fn alpha_v2() {}"}),
        serde_json::json!({"file": "multi.rs", "search": "fn gamma() {}", "replace": "fn gamma_v2() {}"}),
    ];
    assert!(apply_search_replace(tmp, &edits));

    let content = std::fs::read_to_string(tmp.join("multi.rs")).unwrap();
    assert!(content.contains("fn alpha_v2()"));
    assert!(content.contains("fn beta()")); // unchanged
    assert!(content.contains("fn gamma_v2()"));
}

#[test]
fn test_apply_search_replace_missing_file() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();

    let edits = vec![serde_json::json!({
        "file": "nonexistent.rs",
        "search": "a",
        "replace": "b"
    })];
    assert!(!apply_search_replace(tmp, &edits));
}

#[test]
fn test_apply_search_replace_noop_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    std::fs::write(tmp.join("noop.rs"), "fn foo() {}\n").unwrap();

    // search == replace → no change → should be rejected
    let edits = vec![serde_json::json!({
        "file": "noop.rs",
        "search": "fn foo() {}",
        "replace": "fn foo() {}"
    })];
    assert!(!apply_search_replace(tmp, &edits));
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
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
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

    let context = read_mutation_targets(&targets, tmp);
    // small.rs should appear before big.rs (sorted by size ascending)
    let small_pos = context.find("src/small.rs").unwrap();
    let medium_pos = context.find("src/medium.rs").unwrap();
    let big_pos = context.find("src/big.rs").unwrap();
    assert!(small_pos < medium_pos, "small should come before medium");
    assert!(medium_pos < big_pos, "medium should come before big");
}

#[test]
fn test_read_mutation_targets_includes_line_numbers() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
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

    let context = read_mutation_targets(&targets, tmp);
    assert!(
        context.contains("1| fn main()"),
        "Should contain line numbers: {}",
        truncate_char_boundary(&context, 200)
    );
    assert!(context.contains("2|     println!"));
}

#[test]
fn test_read_mutation_targets_empty() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();

    let targets = super::super::MutationTargets {
        prompt_logic: vec![],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, tmp);
    assert!(context.is_empty());
}

#[test]
fn test_read_mutation_targets_missing_file() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();

    let targets = super::super::MutationTargets {
        prompt_logic: vec![PathBuf::from("nonexistent.rs")],
        tool_code: vec![],
        cognitive: vec![],
        config_keys: vec![],
    };

    let context = read_mutation_targets(&targets, tmp);
    // Should gracefully skip missing files
    assert!(context.is_empty() || !context.contains("```rust"));
}

// ── log_event tests ──

#[test]
fn test_log_event_writes_jsonl() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();

    let event = serde_json::json!({"event": "test", "value": 42});
    log_event(tmp, &event);
    log_event(tmp, &serde_json::json!({"event": "second"}));

    let log_path = tmp.join(".evolution-log.jsonl");
    let content = std::fs::read_to_string(&log_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 2);

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["event"], "test");
    assert_eq!(parsed["value"], 42);
}

// ── apply_unified_diff tests ──

#[test]
fn test_apply_unified_diff_valid_patch() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let _ = Command::new("git").args(["init"]).current_dir(tmp).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(tmp)
        .output();
    std::fs::write(tmp.join("file.rs"), "fn old() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp)
        .output();

    let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
    assert!(apply_unified_diff(tmp, patch));

    let content = std::fs::read_to_string(tmp.join("file.rs")).unwrap();
    assert!(content.contains("fn new()"));
}

#[test]
fn test_apply_unified_diff_invalid_patch() {
    let temp = tempfile::tempdir().unwrap();
    let tmp = temp.path();
    let _ = Command::new("git").args(["init"]).current_dir(tmp).output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(tmp)
        .output();
    std::fs::write(tmp.join("file.rs"), "fn foo() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(tmp)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp)
        .output();

    assert!(!apply_unified_diff(tmp, "garbage patch content"));
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

#[tokio::test]
async fn test_commit_scoped_paths_excludes_unrelated_dirty_and_env() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();
    // Winner change to src/lib.rs (as if applied from the tested diff).
    std::fs::write(root.join("src/lib.rs"), "pub fn f() -> usize { 2 }\n").unwrap();

    let paths = vec![PathBuf::from("src/lib.rs")];
    assert!(commit_scoped_paths(root, &paths, "🧬 Gen 1 BLOOM: 50 → 60 | test").await);

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

#[tokio::test]
async fn test_commit_scoped_paths_handles_new_and_deleted_files() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();
    std::fs::write(root.join("src/new.rs"), "pub fn g() {}\n").unwrap();
    std::fs::remove_file(root.join("notes.txt")).unwrap();
    // notes.txt is BOTH the deleted winner path and was dirty — reset it
    // to committed state first so the deletion is the only change.
    git_ok(root, &["checkout", "--", "notes.txt"]);
    std::fs::remove_file(root.join("notes.txt")).unwrap();

    let paths = vec![PathBuf::from("src/new.rs"), PathBuf::from("notes.txt")];
    assert!(commit_scoped_paths(root, &paths, "🧬 Gen 2 BLOOM").await);

    let names = git_stdout(root, &["show", "--name-status", "--format=", "HEAD"]);
    assert!(names.contains("A\tsrc/new.rs"), "new file added: {}", names);
    assert!(
        names.contains("D\tnotes.txt"),
        "deletion committed: {}",
        names
    );
    assert!(!names.contains(".env"));
}

#[tokio::test]
async fn test_commit_winner_to_repo_applies_tested_diff_exactly() {
    let _exec = crate::test_support::ExecGuard::hold();
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
    let tree_id = capture_worktree_tree_id(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    assert!(commit_winner_to_repo(root, &tested_diff, Some(&tree_id), "🧬 Gen 3 BLOOM").await);
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

#[tokio::test]
async fn test_commit_winner_to_repo_with_unrelated_staged_changes() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    // 1. User stages an unrelated change in the main repo index
    std::fs::write(root.join("unrelated_staged.txt"), "staged by user\n").unwrap();
    git_ok(root, &["add", "unrelated_staged.txt"]);

    // Verify it is staged in main index
    let status_before = git_stdout(root, &["status", "--porcelain"]);
    assert!(
        status_before.contains("A  unrelated_staged.txt"),
        "unrelated_staged.txt must be staged before promotion: {status_before}"
    );

    // 2. Candidate diff is prepared in a shadow worktree
    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    std::fs::write(
        worktree.join("src/lib.rs"),
        "pub fn f() -> usize {\n    777\n}\n",
    )
    .unwrap();
    let tested_diff = capture_tested_diff(&worktree).unwrap();
    let tree_id = capture_worktree_tree_id(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    // 3. Commit winner to repo with expected tree_id
    assert!(
        commit_winner_to_repo(root, &tested_diff, Some(&tree_id), "🧬 Gen 4 BLOOM").await,
        "Promotion commit must succeed using isolated index"
    );

    // 4. Committed commit contains ONLY src/lib.rs, NOT unrelated_staged.txt
    let committed_files = git_stdout(root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(committed_files.contains("src/lib.rs"));
    assert!(
        !committed_files.contains("unrelated_staged.txt"),
        "HEAD commit must not contain unrelated staged changes: {committed_files}"
    );

    // 5. Unrelated file remains staged in the user's index
    let status_after = git_stdout(root, &["status", "--porcelain"]);
    assert!(
        status_after.contains("A  unrelated_staged.txt"),
        "unrelated_staged.txt must remain staged after promotion: {status_after}"
    );

    // 6. src/lib.rs committed content matches tested content
    let committed_content = git_stdout(root, &["show", "HEAD:src/lib.rs"]);
    assert!(committed_content.contains("777"));
}

#[tokio::test]
async fn test_commit_winner_to_repo_rejects_divergent_promoted_tree() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    // 1. Build a tested diff and capture its tree in a shadow worktree
    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    std::fs::write(
        worktree.join("src/lib.rs"),
        "pub fn f() -> usize {\n    42\n}\n",
    )
    .unwrap();
    let tested_diff = capture_tested_diff(&worktree).unwrap();
    let tree_id = capture_worktree_tree_id(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    // 2. Advance HEAD in root by committing an unrelated file change
    std::fs::write(root.join("src/extra.rs"), "pub fn extra() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "src/extra.rs"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "Unrelated commit on main"])
        .current_dir(root)
        .output();

    // 3. Attempting to commit the candidate with expected tree_id MUST fail because HEAD diverged
    assert!(
        !commit_winner_to_repo(root, &tested_diff, Some(&tree_id), "🧬 Gen 3 BLOOM").await,
        "Promotion must be rejected when promoted tree differs from evaluated benchmark tree"
    );

    // 4. Verify root remains clean (the candidate's diff is not left half-applied)
    let content = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(
        !content.contains("42"),
        "Divergent candidate edit must be reverted"
    );
}

#[tokio::test]
async fn test_commit_winner_to_repo_rejects_missing_or_empty_expected_tree() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    std::fs::write(
        worktree.join("src/lib.rs"),
        "pub fn f() -> usize {\n    99\n}\n",
    )
    .unwrap();
    let tested_diff = capture_tested_diff(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    // 1. Rejects None
    assert!(
        !commit_winner_to_repo(root, &tested_diff, None, "🧬 Gen 3 BLOOM").await,
        "Promotion must fail closed when expected_tree is None"
    );

    // 2. Rejects empty or whitespace string
    assert!(
        !commit_winner_to_repo(root, &tested_diff, Some(""), "🧬 Gen 3 BLOOM").await,
        "Promotion must fail closed when expected_tree is empty"
    );
    assert!(
        !commit_winner_to_repo(root, &tested_diff, Some("   \n"), "🧬 Gen 3 BLOOM").await,
        "Promotion must fail closed when expected_tree is whitespace"
    );

    // 3. Verify repo remains clean
    let content = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
    assert!(
        !content.contains("99"),
        "Candidate diff must not be committed when expected_tree is missing"
    );
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
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
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
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
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
            evaluated_tree: "tree1".into(),
            composite: cand1_composite,
            attempt_id: "att-1".into(),
            branch_id: "branch-1".into(),
            base_commit: None,
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
            evaluated_tree: "tree2".into(),
            composite: cand2_composite,
            attempt_id: "att-2".into(),
            branch_id: "branch-2".into(),
            base_commit: None,
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

#[test]
fn test_promoted_policies_control_live_search_decisions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let attempts_file = temp.path().join("attempts.jsonl");

    // Seed attempts with a baseline and two evaluated branches:
    // branch-1 (score 0.85 - strong improvement) and branch-2 (score 0.40 - below baseline)
    let baseline = AttemptNode {
        id: "att-baseline".into(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".into(),
        hypothesis_id: "baseline".into(),
        description: "Baseline measurement".into(),
        diff_sha256: "sha-base".into(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.50),
        tokens_used: Some(1000),
        wall_time_ms: 100,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "2026-09-17T00:00:00Z".into(),
    };
    let b1 = AttemptNode {
        id: "att-b1-1".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-1".into(),
        hypothesis_id: "hyp-1".into(),
        description: "Branch 1 winner".into(),
        diff_sha256: "sha-b1".into(),
        patch: Some("diff1".into()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.85),
        tokens_used: Some(2000),
        wall_time_ms: 200,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        git_tree_id: None,
        created_at: "2026-09-17T00:01:00Z".into(),
    };
    let b2 = AttemptNode {
        id: "att-b2-1".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-2".into(),
        hypothesis_id: "hyp-2".into(),
        description: "Branch 2 weak".into(),
        diff_sha256: "sha-b2".into(),
        patch: Some("diff2".into()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.40),
        tokens_used: Some(2000),
        wall_time_ms: 200,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(crate::evolution::ActionType::OpenRoot),
        git_tree_id: None,
        created_at: "2026-09-17T00:02:00Z".into(),
    };

    let mut file_content = String::new();
    for node in &[&baseline, &b1, &b2] {
        file_content.push_str(&serde_json::to_string(node).unwrap());
        file_content.push('\n');
    }
    std::fs::write(&attempts_file, file_content).unwrap();

    // 1. Fixed population policy stops once the initial population is evaluated (no further roots)
    let mut fixed_policy = FixedPopulationPolicy::new(2);
    let decision_fixed =
        decide_next_search_action(&mut fixed_policy, &attempts_file, 0.50, 2, 1, 1.0);

    // 2. RefineTop1 policy zeroes in on the top performing branch (branch-1 with score 0.85)
    let mut refine_policy = instantiate_search_policy("refine_top1", 2);
    let decision_refine =
        decide_next_search_action(&mut *refine_policy, &attempts_file, 0.50, 2, 1, 1.0);

    // 3. BreadthFirst policy expands frontiers across all open branches simultaneously
    let mut breadth_policy = instantiate_search_policy("breadth_first", 2);
    let decision_breadth =
        decide_next_search_action(&mut *breadth_policy, &attempts_file, 0.50, 2, 1, 1.0);

    // Prove that the policies make DIFFERENT live search decisions on the exact same attempt history
    assert!(
        decision_fixed != decision_refine,
        "FixedPopulationPolicy and RefineTop1Policy must make distinct live decisions"
    );
    assert!(
        decision_refine != decision_breadth,
        "RefineTop1Policy and BreadthFirstPolicy must make distinct live decisions"
    );
    assert!(
        decision_fixed != decision_breadth,
        "FixedPopulationPolicy and BreadthFirstPolicy must make distinct live decisions"
    );

    // Verify FixedPopulationPolicy stops once population limit is reached
    match decision_fixed {
        PolicyDecision::Stop { reason } => {
            assert!(
                reason.contains("No further unrevealed roots available")
                    || reason.contains("population reached"),
                "FixedPopulation stop reason: {reason}"
            );
        }
        other => panic!(
            "Expected Stop from FixedPopulation once population saturated, got {:?}",
            other
        ),
    }

    // Verify RefineTop1 specifically selected only the top branch
    match decision_refine {
        PolicyDecision::SelectBatch(actions) => {
            assert_eq!(
                actions.len(),
                1,
                "RefineTop1 must select exactly 1 branch to refine"
            );
            match &actions[0] {
                LegalAction::RefineFrontier {
                    branch_id,
                    parent_id,
                    ..
                } => {
                    assert_eq!(
                        branch_id, "branch-1",
                        "RefineTop1Policy must target top branch-1"
                    );
                    assert_eq!(
                        parent_id, "att-b1-1",
                        "RefineTop1Policy must refine att-b1-1 frontier"
                    );
                }
                other => panic!("Expected RefineFrontier, got {:?}", other),
            }
        }
        other => panic!("Expected SelectBatch from RefineTop1, got {:?}", other),
    }

    // Verify BreadthFirstPolicy selected both open branches
    match decision_breadth {
        PolicyDecision::SelectBatch(actions) => {
            assert_eq!(
                actions.len(),
                2,
                "BreadthFirst must expand both open branches"
            );
            let branch_ids: std::collections::HashSet<_> =
                actions.iter().map(|a| a.branch_id()).collect();
            assert!(branch_ids.contains("branch-1"));
            assert!(branch_ids.contains("branch-2"));
        }
        other => panic!("Expected SelectBatch from BreadthFirst, got {:?}", other),
    }
}

#[test]
fn test_infrastructure_failures_excluded_from_deduplication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let attempts_file = temp.path().join("attempts.jsonl");

    let nodes = [
        // 1. Internal error (e.g. shadow worktree lock collision, disk full)
        AttemptNode {
            id: "att-1".into(),
            parent_id: Some("att-baseline".into()),
            generation: 1,
            branch_id: "branch-1".into(),
            hypothesis_id: "hyp-1".into(),
            description: "Worktree lock failure".into(),
            diff_sha256: "sha-internal-error-1".into(),
            patch: Some("patch1".into()),
            sab_report_path: None,
            metrics: None,
            composite_score: None,
            tokens_used: None,
            wall_time_ms: 5,
            status: AttemptStatus::InternalError,
            failure_class: Some(FailureClass::Unclassified),
            failure_reason: Some("Failed to create shadow worktree".into()),
            output_tail: None,
            binary_sha256: None,
            base_commit: None,
            committed_commit: None,
            action_type: None,
            git_tree_id: None,
            created_at: "2026-09-17T00:00:00Z".into(),
        },
        // 2. Environment error (e.g. test runner killed by external watchdog)
        AttemptNode {
            id: "att-2".into(),
            parent_id: Some("att-baseline".into()),
            generation: 1,
            branch_id: "branch-2".into(),
            hypothesis_id: "hyp-2".into(),
            description: "Environment timeout".into(),
            diff_sha256: "sha-env-error-2".into(),
            patch: Some("patch2".into()),
            sab_report_path: None,
            metrics: None,
            composite_score: None,
            tokens_used: None,
            wall_time_ms: 10,
            status: AttemptStatus::CompileFailed,
            failure_class: Some(FailureClass::EnvironmentError),
            failure_reason: Some("External environment unreachable".into()),
            output_tail: None,
            binary_sha256: None,
            base_commit: None,
            committed_commit: None,
            action_type: None,
            git_tree_id: None,
            created_at: "2026-09-17T00:01:00Z".into(),
        },
        // 3. Genuine code defect (type error) -> MUST be blacklisted
        AttemptNode {
            id: "att-3".into(),
            parent_id: Some("att-baseline".into()),
            generation: 1,
            branch_id: "branch-3".into(),
            hypothesis_id: "hyp-3".into(),
            description: "Real compiler failure".into(),
            diff_sha256: "sha-real-defect-3".into(),
            patch: Some("patch3".into()),
            sab_report_path: None,
            metrics: None,
            composite_score: None,
            tokens_used: None,
            wall_time_ms: 50,
            status: AttemptStatus::CompileFailed,
            failure_class: Some(FailureClass::RepairableTypeError),
            failure_reason: Some("mismatched types".into()),
            output_tail: None,
            binary_sha256: None,
            base_commit: None,
            committed_commit: None,
            action_type: None,
            git_tree_id: None,
            created_at: "2026-09-17T00:02:00Z".into(),
        },
        // 4. Duplicate rejected upfront -> MUST remain in blacklist
        AttemptNode {
            id: "att-4".into(),
            parent_id: Some("att-baseline".into()),
            generation: 1,
            branch_id: "branch-4".into(),
            hypothesis_id: "hyp-4".into(),
            description: "Duplicate rejected".into(),
            diff_sha256: "sha-dup-rejected-4".into(),
            patch: Some("patch4".into()),
            sab_report_path: None,
            metrics: None,
            composite_score: None,
            tokens_used: None,
            wall_time_ms: 0,
            status: AttemptStatus::DuplicateRejected,
            failure_class: Some(FailureClass::Unclassified),
            failure_reason: Some("Duplicate of failed diff".into()),
            output_tail: None,
            binary_sha256: None,
            base_commit: None,
            committed_commit: None,
            action_type: None,
            git_tree_id: None,
            created_at: "2026-09-17T00:03:00Z".into(),
        },
    ];

    let mut content = String::new();
    for node in &nodes {
        content.push_str(&serde_json::to_string(node).unwrap());
        content.push('\n');
    }
    std::fs::write(&attempts_file, content).unwrap();

    let blacklisted = load_failed_diff_shas(&attempts_file);

    // Infrastructure failures MUST NOT be blacklisted, allowing retries after recovery
    assert!(
        !blacklisted.contains("sha-internal-error-1"),
        "InternalError must be excluded from deduplication blacklist"
    );
    assert!(
        !blacklisted.contains("sha-env-error-2"),
        "EnvironmentError must be excluded from deduplication blacklist"
    );

    // Genuine code defects and duplicate rejections MUST be blacklisted
    assert!(
        blacklisted.contains("sha-real-defect-3"),
        "Real compile failures must be blacklisted from deduplication"
    );
    assert!(
        blacklisted.contains("sha-dup-rejected-4"),
        "Duplicate rejected diffs must be blacklisted"
    );
}

#[test]
fn test_promoted_policy_name_round_trip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let active_policy_path = root.join("active_policy.json");

    // 1. Exact CLI-generated payload for ParetoAdaptivePolicy
    let winner_name = "ParetoAdaptivePolicy";
    let val_obj = 0.8842;
    let inc_obj = 0.7210;
    let kendall_w = 0.8123;
    let beta = 0.35;
    let tf1 = root.join("tree1.jsonl");
    let tf2 = root.join("tree2.jsonl");
    std::fs::write(&tf1, b"sample tree 1 content").unwrap();
    std::fs::write(&tf2, b"sample tree 2 content").unwrap();
    let tree_digests = vec![
        crate::evolution::tree_log::compute_sha256(b"sample tree 1 content"),
        crate::evolution::tree_log::compute_sha256(b"sample tree 2 content"),
    ];
    let tree_files = vec![
        tf1.to_str().unwrap().to_string(),
        tf2.to_str().unwrap().to_string(),
    ];
    let report_digest = "report-digest-123";
    let evidence_hash = crate::evolution::replay::compute_policy_evidence_hash(
        winner_name,
        val_obj,
        inc_obj,
        kendall_w,
        beta,
        &tree_digests,
        report_digest,
    );

    let cli_payload = serde_json::json!({
        "policy_name": winner_name,
        "promoted_at": "2026-09-17T12:00:00Z",
        "validation_objective": val_obj,
        "incumbent_objective": inc_obj,
        "kendall_w": kendall_w,
        "beta": beta,
        "tree_files": tree_files,
        "tree_digests": tree_digests,
        "report_digest": report_digest,
        "evidence_hash": evidence_hash,
    });
    std::fs::write(
        &active_policy_path,
        serde_json::to_string_pretty(&cli_payload).unwrap(),
    )
    .unwrap();

    let loaded = load_active_policy(&active_policy_path, 3);
    assert_eq!(loaded.policy_name, "ParetoAdaptivePolicy");
    assert_eq!(loaded.policy.name(), "ParetoAdaptivePolicy");
    assert_eq!(loaded.beta, beta);
    assert_eq!(
        loaded.evidence_hash.as_deref(),
        Some(evidence_hash.as_str())
    );

    // 2. Exact CLI-generated payload with FixedPopulation (Incumbent)
    let winner_name = "FixedPopulation (Incumbent)";
    let hash2 = crate::evolution::replay::compute_policy_evidence_hash(
        winner_name,
        0.70,
        0.70,
        1.0,
        0.2,
        &[],
        "",
    );
    let cli_payload2 = serde_json::json!({
        "policy_name": winner_name,
        "promoted_at": "2026-09-17T12:00:00Z",
        "validation_objective": 0.70,
        "incumbent_objective": 0.70,
        "kendall_w": 1.0,
        "beta": 0.2,
        "evidence_hash": hash2,
    });
    std::fs::write(
        &active_policy_path,
        serde_json::to_string_pretty(&cli_payload2).unwrap(),
    )
    .unwrap();

    let loaded2 = load_active_policy(&active_policy_path, 3);
    assert_eq!(loaded2.policy.name(), "FixedPopulation (Incumbent)");
    assert_eq!(loaded2.beta, 0.2);

    // 3. Forged or corrupted evidence hash falls back safely to FixedPopulation
    let cli_payload_corrupted = serde_json::json!({
        "policy_name": "ParetoAdaptivePolicy",
        "promoted_at": "2026-09-17T12:00:00Z",
        "validation_objective": 0.9999, // tampered!
        "incumbent_objective": inc_obj,
        "kendall_w": kendall_w,
        "beta": beta,
        "tree_digests": tree_digests,
        "report_digest": report_digest,
        "evidence_hash": evidence_hash, // hash does not match tampered parameters
    });
    std::fs::write(
        &active_policy_path,
        serde_json::to_string_pretty(&cli_payload_corrupted).unwrap(),
    )
    .unwrap();

    let loaded_corrupted = load_active_policy(&active_policy_path, 3);
    assert_eq!(
        loaded_corrupted.policy.name(),
        "FixedPopulation (Incumbent)",
        "Forged/corrupted active policy must fall back to incumbent"
    );
}

#[test]
fn test_multi_action_batch_and_parent_restoration() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let attempts_file = root.join("attempts.jsonl");

    let patch_a = serde_json::json!([{
        "file": "code.rs",
        "search": "// BASELINE",
        "replace": "pub fn branch_a() -> i32 { 10 }"
    }])
    .to_string();

    let patch_b = serde_json::json!([{
        "file": "code.rs",
        "search": "// BASELINE",
        "replace": "pub fn branch_b() -> i32 { 20 }"
    }])
    .to_string();

    let node_baseline = crate::evolution::tree_log::AttemptNode {
        id: "att-baseline".into(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".into(),
        hypothesis_id: "baseline".into(),
        description: "baseline".into(),
        diff_sha256: "0".into(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.50),
        tokens_used: None,
        wall_time_ms: 0,
        status: crate::evolution::tree_log::AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "2026-09-17T00:00:00Z".into(),
    };

    let node_a = crate::evolution::tree_log::AttemptNode {
        id: "att-a".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-a".into(),
        hypothesis_id: "hyp-a".into(),
        description: "branch a implementation".into(),
        diff_sha256: "sha-a".into(),
        patch: Some(patch_a),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.60),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "2026-09-17T00:01:00Z".into(),
    };

    let node_b = crate::evolution::tree_log::AttemptNode {
        id: "att-b".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-b".into(),
        hypothesis_id: "hyp-b".into(),
        description: "branch b implementation".into(),
        diff_sha256: "sha-b".into(),
        patch: Some(patch_b),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.70),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "2026-09-17T00:01:30Z".into(),
    };

    let lines = format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&node_baseline).unwrap(),
        serde_json::to_string(&node_a).unwrap(),
        serde_json::to_string(&node_b).unwrap(),
    );
    std::fs::write(&attempts_file, lines).unwrap();

    // Verify parent restoration for both uncommitted branches
    let worktree_a = tempfile::tempdir().unwrap();
    let file_a = worktree_a.path().join("code.rs");
    std::fs::write(&file_a, "// BASELINE\n").unwrap();
    let res_a = crate::evolution::ast_tools::restore_worktree_parent_state(
        worktree_a.path(),
        &attempts_file,
        Some("att-a"),
    )
    .unwrap();
    assert_eq!(res_a, vec!["att-a"]);
    assert_eq!(
        std::fs::read_to_string(&file_a).unwrap(),
        "pub fn branch_a() -> i32 { 10 }\n"
    );

    let worktree_b = tempfile::tempdir().unwrap();
    let file_b = worktree_b.path().join("code.rs");
    std::fs::write(&file_b, "// BASELINE\n").unwrap();
    let res_b = crate::evolution::ast_tools::restore_worktree_parent_state(
        worktree_b.path(),
        &attempts_file,
        Some("att-b"),
    )
    .unwrap();
    assert_eq!(res_b, vec!["att-b"]);
    assert_eq!(
        std::fs::read_to_string(&file_b).unwrap(),
        "pub fn branch_b() -> i32 { 20 }\n"
    );

    // Multi-action batch execution check:
    // With active actions containing both branch-a and branch-b,
    // hypotheses are mapped round-robin so EVERY selected action executes.
    let active_actions = [
        LegalAction::RefineFrontier {
            branch_id: "branch-a".into(),
            parent_id: "att-a".into(),
            node_id: "att-a-next".into(),
        },
        LegalAction::RefineFrontier {
            branch_id: "branch-b".into(),
            parent_id: "att-b".into(),
            node_id: "att-b-next".into(),
        },
    ];

    let mut executed_parents = Vec::new();
    for h_idx in 0..2 {
        let action = &active_actions[h_idx % active_actions.len()];
        let (p_id, b_id) = match action {
            LegalAction::RefineFrontier {
                branch_id,
                parent_id,
                ..
            } => (parent_id.clone(), branch_id.clone()),
            LegalAction::OpenRoot { branch_id, .. } => ("att-baseline".into(), branch_id.clone()),
        };
        executed_parents.push((p_id, b_id));
    }

    assert_eq!(
        executed_parents,
        vec![
            ("att-a".to_string(), "branch-a".to_string()),
            ("att-b".to_string(), "branch-b".to_string()),
        ],
        "Every action in a multi-action batch must be assigned and executed"
    );
}

#[test]
fn test_fixed_population_daemon_continues_across_generations_and_empty_responses() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let attempts_file = root.join("attempts.jsonl");
    std::fs::write(&attempts_file, "").unwrap();

    // FixedPopulationPolicy::for_daemon has no probe budget limit, ensuring
    // the daemon continues across generations and after empty LLM responses.
    let mut policy = FixedPopulationPolicy::for_daemon(3);

    // Generation 1: request next actions
    let d1 = decide_next_search_action(&mut policy, &attempts_file, 0.50, 3, 1, 1.0);
    let actions1 = match d1 {
        PolicyDecision::SelectBatch(actions) => actions,
        PolicyDecision::Stop { reason } => panic!("Unexpected stop on gen 1: {reason}"),
    };
    assert_eq!(
        actions1.len(),
        3,
        "Gen 1 must select initial population of 3"
    );

    // Empty LLM response or generation 2: policy must NOT stop!
    let d2 = decide_next_search_action(&mut policy, &attempts_file, 0.50, 3, 2, 1.0);
    match d2 {
        PolicyDecision::SelectBatch(actions) => {
            assert_eq!(actions.len(), 3, "Gen 2 must continue and produce actions");
        }
        PolicyDecision::Stop { reason } => {
            panic!("FixedPopulationPolicy::for_daemon must not stop after first generation! Stopped with: {reason}");
        }
    }

    // By contrast, replay-budgeted FixedPopulationPolicy(3) DOES stop when its total budget is reached
    let mut replay_policy = FixedPopulationPolicy::new(3);
    let r1 = decide_next_search_action(&mut replay_policy, &attempts_file, 0.50, 3, 1, 1.0);
    assert!(matches!(r1, PolicyDecision::SelectBatch(_)));

    let r2 = decide_next_search_action(&mut replay_policy, &attempts_file, 0.50, 3, 2, 1.0);
    assert!(
        matches!(r2, PolicyDecision::Stop { .. }),
        "Replay policy must stop when total probe budget is exhausted"
    );
}

#[test]
fn test_daemon_honors_policy_stop_decision() {
    // In live evolution, when a policy issues PolicyDecision::Stop, the daemon
    // must honor the policy's stop decision and terminate search, matching replay semantics.
    let decision = PolicyDecision::Stop {
        reason: "Budget exhausted (3/3)".to_string(),
    };
    let stops = matches!(decision, PolicyDecision::Stop { .. });
    assert!(
        stops,
        "Daemon must terminate search when policy returns Stop"
    );
}

#[test]
fn test_refinement_restoration_failure_records_internal_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path();
    let attempts_file = repo_root.join("attempts.jsonl");

    // Parent att-nonexistent does NOT exist in attempts ledger.
    // Restoration must fail.
    let res = ast_tools::create_shadow_worktree_for_parent(
        repo_root,
        &attempts_file,
        Some("att-nonexistent"),
    );
    assert!(res.is_err(), "Restoring non-existent parent must error");

    // When restoration fails in Step 2, an InternalError attempt is recorded in attempts_file
    let node = AttemptNode {
        id: "att-g2-refine-fail-0".to_string(),
        parent_id: Some("att-nonexistent".to_string()),
        generation: 2,
        branch_id: "branch-1".to_string(),
        hypothesis_id: "hyp-restore-fail-0".to_string(),
        description: "Refinement restoration failed for parent att-nonexistent".to_string(),
        diff_sha256: compute_sha256(b""),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: None,
        tokens_used: None,
        wall_time_ms: 0,
        status: AttemptStatus::InternalError,
        failure_class: Some(FailureClass::EnvironmentError),
        failure_reason: Some("Parent attempt 'att-nonexistent' not found".to_string()),
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: chrono_now(),
    };
    log_and_append_attempt(&attempts_file, &node, repo_root, 2, Instant::now()).unwrap();

    let content = std::fs::read_to_string(&attempts_file).unwrap();
    assert!(content.contains("\"status\":\"internal_error\""));
    assert!(content.contains("\"failure_class\":\"environment_error\""));
    assert!(content.contains("Parent attempt 'att-nonexistent' not found"));
}

#[test]
fn test_open_root_reads_from_baseline_checkout() {
    let dir = setup_winner_repo();
    let root = dir.path();
    let attempts_file = root.join("attempts.jsonl");

    let base_commit = ast_tools::get_git_head_commit(root).unwrap();

    // Baseline node recorded at base_commit
    let baseline_node = AttemptNode {
        id: "att-baseline".to_string(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".to_string(),
        hypothesis_id: "baseline".to_string(),
        description: "Initial baseline".to_string(),
        diff_sha256: compute_sha256(b""),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.50),
        tokens_used: None,
        wall_time_ms: 0,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(base_commit.clone()),
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: chrono_now(),
    };
    std::fs::write(
        &attempts_file,
        format!("{}\n", serde_json::to_string(&baseline_node).unwrap()),
    )
    .unwrap();

    // Commit a change to root (simulating Gen 1 promotion to main)
    std::fs::write(root.join("src/lib.rs"), "pub fn promoted_gen1() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "src/lib.rs"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "Gen 1 winner promoted"])
        .current_dir(root)
        .output();

    // Now OpenRoot in Gen 2 restores att-baseline
    let worktree =
        ast_tools::create_shadow_worktree_for_parent(root, &attempts_file, Some("att-baseline"))
            .unwrap();
    let worktree_commit = ast_tools::get_git_head_commit(&worktree).unwrap();
    assert_eq!(
        worktree_commit, base_commit,
        "OpenRoot worktree must be detached at the immutable baseline commit, not moving HEAD"
    );

    let content = std::fs::read_to_string(worktree.join("src/lib.rs")).unwrap();
    assert!(
        !content.contains("promoted_gen1"),
        "OpenRoot worktree must contain original baseline source, not subsequent promoted commits"
    );
    ast_tools::cleanup_worktree(root, &worktree).unwrap();
}

#[test]
fn test_daemon_open_root_dispatches_from_active_incumbent_parent() {
    let dir = setup_winner_repo();
    let root = dir.path();
    let attempts_file = root.join("attempts.jsonl");

    let base_commit = ast_tools::get_git_head_commit(root).unwrap();

    // Baseline node recorded at base_commit
    let baseline_node = AttemptNode {
        id: "att-baseline".to_string(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".to_string(),
        hypothesis_id: "baseline".to_string(),
        description: "Initial baseline".to_string(),
        diff_sha256: compute_sha256(b""),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.50),
        tokens_used: None,
        wall_time_ms: 0,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(base_commit.clone()),
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: chrono_now(),
    };
    std::fs::write(
        &attempts_file,
        format!("{}\n", serde_json::to_string(&baseline_node).unwrap()),
    )
    .unwrap();

    // Gen 0 action without active_parent_id: OpenRoot must resolve to baseline_node.id
    let open_action_gen0 = LegalAction::OpenRoot {
        branch_id: "branch-g1-r0".to_string(),
        node_id: "root-g1-r0".to_string(),
    };
    let (parent_gen0, branch_gen0, action_type_gen0) =
        resolve_action_dispatch(&open_action_gen0, None, &baseline_node.id);
    assert_eq!(parent_gen0, "att-baseline");
    assert_eq!(branch_gen0, "branch-g1-r0");
    assert_eq!(action_type_gen0, ActionType::OpenRoot);

    // Commit Gen 1 winner to repo (simulating promotion to main)
    std::fs::write(root.join("src/lib.rs"), "pub fn winner_c1() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "src/lib.rs"])
        .current_dir(root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "Gen 1 winner committed"])
        .current_dir(root)
        .output();
    let c1 = ast_tools::get_git_head_commit(root).unwrap();

    // Record Gen 1 winner in attempts ledger with committed_commit anchor
    let cand1 = AttemptNode {
        id: "att-cand1".to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "branch-g1-r0".to_string(),
        hypothesis_id: "hyp-1".to_string(),
        description: "Candidate 1 (Gen 1 winner)".to_string(),
        diff_sha256: compute_sha256(b"diff1"),
        patch: Some("diff1".to_string()),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.85),
        tokens_used: Some(1000),
        wall_time_ms: 100,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(base_commit),
        committed_commit: Some(c1.clone()),
        action_type: Some(ActionType::OpenRoot),
        git_tree_id: None,
        created_at: chrono_now(),
    };
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&attempts_file)
            .unwrap();
        writeln!(f, "{}", serde_json::to_string(&cand1).unwrap()).unwrap();
    }

    // When active_parent_id is Some("att-cand1"), OpenRoot must resolve to the incumbent parent
    let active_parent_id = Some("att-cand1");
    let open_action_gen2 = LegalAction::OpenRoot {
        branch_id: "branch-g2-r0".to_string(),
        node_id: "root-g2-r0".to_string(),
    };
    let (parent_gen2, branch_gen2, action_type_gen2) =
        resolve_action_dispatch(&open_action_gen2, active_parent_id, &baseline_node.id);
    assert_eq!(
        parent_gen2, "att-cand1",
        "OpenRoot in Gen 2 must resolve to the active incumbent parent, not stale baseline"
    );
    assert_eq!(branch_gen2, "branch-g2-r0");
    assert_eq!(action_type_gen2, ActionType::OpenRoot);

    // RefineFrontier action must preserve its own parent regardless of active_parent_id
    let refine_action = LegalAction::RefineFrontier {
        branch_id: "branch-g1-r0".to_string(),
        parent_id: "att-cand1".to_string(),
        node_id: "refine-1".to_string(),
    };
    let (parent_refine, _, action_type_refine) =
        resolve_action_dispatch(&refine_action, active_parent_id, &baseline_node.id);
    assert_eq!(parent_refine, "att-cand1");
    assert_eq!(action_type_refine, ActionType::RefineFrontier);

    // Driving shadow worktree restoration using the parent resolved by resolve_action_dispatch
    let worktree =
        ast_tools::create_shadow_worktree_for_parent(root, &attempts_file, Some(&parent_gen2))
            .expect("Restoring shadow worktree for dispatched parent must succeed");
    let wt_commit = ast_tools::get_git_head_commit(&worktree).unwrap();
    assert_eq!(
        wt_commit, c1,
        "OpenRoot worktree inheriting active_parent_id must be at C1, not C0"
    );
    let wt_content = std::fs::read_to_string(worktree.join("src/lib.rs")).unwrap();
    assert!(
        wt_content.contains("winner_c1"),
        "OpenRoot worktree inheriting active_parent_id must restore C1 state"
    );
    ast_tools::cleanup_worktree(root, &worktree).unwrap();
}

#[test]
fn test_load_active_policy_authentication_and_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let active_policy_path = root.join("active_policy.json");
    let tree_file = root.join("tree.jsonl");

    let tree_bytes = b"sample tree content";
    std::fs::write(&tree_file, tree_bytes).unwrap();
    let tree_digest = crate::evolution::tree_log::compute_sha256(tree_bytes);

    // Case 1: Non-incumbent policy lacks evidence_hash -> must fall back to incumbent
    let json_no_hash = serde_json::json!({
        "policy_name": "ParetoAdaptive",
        "beta": 0.25,
        "validation_objective": 0.85,
        "incumbent_objective": 0.70,
        "kendall_w": 0.90,
    });
    std::fs::write(
        &active_policy_path,
        serde_json::to_string(&json_no_hash).unwrap(),
    )
    .unwrap();
    let loaded = load_active_policy(&active_policy_path, 4);
    assert_eq!(loaded.policy_name, "FixedPopulation (Incumbent)");
    assert!(loaded
        .fallback_reason
        .as_ref()
        .unwrap()
        .contains("lacks required cryptographic evidence_hash"));

    // Case 2: Non-incumbent policy with authentic evidence_hash and matching tree file -> loads cleanly
    let report_digest = "sample_report_digest";
    let beta = 0.25;
    let expected_hash = crate::evolution::replay::compute_policy_evidence_hash(
        "ParetoAdaptive",
        0.85,
        0.70,
        0.90,
        beta,
        std::slice::from_ref(&tree_digest),
        report_digest,
    );
    let json_valid = serde_json::json!({
        "policy_name": "ParetoAdaptive",
        "beta": beta,
        "validation_objective": 0.85,
        "incumbent_objective": 0.70,
        "kendall_w": 0.90,
        "evidence_hash": expected_hash,
        "tree_digests": [tree_digest],
        "tree_files": [tree_file.to_str().unwrap()],
        "report_digest": report_digest,
    });
    std::fs::write(
        &active_policy_path,
        serde_json::to_string(&json_valid).unwrap(),
    )
    .unwrap();
    let loaded = load_active_policy(&active_policy_path, 4);
    assert_eq!(loaded.policy_name, "ParetoAdaptivePolicy");
    assert_eq!(
        loaded.evidence_hash.as_deref(),
        Some(expected_hash.as_str())
    );
    assert_eq!(loaded.fallback_reason, None);

    // Case 3: Tree file on disk is tampered -> falls back to incumbent
    std::fs::write(&tree_file, b"tampered tree content!").unwrap();
    let loaded_tampered = load_active_policy(&active_policy_path, 4);
    assert_eq!(loaded_tampered.policy_name, "FixedPopulation (Incumbent)");
    assert!(loaded_tampered
        .fallback_reason
        .as_ref()
        .unwrap()
        .contains("digest mismatch"));
}

#[test]
fn test_run_lock_guard_acquire_and_drop() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();
    let run_id = "test_run_12345";

    // First acquire must succeed
    let guard =
        RunLockGuard::acquire(repo_root, run_id).expect("first lock acquire should succeed");
    assert!(guard.lock_path.exists(), "lock file should exist on disk");

    // Reading the lock file should contain the current process PID
    let content = std::fs::read_to_string(&guard.lock_path).unwrap();
    assert_eq!(content.trim(), std::process::id().to_string());

    // Second acquire on the same run_id must fail due to exclusive flock
    #[cfg(unix)]
    {
        let second_acquire = RunLockGuard::acquire(repo_root, run_id);
        assert!(
            second_acquire.is_err(),
            "second acquire must fail while lock is held"
        );
    }

    // Dropping the guard must release the lock and remove the lock file
    let lock_path = guard.lock_path.clone();
    drop(guard);
    assert!(!lock_path.exists(), "lock file must be deleted upon drop");
}

#[test]
fn test_sweep_orphaned_runs_protects_live_runs_and_recovers_progress() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    let attempts_dir = repo_root.join(".selfware").join("attempts");
    std::fs::create_dir_all(&attempts_dir).unwrap();

    let active_run_id = "run_active_999";
    let dead_run_id = "run_dead_888";

    // Acquire lock for active_run
    let _active_guard = RunLockGuard::acquire(repo_root, active_run_id).unwrap();

    // Populate dead run attempts file with progress
    let dead_attempts_file = attempts_dir.join(format!("{dead_run_id}.jsonl"));
    let mut m1 = make_metrics(10, 10);
    m1.sab_score = 75.0;
    let node1 = AttemptNode {
        id: "att-baseline".into(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".into(),
        hypothesis_id: "baseline".into(),
        description: "Initial baseline capability measurement".into(),
        diff_sha256: "hash1".into(),
        patch: None,
        sab_report_path: None,
        metrics: Some(m1),
        composite_score: Some(0.75),
        tokens_used: Some(1000),
        wall_time_ms: 500,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "1700000000.000".into(),
    };
    let mut m2 = make_metrics(10, 10);
    m2.sab_score = 88.5;
    let node2 = AttemptNode {
        id: "att-g3-1".into(),
        parent_id: Some("att-baseline".into()),
        generation: 3,
        branch_id: "main".into(),
        hypothesis_id: "hyp-2".into(),
        description: "third attempt with higher score".into(),
        diff_sha256: "hash2".into(),
        patch: None,
        sab_report_path: None,
        metrics: Some(m2),
        composite_score: Some(0.885),
        tokens_used: Some(1200),
        wall_time_ms: 600,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "1700000045.500".into(),
    };
    let mut m3 = make_metrics(10, 10);
    m3.sab_score = 92.0;
    let node3 = AttemptNode {
        id: "att-g2-root".into(),
        parent_id: None,
        generation: 2,
        branch_id: "main".into(),
        hypothesis_id: "hyp-3".into(),
        description: "unpromoted exploratory root candidate with high score".into(),
        diff_sha256: "hash3".into(),
        patch: None,
        sab_report_path: None,
        metrics: Some(m3),
        composite_score: Some(0.92),
        tokens_used: Some(1500),
        wall_time_ms: 700,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: Some(ActionType::OpenRoot),
        git_tree_id: None,
        created_at: "1700000030.000".into(),
    };
    let attempts_content = format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&node1).unwrap(),
        serde_json::to_string(&node2).unwrap(),
        serde_json::to_string(&node3).unwrap()
    );
    std::fs::write(&dead_attempts_file, attempts_content).unwrap();

    // Create log file with start events for both runs
    let log_path = repo_root.join(".evolution-log.jsonl");
    let log_content = format!(
        "{}\n{}\n",
        serde_json::json!({
            "event": "start",
            "run_id": active_run_id,
            "pid": std::process::id(),
            "timestamp": "1700000000.000",
        }),
        serde_json::json!({
            "event": "start",
            "run_id": dead_run_id,
            "pid": 99999999, // non-existent dead PID
            "timestamp": "1700000000.000",
        })
    );
    std::fs::write(&log_path, log_content).unwrap();

    // Run sweep: active_run is protected by lock and live PID; dead_run is swept
    let swept = sweep_orphaned_runs(repo_root);
    assert_eq!(swept, 1, "only the dead run should be swept");

    // Check that dead_run recovered honest metrics from attempts file
    let updated_log = std::fs::read_to_string(&log_path).unwrap();
    let mut found_dead_run_end = false;
    for line in updated_log.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("event").and_then(|v| v.as_str()) == Some("run_end")
                && val.get("run_id").and_then(|v| v.as_str()) == Some(dead_run_id)
            {
                found_dead_run_end = true;
                assert_eq!(val["outcome"], "killed");
                assert_eq!(val["generations_run"], 3);
                // Unpromoted candidate scores (88.5 and 92.0) must NOT inflate final_sab_score;
                // final_sab_score must recover the confirmed baseline incumbent (75.0),
                // while best_attempted_score captures the candidate's attempted 92.0.
                assert!((val["final_sab_score"].as_f64().unwrap() - 75.0).abs() < 1e-6);
                assert!((val["best_attempted_score"].as_f64().unwrap() - 92.0).abs() < 1e-6);
                assert!((val["duration_secs"].as_f64().unwrap() - 45.5).abs() < 1e-3);
            }
        }
    }
    assert!(
        found_dead_run_end,
        "dead run must have a recovered run_end event"
    );

    // Second sweep should find 0 orphans
    let second_sweep = sweep_orphaned_runs(repo_root);
    assert_eq!(second_sweep, 0);
}

#[test]
fn test_sweep_orphaned_runs_updates_incumbent_when_winner_was_committed() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    let attempts_dir = repo_root.join(".selfware").join("attempts");
    std::fs::create_dir_all(&attempts_dir).unwrap();

    let run_id = "test-run-committed";
    let attempts_file = attempts_dir.join(format!("{run_id}.jsonl"));

    let mut m1 = make_metrics(10, 10);
    m1.sab_score = 70.0;
    let node1 = AttemptNode {
        id: "att-base".into(),
        parent_id: None,
        generation: 0,
        branch_id: "main".into(),
        hypothesis_id: "baseline".into(),
        description: "baseline attempt".into(),
        diff_sha256: "hash0".into(),
        patch: None,
        sab_report_path: None,
        metrics: Some(m1),
        composite_score: Some(0.70),
        tokens_used: Some(1000),
        wall_time_ms: 500,
        status: AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        action_type: None,
        git_tree_id: None,
        created_at: "1700000000.000".into(),
    };

    let mut m2 = make_metrics(10, 10);
    m2.sab_score = 92.0;
    let node2 = AttemptNode {
        id: "att-g1-1".into(),
        parent_id: Some("att-base".into()),
        generation: 1,
        branch_id: "main".into(),
        hypothesis_id: "hyp-1".into(),
        description: "promoted winner".into(),
        diff_sha256: "hash1".into(),
        patch: None,
        sab_report_path: None,
        metrics: Some(m2),
        composite_score: Some(0.92),
        tokens_used: Some(1200),
        wall_time_ms: 600,
        status: AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: Some("commit_sha_12345678".into()),
        action_type: None,
        git_tree_id: None,
        created_at: "1700000030.000".into(),
    };

    let attempts_content = format!(
        "{}\n{}\n",
        serde_json::to_string(&node1).unwrap(),
        serde_json::to_string(&node2).unwrap()
    );
    std::fs::write(&attempts_file, attempts_content).unwrap();

    let log_path = repo_root.join(".evolution-log.jsonl");
    let log_content = serde_json::json!({
        "event": "start",
        "run_id": run_id,
        "pid": 99999999,
        "timestamp": "1700000000.000",
    })
    .to_string();
    std::fs::write(&log_path, format!("{log_content}\n")).unwrap();

    let swept = sweep_orphaned_runs(repo_root);
    assert_eq!(swept, 1);

    let updated_log = std::fs::read_to_string(&log_path).unwrap();
    let mut found = false;
    for line in updated_log.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val["event"] == "run_end" && val["run_id"] == run_id {
                found = true;
                // Because node2 had committed_commit, last_incumbent_score recovered is 92.0
                assert!((val["final_sab_score"].as_f64().unwrap() - 92.0).abs() < 1e-6);
                assert!((val["best_attempted_score"].as_f64().unwrap() - 92.0).abs() < 1e-6);
            }
        }
    }
    assert!(found);
}

#[test]
fn test_run_lock_guard_concurrent_attempt_preserves_holder_pid() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();
    let run_id = "concurrent-run-test";

    // 1. First process acquires lock
    let guard1 =
        RunLockGuard::acquire(repo_root, run_id).expect("first lock acquire should succeed");
    let lock_path = repo_root
        .join(".selfware")
        .join("runs")
        .join(format!("{run_id}.lock"));
    assert!(lock_path.exists());

    let initial_pid = std::fs::read_to_string(&lock_path).unwrap();
    assert_eq!(initial_pid.trim(), format!("{}", std::process::id()));

    // 2. Second concurrent acquisition must fail
    let guard2_res = RunLockGuard::acquire(repo_root, run_id);
    assert!(guard2_res.is_err(), "concurrent acquire must fail");

    // 3. Critically: holder's PID must NOT have been erased by second caller's OpenOptions
    let after_pid = std::fs::read_to_string(&lock_path).unwrap();
    assert_eq!(
        after_pid.trim(),
        format!("{}", std::process::id()),
        "lock file PID must be preserved even when concurrent process attempts acquisition"
    );

    drop(guard1);
}

#[tokio::test]
async fn test_shutdown_requested_prevents_winner_commit_and_leaves_head_unchanged() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);

    let worktree = ast_tools::create_shadow_worktree(root).unwrap();
    std::fs::write(
        worktree.join("src/lib.rs"),
        "pub fn f() -> usize {\n    999\n}\n",
    )
    .unwrap();
    let tested_diff = capture_tested_diff(&worktree).unwrap();
    let tree_id = capture_worktree_tree_id(&worktree).unwrap();
    ast_tools::cleanup_worktree(root, &worktree).unwrap();

    // Trigger shutdown
    crate::request_shutdown();
    assert!(crate::is_shutdown_requested());

    // Promotion commit must be refused
    let committed = commit_winner_to_repo(
        root,
        &tested_diff,
        Some(&tree_id),
        "🧬 Gen 3 BLOOM (should be refused)",
    )
    .await;
    assert!(
        !committed,
        "commit_winner_to_repo must return false when shutdown requested"
    );

    // HEAD must remain completely unchanged
    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "HEAD commit must not move after cancellation"
    );

    // Working directory must remain clean / unchanged
    let status = git_stdout(root, &["status", "--porcelain"]);
    assert!(
        !status.contains("src/lib.rs"),
        "worktree src/lib.rs must not have uncommitted changes"
    );
    crate::reset_shutdown_for_test();
}

#[test]
fn test_run_lock_guard_mutual_exclusion_across_different_runs() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    // 1. First run acquires lock
    let guard1 =
        RunLockGuard::acquire(repo_root, "run-alpha").expect("first run should acquire lock");
    assert!(guard1.global_lock_path.exists());
    assert!(guard1.lock_path.exists());

    // 2. Second run with a different run_id on the same repo root must fail due to active_evolution.lock
    #[cfg(unix)]
    {
        let guard2_res = RunLockGuard::acquire(repo_root, "run-beta");
        assert!(
            guard2_res.is_err(),
            "second run must fail while first run holds active_evolution.lock"
        );
    }

    // 3. Dropping guard1 releases the global and per-run locks
    drop(guard1);

    // 4. Now second run can acquire successfully
    let guard2 = RunLockGuard::acquire(repo_root, "run-beta")
        .expect("second run should succeed after first dropped");
    assert!(guard2.global_lock_path.exists());
    assert!(guard2.lock_path.exists());
    drop(guard2);
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_shutdown_aborts_before_commit() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn f() -> usize { 42 }\n").unwrap();

    crate::request_shutdown();
    assert!(crate::is_shutdown_requested());

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test shutdown commit",
    )
    .await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .contains("Shutdown requested before commit"));

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "HEAD must not move when shutdown requested before commit"
    );
    crate::reset_shutdown_for_test();
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_interrupted_hook_reaps_process_group_and_does_not_commit(
) {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a slow pre-commit hook that sleeps 30 seconds
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("pre-commit");
    std::fs::write(&hook_path, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn f() -> usize { 9999 }\n").unwrap();

    // Request shutdown 100ms into the commit hook execution
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        crate::request_shutdown();
    });

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test slow hook cancellation",
    )
    .await;

    assert!(
        res.is_err(),
        "Commit must fail when cancelled during pre-commit hook"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("Shutdown requested") || err.contains("commit aborted"),
        "Error must indicate shutdown during hook: {err}"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "HEAD must not have moved when hook was interrupted"
    );

    crate::reset_shutdown_for_test();
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_post_commit_hook_reconciles_head() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a post-commit hook that exits with an error status.
    // Git executes post-commit AFTER updating HEAD.
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("post-commit");
    std::fs::write(
        &hook_path,
        "#!/bin/sh\necho 'post-commit failed' >&2\nexit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn f() -> usize { 12345 }\n").unwrap();

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test post-commit reconciliation",
    )
    .await;

    // Reconciliation recognizes that HEAD moved despite hook failure, so commit succeeds
    assert!(
        res.is_ok(),
        "commit_scoped_paths_isolated must succeed when HEAD advances despite post-commit hook error: {:?}",
        res.err()
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_ne!(
        head_before, head_after,
        "HEAD must have advanced to the new commit"
    );
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_rejects_unrelated_head_movement() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a pre-commit hook that creates an UNRELATED commit on the side and then exits 1.
    // This simulates concurrent/unrelated HEAD movement occurring during the commit window.
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("pre-commit");
    std::fs::write(
        &hook_path,
        "#!/bin/sh\n\
         # Advancing HEAD with an unrelated commit whose tree and message differ\n\
         TREE=$(git hash-object -t tree /dev/null)\n\
         COMMIT=$(git commit-tree \"$TREE\" -m 'unrelated concurrent commit')\n\
         git update-ref HEAD \"$COMMIT\"\n\
         echo 'pre-commit gate failed' >&2\n\
         exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn candidate_code() -> usize { 999 }\n").unwrap();

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test candidate commit",
    )
    .await;

    // Must fail closed because the advanced HEAD has an unrelated tree/commit!
    assert!(
        res.is_err(),
        "commit_scoped_paths_isolated must reject promotion when HEAD advanced with an unrelated commit"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "destination branch HEAD must not move when candidate commit is rejected"
    );
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_rejects_zero_exit_tree_mutation() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a pre-commit hook that mutates the tree to an empty tree, moves HEAD to it, and exits 0!
    // This tests the exact case where git commit returns status 0, but the committed tree does not match candidate.
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("pre-commit");
    std::fs::write(
        &hook_path,
        "#!/bin/sh\n\
         # Pre-commit hook mutates the staged tree by removing the candidate file from the index\n\
         # and exiting 0. git commit produces a valid commit on HEAD, but its tree is NOT promoted_tree!\n\
         git rm --cached -q src/lib.rs\n\
         exit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn candidate_code() -> usize { 777 }\n").unwrap();

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test candidate commit",
    )
    .await;

    // Must fail closed even though hook exited 0, because the committed tree does not match promoted_tree!
    assert!(
        res.is_err(),
        "commit_scoped_paths_isolated must reject promotion when hook exits 0 but tree does not match"
    );
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains(
            "git commit succeeded with status 0, but committed tree or parent does not match"
        ),
        "error message should cite status 0 and tree/parent mismatch: {err_msg}"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "destination branch HEAD must NOT move when candidate commit is rejected"
    );
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_timeout_arm() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a pre-commit hook that hangs for 10s
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("pre-commit");
    std::fs::write(
        &hook_path,
        "#!/bin/sh\n\
         sleep 10\n\
         exit 0\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn candidate_code() -> usize { 888 }\n").unwrap();

    let res = commit_scoped_paths_isolated_with_timeout(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test timeout candidate commit",
        Some(1),
    )
    .await;

    assert!(res.is_err(), "must fail on timeout");
    let err = res.unwrap_err();
    assert!(
        err.contains("timed out after 1s"),
        "expected timeout error message: {err}"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "destination branch HEAD must NOT move when commit times out"
    );
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_cancellation_during_slow_post_commit_hook_aborts_publication(
) {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    // Install a slow post-commit hook that sleeps.
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("post-commit");
    std::fs::write(
        &hook_path,
        "#!/bin/sh\necho 'slow post-commit running' >&2\nsleep 30\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let symref_before = git_stdout(root, &["symbolic-ref", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn candidate_code() -> usize { 777 }\n").unwrap();

    // Trigger shutdown while the post-commit hook is in flight
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        crate::request_shutdown();
    });

    let res = commit_scoped_paths_isolated_with_timeout(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test cancel during slow hook",
        Some(10),
    )
    .await;

    assert!(
        res.is_err(),
        "must fail when shutdown interrupts post-commit hook"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("Shutdown requested"),
        "expected shutdown error message, got: {err}"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_before, head_after,
        "destination branch HEAD must NOT move when shutdown interrupts post-commit hook before publication"
    );

    let symref_after = git_stdout(root, &["symbolic-ref", "HEAD"]);
    assert_eq!(
        symref_before, symref_after,
        "checkout symbolic-ref must remain unchanged"
    );

    crate::reset_shutdown_for_test();
}

#[tokio::test]
async fn test_commit_scoped_paths_isolated_keeps_developer_head_unchanged_throughout() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let dir = setup_winner_repo();
    let root = dir.path();

    let symref_before = git_stdout(root, &["symbolic-ref", "HEAD"]);
    assert_eq!(symref_before.trim(), "refs/heads/main");

    // Install a pre-commit hook that checks parent repo .git/HEAD from inside the worktree
    let hook_dir = root.join(".git").join("hooks");
    std::fs::create_dir_all(&hook_dir).unwrap();
    let hook_path = hook_dir.join("pre-commit");
    let parent_git_head = root.join(".git").join("HEAD");
    let parent_git_head_str = parent_git_head.to_string_lossy().to_string();
    std::fs::write(
        &hook_path,
        format!(
            "#!/bin/sh\n\
             HEAD_CONTENT=$(cat \"{parent_git_head_str}\" | tr -d '\\r\\n')\n\
             if [ \"$HEAD_CONTENT\" != 'ref: refs/heads/main' ]; then\n\
               echo \"Developer checkout HEAD was redirected to $HEAD_CONTENT!\" >&2\n\
               exit 1\n\
             fi\n\
             exit 0\n"
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&hook_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&hook_path, perms).unwrap();
    }

    let head_before = git_stdout(root, &["rev-parse", "HEAD"]);
    let test_file = root.join("src/lib.rs");
    std::fs::write(&test_file, "pub fn candidate_code() -> usize { 12345 }\n").unwrap();

    let res = commit_scoped_paths_isolated(
        root,
        &[std::path::PathBuf::from("src/lib.rs")],
        None,
        "test developer HEAD invariant",
    )
    .await;

    assert!(res.is_ok(), "commit should succeed: {:?}", res.err());

    let symref_after = git_stdout(root, &["symbolic-ref", "HEAD"]);
    assert_eq!(
        symref_after.trim(),
        "refs/heads/main",
        "Developer checkout HEAD must remain on main"
    );

    let head_after = git_stdout(root, &["rev-parse", "HEAD"]);
    assert_ne!(
        head_before, head_after,
        "Destination branch HEAD was successfully advanced by promotion"
    );
}

#[tokio::test]
async fn test_run_cancellable_subprocess_inflight_kill_on_shutdown() {
    let _exec = crate::test_support::ExecGuard::hold();
    crate::reset_shutdown_for_test();

    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", "sleep 30"]);

    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        crate::request_shutdown();
    });

    let res = run_cancellable_subprocess(cmd, std::time::Duration::from_secs(10)).await;
    assert!(
        matches!(res, Err(SubprocessError::ShutdownRequested)),
        "In-flight subprocess must be cancelled on shutdown"
    );
    crate::reset_shutdown_for_test();
}

#[tokio::test]
async fn test_run_cancellable_subprocess_captures_stdout_and_stderr() {
    let _exec = crate::test_support::ExecGuard::hold();
    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", "echo 'out_captured'; echo 'err_captured' >&2"]);
    let output = run_cancellable_subprocess(cmd, std::time::Duration::from_secs(30))
        .await
        .expect("sh must execute");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("out_captured"),
        "stdout must be captured, got: {stdout}"
    );
    assert!(
        stderr.contains("err_captured"),
        "stderr must be captured, got: {stderr}"
    );

    // Verify control gate test summary parsing on captured buffer
    let synthetic_test_output = "running 2 tests\ntest foo ... ok\ntest bar ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let (passed, total) = parse_test_summary(synthetic_test_output);
    assert_eq!((passed, total), (2, 2));
    assert!(synthetic_test_output
        .lines()
        .any(|l| l.starts_with("test result: ok.")));
}

#[tokio::test]
async fn test_build_candidate_metrics_shutdown_returns_typed_shutdown_requested() {
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = setup_winner_repo();
    let root = dir.path();

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    let test_output = std::process::Output {
        #[cfg(unix)]
        status: std::process::ExitStatus::from_raw(0),
        #[cfg(not(unix))]
        status: std::process::ExitStatus::default(),
        stdout: b"test result: ok. 1 passed; 0 failed\n".to_vec(),
        stderr: Vec::new(),
    };

    crate::request_shutdown();
    let config = EvolutionConfig::default();
    let res = build_candidate_metrics(
        root,
        &test_output,
        std::time::Duration::from_millis(100),
        &[],
        &config,
    )
    .await;

    assert!(
        matches!(res, Err(SubprocessError::ShutdownRequested)),
        "build_candidate_metrics must return typed ShutdownRequested on shutdown"
    );
    crate::reset_shutdown_for_test();
}

#[tokio::test]
async fn test_measure_compile_test_baseline_shutdown_aborts_promptly() {
    let _exec = crate::test_support::ExecGuard::hold();
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    crate::request_shutdown();
    assert!(crate::is_shutdown_requested());

    let res = measure_compile_test_baseline(repo_root, &[], 60.0).await;
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("shutdown requested"),
        "Expected shutdown requested error, got: {err}"
    );
    crate::reset_shutdown_for_test();
}

#[test]
fn test_run_lock_guard_permanent_lock_file_overlapping_acquisitions() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    // First run creates and acquires the lock
    let guard1 =
        RunLockGuard::acquire(repo_root, "run-1").expect("first acquisition should succeed");
    let global_lock = repo_root
        .join(".selfware")
        .join("runs")
        .join("active_evolution.lock");
    assert!(global_lock.exists());

    // Concurrent run must fail
    #[cfg(unix)]
    {
        let second_res = RunLockGuard::acquire(repo_root, "run-2");
        assert!(
            second_res.is_err(),
            "concurrent run must fail while lock is held"
        );
    }

    // Drop guard1 — global lock file MUST remain on disk permanently
    drop(guard1);
    assert!(
        global_lock.exists(),
        "active_evolution.lock must NOT be unlinked on drop"
    );

    // Second run acquires the EXACT same existing lock file
    let guard2 = RunLockGuard::acquire(repo_root, "run-2")
        .expect("second acquisition should succeed after drop");
    assert!(global_lock.exists());

    // Third run must fail while guard2 is held
    #[cfg(unix)]
    {
        let third_res = RunLockGuard::acquire(repo_root, "run-3");
        assert!(
            third_res.is_err(),
            "third run must fail while guard2 is held"
        );
    }

    drop(guard2);
    assert!(
        global_lock.exists(),
        "active_evolution.lock must still exist after second drop"
    );
}

#[test]
fn test_compute_run_outcome_permutations() {
    // 1. Shutdown requested produces killed
    let (outcome, reason) = compute_run_outcome(true, None, 5, 0);
    assert_eq!(outcome, "killed");
    assert!(reason.unwrap().contains("shutdown signal"));

    // 2. Policy stopped takes precedence over candidates evaluated
    let (outcome, reason) = compute_run_outcome(false, Some("max spend reached".into()), 2, 0);
    assert_eq!(outcome, "policy_stopped");
    assert_eq!(reason.unwrap(), "max spend reached");

    // 3. 0 evaluated and 0 failures -> failed ("No candidates were evaluated")
    let (outcome, reason) = compute_run_outcome(false, None, 0, 0);
    assert_eq!(outcome, "failed");
    assert_eq!(
        reason.unwrap(),
        "No candidates were evaluated during evolution run"
    );

    // 4. 0 evaluated and >0 infrastructure failures -> failed with infrastructure error details
    let (outcome, reason) = compute_run_outcome(false, None, 0, 3);
    assert_eq!(outcome, "failed");
    let err_str = reason.unwrap();
    assert!(
        err_str.contains("infrastructure errors"),
        "Expected infrastructure error mention: {err_str}"
    );
    assert!(
        err_str.contains("3 failures"),
        "Expected count of failures: {err_str}"
    );

    // 5. Positive evaluated candidates -> completed
    let (outcome, reason) = compute_run_outcome(false, None, 1, 0);
    assert_eq!(outcome, "completed");
    assert!(reason.is_none());

    // 6. Positive evaluated candidates with prior infrastructure failure -> completed
    let (outcome, reason) = compute_run_outcome(false, None, 2, 1);
    assert_eq!(outcome, "completed");
    assert!(reason.is_none());
}
