use super::*;

#[test]
fn test_l3_metadata() {
    let level = L3Architecture::new();
    assert_eq!(level.name(), "L3 Architecture");
    assert_eq!(level.difficulty(), Difficulty::Hard);
}

#[test]
fn test_l3_scenarios_count() {
    let level = L3Architecture::new();
    assert_eq!(level.scenarios().len(), 3);
}

#[test]
fn test_l3_evaluate_all_keywords() {
    let level = L3Architecture::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec![
            "daemon".into(),
            "sandbox".into(),
            "fitness".into(),
        ]),
    };
    let response = "The daemon orchestrates sandbox evaluation using fitness metrics";
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    assert_eq!(score.rating, Rating::Bloom);
}
