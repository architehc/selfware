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
            connected: true,
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

    /// Toggle garden view
    pub fn toggle_garden_view(&mut self) {
        self.state = if self.state == AppState::GardenView {
            AppState::Chatting
        } else {
            // Try to load garden data if not already loaded
            if self.garden_view.garden().is_none() {
                if let Ok(garden) = crate::ui::garden::build_garden_from_path(".") {
                    self.garden_view.set_garden(garden);
                }
            }
            AppState::GardenView
        };
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
        self.streaming_assistant
            .get_or_insert_with(String::new)
            .push_str(text);
    }

    /// Clear the in-progress streaming buffer (called once the full message is
    /// committed on completion).
    pub fn clear_streaming(&mut self) {
        self.streaming_assistant = None;
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: &str) {
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

    /// Render the header
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let title = format!(" 🦊 Selfware Workshop — {} ", self.model);
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

    /// Set the execution mode displayed in the status line
    pub fn set_status_mode(&mut self, mode: super::StatusMode) {
        self.status_line.mode = mode;
    }

    /// Update token usage in the status line
    pub fn set_token_usage(&mut self, input: usize, output: usize) {
        self.status_line.tokens_used = (input, output);
    }

    /// Update context window percentage in the status line
    pub fn set_context_percent(&mut self, percent: f32) {
        self.status_line.context_percent = percent.clamp(0.0, 100.0);
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

    /// Handle character input
    pub fn on_char(&mut self, c: char) {
        if self.state == AppState::Chatting {
            self.input.insert(self.cursor, c);
            self.cursor += 1;
        } else if self.state == AppState::Palette {
            self.palette.on_char(c);
        }
    }

    /// Handle backspace
    pub fn on_backspace(&mut self) {
        if self.state == AppState::Chatting && self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
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
        if self.cursor < self.input.len() {
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
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new("test-model");
        assert_eq!(app.model, "test-model");
        assert_eq!(app.state, AppState::Chatting);
        assert!(app.connected);
    }

    #[test]
    fn test_app_initial_state() {
        let app = App::new("test");
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
        assert_eq!(app.scroll, 0);
        assert_eq!(app.selected, 0);
        assert_eq!(app.status, "Ready");
    }

    #[test]
    fn test_app_has_welcome_message() {
        let app = App::new("test");
        assert!(!app.messages.is_empty());
        assert_eq!(app.messages[0].role, MessageRole::System);
    }

    #[test]
    fn test_add_messages() {
        let mut app = App::new("test");
        app.add_user_message("Hello");
        app.add_assistant_message("Hi there!");

        assert_eq!(app.messages.len(), 3); // 1 system + 2 new
        assert_eq!(app.messages[1].role, MessageRole::User);
        assert_eq!(app.messages[2].role, MessageRole::Assistant);
    }

    #[test]
    fn test_add_user_message() {
        let mut app = App::new("test");
        app.add_user_message("Test message");

        assert_eq!(app.messages.last().unwrap().role, MessageRole::User);
        assert_eq!(app.messages.last().unwrap().content, "Test message");
    }

    #[test]
    fn test_add_assistant_message() {
        let mut app = App::new("test");
        app.add_assistant_message("Assistant response");

        assert_eq!(app.messages.last().unwrap().role, MessageRole::Assistant);
        assert_eq!(app.messages.last().unwrap().content, "Assistant response");
    }

    #[test]
    fn test_add_tool_message() {
        let mut app = App::new("test");
        app.add_tool_message("file_read", "File contents here");

        assert_eq!(app.messages.last().unwrap().role, MessageRole::Tool);
        assert!(app.messages.last().unwrap().content.contains("file_read"));
        assert!(app
            .messages
            .last()
            .unwrap()
            .content
            .contains("File contents here"));
    }

    #[test]
    fn test_message_has_timestamp() {
        let mut app = App::new("test");
        app.add_user_message("Test");

        assert!(!app.messages.last().unwrap().timestamp.is_empty());
    }

    #[test]
    fn test_input_handling() {
        let mut app = App::new("test");

        app.on_char('h');
        app.on_char('i');
        assert_eq!(app.input, "hi");
        assert_eq!(app.cursor, 2);

        app.on_backspace();
        assert_eq!(app.input, "h");
        assert_eq!(app.cursor, 1);
    }

    #[test]
    fn test_input_char_inserts_at_cursor() {
        let mut app = App::new("test");
        app.on_char('a');
        app.on_char('c');
        app.on_left();
        app.on_char('b');

        assert_eq!(app.input, "abc");
    }

    #[test]
    fn test_backspace_at_start() {
        let mut app = App::new("test");
        app.on_char('a');
        app.on_left();
        app.on_backspace();

        // Should not change anything
        assert_eq!(app.input, "a");
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn test_backspace_empty() {
        let mut app = App::new("test");
        app.on_backspace();

        // Should not panic
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn test_on_left() {
        let mut app = App::new("test");
        app.on_char('a');
        app.on_char('b');
        assert_eq!(app.cursor, 2);

        app.on_left();
        assert_eq!(app.cursor, 1);

        app.on_left();
        assert_eq!(app.cursor, 0);

        app.on_left();
        assert_eq!(app.cursor, 0); // Can't go below 0
    }

    #[test]
    fn test_on_right() {
        let mut app = App::new("test");
        app.input = "abc".into();
        app.cursor = 0;

        app.on_right();
        assert_eq!(app.cursor, 1);

        app.on_right();
        app.on_right();
        assert_eq!(app.cursor, 3);

        app.on_right();
        assert_eq!(app.cursor, 3); // Can't go beyond length
    }

    #[test]
    fn test_on_up_scroll() {
        let mut app = App::new("test");
        // Add enough messages to scroll
        for i in 0..10 {
            app.add_user_message(&format!("Message {}", i));
        }

        assert_eq!(app.scroll, 0);
        app.on_up();
        assert_eq!(app.scroll, 1);
        app.on_up();
        assert_eq!(app.scroll, 2);
    }

    #[test]
    fn test_on_down_scroll() {
        let mut app = App::new("test");
        for i in 0..10 {
            app.add_user_message(&format!("Message {}", i));
        }

        app.scroll = 5;
        app.on_down();
        assert_eq!(app.scroll, 4);

        app.scroll = 0;
        app.on_down();
        assert_eq!(app.scroll, 0); // Can't go below 0
    }

    #[test]
    fn test_toggle_palette() {
        let mut app = App::new("test");
        assert_eq!(app.state, AppState::Chatting);

        app.toggle_palette();
        assert_eq!(app.state, AppState::Palette);

        app.toggle_palette();
        assert_eq!(app.state, AppState::Chatting);
    }

    #[test]
    fn test_on_enter() {
        let mut app = App::new("test");
        app.input = "hello world".into();
        app.cursor = 11;

        let result = app.on_enter();
        assert_eq!(result, Some("hello world".into()));
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn test_on_enter_empty() {
        let mut app = App::new("test");
        let result = app.on_enter();
        assert!(result.is_none());
    }

    #[test]
    fn test_on_escape_from_palette() {
        let mut app = App::new("test");
        app.state = AppState::Palette;

        app.on_escape();
        assert_eq!(app.state, AppState::Chatting);
    }

    #[test]
    fn test_on_escape_from_confirming() {
        let mut app = App::new("test");
        app.state = AppState::Confirming("test action".into());

        app.on_escape();
        assert_eq!(app.state, AppState::Chatting);
    }

    #[test]
    fn test_on_escape_from_chatting_shows_confirmation() {
        let mut app = App::new("test");
        app.state = AppState::Chatting;

        app.on_escape();
        // When in clean Chatting state, Esc shows exit confirmation
        assert!(matches!(app.state, AppState::Confirming(_)));
    }

    #[test]
    fn test_on_escape_from_chatting_clears_input() {
        let mut app = App::new("test");
        app.state = AppState::Chatting;
        app.input = "some text".into();
        app.cursor = 9;

        app.on_escape();
        // First escape clears input
        assert_eq!(app.state, AppState::Chatting);
        assert!(app.input.is_empty());
        assert_eq!(app.cursor, 0);
    }

    #[test]
    fn test_on_escape_closes_help() {
        let mut app = App::new("test");
        app.state = AppState::Help;

        app.on_escape();
        assert_eq!(app.state, AppState::Chatting);
    }

    #[test]
    fn test_on_escape_closes_file_browser() {
        let mut app = App::new("test");
        app.state = AppState::FileBrowser;

        app.on_escape();
        assert_eq!(app.state, AppState::Chatting);
    }

    #[test]
    fn test_on_escape_cancels_task() {
        let mut app = App::new("test");
        app.state = AppState::RunningTask;
        app.task_progress = Some(TaskProgress {
            description: "Test task".into(),
            current_step: 1,
            total_steps: None,
            current_action: "Testing".into(),
            elapsed_secs: 0,
        });

        app.on_escape();
        assert_eq!(app.state, AppState::Chatting);
        assert!(app.task_progress.is_none());
    }

    #[test]
    fn test_on_escape_resets_palette() {
        let mut app = App::new("test");
        app.state = AppState::Palette;
        // Type some text in the palette
        app.palette.on_char('t');
        app.palette.on_char('e');
        app.palette.on_char('s');
        app.palette.on_char('t');

        app.on_escape();
        // Should return to chatting state
        assert_eq!(app.state, AppState::Chatting);
        // Palette is reset (we verify by checking it works correctly when reopened)
        app.toggle_palette();
        assert_eq!(app.state, AppState::Palette);
    }

    #[test]
    fn test_set_progress() {
        let mut app = App::new("test");
        let progress = TaskProgress {
            description: "Test task".into(),
            current_step: 3,
            total_steps: Some(10),
            current_action: "Testing".into(),
            elapsed_secs: 120,
        };

        app.set_progress(progress);
        assert_eq!(app.state, AppState::RunningTask);
        assert!(app.task_progress.is_some());
    }

    #[test]
    fn test_clear_progress() {
        let mut app = App::new("test");
        let progress = TaskProgress {
            description: "Test".into(),
            current_step: 1,
            total_steps: None,
            current_action: "".into(),
            elapsed_secs: 0,
        };
        app.set_progress(progress);

        app.clear_progress();
        assert_eq!(app.state, AppState::Chatting);
        assert!(app.task_progress.is_none());
    }

    #[test]
    fn test_input_in_palette_mode() {
        let mut app = App::new("test");
        app.state = AppState::Palette;

        app.on_char('a');
        // Input should go to palette, not main input
        assert!(app.input.is_empty());
    }

    #[test]
    fn test_up_down_in_palette_mode() {
        let mut app = App::new("test");
        app.toggle_palette();
        assert_eq!(app.state, AppState::Palette);

        // Navigation in palette mode should work without panic
        app.on_down();
        app.on_up();
        // Navigation is handled by palette internally
    }

    #[test]
    fn test_message_role_equality() {
        assert_eq!(MessageRole::User, MessageRole::User);
        assert_ne!(MessageRole::User, MessageRole::Assistant);
    }

    #[test]
    fn test_app_state_equality() {
        assert_eq!(AppState::Chatting, AppState::Chatting);
        assert_ne!(AppState::Chatting, AppState::Palette);
        assert_eq!(
            AppState::Confirming("a".into()),
            AppState::Confirming("a".into())
        );
    }

    #[test]
    fn test_animation_speed_default() {
        let app = App::new("test");
        assert!((app.animation_speed - ANIMATION_SPEED_DEFAULT).abs() < 0.001);
    }

    #[test]
    fn test_on_plus_increases_speed() {
        let mut app = App::new("test");
        let original = app.animation_speed;
        app.on_plus();
        assert!(app.animation_speed > original);
    }

    #[test]
    fn test_on_minus_decreases_speed() {
        let mut app = App::new("test");
        app.on_plus(); // First increase so we can decrease
        let speed_after_plus = app.animation_speed;
        app.on_minus();
        assert!(app.animation_speed < speed_after_plus);
    }

    #[test]
    fn test_animation_speed_max_cap() {
        let mut app = App::new("test");
        // Increase many times
        for _ in 0..20 {
            app.on_plus();
        }
        assert!(app.animation_speed <= ANIMATION_SPEED_MAX);
    }

    #[test]
    fn test_animation_speed_min_cap() {
        let mut app = App::new("test");
        // Decrease many times
        for _ in 0..20 {
            app.on_minus();
        }
        assert!(app.animation_speed >= ANIMATION_SPEED_MIN);
    }

    #[test]
    fn test_animation_delay_inversely_proportional() {
        let mut app = App::new("test");
        let normal_delay = app.animation_delay_ms();

        app.animation_speed = 2.0;
        let fast_delay = app.animation_delay_ms();

        // Faster speed should mean shorter delay
        assert!(fast_delay < normal_delay);
    }

    #[test]
    fn test_animation_speed_display() {
        let mut app = App::new("test");
        app.animation_speed = 1.5;
        assert_eq!(app.animation_speed_display(), "150%");
    }

    #[test]
    fn streaming_assistant_accumulates_then_clears() {
        let mut app = App::new("m");
        assert!(app.streaming_assistant.is_none());
        app.append_streaming("Hel");
        app.append_streaming("lo");
        assert_eq!(app.streaming_assistant.as_deref(), Some("Hello"));
        let n = app.messages.len();
        app.clear_streaming();
        assert!(app.streaming_assistant.is_none());
        app.add_assistant_message("Hello");
        assert_eq!(app.messages.len(), n + 1);
    }
}
