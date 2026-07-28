//! Animation Engine for Selfware TUI
//!
//! Provides animated widgets and effects for the terminal UI:
//! - Animated progress bars with wave effects
//! - Agent avatar widgets with pulse animations
//! - Message flow particles between agents
//! - Token stream visualizations
//! - Particle system for sparkle effects

pub mod agent_avatar;
pub mod message_flow;
pub mod particles;
pub mod progress;
pub mod token_stream;

pub use agent_avatar::{ActivityLevel, AgentAvatar, AgentRole};
pub use message_flow::{MessageFlow, MessageFlowManager, MessageType};
pub use particles::{EmitConfig, Particle, ParticleSystem};
pub use progress::AnimatedProgressBar;
pub use token_stream::{TokenSize, TokenStream};

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
}

/// Color palette for animations
pub mod colors {
    use ratatui::style::Color;

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
        Color::Rgb(0x52, 0xD6, 0x81), // Green
        Color::Rgb(0x95, 0xE1, 0xD3), // Mint
        Color::Rgb(0x4E, 0xC5, 0xF1), // Blue
        Color::Rgb(0x6B, 0x7A, 0xF7), // Purple
    ];
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/tui/animation/mod_test.rs"]
mod tests;
