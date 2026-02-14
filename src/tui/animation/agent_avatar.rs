//! Agent Avatar Widget
//!
//! Displays an agent with:
//! - Role-specific icon and color
//! - Pulsing border based on activity level
//! - Token count display
//! - Activity indicators

use super::{colors, Animation};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Agent roles with associated icons and colors
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
    /// Get the emoji icon for this role
    pub fn icon(&self) -> &'static str {
        match self {
            AgentRole::Architect => "🏗️",
            AgentRole::Coder => "💻",
            AgentRole::Tester => "🧪",
            AgentRole::Reviewer => "👁️",
            AgentRole::Documenter => "📚",
            AgentRole::DevOps => "🚀",
            AgentRole::Security => "🔒",
            AgentRole::Performance => "⚡",
        }
    }

    /// Get ASCII fallback icon
    pub fn ascii_icon(&self) -> &'static str {
        match self {
            AgentRole::Architect => "[A]",
            AgentRole::Coder => "[C]",
            AgentRole::Tester => "[T]",
            AgentRole::Reviewer => "[R]",
            AgentRole::Documenter => "[D]",
            AgentRole::DevOps => "[O]",
            AgentRole::Security => "[S]",
            AgentRole::Performance => "[P]",
        }
    }

    /// Get the primary color for this role
    pub fn color(&self) -> Color {
        match self {
            AgentRole::Architect => colors::PRIMARY,     // Coral
            AgentRole::Coder => colors::SECONDARY,       // Blue
            AgentRole::Tester => colors::ACCENT,         // Mint
            AgentRole::Reviewer => colors::PURPLE,       // Purple
            AgentRole::Documenter => colors::WARNING,    // Yellow
            AgentRole::DevOps => colors::ERROR,          // Red
            AgentRole::Security => colors::SUCCESS,      // Green
            AgentRole::Performance => colors::ORANGE,    // Orange
        }
    }

    /// Get the role name
    pub fn name(&self) -> &'static str {
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

/// Activity level affects animation intensity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActivityLevel {
    #[default]
    Idle,
    Low,
    Medium,
    High,
    Max,
    Complete,
    Error,
}

impl ActivityLevel {
    /// Get the activity indicator symbol
    pub fn symbol(&self) -> &'static str {
        match self {
            ActivityLevel::Idle => "○",
            ActivityLevel::Low => "◐",
            ActivityLevel::Medium => "◑",
            ActivityLevel::High => "◓",
            ActivityLevel::Max => "●",
            ActivityLevel::Complete => "✓",
            ActivityLevel::Error => "✗",
        }
    }

    /// Get number of filled dots (out of 5)
    pub fn dots(&self) -> u8 {
        match self {
            ActivityLevel::Idle => 0,
            ActivityLevel::Low => 1,
            ActivityLevel::Medium => 2,
            ActivityLevel::High => 3,
            ActivityLevel::Max => 4,
            ActivityLevel::Complete => 5,
            ActivityLevel::Error => 0,
        }
    }
}

/// Animated agent avatar widget
pub struct AgentAvatar {
    /// Agent's role
    role: AgentRole,
    /// Current activity level
    activity: ActivityLevel,
    /// Token count processed
    token_count: u64,
    /// Pulse animation phase (0.0 to 2π)
    pulse_phase: f32,
    /// Agent ID/name
    name: String,
    /// Use ASCII mode (no emojis)
    ascii_mode: bool,
}

impl AgentAvatar {
    pub fn new(role: AgentRole) -> Self {
        Self {
            role,
            activity: ActivityLevel::Idle,
            token_count: 0,
            pulse_phase: 0.0,
            name: format!("{}-1", role.name()),
            ascii_mode: false,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_activity(mut self, activity: ActivityLevel) -> Self {
        self.activity = activity;
        self
    }

    pub fn with_tokens(mut self, tokens: u64) -> Self {
        self.token_count = tokens;
        self
    }

    pub fn ascii_mode(mut self, enabled: bool) -> Self {
        self.ascii_mode = enabled;
        self
    }

    pub fn set_activity(&mut self, activity: ActivityLevel) {
        self.activity = activity;
    }

    pub fn set_tokens(&mut self, tokens: u64) {
        self.token_count = tokens;
    }

    pub fn add_tokens(&mut self, tokens: u64) {
        self.token_count += tokens;
    }

    pub fn role(&self) -> AgentRole {
        self.role
    }

    pub fn activity(&self) -> ActivityLevel {
        self.activity
    }

    /// Calculate pulse intensity (0-255)
    fn pulse_intensity(&self) -> u8 {
        if self.activity == ActivityLevel::Idle {
            return 255;
        }

        // Pulse between 128-255
        let pulse = (self.pulse_phase.sin() + 1.0) / 2.0;
        (128.0 + pulse * 127.0) as u8
    }

    /// Get color with pulse applied
    fn pulsed_color(&self) -> Color {
        let base = self.role.color();
        let intensity = self.pulse_intensity() as f32 / 255.0;

        // Extract RGB components
        if let Color::Rgb(r, g, b) = base {
            Color::Rgb(
                (r as f32 * intensity) as u8,
                (g as f32 * intensity) as u8,
                (b as f32 * intensity) as u8,
            )
        } else {
            base
        }
    }

    /// Format token count for display
    fn format_tokens(&self) -> String {
        if self.token_count >= 1_000_000 {
            format!("{:.1}M", self.token_count as f64 / 1_000_000.0)
        } else if self.token_count >= 1_000 {
            format!("{}K", self.token_count / 1_000)
        } else {
            format!("{}", self.token_count)
        }
    }
}

impl Animation for AgentAvatar {
    fn update(&mut self, delta_time: f32) {
        // Update pulse phase based on activity
        let speed = match self.activity {
            ActivityLevel::Idle => 0.5,
            ActivityLevel::Low => 1.0,
            ActivityLevel::Medium => 2.0,
            ActivityLevel::High => 3.0,
            ActivityLevel::Max => 5.0,
            ActivityLevel::Complete => 0.0,
            ActivityLevel::Error => 4.0,
        };

        self.pulse_phase += delta_time * speed;
        if self.pulse_phase > std::f32::consts::PI * 2.0 {
            self.pulse_phase -= std::f32::consts::PI * 2.0;
        }
    }

    fn is_complete(&self) -> bool {
        false // Avatars don't complete
    }
}

impl Widget for &AgentAvatar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 12 || area.height < 5 {
            return;
        }

        let pulsed_color = self.pulsed_color();
        let border_style = Style::default().fg(pulsed_color);
        let content_style = Style::default().fg(Color::White);

        // Draw border
        // Top
        buf.get_mut(area.x, area.y).set_symbol("┌").set_style(border_style);
        for x in area.x + 1..area.x + area.width - 1 {
            buf.get_mut(x, area.y).set_symbol("─").set_style(border_style);
        }
        buf.get_mut(area.x + area.width - 1, area.y).set_symbol("┐").set_style(border_style);

        // Sides
        for y in area.y + 1..area.y + area.height - 1 {
            buf.get_mut(area.x, y).set_symbol("│").set_style(border_style);
            buf.get_mut(area.x + area.width - 1, y).set_symbol("│").set_style(border_style);
        }

        // Bottom
        buf.get_mut(area.x, area.y + area.height - 1).set_symbol("└").set_style(border_style);
        for x in area.x + 1..area.x + area.width - 1 {
            buf.get_mut(x, area.y + area.height - 1).set_symbol("─").set_style(border_style);
        }
        buf.get_mut(area.x + area.width - 1, area.y + area.height - 1).set_symbol("┘").set_style(border_style);

        let inner_x = area.x + 2;
        let inner_y = area.y + 1;

        // Line 1: Icon and name
        let icon = if self.ascii_mode {
            self.role.ascii_icon()
        } else {
            self.role.icon()
        };

        // Write icon
        let mut x_offset = 0u16;
        for ch in icon.chars() {
            if inner_x + x_offset < area.x + area.width - 1 {
                buf.get_mut(inner_x + x_offset, inner_y)
                    .set_symbol(&ch.to_string())
                    .set_style(Style::default().fg(self.role.color()));
                x_offset += 1;
            }
        }

        // Write name (truncated)
        let max_name_len = (area.width - 6).min(10) as usize;
        let name_display: String = self.name.chars().take(max_name_len).collect();
        for (i, ch) in name_display.chars().enumerate() {
            let x = inner_x + x_offset + 1 + i as u16;
            if x < area.x + area.width - 1 {
                buf.get_mut(x, inner_y)
                    .set_symbol(&ch.to_string())
                    .set_style(content_style);
            }
        }

        // Line 2: Activity dots
        if area.height > 3 {
            let activity_y = inner_y + 1;
            let dots_filled = self.activity.dots();
            let activity_color = match self.activity {
                ActivityLevel::Complete => colors::SUCCESS,
                ActivityLevel::Error => colors::ERROR,
                _ => pulsed_color,
            };

            for i in 0..5 {
                let symbol = if i < dots_filled { "●" } else { "○" };
                let x = inner_x + i as u16 * 2;
                if x < area.x + area.width - 1 {
                    buf.get_mut(x, activity_y)
                        .set_symbol(symbol)
                        .set_style(Style::default().fg(activity_color));
                }
            }

            // Activity status
            let status = self.activity.symbol();
            let status_x = inner_x + 11;
            if status_x < area.x + area.width - 1 {
                buf.get_mut(status_x, activity_y)
                    .set_symbol(status)
                    .set_style(Style::default().fg(activity_color).add_modifier(Modifier::BOLD));
            }
        }

        // Line 3: Token count
        if area.height > 4 {
            let token_y = inner_y + 2;
            let token_text = format!("💫 {}", self.format_tokens());
            let display_text = if self.ascii_mode {
                format!("* {}", self.format_tokens())
            } else {
                token_text
            };

            for (i, ch) in display_text.chars().enumerate() {
                let x = inner_x + i as u16;
                if x < area.x + area.width - 1 {
                    buf.get_mut(x, token_y)
                        .set_symbol(&ch.to_string())
                        .set_style(Style::default().fg(Color::Gray));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_icon() {
        assert_eq!(AgentRole::Coder.icon(), "💻");
        assert_eq!(AgentRole::Architect.icon(), "🏗️");
    }

    #[test]
    fn test_agent_role_color() {
        let color = AgentRole::Coder.color();
        assert_eq!(color, colors::SECONDARY);
    }

    #[test]
    fn test_agent_avatar_new() {
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
    fn test_agent_avatar_format_tokens() {
        let avatar1 = AgentAvatar::new(AgentRole::Coder).with_tokens(500);
        assert_eq!(avatar1.format_tokens(), "500");

        let avatar2 = AgentAvatar::new(AgentRole::Coder).with_tokens(5_000);
        assert_eq!(avatar2.format_tokens(), "5K");

        let avatar3 = AgentAvatar::new(AgentRole::Coder).with_tokens(1_500_000);
        assert_eq!(avatar3.format_tokens(), "1.5M");
    }

    #[test]
    fn test_activity_level_dots() {
        assert_eq!(ActivityLevel::Idle.dots(), 0);
        assert_eq!(ActivityLevel::Low.dots(), 1);
        assert_eq!(ActivityLevel::Max.dots(), 4);
        assert_eq!(ActivityLevel::Complete.dots(), 5);
    }
}
