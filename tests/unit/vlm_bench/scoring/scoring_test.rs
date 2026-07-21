use super::*;

#[test]
fn test_rating_from_accuracy() {
    // Pass threshold 0.8
    assert_eq!(Rating::from_accuracy(0.85, 0.8), Rating::Bloom);
    assert_eq!(Rating::from_accuracy(0.80, 0.8), Rating::Bloom);
    assert_eq!(Rating::from_accuracy(0.65, 0.8), Rating::Grow); // >= 0.6
    assert_eq!(Rating::from_accuracy(0.45, 0.8), Rating::Wilt); // >= 0.4
    assert_eq!(Rating::from_accuracy(0.30, 0.8), Rating::Frost); // < 0.4
}

#[test]
fn test_rating_display() {
    assert_eq!(format!("{}", Rating::Bloom), "BLOOM");
    assert_eq!(format!("{}", Rating::Frost), "FROST");
}

#[test]
fn test_keyword_accuracy_all_match() {
    let response = "The dashboard panel shows a loading spinner with error status";
    let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
    let acc = keyword_accuracy(response, &expected);
    assert!((acc - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_keyword_accuracy_partial() {
    let response = "The dashboard shows data";
    let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
    let acc = keyword_accuracy(response, &expected);
    assert!((acc - 1.0 / 3.0).abs() < 0.01);
}

#[test]
fn test_keyword_accuracy_case_insensitive() {
    let response = "DASHBOARD Panel SPINNER";
    let expected = vec!["dashboard".into(), "panel".into(), "spinner".into()];
    let acc = keyword_accuracy(response, &expected);
    assert!((acc - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_keyword_accuracy_empty_expected() {
    assert!((keyword_accuracy("anything", &[]) - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_json_field_accuracy_full_match() {
    let response = r#"{"panel": "dashboard", "status": "ok"}"#;
    let expected = serde_json::json!({"panel": "dashboard", "status": "ok"});
    let (acc, details) = json_field_accuracy(response, &expected);
    assert!((acc - 1.0).abs() < f64::EPSILON);
    assert_eq!(details.len(), 2);
}

#[test]
fn test_json_field_accuracy_partial() {
    let response = r#"{"panel": "dashboard", "status": "error"}"#;
    let expected = serde_json::json!({"panel": "dashboard", "status": "ok"});
    let (acc, _details) = json_field_accuracy(response, &expected);
    assert!((acc - 0.5).abs() < f64::EPSILON);
}

#[test]
fn test_json_field_accuracy_with_surrounding_text() {
    let response = r#"Based on my analysis: {"panel": "dashboard"} That's what I see."#;
    let expected = serde_json::json!({"panel": "dashboard"});
    let (acc, _) = json_field_accuracy(response, &expected);
    assert!((acc - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_json_field_accuracy_no_json() {
    let response = "This is just plain text with no JSON";
    let expected = serde_json::json!({"panel": "dashboard"});
    let (acc, _) = json_field_accuracy(response, &expected);
    assert!((acc - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_keyword_overlap() {
    let response = "the quick brown fox jumps over the lazy dog";
    let reference = "the quick brown fox";
    let score = keyword_overlap_score(response, reference);
    assert!((score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_keyword_overlap_no_match() {
    let response = "completely different words here";
    let reference = "the quick brown fox";
    let score = keyword_overlap_score(response, reference);
    assert!(score < 0.5);
}

#[test]
fn test_pearson_correlation_perfect() {
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let r = pearson_correlation(&a, &b);
    assert!((r - 1.0).abs() < 0.001);
}

#[test]
fn test_pearson_correlation_inverse() {
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let b = vec![5.0, 4.0, 3.0, 2.0, 1.0];
    let r = pearson_correlation(&a, &b);
    assert!((r - (-1.0)).abs() < 0.001);
}

#[test]
fn test_pearson_correlation_uncorrelated() {
    let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
    let b = vec![3.0, 1.0, 3.0, 1.0, 3.0];
    let r = pearson_correlation(&a, &b);
    assert!(r.abs() < 0.5, "Expected low correlation, got {}", r);
}

#[test]
fn test_pearson_correlation_short() {
    assert!((pearson_correlation(&[1.0], &[2.0])).abs() < f64::EPSILON);
}

#[test]
fn test_pearson_correlation_mismatched_len() {
    assert!((pearson_correlation(&[1.0, 2.0], &[3.0])).abs() < f64::EPSILON);
}

#[test]
fn test_extract_json_from_response() {
    let json = extract_json_from_response(r#"Sure! {"a": 1}"#);
    assert!(json.is_some());
    assert_eq!(json.unwrap()["a"], 1);
}

#[test]
fn test_extract_json_no_json() {
    assert!(extract_json_from_response("no json here").is_none());
}

#[test]
fn test_tokenize() {
    let tokens = tokenize("Hello, World! This is a test.");
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
    assert!(tokens.contains(&"test".to_string()));
    // Single-char tokens filtered out
    assert!(!tokens.contains(&"a".to_string()));
}

#[test]
fn test_level_score_serde() {
    let score = LevelScore {
        accuracy: 0.85,
        detail_scores: vec![("keywords".into(), 0.9), ("structure".into(), 0.8)],
        response_tokens: 1234,
        latency_ms: 3200,
        rating: Rating::Bloom,
    };
    let json = serde_json::to_string(&score).unwrap();
    let parsed: LevelScore = serde_json::from_str(&json).unwrap();
    assert!((parsed.accuracy - 0.85).abs() < f64::EPSILON);
    assert_eq!(parsed.rating, Rating::Bloom);
}
