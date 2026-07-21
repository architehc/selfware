use super::*;

fn make_error(code: Option<&str>, message: &str, file: &str, line: u32) -> CompilerError {
    CompilerError {
        code: code.map(|s| s.to_string()),
        message: message.to_string(),
        file: file.to_string(),
        line,
        column: 1,
        snippet: String::new(),
        suggestion: None,
        severity: Severity::Error,
    }
}

#[test]
fn test_suggest_fix_e0425() {
    let error = make_error(Some("E0425"), "cannot find value `foo`", "src/main.rs", 10);
    let suggestion = ErrorAnalyzer::suggest_fix(&error);
    assert!(suggestion.is_some());
    assert!(suggestion.unwrap().contains("Cannot find value"));
}

#[test]
fn test_suggest_fix_e0382() {
    let error = make_error(Some("E0382"), "use of moved value", "src/main.rs", 10);
    let suggestion = ErrorAnalyzer::suggest_fix(&error);
    assert!(suggestion.is_some());
    assert!(suggestion.unwrap().contains("clone"));
}

#[test]
fn test_suggest_fix_unknown_code() {
    let error = make_error(Some("E9999"), "unknown error", "src/main.rs", 10);
    let suggestion = ErrorAnalyzer::suggest_fix(&error);
    assert!(suggestion.is_none());
}

#[test]
fn test_prioritize_errors() {
    let errors = vec![
        make_error(Some("clippy::unwrap_used"), "warning", "src/main.rs", 10),
        make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
        make_error(Some("E0425"), "cannot find value", "src/main.rs", 1),
    ];

    let sorted = ErrorAnalyzer::prioritize(&errors);
    // Type errors (E0308) should come before name resolution (E0425)
    assert_eq!(sorted[0].code, Some("E0308".to_string()));
}

#[test]
fn test_group_by_cause() {
    let errors = vec![
        make_error(Some("E0382"), "use of moved value", "src/main.rs", 10),
        make_error(Some("E0382"), "use of moved value", "src/main.rs", 12),
        make_error(Some("E0425"), "cannot find value", "src/other.rs", 5),
    ];

    let groups = ErrorAnalyzer::group_by_cause(&errors);
    // Should group the two E0382 errors together
    assert!(groups.len() <= 2);
}

#[test]
fn test_summarize_by_category() {
    let errors = vec![
        make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
        make_error(Some("E0308"), "type mismatch", "src/main.rs", 10),
        make_error(Some("E0382"), "use of moved value", "src/main.rs", 15),
    ];

    let summary = ErrorAnalyzer::summarize_by_category(&errors);
    assert_eq!(*summary.get("Type errors").unwrap_or(&0), 2);
}

#[test]
fn test_most_actionable() {
    let errors = vec![
        make_error(Some("clippy::unwrap_used"), "warning", "src/main.rs", 10),
        make_error(Some("E0308"), "type mismatch", "src/main.rs", 5),
    ];

    let actionable = ErrorAnalyzer::most_actionable(&errors);
    assert!(actionable.is_some());
    assert_eq!(actionable.unwrap().code, Some("E0308".to_string()));
}

#[test]
fn test_extract_identifier() {
    assert_eq!(extract_identifier("cannot find value `foo`"), Some("foo"));
    assert_eq!(
        extract_identifier("cannot find value `bar_baz`"),
        Some("bar_baz")
    );
    assert_eq!(extract_identifier("some message without identifier"), None);
}

#[test]
fn test_are_related_same_file_nearby() {
    let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
    let b = make_error(Some("E0425"), "error", "src/main.rs", 12);
    assert!(are_related(&a, &b));
}

#[test]
fn test_are_related_same_code() {
    let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
    let b = make_error(Some("E0308"), "error", "src/other.rs", 100);
    assert!(are_related(&a, &b));
}

#[test]
fn test_not_related() {
    let a = make_error(Some("E0308"), "error", "src/main.rs", 10);
    let b = make_error(Some("E0425"), "error", "src/other.rs", 100);
    assert!(!are_related(&a, &b));
}

#[test]
fn test_categorize_clippy() {
    let error = make_error(Some("clippy::unwrap_used"), "unwrap", "src/main.rs", 10);
    assert_eq!(categorize_error(&error), "Clippy lints");
}

#[test]
fn test_categorize_type_error() {
    let error = make_error(Some("E0308"), "type mismatch", "src/main.rs", 10);
    assert_eq!(categorize_error(&error), "Type errors");
}

#[test]
fn test_categorize_borrow_error() {
    let error = make_error(Some("E0382"), "use of moved value", "src/main.rs", 10);
    assert_eq!(categorize_error(&error), "Borrow checker errors");
}
