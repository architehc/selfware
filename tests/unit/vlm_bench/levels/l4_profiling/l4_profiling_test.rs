use super::*;

#[test]
fn test_l4_metadata() {
    let level = L4Profiling::new();
    assert_eq!(level.name(), "L4 Profiling");
    assert_eq!(level.difficulty(), Difficulty::VeryHard);
}

#[test]
fn test_l4_scenarios_count() {
    let level = L4Profiling::new();
    assert_eq!(level.scenarios().len(), 3);
}

#[test]
fn test_l4_evaluate() {
    let level = L4Profiling::new();
    let scenario = BenchScenario {
        id: "test".into(),
        description: "test".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "test".into(),
        expected: ExpectedAnswer::Keywords(vec!["function".into(), "hot".into()]),
    };
    let response = "The hottest function is parse_tokens taking 45% of CPU time";
    let score = level.evaluate(&scenario, response);
    assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
}
