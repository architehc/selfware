use super::*;
use crate::demo::DemoConfig;

#[test]
fn test_factory_scenario_new() {
    let scenario = FeatureFactoryScenario::new();
    assert_eq!(scenario.name(), "Feature Factory");
    assert_eq!(scenario.total_stages(), 7);
}

#[test]
fn test_factory_scenario_full_execution() {
    let mut scenario = FeatureFactoryScenario::new();
    let mut runner = DemoRunner::new(DemoConfig::default());

    scenario.initialize(&mut runner);
    assert_eq!(runner.agents().len(), 4);

    for stage in 0..scenario.total_stages() {
        assert!(scenario.execute_stage(stage, &mut runner));
    }

    assert_eq!(scenario.phase, FactoryPhase::Complete);
    assert_eq!(scenario.tests_passed, 42);
}
