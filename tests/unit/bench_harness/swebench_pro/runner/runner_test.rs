use super::*;

fn dummy_instance() -> Instance {
    Instance {
        instance_id: "demo-1".into(),
        repo: "octocat/hello-world".into(),
        base_commit: "deadbeef".into(),
        problem_statement: "  fix bug ".into(),
        fail_to_pass: serde_json::json!(["tests/test_a.py::test_x"]),
        selected_test_files_to_run: serde_json::json!(["tests/test_a.py"]),
        repo_language: Some("python".into()),
        extra: Default::default(),
    }
}

fn dummy_quant(label: &str) -> QuantSpec {
    use super::super::catalog::{BackendProfile, ThinkingPolicy};

    QuantSpec {
        label: label.into(),
        gguf: "test.gguf".into(),
        alias: "test-alias".into(),
        mmproj: "mmproj.gguf".into(),
        name: "Test".into(),
        ctx: 65_536,
        max_parallel: 1,
        kv_cache_type: "q8_0".into(),
        tensor_split: None,
        temperature: 0.7,
        thinking_policy: ThinkingPolicy::Enable,
        backend: BackendProfile::LlamaCpp,
    }
}

fn dummy_opts(output: PathBuf) -> SwebenchProOpts {
    SwebenchProOpts {
        quants: vec!["q1".into()],
        instance_ids: vec![],
        instances: 1,
        scenario_timeout: Duration::from_secs(1),
        ctx: 65_536,
        parallel: 1,
        concurrency: 1,
        trials: 1,
        candidates: 1,
        output,
        selfware_bin: PathBuf::from("selfware"),
        skip_existing: false,
        llama_opts: LlamaServerOpts::default(),
        endpoint: None,
        instances_jsonl: None,
        prompt_mode: "official".into(),
        prompt_profile: "swebench_pro".into(),
        official_eval: false,
        official_eval_script: PathBuf::from("eval.py"),
        official_eval_raw_sample_path: PathBuf::from("sample.jsonl"),
        official_eval_scripts_dir: PathBuf::from("scripts"),
        official_eval_dockerhub_username: "local".into(),
        official_eval_num_workers: 1,
        official_eval_use_local_docker: true,
        official_eval_redo: false,
        official_eval_block_network: false,
        resume: false,
        force_rerun: false,
    }
}

fn trial_in_state(state: TrialState) -> TrialManifest {
    TrialManifest {
        quant: "q1".into(),
        instance_id: "org__repo-1".into(),
        trial: 1,
        state,
        started_at: None,
        completed_at: None,
        error: None,
        pred_path: None,
        result_path: None,
    }
}

#[test]
fn seed_and_rerun_partition_prevents_force_rerun_double_count() {
    // The resume seed loop seeds a trial IFF `should_skip_trial` is true, and
    // the run gate re-runs a trial IFF it is false — so a trial is seeded XOR
    // re-run, never both. This is the invariant that stops --force-rerun from
    // double-counting a completed trial in aggregate.json.
    let out = std::env::temp_dir().join("sw_partition_test");
    let mut opts = dummy_opts(out);

    // Normal resume: an Evaluated trial is skipped (seeded once, not re-run).
    opts.force_rerun = false;
    assert!(should_skip_trial(
        &opts,
        Some(&trial_in_state(TrialState::Evaluated))
    ));

    // --force-rerun: the Evaluated trial is NOT skipped → it is re-run and
    // must therefore NOT be seeded (else it lands in all_runs twice).
    opts.force_rerun = true;
    assert!(!should_skip_trial(
        &opts,
        Some(&trial_in_state(TrialState::Evaluated))
    ));

    // Failed trials are always re-run on resume → never seeded.
    for st in [
        TrialState::BootFailed,
        TrialState::CloneFailed,
        TrialState::AgentFailed,
        TrialState::Planned,
    ] {
        assert!(!should_skip_trial(&opts, Some(&trial_in_state(st))));
    }
}

#[test]
fn prompt_profile_diagnostic_includes_tests() {
    let p = PromptProfile::SwebenchPro.task_prompt(&dummy_instance(), "diagnostic");
    assert!(p.contains("[mode: diagnostic]"));
    assert!(p.contains("tests/test_a.py::test_x"));
    assert!(p.contains("RELEVANT TEST FILES: tests/test_a.py"));
}

#[test]
fn prompt_profile_official_excludes_tests() {
    let p = PromptProfile::SwebenchPro.task_prompt(&dummy_instance(), "official");
    assert!(p.contains("[mode: official]"));
    assert!(!p.contains("tests/test_a.py::test_x"));
    assert!(!p.contains("RELEVANT TEST FILES:"));
    assert!(!p.contains("FAIL-TO-PASS"));
}

#[test]
fn prompt_profile_contains_tool_contract() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            p.contains("Valid tool call ONLY"),
            "tool contract missing in {} mode",
            mode
        );
        assert!(
            p.contains("NO prose before tool XML"),
            "no-prose rule missing in {} mode",
            mode
        );
    }
}

#[test]
fn prompt_profile_contains_no_test_edit_rule() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            p.contains("Do NOT modify test files"),
            "no-test-edit rule missing in {} mode",
            mode
        );
    }
}

#[test]
fn prompt_profile_contains_verification_requirement() {
    let inst = dummy_instance();
    for mode in &["diagnostic", "official"] {
        let p = PromptProfile::SwebenchPro.task_prompt(&inst, mode);
        assert!(
            p.contains("confirm they pass before finishing"),
            "verification requirement missing in {} mode",
            mode
        );
    }
}

#[test]
fn is_test_only_patch_detects_test_only() {
    let patch = r#"diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(is_test_only_patch(patch));
}

#[test]
fn is_test_only_patch_detects_source_edit() {
    let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(!is_test_only_patch(patch));
}

#[test]
fn test_path_detection_does_not_match_substrings() {
    let patch = r#"diff --git a/src/contest.py b/src/contest.py
--- a/src/contest.py
+++ b/src/contest.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(!is_test_only_patch(patch));
    assert!(!has_test_edit_in_patch(patch));
}

#[test]
fn test_path_detection_matches_common_test_names() {
    assert!(is_test_path("tests/test_user.py"));
    assert!(is_test_path("src/foo/user_test.rs"));
    assert!(is_test_path("web/Button.test.tsx"));
    assert!(is_test_path("pkg/__tests__/button.js"));
    assert!(!is_test_path("src/contest.py"));
    assert!(!is_test_path("src/latest.rs"));
}

#[test]
fn prepare_official_eval_sample_normalizes_uppercase_test_lists() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.jsonl");
    let output = dir.path().join("out").join("sample.normalized.jsonl");
    std::fs::write(
            &input,
            r#"{"instance_id":"i1","FAIL_TO_PASS":["tests/test_a.py::test_x"],"PASS_TO_PASS":"[\"tests/test_b.py::test_y\"]"}"#,
        )
        .unwrap();

    let normalized_path = prepare_official_eval_sample(&input, &output).unwrap();
    let row: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(normalized_path).unwrap()).unwrap();

    assert_eq!(row["fail_to_pass"], r#"["tests/test_a.py::test_x"]"#);
    assert_eq!(row["pass_to_pass"], r#"["tests/test_b.py::test_y"]"#);
}

#[test]
fn nonzero_agent_exit_with_patch_is_patch_captured() {
    let result = PerRunResult {
        instance_id: "i1".into(),
        quant: "q1".into(),
        trial: 1,
        exit_code: 1,
        timed_out: false,
        wall_secs: 1.0,
        patch_lines: 3,
        patch_bytes: 42,
        pred_path: PathBuf::from("i1.pred"),
        error: String::new(),
        empty_diff: false,
        test_only_patch: false,
        has_source_edit: true,
        has_test_edit: false,
        syntax_check_passed: true,
        candidate_num: 0,
    };

    let (state, error) = trial_state_from_result(&result);

    assert_eq!(state, TrialState::PatchCaptured);
    assert!(error.is_none());
}

#[test]
fn median_handles_even_and_odd() {
    assert!((median_f64(&[1.0, 2.0, 3.0]) - 2.0).abs() < 1e-9);
    assert!((median_f64(&[1.0, 2.0, 3.0, 4.0]) - 2.5).abs() < 1e-9);
    assert_eq!(median_f64(&[]), 0.0);
}

#[test]
fn concurrency_clamping_logic() {
    use super::super::catalog::{BackendProfile, QuantSpec, ThinkingPolicy};

    let spec = QuantSpec {
        label: "test".into(),
        gguf: "test.gguf".into(),
        alias: "test-alias".into(),
        mmproj: "mmproj.gguf".into(),
        name: "Test".into(),
        ctx: 65536,
        max_parallel: 2,
        kv_cache_type: "q8_0".into(),
        tensor_split: None,
        temperature: 0.7,
        thinking_policy: ThinkingPolicy::Enable,
        backend: BackendProfile::LlamaCpp,
    };

    // When requested concurrency is below max_parallel, keep it.
    let requested = 1;
    let effective = requested.min(spec.max_parallel).max(1);
    assert_eq!(effective, 1);

    // When requested concurrency exceeds max_parallel, clamp.
    let requested = 8;
    let effective = requested.min(spec.max_parallel).max(1);
    assert_eq!(effective, 2);

    // Zero should bump to 1.
    let requested = 0;
    let effective = requested.min(spec.max_parallel).max(1);
    assert_eq!(effective, 1);
}

#[test]
fn has_test_edit_detects_test_file() {
    let patch = r#"diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(has_test_edit_in_patch(patch));
}

#[test]
fn has_test_edit_detects_no_test_file() {
    let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(!has_test_edit_in_patch(patch));
}

#[test]
fn has_test_edit_mixed_source_and_test() {
    let patch = r#"diff --git a/src/main.py b/src/main.py
--- a/src/main.py
+++ b/src/main.py
@@ -1 +1 @@
-old
+new

diff --git a/tests/test_a.py b/tests/test_a.py
--- a/tests/test_a.py
+++ b/tests/test_a.py
@@ -1 +1 @@
-old
+new
"#;
    assert!(has_test_edit_in_patch(patch));
}

#[test]
fn cheap_syntax_check_accepts_clean_patch() {
    assert!(cheap_syntax_check("diff --git a/foo.py b/foo.py\n+bar\n"));
}

#[test]
fn cheap_syntax_check_rejects_merge_conflict() {
    assert!(!cheap_syntax_check(
        "<<<<<<< HEAD\nfoo\n=======\nbar\n>>>>>>> branch\n"
    ));
}

#[test]
fn cheap_syntax_check_rejects_empty() {
    assert!(!cheap_syntax_check(""));
    assert!(!cheap_syntax_check("   \n  \n"));
}

#[test]
fn best_runs_skips_candidates_and_prefers_earliest_trial() {
    let runs = vec![
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 10,
            patch_bytes: 100,
            pred_path: PathBuf::from("/tmp/a"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 0,
        },
        // A later non-candidate run with a larger patch should not be
        // selected just because it has more lines.
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 2,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 50,
            patch_bytes: 500,
            pred_path: PathBuf::from("/tmp/b"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 0,
        },
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 100,
            patch_bytes: 1000,
            pred_path: PathBuf::from("/tmp/c"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 2,
        },
    ];
    let best = best_runs_by_quant_instance(&runs);
    assert_eq!(best.len(), 1);
    let chosen = best.values().next().unwrap();
    assert_eq!(chosen.trial, 1);
    assert_eq!(chosen.patch_lines, 10); // earliest valid trial, not largest patch
}

#[test]
fn write_aggregate_pass_at_1_vs_pass_at_k() {
    let dir = tempfile::tempdir().unwrap();

    // Write dummy pred files
    std::fs::write(dir.path().join("i1_t1.pred"), "patch1\n").unwrap();
    std::fs::write(dir.path().join("i1_c1.pred"), "patch2\n").unwrap();
    std::fs::write(dir.path().join("i1_c2.pred"), "patch3\n").unwrap();

    let runs = vec![
        // Selected best for instance i1, trial 1
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 2,
            patch_bytes: 10,
            pred_path: dir.path().join("i1_t1.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 0,
        },
        // Candidate 1: smaller diff, no test edits, syntax ok
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 1,
            patch_bytes: 5,
            pred_path: dir.path().join("i1_c1.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 1,
        },
        // Candidate 2: has test edit
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 1,
            patch_bytes: 5,
            pred_path: dir.path().join("i1_c2.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: true,
            syntax_check_passed: true,
            candidate_num: 2,
        },
    ];

    write_aggregate(dir.path(), &runs, None).unwrap();

    let agg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("aggregate.json")).unwrap()).unwrap();

    // pass@1 should be false because no official eval data
    assert_eq!(
        agg["pass_at_1_rate"], 0.0,
        "pass@1 should be 0 without official eval"
    );
    // pass@k_oracle should be true because at least one candidate has
    // source edit + no test edit + syntax ok (the selected best and candidate 1)
    assert_eq!(
        agg["pass_at_k_oracle_rate"], 1.0,
        "pass@k oracle should be 1 via proxy upper bound"
    );
    assert_eq!(
        agg["pass_at_k_oracle_is_proxy"], true,
        "should be marked proxy-based"
    );

    // Entry-level checks
    let entry = &agg["entries"][0];
    assert_eq!(entry["pass_at_1"], false);
    assert_eq!(entry["pass_at_k_oracle"], true);
    // attempted_patch_rate should be based on selected results only
    assert_eq!(entry["attempted_patch_rate"], 1.0);
}

#[test]
fn write_aggregate_does_not_proxy_pass_at_k_when_official_eval_failed() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("selected.pred"), "selected\n").unwrap();
    std::fs::write(dir.path().join("candidate.pred"), "candidate\n").unwrap();

    let runs = vec![
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 2,
            patch_bytes: 10,
            pred_path: dir.path().join("selected.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 0,
        },
        PerRunResult {
            instance_id: "i1".into(),
            quant: "q1".into(),
            trial: 1,
            exit_code: 0,
            timed_out: false,
            wall_secs: 1.0,
            patch_lines: 1,
            patch_bytes: 5,
            pred_path: dir.path().join("candidate.pred"),
            error: String::new(),
            empty_diff: false,
            test_only_patch: false,
            has_source_edit: true,
            has_test_edit: false,
            syntax_check_passed: true,
            candidate_num: 1,
        },
    ];

    let mut by_pair = BTreeMap::new();
    by_pair.insert(
        ("q1".to_string(), "i1".to_string()),
        OfficialEvalStatus {
            eval_completed: true,
            patch_applied: true,
            f2p_p2p_passed: false,
            resolved: false,
            eval_error: String::new(),
        },
    );

    write_aggregate(dir.path(), &runs, Some(&OfficialEvalMap { by_pair })).unwrap();

    let agg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("aggregate.json")).unwrap()).unwrap();

    assert_eq!(agg["pass_at_k_oracle_rate"], 0.0);
    assert_eq!(agg["pass_at_k_oracle_is_proxy"], false);
    assert_eq!(agg["entries"][0]["pass_at_k_oracle"], false);
}

#[test]
fn reconstruct_missing_result_json_is_failed_even_when_pred_exists() {
    let dir = tempfile::tempdir().unwrap();
    let opts = dummy_opts(dir.path().to_path_buf());
    let spec = dummy_quant("q1");
    let inst = dummy_instance();
    let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, 1);
    std::fs::create_dir_all(&trial_dir).unwrap();
    std::fs::write(
        trial_dir.join(format!("{}.pred", inst.instance_id)),
        "patch\n",
    )
    .unwrap();

    let result = reconstruct_result_from_disk(&opts, &spec, &inst, 1).unwrap();

    assert_ne!(result.exit_code, 0);
    assert!(result.error.contains("without result.json"));
    assert_eq!(result.patch_bytes, 6);
}

#[test]
fn reconstruct_trial_pool_recovers_candidate_subdirs() {
    // A multi-candidate trial on disk: the promoted trial-level result.json
    // (candidate_num 0) plus candidate_1/ and candidate_2/ result.json.
    // Resume must recover all three, not just the trial-level one, or the
    // candidate pool (pass@k) collapses.
    let dir = tempfile::tempdir().unwrap();
    let opts = dummy_opts(dir.path().to_path_buf());
    let spec = dummy_quant("q1");
    let inst = dummy_instance();
    let trial_dir = trial_dir_for(&opts.output, &spec.label, &inst.instance_id, 1);
    std::fs::create_dir_all(&trial_dir).unwrap();

    std::fs::write(
        trial_dir.join("result.json"),
        serde_json::to_vec(&make_candidate_result(0, false)).unwrap(),
    )
    .unwrap();
    for c in 1..=2u32 {
        let c_dir = trial_dir.join(format!("candidate_{c}"));
        std::fs::create_dir_all(&c_dir).unwrap();
        std::fs::write(
            c_dir.join("result.json"),
            serde_json::to_vec(&make_candidate_result(c, false)).unwrap(),
        )
        .unwrap();
    }

    let pool = reconstruct_trial_pool_from_disk(&opts, &spec, &inst, 1).unwrap();
    let mut nums: Vec<u32> = pool.iter().map(|r| r.candidate_num).collect();
    nums.sort();
    assert_eq!(
        nums,
        vec![0, 1, 2],
        "pool must recover the trial result AND both candidate subdirs"
    );
}

fn make_candidate_result(candidate_num: u32, has_test_edit: bool) -> PerRunResult {
    PerRunResult {
        instance_id: "i1".into(),
        quant: "q1".into(),
        trial: 1,
        exit_code: 0,
        timed_out: false,
        wall_secs: 1.0,
        patch_lines: 10,
        patch_bytes: 100,
        pred_path: PathBuf::from(format!("/tmp/c{}", candidate_num)),
        error: String::new(),
        empty_diff: false,
        test_only_patch: false,
        has_source_edit: true,
        has_test_edit,
        syntax_check_passed: true,
        candidate_num,
    }
}

#[test]
fn select_best_candidate_prefers_higher_f2p_rate() {
    let c1 = make_candidate_result(1, false);
    let c2 = make_candidate_result(2, false);
    let candidates = vec![c1, c2];
    let mut metrics = BTreeMap::new();
    metrics.insert(
        1,
        OfficialEvalMetrics {
            fail_to_pass_passed: 1,
            fail_to_pass_total: 2,
            pass_to_pass_passed: 0,
            pass_to_pass_total: 0,
            overall_pass: false,
        },
    );
    metrics.insert(
        2,
        OfficialEvalMetrics {
            fail_to_pass_passed: 2,
            fail_to_pass_total: 2,
            pass_to_pass_passed: 0,
            pass_to_pass_total: 0,
            overall_pass: true,
        },
    );
    let best = select_best_candidate(&candidates, &metrics);
    assert_eq!(best.candidate_num, 2);
}

#[test]
fn select_best_candidate_tiebreaks_p2p_then_overall() {
    let c1 = make_candidate_result(1, false);
    let c2 = make_candidate_result(2, false);
    let candidates = vec![c1, c2];
    let mut metrics = BTreeMap::new();
    // Same f2p rate, but c2 has better p2p rate.
    metrics.insert(
        1,
        OfficialEvalMetrics {
            fail_to_pass_passed: 1,
            fail_to_pass_total: 1,
            pass_to_pass_passed: 0,
            pass_to_pass_total: 1,
            overall_pass: false,
        },
    );
    metrics.insert(
        2,
        OfficialEvalMetrics {
            fail_to_pass_passed: 1,
            fail_to_pass_total: 1,
            pass_to_pass_passed: 1,
            pass_to_pass_total: 1,
            overall_pass: true,
        },
    );
    let best = select_best_candidate(&candidates, &metrics);
    assert_eq!(best.candidate_num, 2);
}

#[test]
fn select_best_candidate_falls_back_to_proxy_when_no_f2p_passed() {
    let mut c1 = make_candidate_result(1, false);
    c1.syntax_check_passed = false;
    let c2 = make_candidate_result(2, true); // has test edit -> worse proxy
    let candidates = vec![c1, c2];
    let mut metrics = BTreeMap::new();
    // No candidate passes any fail-to-pass test.
    metrics.insert(
        1,
        OfficialEvalMetrics {
            fail_to_pass_passed: 0,
            fail_to_pass_total: 2,
            pass_to_pass_passed: 0,
            pass_to_pass_total: 0,
            overall_pass: false,
        },
    );
    metrics.insert(
        2,
        OfficialEvalMetrics {
            fail_to_pass_passed: 0,
            fail_to_pass_total: 2,
            pass_to_pass_passed: 1,
            pass_to_pass_total: 1,
            overall_pass: false,
        },
    );
    let best = select_best_candidate(&candidates, &metrics);
    // Proxy ordering picks c1 because it has no test edit, even though c2
    // has a better pass-to-pass rate.
    assert_eq!(best.candidate_num, 1);
}

#[test]
fn parse_official_eval_output_computes_from_tests() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("output.json");
    std::fs::write(
        &output_path,
        r#"{"tests": [
                {"name": "tests/test_a.py::test_x", "status": "PASSED"},
                {"name": "tests/test_b.py::test_y", "status": "passed"},
                {"name": "tests/test_c.py::test_z", "status": "FAILED"}
            ]}"#,
    )
    .unwrap();

    let mut inst = dummy_instance();
    inst.fail_to_pass = serde_json::json!(["tests/test_a.py::test_x"]);
    inst.extra.insert(
        "pass_to_pass".into(),
        serde_json::json!(["tests/test_b.py::test_y", "tests/test_c.py::test_z"]),
    );

    let m = parse_official_eval_output(&output_path, &inst);
    assert_eq!(m.fail_to_pass_passed, 1);
    assert_eq!(m.fail_to_pass_total, 1);
    assert_eq!(m.pass_to_pass_passed, 1); // b passes, c fails
    assert_eq!(m.pass_to_pass_total, 2);
    assert!(!m.overall_pass);
}

#[test]
fn parse_official_eval_output_uses_direct_keys() {
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("output.json");
    std::fs::write(
        &output_path,
        r#"{
                "fail_to_pass_passed": 2,
                "fail_to_pass_total": 3,
                "pass_to_pass_passed": 4,
                "pass_to_pass_total": 5,
                "overall_pass": true
            }"#,
    )
    .unwrap();

    let m = parse_official_eval_output(&output_path, &dummy_instance());
    assert_eq!(m.fail_to_pass_passed, 2);
    assert_eq!(m.fail_to_pass_total, 3);
    assert_eq!(m.pass_to_pass_passed, 4);
    assert_eq!(m.pass_to_pass_total, 5);
    assert!(m.overall_pass);
}

#[test]
fn parse_official_eval_output_missing_file_returns_zeros() {
    let output_path = PathBuf::from("/does/not/exist/output.json");
    let m = parse_official_eval_output(&output_path, &dummy_instance());
    assert_eq!(m.fail_to_pass_passed, 0);
    assert_eq!(m.fail_to_pass_total, 0);
    assert!(!m.overall_pass);
}

#[test]
fn merge_official_metrics_into_result_adds_keys() {
    let dir = tempfile::tempdir().unwrap();
    let result_path = dir.path().join("result.json");
    std::fs::write(
        &result_path,
        r#"{"instance_id": "i1", "quant": "q1", "trial": 1}"#,
    )
    .unwrap();

    let metrics = OfficialEvalMetrics {
        fail_to_pass_passed: 1,
        fail_to_pass_total: 2,
        pass_to_pass_passed: 3,
        pass_to_pass_total: 4,
        overall_pass: true,
    };
    merge_official_metrics_into_result(&result_path, &metrics).unwrap();

    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&result_path).unwrap()).unwrap();
    assert_eq!(value["fail_to_pass_passed"], 1);
    assert_eq!(value["fail_to_pass_total"], 2);
    assert_eq!(value["pass_to_pass_passed"], 3);
    assert_eq!(value["pass_to_pass_total"], 4);
    assert_eq!(value["overall_pass"], true);
}
