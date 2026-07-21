use super::*;

#[test]
fn test_diagnostic_parsing() {
    let json = serde_json::json!({
        "level": "error",
        "message": "cannot find value `x` in this scope",
        "spans": [{
            "is_primary": true,
            "file_name": "src/main.rs",
            "line_start": 42,
            "column_start": 5,
            "text": [{ "text": "    let y = x + 1;" }]
        }]
    });

    let diag = parse_diagnostic(&json).unwrap();
    assert_eq!(diag.level, DiagnosticLevel::Error);
    assert_eq!(diag.line, 42);
    assert_eq!(diag.file, "src/main.rs");
}

#[test]
fn test_error_prompt_formatting() {
    let result = AstMutationResult::compile_failed(vec![CompilerDiagnostic {
        level: DiagnosticLevel::Error,
        message: "mismatched types".to_string(),
        file: "src/memory.rs".to_string(),
        line: 301,
        column: 12,
        span_text: "fn evict_oldest(&mut self) -> u64".to_string(),
    }]);

    let prompt = result.error_prompt();
    assert!(prompt.contains("FROST"));
    assert!(prompt.contains("mismatched types"));
    assert!(prompt.contains("memory.rs:301"));
}

#[test]
fn test_not_found_result() {
    let result = AstMutationResult::not_found("nonexistent_fn");
    assert!(!result.success);
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
fn test_diagnostic_parsing_missing_primary() {
    let json = serde_json::json!({
        "level": "error",
        "message": "some error",
        "spans": [{
            "is_primary": false,
            "file_name": "src/main.rs",
            "line_start": 1,
            "column_start": 1,
            "text": [{ "text": "code" }]
        }]
    });
    // No primary span → parse_diagnostic returns None
    assert!(parse_diagnostic(&json).is_none());
}

#[test]
fn test_diagnostic_parsing_unknown_level() {
    let json = serde_json::json!({
        "level": "ice",
        "message": "internal compiler error",
        "spans": [{
            "is_primary": true,
            "file_name": "src/main.rs",
            "line_start": 1,
            "column_start": 1,
            "text": [{ "text": "code" }]
        }]
    });
    assert!(parse_diagnostic(&json).is_none());
}

#[test]
fn test_diagnostic_parsing_missing_fields() {
    // Completely empty JSON
    let json = serde_json::json!({});
    assert!(parse_diagnostic(&json).is_none());

    // Has level but no message
    let json = serde_json::json!({
        "level": "error"
    });
    assert!(parse_diagnostic(&json).is_none());

    // Has level and message but no spans
    let json = serde_json::json!({
        "level": "error",
        "message": "test"
    });
    assert!(parse_diagnostic(&json).is_none());
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
        CompilerDiagnostic {
            level: DiagnosticLevel::Error,
            message: "type mismatch".to_string(),
            file: "src/lib.rs".to_string(),
            line: 10,
            column: 5,
            span_text: "let x: u32 = \"hello\"".to_string(),
        },
        CompilerDiagnostic {
            level: DiagnosticLevel::Warning,
            message: "unused variable".to_string(),
            file: "src/lib.rs".to_string(),
            line: 20,
            column: 9,
            span_text: String::new(), // empty span
        },
    ]);
    let prompt = result.error_prompt();
    assert!(prompt.contains("type mismatch"));
    assert!(prompt.contains("unused variable"));
    assert!(prompt.contains("lib.rs:10"));
    assert!(prompt.contains("lib.rs:20"));
    // span_text present only for first error
    assert!(prompt.contains("let x: u32"));
}

#[test]
fn test_diagnostic_all_levels() {
    for (level_str, expected) in [
        ("error", DiagnosticLevel::Error),
        ("warning", DiagnosticLevel::Warning),
        ("note", DiagnosticLevel::Note),
        ("help", DiagnosticLevel::Help),
    ] {
        let json = serde_json::json!({
            "level": level_str,
            "message": "test message",
            "spans": [{
                "is_primary": true,
                "file_name": "test.rs",
                "line_start": 1,
                "column_start": 1,
                "text": [{ "text": "code" }]
            }]
        });
        let diag = parse_diagnostic(&json).unwrap();
        assert_eq!(diag.level, expected);
    }
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
