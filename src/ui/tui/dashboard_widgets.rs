//! Dashboard Widgets for Selfware TUI
//!
//! Specialized widgets for the dashboard layout including status bar,
//! garden health, active tools, and log display.

use super::TuiPalette;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Wrap},
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
    /// Streaming content chunk from the assistant
    AssistantDelta { text: String },
    /// Streaming reasoning/thinking chunk
    ThinkingDelta { text: String },
    /// Reasoning phase finished
    ThinkingEnd,
    /// Tool execution progress update
    ToolProgress { name: String, status: String },
    /// Loading spinner started
    SpinnerStart { message: String },
    /// Loading spinner message changed
    SpinnerUpdate { message: String },
    /// Loading spinner finished
    SpinnerStop,
    /// User queued a message during generation
    InputQueued { message: String, position: usize },
    /// Permission requested for tool execution
    PermissionRequested { tool_name: String, reason: String },
    /// Mode change requested (e.g., user selected "Yolo" from permission prompt)
    ModeChangeRequested { mode: crate::config::ExecutionMode },
}

/// Coordinator status for UI display
#[derive(Debug, Clone, Default)]
pub struct CoordinatorUiStatus {
    /// Whether coordinator mode is active
    pub is_active: bool,
    /// Current workflow phase
    pub current_phase: String,
    /// Number of active workers
    pub active_workers: usize,
    /// Total workers
    pub total_workers: usize,
    /// Task ID
    pub task_id: Option<String>,
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
    /// Coordinator mode status
    pub coordinator_status: CoordinatorUiStatus,
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
            // Not connected until the model actually responds — set true on the
            // first response bytes/completion, false on an agent error.
            connected: false,
            status_message: "Ready".to_string(),
            coordinator_status: CoordinatorUiStatus::default(),
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
                self.connected = true;
                self.status_message = "Ready".to_string();
                self.log(LogLevel::Success, &format!("Completed: {}", message));
            }
            TuiEvent::AgentError { message } => {
                self.connected = false;
                self.status_message = format!("Error: {}", truncate_for_display(&message, 30));
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
                // Token usage came back from the model — we're demonstrably connected.
                self.connected = true;
                // Honest total: prompt tokens are real billed usage too, not noise.
                self.tokens_used += prompt_tokens + completion_tokens;
                self.log(
                    LogLevel::Debug,
                    &format!(
                        "+{} tokens ({} prompt + {} completion)",
                        prompt_tokens + completion_tokens,
                        prompt_tokens,
                        completion_tokens
                    ),
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
            TuiEvent::AssistantDelta { text } => {
                self.connected = true;
                tracing::debug!("Assistant delta: {} chars", text.len());
            }
            TuiEvent::ThinkingDelta { text } => {
                self.connected = true;
                tracing::debug!("Thinking delta: {} chars", text.len());
            }
            TuiEvent::ThinkingEnd => {
                tracing::debug!("Thinking ended");
            }
            TuiEvent::ToolProgress { name, status } => {
                self.status_message = format!("{}: {}", name, status);
            }
            TuiEvent::SpinnerStart { message } => {
                self.status_message = message;
            }
            TuiEvent::SpinnerUpdate { message } => {
                self.status_message = message;
            }
            TuiEvent::SpinnerStop => {
                self.status_message = "Ready".to_string();
            }
            TuiEvent::InputQueued { message, position } => {
                self.log(
                    LogLevel::Info,
                    &format!("Queued ({}): {}", position, message),
                );
            }
            TuiEvent::PermissionRequested { tool_name, reason } => {
                self.status_message = format!("Permission needed: {}", tool_name);
                self.log(
                    LogLevel::Warning,
                    &format!("Permission requested for {}: {}", tool_name, reason),
                );
            }
            TuiEvent::ModeChangeRequested { mode } => {
                self.status_message = format!("Mode change: {:?}", mode);
                self.log(
                    LogLevel::Info,
                    &format!("Mode change requested: {:?}", mode),
                );
            }
        }
    }
}

fn truncate_for_display(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
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

    // Build coordinator indicator if active
    let coordinator_spans = if state.coordinator_status.is_active {
        let phase = &state.coordinator_status.current_phase;
        let workers = format!("{} workers", state.coordinator_status.active_workers);
        vec![
            Span::styled(" │ ", TuiPalette::muted_style()),
            Span::styled("👑 ", Style::default().fg(TuiPalette::AMBER)),
            Span::styled(
                format!("Coordinator [{} | {}]", phase, workers),
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
        ]
    } else {
        vec![]
    };

    let mut spans = vec![
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
    ];

    // Add coordinator indicator if active
    spans.extend(coordinator_spans);

    spans.push(Span::styled(" │ ", TuiPalette::muted_style()));
    spans.push(Span::styled(
        &state.status_message,
        if state.status_message.contains("Error") {
            TuiPalette::error_style()
        } else {
            TuiPalette::muted_style()
        },
    ));

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
        ("Space", "Hold display updates"),
        ("z", "Toggle zoom on focused pane"),
        ("Esc", "Cancel task / close overlay"),
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

/// Renders a blocking permission-confirmation modal over the dashboard.
///
/// While this is shown, the main loop routes all key input to the
/// yes/no/deny handler instead of chat/quit/pane navigation -- see
/// `run_tui_dashboard_with_events` for the key-interception logic that
/// pairs with this.
pub fn render_permission_overlay(frame: &mut Frame, area: Rect, tool_name: &str, reason: &str) {
    let modal_area = permission_modal_area(area);
    if modal_area.width < 4 || modal_area.height < 4 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(TuiPalette::AMBER))
        .style(Style::default().bg(TuiPalette::INK))
        .title(Span::styled(
            " Permission Required ",
            Style::default()
                .fg(TuiPalette::AMBER)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    if inner.height < 3 || inner.width == 0 {
        return;
    }

    let tool_area = Rect::new(inner.x, inner.y, inner.width, 1);
    let footer_area = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(1),
        inner.width,
        1,
    );
    // Reserve the tool row, a spacer above the footer, and the footer itself.
    // The details paragraph may be clipped by an exceptionally small terminal,
    // but it can never overwrite the user's permission choices.
    let details_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(3),
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Tool: ", TuiPalette::muted_style()),
            Span::styled(
                tool_name,
                Style::default()
                    .fg(TuiPalette::PARCHMENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        tool_area,
    );

    frame.render_widget(
        Paragraph::new(reason)
            .style(TuiPalette::muted_style())
            .wrap(Wrap { trim: false }),
        details_area,
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "[y]",
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" allow   "),
            Span::styled(
                "[n/Esc]",
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" deny"),
        ])),
        footer_area,
    );
}

fn permission_modal_area(area: Rect) -> Rect {
    let max_width = area.width.saturating_sub(2);
    let max_height = area.height.saturating_sub(2);

    let min_width = 40.min(max_width);
    let max_preferred_width = 120.min(max_width);
    let width = area
        .width
        .saturating_mul(4)
        .checked_div(5)
        .unwrap_or_default()
        .max(min_width)
        .min(max_preferred_width);

    let min_height = 8.min(max_height);
    let max_preferred_height = 24.min(max_height);
    let height = area
        .height
        .saturating_mul(3)
        .checked_div(5)
        .unwrap_or_default()
        .max(min_height)
        .min(max_preferred_height);

    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/dashboard_widgets/dashboard_widgets_test.rs"]
mod tests;
