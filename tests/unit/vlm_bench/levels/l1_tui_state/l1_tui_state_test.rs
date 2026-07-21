use super::*;

#[test]
fn test_l1_metadata() {
    let level = L1TuiState::new();
    assert_eq!(level.name(), "L1 TUI State");
    assert_eq!(level.difficulty(), Difficulty::Easy);
    assert!(!level.description().is_empty());
}

#[test]
fn test_l1_scenarios_count() {
    let level = L1TuiState::new();
    assert_eq!(level.scenarios().len(), 4);
}

#[test]
fn test_l1_scenarios_unique_ids() {
    let level = L1TuiState::new();
    let scenarios = level.scenarios();
    let mut ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), scenarios.len());
}

#[test]
fn test_l1_evaluate_keywords_perfect() {
    let level = L1TuiState::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec!["error".into(), "true".into()]),
    };
    let score = level.evaluate(&scenario, "There is an error, has_error: true");
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    assert_eq!(score.rating, Rating::Bloom);
}

#[test]
fn test_l1_evaluate_keywords_none() {
    let level = L1TuiState::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec!["error".into(), "true".into()]),
    };
    let score = level.evaluate(&scenario, "Everything is fine");
    assert!(score.accuracy < 0.5);
    assert_eq!(score.rating, Rating::Frost);
}

#[test]
fn test_l1_evaluate_json_fields() {
    let level = L1TuiState::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::JsonFields(serde_json::json!({
            "active_panel": "dashboard",
            "theme": "dark"
        })),
    };
    let response = r#"{"active_panel": "dashboard", "theme": "dark", "widget_count": 5}"#;
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    assert_eq!(score.rating, Rating::Bloom);
}
