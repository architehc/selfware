//! Token Challenge Scenario
//!
//! Demonstrates high-throughput token processing:
//! - Performance agent optimizes token usage
//! - Multiple agents compete for efficiency
//! - Shows token stream visualization at high rates

use super::DemoScenario;
use crate::demo::runner::DemoRunner;
use crate::tui::animation::agent_avatar::{ActivityLevel, AgentRole};
use crate::tui::animation::message_flow::MessageType;

/// Token challenge demo - high-throughput token processing
pub struct TokenChallengeScenario {
    /// Current phase
    phase: ChallengePhase,
    /// Token rate achieved
    peak_rate: f64,
    /// Total tokens processed
    total_processed: u64,
    /// Current round
    round: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChallengePhase {
    Warmup,
    Round1,
    Round2,
    Round3,
    Peak,
    Cooldown,
    Results,
}

impl TokenChallengeScenario {
    pub fn new() -> Self {
        Self {
            phase: ChallengePhase::Warmup,
            peak_rate: 0.0,
            total_processed: 0,
            round: 0,
        }
    }
}

impl Default for TokenChallengeScenario {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoScenario for TokenChallengeScenario {
    fn name(&self) -> &str {
        "Token Challenge"
    }

    fn description(&self) -> &str {
        "Watch agents compete in a high-throughput token processing challenge, pushing the limits of parallel execution."
    }

    fn total_stages(&self) -> usize {
        7
    }

    fn initialize(&mut self, runner: &mut DemoRunner) {
        // Add the performance team
        runner.add_agent("performance-1", AgentRole::Performance);
        runner.add_agent("performance-2", AgentRole::Performance);
        runner.add_agent("coder-1", AgentRole::Coder);
        runner.add_agent("coder-2", AgentRole::Coder);

        runner.set_token_rate(0.0);
        runner.set_total_tokens(0);

        self.phase = ChallengePhase::Warmup;
        self.total_processed = 0;
    }

    fn execute_stage(&mut self, stage: usize, runner: &mut DemoRunner) -> bool {
        match stage {
            0 => {
                // Warmup
                self.phase = ChallengePhase::Warmup;
                runner.set_agent_activity("performance-1", ActivityLevel::Low);
                runner.set_agent_activity("performance-2", ActivityLevel::Low);
                runner.set_agent_activity("coder-1", ActivityLevel::Low);
                runner.set_agent_activity("coder-2", ActivityLevel::Low);
                runner.set_token_rate(1000.0);

                runner.add_agent_tokens("performance-1", 1000);
                runner.add_agent_tokens("coder-1", 1000);

                runner.sparkle(20.0, 8.0, 5);
                true
            }
            1 => {
                // Round 1 - moderate speed
                self.phase = ChallengePhase::Round1;
                self.round = 1;

                runner.set_agent_activity("performance-1", ActivityLevel::Medium);
                runner.set_agent_activity("coder-1", ActivityLevel::Medium);
                runner.set_token_rate(5000.0);

                runner.add_agent_tokens("performance-1", 5000);
                runner.add_agent_tokens("coder-1", 5000);
                self.total_processed += 10000;

                // Competition messages
                runner.send_message(
                    "performance-1",
                    "coder-1",
                    MessageType::Request,
                    (15.0, 5.0),
                    (35.0, 5.0),
                );
                true
            }
            2 => {
                // Round 2 - speed up
                self.phase = ChallengePhase::Round2;
                self.round = 2;

                runner.set_agent_activity("performance-1", ActivityLevel::High);
                runner.set_agent_activity("performance-2", ActivityLevel::Medium);
                runner.set_agent_activity("coder-1", ActivityLevel::High);
                runner.set_agent_activity("coder-2", ActivityLevel::Medium);
                runner.set_token_rate(15000.0);

                runner.add_agent_tokens("performance-1", 15000);
                runner.add_agent_tokens("performance-2", 10000);
                runner.add_agent_tokens("coder-1", 15000);
                runner.add_agent_tokens("coder-2", 10000);
                self.total_processed += 50000;

                runner.explode(25.0, 8.0, 10);
                true
            }
            3 => {
                // Round 3 - full throttle
                self.phase = ChallengePhase::Round3;
                self.round = 3;

                runner.set_agent_activity("performance-1", ActivityLevel::Max);
                runner.set_agent_activity("performance-2", ActivityLevel::High);
                runner.set_agent_activity("coder-1", ActivityLevel::Max);
                runner.set_agent_activity("coder-2", ActivityLevel::High);
                runner.set_token_rate(50000.0);

                runner.add_agent_tokens("performance-1", 50000);
                runner.add_agent_tokens("performance-2", 35000);
                runner.add_agent_tokens("coder-1", 50000);
                runner.add_agent_tokens("coder-2", 35000);
                self.total_processed += 170000;

                // High-speed message bursts
                for i in 0..3 {
                    let offset = i as f32 * 5.0;
                    runner.send_message(
                        "performance-1",
                        "coder-1",
                        MessageType::Broadcast,
                        (15.0, 5.0 + offset),
                        (35.0, 5.0 + offset),
                    );
                    runner.send_message(
                        "performance-2",
                        "coder-2",
                        MessageType::Broadcast,
                        (55.0, 5.0 + offset),
                        (75.0, 5.0 + offset),
                    );
                }

                runner.explode(45.0, 10.0, 15);
                true
            }
            4 => {
                // Peak performance
                self.phase = ChallengePhase::Peak;
                self.peak_rate = 100000.0;

                runner.set_agent_activity("performance-1", ActivityLevel::Max);
                runner.set_agent_activity("performance-2", ActivityLevel::Max);
                runner.set_agent_activity("coder-1", ActivityLevel::Max);
                runner.set_agent_activity("coder-2", ActivityLevel::Max);
                runner.set_token_rate(100000.0);

                runner.add_agent_tokens("performance-1", 100000);
                runner.add_agent_tokens("performance-2", 80000);
                runner.add_agent_tokens("coder-1", 100000);
                runner.add_agent_tokens("coder-2", 80000);
                self.total_processed += 360000;

                // Peak celebration
                runner.celebrate(45.0, 10.0);
                runner.sparkle(15.0, 5.0, 20);
                runner.sparkle(35.0, 5.0, 20);
                runner.sparkle(55.0, 5.0, 20);
                runner.sparkle(75.0, 5.0, 20);
                true
            }
            5 => {
                // Cooldown
                self.phase = ChallengePhase::Cooldown;

                runner.set_agent_activity("performance-1", ActivityLevel::Medium);
                runner.set_agent_activity("performance-2", ActivityLevel::Medium);
                runner.set_agent_activity("coder-1", ActivityLevel::Medium);
                runner.set_agent_activity("coder-2", ActivityLevel::Medium);
                runner.set_token_rate(10000.0);

                runner.add_agent_tokens("performance-1", 10000);
                runner.add_agent_tokens("performance-2", 10000);
                runner.add_agent_tokens("coder-1", 10000);
                runner.add_agent_tokens("coder-2", 10000);
                self.total_processed += 40000;
                true
            }
            6 => {
                // Results
                self.phase = ChallengePhase::Results;

                runner.set_agent_activity("performance-1", ActivityLevel::Complete);
                runner.set_agent_activity("performance-2", ActivityLevel::Complete);
                runner.set_agent_activity("coder-1", ActivityLevel::Complete);
                runner.set_agent_activity("coder-2", ActivityLevel::Complete);
                runner.set_token_rate(0.0);

                // Final stats
                runner.set_total_tokens(self.total_processed);

                // Victory effects
                runner.celebrate(45.0, 10.0);
                for x in (10..80).step_by(15) {
                    runner.sparkle(x as f32, 8.0, 15);
                }
                true
            }
            _ => false,
        }
    }

    fn cleanup(&mut self, runner: &mut DemoRunner) {
        runner.set_total_tokens(self.total_processed);
        runner.set_token_rate(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo::DemoConfig;

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
}
