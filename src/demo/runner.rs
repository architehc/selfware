//! Demo Runner
//!
//! Orchestrates demo scenario execution with animation coordination.

use super::{DemoConfig, DemoEvent};
use crate::tui::animation::{
    agent_avatar::{ActivityLevel, AgentAvatar, AgentRole},
    message_flow::{MessageFlow, MessageFlowManager, MessageType},
    particles::ParticleSystem,
    progress::AnimatedProgressBar,
    token_stream::TokenStream,
    Animation, AnimationManager,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Current state of the demo
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoState {
    /// Not yet started
    Idle,
    /// Running a scenario
    Running,
    /// Paused mid-execution
    Paused,
    /// Scenario completed
    Completed,
    /// Error occurred
    Error(String),
}

/// Demo scenario trait
pub trait DemoScenario: Send + Sync {
    /// Get scenario name
    fn name(&self) -> &str;

    /// Get scenario description
    fn description(&self) -> &str;

    /// Get total number of stages
    fn total_stages(&self) -> usize;

    /// Initialize the scenario
    fn initialize(&mut self, runner: &mut DemoRunner);

    /// Execute the next stage
    fn execute_stage(&mut self, stage: usize, runner: &mut DemoRunner) -> bool;

    /// Clean up after completion
    fn cleanup(&mut self, runner: &mut DemoRunner);
}

/// Demo runner that coordinates scenario execution
pub struct DemoRunner {
    /// Configuration
    config: DemoConfig,
    /// Current state
    state: DemoState,
    /// Current scenario name
    scenario_name: Option<String>,
    /// Current stage
    current_stage: usize,
    /// Total stages
    total_stages: usize,
    /// Start time
    start_time: Option<Instant>,
    /// Event history
    events: Vec<DemoEvent>,
    /// Animation manager
    animation_manager: AnimationManager,
    /// Particle system
    particle_system: ParticleSystem,
    /// Message flow manager
    message_flow_manager: MessageFlowManager,
    /// Token stream visualization
    token_stream: TokenStream,
    /// Progress bar
    progress_bar: AnimatedProgressBar,
    /// Agent avatars
    agents: HashMap<String, AgentAvatar>,
}

impl DemoRunner {
    /// Create a new demo runner
    pub fn new(config: DemoConfig) -> Self {
        Self {
            config: config.clone(),
            state: DemoState::Idle,
            scenario_name: None,
            current_stage: 0,
            total_stages: 0,
            start_time: None,
            events: Vec::new(),
            animation_manager: AnimationManager::new(),
            particle_system: ParticleSystem::new(config.max_particles),
            message_flow_manager: MessageFlowManager::new(),
            token_stream: TokenStream::new(50),
            progress_bar: AnimatedProgressBar::new(0.0),
            agents: HashMap::new(),
        }
    }

    /// Start a demo scenario
    pub fn start(&mut self, scenario: &mut dyn DemoScenario) {
        self.scenario_name = Some(scenario.name().to_string());
        self.total_stages = scenario.total_stages();
        self.current_stage = 0;
        self.start_time = Some(Instant::now());
        self.state = DemoState::Running;
        self.events.clear();

        self.emit_event(DemoEvent::ScenarioStarted {
            name: scenario.name().to_string(),
        });

        scenario.initialize(self);
    }

    /// Execute the next stage
    pub fn next_stage(&mut self, scenario: &mut dyn DemoScenario) -> bool {
        if self.state != DemoState::Running {
            return false;
        }

        if self.current_stage >= self.total_stages {
            self.complete(scenario);
            return false;
        }

        let success = scenario.execute_stage(self.current_stage, self);

        if success {
            self.emit_event(DemoEvent::StageCompleted {
                stage: self.current_stage,
                total: self.total_stages,
            });

            self.current_stage += 1;
            self.progress_bar.set_progress(self.current_stage as f32 / self.total_stages as f32);
        }

        success
    }

    /// Complete the scenario
    fn complete(&mut self, scenario: &mut dyn DemoScenario) {
        let duration = self.start_time.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0);

        self.emit_event(DemoEvent::ScenarioCompleted { duration_secs: duration });

        scenario.cleanup(self);
        self.state = DemoState::Completed;
    }

    /// Pause execution
    pub fn pause(&mut self) {
        if self.state == DemoState::Running {
            self.state = DemoState::Paused;
            self.animation_manager.toggle_pause();
        }
    }

    /// Resume execution
    pub fn resume(&mut self) {
        if self.state == DemoState::Paused {
            self.state = DemoState::Running;
            self.animation_manager.toggle_pause();
        }
    }

    /// Reset the runner
    pub fn reset(&mut self) {
        self.state = DemoState::Idle;
        self.scenario_name = None;
        self.current_stage = 0;
        self.total_stages = 0;
        self.start_time = None;
        self.events.clear();
        self.agents.clear();
        self.particle_system.clear();
        self.progress_bar.set_progress(0.0);
    }

    /// Update all animations
    pub fn update(&mut self, delta_time: f32) {
        let adjusted_delta = delta_time * self.config.speed_multiplier;

        self.animation_manager.update(adjusted_delta);
        self.particle_system.update(adjusted_delta);
        self.message_flow_manager.update(adjusted_delta);
        self.token_stream.update(adjusted_delta);
        self.progress_bar.update(adjusted_delta);

        for avatar in self.agents.values_mut() {
            avatar.update(adjusted_delta);
        }
    }

    // === Agent Management ===

    /// Add an agent to the demo
    pub fn add_agent(&mut self, id: &str, role: AgentRole) {
        let avatar = AgentAvatar::new(role).with_name(id);
        self.agents.insert(id.to_string(), avatar);
    }

    /// Set agent activity level
    pub fn set_agent_activity(&mut self, id: &str, level: ActivityLevel) {
        if let Some(avatar) = self.agents.get_mut(id) {
            avatar.set_activity(level);
        }
    }

    /// Add tokens to an agent
    pub fn add_agent_tokens(&mut self, id: &str, tokens: u64) {
        if let Some(avatar) = self.agents.get_mut(id) {
            avatar.add_tokens(tokens);
        }

        self.emit_event(DemoEvent::TokensProcessed {
            count: tokens,
            rate: tokens as f64 / 0.016, // Assume 60fps
        });
    }

    /// Get agent avatar reference
    pub fn agent(&self, id: &str) -> Option<&AgentAvatar> {
        self.agents.get(id)
    }

    // === Message Flow ===

    /// Send a message between agents
    pub fn send_message(&mut self, from: &str, to: &str, msg_type: MessageType, from_pos: (f32, f32), to_pos: (f32, f32)) {
        let flow = MessageFlow::new(from_pos, to_pos, msg_type);
        self.message_flow_manager.add(flow);

        self.emit_event(DemoEvent::MessageSent {
            from: from.to_string(),
            to: to.to_string(),
            msg_type: format!("{:?}", msg_type),
        });
    }

    // === Particle Effects ===

    /// Trigger a sparkle effect
    pub fn sparkle(&mut self, x: f32, y: f32, count: usize) {
        if self.config.particles_enabled {
            self.particle_system.sparkle(x, y, count);
            self.emit_event(DemoEvent::EffectTriggered {
                effect_type: "sparkle".to_string(),
                x, y,
            });
        }
    }

    /// Trigger an explosion effect
    pub fn explode(&mut self, x: f32, y: f32, count: usize) {
        if self.config.particles_enabled {
            self.particle_system.explode(x, y, count);
            self.emit_event(DemoEvent::EffectTriggered {
                effect_type: "explode".to_string(),
                x, y,
            });
        }
    }

    /// Trigger a celebration effect
    pub fn celebrate(&mut self, x: f32, y: f32) {
        if self.config.particles_enabled {
            self.particle_system.celebrate(x, y);
            self.emit_event(DemoEvent::EffectTriggered {
                effect_type: "celebrate".to_string(),
                x, y,
            });
        }
    }

    // === Token Stream ===

    /// Set token rate
    pub fn set_token_rate(&mut self, rate: f64) {
        self.token_stream.set_rate(rate);
    }

    /// Set total tokens
    pub fn set_total_tokens(&mut self, total: u64) {
        self.token_stream.set_total(total);
    }

    // === Event System ===

    /// Emit a demo event
    pub fn emit_event(&mut self, event: DemoEvent) {
        self.events.push(event);
    }

    /// Get event history
    pub fn events(&self) -> &[DemoEvent] {
        &self.events
    }

    // === Accessors ===

    pub fn state(&self) -> &DemoState {
        &self.state
    }

    pub fn current_stage(&self) -> usize {
        self.current_stage
    }

    pub fn total_stages(&self) -> usize {
        self.total_stages
    }

    pub fn progress(&self) -> f32 {
        if self.total_stages == 0 {
            0.0
        } else {
            self.current_stage as f32 / self.total_stages as f32
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.map(|t| t.elapsed()).unwrap_or_default()
    }

    pub fn particle_system(&self) -> &ParticleSystem {
        &self.particle_system
    }

    pub fn message_flow_manager(&self) -> &MessageFlowManager {
        &self.message_flow_manager
    }

    pub fn token_stream(&self) -> &TokenStream {
        &self.token_stream
    }

    pub fn progress_bar(&self) -> &AnimatedProgressBar {
        &self.progress_bar
    }

    pub fn agents(&self) -> &HashMap<String, AgentAvatar> {
        &self.agents
    }

    pub fn config(&self) -> &DemoConfig {
        &self.config
    }
}

impl Default for DemoRunner {
    fn default() -> Self {
        Self::new(DemoConfig::default())
    }
}

#[cfg(test)]
mod tests {
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
}
