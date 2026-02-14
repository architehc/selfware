//! Feature Factory Scenario
//!
//! Demonstrates agents building a new feature together:
//! - Architect designs the feature
//! - Coder implements it
//! - Tester writes and runs tests
//! - DevOps prepares deployment

use super::DemoScenario;
use crate::demo::runner::DemoRunner;
use crate::tui::animation::agent_avatar::{ActivityLevel, AgentRole};
use crate::tui::animation::message_flow::MessageType;

/// Feature factory demo - agents build a feature collaboratively
pub struct FeatureFactoryScenario {
    /// Current phase
    phase: FactoryPhase,
    /// Lines of code "written"
    lines_written: usize,
    /// Tests passed
    tests_passed: usize,
    /// Tests total
    tests_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FactoryPhase {
    Planning,
    Design,
    Implementation,
    Testing,
    Review,
    Deployment,
    Complete,
}

impl FeatureFactoryScenario {
    pub fn new() -> Self {
        Self {
            phase: FactoryPhase::Planning,
            lines_written: 0,
            tests_passed: 0,
            tests_total: 0,
        }
    }
}

impl Default for FeatureFactoryScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoScenario for FeatureFactoryScenario {
    fn name(&self) -> &str {
        "Feature Factory"
    }

    fn description(&self) -> &str {
        "Watch a multi-agent team design, implement, test, and deploy a new feature in real-time."
    }

    fn total_stages(&self) -> usize {
        7
    }

    fn initialize(&mut self, runner: &mut DemoRunner) {
        // Add the feature team
        runner.add_agent("architect", AgentRole::Architect);
        runner.add_agent("coder", AgentRole::Coder);
        runner.add_agent("tester", AgentRole::Tester);
        runner.add_agent("devops", AgentRole::DevOps);

        runner.set_token_rate(500.0);
        runner.set_total_tokens(0);

        self.phase = FactoryPhase::Planning;
    }

    fn execute_stage(&mut self, stage: usize, runner: &mut DemoRunner) -> bool {
        match stage {
            0 => {
                // Planning phase
                self.phase = FactoryPhase::Planning;
                runner.set_agent_activity("architect", ActivityLevel::High);
                runner.add_agent_tokens("architect", 3000);

                runner.sparkle(15.0, 8.0, 8);
                true
            }
            1 => {
                // Design phase
                self.phase = FactoryPhase::Design;
                runner.set_agent_activity("architect", ActivityLevel::Max);
                runner.set_token_rate(3000.0);
                runner.add_agent_tokens("architect", 10000);

                // Architect sends design to coder
                runner.send_message(
                    "architect",
                    "coder",
                    MessageType::Request,
                    (10.0, 5.0),
                    (25.0, 5.0),
                );

                runner.sparkle(20.0, 5.0, 12);
                true
            }
            2 => {
                // Implementation phase
                self.phase = FactoryPhase::Implementation;
                self.lines_written = 450;

                runner.set_agent_activity("architect", ActivityLevel::Low);
                runner.set_agent_activity("coder", ActivityLevel::Max);
                runner.set_token_rate(8000.0);
                runner.add_agent_tokens("coder", 35000);

                // Coder acknowledges
                runner.send_message(
                    "coder",
                    "architect",
                    MessageType::Response,
                    (25.0, 5.0),
                    (10.0, 5.0),
                );

                runner.explode(25.0, 8.0, 8);
                true
            }
            3 => {
                // Testing phase
                self.phase = FactoryPhase::Testing;
                self.tests_total = 42;
                self.tests_passed = 0;

                runner.set_agent_activity("coder", ActivityLevel::Medium);
                runner.set_agent_activity("tester", ActivityLevel::Max);
                runner.set_token_rate(5000.0);
                runner.add_agent_tokens("tester", 15000);

                // Coder sends code to tester
                runner.send_message(
                    "coder",
                    "tester",
                    MessageType::Request,
                    (25.0, 5.0),
                    (40.0, 5.0),
                );
                true
            }
            4 => {
                // Review phase - tests passing
                self.phase = FactoryPhase::Review;
                self.tests_passed = 42;

                runner.set_agent_activity("tester", ActivityLevel::Complete);
                runner.set_token_rate(2000.0);
                runner.add_agent_tokens("tester", 5000);

                // Tester reports success
                runner.send_message(
                    "tester",
                    "coder",
                    MessageType::Response,
                    (40.0, 5.0),
                    (25.0, 5.0),
                );
                runner.send_message(
                    "tester",
                    "architect",
                    MessageType::Consensus,
                    (40.0, 5.0),
                    (10.0, 5.0),
                );

                runner.sparkle(40.0, 5.0, 15);
                true
            }
            5 => {
                // Deployment phase
                self.phase = FactoryPhase::Deployment;

                runner.set_agent_activity("coder", ActivityLevel::Low);
                runner.set_agent_activity("devops", ActivityLevel::Max);
                runner.set_token_rate(4000.0);
                runner.add_agent_tokens("devops", 12000);

                // DevOps gets the green light
                runner.send_message(
                    "architect",
                    "devops",
                    MessageType::Broadcast,
                    (10.0, 5.0),
                    (55.0, 5.0),
                );

                runner.explode(55.0, 8.0, 10);
                true
            }
            6 => {
                // Complete!
                self.phase = FactoryPhase::Complete;

                runner.set_agent_activity("architect", ActivityLevel::Complete);
                runner.set_agent_activity("coder", ActivityLevel::Complete);
                runner.set_agent_activity("tester", ActivityLevel::Complete);
                runner.set_agent_activity("devops", ActivityLevel::Complete);
                runner.set_token_rate(0.0);

                // Celebration effects
                runner.celebrate(30.0, 10.0);
                runner.sparkle(10.0, 8.0, 20);
                runner.sparkle(25.0, 8.0, 20);
                runner.sparkle(40.0, 8.0, 20);
                runner.sparkle(55.0, 8.0, 20);
                true
            }
            _ => false,
        }
    }

    fn cleanup(&mut self, runner: &mut DemoRunner) {
        runner.set_total_tokens(80000);
        runner.set_token_rate(0.0);
    }
}

#[cfg(test)]
mod tests {
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
}
