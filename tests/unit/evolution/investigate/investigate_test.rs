use super::*;
use crate::evolution::tree_log::{compute_sha256, AttemptNode, AttemptStatus};
use std::path::Path;

fn make_test_node(id: &str, patch: &str, status: AttemptStatus) -> AttemptNode {
    let (metrics, sab_report_path) = if status == AttemptStatus::Evaluated {
        let dummy_report = std::env::temp_dir().join(format!("test_sab_report_{id}.json"));
        let report_body = serde_json::json!({
            "schema": "sab-report/1",
            "aggregate_score": 85.0,
            "scenarios_expected": 1,
            "scenarios": [{
                "name": "test_scenario",
                "score": 85.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }]
        });
        let _ = std::fs::write(&dummy_report, serde_json::to_string(&report_body).unwrap());
        (
            Some(crate::evolution::FitnessMetrics {
                sab_score: 85.0,
                tokens_used: Some(1500),
                token_budget: 100_000,
                wall_clock_secs: 1.2,
                timeout_secs: 60.0,
                full_evaluation_secs: Some(1.2),
                test_pass_pct: 100.0,
                binary_size_mb: 15.0,
                max_binary_size_mb: 50.0,
                tests_passed: 10,
                tests_total: 10,
                visual_score: 100.0,
            }),
            Some(dummy_report),
        )
    } else {
        (None, None)
    };

    AttemptNode {
        id: id.to_string(),
        parent_id: Some("att-baseline".to_string()),
        generation: 1,
        branch_id: "branch-1".to_string(),
        hypothesis_id: format!("hyp-{id}"),
        description: "Test hypothesis for investigate engine".to_string(),
        diff_sha256: compute_sha256(patch.as_bytes()),
        patch: Some(patch.to_string()),
        sab_report_path,
        metrics,
        composite_score: if status == AttemptStatus::Evaluated {
            Some(0.85)
        } else {
            None
        },
        tokens_used: Some(1500),
        wall_time_ms: 1200,
        status,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some("deadbeef0123456789".to_string()),
        committed_commit: None,
        action_type: if status == AttemptStatus::Baseline || id.contains("baseline") {
            None
        } else {
            Some(crate::evolution::ActionType::OpenRoot)
        },
        git_tree_id: None,
        created_at: "2026-09-18T20:00:00Z".to_string(),
    }
}

#[test]
fn test_scan_patch_detects_unchecked_unwrap_in_production() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/worker.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let (findings, citations) =
        scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].category, OpaqueCategory::UncheckedUnwrap);
    assert_eq!(findings[0].severity, FindingSeverity::Critical);
    assert!(findings[0].title.contains("Unchecked panic vector"));
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0].file_path, "src/tools/worker.rs");
    assert_eq!(citations[0].line_range, (0, 0));
    assert!(citations[0]
        .hyperlink
        .contains("/workspace/src/tools/worker.rs"));
}

#[test]
fn test_scan_patch_extracts_real_line_numbers_when_file_exists() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();
    std::fs::create_dir_all(repo_root.join("src/tools")).unwrap();
    std::fs::write(
        repo_root.join("src/tools/worker.rs"),
        "// header\n// context\nlet x = foo();\n// footer\n",
    )
    .unwrap();

    let patch = r#"[{"file": "src/tools/worker.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let (findings, citations) =
        scan_patch_for_opaque_structures(patch, repo_root, "att-real-lines", None);

    assert_eq!(findings.len(), 1);
    assert_eq!(citations.len(), 1);
    // Line range grounded in search text at line 3 of the actual file
    assert_eq!(citations[0].line_range, (3, 3));
    assert!(citations[0]
        .hyperlink
        .ends_with("src/tools/worker.rs#L3-L3"));
}

#[test]
fn test_extract_symbols_from_patch_real_extraction() {
    let patch = r#"[{"file": "src/algo.rs", "search": "// empty", "replace": "pub fn compute_hash() {}\npub struct StateTracker {}\npub enum ErrorKind {}"}]"#;
    let symbols = extract_symbols_from_patch(patch);
    assert_eq!(symbols, vec!["ErrorKind", "StateTracker", "compute_hash"]);

    let empty_patch = r#"[{"file": "src/algo.rs", "search": "x = 1;", "replace": "x = 2;"}]"#;
    let empty_symbols = extract_symbols_from_patch(empty_patch);
    assert!(
        empty_symbols.is_empty(),
        "Should be empty when no symbols declared"
    );
}

#[test]
fn test_extract_touched_files_and_safety_gate_on_unified_diff() {
    let repo_root = Path::new("/workspace");
    let patch = "--- a/src/evolution/daemon.rs\n+++ b/src/evolution/daemon.rs\n@@ -1,3 +1,3 @@\n-old\n+new\n";
    let touched = extract_touched_files(patch);
    assert_eq!(touched, vec!["src/evolution/daemon.rs"]);

    let node = make_test_node("att-diff-protected", patch, AttemptStatus::Evaluated);
    let dossier = investigate_attempt(&node, repo_root);

    // Protected path in unified diff must be detected
    assert!(!dossier.degrees.degree_5_safety.protected_paths_clean);
    assert!(dossier.consensus.has_safety_veto);
    assert_eq!(
        dossier.consensus.decision,
        GovernanceDecision::HardRejectVeto
    );
}

#[test]
fn test_simulate_10000_reviewer_governance_rejects_unverified_build_failed() {
    // Finding 2: BuildFailed must NEVER receive approval, even with 0 heuristic findings
    let node = make_test_node("att-build-failed", "", AttemptStatus::BuildFailed);
    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: false,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.decision, GovernanceDecision::HardRejectVeto);
    assert_eq!(consensus.votes_approve, 0);
    assert!(consensus.votes_veto >= 2500);
}

#[test]
fn test_scan_patch_detects_implicit_constant() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/runner.rs", "search": "let timeout = default;", "replace": "let timeout = 5000;"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::ImplicitConstant));
}

#[test]
fn test_scan_patch_detects_undocumented_public_api() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/api.rs", "search": "// empty", "replace": "pub fn execute_action() -> bool { true }"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::UndocumentedPublicApi));
}

#[test]
fn test_scan_patch_detects_silent_fallback() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/loader.rs", "search": "let v = get_data();", "replace": "let v = get_data().unwrap_or_default();"}]"#;
    let (findings, _) = scan_patch_for_opaque_structures(patch, repo_root, "att-test", None);

    assert!(findings
        .iter()
        .any(|f| f.category == OpaqueCategory::SilentFallback));
}

#[test]
fn test_simulate_10000_reviewer_governance_veto_on_safety() {
    let node = make_test_node("att-safety-fail", "", AttemptStatus::SafetyRejected);
    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: false,
        rule1_verified: false,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.total_reviewers, 10000);
    assert!(consensus.has_safety_veto);
    assert_eq!(consensus.decision, GovernanceDecision::HardRejectVeto);
    assert!(consensus.votes_veto >= 3500);
}

#[test]
fn test_simulate_10000_reviewer_governance_approve_on_clean_mutation() {
    let node = make_test_node("att-clean", "", AttemptStatus::Evaluated);
    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: true,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.total_reviewers, 10000);
    assert!(!consensus.has_safety_veto);
    assert_eq!(consensus.decision, GovernanceDecision::ApproveForPromotion);
    assert_eq!(consensus.votes_approve, 10000);
    assert_eq!(consensus.consensus_score, 1.0);
}

#[test]
fn test_investigate_attempt_full_6_degrees() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/helper.rs", "search": "// a", "replace": "// b"}]"#;
    let node = make_test_node("att-full", patch, AttemptStatus::Evaluated);

    let dossier = investigate_attempt(&node, repo_root);
    assert_eq!(dossier.attempt_id, "att-full");
    assert_eq!(
        dossier.degrees.degree_1_intent.hypothesis_id,
        "hyp-att-full"
    );
    assert_eq!(
        dossier.degrees.degree_2_syntax.files_touched,
        vec!["src/tools/helper.rs"]
    );
    assert_eq!(
        dossier.degrees.degree_4_empirical.status,
        AttemptStatus::Evaluated
    );
    assert!(dossier.degrees.degree_5_safety.protected_paths_clean);
    assert_eq!(dossier.degrees.degree_6_lineage.generation, 1);
    assert_eq!(
        dossier.consensus.decision,
        GovernanceDecision::ApproveForPromotion
    );
}

#[test]
fn test_export_markdown_renders_table_and_consensus() {
    let repo_root = Path::new("/workspace");
    let patch = r#"[{"file": "src/tools/worker.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let node = make_test_node("att-doc", patch, AttemptStatus::Evaluated);

    let dossier = investigate_attempt(&node, repo_root);
    let md = export_markdown(&dossier);

    assert!(md.contains("# RSI Investigative Review Dossier: `att-doc`"));
    assert!(md.contains("## 1. The 6 Degrees of Grounded Connection"));
    assert!(md.contains("## 2. Opaque Structure Findings"));
    assert!(md.contains("## 3. Maximum-Power Grounded Citations"));
    assert!(md.contains("## 4. Synthetic Heuristic Scoring (10,000-Reviewer Projection Model)"));
    assert!(md.contains("Frontier Safety & Boundary Compliance"));
    assert!(md.contains("Merkle Tree: `Unmeasured / Not recorded in attempt node`"));
}

#[test]
fn test_scan_patch_detects_blast_radius_and_contract_breakage() {
    let repo_root = Path::new("/workspace");
    // 1. JSON edit modifying protected path
    let patch_blast =
        r#"[{"file": "src/safety/checker.rs", "search": "x = 1;", "replace": "x = 2;"}]"#;
    let (findings_blast, _) =
        scan_patch_for_opaque_structures(patch_blast, repo_root, "att-blast", None);
    assert!(findings_blast
        .iter()
        .any(|f| f.category == OpaqueCategory::BlastRadiusLeak));

    // 2. JSON edit deleting pub fn contract
    let patch_contract = r#"[{"file": "src/tools/api.rs", "search": "pub fn old_api() -> bool { true }", "replace": "fn old_api() -> bool { true }"}]"#;
    let (findings_contract, _) =
        scan_patch_for_opaque_structures(patch_contract, repo_root, "att-contract", None);
    assert!(findings_contract
        .iter()
        .any(|f| f.category == OpaqueCategory::ContractBreakage));

    // 3. Unified diff deleting pub fn contract
    let patch_diff_contract = "--- a/src/tools/api.rs\n+++ b/src/tools/api.rs\n@@ -1,3 +1,1 @@\n-pub fn deleted_api() {}\n";
    let (findings_diff, _) =
        scan_patch_for_opaque_structures(patch_diff_contract, repo_root, "att-diff-contract", None);
    assert!(findings_diff
        .iter()
        .any(|f| f.category == OpaqueCategory::ContractBreakage));
}

#[test]
fn test_investigation_report_path_is_protected() {
    use crate::evolution::is_protected;
    assert!(is_protected(Path::new(
        ".selfware/investigation_report_latest.md"
    )));
    assert!(is_protected(Path::new(".selfware/attempts/run_123.jsonl")));
}

#[test]
fn test_find_affected_callers_real_lookup() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();
    std::fs::create_dir_all(repo_root.join("src/module_a")).unwrap();
    std::fs::create_dir_all(repo_root.join("src/module_b")).unwrap();

    std::fs::write(
        repo_root.join("src/module_a/lib.rs"),
        "pub fn target_action() {}\n",
    )
    .unwrap();
    std::fs::write(
        repo_root.join("src/module_b/caller.rs"),
        "use crate::module_a::target_action;\nfn call_it() { target_action(); }\n",
    )
    .unwrap();

    let symbols = vec!["target_action".to_string()];
    let files_touched = vec!["src/module_a/lib.rs".to_string()];
    let callers = find_affected_callers(repo_root, &symbols, &files_touched);

    assert_eq!(callers.len(), 1);
    assert!(callers[0].contains("src/module_b/caller.rs"));
    assert!(callers[0].contains("target_action"));
}

#[test]
fn test_make_citation_hashes_lines_at_base_commit_not_dirty_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git cmd failed");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test Runner"]);

    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    let original_content = "pub fn greet() {\n    let x = foo();\n    println!(\"done\");\n}\n";
    std::fs::write(repo_root.join("src/lib.rs"), original_content).unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "base commit"]);
    let base_commit = run_git(&["rev-parse", "HEAD"]);

    // Dirty worktree change: mutate the line in the working tree
    let dirty_content = "pub fn greet() {\n    let x = bar();\n    println!(\"done\");\n}\n";
    std::fs::write(repo_root.join("src/lib.rs"), dirty_content).unwrap();

    // Patch targets the code at base_commit
    let patch = r#"[{"file": "src/lib.rs", "search": "let x = foo();", "replace": "let x = foo().unwrap();"}]"#;
    let (findings, citations) = scan_patch_for_opaque_structures(
        patch,
        repo_root,
        "att-citation-commit",
        Some(&base_commit),
    );

    assert_eq!(findings.len(), 1);
    assert_eq!(citations.len(), 1);
    let cit = &citations[0];
    assert_eq!(cit.file_path, "src/lib.rs");
    assert_eq!(cit.line_range, (2, 2));
    assert_eq!(cit.git_commit.as_deref(), Some(base_commit.as_str()));

    // Offending evidence excerpt displays the actual defect with unwrap
    assert_eq!(cit.exact_excerpt, "let x = foo().unwrap();");
    let expected_hash = compute_sha256("let x = foo().unwrap();".as_bytes());
    assert_eq!(cit.content_hash, expected_hash);

    // Pre-mutation baseline excerpt extracted from base_commit
    assert_eq!(cit.before_excerpt.as_deref(), Some("    let x = foo();"));
    assert_eq!(
        cit.after_excerpt.as_deref(),
        Some("let x = foo().unwrap();")
    );

    // Hyperlink points to clickable file URI for local navigation
    assert!(cit.hyperlink.starts_with("file://"));
    assert!(cit.hyperlink.ends_with("src/lib.rs#L2-L2"));
}

#[test]
fn test_validate_benchmark_report_structural_checks() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path();

    let node = make_test_node("att-bench-struct", "", AttemptStatus::Evaluated);

    // 1. Nonexistent path
    let missing = dir.join("missing.json");
    let err = validate_benchmark_report(&missing, &node).unwrap_err();
    assert!(err.contains("does not exist"));

    // 2. Directory instead of file
    let subdir = dir.join("sub_dir");
    std::fs::create_dir(&subdir).unwrap();
    let err = validate_benchmark_report(&subdir, &node).unwrap_err();
    assert!(err.contains("not a regular file"));

    // 3. 0-byte file
    let empty_file = dir.join("empty.json");
    std::fs::write(&empty_file, "").unwrap();
    let err = validate_benchmark_report(&empty_file, &node).unwrap_err();
    assert!(err.contains("empty (0 bytes)"));

    // 4. Corrupted JSON
    let corrupt_file = dir.join("corrupt.json");
    std::fs::write(&corrupt_file, "{not_valid_json}").unwrap();
    let err = validate_benchmark_report(&corrupt_file, &node).unwrap_err();
    assert!(err.contains("corrupted JSON"));

    // 5. Schema mismatch
    let bad_schema = dir.join("bad_schema.json");
    let bad_body = serde_json::json!({
        "schema": "unsupported-schema/99",
        "aggregate_score": 85.0
    });
    std::fs::write(&bad_schema, serde_json::to_string(&bad_body).unwrap()).unwrap();
    let err = validate_benchmark_report(&bad_schema, &node).unwrap_err();
    assert!(err.contains("schema mismatch"));

    // 6. Score mismatch
    let score_mismatch = dir.join("score_mismatch.json");
    let mismatch_body = serde_json::json!({
        "schema": "sab-report/1",
        "aggregate_score": 42.0, // expected 85.0
        "scenarios_expected": 1,
        "scenarios": [{
            "name": "s1",
            "score": 42.0,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(
        &score_mismatch,
        serde_json::to_string(&mismatch_body).unwrap(),
    )
    .unwrap();
    let err = validate_benchmark_report(&score_mismatch, &node).unwrap_err();
    assert!(err.contains("score mismatch"));

    // 7. Binary sha256 mismatch
    let mut node_with_sha = node.clone();
    node_with_sha.binary_sha256 = Some("expected_binary_hash_123456".to_string());
    let sha_mismatch = dir.join("sha_mismatch.json");
    let sha_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "wrong_hash_9999",
        "aggregate_score": 85.0,
        "scenarios_expected": 1,
        "scenarios": [{
            "name": "s1",
            "score": 85.0,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(&sha_mismatch, serde_json::to_string(&sha_body).unwrap()).unwrap();
    let err = validate_benchmark_report(&sha_mismatch, &node_with_sha).unwrap_err();
    assert!(err.contains("binary_sha256 mismatch"));

    // 8. Structurally valid report matching candidate
    let valid_file = dir.join("valid.json");
    let valid_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "expected_binary_hash_123456",
        "aggregate_score": 85.0,
        "scenarios_expected": 1,
        "scenarios": [{
            "name": "s1",
            "score": 85.0,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(&valid_file, serde_json::to_string(&valid_body).unwrap()).unwrap();
    assert!(validate_benchmark_report(&valid_file, &node_with_sha).is_ok());

    // 9. Duplicate scenario names rejected
    let dup_file = dir.join("dup_names.json");
    let dup_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "expected_binary_hash_123456",
        "aggregate_score": 85.0,
        "scenarios_expected": 2,
        "scenarios": [
            {
                "name": "dup_scenario",
                "score": 85.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            },
            {
                "name": "dup_scenario",
                "score": 85.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }
        ]
    });
    std::fs::write(&dup_file, serde_json::to_string(&dup_body).unwrap()).unwrap();
    let err_dup = validate_benchmark_report(&dup_file, &node_with_sha).unwrap_err();
    assert!(err_dup.contains("duplicate scenario name"));

    // 10. Out-of-range score rejected
    let oob_file = dir.join("oob_score.json");
    let oob_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "expected_binary_hash_123456",
        "aggregate_score": 105.0,
        "scenarios_expected": 1,
        "scenarios": [{
            "name": "s1",
            "score": 105.0,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(&oob_file, serde_json::to_string(&oob_body).unwrap()).unwrap();
    let err_oob = validate_benchmark_report(&oob_file, &node_with_sha).unwrap_err();
    assert!(err_oob.contains("invalid score"));

    // 11. Missing required boolean field (broken_tests_fixed) rejected
    let missing_field_file = dir.join("missing_field.json");
    let missing_field_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "expected_binary_hash_123456",
        "aggregate_score": 85.0,
        "scenarios_expected": 1,
        "scenarios": [{
            "name": "s1",
            "score": 85.0,
            "tests_passed": true,
            // broken_tests_fixed missing
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(
        &missing_field_file,
        serde_json::to_string(&missing_field_body).unwrap(),
    )
    .unwrap();
    let err_missing = validate_benchmark_report(&missing_field_file, &node_with_sha).unwrap_err();
    assert!(err_missing.contains("broken_tests_fixed"));

    // 12. Missing scenarios_expected rejected
    let no_expected_file = dir.join("no_expected.json");
    let no_expected_body = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "expected_binary_hash_123456",
        "aggregate_score": 85.0,
        "scenarios": [{
            "name": "s1",
            "score": 85.0,
            "tests_passed": true,
            "broken_tests_fixed": false,
            "clean_exit": true,
            "duration_secs": 1
        }]
    });
    std::fs::write(
        &no_expected_file,
        serde_json::to_string(&no_expected_body).unwrap(),
    )
    .unwrap();
    let err_no_exp = validate_benchmark_report(&no_expected_file, &node_with_sha).unwrap_err();
    assert!(err_no_exp.contains("scenarios_expected"));
}

#[test]
fn test_simulate_10000_reviewer_governance_vetoes_on_corrupt_benchmark_report() {
    let temp = tempfile::tempdir().unwrap();
    let report_path = temp.path().join("corrupt_report.json");
    std::fs::write(&report_path, "not json content").unwrap();

    let mut node = make_test_node("att-corrupt-bench", "", AttemptStatus::Evaluated);
    node.sab_report_path = Some(report_path);

    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: true,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(consensus.decision, GovernanceDecision::HardRejectVeto);
    assert!(consensus
        .deliberation_summary
        .contains("VETO: Integrity failure"));
}

#[test]
fn test_sweep_orphaned_runs_closes_unended_runs() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    let log_path = repo_root.join(".evolution-log.jsonl");
    let sample_log = concat!(
        "{\"event\":\"start\",\"run_id\":\"run_1\"}\n",
        "{\"event\":\"start\",\"run_id\":\"run_2\"}\n",
        "{\"event\":\"run_end\",\"run_id\":\"run_1\",\"outcome\":\"completed\"}\n",
        "{\"event\":\"start\",\"run_id\":\"run_3\"}\n"
    );
    std::fs::write(&log_path, sample_log).unwrap();

    // Run startup sweep
    let closed = crate::evolution::daemon::sweep_orphaned_runs(repo_root);
    assert_eq!(closed, 2, "run_2 and run_3 should be marked as killed");

    // Second sweep should find 0 orphaned runs
    let closed_again = crate::evolution::daemon::sweep_orphaned_runs(repo_root);
    assert_eq!(closed_again, 0);

    // Verify log content has run_end events with outcome: killed
    let updated_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(updated_content.contains("\"run_id\":\"run_2\""));
    assert!(updated_content.contains("\"outcome\":\"killed\""));
    assert!(updated_content.contains("\"run_id\":\"run_3\""));
}

#[test]
fn test_simulate_10000_reviewer_governance_blocks_approval_when_benchmark_missing() {
    let mut node = make_test_node("att-no-bench", "", AttemptStatus::Evaluated);
    node.sab_report_path = None; // benchmark report missing

    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: true,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(
        consensus.decision,
        GovernanceDecision::ConditionalClarification,
        "Approval must be blocked when benchmark report is missing"
    );
    assert!(consensus.votes_approve < 10000);
    assert!(
        consensus
            .deliberation_summary
            .contains("ConditionalClarification")
            || consensus.deliberation_summary.contains("80.0%")
    );
}

#[test]
fn test_simulate_10000_reviewer_governance_vetoes_on_diff_hash_mismatch() {
    let mut node = make_test_node("att-hash-mismatch", "valid_patch", AttemptStatus::Evaluated);
    node.diff_sha256 =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(); // forged hash

    let findings = Vec::new();
    let safety = Degree5Safety {
        protected_paths_clean: true,
        rule1_verified: true,
        merkle_tree_equality: None,
        has_killswitch_bypass: false,
    };

    let consensus = simulate_10000_reviewer_governance(&node, &findings, &safety);
    assert_eq!(
        consensus.decision,
        GovernanceDecision::HardRejectVeto,
        "Cryptographic patch mismatch must trigger hard reject veto"
    );
    assert!(consensus.has_safety_veto || consensus.votes_veto >= 1800);
}

#[test]
fn test_validate_benchmark_report_resolved_relative_path_and_schema() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    let rel_dir = std::path::PathBuf::from(".selfware/reports/sab-test1");
    let full_dir = repo_root.join(&rel_dir);
    std::fs::create_dir_all(&full_dir).unwrap();

    let rel_report = rel_dir.join("sab_report.json");
    let full_report = repo_root.join(&rel_report);

    let valid_report = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "fake_sha_for_test",
        "aggregate_score": 85.0,
        "scenarios_expected": 2,
        "scenarios": [
            {
                "name": "scenario_a",
                "score": 80.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            },
            {
                "name": "scenario_b",
                "score": 90.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }
        ]
    });
    std::fs::write(&full_report, serde_json::to_string(&valid_report).unwrap()).unwrap();

    let metrics = crate::evolution::FitnessMetrics {
        sab_score: 85.0,
        tokens_used: Some(500),
        token_budget: 100_000,
        wall_clock_secs: 2.5,
        timeout_secs: 60.0,
        full_evaluation_secs: Some(2.5),
        test_pass_pct: 85.0,
        binary_size_mb: 4.0,
        max_binary_size_mb: 50.0,
        tests_passed: 17,
        tests_total: 20,
        visual_score: 100.0,
    };

    let mut node = make_test_node("att-test1", "", AttemptStatus::Evaluated);
    node.metrics = Some(metrics);

    // Resolving relative path with repo_root succeeds
    let res = validate_benchmark_report_resolved(&rel_report, Some(repo_root), &node);
    assert!(
        res.is_ok(),
        "Valid relative report must validate successfully: {:?}",
        res.err()
    );

    // Schema mismatch fails
    let bad_schema = serde_json::json!({
        "schema": "unknown-schema/99",
        "aggregate_score": 85.0,
        "scenarios_expected": 1,
        "scenarios": [
            {
                "name": "scenario_a",
                "score": 85.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }
        ]
    });
    std::fs::write(&full_report, serde_json::to_string(&bad_schema).unwrap()).unwrap();
    let res_schema = validate_benchmark_report_resolved(&rel_report, Some(repo_root), &node);
    assert!(res_schema.is_err());
    assert!(res_schema.unwrap_err().contains("schema mismatch"));

    // Inconsistent aggregate score vs scenario average fails
    let inconsistent_score = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "fake_sha_for_test",
        "aggregate_score": 50.0, // scenario average is (80+90)/2 = 85.0 != 50.0
        "scenarios_expected": 2,
        "scenarios": [
            {
                "name": "scenario_a",
                "score": 80.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            },
            {
                "name": "scenario_b",
                "score": 90.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }
        ]
    });
    std::fs::write(
        &full_report,
        serde_json::to_string(&inconsistent_score).unwrap(),
    )
    .unwrap();
    let res_score = validate_benchmark_report_resolved(&rel_report, Some(repo_root), &node);
    assert!(res_score.is_err());
    assert!(res_score.unwrap_err().contains("scenario average"));
}

#[test]
fn test_export_markdown_renders_honest_rule1_labels() {
    let repo_root = Path::new("/test_repo");

    // 1. Evaluated node with test suite passing -> "Candidate Test Suite Passed"
    let mut eval_node = make_test_node("att-eval", "add feature", AttemptStatus::Evaluated);
    eval_node.metrics = Some(crate::evolution::FitnessMetrics {
        sab_score: 80.0,
        tokens_used: Some(100),
        token_budget: 1000,
        wall_clock_secs: 1.0,
        timeout_secs: 60.0,
        full_evaluation_secs: Some(1.0),
        test_pass_pct: 100.0,
        binary_size_mb: 2.0,
        max_binary_size_mb: 50.0,
        tests_passed: 10,
        tests_total: 10,
        visual_score: 100.0,
    });
    let eval_dossier = investigate_attempt(&eval_node, repo_root);
    let eval_md = export_markdown(&eval_dossier);
    assert!(eval_md.contains("Rule 1: `Candidate Test Suite Passed`"));
    assert!(!eval_md.contains("compile+test+fmt+clippy green"));

    // 2. Baseline node -> "Baseline Benchmark Verified"
    let mut base_node = make_test_node("att-base", "baseline run", AttemptStatus::Baseline);
    base_node.metrics = Some(crate::evolution::FitnessMetrics {
        sab_score: 75.0,
        tokens_used: None,
        token_budget: 1000,
        wall_clock_secs: 1.0,
        timeout_secs: 60.0,
        full_evaluation_secs: Some(1.0),
        test_pass_pct: 75.0,
        binary_size_mb: 2.0,
        max_binary_size_mb: 50.0,
        tests_passed: 3,
        tests_total: 4,
        visual_score: 100.0,
    });
    let base_dossier = investigate_attempt(&base_node, repo_root);
    let base_md = export_markdown(&base_dossier);
    assert!(base_md.contains("Rule 1: `Baseline Benchmark Verified`"));

    // 3. Failing node -> "Unverified / Failing Gates"
    let fail_node = make_test_node("att-fail", "bad candidate", AttemptStatus::CompileFailed);
    let fail_dossier = investigate_attempt(&fail_node, repo_root);
    let fail_md = export_markdown(&fail_dossier);
    assert!(fail_md.contains("Rule 1: `Unverified / Failing Gates`"));
}

#[test]
fn test_validate_benchmark_report_resolved_rejects_path_traversal() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path().join("repo");
    let outside_dir = temp.path().join("outside");
    std::fs::create_dir_all(&repo_root).unwrap();
    std::fs::create_dir_all(&outside_dir).unwrap();

    let outside_file = outside_dir.join("leaked_report.json");
    let valid_report = serde_json::json!({
        "schema": "sab-report/1",
        "binary_sha256": "fake_sha",
        "aggregate_score": 80.0,
        "scenarios_expected": 1,
        "scenarios": [
            {
                "name": "scenario_a",
                "score": 80.0,
                "tests_passed": true,
                "broken_tests_fixed": false,
                "clean_exit": true,
                "duration_secs": 1
            }
        ]
    });
    std::fs::write(&outside_file, serde_json::to_string(&valid_report).unwrap()).unwrap();

    let mut node = make_test_node("att-traversal", "", AttemptStatus::Evaluated);
    node.metrics = Some(crate::evolution::FitnessMetrics {
        sab_score: 80.0,
        tokens_used: Some(100),
        token_budget: 1000,
        wall_clock_secs: 1.0,
        timeout_secs: 60.0,
        full_evaluation_secs: Some(1.0),
        test_pass_pct: 100.0,
        binary_size_mb: 2.0,
        max_binary_size_mb: 50.0,
        tests_passed: 1,
        tests_total: 1,
        visual_score: 100.0,
    });

    // Attempt traversal using relative path with ../
    let traversal_rel = Path::new("../outside/leaked_report.json");
    let res = validate_benchmark_report_resolved(traversal_rel, Some(&repo_root), &node);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(
        err.contains("escapes repository root") || err.contains("traversal denied"),
        "Expected traversal error but got: {err}"
    );
}

#[test]
fn test_investigate_citations_export_immutable_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    // Initialize git repo and commit a file
    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {:?} failed", args);
        output
    };

    run_git(&["init"]);
    run_git(&["config", "user.email", "test@selfware.ai"]);
    run_git(&["config", "user.name", "Test Agent"]);

    let src_dir = repo_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let test_file = src_dir.join("main.rs");
    let original_code =
        "fn main() {\n    let secret = 100;\n    println!(\"val: {}\", secret);\n}\n";
    std::fs::write(&test_file, original_code).unwrap();

    run_git(&["add", "."]);
    run_git(&["commit", "-m", "initial commit"]);
    let commit_out = run_git(&["rev-parse", "HEAD"]);
    let commit_sha = String::from_utf8_lossy(&commit_out.stdout)
        .trim()
        .to_string();

    let diff = "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -2,2 +2,2 @@\n-    let secret = 100;\n+    let secret = 200;\n";
    let mut node = make_test_node("att-snap-test", diff, AttemptStatus::Evaluated);
    node.base_commit = Some(commit_sha.clone());

    let dossier = investigate_attempt(&node, repo_root);
    assert!(
        !dossier.citations.is_empty(),
        "citations should be extracted from patch"
    );

    for citation in &dossier.citations {
        assert!(
            citation
                .hyperlink
                .contains(&format!(".selfware/snapshots/{commit_sha}")),
            "Citation hyperlink '{}' must point to immutable snapshot dir for commit {}",
            citation.hyperlink,
            commit_sha
        );
        let snapshot_path = repo_root
            .join(".selfware")
            .join("snapshots")
            .join(&commit_sha)
            .join(&citation.file_path);
        assert!(
            snapshot_path.exists(),
            "Snapshot file must be written to disk at {}",
            snapshot_path.display()
        );
        let content = std::fs::read_to_string(&snapshot_path).unwrap();
        assert_eq!(content, original_code);
    }
}

#[test]
fn test_investigate_citations_traversal_path_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    // Initialize real git repo
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(repo_root)
        .output();
    let src_dir = repo_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("lib.rs"), "fn hello() {}\n").unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_root)
        .output();

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    // Malicious edit attempting directory traversal with an opaque defect (.unwrap())
    let edits = serde_json::json!([
        {
            "file": "../escaped_secret.rs",
            "search": "fn old() {}",
            "replace": "fn bad() { let _ = x.unwrap(); }"
        }
    ]);
    let patch = serde_json::to_string(&edits).unwrap();

    let (findings, _citations) =
        scan_patch_for_opaque_structures(&patch, repo_root, "att-snap-traversal", Some(&head_sha));

    assert!(
        !findings.is_empty(),
        "Must detect unchecked unwrap in patch"
    );
    let citation = &findings[0].citation;
    assert_eq!(
        citation.hyperlink, "unavailable://invalid-path/../escaped_secret.rs",
        "Traversal path must fail closed with unavailable link"
    );

    // Escaped file or directory must not have been created anywhere
    let escaped_path = repo_root.join("../escaped_secret.rs");
    assert!(
        !escaped_path.exists(),
        "Parent traversal path must not be created"
    );
    let snapshot_escaped = repo_root.join(".selfware/snapshots/escaped_secret.rs");
    assert!(
        !snapshot_escaped.exists(),
        "Snapshot directory escape must not occur"
    );
}

#[test]
fn test_investigate_citations_refreshes_corrupted_cached_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let repo_root = temp.path();

    // Initialize real git repo
    let _ = Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(repo_root)
        .output();
    let src_dir = repo_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let original_content = "pub fn honest_code() {\n    // valid baseline\n}\n";
    std::fs::write(src_dir.join("lib.rs"), original_content).unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_root)
        .output();

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    // Plant a corrupted snapshot on disk
    let snapshot_file = repo_root
        .join(".selfware")
        .join("snapshots")
        .join(&head_sha)
        .join("src/lib.rs");
    std::fs::create_dir_all(snapshot_file.parent().unwrap()).unwrap();
    std::fs::write(&snapshot_file, "corrupted garbage content").unwrap();

    // Patch targeting src/lib.rs with an unwrap defect
    let edits = serde_json::json!([
        {
            "file": "src/lib.rs",
            "search": "honest_code()",
            "replace": "honest_code() { let _ = x.unwrap(); }"
        }
    ]);
    let patch = serde_json::to_string(&edits).unwrap();

    let (findings, _citations) =
        scan_patch_for_opaque_structures(&patch, repo_root, "att-refresh-test", Some(&head_sha));

    assert!(!findings.is_empty());
    let citation = &findings[0].citation;
    assert!(citation.hyperlink.starts_with("file://"));

    // Corrupted snapshot must have been detected, verified against Git, and overwritten with real content
    let refreshed_content = std::fs::read_to_string(&snapshot_file).unwrap();
    assert_eq!(
        refreshed_content, original_content,
        "Corrupted snapshot must be refreshed from Git"
    );
}

#[test]
fn test_scan_patch_failed_snapshot_replacement_fails_closed() {
    let _guard = crate::test_support::ExecGuard::hold();
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path();

    let _ = Command::new("git")
        .args(["init"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.email", "evo@test"])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["config", "user.name", "Evo Test"])
        .current_dir(repo_root)
        .output();
    let src_dir = repo_root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let original_content = "pub fn secure_code() {\n    // valid baseline\n}\n";
    std::fs::write(src_dir.join("lib.rs"), original_content).unwrap();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_root)
        .output();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_root)
        .output();

    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

    // Create a directory at snapshot_file path so rename fails and read fails (unreplaceable invalid destination)
    let snapshot_file = repo_root
        .join(".selfware")
        .join("snapshots")
        .join(&head_sha)
        .join("src/lib.rs");
    std::fs::create_dir_all(&snapshot_file).unwrap();

    let edits = serde_json::json!([
        {
            "file": "src/lib.rs",
            "search": "secure_code()",
            "replace": "secure_code() { let _ = x.unwrap(); }"
        }
    ]);
    let patch = serde_json::to_string(&edits).unwrap();

    let (findings, _citations) =
        scan_patch_for_opaque_structures(&patch, repo_root, "att-fail-replace", Some(&head_sha));

    assert!(!findings.is_empty());
    let citation = &findings[0].citation;
    // Failed replacement must FAIL CLOSED to unavailable:// instead of issuing a file:// link to invalid destination
    assert!(
        citation.hyperlink.starts_with("unavailable://"),
        "Citation must fail closed with unavailable:// when snapshot replacement fails: {}",
        citation.hyperlink
    );
}
