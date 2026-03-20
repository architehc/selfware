//! TUI Event Handling Tests

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Test is_key helper function
#[test]
fn test_is_key_helper() {
    use selfware::ui::tui::is_key;

    let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));

    assert!(is_key(&event, KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(!is_key(&event, KeyCode::Char('b'), KeyModifiers::NONE));
    assert!(!is_key(&event, KeyCode::Char('a'), KeyModifiers::CONTROL));
}

/// Test is_quit helper with various quit combinations
#[test]
fn test_is_quit_combinations() {
    use selfware::ui::tui::is_quit;

    // Test 'q' key
    let q_event = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    // Note: is_quit also checks signal_received(), so this may not always return true

    // Test Ctrl+C
    let ctrl_c_event = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    // Same caveat applies

    // Just verify these don't panic
    let _ = is_quit(&q_event);
    let _ = is_quit(&ctrl_c_event);
}

/// Test key matching for common shortcuts
#[test]
fn test_common_shortcuts() {
    use selfware::ui::tui::is_key;

    // Ctrl+N
    let ctrl_n = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
    assert!(is_key(&ctrl_n, KeyCode::Char('n'), KeyModifiers::CONTROL));

    // Ctrl+P
    let ctrl_p = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert!(is_key(&ctrl_p, KeyCode::Char('p'), KeyModifiers::CONTROL));

    // Enter
    let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(is_key(&enter, KeyCode::Enter, KeyModifiers::NONE));

    // Escape
    let esc = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(is_key(&esc, KeyCode::Esc, KeyModifiers::NONE));

    // Tab
    let tab = Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert!(is_key(&tab, KeyCode::Tab, KeyModifiers::NONE));
}

/// Test arrow key events
#[test]
fn test_arrow_keys() {
    use selfware::ui::tui::is_key;

    let up = Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    let down = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let left = Event::Key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
    let right = Event::Key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));

    assert!(is_key(&up, KeyCode::Up, KeyModifiers::NONE));
    assert!(is_key(&down, KeyCode::Down, KeyModifiers::NONE));
    assert!(is_key(&left, KeyCode::Left, KeyModifiers::NONE));
    assert!(is_key(&right, KeyCode::Right, KeyModifiers::NONE));
}

/// Test page up/down keys
#[test]
fn test_page_keys() {
    use selfware::ui::tui::is_key;

    let page_up = Event::Key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
    let page_down = Event::Key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
    let home = Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    let end = Event::Key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE));

    assert!(is_key(&page_up, KeyCode::PageUp, KeyModifiers::NONE));
    assert!(is_key(&page_down, KeyCode::PageDown, KeyModifiers::NONE));
    assert!(is_key(&home, KeyCode::Home, KeyModifiers::NONE));
    assert!(is_key(&end, KeyCode::End, KeyModifiers::NONE));
}

/// Test backspace and delete keys
#[test]
fn test_edit_keys() {
    use selfware::ui::tui::is_key;

    let backspace = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
    let delete = Event::Key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE));

    assert!(is_key(&backspace, KeyCode::Backspace, KeyModifiers::NONE));
    assert!(is_key(&delete, KeyCode::Delete, KeyModifiers::NONE));
}

/// Test function keys
#[test]
fn test_function_keys() {
    use selfware::ui::tui::is_key;

    let f1 = Event::Key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
    let f12 = Event::Key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));

    assert!(is_key(&f1, KeyCode::F(1), KeyModifiers::NONE));
    assert!(is_key(&f12, KeyCode::F(12), KeyModifiers::NONE));
}

/// Test modifier combinations
#[test]
fn test_modifier_combinations() {
    use selfware::ui::tui::is_key;

    // Ctrl+S
    let ctrl_s = Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(is_key(&ctrl_s, KeyCode::Char('s'), KeyModifiers::CONTROL));

    // Alt+F
    let alt_f = Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::ALT));
    assert!(is_key(&alt_f, KeyCode::Char('f'), KeyModifiers::ALT));

    // Shift+Tab
    let shift_tab = Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert!(is_key(&shift_tab, KeyCode::BackTab, KeyModifiers::SHIFT));
}

/// Test event handling with non-key events
#[test]
fn test_non_key_events() {
    use selfware::ui::tui::is_key;

    // Resize event should not match any key
    let resize = Event::Resize(80, 24);
    assert!(!is_key(&resize, KeyCode::Char('a'), KeyModifiers::NONE));

    // Focus event
    let focus_gained = Event::FocusGained;
    assert!(!is_key(
        &focus_gained,
        KeyCode::Char('a'),
        KeyModifiers::NONE
    ));

    let focus_lost = Event::FocusLost;
    assert!(!is_key(&focus_lost, KeyCode::Char('a'), KeyModifiers::NONE));
}

/// Test Event construction for testing
#[test]
fn test_event_construction() {
    // These should all compile and work
    let _char_key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let _ctrl_char = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let _alt_char = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::ALT);

    // Special keys
    let _enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let _esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let _tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
}

/// Test palette command parsing (if palette exists)
#[test]
fn test_command_palette_navigation() {
    use selfware::ui::tui::palette::CommandPalette;

    // Create palette
    let mut palette = CommandPalette::new();

    // Open and close
    palette.open();
    assert!(palette.is_open());

    palette.close();
    assert!(!palette.is_open());

    // Toggle
    palette.toggle();
    assert!(palette.is_open());

    palette.toggle();
    assert!(!palette.is_open());
}

/// Test AppState transitions
#[test]
fn test_app_states() {
    use selfware::ui::tui::app::AppState;

    let states = [
        AppState::Dashboard,
        AppState::Chat,
        AppState::Task,
        AppState::Garden,
    ];

    // Verify all states can be created
    for state in &states {
        let _ = format!("{:?}", state);
    }
}

/// Test message role types
#[test]
fn test_message_roles() {
    use selfware::ui::tui::app::MessageRole;

    let user = MessageRole::User;
    let assistant = MessageRole::Assistant;
    let system = MessageRole::System;

    // Just verify they exist
    let _ = format!("{:?}", user);
    let _ = format!("{:?}", assistant);
    let _ = format!("{:?}", system);
}
