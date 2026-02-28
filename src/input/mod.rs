//! Modern Input System for Selfware
//!
//! Rich terminal input with autocomplete, history, and vim keybindings.
//! Built on reedline for a professional IDE-like experience.

pub mod command_registry;
mod completer;
mod highlighter;
mod prompt;

pub use completer::SelfwareCompleter;
pub use highlighter::SelfwareHighlighter;
pub use prompt::SelfwarePrompt;

use anyhow::Result;
use reedline::{
    default_emacs_keybindings, ColumnarMenu, DefaultHinter, DefaultValidator, EditCommand, Emacs,
    FileBackedHistory, KeyCode, KeyModifiers, Keybindings, MenuBuilder, Reedline, ReedlineEvent,
    ReedlineMenu, Signal, Vi,
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

        // Shift+Tab to toggle auto-edit mode (via host command)
        keybindings.add_binding(
            KeyModifiers::SHIFT,
            KeyCode::BackTab,
            ReedlineEvent::ExecuteHostCommand("__toggle_auto_edit__".to_string()),
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
        match self.editor.read_line(&self.prompt) {
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

    /// Update the prompt context
    pub fn set_prompt_context(&mut self, model: &str, step: usize) {
        self.prompt = SelfwarePrompt::with_context(model, step);
    }

    /// Update the prompt with full context including token usage
    pub fn set_prompt_full_context(&mut self, model: &str, step: usize, context_pct: f64) {
        self.prompt = SelfwarePrompt::with_full_context(model, step, context_pct);
    }

    /// Add tool names for completion
    pub fn add_tools(&mut self, tools: Vec<String>) {
        // Note: We'd need to rebuild the completer for dynamic updates
        // For now, tools should be passed at construction time
        let _ = tools;
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
        assert!(config.commands.contains(&"/swarm".into()));
        assert!(config.commands.contains(&"/queue".into()));
        assert!(config.commands.contains(&"/diff".into()));
        assert!(config.commands.contains(&"/git".into()));
        assert!(config.commands.contains(&"/undo".into()));
        assert!(config.commands.contains(&"/cost".into()));
        assert!(config.commands.contains(&"/model".into()));
        assert!(config.commands.contains(&"/compact".into()));
        assert!(config.commands.contains(&"/verbose".into()));
        assert!(config.commands.contains(&"/config".into()));
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
        let _host_cmd = ReadlineResult::HostCommand("__toggle_yolo__".into());
    }

    #[test]
    fn test_readline_result_debug() {
        let result = ReadlineResult::Line("test".into());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Line"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_readline_result_host_command_debug() {
        let result = ReadlineResult::HostCommand("__toggle_yolo__".into());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("HostCommand"));
        assert!(debug_str.contains("__toggle_yolo__"));
    }

    #[test]
    fn test_input_config_new_commands() {
        let config = InputConfig::default();
        assert!(config.commands.contains(&"/vim".into()));
        assert!(config.commands.contains(&"/copy".into()));
        assert!(config.commands.contains(&"/restore".into()));
        assert!(config.commands.contains(&"/chat".into()));
        assert!(config.commands.contains(&"/theme".into()));
    }
}
