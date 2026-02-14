//! Bug Hunt Safari Scenario
//!
//! Demonstrates agents hunting down and fixing bugs:
//! - Tester reproduces the bug
//! - Coder traces the issue
//! - Security agent checks for vulnerabilities
//! - Reviewer validates the fix

use super::DemoScenario;
use crate::demo::runner::DemoRunner;
use crate::tui::animation::agent_avatar::{ActivityLevel, AgentRole};
use crate::tui::animation::message_flow::MessageType;

/// Bug hunt safari demo - agents track down and fix bugs
pub struct BugHuntSafariScenario {
    /// Current phase
    phase: HuntPhase,
    /// Bugs found
    bugs_found: usize,
    /// Bugs fixed
    bugs_fixed: usize,
    /// Security issues found
    security_issues: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HuntPhase {
    Preparation,
    Reproduction,
    Investigation,
    SecurityScan,
    Fix,
    Verification,
    Complete,
}

impl BugHuntSafariScenario {
    pub fn new() -> Self {
        Self {
            phase: HuntPhase::Preparation,
            bugs_found: 0,
            bugs_fixed: 0,
            security_issues: 0,
        }
    }
}

impl Default for BugHuntSafariScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoScenario for BugHuntSafariScenario {
    fn name(&self) -> &str {
        "Bug Hunt Safari"
    }

    fn description(&self) -> &str {
        "Join the hunt as agents track down elusive bugs, identify security vulnerabilities, and implement fixes."
    }

    fn total_stages(&self) -> usize {
        7
    }

    fn initialize(&mut self, runner: &mut DemoRunner) {
        // Add the bug hunting team
        runner.add_agent("tester", AgentRole::Tester);
        runner.add_agent("coder", AgentRole::Coder);
        runner.add_agent("security", AgentRole::Security);
        runner.add_agent("reviewer", AgentRole::Reviewer);

        runner.set_token_rate(1000.0);
        runner.set_total_tokens(0);

        self.phase = HuntPhase::Preparation;
    }

    fn execute_stage(&mut self, stage: usize, runner: &mut DemoRunner) -> bool {
        match stage {
            0 => {
                // Preparation - tester analyzes bug report
                self.phase = HuntPhase::Preparation;
                runner.set_agent_activity("tester", ActivityLevel::High);
                runner.add_agent_tokens("tester", 4000);

                runner.sparkle(15.0, 5.0, 10);
                true
            }
            1 => {
                // Reproduction - tester reproduces the bug
                self.phase = HuntPhase::Reproduction;
                self.bugs_found = 1;

                runner.set_agent_activity("tester", ActivityLevel::Max);
                runner.set_token_rate(3000.0);
                runner.add_agent_tokens("tester", 8000);

                // Bug found! Error message
                runner.send_message(
                    "tester",
                    "coder",
                    MessageType::Error,
                    (15.0, 5.0),
                    (30.0, 5.0),
                );

                runner.explode(15.0, 8.0, 12);
                true
            }
            2 => {
                // Investigation - coder traces the bug
                self.phase = HuntPhase::Investigation;

                runner.set_agent_activity("tester", ActivityLevel::Medium);
                runner.set_agent_activity("coder", ActivityLevel::Max);
                runner.set_token_rate(6000.0);
                runner.add_agent_tokens("coder", 20000);

                // Coder investigates
                runner.send_message(
                    "coder",
                    "tester",
                    MessageType::Response,
                    (30.0, 5.0),
                    (15.0, 5.0),
                );

                runner.sparkle(30.0, 8.0, 8);
                true
            }
            3 => {
                // Security scan - check for related vulnerabilities
                self.phase = HuntPhase::SecurityScan;
                self.security_issues = 2;

                runner.set_agent_activity("coder", ActivityLevel::High);
                runner.set_agent_activity("security", ActivityLevel::Max);
                runner.set_token_rate(4000.0);
                runner.add_agent_tokens("security", 12000);

                // Security agent joins
                runner.send_message(
                    "coder",
                    "security",
                    MessageType::Request,
                    (30.0, 5.0),
                    (45.0, 5.0),
                );

                // Security finds issues!
                runner.send_message(
                    "security",
                    "coder",
                    MessageType::Error,
                    (45.0, 5.0),
                    (30.0, 5.0),
                );

                runner.explode(45.0, 8.0, 8);
                true
            }
            4 => {
                // Fix - coder implements the fix
                self.phase = HuntPhase::Fix;
                self.bugs_fixed = 1;

                runner.set_agent_activity("security", ActivityLevel::Medium);
                runner.set_agent_activity("coder", ActivityLevel::Max);
                runner.set_token_rate(7000.0);
                runner.add_agent_tokens("coder", 25000);

                // Coder fixes the bug
                runner.sparkle(30.0, 5.0, 15);
                true
            }
            5 => {
                // Verification - reviewer and tester verify
                self.phase = HuntPhase::Verification;

                runner.set_agent_activity("coder", ActivityLevel::Low);
                runner.set_agent_activity("reviewer", ActivityLevel::High);
                runner.set_agent_activity("tester", ActivityLevel::High);
                runner.set_token_rate(3000.0);
                runner.add_agent_tokens("reviewer", 8000);
                runner.add_agent_tokens("tester", 5000);

                // Code review
                runner.send_message(
                    "coder",
                    "reviewer",
                    MessageType::Request,
                    (30.0, 5.0),
                    (60.0, 5.0),
                );

                // Reviewer approves
                runner.send_message(
                    "reviewer",
                    "coder",
                    MessageType::Consensus,
                    (60.0, 5.0),
                    (30.0, 5.0),
                );

                runner.sparkle(60.0, 5.0, 10);
                true
            }
            6 => {
                // Complete!
                self.phase = HuntPhase::Complete;

                runner.set_agent_activity("tester", ActivityLevel::Complete);
                runner.set_agent_activity("coder", ActivityLevel::Complete);
                runner.set_agent_activity("security", ActivityLevel::Complete);
                runner.set_agent_activity("reviewer", ActivityLevel::Complete);
                runner.set_token_rate(0.0);

                // Victory celebration
                runner.celebrate(35.0, 10.0);
                runner.sparkle(15.0, 8.0, 15);
                runner.sparkle(30.0, 8.0, 15);
                runner.sparkle(45.0, 8.0, 15);
                runner.sparkle(60.0, 8.0, 15);
                true
            }
            _ => false,
        }
    }

    fn cleanup(&mut self, runner: &mut DemoRunner) {
        runner.set_total_tokens(82000);
        runner.set_token_rate(0.0);
    }
}

#[cfg(test)]
mod tests {
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
}
