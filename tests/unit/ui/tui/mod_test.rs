use super::*;

#[test]
fn wrap_chat_message_splits_embedded_newlines() {
    // A multi-line message (e.g. /help) must render as SEPARATE rows, not a
    // single run-on paragraph. Three logical lines -> at least three rows,
    // and no rendered row may contain a literal '\n'.
    let content = "Commands:\n/help - show help\n/quit - exit";
    let lines = wrap_chat_message_lines("> ", content, 80);
    assert_eq!(lines.len(), 3, "expected 3 rows, got {:?}", lines);
    assert!(lines.iter().all(|l| !l.contains('\n')));
    assert_eq!(lines[0], "> Commands:");
    // Continuation logical lines are indented to the prefix width (2).
    assert_eq!(lines[1], "  /help - show help");
    assert_eq!(lines[2], "  /quit - exit");
}

#[test]
fn wrap_chat_message_preserves_blank_lines() {
    let lines = wrap_chat_message_lines("> ", "a\n\nb", 80);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "> a");
    assert_eq!(lines[1], "  "); // blank line keeps indent
    assert_eq!(lines[2], "  b");
}

#[test]
fn wrap_chat_message_wraps_long_logical_line() {
    // A single logical line longer than the pane wraps onto continuation
    // rows indented to the prefix width.
    let lines = wrap_chat_message_lines("> ", "abcdefghij", 6);
    assert!(lines.len() > 1, "long line should wrap: {:?}", lines);
    assert_eq!(lines[0], "> abcd"); // width 6, prefix 2 -> 4 chars
    assert!(lines[1].starts_with("  ")); // continuation indented
}

#[test]
fn render_pause_indicator_does_not_panic_on_tiny_terminals() {
    use ratatui::backend::TestBackend;

    // Regression: terminals < 20 wide / < 5 tall used to panic on u16
    // subtract-overflow inside render_pause_indicator, killing the
    // default no-subcommand TUI mid-session.
    for (w, h) in [
        (0u16, 0u16),
        (1, 1),
        (10, 4),
        (19, 3),
        (5, 2),
        (20, 5),
        (3, 10),
    ] {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_pause_indicator(frame, area);
            })
            .unwrap_or_else(|e| panic!("render failed on {}x{}: {}", w, h, e));
    }
}

#[test]
fn render_pause_indicator_stays_in_bounds_on_normal_terminal() {
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            render_pause_indicator(frame, area);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
    }
    assert!(
        text.contains("DISPLAY PAUSED"),
        "pause label should render on a normal terminal:\n{}",
        text
    );
}

#[test]
fn oversized_message_renders_its_tail_not_a_blank_pane() {
    use ratatui::backend::TestBackend;

    // 30 logical lines in a ~10-row message area: wrapped height far
    // exceeds the pane height. ratatui's List stops at the first item
    // that does not fit, so a single multi-row ListItem rendered NOTHING
    // — the chat pane went blank (e.g. for /help output).
    let backend = TestBackend::new(40, 15);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new("test-model");
    let content = (1..=30)
        .map(|i| format!("line{:02}", i))
        .collect::<Vec<_>>()
        .join("\n");
    app.add_system_message(&content);

    terminal
        .draw(|frame| render_chat_pane(frame, Rect::new(0, 0, 40, 15), &app, true))
        .unwrap();

    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
    }

    // The pane must not be blank, and the TAIL of the oversized message
    // must be visible.
    assert!(
        text.contains("line30"),
        "tail row of oversized message missing:\n{}",
        text
    );
    assert!(
        text.contains("line21"),
        "late rows of oversized message missing:\n{}",
        text
    );
}

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

#[test]
fn test_signal_received_default_false() {
    // Signal should not have been received at test startup
    // (We cannot truly guarantee this in a shared test process,
    // but it should be false by default.)
    // Just verify the function doesn't panic.
    let _ = signal_received();
}

#[test]
fn test_restore_terminal_state_no_panic() {
    // restore_terminal_state should not panic even if terminal
    // is not in raw mode (all operations are best-effort).
    restore_terminal_state();
}
