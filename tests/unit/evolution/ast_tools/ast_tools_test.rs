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
