//! Custom TUI Widgets for Selfware
//!
//! Reusable components with the organic selfware aesthetic.

// Feature-gated module - dead_code lint disabled at crate level

use super::TuiPalette;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Widget},
    Frame,
};

/// A spinner widget showing progress with garden metaphors
pub struct GardenSpinner {
    /// Current frame (0-3)
    frame: usize,
    /// Message to display
    message: String,
}

impl GardenSpinner {
    /// Create a new spinner
    pub fn new(message: &str) -> Self {
        Self {
            frame: 0,
            message: message.into(),
        }
    }

    /// Advance to next frame
    pub fn tick(&mut self) {
        self.frame = (self.frame + 1) % 4;
    }

    /// Get the current spinner character
    fn spinner_char(&self) -> &'static str {
        match self.frame {
            0 => "🌱",
            1 => "🌿",
            2 => "🍃",
            _ => "🌳",
        }
    }
}

impl Widget for GardenSpinner {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let text = format!("{} {}", self.spinner_char(), self.message);
        let paragraph = Paragraph::new(text).style(Style::default().fg(TuiPalette::AMBER));
        paragraph.render(area, buf);
    }
}

/// A progress bar with garden theme
pub struct GrowthGauge {
    /// Progress ratio (0.0 to 1.0)
    ratio: f64,
    /// Label
    label: String,
}

impl GrowthGauge {
    /// Create a new gauge
    pub fn new(ratio: f64, label: &str) -> Self {
        Self {
            ratio: ratio.clamp(0.0, 1.0),
            label: label.into(),
        }
    }

    /// Get growth stage based on progress
    fn growth_stage(&self) -> &'static str {
        match (self.ratio * 100.0) as u8 {
            0..=25 => "Seedling",
            26..=50 => "Sprouting",
            51..=75 => "Growing",
            76..=99 => "Flourishing",
            _ => "Mature",
        }
    }

    /// Get the bar characters
    #[allow(dead_code)]
    fn bar_chars(&self, width: usize) -> String {
        let filled = ((self.ratio * width as f64) as usize).min(width);
        let empty = width - filled;

        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for GrowthGauge {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let gauge = Gauge::default()
            .block(Block::default())
            .gauge_style(Style::default().fg(TuiPalette::GARDEN_GREEN))
            .ratio(self.ratio)
            .label(format!(
                "{} {} ({:.0}%)",
                self.growth_stage(),
                self.label,
                self.ratio * 100.0
            ));
        gauge.render(area, buf);
    }
}

/// A status indicator
pub struct StatusIndicator {
    /// Status type
    status: StatusType,
    /// Label
    label: String,
}

/// Types of status
#[derive(Debug, Clone, Copy)]
pub enum StatusType {
    Success,
    Warning,
    Error,
    Info,
    Loading,
}

impl StatusIndicator {
    /// Create a new status indicator
    pub fn new(status: StatusType, label: &str) -> Self {
        Self {
            status,
            label: label.into(),
        }
    }

    /// Get icon for status
    fn icon(&self) -> &'static str {
        match self.status {
            StatusType::Success => "✿",
            StatusType::Warning => "🥀",
            StatusType::Error => "❄️",
            StatusType::Info => "📋",
            StatusType::Loading => "⏳",
        }
    }

    /// Get style for status
    fn style(&self) -> Style {
        match self.status {
            StatusType::Success => TuiPalette::success_style(),
            StatusType::Warning => TuiPalette::warning_style(),
            StatusType::Error => TuiPalette::error_style(),
            StatusType::Info => TuiPalette::muted_style(),
            StatusType::Loading => Style::default().fg(TuiPalette::AMBER),
        }
    }
}

impl Widget for StatusIndicator {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let text = format!("{} {}", self.icon(), self.label);
        let paragraph = Paragraph::new(text).style(self.style());
        paragraph.render(area, buf);
    }
}

/// A tool output panel
pub struct ToolOutput {
    /// Tool name
    tool_name: String,
    /// Output content
    output: String,
    /// Whether it succeeded
    success: bool,
}

impl ToolOutput {
    /// Create a new tool output panel
    pub fn new(tool_name: &str, output: &str, success: bool) -> Self {
        Self {
            tool_name: tool_name.into(),
            output: output.into(),
            success,
        }
    }
}

impl Widget for ToolOutput {
    fn render(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let status_icon = if self.success { "✓" } else { "✗" };
        let status_style = if self.success {
            TuiPalette::success_style()
        } else {
            TuiPalette::error_style()
        };

        let title = Line::from(vec![
            Span::styled(format!("{} ", status_icon), status_style),
            Span::styled(
                &self.tool_name,
                Style::default()
                    .fg(TuiPalette::COPPER)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(TuiPalette::border_style())
            .title(title);

        let inner = block.inner(area);
        block.render(area, buf);

        let output = Paragraph::new(self.output.as_str()).style(TuiPalette::muted_style());
        output.render(inner, buf);
    }
}



#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/widgets/widgets_test.rs"]
mod tests;
