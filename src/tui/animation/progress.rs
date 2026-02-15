//! Animated Progress Bar Widget
//!
//! A progress bar with gradient coloring and a wave animation effect.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::time::Instant;

/// An animated progress bar with gradient coloring and wave effect.
#[derive(Debug, Clone)]
pub struct AnimatedProgressBar {
    /// Current progress value (0.0 to 1.0)
    progress: f32,
    /// Whether animation is enabled
    animated: bool,
    /// Current animation frame counter (wraps at 255)
    animation_frame: u8,
    /// Timestamp of last update
    last_update: Instant,
}

impl AnimatedProgressBar {
    /// Create a new animated progress bar.
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            animated: true,
            animation_frame: 0,
            last_update: Instant::now(),
        }
    }

    /// Set whether the progress bar is animated.
    pub fn with_animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    /// Get the current progress value.
    pub fn progress(&self) -> f32 {
        self.progress
    }

    /// Set the progress value (clamped to 0.0..=1.0).
    pub fn set_progress(&mut self, progress: f32) {
        self.progress = progress.clamp(0.0, 1.0);
    }

    /// Whether animation is enabled.
    pub fn is_animated(&self) -> bool {
        self.animated
    }

    /// Get the current animation frame.
    pub fn animation_frame(&self) -> u8 {
        self.animation_frame
    }

    /// Get the last update timestamp.
    pub fn last_update(&self) -> Instant {
        self.last_update
    }

    /// Advance the animation by one frame and record the update time.
    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);
        self.last_update = Instant::now();
    }

    /// Compute the gradient color for a given position within the bar.
    ///
    /// The gradient transitions from amber through green to a bright teal,
    /// with a wave offset when animation is enabled.
    fn gradient_color(&self, position: f32) -> Color {
        let wave_offset = if self.animated {
            ((self.animation_frame as f32 / 10.0) + position * 6.0).sin() * 0.15
        } else {
            0.0
        };

        let t = (position + wave_offset).clamp(0.0, 1.0);

        // Amber (212,163,115) -> Green (96,108,56) -> Teal (64,224,208)
        let (r, g, b) = if t < 0.5 {
            let s = t * 2.0;
            (
                212.0 - (212.0 - 96.0) * s,
                163.0 - (163.0 - 108.0) * s,
                115.0 - (115.0 - 56.0) * s,
            )
        } else {
            let s = (t - 0.5) * 2.0;
            (
                96.0 - (96.0 - 64.0) * s,
                108.0 + (224.0 - 108.0) * s,
                56.0 + (208.0 - 56.0) * s,
            )
        };

        Color::Rgb(r as u8, g as u8, b as u8)
    }
}

impl Widget for AnimatedProgressBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let filled_width = ((self.progress * area.width as f32) as u16).min(area.width);

        // Render the filled portion with gradient colors
        for x in 0..filled_width {
            let px = area.x + x;
            let py = area.y;
            if px < area.right() && py < area.bottom() {
                let position = x as f32 / area.width as f32;
                let color = self.gradient_color(position);
                let cell = buf.get_mut(px, py);
                cell.set_symbol("\u{2588}"); // Full block character
                cell.set_style(Style::default().fg(color));
            }
        }

        // Render the unfilled portion
        for x in filled_width..area.width {
            let px = area.x + x;
            let py = area.y;
            if px < area.right() && py < area.bottom() {
                let cell = buf.get_mut(px, py);
                cell.set_symbol("\u{2591}"); // Light shade character
                cell.set_style(Style::default().fg(Color::DarkGray));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_creation() {
        let bar = AnimatedProgressBar::new(0.5);
        assert!((bar.progress() - 0.5).abs() < f32::EPSILON);
        assert!(bar.is_animated());
        assert_eq!(bar.animation_frame(), 0);
    }

    #[test]
    fn test_progress_bar_update() {
        let mut bar = AnimatedProgressBar::new(0.0);
        let initial_time = bar.last_update();

        // Small sleep so that timestamp differs
        std::thread::sleep(std::time::Duration::from_millis(1));

        bar.set_progress(0.75);
        bar.tick();

        assert!((bar.progress() - 0.75).abs() < f32::EPSILON);
        assert_eq!(bar.animation_frame(), 1);
        assert!(bar.last_update() >= initial_time);
    }

    #[test]
    fn test_progress_bar_clamping() {
        let bar_over = AnimatedProgressBar::new(1.5);
        assert!((bar_over.progress() - 1.0).abs() < f32::EPSILON);

        let bar_under = AnimatedProgressBar::new(-0.5);
        assert!((bar_under.progress() - 0.0).abs() < f32::EPSILON);

        let mut bar = AnimatedProgressBar::new(0.5);
        bar.set_progress(2.0);
        assert!((bar.progress() - 1.0).abs() < f32::EPSILON);

        bar.set_progress(-1.0);
        assert!((bar.progress() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_progress_bar_with_animated() {
        let bar = AnimatedProgressBar::new(0.5).with_animated(false);
        assert!(!bar.is_animated());
    }

    #[test]
    fn test_progress_bar_tick_wraps() {
        let mut bar = AnimatedProgressBar::new(0.5);
        for _ in 0..256 {
            bar.tick();
        }
        // After 256 ticks, u8 wraps back to 0
        assert_eq!(bar.animation_frame(), 0);
    }

    #[test]
    fn test_gradient_color_returns_valid_color() {
        let bar = AnimatedProgressBar::new(0.5);
        let color = bar.gradient_color(0.0);
        // Should be near amber at position 0
        match color {
            Color::Rgb(r, g, _b) => {
                assert!(r > 150); // close to 212
                assert!(g > 100); // close to 163
            }
            _ => panic!("Expected Rgb color"),
        }
    }
}
