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
    assert!(config.commands.contains(&"/last".into()));
    assert!(config.commands.contains(&"/debug".into()));
    assert!(config.commands.contains(&"/debug-log".into()));
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

#[test]
fn test_vi_keybindings_preserve_custom_bindings() {
    let vi_kb = SelfwareEditor::build_keybindings(InputMode::Vi);
    let emacs_kb = SelfwareEditor::build_keybindings(InputMode::Emacs);

    for (mods, code) in [
        (KeyModifiers::NONE, KeyCode::Tab),
        (KeyModifiers::NONE, KeyCode::Char('/')),
        (KeyModifiers::NONE, KeyCode::Enter),
        (KeyModifiers::SHIFT, KeyCode::BackTab),
        (KeyModifiers::CONTROL, KeyCode::Char('y')),
        (KeyModifiers::CONTROL, KeyCode::Char('j')),
        (KeyModifiers::CONTROL, KeyCode::Char('x')),
    ] {
        assert_eq!(
            vi_kb.find_binding(mods, code),
            emacs_kb.find_binding(mods, code),
            "Vi mode must preserve custom binding for ({mods:?}, {code:?})"
        );
        assert!(
            vi_kb.find_binding(mods, code).is_some(),
            "Custom binding ({mods:?}, {code:?}) must be present in Vi mode"
        );
    }
}

#[test]
fn test_toggle_vim_mode_updates_exact_accept() {
    let config = InputConfig {
        history_path: None,
        ..Default::default()
    };
    let mut editor = SelfwareEditor::new(config).expect("create editor");
    let initial_arc = editor.exact_accept.clone();
    initial_arc.store(true, Ordering::SeqCst);

    let mode = editor.toggle_vim_mode().expect("toggle to Vi");
    assert_eq!(mode, InputMode::Vi);
    assert!(
        !Arc::ptr_eq(&initial_arc, &editor.exact_accept),
        "toggle_vim_mode must rebind exact_accept to the new editor's completion menu flag"
    );
    assert!(
        !editor.exact_accept.load(Ordering::SeqCst),
        "new editor exact_accept starts false"
    );

    let mode_back = editor.toggle_vim_mode().expect("toggle back to Emacs");
    assert_eq!(mode_back, InputMode::Emacs);
}
