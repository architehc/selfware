use super::*;

#[test]
fn test_l5_metadata() {
    let level = L5Layout::new();
    assert_eq!(level.name(), "L5 Layout");
    assert_eq!(level.difficulty(), Difficulty::Extreme);
}

#[test]
fn test_l5_scenarios_count() {
    let level = L5Layout::new();
    assert_eq!(level.scenarios().len(), 3);
}

#[test]
fn test_l5_evaluate_partial() {
    let level = L5Layout::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec![
            "layout".into(),
            "horizontal".into(),
            "constraint".into(),
            "percentage".into(),
        ]),
    };
    // Only 2/4 keywords
    let response = "The layout uses a horizontal split";
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 0.5).abs() < f64::EPSILON);
}
