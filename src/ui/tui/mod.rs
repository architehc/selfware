//! Selfware Terminal UI
//!
//! Rich TUI built on ratatui with split panes, status bar, and command palette.

// Feature-gated module - dead_code lint disabled at crate level

pub mod animation;
mod app;
mod dashboard_widgets;
pub mod garden_view;
mod layout;
mod markdown;
mod palette;
mod swarm_app;
mod swarm_state;
mod swarm_widgets;
mod widgets;

pub use app::{App, AppState, ChatMessage, MessageRole, TaskProgress};
pub use dashboard_widgets::{
    render_active_tools, render_garden_health, render_help_overlay, render_logs, render_status_bar,
    ActiveTool, DashboardState, LogEntry, LogLevel, SharedDashboardState, TuiEvent,
};
pub use garden_view::{render_garden_view, GardenFocus, GardenItem, GardenView};
pub use layout::{LayoutEngine, LayoutNode, LayoutPreset, Pane, PaneId, PaneType, SplitDirection};
pub use markdown::MarkdownRenderer;
pub use palette::CommandPalette;
pub use widgets::{GardenSpinner, GrowthGauge, StatusIndicator, StatusType, ToolOutput};

// Re-export swarm components
pub use swarm_app::{SwarmApp, SwarmAppState};
pub use swarm_state::{
    AgentUiState, DecisionView, EventType, MemoryEntryView, SwarmEvent, SwarmStats, SwarmUiState,
    TaskView,
};
pub use swarm_widgets::{
    render_agent_swarm, render_decisions, render_shared_memory, render_swarm_events,
    render_swarm_health, render_swarm_help, render_swarm_status_bar, render_task_queue,
};

// Re-export animation components for convenience
pub use animation::{
    agent_avatar::{ActivityLevel, AgentAvatar, AgentRole},
    message_flow::{MessageFlow, MessageFlowManager, MessageType},
    particles::{EmitConfig, Particle, ParticleSystem},
    progress::AnimatedProgressBar,
    token_stream::{TokenSize, TokenStream},
    Animation, AnimationManager,
};

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    Frame, Terminal,
};
use std::io::{self, Stdout};
use std::time::{Duration, Instant};

/// The Selfware color palette for TUI
///
/// Colors are derived from the current UI theme. Use the static methods
/// to get theme-appropriate styles, or the const colors for the default
/// amber theme when performance is critical.
pub struct TuiPalette;

impl TuiPalette {
    // Default colors (Amber theme) - used for const contexts
    pub const AMBER: Color = Color::Rgb(212, 163, 115);
    pub const GARDEN_GREEN: Color = Color::Rgb(96, 108, 56);
    pub const SOIL_BROWN: Color = Color::Rgb(188, 108, 37);
    pub const INK: Color = Color::Rgb(40, 54, 24);
    pub const PARCHMENT: Color = Color::Rgb(254, 250, 224);

    // Accent colors (default theme)
    pub const RUST: Color = Color::Rgb(139, 69, 19);
    pub const COPPER: Color = Color::Rgb(184, 115, 51);
    pub const SAGE: Color = Color::Rgb(143, 151, 121);
    pub const STONE: Color = Color::Rgb(128, 128, 128);

    // Status colors (default theme)
    pub const BLOOM: Color = Color::Rgb(144, 190, 109);
    pub const WILT: Color = Color::Rgb(188, 108, 37);
    pub const FROST: Color = Color::Rgb(100, 100, 120);

    /// Convert a colored::CustomColor to a ratatui Color
    fn to_ratatui_color(c: colored::CustomColor) -> Color {
        Color::Rgb(c.r, c.g, c.b)
    }

    /// Get the current theme's primary color
    pub fn primary() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.primary)
    }

    /// Get the current theme's success color
    pub fn success() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.success)
    }

    /// Get the current theme's warning color
    pub fn warning() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.warning)
    }

    /// Get the current theme's error color
    pub fn error() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.error)
    }

    /// Get the current theme's muted color
    pub fn muted() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.muted)
    }

    /// Get the current theme's accent color
    pub fn accent() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.accent)
    }

    /// Get the current theme's tool color
    pub fn tool() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.tool)
    }

    /// Get the current theme's path color
    pub fn path() -> Color {
        let theme = crate::ui::theme::current_theme();
        Self::to_ratatui_color(theme.path)
    }

    /// Style for titles (uses current theme)
    pub fn title_style() -> Style {
        Style::default()
            .fg(Self::primary())
            .add_modifier(Modifier::BOLD)
    }

    /// Style for selected items
    pub fn selected_style() -> Style {
        Style::default()
            .bg(Self::success())
            .fg(Self::PARCHMENT)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for success messages
    pub fn success_style() -> Style {
        Style::default().fg(Self::success())
    }

    /// Style for warning messages
    pub fn warning_style() -> Style {
        Style::default().fg(Self::warning())
    }

    /// Style for error messages
    pub fn error_style() -> Style {
        Style::default().fg(Self::error())
    }

    /// Style for muted text
    pub fn muted_style() -> Style {
        Style::default().fg(Self::muted())
    }

    /// Style for paths
    pub fn path_style() -> Style {
        Style::default()
            .fg(Self::path())
            .add_modifier(Modifier::ITALIC)
    }

    /// Border style
    pub fn border_style() -> Style {
        Style::default().fg(Self::path())
    }
}

/// Terminal wrapper for TUI operations
pub struct TuiTerminal {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TuiTerminal {
    /// Create and initialize the terminal
    pub fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        // Install a panic hook that restores the terminal to normal state
        // BEFORE printing the panic message. Without this, a panic leaves the
        // terminal in raw mode with the alternate screen active, making the
        // error invisible and the shell unusable. The Drop impl may not run
        // reliably during a panic unwind.
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            // Best-effort restore — ignore errors since we're already panicking
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            original_hook(panic_info);
        }));

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Get mutable reference to terminal
    pub fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<Stdout>> {
        &mut self.terminal
    }

    /// Get terminal size
    pub fn size(&self) -> Result<Rect> {
        Ok(self.terminal.size()?)
    }

    /// Restore terminal to normal state
    pub fn restore(&mut self) -> Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for TuiTerminal {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

/// Create a standard layout with header, main content, and status bar
pub fn standard_layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Main content
            Constraint::Length(1), // Status bar
        ])
        .split(area)
        .to_vec()
}

/// Create a split layout for chat and file explorer
pub fn split_layout(area: Rect, left_percent: u16) -> (Rect, Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_percent),
            Constraint::Percentage(100 - left_percent),
        ])
        .split(area);

    (chunks[0], chunks[1])
}

/// Read next terminal event with timeout
pub fn read_event(timeout_ms: u64) -> Result<Option<Event>> {
    if event::poll(std::time::Duration::from_millis(timeout_ms))? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Check for specific key press
pub fn is_key(event: &Event, key: KeyCode, modifiers: KeyModifiers) -> bool {
    matches!(
        event,
        Event::Key(k) if k.code == key && k.modifiers == modifiers
    )
}

/// Check for quit keys (q, Ctrl+C)
/// Note: Ctrl+D is reserved for dashboard toggle
pub fn is_quit(event: &Event) -> bool {
    is_key(event, KeyCode::Char('q'), KeyModifiers::NONE)
        || is_key(event, KeyCode::Char('c'), KeyModifiers::CONTROL)
}

enum QuitDecision {
    None,
    Armed,
    Quit,
}

fn evaluate_quit_key(
    key: &crossterm::event::KeyEvent,
    allow_q: bool,
    quit_armed_at: &mut Option<Instant>,
) -> QuitDecision {
    if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
        return QuitDecision::Quit;
    }

    if key.code == KeyCode::Char('q') && key.modifiers == KeyModifiers::NONE && allow_q {
        let now = Instant::now();
        let timeout = Duration::from_secs(2);
        if let Some(armed_at) = *quit_armed_at {
            if now.duration_since(armed_at) <= timeout {
                *quit_armed_at = None;
                return QuitDecision::Quit;
            }
        }
        *quit_armed_at = Some(now);
        return QuitDecision::Armed;
    }

    *quit_armed_at = None;
    QuitDecision::None
}

fn with_dashboard_state<R>(
    shared_state: &SharedDashboardState,
    f: impl FnOnce(&mut DashboardState) -> R,
) -> R {
    let mut state = shared_state.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut state)
}

fn dashboard_state_snapshot(shared_state: &SharedDashboardState) -> DashboardState {
    shared_state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

fn truncate_for_display(input: &str, max_chars: usize) -> String {
    input.chars().take(max_chars).collect()
}

/// Run the TUI application
///
/// This creates a full terminal UI with chat, command palette, and status bar.
/// Returns when the user quits (q or Ctrl+C).
pub fn run_tui(model: &str) -> Result<Vec<String>> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = App::new(model);
    let mut user_inputs = Vec::new();

    // Create layout engine for advanced pane management
    let mut layout_engine = LayoutEngine::new();
    layout_engine.apply_preset(LayoutPreset::Focus);

    // Create widgets for status display
    let mut spinner = GardenSpinner::new("Processing...");
    let status_indicator = StatusIndicator::new(StatusType::Info, "Connected");

    // Create markdown renderer for rich message display
    let md_renderer = MarkdownRenderer::new();

    loop {
        // Update spinner animation
        spinner.tick();

        // Render the app
        terminal.terminal().draw(|frame| {
            app.render(frame);

            // Render additional widgets based on state
            if app.state == AppState::RunningTask {
                if let Some(ref progress) = app.task_progress {
                    let gauge = GrowthGauge::new(
                        progress.current_step as f64 / progress.total_steps.unwrap_or(10) as f64,
                        "Task",
                    );
                    // Gauge would be rendered in the progress area
                    let _ = gauge; // Use the gauge
                }
            }

            // Layout engine manages pane positions
            let _panes = layout_engine.calculate_layout(frame.size());

            // Status indicator would show connection state
            let _ = &status_indicator;

            // Markdown renderer would format assistant messages
            let _ = &md_renderer;
        })?;

        // Handle events
        if let Some(event) = read_event(100)? {
            if is_quit(&event) {
                break;
            }

            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(input) = app.on_enter() {
                            if input.starts_with('/') {
                                // Command
                                app.add_user_message(&input);
                                app.status = format!("Executed: {}", input);
                            } else {
                                // Regular message
                                app.add_user_message(&input);
                                user_inputs.push(input);
                            }
                        }
                    }
                    KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                        app.toggle_palette();
                    }
                    KeyCode::Char(c) => app.on_char(c),
                    KeyCode::Backspace => app.on_backspace(),
                    KeyCode::Left => app.on_left(),
                    KeyCode::Right => app.on_right(),
                    KeyCode::Up => app.on_up(),
                    KeyCode::Down => app.on_down(),
                    KeyCode::Esc => app.on_escape(),
                    KeyCode::Tab => {
                        // Cycle through panes
                        layout_engine.focus_next();
                    }
                    _ => {}
                }
            }
        }
    }

    terminal.restore()?;
    Ok(user_inputs)
}

/// Run TUI with a message handler callback
pub fn run_tui_with_handler<F>(model: &str, mut handler: F) -> Result<()>
where
    F: FnMut(&str) -> Option<String>,
{
    let mut terminal = TuiTerminal::new()?;
    let mut app = App::new(model);

    loop {
        terminal.terminal().draw(|frame| {
            app.render(frame);
        })?;

        if let Some(event) = read_event(100)? {
            if is_quit(&event) {
                break;
            }

            if let Event::Key(key) = event {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(input) = app.on_enter() {
                            app.add_user_message(&input);

                            // Call handler and get response
                            if let Some(response) = handler(&input) {
                                app.add_assistant_message(&response);
                            }
                        }
                    }
                    KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                        app.toggle_palette();
                    }
                    KeyCode::Char(c) => app.on_char(c),
                    KeyCode::Backspace => app.on_backspace(),
                    KeyCode::Left => app.on_left(),
                    KeyCode::Right => app.on_right(),
                    KeyCode::Up => app.on_up(),
                    KeyCode::Down => app.on_down(),
                    KeyCode::Esc => app.on_escape(),
                    _ => {}
                }
            }
        }
    }

    terminal.restore()?;
    Ok(())
}

/// Run the TUI dashboard mode
///
/// This creates a full terminal dashboard UI with:
/// - Status bar showing model, tokens, elapsed time
/// - Main chat pane (60% width)
/// - Garden health widget
/// - Active tools widget
/// - Logs panel at bottom
///
/// Keyboard shortcuts:
/// - q (press twice) / Ctrl+C: Quit
/// - ?: Toggle help overlay
/// - Ctrl+D: Toggle dashboard/focus mode
/// - Ctrl+G: Toggle garden view zoom
/// - Ctrl+L: Toggle logs view zoom
/// - Tab: Cycle focus between panes
/// - z: Toggle zoom on focused pane
/// - Alt+1-6: Quick layout presets
pub fn run_tui_dashboard(model: &str) -> Result<Vec<String>> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = App::new(model);
    let mut layout_engine = LayoutEngine::new();
    let mut dashboard_state = DashboardState::new(model);
    let mut garden_view = garden_view::GardenView::new();
    let mut user_inputs = Vec::new();
    let mut show_help = false;
    let mut paused = false;
    let mut quit_armed_at: Option<Instant> = None;

    // Scan current directory for garden view
    let cwd = std::env::current_dir().unwrap_or_default();
    let garden = crate::ui::garden::scan_directory(&cwd);
    dashboard_state.log(
        LogLevel::Info,
        &format!("Scanned garden: {} plants", garden.total_plants),
    );
    garden_view.set_garden(garden);

    // Apply dashboard layout preset
    layout_engine.apply_preset(LayoutPreset::Dashboard);
    dashboard_state.log(LogLevel::Info, "Dashboard initialized");
    dashboard_state.log(LogLevel::Success, "Connected to model");

    loop {
        // Render the dashboard
        terminal.terminal().draw(|frame| {
            let area = frame.size();

            // Calculate pane layouts
            let pane_layouts = layout_engine.calculate_layout(area);

            // Render each pane based on its type
            for (pane_id, pane_area) in &pane_layouts {
                if let Some(pane) = layout_engine.get_pane(*pane_id) {
                    match pane.pane_type {
                        PaneType::StatusBar => {
                            render_status_bar(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::Chat => {
                            // Render chat in this pane
                            render_chat_pane(frame, *pane_area, &app, pane.focused);
                        }
                        PaneType::GardenHealth => {
                            render_garden_health(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::ActiveTools => {
                            render_active_tools(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::Logs => {
                            render_logs(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::GardenView => {
                            render_garden_view(frame, *pane_area, &mut garden_view, pane.focused);
                        }
                        PaneType::Editor => {
                            render_editor_pane(frame, *pane_area, pane);
                        }
                        PaneType::Terminal => {
                            render_terminal_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Explorer => {
                            render_explorer_pane(frame, *pane_area, pane);
                        }
                        PaneType::Diff => {
                            render_diff_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Debug => {
                            render_debug_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Help => {
                            render_help_pane(frame, *pane_area, pane);
                        }
                    }
                }
            }

            // Render help overlay if active
            if show_help {
                render_help_overlay(frame, area);
            }

            // Render pause indicator if paused
            if paused {
                render_pause_indicator(frame, area);
            }
        })?;

        // Handle events
        if let Some(Event::Key(key)) = read_event(100)? {
            // Check if we're in input mode (chat focused with non-empty input or chatting state)
            let in_input_mode = app.state == AppState::Chatting && !show_help;
            let allow_q_quit = !in_input_mode || app.input.is_empty();

            match evaluate_quit_key(&key, allow_q_quit, &mut quit_armed_at) {
                QuitDecision::Quit => {
                    dashboard_state.log(LogLevel::Info, "Shutting down...");
                    break;
                }
                QuitDecision::Armed => {
                    dashboard_state.log(
                        LogLevel::Warning,
                        "Press q again within 2s to quit (or Ctrl+C to force quit).",
                    );
                    continue;
                }
                QuitDecision::None => {}
            }

            match key.code {
                // Toggle help overlay (works anywhere)
                KeyCode::Char('?') if !in_input_mode || app.input.is_empty() => {
                    show_help = !show_help;
                }

                // Toggle dashboard/focus mode (Ctrl+D or 'd' when input is empty)
                KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                    if layout_engine.current_preset() == LayoutPreset::Dashboard {
                        layout_engine.apply_preset(LayoutPreset::Focus);
                        dashboard_state.log(LogLevel::Info, "Switched to focus mode");
                    } else {
                        layout_engine.apply_preset(LayoutPreset::Dashboard);
                        dashboard_state.log(LogLevel::Info, "Switched to dashboard mode");
                    }
                }

                // Toggle garden view (Ctrl+G)
                KeyCode::Char('g') if key.modifiers == KeyModifiers::CONTROL => {
                    // Find garden pane and focus/zoom it
                    for pane_id in layout_engine.pane_ids() {
                        if let Some(pane) = layout_engine.get_pane(pane_id) {
                            if pane.pane_type == PaneType::GardenView {
                                layout_engine.set_focus(pane_id);
                                layout_engine.toggle_zoom();
                                dashboard_state.log(LogLevel::Info, "Toggled garden view");
                                break;
                            }
                        }
                    }
                }

                // Toggle logs view (Ctrl+L)
                KeyCode::Char('l') if key.modifiers == KeyModifiers::CONTROL => {
                    for pane_id in layout_engine.pane_ids() {
                        if let Some(pane) = layout_engine.get_pane(pane_id) {
                            if pane.pane_type == PaneType::Logs {
                                layout_engine.set_focus(pane_id);
                                layout_engine.toggle_zoom();
                                dashboard_state.log(LogLevel::Info, "Toggled logs view");
                                break;
                            }
                        }
                    }
                }

                // Pause/resume (works when input is empty)
                KeyCode::Char(' ') if app.input.is_empty() => {
                    paused = !paused;
                    if paused {
                        dashboard_state.log(LogLevel::Warning, "Streaming paused");
                    } else {
                        dashboard_state.log(LogLevel::Info, "Streaming resumed");
                    }
                }

                // Zoom toggle
                KeyCode::Char('z') => {
                    layout_engine.toggle_zoom();
                }

                // Animation speed controls
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    app.on_plus();
                    dashboard_state.log(LogLevel::Info, &app.status);
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    app.on_minus();
                    dashboard_state.log(LogLevel::Info, &app.status);
                }

                // Cycle focus
                KeyCode::Tab => {
                    layout_engine.focus_next();
                }
                KeyCode::BackTab => {
                    layout_engine.focus_prev();
                }

                // Quick layout presets
                KeyCode::Char('1') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Focus);
                    dashboard_state.log(LogLevel::Info, "Layout: Focus");
                }
                KeyCode::Char('2') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Coding);
                    dashboard_state.log(LogLevel::Info, "Layout: Coding");
                }
                KeyCode::Char('3') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Debugging);
                    dashboard_state.log(LogLevel::Info, "Layout: Debugging");
                }
                KeyCode::Char('4') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Review);
                    dashboard_state.log(LogLevel::Info, "Layout: Review");
                }
                KeyCode::Char('5') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Explore);
                    dashboard_state.log(LogLevel::Info, "Layout: Explore");
                }
                KeyCode::Char('6') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::FullWorkspace);
                    dashboard_state.log(LogLevel::Info, "Layout: Full Workspace");
                }

                // Chat input handling
                KeyCode::Enter => {
                    if !show_help {
                        if let Some(input) = app.on_enter() {
                            if input.starts_with('/') {
                                app.add_user_message(&input);
                                app.status = format!("Executed: {}", input);
                                dashboard_state.log(LogLevel::Info, &format!("Command: {}", input));
                            } else {
                                app.add_user_message(&input);
                                user_inputs.push(input.clone());
                                dashboard_state.log(
                                    LogLevel::Info,
                                    &format!("User: {}", truncate_for_display(&input, 50)),
                                );
                            }
                        }
                    }
                }

                KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                    app.toggle_palette();
                }

                KeyCode::Char(c) if !show_help => app.on_char(c),
                KeyCode::Backspace if !show_help => app.on_backspace(),
                KeyCode::Left if !show_help => app.on_left(),
                KeyCode::Right if !show_help => app.on_right(),
                KeyCode::Up if !show_help => app.on_up(),
                KeyCode::Down if !show_help => app.on_down(),
                KeyCode::Esc => {
                    if show_help {
                        show_help = false;
                    } else if layout_engine.is_zoomed() {
                        layout_engine.toggle_zoom();
                    } else {
                        app.on_escape();
                    }
                }
                _ => {}
            }
        }
    }

    terminal.restore()?;
    Ok(user_inputs)
}

/// Run the TUI dashboard with shared state and event receiver
///
/// This version allows external code (like the Agent) to send events that
/// update the dashboard in real-time. The event_rx is polled non-blocking
/// on each frame.
pub fn run_tui_dashboard_with_events(
    model: &str,
    shared_state: SharedDashboardState,
    event_rx: std::sync::mpsc::Receiver<TuiEvent>,
    user_input_tx: std::sync::mpsc::Sender<String>,
) -> Result<()> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = App::new(model);
    let mut layout_engine = LayoutEngine::new();
    let mut garden_view = garden_view::GardenView::new();
    let mut show_help = false;
    let mut paused = false;
    let mut quit_armed_at: Option<Instant> = None;

    // Scan current directory for garden view
    let cwd = std::env::current_dir().unwrap_or_default();
    let garden = crate::ui::garden::scan_directory(&cwd);
    with_dashboard_state(&shared_state, |state| {
        state.log(
            LogLevel::Info,
            &format!("Scanned garden: {} plants", garden.total_plants),
        );
    });
    garden_view.set_garden(garden);

    // Apply dashboard layout preset
    layout_engine.apply_preset(LayoutPreset::Dashboard);
    with_dashboard_state(&shared_state, |state| {
        state.log(LogLevel::Info, "Dashboard initialized");
        state.log(LogLevel::Success, "Connected to model");
    });

    loop {
        // Process any pending events from the agent (non-blocking)
        loop {
            match event_rx.try_recv() {
                Ok(event) => {
                    with_dashboard_state(&shared_state, |state| state.process_event(event));
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Sender dropped, log and continue
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Warning, "Event channel disconnected");
                    });
                    break;
                }
            }
        }

        // Get a copy of the dashboard state for rendering
        let dashboard_state = dashboard_state_snapshot(&shared_state);

        // Render the dashboard
        terminal.terminal().draw(|frame| {
            let area = frame.size();
            let pane_layouts = layout_engine.calculate_layout(area);

            for (pane_id, pane_area) in &pane_layouts {
                if let Some(pane) = layout_engine.get_pane(*pane_id) {
                    match pane.pane_type {
                        PaneType::StatusBar => {
                            render_status_bar(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::Chat => {
                            render_chat_pane(frame, *pane_area, &app, pane.focused);
                        }
                        PaneType::GardenHealth => {
                            render_garden_health(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::ActiveTools => {
                            render_active_tools(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::Logs => {
                            render_logs(frame, *pane_area, &dashboard_state);
                        }
                        PaneType::GardenView => {
                            render_garden_view(frame, *pane_area, &mut garden_view, pane.focused);
                        }
                        PaneType::Editor => {
                            render_editor_pane(frame, *pane_area, pane);
                        }
                        PaneType::Terminal => {
                            render_terminal_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Explorer => {
                            render_explorer_pane(frame, *pane_area, pane);
                        }
                        PaneType::Diff => {
                            render_diff_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Debug => {
                            render_debug_pane(frame, *pane_area, pane, &dashboard_state);
                        }
                        PaneType::Help => {
                            render_help_pane(frame, *pane_area, pane);
                        }
                    }
                }
            }

            if show_help {
                render_help_overlay(frame, area);
            }

            if paused {
                render_pause_indicator(frame, area);
            }
        })?;

        // Handle events (same logic as run_tui_dashboard)
        if let Some(Event::Key(key)) = read_event(100)? {
            let in_input_mode = app.state == AppState::Chatting && !show_help;
            let allow_q_quit = !in_input_mode || app.input.is_empty();

            match evaluate_quit_key(&key, allow_q_quit, &mut quit_armed_at) {
                QuitDecision::Quit => {
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Shutting down...");
                    });
                    break;
                }
                QuitDecision::Armed => {
                    with_dashboard_state(&shared_state, |state| {
                        state.log(
                            LogLevel::Warning,
                            "Press q again within 2s to quit (or Ctrl+C to force quit).",
                        )
                    });
                    continue;
                }
                QuitDecision::None => {}
            }

            match key.code {
                KeyCode::Char('?') if !in_input_mode || app.input.is_empty() => {
                    show_help = !show_help;
                }
                KeyCode::Char('d') if key.modifiers == KeyModifiers::CONTROL => {
                    if layout_engine.current_preset() == LayoutPreset::Dashboard {
                        layout_engine.apply_preset(LayoutPreset::Focus);
                        with_dashboard_state(&shared_state, |state| {
                            state.log(LogLevel::Info, "Switched to focus mode");
                        });
                    } else {
                        layout_engine.apply_preset(LayoutPreset::Dashboard);
                        with_dashboard_state(&shared_state, |state| {
                            state.log(LogLevel::Info, "Switched to dashboard mode");
                        });
                    }
                }
                KeyCode::Char('g') if key.modifiers == KeyModifiers::CONTROL => {
                    for pane_id in layout_engine.pane_ids() {
                        if let Some(pane) = layout_engine.get_pane(pane_id) {
                            if pane.pane_type == PaneType::GardenView {
                                layout_engine.set_focus(pane_id);
                                layout_engine.toggle_zoom();
                                with_dashboard_state(&shared_state, |state| {
                                    state.log(LogLevel::Info, "Toggled garden view");
                                });
                                break;
                            }
                        }
                    }
                }
                KeyCode::Char('l') if key.modifiers == KeyModifiers::CONTROL => {
                    for pane_id in layout_engine.pane_ids() {
                        if let Some(pane) = layout_engine.get_pane(pane_id) {
                            if pane.pane_type == PaneType::Logs {
                                layout_engine.set_focus(pane_id);
                                layout_engine.toggle_zoom();
                                with_dashboard_state(&shared_state, |state| {
                                    state.log(LogLevel::Info, "Toggled logs view");
                                });
                                break;
                            }
                        }
                    }
                }
                KeyCode::Char(' ') if app.input.is_empty() => {
                    paused = !paused;
                    with_dashboard_state(&shared_state, |state| {
                        if paused {
                            state.log(LogLevel::Warning, "Streaming paused");
                        } else {
                            state.log(LogLevel::Info, "Streaming resumed");
                        }
                    });
                }
                KeyCode::Char('z') => {
                    layout_engine.toggle_zoom();
                }
                // Animation speed controls
                KeyCode::Char('+') | KeyCode::Char('=') => {
                    app.on_plus();
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, &app.status);
                    });
                }
                KeyCode::Char('-') | KeyCode::Char('_') => {
                    app.on_minus();
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, &app.status);
                    });
                }
                KeyCode::Tab => {
                    layout_engine.focus_next();
                }
                KeyCode::BackTab => {
                    layout_engine.focus_prev();
                }
                KeyCode::Char('1') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Focus);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Focus");
                    });
                }
                KeyCode::Char('2') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Coding);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Coding");
                    });
                }
                KeyCode::Char('3') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Debugging);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Debugging");
                    });
                }
                KeyCode::Char('4') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Review);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Review");
                    });
                }
                KeyCode::Char('5') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::Explore);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Explore");
                    });
                }
                KeyCode::Char('6') if key.modifiers == KeyModifiers::ALT => {
                    layout_engine.apply_preset(LayoutPreset::FullWorkspace);
                    with_dashboard_state(&shared_state, |state| {
                        state.log(LogLevel::Info, "Layout: Full Workspace");
                    });
                }
                KeyCode::Enter => {
                    if !show_help {
                        if let Some(input) = app.on_enter() {
                            if input.starts_with('/') {
                                app.add_user_message(&input);
                                app.status = format!("Executed: {}", input);
                                with_dashboard_state(&shared_state, |state| {
                                    state.log(LogLevel::Info, &format!("Command: {}", input));
                                });
                                // Commands like /clear or /analyze could also be sent to the agent
                                let _ = user_input_tx.send(input);
                            } else {
                                app.add_user_message(&input);
                                let _ = user_input_tx.send(input.clone());
                                with_dashboard_state(&shared_state, |state| {
                                    state.log(
                                        LogLevel::Info,
                                        &format!("User: {}", truncate_for_display(&input, 50)),
                                    );
                                });
                            }
                        }
                    }
                }
                KeyCode::Char('p') if key.modifiers == KeyModifiers::CONTROL => {
                    app.toggle_palette();
                }
                KeyCode::Char(c) if !show_help => app.on_char(c),
                KeyCode::Backspace if !show_help => app.on_backspace(),
                KeyCode::Left if !show_help => app.on_left(),
                KeyCode::Right if !show_help => app.on_right(),
                KeyCode::Up if !show_help => app.on_up(),
                KeyCode::Down if !show_help => app.on_down(),
                KeyCode::Esc => {
                    if show_help {
                        show_help = false;
                    } else if layout_engine.is_zoomed() {
                        layout_engine.toggle_zoom();
                    } else {
                        app.on_escape();
                    }
                }
                _ => {}
            }
        }
    }

    terminal.restore()?;
    Ok(())
}

/// Create a channel pair for sending events to the TUI dashboard
///
/// Returns (sender, receiver) tuple. Pass the receiver to `run_tui_dashboard_with_events`
/// and keep the sender to send events from your agent code.
pub fn create_event_channel() -> (
    std::sync::mpsc::Sender<TuiEvent>,
    std::sync::mpsc::Receiver<TuiEvent>,
) {
    std::sync::mpsc::channel()
}

/// Render a chat pane
fn render_chat_pane(frame: &mut Frame, area: Rect, app: &App, focused: bool) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let border_style = if focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(" 💬 Chat ", TuiPalette::title_style()));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner area for messages and input
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),    // Messages
            Constraint::Length(3), // Input
        ])
        .split(inner);

    // Render messages
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .rev()
        .skip(app.scroll)
        .take(chunks[0].height as usize)
        .map(|msg| {
            let style = match msg.role {
                MessageRole::User => Style::default().fg(TuiPalette::AMBER),
                MessageRole::Assistant => Style::default().fg(TuiPalette::GARDEN_GREEN),
                MessageRole::System => TuiPalette::muted_style(),
                MessageRole::Tool => Style::default().fg(TuiPalette::COPPER),
            };

            let prefix = match msg.role {
                MessageRole::User => "You",
                MessageRole::Assistant => "🦊",
                MessageRole::System => "📋",
                MessageRole::Tool => "🔧",
            };

            let content = format!("{} {} {}", msg.timestamp, prefix, msg.content);
            ListItem::new(Line::from(Span::styled(content, style)))
        })
        .collect();

    let messages = List::new(items);
    frame.render_widget(messages, chunks[0]);

    // Render input
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused && app.state == AppState::Chatting {
            TuiPalette::title_style()
        } else {
            TuiPalette::muted_style()
        })
        .title(" Input ");

    let input_inner = input_block.inner(chunks[1]);
    frame.render_widget(input_block, chunks[1]);

    let input_text = Paragraph::new(format!("❯ {}", app.input))
        .style(Style::default().fg(TuiPalette::PARCHMENT));
    frame.render_widget(input_text, input_inner);

    // Show cursor if focused and chatting
    if focused && app.state == AppState::Chatting {
        frame.set_cursor(input_inner.x + 2 + app.cursor as u16, input_inner.y);
    }
}

fn render_editor_pane(frame: &mut Frame, area: Rect, pane: &Pane) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = Paragraph::new(
        "  No file open\n\n  Use /analyze <path> to inspect code\n  Use Alt+2 for coding layout",
    )
    .style(TuiPalette::muted_style());
    frame.render_widget(content, inner);
}

fn render_terminal_pane(frame: &mut Frame, area: Rect, pane: &Pane, state: &DashboardState) {
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if state.logs.is_empty() {
        let empty = Paragraph::new("  No terminal output yet").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    let items: Vec<ListItem> = state
        .logs
        .iter()
        .rev()
        .take(inner.height as usize)
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", entry.level.icon()), entry.level.style()),
                Span::styled(&entry.message, TuiPalette::muted_style()),
            ]))
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn render_explorer_pane(frame: &mut Frame, area: Rect, pane: &Pane) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut entries = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(".") {
        for entry in read_dir.flatten() {
            let file_type = entry.file_type().ok();
            let icon = match file_type {
                Some(ft) if ft.is_dir() => "📁",
                Some(ft) if ft.is_symlink() => "🔗",
                _ => "📄",
            };
            entries.push(format!("{} {}", icon, entry.file_name().to_string_lossy()));
        }
    }

    entries.sort();
    if entries.is_empty() {
        let empty = Paragraph::new("  No files found").style(TuiPalette::muted_style());
        frame.render_widget(empty, inner);
        return;
    }

    let items: Vec<ListItem> = entries
        .into_iter()
        .take(inner.height as usize)
        .map(ListItem::new)
        .collect();
    frame.render_widget(List::new(items), inner);
}

fn render_diff_pane(frame: &mut Frame, area: Rect, pane: &Pane, state: &DashboardState) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let summary = format!(
        "  No active diff\n\n  Session logs: {}\n  Active tools: {}\n  Use review workflows to populate diff content",
        state.logs.len(),
        state.active_tools.len()
    );
    frame.render_widget(
        Paragraph::new(summary).style(TuiPalette::muted_style()),
        inner,
    );
}

fn render_debug_pane(frame: &mut Frame, area: Rect, pane: &Pane, state: &DashboardState) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = format!(
        "  Connected: {}\n  Tokens used: {}\n  Active tools: {}\n  Logs: {}\n  Status: {}",
        if state.connected { "yes" } else { "no" },
        state.tokens_used,
        state.active_tools.len(),
        state.logs.len(),
        state.status_message
    );
    frame.render_widget(
        Paragraph::new(content).style(TuiPalette::muted_style()),
        inner,
    );
}

fn render_help_pane(frame: &mut Frame, area: Rect, pane: &Pane) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let border_style = if pane.focused {
        TuiPalette::title_style()
    } else {
        TuiPalette::border_style()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} {} ", pane.pane_type.icon(), pane.title()),
            TuiPalette::title_style(),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = "  ?        Toggle help\n  q        Quit (press twice)\n  Ctrl+C   Force quit\n  Tab      Cycle panes\n  z        Zoom pane\n  Alt+1-6  Layout presets";
    frame.render_widget(
        Paragraph::new(content).style(TuiPalette::muted_style()),
        inner,
    );
}

/// Render pause indicator
fn render_pause_indicator(frame: &mut Frame, area: Rect) {
    use ratatui::text::Span;
    use ratatui::widgets::{Block, Paragraph};

    let width = 20;
    let height = 3;
    let x = (area.width - width) / 2;
    let y = area.height - height - 2;

    let indicator_area = Rect::new(x, y, width, height);

    let block = Block::default().style(Style::default().bg(TuiPalette::WILT));

    frame.render_widget(block, indicator_area);

    let text = Paragraph::new(Span::styled(
        "  ⏸ PAUSED  ",
        Style::default()
            .fg(TuiPalette::PARCHMENT)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(text, Rect::new(x, y + 1, width, 1));
}

#[allow(clippy::items_after_test_module)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_default_colors() {
        // Verify default amber theme colors are defined correctly
        assert_eq!(TuiPalette::AMBER, Color::Rgb(212, 163, 115));
        assert_eq!(TuiPalette::GARDEN_GREEN, Color::Rgb(96, 108, 56));
    }

    #[test]
    fn test_palette_styles() {
        let title = TuiPalette::title_style();
        assert!(title.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_palette_theme_integration() {
        use crate::ui::theme::{set_theme, ThemeId};

        // Test with Amber theme
        set_theme(ThemeId::Amber);
        let primary = TuiPalette::primary();
        assert_eq!(primary, Color::Rgb(212, 163, 115)); // Amber primary

        // Test with Ocean theme
        set_theme(ThemeId::Ocean);
        let primary = TuiPalette::primary();
        assert_eq!(primary, Color::Rgb(100, 149, 237)); // Ocean primary (Cornflower blue)

        // Test success style respects theme
        set_theme(ThemeId::HighContrast);
        let success = TuiPalette::success();
        assert_eq!(success, Color::Rgb(0, 255, 0)); // High contrast lime green

        // Reset to default
        set_theme(ThemeId::Amber);
    }

    #[test]
    fn test_standard_layout() {
        let area = Rect::new(0, 0, 100, 50);
        let layout = standard_layout(area);
        assert_eq!(layout.len(), 3);
        assert_eq!(layout[0].height, 3); // Header
        assert_eq!(layout[2].height, 1); // Status bar
    }

    #[test]
    fn test_split_layout() {
        let area = Rect::new(0, 0, 100, 50);
        let (left, right) = split_layout(area, 30);
        assert_eq!(left.width, 30);
        assert_eq!(right.width, 70);
    }

    #[test]
    fn test_split_layout_50_50() {
        let area = Rect::new(0, 0, 100, 50);
        let (left, right) = split_layout(area, 50);
        assert_eq!(left.width, 50);
        assert_eq!(right.width, 50);
    }

    #[test]
    fn test_split_layout_extreme_left() {
        let area = Rect::new(0, 0, 100, 50);
        let (left, right) = split_layout(area, 90);
        assert_eq!(left.width, 90);
        assert_eq!(right.width, 10);
    }

    #[test]
    fn test_standard_layout_small_area() {
        let area = Rect::new(0, 0, 50, 20);
        let layout = standard_layout(area);
        assert_eq!(layout.len(), 3);
        assert_eq!(layout[0].height, 3);
        assert_eq!(layout[2].height, 1);
    }

    #[test]
    fn test_palette_accent_colors() {
        assert_eq!(TuiPalette::RUST, Color::Rgb(139, 69, 19));
        assert_eq!(TuiPalette::COPPER, Color::Rgb(184, 115, 51));
        assert_eq!(TuiPalette::SAGE, Color::Rgb(143, 151, 121));
        assert_eq!(TuiPalette::STONE, Color::Rgb(128, 128, 128));
    }

    #[test]
    fn test_palette_status_colors() {
        assert_eq!(TuiPalette::BLOOM, Color::Rgb(144, 190, 109));
        assert_eq!(TuiPalette::WILT, Color::Rgb(188, 108, 37));
        assert_eq!(TuiPalette::FROST, Color::Rgb(100, 100, 120));
    }

    #[test]
    fn test_palette_selected_style() {
        let style = TuiPalette::selected_style();
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_palette_success_style() {
        let style = TuiPalette::success_style();
        // Style should have a foreground color set
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_palette_warning_style() {
        let style = TuiPalette::warning_style();
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_palette_error_style() {
        let style = TuiPalette::error_style();
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_palette_muted_style() {
        let style = TuiPalette::muted_style();
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_palette_path_style() {
        let style = TuiPalette::path_style();
        assert!(style.fg.is_some());
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_palette_border_style() {
        let style = TuiPalette::border_style();
        assert!(style.fg.is_some());
    }

    #[test]
    fn test_palette_ink_parchment() {
        assert_eq!(TuiPalette::INK, Color::Rgb(40, 54, 24));
        assert_eq!(TuiPalette::PARCHMENT, Color::Rgb(254, 250, 224));
    }

    #[test]
    fn test_palette_soil_brown() {
        assert_eq!(TuiPalette::SOIL_BROWN, Color::Rgb(188, 108, 37));
    }

    #[test]
    fn test_is_quit_q() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(is_quit(&event));
    }

    #[test]
    fn test_is_quit_ctrl_c() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(is_quit(&event));
    }

    #[test]
    fn test_is_quit_other_key() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!is_quit(&event));
    }

    #[test]
    fn test_is_key_match() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(is_key(&event, KeyCode::Enter, KeyModifiers::NONE));
    }

    #[test]
    fn test_is_key_no_match_code() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(!is_key(&event, KeyCode::Esc, KeyModifiers::NONE));
    }

    #[test]
    fn test_is_key_no_match_modifiers() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(!is_key(&event, KeyCode::Char('a'), KeyModifiers::CONTROL));
    }

    #[test]
    fn test_is_key_with_ctrl() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let event = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
        assert!(is_key(&event, KeyCode::Char('p'), KeyModifiers::CONTROL));
    }

    #[test]
    fn test_standard_layout_large_area() {
        let area = Rect::new(0, 0, 200, 100);
        let layout = standard_layout(area);
        assert_eq!(layout.len(), 3);
        // Main content should get most space
        assert!(layout[1].height > layout[0].height);
        assert!(layout[1].height > layout[2].height);
    }

    #[test]
    fn test_split_layout_preserves_y() {
        let area = Rect::new(10, 20, 100, 50);
        let (left, right) = split_layout(area, 30);
        assert_eq!(left.y, 20);
        assert_eq!(right.y, 20);
    }

    #[test]
    fn test_palette_primary() {
        let primary = TuiPalette::primary();
        // Should return a valid RGB color
        assert!(matches!(primary, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_palette_accent() {
        let accent = TuiPalette::accent();
        assert!(matches!(accent, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_palette_tool() {
        let tool = TuiPalette::tool();
        assert!(matches!(tool, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_palette_path() {
        let path = TuiPalette::path();
        assert!(matches!(path, Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_create_event_channel() {
        let (tx, rx) = create_event_channel();
        // Should be able to send and receive
        tx.send(TuiEvent::Log {
            level: LogLevel::Info,
            message: "test".to_string(),
        })
        .unwrap();
        let event = rx.recv().unwrap();
        if let TuiEvent::Log { level, message } = event {
            assert_eq!(message, "test");
            assert!(matches!(level, LogLevel::Info));
        } else {
            panic!("Wrong event type");
        }
    }
}

/// Run the TUI in swarm mode
///
/// This creates a full terminal UI for visualizing and interacting with
/// the agent swarm system. Features include:
/// - Real-time agent status visualization
/// - Shared memory browser
/// - Task queue monitoring
/// - Decision/consensus tracking
/// - Event logging
///
/// Keyboard shortcuts:
/// - q / Ctrl+C: Quit
/// - ?: Toggle help overlay
/// - Space: Pause/resume
/// - r: Refresh swarm state
/// - t: Add sample task
/// - c: Create sample decision
/// - v: Cast sample vote
/// - Alt+1-3: Layout presets
/// - Tab: Cycle focus between panes
/// - z: Toggle zoom
pub fn run_tui_swarm() -> Result<()> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = SwarmApp::new();
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);

    loop {
        // Handle timing
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        // Render
        terminal.terminal().draw(|frame| {
            app.render(frame);
        })?;

        // Handle events
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if !app.handle_event(Event::Key(key)) {
                    break;
                }
            }
        }

        // Update on tick
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }

    terminal.restore()?;
    Ok(())
}

/// Run the TUI swarm with a custom swarm configuration
///
/// Similar to `run_tui_swarm` but allows specifying custom agent roles.
///
/// # Example
///
/// ```rust,no_run
/// use selfware::ui::tui::run_tui_swarm_with_roles;
/// use selfware::orchestration::swarm::AgentRole;
///
/// let roles = vec![
///     AgentRole::Architect,
///     AgentRole::Coder,
///     AgentRole::Tester,
/// ];
/// run_tui_swarm_with_roles(roles).unwrap();
/// ```
pub fn run_tui_swarm_with_roles(roles: Vec<crate::orchestration::swarm::AgentRole>) -> Result<()> {
    let mut terminal = TuiTerminal::new()?;
    let mut app = SwarmApp::with_config(roles);
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(100);

    loop {
        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        terminal.terminal().draw(|frame| {
            app.render(frame);
        })?;

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if !app.handle_event(Event::Key(key)) {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }

    terminal.restore()?;
    Ok(())
}
