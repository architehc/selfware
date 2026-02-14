//! Demo Framework for Selfware TUI
//!
//! Provides demonstration scenarios showcasing:
//! - Animated agent interactions
//! - Token stream visualizations
//! - Multi-agent coordination
//! - Progress tracking and effects

#![allow(dead_code)]

mod runner;
mod scenarios;

pub use runner::{DemoRunner, DemoState};
pub use scenarios::{
    archaeology::CodebaseArchaeologyScenario,
    bug_hunt::BugHuntSafariScenario,
    factory::FeatureFactoryScenario,
    token_challenge::TokenChallengeScenario,
    DemoScenario,
};

use std::time::Duration;

/// Configuration for demo execution
#[derive(Debug, Clone)]
pub struct DemoConfig {
    /// Animation speed multiplier (1.0 = normal)
    pub speed_multiplier: f32,
    /// Whether to auto-advance through stages
    pub auto_advance: bool,
    /// Delay between auto-advance steps
    pub step_delay: Duration,
    /// Enable particle effects
    pub particles_enabled: bool,
    /// Maximum particles in effects
    pub max_particles: usize,
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self {
            speed_multiplier: 1.0,
            auto_advance: false,
            step_delay: Duration::from_millis(500),
            particles_enabled: true,
            max_particles: 100,
        }
    }
}

impl DemoConfig {
    /// Create fast demo config for testing
    pub fn fast() -> Self {
        Self {
            speed_multiplier: 2.0,
            auto_advance: true,
            step_delay: Duration::from_millis(200),
            ..Default::default()
        }
    }

    /// Create slow demo config for presentations
    pub fn presentation() -> Self {
        Self {
            speed_multiplier: 0.75,
            auto_advance: false,
            step_delay: Duration::from_secs(1),
            max_particles: 200,
            ..Default::default()
        }
    }
}

/// Demo event for tracking what happened
#[derive(Debug, Clone)]
pub enum DemoEvent {
    /// Scenario started
    ScenarioStarted { name: String },
    /// Stage completed
    StageCompleted { stage: usize, total: usize },
    /// Agent action occurred
    AgentAction { agent: String, action: String },
    /// Message sent between agents
    MessageSent { from: String, to: String, msg_type: String },
    /// Tokens processed
    TokensProcessed { count: u64, rate: f64 },
    /// Effect triggered
    EffectTriggered { effect_type: String, x: f32, y: f32 },
    /// Scenario completed
    ScenarioCompleted { duration_secs: f32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demo_config_default() {
        let config = DemoConfig::default();
        assert!((config.speed_multiplier - 1.0).abs() < 0.001);
        assert!(!config.auto_advance);
        assert!(config.particles_enabled);
    }

    #[test]
    fn test_demo_config_fast() {
        let config = DemoConfig::fast();
        assert!(config.speed_multiplier > 1.0);
        assert!(config.auto_advance);
    }

    #[test]
    fn test_demo_config_presentation() {
        let config = DemoConfig::presentation();
        assert!(config.speed_multiplier < 1.0);
        assert!(!config.auto_advance);
        assert!(config.max_particles > 100);
    }
}
