//! Dashboard Widgets for Selfware TUI
//!
//! Specialized widgets for the dashboard layout including status bar,
//! garden health, active tools, and log display.

use super::TuiPalette;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
    Frame,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Events sent from the agent to update the TUI dashboard
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// Agent started processing
    AgentStarted,
    /// Agent completed successfully
    AgentCompleted { message: String },
    /// Agent encountered an error
    AgentError { message: String },
    /// Tool execution started
    ToolStarted { name: String },
    /// Tool execution completed
    ToolCompleted {
        name: String,
        success: bool,
        duration_ms: u64,
    },
    /// Token usage update
    TokenUsage {
        prompt_tokens: u64,
        completion_tokens: u64,
    },
    /// Status message update
    StatusUpdate { message: String },
    /// Garden health update (from code analysis or other metrics)
    GardenHealthUpdate { health: f64 },
    /// Log message
    Log { level: LogLevel, message: String },
}

/// Dashboard state containing all widget data
#[derive(Debug, Clone)]
pub struct DashboardState {
    /// Model name being used
    pub model: String,
    /// Total tokens used in session
    pub tokens_used: u64,
    /// Session start time
    pub session_start: Instant,
    /// Garden health percentage (0.0 - 1.0)
    pub garden_health: f64,
    /// Active tools currently running
    pub active_tools: Vec<ActiveTool>,
    /// Recent log entries
    pub logs: Vec<LogEntry>,
    /// Whether the agent is connected
    pub connected: bool,
    /// Current status message
    pub status_message: String,
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            model: "Unknown".to_string(),
            tokens_used: 0,
            session_start: Instant::now(),
            garden_health: 1.0,
            active_tools: Vec::new(),
            logs: Vec::new(),
            connected: true,
            status_message: "Ready".to_string(),
        }
    }
}

impl DashboardState {
    /// Create a new dashboard state with the given model
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            ..Default::default()
        }
    }

    /// Get elapsed session time
    pub fn elapsed(&self) -> Duration {
        self.session_start.elapsed()
    }

    /// Format elapsed time as HH:MM:SS
    pub fn elapsed_formatted(&self) -> String {
        let secs = self.elapsed().as_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;
        format!("{:02}:{:02}:{:02}", hours, mins, secs)
    }

    /// Add a log entry
    pub fn log(&mut self, level: LogLevel, message: &str) {
        self.logs.push(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.to_string(),
        });
        // Keep only last 100 logs
        if self.logs.len() > 100 {
            self.logs.remove(0);
        }
    }

    /// Start tracking an active tool
    pub fn tool_start(&mut self, name: &str) {
        self.active_tools.push(ActiveTool {
            name: name.to_string(),
            progress: 0.0,
            started: Instant::now(),
        });
    }

    /// Update tool progress
    pub fn tool_progress(&mut self, name: &str, progress: f64) {
        if let Some(tool) = self.active_tools.iter_mut().find(|t| t.name == name) {
            tool.progress = progress.clamp(0.0, 1.0);
        }
    }

    /// Complete and remove a tool
    pub fn tool_complete(&mut self, name: &str) {
        self.active_tools.retain(|t| t.name != name);
    }

    /// Process a TUI event and update state accordingly
    pub fn process_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::AgentStarted => {
                self.status_message = "Agent working...".to_string();
                self.log(LogLevel::Info, "Agent started processing");
            }
            TuiEvent::AgentCompleted { message } => {
                self.status_message = "Ready".to_string();
                self.log(LogLevel::Success, &format!("Completed: {}", message));
            }
            TuiEvent::AgentError { message } => {
                self.status_message = format!("Error: {}", &message[..message.len().min(30)]);
                self.log(LogLevel::Error, &message);
            }
            TuiEvent::ToolStarted { name } => {
                self.tool_start(&name);
                self.status_message = format!("Running: {}", name);
            }
            TuiEvent::ToolCompleted {
                name,
                success,
                duration_ms,
            } => {
                self.tool_complete(&name);
                if success {
                    self.log(
                        LogLevel::Success,
                        &format!("{} completed ({}ms)", name, duration_ms),
                    );
                } else {
                    self.log(
                        LogLevel::Warning,
                        &format!("{} failed ({}ms)", name, duration_ms),
                    );
                }
            }
            TuiEvent::TokenUsage {
                prompt_tokens,
                completion_tokens,
            } => {
                self.tokens_used += completion_tokens;
                self.log(
                    LogLevel::Debug,
                    &format!("+{} tokens (prompt: {})", completion_tokens, prompt_tokens),
                );
            }
            TuiEvent::StatusUpdate { message } => {
                self.status_message = message.clone();
                self.log(LogLevel::Info, &message);
            }
            TuiEvent::GardenHealthUpdate { health } => {
                self.garden_health = health.clamp(0.0, 1.0);
            }
            TuiEvent::Log { level, message } => {
                self.log(level, &message);
            }
        }
    }
}

/// Thread-safe wrapper for DashboardState
pub type SharedDashboardState = Arc<Mutex<DashboardState>>;

/// An active tool being tracked
#[derive(Debug, Clone)]
pub struct ActiveTool {
    /// Tool name
    pub name: String,
    /// Progress (0.0 - 1.0)
    pub progress: f64,
    /// When the tool started
    pub started: Instant,
}

impl ActiveTool {
    /// Get elapsed time for this tool
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

/// Log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Timestamp string
    pub timestamp: String,
    /// Log level
    pub level: LogLevel,
    /// Message
    pub message: String,
}

/// Log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

impl LogLevel {
    /// Get icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            LogLevel::Info => "ℹ",
            LogLevel::Success => "✓",
            LogLevel::Warning => "⚠",
            LogLevel::Error => "✗",
            LogLevel::Debug => "◇",
        }
    }

    /// Get style for this level
    pub fn style(&self) -> Style {
        match self {
            LogLevel::Info => TuiPalette::muted_style(),
            LogLevel::Success => TuiPalette::success_style(),
            LogLevel::Warning => TuiPalette::warning_style(),
            LogLevel::Error => TuiPalette::error_style(),
            LogLevel::Debug => Style::default().fg(TuiPalette::SAGE),
        }
    }
}

/// Render the status bar widget
pub fn render_status_bar(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let connection_icon = if state.connected { "●" } else { "○" };
    let connection_style = if state.connected {
        TuiPalette::success_style()
    } else {
        TuiPalette::error_style()
    };

    // Format tokens with K suffix for large numbers
    let tokens_display = if state.tokens_used >= 1000 {
        format!("{}K", state.tokens_used / 1000)
    } else {
        state.tokens_used.to_string()
    };

    let spans = vec![
        Span::styled(format!(" {} ", connection_icon), connection_style),
        Span::styled(
            format!("{} ", state.model),
            Style::default()
                .fg(TuiPalette::AMBER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", TuiPalette::muted_style()),
        Span::styled("Tokens: ", TuiPalette::muted_style()),
        Span::styled(tokens_display, Style::default().fg(TuiPalette::COPPER)),
        Span::styled(" │ ", TuiPalette::muted_style()),
        Span::styled("⏱ ", TuiPalette::muted_style()),
        Span::styled(
            state.elapsed_formatted(),
            Style::default().fg(TuiPalette::SAGE),
        ),
        Span::styled(" │ ", TuiPalette::muted_style()),
        Span::styled(
            &state.status_message,
            if state.status_message.contains("Error") {
                TuiPalette::error_style()
            } else {
                TuiPalette::muted_style()
            },
        ),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(
            " 🦊 Selfware Dashboard ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, inner);
}

/// Render the garden health widget
pub fn render_garden_health(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(
            " 🌱 Garden Health ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Determine health stage
    let (stage, icon) = match (state.garden_health * 100.0) as u8 {
        0..=25 => ("Wilting", "🥀"),
        26..=50 => ("Recovering", "🌿"),
        51..=75 => ("Growing", "🌳"),
        76..=90 => ("Flourishing", "🌲"),
        _ => ("Thriving", "🌸"),
    };

    // Health bar
    let health_color = if state.garden_health > 0.75 {
        TuiPalette::BLOOM
    } else if state.garden_health > 0.5 {
        TuiPalette::GARDEN_GREEN
    } else if state.garden_health > 0.25 {
        TuiPalette::WILT
    } else {
        TuiPalette::FROST
    };

    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(health_color))
        .ratio(state.garden_health)
        .label(format!(
            "{} {} ({:.0}%)",
            icon,
            stage,
            state.garden_health * 100.0
        ));

    frame.render_widget(gauge, inner);
}

/// Render the active tools widget
pub fn render_active_tools(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 🔧 Active Tools ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.active_tools.is_empty() {
        let idle = Paragraph::new("  No active tools").style(TuiPalette::muted_style());
        frame.render_widget(idle, inner);
        return;
    }

    let items: Vec<ListItem> = state
        .active_tools
        .iter()
        .take(inner.height as usize)
        .map(|tool| {
            // Progress dots: ●●●○○ style
            let filled = (tool.progress * 5.0) as usize;
            let empty = 5 - filled;
            let progress_dots = format!("{}{}", "●".repeat(filled), "○".repeat(empty));

            let elapsed = tool.elapsed().as_secs();
            let time_str = if elapsed >= 60 {
                format!("{}m{}s", elapsed / 60, elapsed % 60)
            } else {
                format!("{}s", elapsed)
            };

            ListItem::new(Line::from(vec![
                Span::styled("  🔧 ", Style::default().fg(TuiPalette::COPPER)),
                Span::styled(
                    &tool.name,
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(progress_dots, Style::default().fg(TuiPalette::GARDEN_GREEN)),
                Span::styled(format!(" {}", time_str), TuiPalette::muted_style()),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render the logs widget
pub fn render_logs(frame: &mut Frame, area: Rect, state: &DashboardState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::border_style())
        .title(Span::styled(" 📜 Logs ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.logs.is_empty() {
        let empty = Paragraph::new("  No logs yet").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    // Show most recent logs that fit
    let max_logs = inner.height as usize;
    let items: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(max_logs)
        .map(|entry| {
            let icon_span = Span::styled(format!(" {} ", entry.level.icon()), entry.level.style());
            let time_span =
                Span::styled(format!("{} ", entry.timestamp), TuiPalette::muted_style());
            let msg_span = Span::styled(&entry.message, entry.level.style());

            ListItem::new(Line::from(vec![icon_span, time_span, msg_span]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render keyboard help overlay
pub fn render_help_overlay(frame: &mut Frame, area: Rect) {
    // Center the help box
    let width = 50.min(area.width - 4);
    let height = 15.min(area.height - 4);
    let x = (area.width - width) / 2;
    let y = (area.height - height) / 2;

    let help_area = Rect::new(x, y, width, height);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(TuiPalette::title_style())
        .style(Style::default().bg(TuiPalette::INK))
        .title(Span::styled(
            " ❓ Keyboard Shortcuts ",
            TuiPalette::title_style(),
        ));

    let inner = block.inner(help_area);
    frame.render_widget(block, help_area);

    let shortcuts = vec![
        ("q / Ctrl+C", "Quit (q twice)"),
        ("?", "Toggle this help"),
        ("Ctrl+D", "Toggle dashboard view"),
        ("Ctrl+G", "Toggle garden view"),
        ("Ctrl+L", "Toggle log view"),
        ("Tab", "Cycle focus between panes"),
        ("Space", "Pause/resume (input empty)"),
        ("z", "Toggle zoom on focused pane"),
        ("Esc", "Unzoom / close overlay"),
        ("Alt+1-6", "Quick layout presets"),
    ];

    let items: Vec<ListItem> = shortcuts
        .iter()
        .map(|(key, action)| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:12} ", key),
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*action, TuiPalette::muted_style()),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_state_default() {
        let state = DashboardState::default();
        assert_eq!(state.model, "Unknown");
        assert_eq!(state.tokens_used, 0);
        assert!(state.connected);
        assert!(state.active_tools.is_empty());
        assert!(state.logs.is_empty());
    }

    #[test]
    fn test_dashboard_state_new() {
        let state = DashboardState::new("test-model");
        assert_eq!(state.model, "test-model");
    }

    #[test]
    fn test_elapsed_formatted() {
        let state = DashboardState::default();
        // Just check it doesn't panic and returns a reasonable format
        let formatted = state.elapsed_formatted();
        assert!(formatted.contains(":"));
    }

    #[test]
    fn test_log_entry() {
        let mut state = DashboardState::default();
        state.log(LogLevel::Info, "Test message");

        assert_eq!(state.logs.len(), 1);
        assert_eq!(state.logs[0].message, "Test message");
        assert_eq!(state.logs[0].level, LogLevel::Info);
    }

    #[test]
    fn test_log_max_capacity() {
        let mut state = DashboardState::default();
        for i in 0..150 {
            state.log(LogLevel::Info, &format!("Message {}", i));
        }

        // Should only keep last 100
        assert_eq!(state.logs.len(), 100);
        // First message should be "Message 50" (0-49 were removed)
        assert!(state.logs[0].message.contains("50"));
    }

    #[test]
    fn test_tool_tracking() {
        let mut state = DashboardState::default();

        state.tool_start("file_read");
        assert_eq!(state.active_tools.len(), 1);
        assert_eq!(state.active_tools[0].name, "file_read");

        state.tool_progress("file_read", 0.5);
        assert_eq!(state.active_tools[0].progress, 0.5);

        state.tool_complete("file_read");
        assert!(state.active_tools.is_empty());
    }

    #[test]
    fn test_tool_progress_clamp() {
        let mut state = DashboardState::default();
        state.tool_start("test");

        state.tool_progress("test", 1.5);
        assert_eq!(state.active_tools[0].progress, 1.0);

        state.tool_progress("test", -0.5);
        assert_eq!(state.active_tools[0].progress, 0.0);
    }

    #[test]
    fn test_log_level_icons() {
        assert_eq!(LogLevel::Info.icon(), "ℹ");
        assert_eq!(LogLevel::Success.icon(), "✓");
        assert_eq!(LogLevel::Warning.icon(), "⚠");
        assert_eq!(LogLevel::Error.icon(), "✗");
        assert_eq!(LogLevel::Debug.icon(), "◇");
    }

    #[test]
    fn test_log_level_style() {
        // Just ensure they don't panic
        let _ = LogLevel::Info.style();
        let _ = LogLevel::Success.style();
        let _ = LogLevel::Warning.style();
        let _ = LogLevel::Error.style();
        let _ = LogLevel::Debug.style();
    }

    #[test]
    fn test_active_tool_elapsed() {
        let tool = ActiveTool {
            name: "test".to_string(),
            progress: 0.0,
            started: Instant::now(),
        };
        // Just ensure it doesn't panic
        let _ = tool.elapsed();
    }
}
