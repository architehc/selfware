//! Modern Input System for Selfware
//!
//! Rich terminal input with autocomplete, history, and vim keybindings.
//! Built on reedline for a professional IDE-like experience.

pub mod command_registry;
mod completer;
mod highlighter;
mod prompt;
mod submit_menu;

pub use completer::SelfwareCompleter;
pub use highlighter::SelfwareHighlighter;
pub use prompt::SelfwarePrompt;
pub use submit_menu::SubmitOnExactAcceptMenu;

use anyhow::Result;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultHinter, DefaultValidator, EditCommand, Emacs,
    FileBackedHistory, KeyCode, KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent,
    ReedlineMenu, Signal, Vi,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Input mode for the editor
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum InputMode {
    #[default]
    Emacs,
    Vi,
}

/// Configuration for the input system
#[derive(Debug, Clone)]
pub struct InputConfig {
    /// Keybinding mode (emacs or vi)
    pub mode: InputMode,
    /// Path to history file
    pub history_path: Option<PathBuf>,
    /// Maximum history entries
    pub max_history: usize,
    /// Enable syntax highlighting
    pub syntax_highlight: bool,
    /// Show inline hints
    pub show_hints: bool,
    /// Available tool names for completion
    pub tool_names: Vec<String>,
    /// Available commands for completion
    pub commands: Vec<String>,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            mode: InputMode::Emacs,
            history_path: dirs_history_path(),
            max_history: 10000,
            syntax_highlight: true,
            show_hints: true,
            tool_names: vec![],
            commands: command_registry::command_names(),
        }
    }
}

/// Get the default history path
fn dirs_history_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|p| p.join("selfware").join("history.txt"))
}

/// Modern line editor with IDE-like features
pub struct SelfwareEditor {
    editor: Reedline,
    prompt: SelfwarePrompt,
    config: InputConfig,
    /// Set by the completion menu when an Enter-accept left the line
    /// unchanged (see `submit_menu`): the line is then submitted.
    exact_accept: Arc<AtomicBool>,
}

impl SelfwareEditor {
    /// Create a new editor with configuration
    pub fn new(config: InputConfig) -> Result<Self> {
        // Set up history
        let history = if let Some(path) = &config.history_path {
            // Ensure parent directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            Box::new(FileBackedHistory::with_file(
                config.max_history,
                path.clone(),
            )?)
        } else {
            Box::new(FileBackedHistory::new(config.max_history)?)
        };

        // Set up completer
        let completer = Box::new(SelfwareCompleter::new(
            config.tool_names.clone(),
            config.commands.clone(),
        ));

        // Set up highlighter
        let highlighter = Box::new(SelfwareHighlighter::new());

        // Set up hinter
        let hinter = Box::new(DefaultHinter::default());

        // Set up completion menu - IDE style that cycles with Tab. Wrapped so
        // Enter on an already-complete line submits it (see submit_menu).
        let exact_accept = Arc::new(AtomicBool::new(false));
        let completion_menu = Box::new(SubmitOnExactAcceptMenu::new(
            ColumnarMenu::default()
                .with_name("completion_menu")
                .with_columns(1) // Single column for clearer selection
                .with_column_padding(2)
                .with_marker(" > "), // Show selection marker
            exact_accept.clone(),
        ));

        // Set up keybindings
        let keybindings = Self::build_keybindings(config.mode);

        // Build the editor
        let edit_mode: Box<dyn reedline::EditMode> = match config.mode {
            InputMode::Emacs => Box::new(Emacs::new(keybindings)),
            InputMode::Vi => Box::new(Vi::default()),
        };

        // Set up validator
        let validator = Box::new(DefaultValidator);

        // Configure external editor for Ctrl+X
        let editor_cmd = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".to_string());
        let temp_file =
            std::env::temp_dir().join(format!("selfware_edit_{}.tmp", std::process::id()));
        let buffer_editor = std::process::Command::new(editor_cmd);

        let mut editor = Reedline::create()
            .with_history(history)
            .with_completer(completer)
            .with_quick_completions(true)
            .with_partial_completions(true)
            .with_hinter(hinter)
            .with_highlighter(highlighter)
            .with_validator(validator)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(edit_mode)
            .with_buffer_editor(buffer_editor, temp_file);

        // Add Ctrl+R for history search
        editor = editor.with_history_exclusion_prefix(Some(" ".into()));

        let prompt = SelfwarePrompt::new();

        Ok(Self {
            editor,
            prompt,
            config,
            exact_accept,
        })
    }

    /// Build keybindings for the given mode
    fn build_keybindings(mode: InputMode) -> Keybindings {
        let mut keybindings = match mode {
            InputMode::Emacs => default_emacs_keybindings(),
            InputMode::Vi => Keybindings::default(),
        };

        // Tab for completion
        // - First Tab: complete if single match, otherwise open menu
        // - Subsequent Tabs: cycle through menu items
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Tab,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete, // Complete history hint first
                ReedlineEvent::Edit(vec![EditCommand::Complete]), // Try inline completion
                ReedlineEvent::Menu("completion_menu".to_string()), // Open menu for visibility
                ReedlineEvent::MenuNext,            // Then cycle entries
            ]),
        );

        // Typing "/" opens slash command menu (Qwen-style)
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Char('/'),
            ReedlineEvent::Multiple(vec![
                ReedlineEvent::Edit(vec![EditCommand::InsertChar('/')]),
                ReedlineEvent::Menu("completion_menu".to_string()),
            ]),
        );

        // Enter: with the completion menu open, reedline's Enter only accepts
        // the selection — `/quit` + Enter left `/quit ` in the buffer and the
        // session running. The host command that follows lets `read_line`
        // submit the line when that accept changed nothing (the user had
        // already typed the whole command). With no menu open, Enter submits
        // and the host command is never reached.
        keybindings.add_binding(KeyModifiers::NONE, KeyCode::Enter, enter_event());

        // Shift+Tab to cycle execution mode: normal → auto-edit → yolo → daemon → normal
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::BackTab,
            ReedlineEvent::ExecuteHostCommand("__cycle_mode__".to_string()),
        );

        // Escape to close menu without selecting
        keybindings.add_binding(KeyModifiers::NONE, KeyCode::Esc, ReedlineEvent::Esc);

        // Right arrow accepts the current hint/suggestion
        keybindings.add_binding(
            KeyModifiers::NONE,
            KeyCode::Right,
            ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::Edit(vec![EditCommand::MoveRight { select: false }]),
            ]),
        );

        // Ctrl+J to insert newline (multi-line input)
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('j'),
            ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
        );

        // Ctrl+Y to toggle YOLO mode (via host command)
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('y'),
            ReedlineEvent::ExecuteHostCommand("__toggle_yolo__".to_string()),
        );

        // Ctrl+X to open external editor
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char('x'),
            ReedlineEvent::OpenEditor,
        );

        // Ctrl+Space for command palette (we'll handle this in the app)
        keybindings.add_binding(
            KeyModifiers::CONTROL,
            KeyCode::Char(' '),
            ReedlineEvent::Edit(vec![EditCommand::InsertString("".into())]),
        );

        keybindings
    }

    /// Read a line from the user
    pub fn read_line(&mut self) -> Result<ReadlineResult> {
        loop {
            let signal = self.editor.read_line(&self.prompt);
            if matches!(&signal, Ok(Signal::Success(line)) if line == submit_menu::MENU_ACCEPT_HOST_COMMAND)
            {
                if !self.exact_accept.swap(false, Ordering::SeqCst) {
                    // The accept completed something (or Enter only inserted
                    // a continuation newline): keep editing.
                    continue;
                }
                return Ok(self.submit_accepted_line());
            }
            return classify_signal(signal);
        }
    }

    /// Submit the current buffer after an Enter whose menu accept left it
    /// unchanged — what Enter does without a menu: the line is returned,
    /// recorded in history, and the editor starts empty next time.
    fn submit_accepted_line(&mut self) -> ReadlineResult {
        use std::io::Write;
        let line = self.editor.current_buffer_contents().trim_end().to_string();
        self.editor.run_edit_commands(&[EditCommand::Clear]);
        if !line.trim().is_empty() {
            let _ = self
                .editor
                .history_mut()
                .save(reedline::HistoryItem::from_command_line(line.clone()));
        }
        // The editor exited mid-line (host command): clear the closed menu
        // below the cursor and move to a fresh line, as a submit would.
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b[J\r\n");
        let _ = out.flush();
        ReadlineResult::Line(line)
    }

    /// Update the prompt with full context including token usage
    pub fn set_prompt_full_context(&mut self, model: &str, step: usize, context_pct: f64) {
        self.prompt = SelfwarePrompt::with_full_context(model, step, context_pct);
    }

    /// Toggle between Emacs and Vi mode, returns the new mode
    pub fn toggle_vim_mode(&mut self) -> Result<InputMode> {
        let new_mode = match self.config.mode {
            InputMode::Emacs => InputMode::Vi,
            InputMode::Vi => InputMode::Emacs,
        };
        self.config.mode = new_mode;

        // Rebuild the editor with new mode
        let new_editor = SelfwareEditor::new(self.config.clone())?;
        self.editor = new_editor.editor;
        Ok(new_mode)
    }
}

/// The Enter binding: reedline's Enter (submit, or accept the open menu's
/// selection), then the host command that lets `read_line` submit a line the
/// accept left unchanged. When Enter submits, `Multiple` stops there.
fn enter_event() -> ReedlineEvent {
    ReedlineEvent::Multiple(vec![
        ReedlineEvent::Enter,
        ReedlineEvent::ExecuteHostCommand(submit_menu::MENU_ACCEPT_HOST_COMMAND.to_string()),
    ])
}

/// Map a reedline signal to a [`ReadlineResult`].
fn classify_signal(signal: std::io::Result<Signal>) -> Result<ReadlineResult> {
    match signal {
        Ok(Signal::Success(line)) => {
            // Detect sentinel values from ExecuteHostCommand keybindings
            if line.starts_with("__") && line.ends_with("__") {
                Ok(ReadlineResult::HostCommand(line))
            } else {
                Ok(ReadlineResult::Line(line))
            }
        }
        Ok(Signal::CtrlC) => Ok(ReadlineResult::Interrupt),
        Ok(Signal::CtrlD) => Ok(ReadlineResult::Eof),
        Err(e) => Err(e.into()),
    }
}

/// Result of reading a line
#[derive(Debug)]
pub enum ReadlineResult {
    /// A line was entered
    Line(String),
    /// Ctrl+C was pressed
    Interrupt,
    /// Ctrl+D was pressed (EOF)
    Eof,
    /// Host command triggered by keybinding (e.g., "__toggle_yolo__")
    HostCommand(String),
}

#[cfg(test)]
#[path = "../../tests/unit/input/mod_test.rs"]
mod tests;
