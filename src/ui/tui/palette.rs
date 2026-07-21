//! Command Palette
//!
//! Fuzzy command search like VS Code (Ctrl+P / Ctrl+Shift+P).

// Feature-gated module - dead_code lint disabled at crate level

use super::TuiPalette;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
    layout::Rect,
    style::Style,
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
pub enum CommandCategory {
    Chat,
    File,
    #[allow(dead_code)]
    Git,
    Tool,
    Navigation,
    #[allow(dead_code)]
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
                shortcut: Some("Ctrl+C".into()),
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

            scored.sort_by_key(|x| std::cmp::Reverse(x.0));
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
        use ratatui::layout::Alignment;
        use ratatui::style::Modifier;

        // Clear background
        frame.render_widget(Clear, area);

        // Create block with prominent border to indicate modal state
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            )
            .title(Span::styled(
                " 🎯 Command Palette ",
                Style::default()
                    .fg(TuiPalette::AMBER)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 3 {
            return;
        }

        // Render query input with clear focus indicator
        let query_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let query_text = if self.query.is_empty() {
            Paragraph::new("Type to search commands...").style(TuiPalette::muted_style())
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "❯ ",
                    Style::default()
                        .fg(TuiPalette::AMBER)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(&self.query),
            ]))
        };
        frame.render_widget(query_text, query_area);

        // Show cursor in palette input
        let cursor_x = inner.x + 2 + self.query.len() as u16;
        frame.set_cursor_position((cursor_x.min(inner.x + inner.width - 1), inner.y));

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

        // Render help footer if there's room
        if inner.height > 5 {
            let footer_y = inner.y + inner.height - 1;
            let footer_area = Rect::new(inner.x, footer_y, inner.width, 1);
            let footer = Paragraph::new(
                Line::from(vec![
                    Span::styled("↑/↓: navigate", TuiPalette::muted_style()),
                    Span::raw(" │ "),
                    Span::styled("Enter: select", TuiPalette::muted_style()),
                    Span::raw(" │ "),
                    Span::styled("Esc: close", TuiPalette::muted_style()),
                ])
                .alignment(Alignment::Center),
            );
            frame.render_widget(footer, footer_area);
        }
    }
}

impl Default for CommandPalette {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/tui/palette/palette_test.rs"]
mod tests;
