//! Modern Input System for Selfware
//!
//! Rich terminal input with autocomplete, history, and vim keybindings.
//! Built on reedline for a professional IDE-like experience.

mod completer;
mod ghost_text;
mod highlighter;
mod prompt;
mod validator;

pub use completer::SelfwareCompleter;
pub use ghost_text::GhostTextHinter;
pub use highlighter::SelfwareHighlighter;
pub use prompt::SelfwarePrompt;
pub use validator::BracketValidator;

use anyhow::Result;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, EditCommand, Emacs, FileBackedHistory, KeyCode,
    KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent, ReedlineMenu, Signal, Vi,
};
use std::path::PathBuf;

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
            commands: vec![
                "/help".into(),
                "/status".into(),
                "/stats".into(),
                "/mode".into(),
                "/ctx".into(),
                "/ctx clear".into(),
                "/ctx load".into(),
                "/ctx reload".into(),
                "/ctx copy".into(),
                "/compress".into(),
                "/context".into(),
                "/memory".into(),
                "/clear".into(),
                "/tools".into(),
                "/analyze".into(),
                "/review".into(),
                "/plan".into(),
                "/garden".into(),
                "/journal".into(),
                "/palette".into(),
                "exit".into(),
                "quit".into(),
            ],
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

        // Set up hinter with ghost text support
        let hinter = Box::new(GhostTextHinter::new());

        // Set up completion menu - IDE style that cycles with Tab
        let completion_menu = Box::new(
            ColumnarMenu::default()
                .with_name("completion_menu")
                .with_columns(1) // Single column for clearer selection
                .with_column_padding(2)
                .with_marker(" > "), // Show selection marker
        );

        // Set up keybindings
        let keybindings = Self::build_keybindings(config.mode);

        // Build the editor
        let edit_mode: Box<dyn reedline::EditMode> = match config.mode {
            InputMode::Emacs => Box::new(Emacs::new(keybindings)),
            InputMode::Vi => Box::new(Vi::default()),
        };

        // Set up bracket validator
        let validator = Box::new(BracketValidator::new());

        let mut editor = Reedline::create()
            .with_history(history)
            .with_completer(completer)
            .with_quick_completions(true)
            .with_partial_completions(true)
            .with_hinter(hinter)
            .with_highlighter(highlighter)
            .with_validator(validator)
            .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
            .with_edit_mode(edit_mode);

        // Add Ctrl+R for history search
        editor = editor.with_history_exclusion_prefix(Some(" ".into()));

        let prompt = SelfwarePrompt::new();

        Ok(Self { editor, prompt })
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
                ReedlineEvent::MenuNext,            // If menu open, go to next item
                ReedlineEvent::Menu("completion_menu".to_string()), // Otherwise open menu
            ]),
        );

        // Shift+Tab to go backwards in menu
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::BackTab,
            ReedlineEvent::MenuPrevious,
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
        match self.editor.read_line(&self.prompt) {
            Ok(Signal::Success(line)) => Ok(ReadlineResult::Line(line)),
            Ok(Signal::CtrlC) => Ok(ReadlineResult::Interrupt),
            Ok(Signal::CtrlD) => Ok(ReadlineResult::Eof),
            Err(e) => Err(e.into()),
        }
    }

    /// Update the prompt context
    pub fn set_prompt_context(&mut self, model: &str, step: usize) {
        self.prompt = SelfwarePrompt::with_context(model, step);
    }

    /// Add tool names for completion
    pub fn add_tools(&mut self, tools: Vec<String>) {
        // Note: We'd need to rebuild the completer for dynamic updates
        // For now, tools should be passed at construction time
        let _ = tools;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_config_default() {
        let config = InputConfig::default();
        assert_eq!(config.mode, InputMode::Emacs);
        assert_eq!(config.max_history, 10000);
        assert!(config.commands.contains(&"/help".into()));
    }

    #[test]
    fn test_input_config_default_commands() {
        let config = InputConfig::default();

        assert!(config.commands.contains(&"/help".into()));
        assert!(config.commands.contains(&"/status".into()));
        assert!(config.commands.contains(&"/stats".into()));
        assert!(config.commands.contains(&"/mode".into()));
        assert!(config.commands.contains(&"/ctx".into()));
        assert!(config.commands.contains(&"/compress".into()));
        assert!(config.commands.contains(&"/context".into()));
        assert!(config.commands.contains(&"/memory".into()));
        assert!(config.commands.contains(&"/clear".into()));
        assert!(config.commands.contains(&"/tools".into()));
        assert!(config.commands.contains(&"/analyze".into()));
        assert!(config.commands.contains(&"/review".into()));
        assert!(config.commands.contains(&"/plan".into()));
        assert!(config.commands.contains(&"/garden".into()));
        assert!(config.commands.contains(&"/journal".into()));
        assert!(config.commands.contains(&"/palette".into()));
        assert!(config.commands.contains(&"exit".into()));
        assert!(config.commands.contains(&"quit".into()));
    }

    #[test]
    fn test_input_config_custom() {
        let config = InputConfig {
            mode: InputMode::Vi,
            max_history: 500,
            tool_names: vec!["my_tool".into()],
            ..Default::default()
        };

        assert_eq!(config.mode, InputMode::Vi);
        assert_eq!(config.max_history, 500);
        assert!(config.tool_names.contains(&"my_tool".into()));
    }

    #[test]
    fn test_input_mode_default() {
        assert_eq!(InputMode::default(), InputMode::Emacs);
    }

    #[test]
    fn test_input_mode_equality() {
        assert_eq!(InputMode::Emacs, InputMode::Emacs);
        assert_eq!(InputMode::Vi, InputMode::Vi);
        assert_ne!(InputMode::Emacs, InputMode::Vi);
    }

    #[test]
    fn test_input_config_syntax_highlight() {
        let config = InputConfig::default();
        assert!(config.syntax_highlight);
    }

    #[test]
    fn test_input_config_show_hints() {
        let config = InputConfig::default();
        assert!(config.show_hints);
    }

    #[test]
    fn test_dirs_history_path() {
        // Should return Some path or None depending on environment
        let path = dirs_history_path();
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("selfware"));
            assert!(p.to_string_lossy().contains("history"));
        }
    }

    #[test]
    fn test_readline_result_variants() {
        // Just verify the enum variants exist and can be constructed
        let _line = ReadlineResult::Line("test".into());
        let _interrupt = ReadlineResult::Interrupt;
        let _eof = ReadlineResult::Eof;
    }

    #[test]
    fn test_readline_result_debug() {
        let result = ReadlineResult::Line("test".into());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Line"));
        assert!(debug_str.contains("test"));
    }
}
