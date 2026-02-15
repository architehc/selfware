//! Agent Avatar Widget
//!
//! Animated avatars representing different agent roles with pulse effects
//! based on activity level.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use std::time::Instant;

/// Roles an agent can have in the multi-agent system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentRole {
    Architect,
    Coder,
    Tester,
    Reviewer,
    Documenter,
    DevOps,
    Security,
    Performance,
}

impl AgentRole {
    /// Get the icon representing this role.
    pub fn icon(&self) -> &'static str {
        match self {
            AgentRole::Architect => "\u{1f3d7}",  // construction
            AgentRole::Coder => "\u{1f4bb}",      // laptop
            AgentRole::Tester => "\u{1f9ea}",     // test tube
            AgentRole::Reviewer => "\u{1f50d}",   // magnifying glass
            AgentRole::Documenter => "\u{1f4dd}", // memo
            AgentRole::DevOps => "\u{2699}",      // gear
            AgentRole::Security => "\u{1f6e1}",   // shield
            AgentRole::Performance => "\u{26a1}",  // lightning
        }
    }

    /// Get the color associated with this role.
    pub fn color(&self) -> Color {
        match self {
            AgentRole::Architect => Color::Rgb(100, 149, 237),  // Cornflower blue
            AgentRole::Coder => Color::Rgb(96, 108, 56),        // Garden green
            AgentRole::Tester => Color::Rgb(212, 163, 115),     // Amber
            AgentRole::Reviewer => Color::Rgb(184, 115, 51),    // Copper
            AgentRole::Documenter => Color::Rgb(143, 151, 121), // Sage
            AgentRole::DevOps => Color::Rgb(139, 69, 19),       // Rust
            AgentRole::Security => Color::Rgb(220, 20, 60),     // Crimson
            AgentRole::Performance => Color::Rgb(255, 165, 0),  // Orange
        }
    }

    /// Get a human-readable label for this role.
    pub fn label(&self) -> &'static str {
        match self {
            AgentRole::Architect => "Architect",
            AgentRole::Coder => "Coder",
            AgentRole::Tester => "Tester",
            AgentRole::Reviewer => "Reviewer",
            AgentRole::Documenter => "Documenter",
            AgentRole::DevOps => "DevOps",
            AgentRole::Security => "Security",
            AgentRole::Performance => "Performance",
        }
    }
}

/// Level of activity for an agent, affecting pulse animation speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityLevel {
    /// Agent is idle, no pulse
    Idle,
    /// Low activity, slow pulse
    Low,
    /// Medium activity, moderate pulse
    Medium,
    /// High activity, fast pulse
    High,
    /// Maximum activity, rapid pulse
    Max,
    /// Task complete, steady glow
    Complete,
}

impl ActivityLevel {
    /// Get the pulse speed multiplier for this activity level.
    /// Returns None for Idle (no pulse) and Complete (steady).
    pub fn pulse_speed(&self) -> Option<f32> {
        match self {
            ActivityLevel::Idle => None,
            ActivityLevel::Low => Some(1.0),
            ActivityLevel::Medium => Some(2.0),
            ActivityLevel::High => Some(4.0),
            ActivityLevel::Max => Some(8.0),
            ActivityLevel::Complete => None,
        }
    }
}

/// An animated agent avatar with a pulse effect.
#[derive(Debug, Clone)]
pub struct AgentAvatar {
    /// The agent's role
    role: AgentRole,
    /// Current activity level
    activity: ActivityLevel,
    /// Animation frame counter
    animation_frame: u8,
    /// Last update timestamp
    last_update: Instant,
    /// Optional agent name
    name: Option<String>,
}

impl AgentAvatar {
    /// Create a new agent avatar for the given role.
    pub fn new(role: AgentRole) -> Self {
        Self {
            role,
            activity: ActivityLevel::Idle,
            animation_frame: 0,
            last_update: Instant::now(),
            name: None,
        }
    }

    /// Set the activity level.
    pub fn with_activity(mut self, activity: ActivityLevel) -> Self {
        self.activity = activity;
        self
    }

    /// Set an optional name for the agent.
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    /// Get the agent's role.
    pub fn role(&self) -> AgentRole {
        self.role
    }

    /// Get the current activity level.
    pub fn activity(&self) -> ActivityLevel {
        self.activity
    }

    /// Set the activity level.
    pub fn set_activity(&mut self, activity: ActivityLevel) {
        self.activity = activity;
    }

    /// Advance the animation by one frame.
    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
        self.last_update = Instant::now();
    }

    /// Compute the current pulse intensity (0.0 to 1.0).
    pub fn pulse_intensity(&self) -> f32 {
        match self.activity {
            ActivityLevel::Idle => 0.3,
            ActivityLevel::Complete => 1.0,
            _ => {
                let speed = self.activity.pulse_speed().unwrap_or(1.0);
                let t = self.animation_frame as f32 * speed / 20.0;
                0.5 + 0.5 * t.sin()
            }
        }
    }

    /// Get the display style adjusted by pulse intensity.
    fn pulse_style(&self) -> Style {
        let base_color = self.role.color();
        let intensity = self.pulse_intensity();

        // Modulate the color brightness by the pulse intensity
        let (r, g, b) = match base_color {
            Color::Rgb(r, g, b) => (r, g, b),
            _ => (200, 200, 200),
        };

        let r = (r as f32 * intensity) as u8;
        let g = (g as f32 * intensity) as u8;
        let b = (b as f32 * intensity) as u8;

        let mut style = Style::default().fg(Color::Rgb(r, g, b));
        if self.activity == ActivityLevel::Complete {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

impl Widget for AgentAvatar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let label = self
            .name
            .as_deref()
            .unwrap_or_else(|| self.role.label());
        let display = format!("{} {}", self.role.icon(), label);
        let style = self.pulse_style();

        let line = Line::from(Span::styled(display, style));
        let paragraph = Paragraph::new(line);
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_architect_icon_and_color() {
        let role = AgentRole::Architect;
        assert_eq!(role.icon(), "\u{1f3d7}");
        assert_eq!(role.color(), Color::Rgb(100, 149, 237));
    }

    #[test]
    fn test_coder_icon_and_color() {
        let role = AgentRole::Coder;
        assert_eq!(role.icon(), "\u{1f4bb}");
        assert_eq!(role.color(), Color::Rgb(96, 108, 56));
    }

    #[test]
    fn test_tester_icon_and_color() {
        let role = AgentRole::Tester;
        assert_eq!(role.icon(), "\u{1f9ea}");
        assert_eq!(role.color(), Color::Rgb(212, 163, 115));
    }

    #[test]
    fn test_reviewer_icon_and_color() {
        let role = AgentRole::Reviewer;
        assert_eq!(role.icon(), "\u{1f50d}");
        assert_eq!(role.color(), Color::Rgb(184, 115, 51));
    }

    #[test]
    fn test_documenter_icon_and_color() {
        let role = AgentRole::Documenter;
        assert_eq!(role.icon(), "\u{1f4dd}");
        assert_eq!(role.color(), Color::Rgb(143, 151, 121));
    }

    #[test]
    fn test_devops_icon_and_color() {
        let role = AgentRole::DevOps;
        assert_eq!(role.icon(), "\u{2699}");
        assert_eq!(role.color(), Color::Rgb(139, 69, 19));
    }

    #[test]
    fn test_security_icon_and_color() {
        let role = AgentRole::Security;
        assert_eq!(role.icon(), "\u{1f6e1}");
        assert_eq!(role.color(), Color::Rgb(220, 20, 60));
    }

    #[test]
    fn test_performance_icon_and_color() {
        let role = AgentRole::Performance;
        assert_eq!(role.icon(), "\u{26a1}");
        assert_eq!(role.color(), Color::Rgb(255, 165, 0));
    }

    #[test]
    fn test_activity_level_pulse_speed() {
        assert!(ActivityLevel::Idle.pulse_speed().is_none());
        assert!((ActivityLevel::Low.pulse_speed().unwrap() - 1.0).abs() < f32::EPSILON);
        assert!((ActivityLevel::Medium.pulse_speed().unwrap() - 2.0).abs() < f32::EPSILON);
        assert!((ActivityLevel::High.pulse_speed().unwrap() - 4.0).abs() < f32::EPSILON);
        assert!((ActivityLevel::Max.pulse_speed().unwrap() - 8.0).abs() < f32::EPSILON);
        assert!(ActivityLevel::Complete.pulse_speed().is_none());
    }

    #[test]
    fn test_agent_avatar_creation() {
        let avatar = AgentAvatar::new(AgentRole::Coder);
        assert_eq!(avatar.role(), AgentRole::Coder);
        assert_eq!(avatar.activity(), ActivityLevel::Idle);
    }

    #[test]
    fn test_agent_avatar_with_activity() {
        let avatar = AgentAvatar::new(AgentRole::Tester)
            .with_activity(ActivityLevel::High);
        assert_eq!(avatar.activity(), ActivityLevel::High);
    }

    #[test]
    fn test_agent_avatar_set_activity() {
        let mut avatar = AgentAvatar::new(AgentRole::Architect);
        avatar.set_activity(ActivityLevel::Complete);
        assert_eq!(avatar.activity(), ActivityLevel::Complete);
    }

    #[test]
    fn test_agent_avatar_pulse_idle() {
        let avatar = AgentAvatar::new(AgentRole::Coder);
        let intensity = avatar.pulse_intensity();
        assert!((intensity - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_agent_avatar_pulse_complete() {
        let avatar = AgentAvatar::new(AgentRole::Coder)
            .with_activity(ActivityLevel::Complete);
        let intensity = avatar.pulse_intensity();
        assert!((intensity - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_agent_avatar_pulse_animated() {
        let mut avatar = AgentAvatar::new(AgentRole::Coder)
            .with_activity(ActivityLevel::Medium);

        // Pulse should vary across frames
        let mut intensities = Vec::new();
        for _ in 0..20 {
            intensities.push(avatar.pulse_intensity());
            avatar.tick();
        }

        // Not all values should be the same (sine wave oscillates)
        let first = intensities[0];
        let has_variation = intensities.iter().any(|&v| (v - first).abs() > 0.01);
        assert!(has_variation, "Pulse should vary across frames");
    }

    #[test]
    fn test_agent_avatar_with_name() {
        let avatar = AgentAvatar::new(AgentRole::Security)
            .with_name("Guardian");
        assert_eq!(avatar.name.as_deref(), Some("Guardian"));
    }

    #[test]
    fn test_role_labels() {
        assert_eq!(AgentRole::Architect.label(), "Architect");
        assert_eq!(AgentRole::Coder.label(), "Coder");
        assert_eq!(AgentRole::Tester.label(), "Tester");
        assert_eq!(AgentRole::Reviewer.label(), "Reviewer");
        assert_eq!(AgentRole::Documenter.label(), "Documenter");
        assert_eq!(AgentRole::DevOps.label(), "DevOps");
        assert_eq!(AgentRole::Security.label(), "Security");
        assert_eq!(AgentRole::Performance.label(), "Performance");
    }
}
