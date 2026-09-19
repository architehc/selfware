use super::*;
use crate::evolve::diagnostics::DiagnosticSpan;

fn diag(level: &str, message: &str, span: Option<DiagnosticSpan>) -> CompilerDiagnostic {
    CompilerDiagnostic {
        level: level.to_string(),
        code: None,
        message: message.to_string(),
        rendered: None,
        spans: span.into_iter().collect(),
    }
}

fn span(file: &str, line: usize, column: usize, label: Option<&str>) -> DiagnosticSpan {
    DiagnosticSpan {
        file: file.to_string(),
        line_start: line,
        line_end: line,
        column_start: column,
        column_end: column + 1,
        is_primary: true,
        label: label.map(str::to_string),
    }
}

#[test]
fn test_error_prompt_formatting() {
    let result = AstMutationResult::compile_failed(vec![diag(
        "error",
        "mismatched types",
        Some(span(
            "src/memory.rs",
            301,
            12,
            Some("fn evict_oldest(&mut self) -> u64"),
        )),
    )]);

    let prompt = result.error_prompt();
    assert!(prompt.contains("FROST"));
    assert!(prompt.contains("mismatched types"));
    assert!(prompt.contains("memory.rs:301"));
    assert!(prompt.contains("fn evict_oldest"));
}

#[test]
fn test_not_found_result() {
    let result = AstMutationResult::not_found("nonexistent_fn");
    assert!(!result.success);
    assert_eq!(result.compiler_errors.len(), 1);
    assert_eq!(result.compiler_errors[0].level, "error");
    assert!(result.compiler_errors[0].spans.is_empty());
    assert!(result.error_prompt().contains("nonexistent_fn"));
}

#[test]
fn test_is_protected_from_parent() {
    use super::super::is_protected;
    assert!(is_protected(Path::new("src/evolution/ast_tools.rs")));
    assert!(!is_protected(Path::new("src/tools/file_edit.rs")));
}

#[test]
fn test_error_prompt_success_case() {
    let result = AstMutationResult {
        success: true,
        compiler_errors: vec![],
        diff: "some diff".to_string(),
        worktree_path: Some(PathBuf::from("/tmp/test")),
    };
    assert_eq!(result.error_prompt(), "Mutation compiled successfully.");
}

#[test]
fn test_compile_failed_empty_errors() {
    let result = AstMutationResult::compile_failed(vec![]);
    assert!(!result.success);
    assert!(result.compiler_errors.is_empty());
    assert!(result.diff.is_empty());
    assert!(result.worktree_path.is_none());
    // error_prompt should still show FROST header even with no errors
    let prompt = result.error_prompt();
    assert!(prompt.contains("FROST"));
}

#[test]
fn test_uuid_short_uniqueness() {
    let a = uuid_short();
    // Small sleep to ensure different nanos
    std::thread::sleep(std::time::Duration::from_millis(1));
    let b = uuid_short();
    assert_ne!(a, b, "Two uuid_short calls should produce different values");
}

#[test]
fn test_error_prompt_multiple_errors() {
    let result = AstMutationResult::compile_failed(vec![
        diag(
            "error",
            "type mismatch",
            Some(span("src/lib.rs", 10, 5, Some("let x: u32 = \"hello\""))),
        ),
        diag(
            "warning",
            "unused variable",
            Some(span("src/lib.rs", 20, 9, None)),
        ),
    ]);
    let prompt = result.error_prompt();
    assert!(prompt.contains("type mismatch"));
    assert!(prompt.contains("unused variable"));
    assert!(prompt.contains("lib.rs:10"));
    assert!(prompt.contains("lib.rs:20"));
    // span label present only for first error
    assert!(prompt.contains("let x: u32"));
}

#[test]
fn test_error_prompt_without_spans() {
    // Diagnostics with no spans still render level + message.
    let result = AstMutationResult::compile_failed(vec![diag("error", "link failure", None)]);
    let prompt = result.error_prompt();
    assert!(prompt.contains("[error] link failure"));
}

#[test]
fn test_worktree_error_display() {
    let git_err = WorktreeError::GitFailed("branch conflict".to_string());
    assert!(format!("{}", git_err).contains("branch conflict"));

    let io_err = WorktreeError::IoError(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    assert!(format!("{}", io_err).contains("IO error"));
}

#[test]
fn test_restore_worktree_parent_state() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path();
    let test_file = root.join("lib.rs");
    std::fs::write(&test_file, "fn initial() {}\n").unwrap();

    let patch1 = serde_json::json!([{
        "file": "lib.rs",
        "search": "fn initial() {}",
        "replace": "fn step_one() {}"
    }])
    .to_string();

    let patch2 = serde_json::json!([{
        "file": "lib.rs",
        "search": "fn step_one() {}",
        "replace": "fn step_two() {}"
    }])
    .to_string();

    let attempts_file = root.join("attempts.jsonl");
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
        composite_score: Some(50.0),
        tokens_used: None,
        wall_time_ms: 0,
        status: crate::evolution::tree_log::AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        created_at: "2026-09-17T00:00:00Z".into(),
    };

    let node_1 = crate::evolution::tree_log::AttemptNode {
        id: "att-1".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-1".into(),
        hypothesis_id: "hyp-1".into(),
        description: "step 1".into(),
        diff_sha256: "1".into(),
        patch: Some(patch1),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(60.0),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        created_at: "2026-09-17T00:01:00Z".into(),
    };

    let node_2 = crate::evolution::tree_log::AttemptNode {
        id: "att-2".into(),
        parent_id: Some("att-1".into()),
        generation: 2,
        branch_id: "branch-1".into(),
        hypothesis_id: "hyp-2".into(),
        description: "step 2".into(),
        diff_sha256: "2".into(),
        patch: Some(patch2),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(70.0),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: None,
        committed_commit: None,
        created_at: "2026-09-17T00:02:00Z".into(),
    };

    let lines = format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&node_baseline).unwrap(),
        serde_json::to_string(&node_1).unwrap(),
        serde_json::to_string(&node_2).unwrap(),
    );
    std::fs::write(&attempts_file, lines).unwrap();

    // 1. None or att-baseline returns Ok(empty) without touching files
    let res_none = restore_worktree_parent_state(root, &attempts_file, None).unwrap();
    assert!(res_none.is_empty());
    let res_base =
        restore_worktree_parent_state(root, &attempts_file, Some("att-baseline")).unwrap();
    assert!(res_base.is_empty());
    assert_eq!(
        std::fs::read_to_string(&test_file).unwrap(),
        "fn initial() {}\n"
    );

    // 2. Restoring att-1 applies patch1
    let res_1 = restore_worktree_parent_state(root, &attempts_file, Some("att-1")).unwrap();
    assert_eq!(res_1, vec!["att-1"]);
    assert_eq!(
        std::fs::read_to_string(&test_file).unwrap(),
        "fn step_one() {}\n"
    );

    // 3. Restoring att-2 from att-1 state applies patch2
    let res_2 = restore_worktree_parent_state(root, &attempts_file, Some("att-2")).unwrap();
    assert_eq!(res_2, vec!["att-2"]);
    assert_eq!(
        std::fs::read_to_string(&test_file).unwrap(),
        "fn step_two() {}\n"
    );

    // 4. Unknown parent error
    let res_err = restore_worktree_parent_state(root, &attempts_file, Some("att-unknown"));
    assert!(res_err.is_err());
}

#[test]
fn test_sibling_restoration_after_another_branch_committed() {
    let temp_dir = tempfile::tempdir().unwrap();
    let repo_root = temp_dir.path();

    // 1. Initialize git repo
    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git failed");
        assert!(output.status.success(), "git command failed: {:?}", args);
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&["init"]);
    run_git(&["config", "user.name", "Selfware Test"]);
    run_git(&["config", "user.email", "test@selfware.ai"]);

    let lib_rs = repo_root.join("src").join("lib.rs");
    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(
        &lib_rs,
        "pub fn common() -> &'static str { \"baseline\" }\n",
    )
    .unwrap();

    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "Initial commit C0"]);
    let c0 = run_git(&["rev-parse", "HEAD"]);

    // 2. Prepare patches for sibling A and sibling B that touch the SAME lines
    let patch_a = serde_json::json!([{
        "file": "src/lib.rs",
        "search": "pub fn common() -> &'static str { \"baseline\" }",
        "replace": "pub fn common() -> &'static str { \"branch_a\" }"
    }])
    .to_string();

    let patch_b = serde_json::json!([{
        "file": "src/lib.rs",
        "search": "pub fn common() -> &'static str { \"baseline\" }",
        "replace": "pub fn common() -> &'static str { \"branch_b\" }"
    }])
    .to_string();

    let attempts_file = repo_root.join("attempts.jsonl");
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
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-17T00:00:00Z".into(),
    };

    let node_a = crate::evolution::tree_log::AttemptNode {
        id: "att-sibling-a".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-a".into(),
        hypothesis_id: "hyp-a".into(),
        description: "sibling a".into(),
        diff_sha256: "sha-a".into(),
        patch: Some(patch_a),
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
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-17T00:01:00Z".into(),
    };

    let node_b = crate::evolution::tree_log::AttemptNode {
        id: "att-sibling-b".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-b".into(),
        hypothesis_id: "hyp-b".into(),
        description: "sibling b".into(),
        diff_sha256: "sha-b".into(),
        patch: Some(patch_b),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.65),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-17T00:02:00Z".into(),
    };

    let lines = format!(
        "{}\n{}\n{}\n",
        serde_json::to_string(&node_baseline).unwrap(),
        serde_json::to_string(&node_a).unwrap(),
        serde_json::to_string(&node_b).unwrap(),
    );
    std::fs::write(&attempts_file, lines).unwrap();

    // 3. Promote/commit Sibling A to main repo, advancing HEAD to commit C1
    std::fs::write(
        &lib_rs,
        "pub fn common() -> &'static str { \"branch_a\" }\n",
    )
    .unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "Promote sibling A to main (C1)"]);
    let c1 = run_git(&["rev-parse", "HEAD"]);
    assert_ne!(c0, c1, "HEAD must have moved to C1");

    // 4. Now restore Sibling B in a shadow worktree
    let worktree_b =
        create_shadow_worktree_for_parent(repo_root, &attempts_file, Some("att-sibling-b"))
            .expect("Sibling B must restore cleanly despite HEAD having moved to C1");

    // 5. Verify the restored worktree contains Sibling B's state, NOT Sibling A's commit
    let restored_file = worktree_b.join("src").join("lib.rs");
    let restored_content = std::fs::read_to_string(&restored_file).unwrap();
    assert!(
        restored_content.contains("\"branch_b\""),
        "Restored worktree must contain Sibling B's change"
    );
    assert!(
        !restored_content.contains("\"branch_a\""),
        "Restored worktree must NOT contain promoted Sibling A changes from moving HEAD"
    );

    // 6. Cleanup worktree
    cleanup_worktree(repo_root, &worktree_b).unwrap();
}

#[test]
fn test_restore_worktree_promoted_parent_short_circuit() {
    // Verify that when a parent has committed_commit: Some(C1),
    // restoring the parent checks out C1 directly and does not re-apply the diff.
    let temp_repo = tempfile::tempdir().unwrap();
    let repo_root = temp_repo.path();

    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git cmd failed");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test Runner"]);

    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    let lib_rs = repo_root.join("src").join("lib.rs");
    std::fs::write(&lib_rs, "pub fn v0() -> i32 { 0 }\n").unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "C0: Baseline"]);
    let c0 = run_git(&["rev-parse", "HEAD"]);

    let patch_a = r#"[{"path": "src/lib.rs", "search": "pub fn v0() -> i32 { 0 }", "replace": "pub fn v0() -> i32 { 0 }\npub fn v1() -> i32 { 1 }"}]"#.to_string();

    // Now promote A to repo_root, advancing HEAD to C1
    std::fs::write(
        &lib_rs,
        "pub fn v0() -> i32 { 0 }\npub fn v1() -> i32 { 1 }\n",
    )
    .unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "C1: Promoted A"]);
    let c1 = run_git(&["rev-parse", "HEAD"]);
    assert_ne!(c0, c1);

    let attempts_file = repo_root.join("attempts.jsonl");
    let node_baseline = crate::evolution::tree_log::AttemptNode {
        id: "att-baseline".into(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".into(),
        hypothesis_id: "baseline".into(),
        description: "Baseline".into(),
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
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-17T00:00:00Z".into(),
    };
    // Node A records committed_commit: Some(c1)
    let node_a = crate::evolution::tree_log::AttemptNode {
        id: "att-a".into(),
        parent_id: Some("att-baseline".into()),
        generation: 1,
        branch_id: "branch-a".into(),
        hypothesis_id: "hyp-a".into(),
        description: "Branch A (promoted)".into(),
        diff_sha256: "sha-a".into(),
        patch: Some(patch_a),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.80),
        tokens_used: None,
        wall_time_ms: 10,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0.clone()),
        committed_commit: Some(c1.clone()),
        created_at: "2026-09-17T00:01:00Z".into(),
    };

    let lines = format!(
        "{}\n{}\n",
        serde_json::to_string(&node_baseline).unwrap(),
        serde_json::to_string(&node_a).unwrap(),
    );
    std::fs::write(&attempts_file, lines).unwrap();

    // Now restore a shadow worktree for parent "att-a" (as would be done for a refinement of A)
    let worktree_refine =
        create_shadow_worktree_for_parent(repo_root, &attempts_file, Some("att-a"))
            .expect("Parent A must restore cleanly via committed_commit short-circuit");

    // The worktree HEAD must be exactly C1
    let wt_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&worktree_refine)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap();
    assert_eq!(wt_head, c1);

    // The working copy must be clean (no unstaged changes, diff should be empty)
    let diff_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&worktree_refine)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap();
    assert!(
        diff_output.is_empty(),
        "Restored worktree must be clean at C1: {}",
        diff_output
    );

    cleanup_worktree(repo_root, &worktree_refine).unwrap();
}

#[test]
fn test_strict_patch_fails_on_divergent_code() {
    let temp_repo = tempfile::tempdir().unwrap();
    let repo_root = temp_repo.path();

    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git cmd failed");
        assert!(output.status.success());
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test Runner"]);

    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn completely_different() {}\n",
    )
    .unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "initial"]);

    // A unified diff expecting entirely different context lines
    let bad_patch = r#"--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
-pub fn expected_function() {
+pub fn modified_function() {
     println!("hello");
 }
"#;

    let res = apply_strict_patch(repo_root, bad_patch);
    assert!(!res, "apply_strict_patch must fail on divergent context");
}

#[test]
fn test_restore_worktree_candidate_with_new_file() {
    let temp_repo = tempfile::tempdir().unwrap();
    let repo_root = temp_repo.path();

    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git cmd failed");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test Runner"]);

    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(repo_root.join("src/lib.rs"), "pub fn root() {}\n").unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "initial"]);
    let c0 = run_git(&["rev-parse", "HEAD"]);

    // Unified diff creating a completely new file
    let patch_new_file = "diff --git a/src/new_mod.rs b/src/new_mod.rs\nnew file mode 100644\n--- /dev/null\n+++ b/src/new_mod.rs\n@@ -0,0 +1,3 @@\n+pub fn new_feature() -> bool {\n+    true\n+}\n".to_string();

    // Create a temporary worktree to capture the exact canonical diff produced by this patch
    let wt = create_shadow_worktree(repo_root).unwrap();
    assert!(apply_strict_patch(&wt, &patch_new_file));
    let add_out = std::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["add", "-A"])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert!(add_out.status.success());
    let diff_out = std::process::Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["diff", "--cached", "--binary", "HEAD"])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert!(diff_out.status.success());
    let canonical_diff = String::from_utf8_lossy(&diff_out.stdout).to_string();
    let diff_sha = crate::evolution::tree_log::compute_sha256(canonical_diff.as_bytes());
    cleanup_worktree(repo_root, &wt).unwrap();

    let attempts_file = repo_root.join("attempts.jsonl");
    let node_baseline = crate::evolution::tree_log::AttemptNode {
        id: "att-base".into(),
        parent_id: None,
        generation: 0,
        branch_id: "base".into(),
        hypothesis_id: "base".into(),
        description: "Base".into(),
        diff_sha256: "0".into(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.5),
        tokens_used: None,
        wall_time_ms: 0,
        status: crate::evolution::tree_log::AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-18T00:00:00Z".into(),
    };

    let node_candidate = crate::evolution::tree_log::AttemptNode {
        id: "att-newfile".into(),
        parent_id: Some("att-base".into()),
        generation: 1,
        branch_id: "branch-newfile".into(),
        hypothesis_id: "hyp-newfile".into(),
        description: "Candidate adding new file".into(),
        diff_sha256: diff_sha,
        patch: Some(patch_new_file),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.9),
        tokens_used: None,
        wall_time_ms: 100,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0),
        committed_commit: None,
        created_at: "2026-09-18T00:01:00Z".into(),
    };

    let lines = format!(
        "{}\n{}\n",
        serde_json::to_string(&node_baseline).unwrap(),
        serde_json::to_string(&node_candidate).unwrap()
    );
    std::fs::write(&attempts_file, lines).unwrap();

    // Restoration must succeed and verify diff fidelity against newly added file
    let restored_wt =
        create_shadow_worktree_for_parent(repo_root, &attempts_file, Some("att-newfile"))
            .expect("Parent adding new file must restore cleanly with staged diff fidelity");

    assert!(restored_wt.join("src/new_mod.rs").exists());
    let content = std::fs::read_to_string(restored_wt.join("src/new_mod.rs")).unwrap();
    assert!(content.contains("pub fn new_feature()"));

    cleanup_worktree(repo_root, &restored_wt).unwrap();
}

#[test]
fn test_promote_refine_promote() {
    let temp_repo = tempfile::tempdir().unwrap();
    let repo_root = temp_repo.path();

    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(args)
            .current_dir(repo_root)
            .output()
            .expect("git cmd failed");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };

    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@example.com"]);
    run_git(&["config", "user.name", "Test Runner"]);

    std::fs::create_dir_all(repo_root.join("src")).unwrap();
    std::fs::write(
        repo_root.join("src/lib.rs"),
        "pub fn root() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    run_git(&["add", "src/lib.rs"]);
    run_git(&["commit", "-m", "initial C0"]);
    let c0 = run_git(&["rev-parse", "HEAD"]);

    let attempts_file = repo_root.join("attempts.jsonl");

    // Baseline attempt at C0
    let node_baseline = crate::evolution::tree_log::AttemptNode {
        id: "att-baseline".into(),
        parent_id: None,
        generation: 0,
        branch_id: "baseline".into(),
        hypothesis_id: "baseline".into(),
        description: "Initial baseline measurement".into(),
        diff_sha256: "0".into(),
        patch: None,
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.5),
        tokens_used: None,
        wall_time_ms: 0,
        status: crate::evolution::tree_log::AttemptStatus::Baseline,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0.clone()),
        committed_commit: None,
        created_at: "2026-09-18T00:00:00Z".into(),
    };
    std::fs::write(
        &attempts_file,
        format!("{}\n", serde_json::to_string(&node_baseline).unwrap()),
    )
    .unwrap();

    // ─── Gen 0: Candidate 1 modifies root() to return 2 ───
    let wt1 = create_shadow_worktree_for_parent(repo_root, &attempts_file, Some("att-baseline"))
        .expect("Gen 0 worktree from baseline must succeed");
    std::fs::write(wt1.join("src/lib.rs"), "pub fn root() -> u32 {\n    2\n}\n").unwrap();

    let diff1 = {
        let add = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["add", "-A"])
            .current_dir(&wt1)
            .output()
            .unwrap();
        assert!(add.status.success());
        let diff = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["diff", "--cached", "--binary", "HEAD"])
            .current_dir(&wt1)
            .output()
            .unwrap();
        assert!(diff.status.success());
        String::from_utf8_lossy(&diff.stdout).to_string()
    };
    let tree1 = crate::evolution::daemon::capture_worktree_tree_id(&wt1)
        .expect("Must capture tree1 digest");
    cleanup_worktree(repo_root, &wt1).unwrap();

    // Promote Candidate 1 to repo
    let ok1 = crate::evolution::daemon::commit_winner_to_repo(
        repo_root,
        &diff1,
        Some(&tree1),
        "Gen 0 promotion",
    );
    assert!(ok1, "Gen 0 candidate promotion must succeed");
    let c1 = run_git(&["rev-parse", "HEAD"]);
    assert_ne!(c1, c0, "C1 must advance past C0");

    let node_cand1 = crate::evolution::tree_log::AttemptNode {
        id: "att-cand1".into(),
        parent_id: Some("att-baseline".into()),
        generation: 0,
        branch_id: "branch-0".into(),
        hypothesis_id: "hyp-0".into(),
        description: "Candidate 1 (Gen 0)".into(),
        diff_sha256: crate::evolution::tree_log::compute_sha256(diff1.as_bytes()),
        patch: Some(diff1),
        sab_report_path: None,
        metrics: None,
        composite_score: Some(0.8),
        tokens_used: None,
        wall_time_ms: 100,
        status: crate::evolution::tree_log::AttemptStatus::Evaluated,
        failure_class: None,
        failure_reason: None,
        output_tail: None,
        binary_sha256: None,
        base_commit: Some(c0),
        committed_commit: None,
        created_at: "2026-09-18T00:01:00Z".into(),
    };
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&attempts_file)
        .unwrap();
    use std::io::Write;
    writeln!(f, "{}", serde_json::to_string(&node_cand1).unwrap()).unwrap();

    // Record committed commit anchor for att-cand1
    crate::evolution::tree_log::AttemptTree::record_committed_commit(
        &attempts_file,
        "att-cand1",
        &c1,
    )
    .expect("Must record committed_commit anchor");

    // ─── Gen 1: Candidate 2 refines Candidate 1, modifying root() to return 3 ───
    let wt2 = create_shadow_worktree_for_parent(repo_root, &attempts_file, Some("att-cand1"))
        .expect("Gen 1 worktree refining att-cand1 must succeed");
    let restored_content = std::fs::read_to_string(wt2.join("src/lib.rs")).unwrap();
    assert!(
        restored_content.contains("2"),
        "Restored worktree for att-cand1 must contain Gen 0 state (return 2)"
    );

    std::fs::write(wt2.join("src/lib.rs"), "pub fn root() -> u32 {\n    3\n}\n").unwrap();

    let diff2 = {
        let add = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["add", "-A"])
            .current_dir(&wt2)
            .output()
            .unwrap();
        assert!(add.status.success());
        let diff = std::process::Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["diff", "--cached", "--binary", "HEAD"])
            .current_dir(&wt2)
            .output()
            .unwrap();
        assert!(diff.status.success());
        String::from_utf8_lossy(&diff.stdout).to_string()
    };
    let tree2 = crate::evolution::daemon::capture_worktree_tree_id(&wt2)
        .expect("Must capture tree2 digest");
    cleanup_worktree(repo_root, &wt2).unwrap();

    // Promote Candidate 2 to repo
    let ok2 = crate::evolution::daemon::commit_winner_to_repo(
        repo_root,
        &diff2,
        Some(&tree2),
        "Gen 1 promotion",
    );
    assert!(
        ok2,
        "Gen 1 candidate refining Gen 0 must promote cleanly with tree verification"
    );
    let c2 = run_git(&["rev-parse", "HEAD"]);
    assert_ne!(c2, c1, "C2 must advance past C1");

    let final_content = std::fs::read_to_string(repo_root.join("src/lib.rs")).unwrap();
    assert!(
        final_content.contains("3"),
        "Repo root must contain final Gen 1 state (return 3)"
    );
}
