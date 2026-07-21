use super::*;
use crate::demo::DemoConfig;

#[test]
fn test_bug_hunt_scenario_new() {
    let scenario = BugHuntSafariScenario::new();
    assert_eq!(scenario.name(), "Bug Hunt Safari");
    assert_eq!(scenario.total_stages(), 7);
}

#[test]
fn test_bug_hunt_full_execution() {
    let mut scenario = BugHuntSafariScenario::new();
    let mut runner = DemoRunner::new(DemoConfig::default());

    scenario.initialize(&mut runner);
    assert_eq!(runner.agents().len(), 4);

    for stage in 0..scenario.total_stages() {
        assert!(scenario.execute_stage(stage, &mut runner));
    }

    assert_eq!(scenario.phase, HuntPhase::Complete);
    assert_eq!(scenario.bugs_found, 1);
    assert_eq!(scenario.bugs_fixed, 1);
    assert_eq!(scenario.security_issues, 2);
}
