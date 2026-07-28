use super::*;
use crate::ui::demo::DemoConfig;

#[test]
fn test_archaeology_scenario_new() {
    let scenario = CodebaseArchaeologyScenario::new();
    assert_eq!(scenario.name(), "Codebase Archaeology");
    assert_eq!(scenario.total_stages(), 6);
}

#[test]
fn test_archaeology_scenario_execution() {
    let mut scenario = CodebaseArchaeologyScenario::new();
    let mut runner = DemoRunner::new(DemoConfig::default());

    scenario.initialize(&mut runner);
    assert_eq!(runner.agents().len(), 3);

    // Execute all stages
    for stage in 0..scenario.total_stages() {
        assert!(scenario.execute_stage(stage, &mut runner));
    }

    // Check completion
    assert_eq!(scenario.phase, ArchaeologyPhase::Report);
}
