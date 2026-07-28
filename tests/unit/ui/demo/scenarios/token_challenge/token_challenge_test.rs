use super::*;
use crate::ui::demo::DemoConfig;

#[test]
fn test_token_challenge_new() {
    let scenario = TokenChallengeScenario::new();
    assert_eq!(scenario.name(), "Token Challenge");
    assert_eq!(scenario.total_stages(), 7);
}

#[test]
fn test_token_challenge_full_execution() {
    let mut scenario = TokenChallengeScenario::new();
    let mut runner = DemoRunner::new(DemoConfig::default());

    scenario.initialize(&mut runner);
    assert_eq!(runner.agents().len(), 4);

    for stage in 0..scenario.total_stages() {
        assert!(scenario.execute_stage(stage, &mut runner));
    }

    assert_eq!(scenario.phase, ChallengePhase::Results);
    assert!(scenario.total_processed > 500000);
    assert!((scenario.peak_rate - 100000.0).abs() < 0.001);
}
