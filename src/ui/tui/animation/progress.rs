//! Animated Progress Bar Widget
//!
//! Features:
//! - Wave animation effect
//! - Gradient colors
//! - Smooth transitions

use super::{colors, Animation};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use std::time::{Duration, Instant};

/// Animated progress bar with wave effects
pub struct AnimatedProgressBar {
    /// Current progress (0.0 to 1.0)
    progress: f32,
    /// Target progress for smooth transitions
    target_progress: f32,
    /// Enable wave animation
    animated: bool,
    /// Last animation update time
    last_update: Instant,
    /// Current animation frame
    animation_frame: u8,
    /// Label to display
    label: Option<String>,
    /// Show percentage
    show_percentage: bool,
}

impl AnimatedProgressBar {
    pub fn new(progress: f32) -> Self {
        Self {
            progress: progress.clamp(0.0, 1.0),
            target_progress: progress.clamp(0.0, 1.0),
            animated: true,
            last_update: Instant::now(),
            animation_frame: 0,
            label: None,
            show_percentage: true,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn show_percentage(mut self, show: bool) -> Self {
        self.show_percentage = show;
        self
    }

    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }

    pub fn set_progress(&mut self, progress: f32) {
        self.target_progress = progress.clamp(0.0, 1.0);
    }

    pub fn progress(&self) -> f32 {
        self.progress
    }

    pub fn update(&mut self, delta_time: f32) {
        // Smooth transition to target
        let diff = self.target_progress - self.progress;
        if diff.abs() > 0.001 {
            self.progress += diff * delta_time * 5.0;
            self.progress = self.progress.clamp(0.0, 1.0);
        } else {
            self.progress = self.target_progress;
        }

        // Update animation frame
        if self.last_update.elapsed() > Duration::from_millis(100) {
            self.animation_frame = (self.animation_frame + 1) % 8;
            self.last_update = Instant::now();
        }
    }

    fn get_wave_char(&self, x: u16, filled: bool) -> char {
        if !filled {
            return '░';
        }

        if !self.animated {
            return '▓';
        }

        // Wave effect
        match (x as u8 + self.animation_frame) % 4 {
            0 => '█',
            1 => '▓',
            2 => '▒',
            _ => '░',
        }
    }

    fn get_gradient_color(&self, x: u16, width: u16) -> Color {
        let gradient = &colors::GRADIENT;
        let idx = (x as usize * gradient.len()) / width as usize;
        gradient[idx.min(gradient.len() - 1)]
    }
}

impl Animation for AnimatedProgressBar {
    fn update(&mut self, delta_time: f32) {
        AnimatedProgressBar::update(self, delta_time);
    }

    fn is_complete(&self) -> bool {
        // Progress bars don't complete, they're continuous
        false
    }
}

impl Widget for &AnimatedProgressBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 3 || area.height < 1 {
            return;
        }

        // Calculate label and percentage space
        let label_len = self.label.as_ref().map(|l| l.len() + 1).unwrap_or(0) as u16;
        let pct_len = if self.show_percentage { 5 } else { 0 }; // " 100%"
        let bar_width = area.width.saturating_sub(label_len + pct_len);

        if bar_width < 3 {
            return;
        }

        let filled = ((bar_width as f32 * self.progress) as u16).min(bar_width);
        let bar_start = area.x + label_len;

        // Render label
        if let Some(label) = &self.label {
            for (i, ch) in label.chars().enumerate() {
                if i as u16 >= label_len {
                    break;
                }
                buf[(area.x + i as u16, area.y)]
                    .set_symbol(&ch.to_string())
                    .set_style(Style::default().fg(Color::White));
            }
        }

        // Render progress bar
        for x in 0..bar_width {
            let is_filled = x < filled;
            let symbol = self.get_wave_char(x, is_filled);
            let color = if is_filled {
                self.get_gradient_color(x, bar_width)
            } else {
                Color::DarkGray
            };

            buf[(bar_start + x, area.y)]
                .set_symbol(&symbol.to_string())
                .set_style(Style::default().fg(color));
        }

        // Render percentage
        if self.show_percentage {
            let pct = format!("{:3}%", (self.progress * 100.0) as u8);
            let pct_start = bar_start + bar_width + 1;
            for (i, ch) in pct.chars().enumerate() {
                if pct_start + (i as u16) < area.x + area.width {
                    buf[(pct_start + i as u16, area.y)]
                        .set_symbol(&ch.to_string())
                        .set_style(Style::default().fg(Color::Gray));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/ui/tui/animation/progress/progress_test.rs"]
mod tests;
