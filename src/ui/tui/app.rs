//! Selfware TUI Application
//!
//! State machine for the terminal UI with multi-pane layouts.

// Feature-gated module - dead_code lint disabled at crate level

use super::{wrap_chat_message, CommandPalette, StatusLine, TuiPalette};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

/// Application state
#[derive(Debug, Clone, PartialEq)]
pub enum AppState {
    /// Normal chat mode
    Chatting,
    /// Running a task with progress
    RunningTask,
    /// Command palette is open
    Palette,
    /// Browsing files
    FileBrowser,
    /// Viewing help
    Help,
    /// Confirming an action
    Confirming(String),
    /// Viewing digital garden
    GardenView,
}

/// A chat message for display
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
}

/// Task progress for display
#[derive(Debug, Clone)]
pub struct TaskProgress {
    pub description: String,
    pub current_step: usize,
    pub total_steps: Option<usize>,
    pub current_action: String,
    pub elapsed_secs: u64,
}

/// Animation speed settings
pub const ANIMATION_SPEED_MIN: f64 = 0.25;
pub const ANIMATION_SPEED_MAX: f64 = 4.0;
pub const ANIMATION_SPEED_STEP: f64 = 0.25;
pub const ANIMATION_SPEED_DEFAULT: f64 = 1.0;

/// The main TUI application
pub struct App {
    /// Current state
    pub state: AppState,
    /// Chat messages
    pub messages: Vec<ChatMessage>,
    /// Current input buffer
    pub input: String,
    /// Cursor position in input
    pub cursor: usize,
    /// Command palette
    pub palette: CommandPalette,
    /// Task progress (if running)
    pub task_progress: Option<TaskProgress>,
    /// Status bar message
    pub status: String,
    /// Model name
    pub model: String,
    /// Whether we're connected
    pub connected: bool,
    /// Scroll offset for messages
    pub scroll: usize,
    /// Selected item in lists
    pub selected: usize,
    /// Animation speed multiplier (1.0 = normal, 2.0 = faster, 0.5 = slower)
    pub animation_speed: f64,
    /// Verbose output mode (toggled by /verbose and /compact)
    pub verbose: bool,
    /// Garden view for codebase visualization
    pub garden_view: super::GardenView,
    /// Intelligent status line
    pub status_line: StatusLine,
    /// Discovered skills for slash-command execution
    pub skill_registry: Option<crate::skills::SkillRegistry>,
    /// In-progress assistant response streamed token-by-token. Rendered as the
    /// newest (live) chat line while generating; committed to `messages` and
    /// cleared on completion.
    pub streaming_assistant: Option<String>,
}

impl App {
    /// Create a new app instance
    pub fn new(model: &str) -> Self {
        Self {
            state: AppState::Chatting,
            messages: vec![ChatMessage {
                role: MessageRole::System,
                content: "Welcome to your workshop. How can I help you tend your garden today?"
                    .into(),
                timestamp: chrono::Local::now().format("%H:%M").to_string(),
            }],
            input: String::new(),
            cursor: 0,
            palette: CommandPalette::new(),
            task_progress: None,
            status: "Ready".into(),
            model: model.into(),
            // Not connected until the model actually responds (see
            // append_streaming / add_assistant_message).
            connected: false,
            scroll: 0,
            selected: 0,
            animation_speed: ANIMATION_SPEED_DEFAULT,
            verbose: false,
            garden_view: super::GardenView::new(),
            status_line: StatusLine::with_session(model),
            skill_registry: None,
            streaming_assistant: None,
        }
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.into(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
        });
    }

    /// Append a streamed chunk to the in-progress assistant response.
    pub fn append_streaming(&mut self, text: &str) {
        // Streamed bytes from the model prove the connection is live.
        self.connected = true;
        self.streaming_assistant
            .get_or_insert_with(String::new)
            .push_str(text);
    }

    /// Clear the in-progress streaming buffer (called once the full message is
    /// committed on completion).
    pub fn clear_streaming(&mut self) {
        self.streaming_assistant = None;
    }

    /// Finalize the in-progress streamed turn: commit it as a static assistant
    /// message and clear the live buffer. Called at each turn boundary (tool
    /// start, new step) so a multi-turn run renders as separate messages
    /// instead of one ever-growing run-on block. No-op when the buffer is
    /// empty or whitespace-only.
    pub fn commit_streaming(&mut self) {
        if let Some(text) = self.streaming_assistant.take() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                self.add_assistant_message(trimmed);
            }
        }
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: &str) {
        // Receiving an assistant message means we reached the model.
        self.connected = true;
        self.messages.push(ChatMessage {
            role: MessageRole::Assistant,
            content: content.into(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
        });
    }

    /// Add a system message
    pub fn add_system_message(&mut self, content: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::System,
            content: content.into(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
        });
    }

    /// Add a tool output message
    pub fn add_tool_message(&mut self, tool_name: &str, output: &str) {
        self.messages.push(ChatMessage {
            role: MessageRole::Tool,
            content: format!("[{}] {}", tool_name, output),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
        });
    }

    /// Clear chat history (keeping a fresh system message).
    ///
    /// Also clears any in-progress streaming buffer so that a partial
    /// assistant response does not persist after the history is wiped.
    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.clear_streaming();
        // /clear resets the underlying model conversation (see the /clear
        // handler), so a "partial" clear that wiped only the transcript left
        // stale task progress and token/context counters on screen. Reset those
        // too so the cleared view truthfully reflects a fresh conversation.
        self.task_progress = None;
        self.state = AppState::Chatting;
        self.status_line.reset_usage();
        self.messages.push(ChatMessage {
            role: MessageRole::System,
            content: "Chat cleared.".into(),
            timestamp: chrono::Local::now().format("%H:%M").to_string(),
        });
        self.scroll = 0;
    }

    /// Set task progress
    pub fn set_progress(&mut self, progress: TaskProgress) {
        self.task_progress = Some(progress);
        self.state = AppState::RunningTask;
    }

    /// Update just the step counters of the progress gauge from a live
    /// "Step X/Y" signal, so the gauge advances during a run instead of being
    /// frozen at its initial value. Creates the progress entry if a run is
    /// active but none was set yet.
    pub fn update_step_progress(&mut self, current_step: usize, total_steps: usize) {
        match self.task_progress.as_mut() {
            Some(p) => {
                p.current_step = current_step;
                p.total_steps = Some(total_steps);
            }
            None => {
                self.set_progress(TaskProgress {
                    description: "Processing...".into(),
                    current_step,
                    total_steps: Some(total_steps),
                    current_action: "Working".into(),
                    elapsed_secs: 0,
                });
            }
        }
    }

    /// Clear task progress
    pub fn clear_progress(&mut self) {
        self.task_progress = None;
        self.state = AppState::Chatting;
    }

    /// Toggle command palette
    pub fn toggle_palette(&mut self) {
        self.state = if self.state == AppState::Palette {
            AppState::Chatting
        } else {
            AppState::Palette
        };
    }

    /// Render the application
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Handle garden view as full-screen overlay
        if self.state == AppState::GardenView {
            self.render_header(
                frame,
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3)])
                    .split(area)[0],
            );

            let main_area = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header (already rendered)
                    Constraint::Min(10),   // Garden view
                    Constraint::Length(1), // Status bar
                ])
                .split(area)[1];

            self.garden_view.render(frame, main_area);

            let status_area = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(10),
                    Constraint::Length(1),
                ])
                .split(area)[2];
            self.render_status_bar(frame, status_area);
            return;
        }

        // Create main layout
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(10),   // Main content
                Constraint::Length(3), // Input area
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        self.render_header(frame, chunks[0]);
        self.render_messages(frame, chunks[1]);
        self.render_input(frame, chunks[2]);
        self.render_status_bar(frame, chunks[3]);

        // Render overlay if palette is open
        if self.state == AppState::Palette {
            self.render_palette(frame, area);
        }
    }

    /// Build the header title, appending live step progress while a task runs.
    fn header_title(&self) -> String {
        match (&self.state, self.task_progress.as_ref()) {
            (AppState::RunningTask, Some(p)) => {
                let steps = match p.total_steps {
                    Some(total) => format!("{}/{}", p.current_step, total),
                    None => p.current_step.to_string(),
                };
                format!(
                    " 🦊 Selfware Workshop — {} · step {} · {} ",
                    self.model, steps, p.current_action
                )
            }
            _ => format!(" 🦊 Selfware Workshop — {} ", self.model),
        }
    }

    /// Render the header
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = self.header_title();
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(TuiPalette::border_style())
            .title(Span::styled(title, TuiPalette::title_style()));

        frame.render_widget(block, area);
    }

    /// Render chat messages
    fn render_messages(&self, frame: &mut Frame, area: Rect) {
        // Messages pane is "focused" when we're not in palette or other overlay modes
        let is_focused = self.state == AppState::Chatting || self.state == AppState::RunningTask;
        let border_style = if is_focused {
            TuiPalette::border_style()
        } else {
            TuiPalette::muted_style()
        };

        let title = if is_focused {
            " Messages ".into()
        } else {
            format!(" Messages [{}] ", self.state_label())
        };

        let inner = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, border_style));

        let inner_area = inner.inner(area);
        frame.render_widget(inner, area);

        // Build message list with line wrapping
        let msg_width = inner_area.width as usize;
        let items: Vec<ListItem> = self
            .messages
            .iter()
            .rev()
            .skip(self.scroll)
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

                let prefix_str = format!("{} {} ", msg.timestamp, prefix);
                wrap_chat_message(&prefix_str, &msg.content, style, msg_width)
            })
            .collect();

        let messages = List::new(items);
        frame.render_widget(messages, inner_area);
    }

    /// Render input area
    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let is_focused = self.state == AppState::Chatting;
        let border_style = if is_focused {
            Style::default()
                .fg(TuiPalette::AMBER)
                .add_modifier(Modifier::BOLD)
        } else {
            TuiPalette::muted_style()
        };

        let title = if is_focused {
            " Input ".into()
        } else {
            format!(" Input [{}] ", self.state_label())
        };

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(title, border_style));

        let inner = input_block.inner(area);
        frame.render_widget(input_block, area);

        let input_text = if is_focused {
            Paragraph::new(format!("❯ {}", self.input))
                .style(Style::default().fg(TuiPalette::PARCHMENT))
        } else {
            Paragraph::new(" — ".to_string()).style(TuiPalette::muted_style())
        };
        frame.render_widget(input_text, inner);

        // Show cursor only when focused
        if is_focused {
            frame.set_cursor_position((inner.x + 2 + self.cursor as u16, inner.y));
        }
    }

    /// Get a short label for the current state
    fn state_label(&self) -> &'static str {
        match self.state {
            AppState::Chatting => "chat",
            AppState::RunningTask => "task",
            AppState::Palette => "palette",
            AppState::FileBrowser => "files",
            AppState::Help => "help",
            AppState::Confirming(_) => "confirm",
            AppState::GardenView => "garden",
        }
    }

    /// Render status bar
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        self.status_line.render(frame, area);
    }

    /// Update the status line with current app state
    pub fn refresh_status_line(&mut self) {
        self.status_line.connected = self.connected;
        self.status_line.status_message = Some(self.status.clone());
    }

    /// Render command palette overlay
    fn render_palette(&self, frame: &mut Frame, area: Rect) {
        // Center the palette
        let palette_width = 60.min(area.width - 4);
        let palette_height = 15.min(area.height - 4);
        let x = (area.width - palette_width) / 2;
        let y = (area.height - palette_height) / 3;

        let palette_area = Rect::new(x, y, palette_width, palette_height);

        // Clear background
        let clear = Block::default().style(Style::default().bg(TuiPalette::INK));
        frame.render_widget(clear, palette_area);

        // Render palette
        self.palette.render(frame, palette_area, self.selected);
    }

    /// Byte offset in `self.input` for the current char-index cursor.
    fn cursor_byte_offset(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    /// Handle character input
    pub fn on_char(&mut self, c: char) {
        if self.state == AppState::Chatting {
            let byte_idx = self.cursor_byte_offset();
            self.input.insert(byte_idx, c);
            self.cursor += 1;
        } else if self.state == AppState::Palette {
            self.palette.on_char(c);
        }
    }

    /// Handle backspace
    pub fn on_backspace(&mut self) {
        if self.state == AppState::Chatting && self.cursor > 0 {
            self.cursor -= 1;
            let byte_idx = self.cursor_byte_offset();
            self.input.remove(byte_idx);
        } else if self.state == AppState::Palette {
            self.palette.on_backspace();
        }
    }

    /// Handle left arrow
    pub fn on_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Handle right arrow
    pub fn on_right(&mut self) {
        // Bound by CHARACTER count (cursor is a char index), not byte length.
        if self.cursor < self.input.chars().count() {
            self.cursor += 1;
        }
    }

    /// Handle up arrow
    pub fn on_up(&mut self) {
        if self.state == AppState::Palette {
            self.palette.previous();
        } else if self.scroll + 1 < self.messages.len() {
            self.scroll += 1;
        }
    }

    /// Handle down arrow
    pub fn on_down(&mut self) {
        if self.state == AppState::Palette {
            self.palette.next();
        } else if self.scroll > 0 {
            self.scroll -= 1;
        }
    }

    /// Handle enter key
    pub fn on_enter(&mut self) -> Option<String> {
        if self.state == AppState::Palette {
            if let Some(cmd) = self.palette.selected_command() {
                self.state = AppState::Chatting;
                return Some(cmd);
            }
            None
        } else if !self.input.is_empty() {
            let input = std::mem::take(&mut self.input);
            self.cursor = 0;
            Some(input)
        } else {
            None
        }
    }

    /// Handle escape key
    ///
    /// Esc behavior hierarchy:
    /// 1. If palette is open -> close palette and reset it
    /// 2. If confirming -> cancel confirmation
    /// 3. If running task -> cancel/close task view
    /// 4. If file browser -> close file browser
    /// 5. If help -> close help
    /// 6. If chatting with input -> clear input
    /// 7. If chatting (clean state) -> show exit confirmation
    pub fn on_escape(&mut self) {
        match self.state {
            AppState::Palette => {
                self.palette.reset();
                self.state = AppState::Chatting;
                self.status = "Command palette closed".into();
            }
            AppState::Confirming(_) => {
                self.state = AppState::Chatting;
                self.status = "Cancelled".into();
            }
            AppState::RunningTask => {
                self.task_progress = None;
                self.state = AppState::Chatting;
                self.status = "Task view closed".into();
            }
            AppState::FileBrowser => {
                self.state = AppState::Chatting;
                self.status = "File browser closed".into();
            }
            AppState::Help => {
                self.state = AppState::Chatting;
                self.status = "Help closed".into();
            }
            AppState::GardenView => {
                self.state = AppState::Chatting;
                self.status = "Garden view closed".into();
            }
            AppState::Chatting => {
                if !self.input.is_empty() {
                    // First escape clears input
                    self.input.clear();
                    self.cursor = 0;
                    self.status = "Input cleared".into();
                } else {
                    // Clean state - show exit confirmation
                    self.state = AppState::Confirming("Press Enter to exit, Esc to cancel".into());
                    self.status = "Exit?".into();
                }
            }
        }
    }

    /// Increase animation speed (+ key)
    pub fn on_plus(&mut self) {
        self.animation_speed =
            (self.animation_speed + ANIMATION_SPEED_STEP).min(ANIMATION_SPEED_MAX);
        self.status = format!("Animation speed: {:.0}%", self.animation_speed * 100.0);
    }

    /// Decrease animation speed (- key)
    pub fn on_minus(&mut self) {
        self.animation_speed =
            (self.animation_speed - ANIMATION_SPEED_STEP).max(ANIMATION_SPEED_MIN);
        self.status = format!("Animation speed: {:.0}%", self.animation_speed * 100.0);
    }

    /// Get animation delay based on current speed
    /// Returns the delay in milliseconds to use between animation frames
    pub fn animation_delay_ms(&self) -> u64 {
        // Base delay is 100ms, adjusted by speed (faster = shorter delay)
        let base_delay = 100.0;
        (base_delay / self.animation_speed) as u64
    }

    /// Get animation speed as percentage string
    pub fn animation_speed_display(&self) -> String {
        format!("{:.0}%", self.animation_speed * 100.0)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/app/app_test.rs"]
mod tests;
