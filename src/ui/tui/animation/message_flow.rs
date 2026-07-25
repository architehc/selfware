//! Message Flow Animation
//!
//! Visualizes messages flowing between agents with:
//! - Animated particles traveling between points
//! - Trail effects
//! - Message type indicators

use super::{colors, Animation};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Types of messages that can flow between agents
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// Request from one agent to another
    Request,
    /// Response to a request
    Response,
    /// Broadcast to all agents
    Broadcast,
    /// Consensus voting message
    Consensus,
    /// Error notification
    Error,
}

impl MessageType {
    /// Get the symbol for this message type
    pub fn symbol(&self) -> &'static str {
        match self {
            MessageType::Request => "●",
            MessageType::Response => "◆",
            MessageType::Broadcast => "★",
            MessageType::Consensus => "◎",
            MessageType::Error => "✗",
        }
    }

    /// Get the color for this message type
    pub fn color(&self) -> Color {
        match self {
            MessageType::Request => colors::SECONDARY, // Blue
            MessageType::Response => colors::SUCCESS,  // Green
            MessageType::Broadcast => colors::WARNING, // Yellow
            MessageType::Consensus => colors::PRIMARY, // Coral
            MessageType::Error => colors::ERROR,       // Red
        }
    }
}

/// Animated message flowing between two points
pub struct MessageFlow {
    /// Starting position (x, y)
    from: (f32, f32),
    /// Ending position (x, y)
    to: (f32, f32),
    /// Current progress (0.0 to 1.0)
    progress: f32,
    /// Animation speed (progress per second)
    speed: f32,
    /// Type of message
    message_type: MessageType,
    /// Trail length (number of trailing particles)
    trail_length: u8,
    /// Whether to loop the animation
    looping: bool,
}

impl MessageFlow {
    pub fn new(from: (f32, f32), to: (f32, f32), message_type: MessageType) -> Self {
        Self {
            from,
            to,
            progress: 0.0,
            speed: 2.0, // Complete in 0.5 seconds
            message_type,
            trail_length: 5,
            looping: false,
        }
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Get the current position of the message particle
    pub fn current_position(&self) -> (f32, f32) {
        let x = self.from.0 + (self.to.0 - self.from.0) * self.progress;
        let y = self.from.1 + (self.to.1 - self.from.1) * self.progress;
        (x, y)
    }

    /// Get trail positions (older positions)
    pub fn trail_positions(&self) -> Vec<(f32, f32)> {
        let mut positions = Vec::new();
        for i in 1..=self.trail_length {
            let trail_progress = self.progress - (i as f32 * 0.05);
            if trail_progress > 0.0 {
                let x = self.from.0 + (self.to.0 - self.from.0) * trail_progress;
                let y = self.from.1 + (self.to.1 - self.from.1) * trail_progress;
                positions.push((x, y));
            }
        }
        positions
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn message_type(&self) -> MessageType {
        self.message_type
    }
}

impl Animation for MessageFlow {
    fn update(&mut self, delta_time: f32) {
        self.progress += self.speed * delta_time;

        if self.progress >= 1.0 {
            if self.looping {
                self.progress = 0.0;
            } else {
                self.progress = 1.0;
            }
        }
    }

    fn is_complete(&self) -> bool {
        !self.looping && self.progress >= 1.0
    }
}

impl Widget for &MessageFlow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let symbol = self.message_type.symbol();
        let color = self.message_type.color();

        // Draw trail first (so main particle is on top)
        for (i, (tx, ty)) in self.trail_positions().iter().enumerate() {
            let x = tx.round() as u16;
            let y = ty.round() as u16;

            if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
                // Fade trail based on position
                let fade = 1.0 - (i as f32 * 0.15);
                let trail_color = if let Color::Rgb(r, g, b) = color {
                    Color::Rgb(
                        (r as f32 * fade) as u8,
                        (g as f32 * fade) as u8,
                        (b as f32 * fade) as u8,
                    )
                } else {
                    color
                };

                buf[(x, y)]
                    .set_symbol("·")
                    .set_style(Style::default().fg(trail_color));
            }
        }

        // Draw main particle
        let (px, py) = self.current_position();
        let x = px.round() as u16;
        let y = py.round() as u16;

        if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
            buf[(x, y)]
                .set_symbol(symbol)
                .set_style(Style::default().fg(color).add_modifier(Modifier::BOLD));
        }
    }
}

/// Manager for multiple message flows
pub struct MessageFlowManager {
    flows: Vec<MessageFlow>,
}

impl MessageFlowManager {
    pub fn new() -> Self {
        Self { flows: Vec::new() }
    }

    pub fn add(&mut self, flow: MessageFlow) {
        self.flows.push(flow);
    }

    pub fn update(&mut self, delta_time: f32) {
        for flow in &mut self.flows {
            flow.update(delta_time);
        }
        // Remove completed flows
        self.flows.retain(|f| !f.is_complete());
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        for flow in &self.flows {
            flow.render(area, buf);
        }
    }

    pub fn flow_count(&self) -> usize {
        self.flows.len()
    }
}

impl Default for MessageFlowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/tui/animation/message_flow/message_flow_test.rs"]
mod tests;
