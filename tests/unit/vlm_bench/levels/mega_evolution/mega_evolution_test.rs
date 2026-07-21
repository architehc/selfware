use super::*;

#[test]
fn test_mega_metadata() {
    let level = MegaEvolution::new();
    assert_eq!(level.name(), "Mega Evolution");
    assert_eq!(level.difficulty(), Difficulty::Mega);
}

#[test]
fn test_mega_scenarios_count() {
    let level = MegaEvolution::new();
    assert_eq!(level.scenarios().len(), 3);
}

#[test]
fn test_extract_dimension_scores_valid() {
    let response = r#"{"composition": 80, "hierarchy": 70, "readability": 90, "consistency": 75, "accessibility": 85}"#;
    let dims = [
        "composition",
        "hierarchy",
        "readability",
        "consistency",
        "accessibility",
    ];
    let scores = extract_dimension_scores(response, &dims);
    assert_eq!(scores.len(), 5);
    assert!((scores[0] - 80.0).abs() < f64::EPSILON);
    assert!((scores[2] - 90.0).abs() < f64::EPSILON);
}

#[test]
fn test_extract_dimension_scores_with_text() {
    let response = r#"Here are my scores: {"composition": 60, "hierarchy": 55, "readability": 70, "consistency": 65, "accessibility": 50} That's my analysis."#;
    let dims = [
        "composition",
        "hierarchy",
        "readability",
        "consistency",
        "accessibility",
    ];
    let scores = extract_dimension_scores(response, &dims);
    assert_eq!(scores.len(), 5);
}

#[test]
fn test_extract_dimension_scores_no_json() {
    let response = "The composition is good and hierarchy is clear";
    let dims = ["composition", "hierarchy"];
    let scores = extract_dimension_scores(response, &dims);
    assert!(scores.is_empty());
}

#[test]
fn test_evaluate_visual_scores_perfect_correlation() {
    let response = r#"{"composition": 75, "hierarchy": 70, "readability": 80, "consistency": 72, "accessibility": 68}"#;
    let ground_truth = vec![75.0, 70.0, 80.0, 72.0, 68.0];
    let score = evaluate_visual_scores(response, &ground_truth);
    // Perfect match should have high correlation
    assert!(score.accuracy > 0.9);
    assert_eq!(score.rating, Rating::Bloom);
}

#[test]
fn test_evaluate_visual_scores_no_response() {
    let ground_truth = vec![75.0, 70.0, 80.0, 72.0, 68.0];
    let score = evaluate_visual_scores("No JSON here", &ground_truth);
    assert!((score.accuracy - 0.0).abs() < f64::EPSILON);
    assert_eq!(score.rating, Rating::Frost);
}

#[test]
fn test_mega_evaluate_keywords() {
    let level = MegaEvolution::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec![
            "composition".into(),
            "hierarchy".into(),
            "readability".into(),
            "improving".into(),
        ]),
    };
    let response = "The composition and hierarchy are strong, readability is improving";
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
}
