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

/// Render a keyboard shortcut hint
#[allow(dead_code)]
pub fn render_shortcut(frame: &mut Frame, area: Rect, key: &str, action: &str) {
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", key),
            Style::default()
                .fg(TuiPalette::INK)
                .bg(TuiPalette::SAGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(action, TuiPalette::muted_style()),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render a help bar at the bottom
#[allow(dead_code)]
pub fn render_help_bar(frame: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, action)| {
            vec![
                Span::styled(
                    format!(" {} ", key),
                    Style::default().fg(TuiPalette::INK).bg(TuiPalette::SAGE),
                ),
                Span::styled(format!(" {} ", action), TuiPalette::muted_style()),
                Span::raw("  "),
            ]
        })
        .collect();

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_tick() {
        let mut spinner = GardenSpinner::new("Loading");
        assert_eq!(spinner.frame, 0);

        spinner.tick();
        assert_eq!(spinner.frame, 1);

        spinner.tick();
        spinner.tick();
        spinner.tick();
        assert_eq!(spinner.frame, 0); // Wrapped around
    }

    #[test]
    fn test_growth_gauge_stages() {
        assert_eq!(GrowthGauge::new(0.1, "").growth_stage(), "Seedling");
        assert_eq!(GrowthGauge::new(0.3, "").growth_stage(), "Sprouting");
        assert_eq!(GrowthGauge::new(0.6, "").growth_stage(), "Growing");
        assert_eq!(GrowthGauge::new(0.9, "").growth_stage(), "Flourishing");
        assert_eq!(GrowthGauge::new(1.0, "").growth_stage(), "Mature");
    }

    #[test]
    fn test_status_indicator_icons() {
        assert_eq!(StatusIndicator::new(StatusType::Success, "").icon(), "✿");
        assert_eq!(StatusIndicator::new(StatusType::Warning, "").icon(), "🥀");
        assert_eq!(StatusIndicator::new(StatusType::Error, "").icon(), "❄️");
    }

    #[test]
    fn test_growth_gauge_clamp() {
        let g1 = GrowthGauge::new(1.5, "test");
        assert_eq!(g1.ratio, 1.0);

        let g2 = GrowthGauge::new(-0.5, "test");
        assert_eq!(g2.ratio, 0.0);
    }

    #[test]
    fn test_spinner_creation() {
        let spinner = GardenSpinner::new("Testing");
        assert_eq!(spinner.frame, 0);
        assert_eq!(spinner.message, "Testing");
    }

    #[test]
    fn test_spinner_chars_all_frames() {
        let mut spinner = GardenSpinner::new("test");
        assert_eq!(spinner.spinner_char(), "🌱");

        spinner.tick();
        assert_eq!(spinner.spinner_char(), "🌿");

        spinner.tick();
        assert_eq!(spinner.spinner_char(), "🍃");

        spinner.tick();
        assert_eq!(spinner.spinner_char(), "🌳");

        spinner.tick();
        assert_eq!(spinner.spinner_char(), "🌱"); // Wrapped
    }

    #[test]
    fn test_growth_gauge_all_stages() {
        // Test boundary values
        assert_eq!(GrowthGauge::new(0.0, "").growth_stage(), "Seedling");
        assert_eq!(GrowthGauge::new(0.25, "").growth_stage(), "Seedling");
        assert_eq!(GrowthGauge::new(0.26, "").growth_stage(), "Sprouting");
        assert_eq!(GrowthGauge::new(0.50, "").growth_stage(), "Sprouting");
        assert_eq!(GrowthGauge::new(0.51, "").growth_stage(), "Growing");
        assert_eq!(GrowthGauge::new(0.75, "").growth_stage(), "Growing");
        assert_eq!(GrowthGauge::new(0.76, "").growth_stage(), "Flourishing");
        assert_eq!(GrowthGauge::new(0.99, "").growth_stage(), "Flourishing");
        assert_eq!(GrowthGauge::new(1.0, "").growth_stage(), "Mature");
    }

    #[test]
    fn test_growth_gauge_bar_chars() {
        let gauge = GrowthGauge::new(0.5, "test");
        let bar = gauge.bar_chars(10);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 5);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 5);
    }

    #[test]
    fn test_growth_gauge_bar_chars_full() {
        let gauge = GrowthGauge::new(1.0, "test");
        let bar = gauge.bar_chars(10);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 10);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 0);
    }

    #[test]
    fn test_growth_gauge_bar_chars_empty() {
        let gauge = GrowthGauge::new(0.0, "test");
        let bar = gauge.bar_chars(10);
        assert_eq!(bar.chars().filter(|&c| c == '█').count(), 0);
        assert_eq!(bar.chars().filter(|&c| c == '░').count(), 10);
    }

    #[test]
    fn test_status_indicator_all_icons() {
        assert_eq!(StatusIndicator::new(StatusType::Info, "").icon(), "📋");
        assert_eq!(StatusIndicator::new(StatusType::Loading, "").icon(), "⏳");
    }

    #[test]
    fn test_status_indicator_creation() {
        let indicator = StatusIndicator::new(StatusType::Success, "All good");
        assert_eq!(indicator.label, "All good");
    }

    #[test]
    fn test_status_type_debug() {
        let status = StatusType::Success;
        let debug_str = format!("{:?}", status);
        assert_eq!(debug_str, "Success");
    }

    #[test]
    fn test_tool_output_creation() {
        let output = ToolOutput::new("my_tool", "output data", true);
        assert_eq!(output.tool_name, "my_tool");
        assert_eq!(output.output, "output data");
        assert!(output.success);
    }

    #[test]
    fn test_tool_output_failure() {
        let output = ToolOutput::new("failing_tool", "error message", false);
        assert!(!output.success);
    }

    #[test]
    fn test_growth_gauge_label() {
        let gauge = GrowthGauge::new(0.5, "my_label");
        assert_eq!(gauge.label, "my_label");
        assert_eq!(gauge.ratio, 0.5);
    }

    #[test]
    fn test_spinner_multiple_ticks() {
        let mut spinner = GardenSpinner::new("test");
        for _ in 0..100 {
            spinner.tick();
        }
        // Should be at frame 0 (100 % 4 == 0)
        assert_eq!(spinner.frame, 0);
    }

    #[test]
    fn test_status_indicator_style_returns_style() {
        // Just ensure style() doesn't panic for any variant
        let _ = StatusIndicator::new(StatusType::Success, "").style();
        let _ = StatusIndicator::new(StatusType::Warning, "").style();
        let _ = StatusIndicator::new(StatusType::Error, "").style();
        let _ = StatusIndicator::new(StatusType::Info, "").style();
        let _ = StatusIndicator::new(StatusType::Loading, "").style();
    }

    #[test]
    fn test_status_type_clone() {
        let status = StatusType::Warning;
        let cloned = status;
        assert!(matches!(cloned, StatusType::Warning));
    }

    #[test]
    fn test_growth_gauge_extreme_values() {
        // Values should be clamped
        let g1 = GrowthGauge::new(100.0, "test");
        assert_eq!(g1.ratio, 1.0);

        let g2 = GrowthGauge::new(-100.0, "test");
        assert_eq!(g2.ratio, 0.0);

        let g3 = GrowthGauge::new(f64::INFINITY, "test");
        assert_eq!(g3.ratio, 1.0);
    }
}
