//! Message Flow Animation
//!
//! Visualizes animated messages flowing between points in the TUI,
//! representing inter-agent communication.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::time::Instant;

/// Types of messages that can flow between agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    /// A request from one agent to another
    Request,
    /// A response back to the requester
    Response,
    /// A broadcast to all agents
    Broadcast,
    /// A consensus-building message
    Consensus,
}

impl MessageType {
    /// Get the display symbol for this message type.
    pub fn symbol(&self) -> &'static str {
        match self {
            MessageType::Request => "\u{25b6}",   // right-pointing triangle
            MessageType::Response => "\u{25c0}",  // left-pointing triangle
            MessageType::Broadcast => "\u{25c6}", // black diamond
            MessageType::Consensus => "\u{2605}", // star
        }
    }

    /// Get the color associated with this message type.
    pub fn color(&self) -> Color {
        match self {
            MessageType::Request => Color::Rgb(100, 149, 237),   // Cornflower blue
            MessageType::Response => Color::Rgb(144, 190, 109),  // Bloom green
            MessageType::Broadcast => Color::Rgb(212, 163, 115), // Amber
            MessageType::Consensus => Color::Rgb(184, 115, 51),  // Copper
        }
    }
}

/// A point in 2D space for message source/destination.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    /// Create a new point.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// An animated message flowing from a source to a destination point.
#[derive(Debug, Clone)]
pub struct MessageFlow {
    /// Type of this message
    message_type: MessageType,
    /// Source point (normalized 0.0..1.0)
    source: Point,
    /// Destination point (normalized 0.0..1.0)
    destination: Point,
    /// Current progress along the path (0.0 = at source, 1.0 = at destination)
    progress: f32,
    /// Speed of movement (progress units per update)
    speed: f32,
    /// Whether this message has reached its destination
    completed: bool,
    /// Timestamp of creation
    created_at: Instant,
}

impl MessageFlow {
    /// Create a new message flow between two points.
    pub fn new(message_type: MessageType, source: Point, destination: Point) -> Self {
        Self {
            message_type,
            source,
            destination,
            progress: 0.0,
            speed: 0.05,
            completed: false,
            created_at: Instant::now(),
        }
    }

    /// Set the movement speed (progress per update call).
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.max(0.001);
        self
    }

    /// Get the message type.
    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    /// Get the current progress (0.0..=1.0).
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Update the message position, advancing along the path.
    /// Returns true if the message just completed on this update.
    pub fn update(&mut self) -> bool {
        if self.completed {
            return false;
        }

        self.progress += self.speed;
        if self.progress >= 1.0 {
            self.progress = 1.0;
            self.completed = true;
            return true;
        }
        false
    }

    /// Check whether the message has reached its destination.
    pub fn is_complete(&self) -> bool {
        self.completed
    }

    /// Get the current interpolated position of the message.
    pub fn current_position(&self) -> Point {
        let t = self.progress.clamp(0.0, 1.0);
        Point {
            x: self.source.x + (self.destination.x - self.source.x) * t,
            y: self.source.y + (self.destination.y - self.source.y) * t,
        }
    }

    /// Get the elapsed time since creation.
    pub fn elapsed(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}

impl Widget for MessageFlow {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let pos = self.current_position();
        let screen_x = area.x + (pos.x * (area.width.saturating_sub(1)) as f32) as u16;
        let screen_y = area.y + (pos.y * (area.height.saturating_sub(1)) as f32) as u16;

        // Only render if within bounds
        if screen_x >= area.x
            && screen_x < area.right()
            && screen_y >= area.y
            && screen_y < area.bottom()
        {
            let style = Style::default().fg(self.message_type.color());
            let cell = buf.get_mut(screen_x, screen_y);
            cell.set_symbol(self.message_type.symbol());
            cell.set_style(style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_type_symbols() {
        assert_eq!(MessageType::Request.symbol(), "\u{25b6}");
        assert_eq!(MessageType::Response.symbol(), "\u{25c0}");
        assert_eq!(MessageType::Broadcast.symbol(), "\u{25c6}");
        assert_eq!(MessageType::Consensus.symbol(), "\u{2605}");
    }

    #[test]
    fn test_message_type_colors() {
        // Each type should return a distinct Rgb color
        let colors: Vec<Color> = vec![
            MessageType::Request.color(),
            MessageType::Response.color(),
            MessageType::Broadcast.color(),
            MessageType::Consensus.color(),
        ];
        for color in &colors {
            assert!(matches!(color, Color::Rgb(_, _, _)));
        }
        // All should be distinct
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j]);
            }
        }
    }

    #[test]
    fn test_message_flow_creation() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 1.0);
        let flow = MessageFlow::new(MessageType::Request, src, dst);

        assert_eq!(flow.message_type(), MessageType::Request);
        assert!((flow.progress() - 0.0).abs() < f32::EPSILON);
        assert!(!flow.is_complete());
    }

    #[test]
    fn test_message_flow_update_progression() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 1.0);
        let mut flow = MessageFlow::new(MessageType::Response, src, dst)
            .with_speed(0.25);

        // After first update: 0.25
        let completed = flow.update();
        assert!(!completed);
        assert!((flow.progress() - 0.25).abs() < f32::EPSILON);

        // After second update: 0.50
        flow.update();
        assert!((flow.progress() - 0.50).abs() < f32::EPSILON);

        // After third update: 0.75
        flow.update();
        assert!((flow.progress() - 0.75).abs() < f32::EPSILON);

        // After fourth update: 1.0 (complete)
        let completed = flow.update();
        assert!(completed);
        assert!(flow.is_complete());
        assert!((flow.progress() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_message_flow_no_update_after_complete() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 0.0);
        let mut flow = MessageFlow::new(MessageType::Broadcast, src, dst)
            .with_speed(1.0);

        let completed = flow.update();
        assert!(completed);
        assert!(flow.is_complete());

        // Subsequent updates should return false
        let completed_again = flow.update();
        assert!(!completed_again);
    }

    #[test]
    fn test_message_flow_current_position_start() {
        let src = Point::new(0.2, 0.3);
        let dst = Point::new(0.8, 0.9);
        let flow = MessageFlow::new(MessageType::Consensus, src, dst);

        let pos = flow.current_position();
        assert!((pos.x - 0.2).abs() < f32::EPSILON);
        assert!((pos.y - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_message_flow_current_position_midway() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 1.0);
        let mut flow = MessageFlow::new(MessageType::Request, src, dst)
            .with_speed(0.5);

        flow.update();
        let pos = flow.current_position();
        assert!((pos.x - 0.5).abs() < f32::EPSILON);
        assert!((pos.y - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_message_flow_current_position_end() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 1.0);
        let mut flow = MessageFlow::new(MessageType::Request, src, dst)
            .with_speed(1.0);

        flow.update();
        let pos = flow.current_position();
        assert!((pos.x - 1.0).abs() < f32::EPSILON);
        assert!((pos.y - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_point_creation() {
        let p = Point::new(0.5, 0.75);
        assert!((p.x - 0.5).abs() < f32::EPSILON);
        assert!((p.y - 0.75).abs() < f32::EPSILON);
    }

    #[test]
    fn test_message_flow_with_speed() {
        let src = Point::new(0.0, 0.0);
        let dst = Point::new(1.0, 0.0);
        let flow = MessageFlow::new(MessageType::Request, src, dst)
            .with_speed(0.1);

        // Speed should be accepted
        let mut f = flow;
        f.update();
        assert!((f.progress() - 0.1).abs() < f32::EPSILON);
    }
}
