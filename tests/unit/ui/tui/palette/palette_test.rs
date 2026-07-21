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
    let categories: Vec<CommandCategory> = palette.commands.iter().map(|c| c.category).collect();
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
