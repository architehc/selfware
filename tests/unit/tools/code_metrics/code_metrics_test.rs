use super::*;

#[test]
fn test_complexity_level_from_score() {
    assert_eq!(ComplexityLevel::from_score(5), ComplexityLevel::Low);
    assert_eq!(ComplexityLevel::from_score(10), ComplexityLevel::Low);
    assert_eq!(ComplexityLevel::from_score(15), ComplexityLevel::Medium);
    assert_eq!(ComplexityLevel::from_score(30), ComplexityLevel::High);
    assert_eq!(ComplexityLevel::from_score(50), ComplexityLevel::High);
    assert_eq!(ComplexityLevel::from_score(100), ComplexityLevel::Critical);
}

#[test]
fn test_complexity_level_suggestions() {
    assert!(ComplexityLevel::Low.suggestion().contains("No action"));
    assert!(ComplexityLevel::Medium.suggestion().contains("breaking"));
    assert!(ComplexityLevel::High.suggestion().contains("refactored"));
    assert!(ComplexityLevel::Critical.suggestion().contains("Critical"));
}

#[test]
fn test_calculate_complexity_simple() {
    let body = "let x = 5;\nlet y = 10;\nx + y"; // No return, no branches
    let complexity = CodeMetricsTool::calculate_complexity(body);
    assert_eq!(complexity, 1); // Base complexity only
}

#[test]
fn test_calculate_complexity_with_if() {
    let body = "if x > 5 {\n    return 1;\n} else {\n    return 0;\n}";
    let complexity = CodeMetricsTool::calculate_complexity(body);
    assert!(complexity >= 3); // Base + if + else
}

#[test]
fn test_calculate_complexity_with_loops() {
    let body = "for i in 0..10 {\n    while x < 5 {\n        x += 1;\n    }\n}";
    let complexity = CodeMetricsTool::calculate_complexity(body);
    assert!(complexity >= 3); // Base + for + while
}

#[test]
fn test_calculate_complexity_with_match() {
    let body = "match x {\n    1 => true,\n    2 => false,\n    _ => true,\n}";
    let complexity = CodeMetricsTool::calculate_complexity(body);
    assert!(complexity >= 2); // Base + match
}

#[test]
fn test_tool_metadata() {
    let tool = CodeMetricsTool::new();
    assert_eq!(tool.name(), "code_metrics");
    assert!(!tool.description().is_empty());

    let schema = tool.schema();
    assert!(schema.get("required").is_some());
    assert!(schema.get("properties").unwrap().get("file_path").is_some());
}
