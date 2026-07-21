use super::*;

#[test]
fn test_keyword_evaluator_all_present() {
    let eval = KeywordEvaluator::new(vec!["hello".into(), "world".into()]);
    let result = eval.evaluate("Hello, World!");
    assert_eq!(result.score, 1.0);
    assert!(result.passed);
    assert_eq!(result.details.len(), 2);
}

#[test]
fn test_keyword_evaluator_partial() {
    let eval = KeywordEvaluator::new(vec!["hello".into(), "missing".into()]);
    let result = eval.evaluate("Hello!");
    assert!((result.score - 0.5).abs() < f64::EPSILON);
    assert!(result.passed); // 0.5 >= default threshold 0.5
}

#[test]
fn test_keyword_evaluator_none() {
    let eval = KeywordEvaluator::new(vec!["missing".into()]).with_threshold(0.5);
    let result = eval.evaluate("nothing here");
    assert_eq!(result.score, 0.0);
    assert!(!result.passed);
}

#[test]
fn test_json_evaluator_valid() {
    let eval = JsonEvaluator::new(vec!["name".into(), "value".into()]);
    let result = eval.evaluate(r#"{"name": "test", "value": 42}"#);
    assert_eq!(result.score, 1.0);
    assert!(result.passed);
}

#[test]
fn test_json_evaluator_markdown_fence() {
    let eval = JsonEvaluator::new(vec!["name".into()]);
    let result = eval.evaluate("Here is the result:\n```json\n{\"name\": \"test\"}\n```\n");
    assert!(result.passed);
}

#[test]
fn test_json_evaluator_invalid() {
    let eval = JsonEvaluator::new(vec![]);
    let result = eval.evaluate("not json at all");
    assert_eq!(result.score, 0.0);
    assert!(!result.passed);
}

#[test]
fn test_noop_evaluator() {
    let eval = NoopEvaluator;
    let result = eval.evaluate("anything");
    assert_eq!(result.score, 1.0);
    assert!(result.passed);
}

#[test]
fn test_extract_json_block() {
    assert_eq!(extract_json_block(r#"{"a":1}"#), r#"{"a":1}"#);
    assert_eq!(extract_json_block("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
    assert_eq!(
        extract_json_block("Some text\n```\n{\"a\":1}\n```\nmore"),
        r#"{"a":1}"#
    );
}
