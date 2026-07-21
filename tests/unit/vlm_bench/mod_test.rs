use super::*;

#[test]
fn test_difficulty_ordering() {
    assert!(Difficulty::Easy < Difficulty::Medium);
    assert!(Difficulty::Medium < Difficulty::Hard);
    assert!(Difficulty::Hard < Difficulty::VeryHard);
    assert!(Difficulty::VeryHard < Difficulty::Extreme);
    assert!(Difficulty::Extreme < Difficulty::Mega);
}

#[test]
fn test_difficulty_display() {
    assert_eq!(format!("{}", Difficulty::Easy), "Easy");
    assert_eq!(format!("{}", Difficulty::VeryHard), "Very Hard");
    assert_eq!(format!("{}", Difficulty::Mega), "Mega");
}

#[test]
fn test_bench_scenario_serde_roundtrip() {
    let scenario = BenchScenario {
        id: "test_01".into(),
        description: "Test scenario".into(),
        image_path: PathBuf::from("test.png"),
        prompt: "What do you see?".into(),
        expected: ExpectedAnswer::Keywords(vec!["dashboard".into(), "panel".into()]),
    };
    let json = serde_json::to_string(&scenario).unwrap();
    let parsed: BenchScenario = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "test_01");
    assert_eq!(parsed.prompt, "What do you see?");
}

#[test]
fn test_expected_answer_variants() {
    let kw = ExpectedAnswer::Keywords(vec!["a".into()]);
    let json_str = serde_json::to_string(&kw).unwrap();
    assert!(json_str.contains("Keywords"));

    let jf = ExpectedAnswer::JsonFields(serde_json::json!({"panel": "dashboard"}));
    let json_str = serde_json::to_string(&jf).unwrap();
    assert!(json_str.contains("JsonFields"));

    let vs = ExpectedAnswer::VisualScores(vec![80.0, 70.0, 90.0]);
    let json_str = serde_json::to_string(&vs).unwrap();
    assert!(json_str.contains("VisualScores"));
}
