use super::*;

#[test]
fn test_l2_metadata() {
    let level = L2Diagnostics::new();
    assert_eq!(level.name(), "L2 Diagnostics");
    assert_eq!(level.difficulty(), Difficulty::Medium);
}

#[test]
fn test_l2_scenarios_count() {
    let level = L2Diagnostics::new();
    assert_eq!(level.scenarios().len(), 3);
}

#[test]
fn test_l2_evaluate_lifetime() {
    let level = L2Diagnostics::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::JsonFields(serde_json::json!({
            "error_type": "lifetime"
        })),
    };
    let response =
        r#"{"error_code": "E0106", "error_type": "lifetime", "file": "src/main.rs", "line": 42}"#;
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_l2_evaluate_keywords() {
    let level = L2Diagnostics::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec![
            "type".into(),
            "mismatch".into(),
            "expected".into(),
        ]),
    };
    let response = "The error shows a type mismatch: expected u32 but found String";
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
}
