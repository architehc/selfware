//! Selfware Terminal UI
//!
//! Rich TUI built on ratatui with split panes, status bar, and command palette.

// Feature-gated module - dead_code lint disabled at crate level

mod app;
mod layout;
mod markdown;
mod palette;
mod widgets;

pub use palette::CommandPalette;

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
    Terminal,
};
use std::io::{self, Stdout};

/// The Selfware color palette for TUI
pub struct TuiPalette;

impl TuiPalette {
    // Primary colors - warm and inviting
    pub const AMBER: Color = Color::Rgb(212, 163, 115);
    pub const GARDEN_GREEN: Color = Color::Rgb(96, 108, 56);
    pub const SOIL_BROWN: Color = Color::Rgb(188, 108, 37);
    pub const INK: Color = Color::Rgb(40, 54, 24);
    pub const PARCHMENT: Color = Color::Rgb(254, 250, 224);

    // Accent colors
    pub const RUST: Color = Color::Rgb(139, 69, 19);
    pub const COPPER: Color = Color::Rgb(184, 115, 51);
    pub const SAGE: Color = Color::Rgb(143, 151, 121);
    pub const STONE: Color = Color::Rgb(128, 128, 128);

    // Status colors
    pub const BLOOM: Color = Color::Rgb(144, 190, 109);
    pub const WILT: Color = Color::Rgb(188, 108, 37);
    pub const FROST: Color = Color::Rgb(100, 100, 120);

    /// Style for titles
    pub fn title_style() -> Style {
        Style::default()
            .fg(Self::AMBER)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for selected items
    pub fn selected_style() -> Style {
        Style::default()
            .bg(Self::GARDEN_GREEN)
            .fg(Self::PARCHMENT)
            .add_modifier(Modifier::BOLD)
    }

    /// Style for success messages
    pub fn success_style() -> Style {
        Style::default().fg(Self::BLOOM)
    }

    /// Style for warning messages
    pub fn warning_style() -> Style {
        Style::default().fg(Self::WILT)
    }

    /// Style for error messages
    pub fn error_style() -> Style {
        Style::default().fg(Self::FROST)
    }

    /// Style for muted text
    pub fn muted_style() -> Style {
        Style::default().fg(Self::STONE)
    }

    /// Style for paths
    pub fn path_style() -> Style {
        Style::default()
            .fg(Self::SAGE)
            .add_modifier(Modifier::ITALIC)
    }

    /// Border style
    pub fn border_style() -> Style {
        Style::default().fg(Self::SAGE)
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

/// Check for quit keys (q, Ctrl+C, Ctrl+D)
pub fn is_quit(event: &Event) -> bool {
    is_key(event, KeyCode::Char('q'), KeyModifiers::NONE)
        || is_key(event, KeyCode::Char('c'), KeyModifiers::CONTROL)
        || is_key(event, KeyCode::Char('d'), KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_palette_colors() {
        // Just verify the colors are defined correctly
        assert_eq!(TuiPalette::AMBER, Color::Rgb(212, 163, 115));
        assert_eq!(TuiPalette::GARDEN_GREEN, Color::Rgb(96, 108, 56));
    }

    #[test]
    fn test_palette_styles() {
        let title = TuiPalette::title_style();
        assert!(title.add_modifier.contains(Modifier::BOLD));
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
}
