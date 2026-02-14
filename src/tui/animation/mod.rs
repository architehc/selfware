//! Animation Engine for Selfware TUI
//!
//! Provides animated widgets and effects for the terminal UI:
//! - Animated progress bars with wave effects
//! - Agent avatar widgets with pulse animations
//! - Message flow particles between agents
//! - Token stream visualizations
//! - Particle system for sparkle effects

pub mod progress;
pub mod agent_avatar;
pub mod message_flow;
pub mod token_stream;
pub mod particles;

pub use progress::AnimatedProgressBar;
pub use agent_avatar::{AgentAvatar, AgentRole, ActivityLevel};
pub use message_flow::{MessageFlow, MessageFlowManager, MessageType};
pub use token_stream::{TokenStream, TokenSize};
pub use particles::{ParticleSystem, Particle, EmitConfig};

use std::time::Instant;

/// Animation manager that coordinates all animations
pub struct AnimationManager {
    animations: Vec<Box<dyn Animation>>,
    _last_update: Instant,
    paused: bool,
}

/// Trait for animatable elements
pub trait Animation: Send + Sync {
    fn update(&mut self, delta_time: f32);
    fn is_complete(&self) -> bool;
}

impl AnimationManager {
    pub fn new() -> Self {
        Self {
            animations: Vec::new(),
            _last_update: Instant::now(),
            paused: false,
        }
    }

    pub fn add<A: Animation + 'static>(&mut self, animation: A) {
        self.animations.push(Box::new(animation));
    }

    pub fn update(&mut self, delta_time: f32) {
        if self.paused {
            return;
        }

        for animation in &mut self.animations {
            animation.update(delta_time);
        }

        // Remove completed animations
        self.animations.retain(|a| !a.is_complete());
    }

    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn animation_count(&self) -> usize {
        self.animations.len()
    }
}

impl Default for AnimationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Animation timing constants
pub mod timing {
    use std::time::Duration;

    /// Micro animations (button presses, instant feedback)
    pub const MICRO: Duration = Duration::from_millis(100);

    /// Standard transitions
    pub const STANDARD: Duration = Duration::from_millis(300);

    /// Emphasis animations (notifications)
    pub const EMPHASIS: Duration = Duration::from_millis(500);

    /// Dramatic scene changes
    pub const DRAMATIC: Duration = Duration::from_millis(1000);

    /// Ambient background animations
    pub const AMBIENT_MIN: Duration = Duration::from_millis(2000);
    pub const AMBIENT_MAX: Duration = Duration::from_millis(5000);
}

/// Color palette for animations
pub mod colors {
    use ratatui::style::Color;

    /// Background (Deep Navy)
    pub const BACKGROUND: Color = Color::Rgb(0x1A, 0x1A, 0x2E);

    /// Primary (Coral) - Used for agents
    pub const PRIMARY: Color = Color::Rgb(0xFF, 0x6B, 0x6B);

    /// Secondary (Sky Blue) - Used for messages
    pub const SECONDARY: Color = Color::Rgb(0x4E, 0xC5, 0xF1);

    /// Accent (Mint) - Used for success
    pub const ACCENT: Color = Color::Rgb(0x95, 0xE1, 0xD3);

    /// Warning (Yellow)
    pub const WARNING: Color = Color::Rgb(0xFF, 0xD9, 0x3D);

    /// Error (Red)
    pub const ERROR: Color = Color::Rgb(0xFF, 0x5F, 0x5F);

    /// Success (Green)
    pub const SUCCESS: Color = Color::Rgb(0x52, 0xD6, 0x81);

    /// Purple accent
    pub const PURPLE: Color = Color::Rgb(0x6B, 0x7A, 0xF7);

    /// Orange accent
    pub const ORANGE: Color = Color::Rgb(0xFF, 0x9F, 0x43);

    /// Gradient for progress bars
    pub const GRADIENT: [Color; 4] = [
        Color::Rgb(0x52, 0xD6, 0x81),  // Green
        Color::Rgb(0x95, 0xE1, 0xD3),  // Mint
        Color::Rgb(0x4E, 0xC5, 0xF1),  // Blue
        Color::Rgb(0x6B, 0x7A, 0xF7),  // Purple
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAnimation {
        updates: u32,
        complete_after: u32,
    }

    impl Animation for TestAnimation {
        fn update(&mut self, _delta_time: f32) {
            self.updates += 1;
        }

        fn is_complete(&self) -> bool {
            self.updates >= self.complete_after
        }
    }

    #[test]
    fn test_animation_manager_new() {
        let manager = AnimationManager::new();
        assert_eq!(manager.animation_count(), 0);
        assert!(!manager.is_paused());
    }

    #[test]
    fn test_animation_manager_add() {
        let mut manager = AnimationManager::new();
        manager.add(TestAnimation { updates: 0, complete_after: 5 });
        assert_eq!(manager.animation_count(), 1);
    }

    #[test]
    fn test_animation_manager_update() {
        let mut manager = AnimationManager::new();
        manager.add(TestAnimation { updates: 0, complete_after: 3 });

        manager.update(0.016);
        assert_eq!(manager.animation_count(), 1);

        manager.update(0.016);
        manager.update(0.016);
        assert_eq!(manager.animation_count(), 0); // Should be removed after completing
    }

    #[test]
    fn test_animation_manager_pause() {
        let mut manager = AnimationManager::new();
        manager.add(TestAnimation { updates: 0, complete_after: 10 });

        manager.toggle_pause();
        assert!(manager.is_paused());

        // Updates should not happen while paused
        manager.update(0.016);
        assert_eq!(manager.animation_count(), 1);

        manager.toggle_pause();
        assert!(!manager.is_paused());
    }
}
