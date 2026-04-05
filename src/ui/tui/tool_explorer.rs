//! Tool Explorer Panel for Selfware TUI
//!
//! Displays available tools organized by category, inspired by Hermes Agent

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;

/// Information about a tool
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub category: ToolCategory,
    pub description: String,
    pub shortcut: Option<char>,
    pub icon: String,
}

/// Tool categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    File,
    Search,
    Build,
    Browser,
    Shell,
    Git,
    Computer,
    Http,
    Lsp,
    Other,
}

impl ToolCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::File => "File Operations",
            Self::Search => "Search",
            Self::Build => "Build & Package",
            Self::Browser => "Browser",
            Self::Shell => "Shell",
            Self::Git => "Git",
            Self::Computer => "Computer Control",
            Self::Http => "HTTP",
            Self::Lsp => "LSP",
            Self::Other => "Other",
        }
    }

    pub fn icon(&self) -> &str {
        match self {
            Self::File => "📄",
            Self::Search => "🔍",
            Self::Build => "📦",
            Self::Browser => "🌐",
            Self::Shell => "⚡",
            Self::Git => "🔀",
            Self::Computer => "🖥️ ",
            Self::Http => "🌍",
            Self::Lsp => "🔮",
            Self::Other => "🔧",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::File => Color::Rgb(212, 163, 115),      // Amber
            Self::Search => Color::Rgb(144, 190, 109),    // Sage green
            Self::Build => Color::Rgb(184, 115, 51),      // Copper
            Self::Browser => Color::Rgb(100, 149, 237),   // Cornflower blue
            Self::Shell => Color::Rgb(255, 215, 0),       // Gold
            Self::Git => Color::Rgb(255, 109, 49),        // Orange
            Self::Computer => Color::Rgb(147, 112, 219),  // Purple
            Self::Http => Color::Rgb(64, 156, 255),       // Blue
            Self::Lsp => Color::Rgb(255, 105, 180),       // Hot pink
            Self::Other => Color::Gray,
        }
    }
}

/// The Tool Explorer panel
pub struct ToolExplorer {
    tools: HashMap<ToolCategory, Vec<ToolInfo>>,
    selected_category: Option<ToolCategory>,
    selected_tool: Option<String>,
    expanded_categories: Vec<ToolCategory>,
    visible: bool,
}

impl Default for ToolExplorer {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolExplorer {
    /// Create a new Tool Explorer with default tools
    pub fn new() -> Self {
        let mut explorer = Self {
            tools: HashMap::new(),
            selected_category: None,
            selected_tool: None,
            expanded_categories: vec![],
            visible: false,
        };
        explorer.load_default_tools();
        explorer
    }

    /// Load the default set of tools
    fn load_default_tools(&mut self) {
        let default_tools = vec![
            // File operations
            ToolInfo {
                name: "file_read".to_string(),
                category: ToolCategory::File,
                description: "Read file contents".to_string(),
                shortcut: Some('r'),
                icon: "📖".to_string(),
            },
            ToolInfo {
                name: "file_write".to_string(),
                category: ToolCategory::File,
                description: "Write or create a file".to_string(),
                shortcut: Some('w'),
                icon: "✍️ ".to_string(),
            },
            ToolInfo {
                name: "file_edit".to_string(),
                category: ToolCategory::File,
                description: "Edit existing file content".to_string(),
                shortcut: Some('e'),
                icon: "✏️ ".to_string(),
            },
            ToolInfo {
                name: "glob".to_string(),
                category: ToolCategory::File,
                description: "Find files by pattern".to_string(),
                shortcut: Some('g'),
                icon: "🔎".to_string(),
            },
            // Search
            ToolInfo {
                name: "grep_search".to_string(),
                category: ToolCategory::Search,
                description: "Search file contents".to_string(),
                shortcut: Some('/'),
                icon: "🔍".to_string(),
            },
            ToolInfo {
                name: "code_map".to_string(),
                category: ToolCategory::Search,
                description: "Analyze codebase structure".to_string(),
                shortcut: Some('m'),
                icon: "🗺️ ".to_string(),
            },
            // Build
            ToolInfo {
                name: "cargo".to_string(),
                category: ToolCategory::Build,
                description: "Run cargo commands".to_string(),
                shortcut: Some('c'),
                icon: "🦀".to_string(),
            },
            ToolInfo {
                name: "npm".to_string(),
                category: ToolCategory::Build,
                description: "Run npm commands".to_string(),
                shortcut: Some('n'),
                icon: "📦".to_string(),
            },
            ToolInfo {
                name: "pip".to_string(),
                category: ToolCategory::Build,
                description: "Run pip commands".to_string(),
                shortcut: Some('p'),
                icon: "🐍".to_string(),
            },
            // Shell
            ToolInfo {
                name: "shell".to_string(),
                category: ToolCategory::Shell,
                description: "Execute shell commands".to_string(),
                shortcut: Some('!'),
                icon: "⚡".to_string(),
            },
            // Git
            ToolInfo {
                name: "git".to_string(),
                category: ToolCategory::Git,
                description: "Git operations".to_string(),
                shortcut: Some('v'),
                icon: "🔀".to_string(),
            },
            // Browser
            ToolInfo {
                name: "browser_navigate".to_string(),
                category: ToolCategory::Browser,
                description: "Navigate to URL".to_string(),
                shortcut: Some('u'),
                icon: "🌐".to_string(),
            },
            ToolInfo {
                name: "browser_click".to_string(),
                category: ToolCategory::Browser,
                description: "Click on element".to_string(),
                shortcut: None,
                icon: "🖱️ ".to_string(),
            },
            // LSP
            ToolInfo {
                name: "lsp_find_references".to_string(),
                category: ToolCategory::Lsp,
                description: "Find symbol references".to_string(),
                shortcut: Some('f'),
                icon: "🔮".to_string(),
            },
        ];

        for tool in default_tools {
            self.add_tool(tool);
        }
    }

    /// Add a tool to the explorer
    pub fn add_tool(&mut self, tool: ToolInfo) {
        self.tools
            .entry(tool.category)
            .or_default()
            .push(tool);
    }

    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the explorer
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the explorer
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Select next category
    pub fn next_category(&mut self) {
        let categories: Vec<_> = self.tools.keys().cloned().collect();
        if categories.is_empty() {
            return;
        }

        self.selected_category = match self.selected_category {
            None => Some(categories[0]),
            Some(current) => {
                let pos = categories.iter().position(|c| c == &current).unwrap_or(0);
                Some(categories[(pos + 1) % categories.len()])
            }
        };
    }

    /// Select previous category
    pub fn prev_category(&mut self) {
        let categories: Vec<_> = self.tools.keys().cloned().collect();
        if categories.is_empty() {
            return;
        }

        self.selected_category = match self.selected_category {
            None => Some(categories[categories.len() - 1]),
            Some(current) => {
                let pos = categories.iter().position(|c| c == &current).unwrap_or(0);
                let new_pos = if pos == 0 {
                    categories.len() - 1
                } else {
                    pos - 1
                };
                Some(categories[new_pos])
            }
        };
    }

    /// Toggle expansion of a category
    pub fn toggle_category(&mut self, category: ToolCategory) {
        if let Some(pos) = self.expanded_categories.iter().position(|c| c == &category) {
            self.expanded_categories.remove(pos);
        } else {
            self.expanded_categories.push(category);
        }
    }

    /// Get total tool count
    pub fn total_tools(&self) -> usize {
        self.tools.values().map(|v| v.len()).sum()
    }

    /// Get count by category
    pub fn count_by_category(&self) -> HashMap<ToolCategory, usize> {
        self.tools
            .iter()
            .map(|(cat, tools)| (*cat, tools.len()))
            .collect()
    }

    /// Render the tool explorer panel
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Clear the area first
        frame.render_widget(Clear, area);

        // Create the main block
        let block = Block::default()
            .title(format!(" Available Tools ({} total) ", self.total_tools()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(212, 163, 115)));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Split into categories list and details
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(inner_area);

        // Render categories list
        self.render_categories(frame, chunks[0]);

        // Render details panel
        self.render_details(frame, chunks[1]);
    }

    fn render_categories(&self, frame: &mut Frame, area: Rect) {
        let mut items: Vec<ListItem> = vec![];

        // Sort categories for consistent display
        let mut categories: Vec<_> = self.tools.keys().cloned().collect();
        categories.sort_by_key(|c| c.as_str());

        for category in categories {
            let tools = self.tools.get(&category).unwrap();
            let is_expanded = self.expanded_categories.contains(&category);
            let is_selected = self.selected_category == Some(category);

            // Category header
            let expand_icon = if is_expanded { "▼" } else { "▶" };
            let category_text = format!(
                "{} {} {} ({})",
                expand_icon,
                category.icon(),
                category.as_str(),
                tools.len()
            );

            let style = if is_selected {
                Style::default()
                    .fg(category.color())
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
                    .fg(category.color())
                    .add_modifier(Modifier::BOLD)
            };

            items.push(ListItem::new(category_text).style(style));

            // Tools in category (if expanded)
            if is_expanded {
                for tool in tools {
                    let tool_text = format!("  {} {} ", tool.icon, tool.name);
                    let tool_style = if self.selected_tool == Some(tool.name.clone()) {
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    items.push(ListItem::new(tool_text).style(tool_style));
                }
            }
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::RIGHT))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

        frame.render_widget(list, area);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let text = if let Some(category) = self.selected_category {
            if let Some(tools) = self.tools.get(&category) {
                let mut lines = vec![
                    Line::from(vec![
                        Span::styled(category.icon().to_string(), Style::default().fg(category.color())),
                        Span::styled(
                            format!(" {}", category.as_str()),
                            Style::default()
                                .fg(category.color())
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(""),
                ];

                for tool in tools {
                    lines.push(Line::from(vec![
                        Span::styled(&tool.icon, Style::default().fg(Color::White)),
                        Span::styled(
                            format!(" {} ", tool.name),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                    ]));
                    lines.push(Line::from(vec![Span::styled(
                        format!("   {}", tool.description),
                        Style::default().fg(Color::Gray),
                    )]));
                    if let Some(shortcut) = tool.shortcut {
                        lines.push(Line::from(vec![Span::styled(
                            format!("   Shortcut: Alt+{}", shortcut),
                            Style::default().fg(Color::DarkGray),
                        )]));
                    }
                    lines.push(Line::from(""));
                }

                Text::from(lines)
            } else {
                Text::from("Select a category to see tools")
            }
        } else {
            Text::from("Select a category to see tools")
        };

        let paragraph = Paragraph::new(text)
            .block(Block::default().padding(ratatui::widgets::Padding::new(1, 1, 0, 0)))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Render a compact view (for status bar integration)
    pub fn render_compact(&self, frame: &mut Frame, area: Rect) {
        let counts = self.count_by_category();
        let mut spans = vec![Span::styled("Tools: ", Style::default().fg(Color::Gray))];

        for (category, count) in counts.iter().take(5) {
            spans.push(Span::styled(
                format!("{} {} ", category.icon(), count),
                Style::default().fg(category.color()),
            ));
        }

        if counts.len() > 5 {
            spans.push(Span::styled(
                format!("+{} ", counts.len() - 5),
                Style::default().fg(Color::Gray),
            ));
        }

        let text = Text::from(Line::from(spans));
        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_explorer_creation() {
        let explorer = ToolExplorer::new();
        assert!(explorer.total_tools() > 0);
    }

    #[test]
    fn test_category_icons() {
        assert_eq!(ToolCategory::File.icon(), "📄");
        assert_eq!(ToolCategory::Shell.icon(), "⚡");
    }

    #[test]
    fn test_add_tool() {
        let mut explorer = ToolExplorer::new();
        let initial_count = explorer.total_tools();

        explorer.add_tool(ToolInfo {
            name: "test_tool".to_string(),
            category: ToolCategory::Other,
            description: "Test".to_string(),
            shortcut: None,
            icon: "🧪".to_string(),
        });

        assert_eq!(explorer.total_tools(), initial_count + 1);
    }

    #[test]
    fn test_toggle_visibility() {
        let mut explorer = ToolExplorer::new();
        assert!(!explorer.is_visible());

        explorer.toggle();
        assert!(explorer.is_visible());

        explorer.toggle();
        assert!(!explorer.is_visible());
    }
}
