//! Intelligent status line for the Selfware TUI
//!
//! Displays real-time session information: model, execution mode,
//! token usage, context window percentage, and session identity.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use unicode_width::UnicodeWidthStr;

use super::TuiPalette;

/// Execution mode for the status line
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StatusMode {
    #[default]
    Normal,
    Plan,
    Auto,
    Yolo,
}

impl StatusMode {
    /// Label shown in the status line
    pub fn label(&self) -> &'static str {
        match self {
            StatusMode::Normal => "NORMAL",
            StatusMode::Plan => "PLAN",
            StatusMode::Auto => "AUTO",
            StatusMode::Yolo => "YOLO",
        }
    }

    /// Color style for the mode badge
    pub fn style(&self) -> Style {
        match self {
            StatusMode::Normal => Style::default().fg(TuiPalette::SAGE),
            StatusMode::Plan => Style::default()
                .fg(TuiPalette::AMBER)
                .add_modifier(Modifier::BOLD),
            StatusMode::Auto => Style::default()
                .fg(TuiPalette::COPPER)
                .add_modifier(Modifier::BOLD),
            StatusMode::Yolo => Style::default()
                .fg(TuiPalette::error())
                .add_modifier(Modifier::BOLD),
        }
    }
}

impl From<crate::config::ExecutionMode> for StatusMode {
    fn from(mode: crate::config::ExecutionMode) -> Self {
        match mode {
            crate::config::ExecutionMode::Normal => StatusMode::Normal,
            crate::config::ExecutionMode::AutoEdit => StatusMode::Auto,
            crate::config::ExecutionMode::Yolo => StatusMode::Yolo,
            crate::config::ExecutionMode::Daemon => StatusMode::Yolo,
        }
    }
}

/// Real-time status line data for the TUI
#[derive(Debug, Clone, PartialEq)]
pub struct StatusLine {
    /// Current model name
    pub model: String,
    /// Token usage as (input_tokens, output_tokens)
    pub tokens_used: (usize, usize),
    /// Context window percentage used (0.0 - 100.0)
    pub context_percent: f32,
    /// Execution / permission mode
    pub mode: StatusMode,
    /// Session name or identifier
    pub session_id: String,
    /// Connection state
    pub connected: bool,
    /// Optional extra status message
    pub status_message: Option<String>,
}

impl Default for StatusLine {
    fn default() -> Self {
        Self {
            model: "Unknown".into(),
            tokens_used: (0, 0),
            context_percent: 0.0,
            mode: StatusMode::Normal,
            session_id: "-".into(),
            connected: true,
            status_message: None,
        }
    }
}

impl StatusLine {
    /// Create a new status line for the given model
    pub fn new(model: &str) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Create a new status line with a generated session id
    pub fn with_session(model: &str) -> Self {
        let session_id = format!("{:.8}", uuid::Uuid::new_v4());
        Self {
            model: model.into(),
            session_id,
            ..Default::default()
        }
    }

    /// Total tokens (input + output)
    pub fn total_tokens(&self) -> usize {
        self.tokens_used.0 + self.tokens_used.1
    }

    /// Reset per-conversation usage counters. Called when the conversation is
    /// cleared (`/clear`) so the token totals and context gauge don't keep
    /// reporting the now-discarded conversation. Session identity and model are
    /// preserved — the TUI session itself continues.
    pub fn reset_usage(&mut self) {
        self.tokens_used = (0, 0);
        self.context_percent = 0.0;
    }

    /// Format tokens with K/M suffix for large numbers
    fn fmt_tokens(n: usize) -> String {
        if n >= 1_000_000 {
            format!("{:.1}M", n as f64 / 1_000_000.0)
        } else if n >= 1_000 {
            format!("{:.1}K", n as f64 / 1_000.0)
        } else {
            n.to_string()
        }
    }

    /// Color for the context percentage based on usage
    fn context_style(&self) -> Style {
        if self.context_percent >= 90.0 {
            TuiPalette::error_style()
        } else if self.context_percent >= 75.0 {
            TuiPalette::warning_style()
        } else {
            TuiPalette::success_style()
        }
    }

    /// Render the status line into the given area
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let connection_icon = if self.connected { "●" } else { "○" };
        let connection_style = if self.connected {
            TuiPalette::success_style()
        } else {
            TuiPalette::error_style()
        };

        let total = Self::fmt_tokens(self.total_tokens());
        let input = Self::fmt_tokens(self.tokens_used.0);
        let output = Self::fmt_tokens(self.tokens_used.1);

        // Left side: connection, model, mode
        let left = vec![
            Span::styled(format!(" {} ", connection_icon), connection_style),
            Span::styled(
                format!("{} ", self.model),
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("[", TuiPalette::muted_style()),
            Span::styled(self.mode.label(), self.mode.style()),
            Span::styled("] ", TuiPalette::muted_style()),
        ];

        // Center: token usage + context window
        let center = vec![
            Span::styled("Tokens: ", TuiPalette::muted_style()),
            Span::styled(
                format!("{} ", total),
                Style::default().fg(TuiPalette::COPPER),
            ),
            Span::styled(
                format!("(in:{} out:{}) ", input, output),
                TuiPalette::muted_style(),
            ),
            Span::styled("Ctx: ", TuiPalette::muted_style()),
            Span::styled(
                format!("{:.0}%", self.context_percent),
                self.context_style(),
            ),
        ];

        // Right side: session id + optional status
        let mut right = vec![
            Span::styled("Session: ", TuiPalette::muted_style()),
            Span::styled(&self.session_id, Style::default().fg(TuiPalette::SAGE)),
        ];

        if let Some(ref msg) = self.status_message {
            right.push(Span::styled(" | ", TuiPalette::muted_style()));
            right.push(Span::styled(msg, TuiPalette::muted_style()));
        }

        // Measure approximate widths to place center
        let left_text: String = left.iter().map(|s| s.content.clone()).collect();
        let right_text: String = right.iter().map(|s| s.content.clone()).collect();
        let center_text: String = center.iter().map(|s| s.content.clone()).collect();

        let left_width = UnicodeWidthStr::width(left_text.as_str()) as u16;
        let right_width = UnicodeWidthStr::width(right_text.as_str()) as u16;
        let center_width = UnicodeWidthStr::width(center_text.as_str()) as u16;

        let available = area.width.saturating_sub(left_width + right_width);
        let padding = available.saturating_sub(center_width) / 2;

        let mut spans = left;
        spans.push(Span::raw(" ".repeat(padding as usize)));
        spans.extend(center);

        let right_padding = area
            .width
            .saturating_sub(left_width + padding + center_width + right_width);
        spans.push(Span::raw(" ".repeat(right_padding as usize)));
        spans.extend(right);

        let paragraph = Paragraph::new(Line::from(spans));
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_line_default() {
        let sl = StatusLine::default();
        assert_eq!(sl.model, "Unknown");
        assert_eq!(sl.tokens_used, (0, 0));
        assert_eq!(sl.context_percent, 0.0);
        assert_eq!(sl.mode, StatusMode::Normal);
        assert!(sl.connected);
    }

    #[test]
    fn test_status_line_new() {
        let sl = StatusLine::new("qwen3-5-27b");
        assert_eq!(sl.model, "qwen3-5-27b");
        assert_eq!(sl.total_tokens(), 0);
    }

    #[test]
    fn test_total_tokens() {
        let sl = StatusLine {
            tokens_used: (150, 75),
            ..Default::default()
        };
        assert_eq!(sl.total_tokens(), 225);
    }

    #[test]
    fn test_fmt_tokens() {
        assert_eq!(StatusLine::fmt_tokens(500), "500");
        assert_eq!(StatusLine::fmt_tokens(1500), "1.5K");
        assert_eq!(StatusLine::fmt_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_mode_labels() {
        assert_eq!(StatusMode::Normal.label(), "NORMAL");
        assert_eq!(StatusMode::Plan.label(), "PLAN");
        assert_eq!(StatusMode::Auto.label(), "AUTO");
        assert_eq!(StatusMode::Yolo.label(), "YOLO");
    }

    #[test]
    fn test_context_style_thresholds() {
        let low = StatusLine {
            context_percent: 50.0,
            ..Default::default()
        };
        let mid = StatusLine {
            context_percent: 80.0,
            ..Default::default()
        };
        let high = StatusLine {
            context_percent: 95.0,
            ..Default::default()
        };
        // Just ensure they don't panic
        let _ = low.context_style();
        let _ = mid.context_style();
        let _ = high.context_style();
    }
}
