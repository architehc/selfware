use super::*;

struct TestScenario {
    name: String,
    stages: usize,
    initialized: bool,
    executed_stages: Vec<usize>,
    cleaned_up: bool,
}

impl TestScenario {
    fn new(stages: usize) -> Self {
        Self {
            name: "Test Scenario".to_string(),
            stages,
            initialized: false,
            executed_stages: Vec::new(),
            cleaned_up: false,
        }
    }
}

impl DemoScenario for TestScenario {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        "A test scenario"
    }

    fn total_stages(&self) -> usize {
        self.stages
    }

    fn initialize(&mut self, _runner: &mut DemoRunner) {
        self.initialized = true;
    }

    fn execute_stage(&mut self, stage: usize, _runner: &mut DemoRunner) -> bool {
        self.executed_stages.push(stage);
        true
    }

    fn cleanup(&mut self, _runner: &mut DemoRunner) {
        self.cleaned_up = true;
    }
}

#[test]
fn test_demo_runner_new() {
    let runner = DemoRunner::new(DemoConfig::default());
    assert_eq!(*runner.state(), DemoState::Idle);
    assert_eq!(runner.current_stage(), 0);
}

#[test]
fn test_demo_runner_start() {
    let mut runner = DemoRunner::new(DemoConfig::default());
    let mut scenario = TestScenario::new(5);

    runner.start(&mut scenario);

    assert!(scenario.initialized);
    assert_eq!(*runner.state(), DemoState::Running);
    assert_eq!(runner.total_stages(), 5);
}

#[test]
fn test_demo_runner_stages() {
    let mut runner = DemoRunner::new(DemoConfig::default());
    let mut scenario = TestScenario::new(3);

    runner.start(&mut scenario);

    assert!(runner.next_stage(&mut scenario));
    assert_eq!(runner.current_stage(), 1);

    assert!(runner.next_stage(&mut scenario));
    assert_eq!(runner.current_stage(), 2);

    assert!(runner.next_stage(&mut scenario));
    assert_eq!(runner.current_stage(), 3);

    // Should complete and return false
    assert!(!runner.next_stage(&mut scenario));
    assert_eq!(*runner.state(), DemoState::Completed);
    assert!(scenario.cleaned_up);
}

#[test]
fn test_demo_runner_pause_resume() {
    let mut runner = DemoRunner::new(DemoConfig::default());
    let mut scenario = TestScenario::new(3);

    runner.start(&mut scenario);
    assert_eq!(*runner.state(), DemoState::Running);

    runner.pause();
    assert_eq!(*runner.state(), DemoState::Paused);

    runner.resume();
    assert_eq!(*runner.state(), DemoState::Running);
}

#[test]
fn test_demo_runner_agents() {
    let mut runner = DemoRunner::new(DemoConfig::default());

    runner.add_agent("coder-1", AgentRole::Coder);
    runner.add_agent("tester-1", AgentRole::Tester);

    assert_eq!(runner.agents().len(), 2);
    assert!(runner.agent("coder-1").is_some());
    assert!(runner.agent("unknown").is_none());
}

#[test]
fn test_demo_runner_effects() {
    let mut runner = DemoRunner::new(DemoConfig::default());

    runner.sparkle(10.0, 10.0, 5);
    runner.explode(20.0, 20.0, 10);
    runner.celebrate(30.0, 30.0);

    assert_eq!(runner.events().len(), 3);
}
