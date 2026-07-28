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
