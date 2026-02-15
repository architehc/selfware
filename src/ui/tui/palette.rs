//! Command Palette
//!
//! Fuzzy command search like VS Code (Ctrl+P / Ctrl+Shift+P).

// Feature-gated module - dead_code lint disabled at crate level

use super::TuiPalette;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

/// A command in the palette
#[derive(Debug, Clone)]
pub struct PaletteCommand {
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// The actual command to execute
    pub command: String,
    /// Category for grouping
    pub category: CommandCategory,
    /// Keyboard shortcut (for display)
    pub shortcut: Option<String>,
}

/// Command categories
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum CommandCategory {
    Chat,
    File,
    Git,
    Tool,
    Navigation,
    Settings,
}

impl CommandCategory {
    fn icon(&self) -> &'static str {
        match self {
            Self::Chat => "💬",
            Self::File => "📄",
            Self::Git => "🌿",
            Self::Tool => "🔧",
            Self::Navigation => "🧭",
            Self::Settings => "⚙️",
        }
    }
}

/// The command palette
pub struct CommandPalette {
    /// All available commands
    commands: Vec<PaletteCommand>,
    /// Filtered commands based on query
    filtered: Vec<usize>,
    /// Current query
    query: String,
    /// Selected index
    selected: usize,
    /// Fuzzy matcher
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    /// Create a new command palette with default commands
    pub fn new() -> Self {
        let commands = Self::default_commands();
        let filtered: Vec<usize> = (0..commands.len()).collect();

        Self {
            commands,
            filtered,
            query: String::new(),
            selected: 0,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// Get default commands
    fn default_commands() -> Vec<PaletteCommand> {
        vec![
            // Chat commands
            PaletteCommand {
                name: "Clear conversation".into(),
                description: "Clear all messages and start fresh".into(),
                command: "/clear".into(),
                category: CommandCategory::Chat,
                shortcut: None,
            },
            PaletteCommand {
                name: "Show help".into(),
                description: "Display available commands".into(),
                command: "/help".into(),
                category: CommandCategory::Chat,
                shortcut: Some("F1".into()),
            },
            PaletteCommand {
                name: "Show status".into(),
                description: "Display agent status and memory".into(),
                command: "/status".into(),
                category: CommandCategory::Chat,
                shortcut: None,
            },
            PaletteCommand {
                name: "Show memory".into(),
                description: "Display memory statistics".into(),
                command: "/memory".into(),
                category: CommandCategory::Chat,
                shortcut: None,
            },
            // File commands
            PaletteCommand {
                name: "Analyze codebase".into(),
                description: "Survey the structure of a directory".into(),
                command: "/analyze ".into(),
                category: CommandCategory::File,
                shortcut: None,
            },
            PaletteCommand {
                name: "Review file".into(),
                description: "Review code in a specific file".into(),
                command: "/review ".into(),
                category: CommandCategory::File,
                shortcut: None,
            },
            PaletteCommand {
                name: "View garden".into(),
                description: "Visualize codebase as digital garden".into(),
                command: "/garden".into(),
                category: CommandCategory::File,
                shortcut: None,
            },
            // Tool commands
            PaletteCommand {
                name: "List tools".into(),
                description: "Show all available tools".into(),
                command: "/tools".into(),
                category: CommandCategory::Tool,
                shortcut: None,
            },
            PaletteCommand {
                name: "Create plan".into(),
                description: "Create a detailed plan for a task".into(),
                command: "/plan ".into(),
                category: CommandCategory::Tool,
                shortcut: None,
            },
            // Navigation commands
            PaletteCommand {
                name: "View journal".into(),
                description: "Browse saved task entries".into(),
                command: "/journal".into(),
                category: CommandCategory::Navigation,
                shortcut: None,
            },
            // Exit
            PaletteCommand {
                name: "Exit".into(),
                description: "Leave the workshop".into(),
                command: "exit".into(),
                category: CommandCategory::Navigation,
                shortcut: Some("Ctrl+D".into()),
            },
        ]
    }

    /// Filter commands based on current query
    pub fn filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.commands.len()).collect();
        } else {
            let mut scored: Vec<(i64, usize)> = self
                .commands
                .iter()
                .enumerate()
                .filter_map(|(i, cmd)| {
                    let name_score = self.matcher.fuzzy_match(&cmd.name, &self.query);
                    let desc_score = self.matcher.fuzzy_match(&cmd.description, &self.query);
                    let cmd_score = self.matcher.fuzzy_match(&cmd.command, &self.query);

                    let best_score = [name_score, desc_score, cmd_score]
                        .into_iter()
                        .flatten()
                        .max();

                    best_score.map(|score| (score, i))
                })
                .collect();

            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }

        // Reset selection
        self.selected = 0;
    }

    /// Handle character input
    pub fn on_char(&mut self, c: char) {
        self.query.push(c);
        self.filter();
    }

    /// Handle backspace
    pub fn on_backspace(&mut self) {
        self.query.pop();
        self.filter();
    }

    /// Select next item
    pub fn next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    /// Select previous item
    pub fn previous(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.filtered.len() - 1);
        }
    }

    /// Get the selected command
    pub fn selected_command(&self) -> Option<String> {
        self.filtered
            .get(self.selected)
            .map(|&i| self.commands[i].command.clone())
    }

    /// Reset the palette
    pub fn reset(&mut self) {
        self.query.clear();
        self.selected = 0;
        self.filter();
    }

    /// Render the palette
    pub fn render(&self, frame: &mut Frame, area: Rect, _selected_override: usize) {
        // Clear background
        frame.render_widget(Clear, area);

        // Create block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(TuiPalette::title_style())
            .title(" 🎯 Command Palette ");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 3 {
            return;
        }

        // Render query input
        let query_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let query_text = if self.query.is_empty() {
            Paragraph::new("Type to search commands...").style(TuiPalette::muted_style())
        } else {
            Paragraph::new(format!("❯ {}", self.query))
                .style(Style::default().fg(TuiPalette::AMBER))
        };
        frame.render_widget(query_text, query_area);

        // Render filtered commands
        let list_area = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(2),
        );

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .enumerate()
            .take(list_area.height as usize)
            .map(|(i, &cmd_idx)| {
                let cmd = &self.commands[cmd_idx];
                let is_selected = i == self.selected;

                let style = if is_selected {
                    TuiPalette::selected_style()
                } else {
                    Style::default()
                };

                let shortcut = cmd
                    .shortcut
                    .as_ref()
                    .map(|s| format!(" [{}]", s))
                    .unwrap_or_default();

                let line = Line::from(vec![
                    Span::raw(format!("{} ", cmd.category.icon())),
                    Span::styled(&cmd.name, style.add_modifier(Modifier::BOLD)),
                    Span::styled(shortcut, TuiPalette::muted_style()),
                    Span::raw(" — "),
                    Span::styled(&cmd.description, TuiPalette::muted_style()),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, list_area);
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_creation() {
        let palette = CommandPalette::new();
        assert!(!palette.commands.is_empty());
        assert_eq!(palette.filtered.len(), palette.commands.len());
    }

    #[test]
    fn test_palette_default() {
        let palette = CommandPalette::default();
        assert!(!palette.commands.is_empty());
    }

    #[test]
    fn test_default_commands_exist() {
        let palette = CommandPalette::new();

        // Check that essential commands exist
        let commands: Vec<&str> = palette
            .commands
            .iter()
            .map(|c| c.command.as_str())
            .collect();
        assert!(commands.contains(&"/clear"));
        assert!(commands.contains(&"/help"));
        assert!(commands.contains(&"/status"));
        assert!(commands.contains(&"/tools"));
        assert!(commands.contains(&"exit"));
    }

    #[test]
    fn test_command_categories() {
        let palette = CommandPalette::new();

        // Check that we have commands in different categories
        let categories: Vec<CommandCategory> =
            palette.commands.iter().map(|c| c.category).collect();
        assert!(categories.contains(&CommandCategory::Chat));
        assert!(categories.contains(&CommandCategory::File));
        assert!(categories.contains(&CommandCategory::Tool));
        assert!(categories.contains(&CommandCategory::Navigation));
    }

    #[test]
    fn test_category_icons() {
        assert_eq!(CommandCategory::Chat.icon(), "💬");
        assert_eq!(CommandCategory::File.icon(), "📄");
        assert_eq!(CommandCategory::Git.icon(), "🌿");
        assert_eq!(CommandCategory::Tool.icon(), "🔧");
        assert_eq!(CommandCategory::Navigation.icon(), "🧭");
        assert_eq!(CommandCategory::Settings.icon(), "⚙️");
    }

    #[test]
    fn test_filtering() {
        let mut palette = CommandPalette::new();

        palette.on_char('c');
        palette.on_char('l');
        palette.on_char('e');
        palette.on_char('a');
        palette.on_char('r');

        // Should filter to "clear" related commands
        assert!(!palette.filtered.is_empty());

        // First result should be "Clear conversation"
        if let Some(&first_idx) = palette.filtered.first() {
            assert!(palette.commands[first_idx]
                .name
                .to_lowercase()
                .contains("clear"));
        }
    }

    #[test]
    fn test_filtering_empty_query() {
        let mut palette = CommandPalette::new();
        let initial_count = palette.filtered.len();

        palette.filter();

        assert_eq!(palette.filtered.len(), initial_count);
    }

    #[test]
    fn test_filtering_no_match() {
        let mut palette = CommandPalette::new();

        palette.on_char('x');
        palette.on_char('y');
        palette.on_char('z');
        palette.on_char('z');
        palette.on_char('y');

        // Should have few or no matches for nonsense
        // (fuzzy matching might still find some)
    }

    #[test]
    fn test_filtering_partial_match() {
        let mut palette = CommandPalette::new();

        palette.on_char('h');
        palette.on_char('e');
        palette.on_char('l');

        // Should match "help" commands
        assert!(!palette.filtered.is_empty());
    }

    #[test]
    fn test_on_char() {
        let mut palette = CommandPalette::new();
        assert!(palette.query.is_empty());

        palette.on_char('a');
        assert_eq!(palette.query, "a");

        palette.on_char('b');
        assert_eq!(palette.query, "ab");
    }

    #[test]
    fn test_on_backspace() {
        let mut palette = CommandPalette::new();
        palette.on_char('a');
        palette.on_char('b');
        palette.on_char('c');
        assert_eq!(palette.query, "abc");

        palette.on_backspace();
        assert_eq!(palette.query, "ab");

        palette.on_backspace();
        assert_eq!(palette.query, "a");

        palette.on_backspace();
        assert!(palette.query.is_empty());

        // Backspace on empty should be safe
        palette.on_backspace();
        assert!(palette.query.is_empty());
    }

    #[test]
    fn test_navigation() {
        let mut palette = CommandPalette::new();
        assert_eq!(palette.selected, 0);

        palette.next();
        assert_eq!(palette.selected, 1);

        palette.previous();
        assert_eq!(palette.selected, 0);

        // Wrap around
        palette.previous();
        assert_eq!(palette.selected, palette.filtered.len() - 1);
    }

    #[test]
    fn test_navigation_wrap_forward() {
        let mut palette = CommandPalette::new();
        let count = palette.filtered.len();

        // Go to last item
        for _ in 0..count - 1 {
            palette.next();
        }
        assert_eq!(palette.selected, count - 1);

        // Next should wrap to 0
        palette.next();
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn test_navigation_empty() {
        let mut palette = CommandPalette::new();
        // Filter to get no results
        palette.query = "xyzxyzxyz".to_string();
        palette.filter();

        // Navigation on empty should not panic
        palette.next();
        palette.previous();
    }

    #[test]
    fn test_selected_command() {
        let palette = CommandPalette::new();
        let cmd = palette.selected_command();
        assert!(cmd.is_some());
    }

    #[test]
    fn test_selected_command_after_navigation() {
        let mut palette = CommandPalette::new();

        let first = palette.selected_command();
        palette.next();
        let second = palette.selected_command();

        // Should be different commands
        assert_ne!(first, second);
    }

    #[test]
    fn test_reset() {
        let mut palette = CommandPalette::new();
        palette.on_char('t');
        palette.on_char('e');
        palette.on_char('s');
        palette.on_char('t');
        palette.next();
        palette.next();

        palette.reset();

        assert!(palette.query.is_empty());
        assert_eq!(palette.selected, 0);
        assert_eq!(palette.filtered.len(), palette.commands.len());
    }

    #[test]
    fn test_filter_resets_selection() {
        let mut palette = CommandPalette::new();
        palette.next();
        palette.next();
        assert_eq!(palette.selected, 2);

        palette.on_char('a');
        // Selection should reset when filtering
        assert_eq!(palette.selected, 0);
    }

    #[test]
    fn test_command_has_description() {
        let palette = CommandPalette::new();

        for cmd in &palette.commands {
            assert!(
                !cmd.description.is_empty(),
                "Command {} has no description",
                cmd.name
            );
        }
    }

    #[test]
    fn test_command_has_name() {
        let palette = CommandPalette::new();

        for cmd in &palette.commands {
            assert!(!cmd.name.is_empty(), "Command has empty name");
        }
    }

    #[test]
    fn test_command_has_command() {
        let palette = CommandPalette::new();

        for cmd in &palette.commands {
            assert!(
                !cmd.command.is_empty(),
                "Command {} has empty command",
                cmd.name
            );
        }
    }
}
