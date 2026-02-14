//! Codebase Archaeology Scenario
//!
//! Demonstrates agents exploring and understanding a legacy codebase:
//! - Architect analyzes structure
//! - Documenter creates documentation
//! - Reviewer identifies patterns

use super::DemoScenario;
use crate::demo::runner::DemoRunner;
use crate::tui::animation::agent_avatar::{ActivityLevel, AgentRole};
use crate::tui::animation::message_flow::MessageType;

/// Codebase archaeology demo - agents explore legacy code
pub struct CodebaseArchaeologyScenario {
    /// Current phase of exploration
    phase: ArchaeologyPhase,
    /// Files "discovered"
    discovered_files: usize,
    /// Patterns identified
    patterns_found: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArchaeologyPhase {
    Setup,
    InitialScan,
    DeepAnalysis,
    PatternRecognition,
    Documentation,
    Report,
}

impl CodebaseArchaeologyScenario {
    pub fn new() -> Self {
        Self {
            phase: ArchaeologyPhase::Setup,
            discovered_files: 0,
            patterns_found: 0,
        }
    }
}

impl Default for CodebaseArchaeologyScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoScenario for CodebaseArchaeologyScenario {
    fn name(&self) -> &str {
        "Codebase Archaeology"
    }

    fn description(&self) -> &str {
        "Watch agents explore and document a legacy codebase, discovering patterns and creating comprehensive documentation."
    }

    fn total_stages(&self) -> usize {
        6
    }

    fn initialize(&mut self, runner: &mut DemoRunner) {
        // Add the archaeology team
        runner.add_agent("architect", AgentRole::Architect);
        runner.add_agent("documenter", AgentRole::Documenter);
        runner.add_agent("reviewer", AgentRole::Reviewer);

        // Initial token stream setup
        runner.set_token_rate(1000.0);
        runner.set_total_tokens(0);

        self.phase = ArchaeologyPhase::Setup;
    }

    fn execute_stage(&mut self, stage: usize, runner: &mut DemoRunner) -> bool {
        match stage {
            0 => {
                // Setup: Architect starts
                self.phase = ArchaeologyPhase::InitialScan;
                runner.set_agent_activity("architect", ActivityLevel::High);
                runner.set_agent_activity("documenter", ActivityLevel::Idle);
                runner.set_agent_activity("reviewer", ActivityLevel::Idle);
                runner.set_token_rate(2500.0);
                runner.add_agent_tokens("architect", 5000);

                // Sparkle effect as scanning begins
                runner.sparkle(20.0, 10.0, 15);
                true
            }
            1 => {
                // Initial scan complete, deep analysis begins
                self.phase = ArchaeologyPhase::DeepAnalysis;
                self.discovered_files = 127;

                runner.set_agent_activity("architect", ActivityLevel::Max);
                runner.set_token_rate(5000.0);
                runner.add_agent_tokens("architect", 15000);

                // Message from architect
                runner.send_message(
                    "architect",
                    "documenter",
                    MessageType::Request,
                    (10.0, 5.0),
                    (30.0, 5.0),
                );
                true
            }
            2 => {
                // Pattern recognition phase
                self.phase = ArchaeologyPhase::PatternRecognition;
                self.patterns_found = 12;

                runner.set_agent_activity("architect", ActivityLevel::Medium);
                runner.set_agent_activity("reviewer", ActivityLevel::High);
                runner.set_token_rate(3500.0);
                runner.add_agent_tokens("reviewer", 8000);

                // Reviewer joins the analysis
                runner.send_message(
                    "architect",
                    "reviewer",
                    MessageType::Broadcast,
                    (10.0, 5.0),
                    (50.0, 5.0),
                );

                runner.sparkle(50.0, 8.0, 10);
                true
            }
            3 => {
                // Documentation phase
                self.phase = ArchaeologyPhase::Documentation;

                runner.set_agent_activity("architect", ActivityLevel::Low);
                runner.set_agent_activity("documenter", ActivityLevel::Max);
                runner.set_agent_activity("reviewer", ActivityLevel::Medium);
                runner.set_token_rate(4000.0);
                runner.add_agent_tokens("documenter", 20000);

                // Documenter responds to architect
                runner.send_message(
                    "documenter",
                    "architect",
                    MessageType::Response,
                    (30.0, 5.0),
                    (10.0, 5.0),
                );
                true
            }
            4 => {
                // Consensus building
                runner.send_message(
                    "reviewer",
                    "architect",
                    MessageType::Consensus,
                    (50.0, 5.0),
                    (10.0, 5.0),
                );
                runner.send_message(
                    "reviewer",
                    "documenter",
                    MessageType::Consensus,
                    (50.0, 5.0),
                    (30.0, 5.0),
                );

                runner.add_agent_tokens("reviewer", 5000);
                true
            }
            5 => {
                // Final report
                self.phase = ArchaeologyPhase::Report;

                runner.set_agent_activity("architect", ActivityLevel::Complete);
                runner.set_agent_activity("documenter", ActivityLevel::Complete);
                runner.set_agent_activity("reviewer", ActivityLevel::Complete);
                runner.set_token_rate(0.0);

                // Celebration!
                runner.celebrate(30.0, 10.0);
                runner.sparkle(10.0, 10.0, 20);
                runner.sparkle(50.0, 10.0, 20);
                true
            }
            _ => false,
        }
    }

    fn cleanup(&mut self, runner: &mut DemoRunner) {
        // Final token count
        runner.set_total_tokens(53000);
        runner.set_token_rate(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoConfig;

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
}
