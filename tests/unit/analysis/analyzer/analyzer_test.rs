use super::*;

#[test]
fn test_error_analyzer_new() {
    let analyzer = ErrorAnalyzer::new();
    assert!(!analyzer.patterns.is_empty());
}

#[test]
fn test_analyze_e0425() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0425"),
        "cannot find value `foo` in this scope",
        "src/main.rs",
        Some(10),
        Some(5),
    );

    assert_eq!(error.category, ErrorCategory::UnresolvedImport);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_analyze_e0308() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0308"),
        "mismatched types: expected `String`, found `&str`",
        "src/lib.rs",
        Some(20),
        None,
    );

    assert_eq!(error.category, ErrorCategory::TypeError);
    assert_eq!(error.priority, 1); // Highest priority
}

#[test]
fn test_analyze_e0382() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0382"),
        "use of moved value: `data`",
        "src/lib.rs",
        Some(15),
        None,
    );

    assert_eq!(error.category, ErrorCategory::BorrowError);
    assert!(error.suggestion.is_some());
    assert!(error.suggestion.unwrap().notes.unwrap().contains("clone"));
}

#[test]
fn test_analyze_batch_prioritizes() {
    let analyzer = ErrorAnalyzer::new();
    let errors = analyzer.analyze_batch(&[
        (None, "unused variable: `x`", "src/main.rs", Some(5), None),
        (
            Some("E0308"),
            "mismatched types",
            "src/main.rs",
            Some(10),
            None,
        ),
        (
            Some("E0433"),
            "unresolved import",
            "src/main.rs",
            Some(1),
            None,
        ),
    ]);

    // Type error should be first
    assert_eq!(errors[0].code.as_deref(), Some("E0308"));
}

#[test]
fn test_group_by_category() {
    let analyzer = ErrorAnalyzer::new();
    let errors = vec![
        analyzer.analyze(Some("E0308"), "mismatched types", "a.rs", None, None),
        analyzer.analyze(Some("E0308"), "mismatched types", "b.rs", None, None),
        analyzer.analyze(Some("E0433"), "unresolved import", "c.rs", None, None),
    ];

    let groups = analyzer.group_by_category(&errors);
    assert_eq!(
        groups.get(&ErrorCategory::TypeError).map(|v| v.len()),
        Some(2)
    );
    assert_eq!(
        groups
            .get(&ErrorCategory::UnresolvedImport)
            .map(|v| v.len()),
        Some(1)
    );
}

#[test]
fn test_first_to_fix() {
    let analyzer = ErrorAnalyzer::new();
    let errors = vec![
        analyzer.analyze(None, "unused variable", "a.rs", None, None),
        analyzer.analyze(Some("E0308"), "mismatched types", "b.rs", None, None),
    ];

    let first = analyzer.first_to_fix(&errors);
    assert!(first.is_some());
    assert_eq!(first.unwrap().code.as_deref(), Some("E0308"));
}

#[test]
fn test_error_category_priority() {
    assert!(ErrorCategory::TypeError.priority() < ErrorCategory::UnusedWarning.priority());
    assert!(ErrorCategory::BorrowError.priority() < ErrorCategory::StyleWarning.priority());
}

#[test]
fn test_extract_identifier() {
    let message = "cannot find value `foo` in this scope";
    let result = extract_identifier(message, "cannot find value `", "`");
    assert_eq!(result, Some("foo"));
}

#[test]
fn test_extract_between() {
    let message = "expected `String`, found `&str`";
    let result = extract_between(message, "expected `", "`");
    assert_eq!(result, Some("String".to_string()));
}

#[test]
fn test_summary() {
    let analyzer = ErrorAnalyzer::new();
    let errors = vec![
        analyzer.analyze(Some("E0308"), "mismatched types", "a.rs", None, None),
        analyzer.analyze(None, "unused variable", "b.rs", None, None),
    ];

    let summary = analyzer.summary(&errors);
    assert!(summary.contains("Total errors: 2"));
    assert!(summary.contains("By category:"));
}

#[test]
fn test_unused_warning_detection() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(None, "unused variable: `x`", "src/main.rs", Some(5), None);

    assert_eq!(error.category, ErrorCategory::UnusedWarning);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_e0599_no_method() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0599"),
        "no method named `foo` found for struct `Bar`",
        "src/main.rs",
        None,
        None,
    );

    assert_eq!(error.category, ErrorCategory::TraitError);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_e0106_lifetime() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0106"),
        "missing lifetime specifier",
        "src/lib.rs",
        None,
        None,
    );

    assert_eq!(error.category, ErrorCategory::LifetimeError);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_e0277_trait_bound() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0277"),
        "the trait bound `Foo: Clone` is not satisfied",
        "src/lib.rs",
        None,
        None,
    );

    assert_eq!(error.category, ErrorCategory::TraitError);
}

#[test]
fn test_fix_suggestion_auto_fixable() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(None, "unused import: `std::fmt`", "src/lib.rs", None, None);

    assert!(error
        .suggestion
        .as_ref()
        .map(|s| s.auto_fixable)
        .unwrap_or(false));
}

#[test]
fn test_dead_code_warning() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        None,
        "function is never used: `foo` [dead_code]",
        "src/lib.rs",
        None,
        None,
    );

    assert_eq!(error.category, ErrorCategory::UnusedWarning);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_analyzed_error_serialization() {
    let error = AnalyzedError {
        code: Some("E0308".to_string()),
        message: "test".to_string(),
        file: "test.rs".to_string(),
        line: Some(1),
        column: Some(1),
        category: ErrorCategory::TypeError,
        priority: 1,
        suggestion: None,
        related_errors: vec![],
    };

    let json = serde_json::to_string(&error).unwrap();
    assert!(json.contains("E0308"));
}

#[test]
fn test_error_category_all_priorities() {
    let categories = [
        ErrorCategory::TypeError,
        ErrorCategory::UnresolvedImport,
        ErrorCategory::BorrowError,
        ErrorCategory::LifetimeError,
        ErrorCategory::TraitError,
        ErrorCategory::ArgumentError,
        ErrorCategory::PatternError,
        ErrorCategory::UnusedWarning,
        ErrorCategory::StyleWarning,
        ErrorCategory::Other,
    ];

    for cat in categories {
        let priority = cat.priority();
        assert!(priority > 0);
    }
}

#[test]
fn test_error_category_clone() {
    let cat = ErrorCategory::BorrowError;
    let cloned = cat;
    assert_eq!(cat, cloned);
}

#[test]
fn test_error_category_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(ErrorCategory::TypeError);
    set.insert(ErrorCategory::BorrowError);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_fix_suggestion_clone() {
    let fix = FixSuggestion {
        description: "Add clone()".to_string(),
        fix_code: Some(".clone()".to_string()),
        confidence: 0.8,
        auto_fixable: false,
        notes: Some("Note".to_string()),
    };

    let cloned = fix.clone();
    assert_eq!(fix.description, cloned.description);
    assert_eq!(fix.confidence, cloned.confidence);
}

#[test]
fn test_fix_suggestion_serde() {
    let fix = FixSuggestion {
        description: "Fix it".to_string(),
        fix_code: None,
        confidence: 0.5,
        auto_fixable: true,
        notes: None,
    };

    let json = serde_json::to_string(&fix).unwrap();
    assert!(json.contains("Fix it"));

    let parsed: FixSuggestion = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.description, fix.description);
}

#[test]
fn test_analyzed_error_clone() {
    let error = AnalyzedError {
        code: Some("E0001".to_string()),
        message: "error".to_string(),
        file: "test.rs".to_string(),
        line: Some(10),
        column: Some(5),
        category: ErrorCategory::Other,
        priority: 10,
        suggestion: None,
        related_errors: vec!["related".to_string()],
    };

    let cloned = error.clone();
    assert_eq!(error.code, cloned.code);
    assert_eq!(error.file, cloned.file);
}

#[test]
fn test_analyzed_error_deserialize() {
    let json = r#"{
            "code": "E0425",
            "message": "cannot find value",
            "file": "main.rs",
            "line": 5,
            "column": null,
            "category": "unresolved_import",
            "priority": 2,
            "suggestion": null,
            "related_errors": []
        }"#;

    let error: AnalyzedError = serde_json::from_str(json).unwrap();
    assert_eq!(error.code, Some("E0425".to_string()));
    assert_eq!(error.category, ErrorCategory::UnresolvedImport);
}

#[test]
fn test_analyzer_default() {
    let analyzer = ErrorAnalyzer::default();
    let error = analyzer.analyze(None, "test", "test.rs", None, None);
    assert_eq!(error.category, ErrorCategory::Other);
}

#[test]
fn test_e0382_moved_value() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0382"),
        "use of moved value: `x`",
        "src/main.rs",
        Some(10),
        Some(5),
    );

    assert_eq!(error.category, ErrorCategory::BorrowError);
    assert!(error.suggestion.is_some());
    assert!(error.suggestion.as_ref().unwrap().fix_code.is_some());
}

#[test]
fn test_e0502_borrow_conflict() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0502"),
        "cannot borrow `x` as mutable because it is also borrowed as immutable",
        "src/lib.rs",
        None,
        None,
    );

    assert_eq!(error.category, ErrorCategory::BorrowError);
}

#[test]
fn test_e0425_cannot_find_value() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0425"),
        "cannot find value `undefined_var` in this scope",
        "src/main.rs",
        Some(5),
        None,
    );

    assert_eq!(error.category, ErrorCategory::UnresolvedImport);
    assert!(error.suggestion.is_some());
}

#[test]
fn test_e0433_unresolved_import() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0433"),
        "unresolved import `foo::bar`",
        "src/lib.rs",
        Some(1),
        None,
    );

    assert_eq!(error.category, ErrorCategory::UnresolvedImport);
}

#[test]
fn test_analyze_batch_empty() {
    let analyzer = ErrorAnalyzer::new();
    let errors = analyzer.analyze_batch(&[]);
    assert!(errors.is_empty());
}

#[test]
fn test_first_to_fix_empty() {
    let analyzer = ErrorAnalyzer::new();
    let errors: Vec<AnalyzedError> = vec![];
    let first = analyzer.first_to_fix(&errors);
    assert!(first.is_none());
}

#[test]
fn test_group_by_category_empty() {
    let analyzer = ErrorAnalyzer::new();
    let errors: Vec<AnalyzedError> = vec![];
    let groups = analyzer.group_by_category(&errors);
    assert!(groups.is_empty());
}

#[test]
fn test_extract_identifier_not_found() {
    let message = "some other message";
    let result = extract_identifier(message, "cannot find `", "`");
    assert!(result.is_none());
}

#[test]
fn test_error_category_serde() {
    let cat = ErrorCategory::LifetimeError;
    let json = serde_json::to_string(&cat).unwrap();
    assert!(json.contains("lifetime_error"));

    let parsed: ErrorCategory = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, cat);
}

#[test]
fn test_analyze_with_all_fields() {
    let analyzer = ErrorAnalyzer::new();
    let error = analyzer.analyze(
        Some("E0308"),
        "expected `i32`, found `String`",
        "/path/to/file.rs",
        Some(42),
        Some(10),
    );

    assert_eq!(error.code, Some("E0308".to_string()));
    assert_eq!(error.file, "/path/to/file.rs");
    assert_eq!(error.line, Some(42));
    assert_eq!(error.column, Some(10));
}
