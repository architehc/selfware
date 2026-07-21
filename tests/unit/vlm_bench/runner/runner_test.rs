use super::*;

#[test]
fn test_pass_thresholds() {
    assert!((pass_threshold_for(super::super::Difficulty::Easy) - 0.80).abs() < f64::EPSILON);
    assert!((pass_threshold_for(super::super::Difficulty::Medium) - 0.70).abs() < f64::EPSILON);
    assert!((pass_threshold_for(super::super::Difficulty::Hard) - 0.60).abs() < f64::EPSILON);
    assert!((pass_threshold_for(super::super::Difficulty::VeryHard) - 0.50).abs() < f64::EPSILON);
    assert!((pass_threshold_for(super::super::Difficulty::Extreme) - 0.40).abs() < f64::EPSILON);
    assert!((pass_threshold_for(super::super::Difficulty::Mega) - 0.50).abs() < f64::EPSILON);
}

#[test]
fn test_runner_creation_validates_config() {
    let bad_config = VlmBenchConfig {
        endpoint: String::new(),
        ..VlmBenchConfig::default()
    };
    assert!(VlmBenchRunner::new(bad_config, vec![]).is_err());
}

#[test]
fn test_runner_creation_ok() {
    let config = VlmBenchConfig::default();
    let runner = VlmBenchRunner::new(config, vec![]);
    assert!(runner.is_ok());
}
