//! Dashboard Widgets for Selfware TUI
//!
//! Specialized widgets for the dashboard layout including status bar,
//! garden health, active tools, and log display.

use super::TuiPalette;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
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
    PermissionRequested {
        prompt: crate::safety::confirm_view::PermissionPrompt,
    },
    /// Mode change requested (e.g., user selected "Yolo" from permission prompt)
    ModeChangeRequested { mode: crate::config::ExecutionMode },
    /// End-of-task outcome: the failure-mode banner line and the run
    /// summary (the CLI prints the same after a task; the TUI showed
    /// nothing).
    RunOutcome { summary: String },
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
    /// Garden health percentage (0.0 - 1.0): share of scanned files changed
    /// in the last 90 days. Meaningful only when `garden_health_measured`.
    pub garden_health: f64,
    /// Whether `garden_health` came from a scan (a `GardenHealthUpdate`).
    /// Until then the panel says "not measured" instead of a default 100%.
    pub garden_health_measured: bool,
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
            garden_health_measured: false,
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
                self.garden_health_measured = true;
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
            TuiEvent::PermissionRequested { prompt } => {
                self.status_message = format!("Permission needed: {}", prompt.tool_name);
                self.log(
                    LogLevel::Warning,
                    &format!("Permission requested: {}", prompt.summary()),
                );
            }
            TuiEvent::RunOutcome { summary } => {
                let level = if summary.starts_with('✅') {
                    LogLevel::Success
                } else {
                    LogLevel::Warning
                };
                for line in summary.lines().filter(|l| !l.trim().is_empty()) {
                    self.log(level, line);
                }
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

    // Rule 3: never render a green 100% that nothing measured.
    if !state.garden_health_measured {
        let empty =
            Paragraph::new("  not measured (no files scanned)").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

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
            "{} {} — {:.0}% of files changed in the last 90 days",
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
        ("Ctrl+T", "Tasks: open, edit, pause, cancel"),
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

/// Hard-wrap `text` into rows of at most `width` display columns.
///
/// Explicit (not ratatui's word wrap) so the modal's computed height matches
/// what is drawn exactly: the footer can never be pushed out of the box.
fn wrap_to_width(text: &str, width: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let width = usize::from(width.max(1));
    let mut rows = Vec::new();
    let mut row = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push(ch);
        used += w;
    }
    rows.push(row);
    rows
}

fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

fn risk_style(risk: crate::safety::confirm_view::RiskTag) -> Style {
    use crate::safety::confirm_view::RiskTag;
    let color = match risk {
        RiskTag::Reads => TuiPalette::BLOOM,
        RiskTag::RunsCommand | RiskTag::WritesWorkspace => TuiPalette::AMBER,
        _ => TuiPalette::WILT,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn confirm_line_style(kind: crate::safety::confirm_view::LineKind) -> Style {
    use crate::safety::confirm_view::LineKind;
    match kind {
        LineKind::Header => Style::default()
            .fg(TuiPalette::PARCHMENT)
            .add_modifier(Modifier::BOLD),
        LineKind::Hunk => Style::default().fg(TuiPalette::SAGE),
        LineKind::Added => Style::default().fg(TuiPalette::BLOOM),
        LineKind::Removed => Style::default().fg(TuiPalette::WILT),
        LineKind::Context => TuiPalette::muted_style(),
        LineKind::Field => Style::default().fg(TuiPalette::PARCHMENT),
        LineKind::Note => TuiPalette::muted_style().add_modifier(Modifier::ITALIC),
    }
}

/// The modal's content, pre-wrapped to `width`: (top rows, footer rows).
/// Top = tool + risk, why, blank, body. Footer = the standing options (only
/// those this prompt offers), then the always-present `[y] allow` /
/// `[n/Esc] deny` row LAST, so it is the bottom row of the box.
fn permission_modal_rows(
    prompt: &crate::safety::confirm_view::PermissionPrompt,
    width: u16,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let key_style = Style::default()
        .fg(TuiPalette::AMBER)
        .add_modifier(Modifier::BOLD);
    let mut top: Vec<Line<'static>> = Vec::new();

    top.push(Line::from(vec![
        Span::styled("Tool: ", TuiPalette::muted_style()),
        Span::styled(
            prompt.tool_name.clone(),
            Style::default()
                .fg(TuiPalette::PARCHMENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(prompt.risk.label(), risk_style(prompt.risk)),
    ]));
    if let Some(why) = &prompt.reason {
        // Reason text may come from a safety gate that quotes a command:
        // sanitize like every other displayed argument.
        let why = crate::safety::confirm_view::sanitize_line(why, 600);
        for (i, row) in wrap_to_width(&format!("Why: {}", why), width)
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                let rest = row.strip_prefix("Why: ").unwrap_or(&row).to_string();
                top.push(Line::from(vec![
                    Span::styled("Why: ", Style::default().fg(TuiPalette::AMBER)),
                    Span::styled(rest, TuiPalette::muted_style()),
                ]));
            } else {
                top.push(Line::from(Span::styled(row, TuiPalette::muted_style())));
            }
        }
    }
    // The full diff (`v`) replaces the bounded body while it is open,
    // from its scroll position on.
    let open_view = prompt.full_view.as_ref().filter(|v| v.open);
    let body: &[crate::safety::confirm_view::ConfirmLine] = match open_view {
        Some(view) => &view.lines[view.scroll.min(view.lines.len())..],
        None => &prompt.body,
    };
    if let Some(view) = open_view {
        top.push(Line::from(""));
        top.push(Line::from(Span::styled(
            format!(
                "Full diff — line {} of {}",
                (view.scroll + 1).min(view.lines.len()),
                view.lines.len()
            ),
            TuiPalette::muted_style(),
        )));
    }
    if !body.is_empty() {
        top.push(Line::from(""));
        for line in body {
            for row in wrap_to_width(&line.text, width) {
                top.push(Line::from(Span::styled(row, confirm_line_style(line.kind))));
            }
        }
    }

    let mut footer: Vec<Line<'static>> = Vec::new();
    match prompt.full_view.as_ref() {
        Some(view) if view.open => footer.push(Line::from(vec![
            Span::styled("[v/Esc]", key_style),
            Span::raw(" back   "),
            Span::styled("[↑↓ PgUp PgDn]", key_style),
            Span::raw(" scroll"),
        ])),
        Some(_) => footer.push(Line::from(vec![
            Span::styled("[v]", key_style),
            Span::raw(" view full diff"),
        ])),
        None => {}
    }
    if prompt.allow_always {
        let text = format!(" always allow {} (session)", prompt.tool_name);
        footer.push(Line::from(vec![
            Span::styled("[a]", key_style),
            Span::raw(text),
        ]));
    }
    if let Some(rule) = &prompt.shell_rule {
        for (i, row) in wrap_to_width(&format!("[p] always allow {} (session)", rule), width)
            .into_iter()
            .enumerate()
        {
            if i == 0 {
                let rest = row.strip_prefix("[p]").unwrap_or(&row).to_string();
                footer.push(Line::from(vec![
                    Span::styled("[p]", key_style),
                    Span::raw(rest),
                ]));
            } else {
                footer.push(Line::from(row));
            }
        }
    }
    // While the full diff is open, Esc closes it instead of denying.
    let deny_keys = if open_view.is_some() {
        "[n]"
    } else {
        "[n/Esc]"
    };
    footer.push(Line::from(vec![
        Span::styled("[y]", key_style),
        Span::raw(" allow   "),
        Span::styled(deny_keys, key_style),
        Span::raw(" deny"),
    ]));
    (top, footer)
}

/// Renders a blocking permission-confirmation modal over the dashboard.
///
/// Sized to its content (not a fixed fraction of the screen): tool + risk
/// tag, the reason, the same readable argument / diff view the CLI prints,
/// and only the answers this prompt offers. When the terminal is too small
/// the body is clipped — the footer with the choices never is.
///
/// While this is shown, the main loop routes all key input to
/// `permission_key_answer` instead of chat/quit/pane navigation -- see
/// `run_tui_dashboard_with_events`.
pub fn render_permission_overlay(
    frame: &mut Frame,
    area: Rect,
    prompt: &crate::safety::confirm_view::PermissionPrompt,
) {
    let modal_area = permission_modal_area(area, prompt);
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

    let (top, footer) = permission_modal_rows(prompt, inner.width);
    // Footer rows at the bottom (never clipped by the body); a spacer row
    // above them when there is room; the top section gets the rest.
    let footer_h = (footer.len() as u16).min(inner.height);
    let footer_area = Rect::new(
        inner.x,
        inner.y + inner.height - footer_h,
        inner.width,
        footer_h,
    );
    let top_h = inner.height.saturating_sub(footer_h + 1);
    let top_area = Rect::new(inner.x, inner.y, inner.width, top_h);

    frame.render_widget(Paragraph::new(top), top_area);
    frame.render_widget(Paragraph::new(footer), footer_area);
}

/// Content-sized modal rectangle, centred and clamped to `area`.
fn permission_modal_area(
    area: Rect,
    prompt: &crate::safety::confirm_view::PermissionPrompt,
) -> Rect {
    let max_width = area.width.saturating_sub(2);
    let max_height = area.height.saturating_sub(2);

    // Natural width: the widest unwrapped content line (+2 for borders).
    let (top, footer) = permission_modal_rows(prompt, u16::MAX);
    let natural = top
        .iter()
        .chain(footer.iter())
        .map(|line| {
            line.spans
                .iter()
                .map(|span| display_width(&span.content))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
        .saturating_add(2);
    let natural = u16::try_from(natural).unwrap_or(u16::MAX);
    let width = natural.clamp(40.min(max_width), 120.min(max_width));

    // Height from the content wrapped at the chosen inner width: top rows +
    // spacer + footer rows + 2 borders.
    let inner_width = width.saturating_sub(2).max(1);
    let (top, footer) = permission_modal_rows(prompt, inner_width);
    let rows = top.len() + 1 + footer.len() + 2;
    let height = u16::try_from(rows)
        .unwrap_or(u16::MAX)
        .clamp(6.min(max_height), max_height);

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
