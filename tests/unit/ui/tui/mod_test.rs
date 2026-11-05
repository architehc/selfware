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

#[test]
fn parse_step_status_reads_the_step_number_only() {
    assert_eq!(parse_step_status("Step 2"), Some(2));
    // Legacy "N/M": M was the iteration cap, never shown as a total.
    assert_eq!(parse_step_status("Step 2/400"), Some(2));
    assert_eq!(parse_step_status("Step 3 failed: boom"), None);
    assert_eq!(parse_step_status("Planning"), None);
}

#[test]
fn permission_keys_map_only_to_offered_answers() {
    use crate::safety::confirm_view::{PermissionAnswer, PermissionPrompt, RiskTag};
    let mut prompt = PermissionPrompt {
        tool_name: "shell_exec".to_string(),
        risk: RiskTag::RunsCommand,
        reason: None,
        body: vec![],
        allow_always: true,
        shell_rule: Some("commands starting with `cargo test`".to_string()),
        full_view: None,
    };
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Char('y')),
        Some(PermissionAnswer::Once)
    );
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Char('a')),
        Some(PermissionAnswer::AlwaysTool)
    );
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Char('!')),
        Some(PermissionAnswer::EnableYolo)
    );
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Char('p')),
        Some(PermissionAnswer::ShellRule)
    );
    for key in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        assert_eq!(
            permission_key_answer(&prompt, key),
            Some(PermissionAnswer::Deny)
        );
    }
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('x')), None);

    // Un-offered standing answers keep the modal open — never a silent yes.
    prompt.allow_always = false;
    prompt.shell_rule = None;
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('a')), None);
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('!')), None);
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('p')), None);
}

// ── chat pane order (0.9.1: newest-first put tool events above the question) ──

#[test]
fn chat_rows_are_chronological_with_newest_at_the_bottom() {
    let entries = vec![
        ("You ".to_string(), "question".to_string(), 0u8),
        ("🔧 ".to_string(), "[file_read] started".to_string(), 1u8),
        ("🦊 ".to_string(), "answer".to_string(), 2u8),
    ];
    let rows = chat_rows_chronological(&entries, 40, 10);
    let texts: Vec<&str> = rows.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(texts.len(), 3);
    assert!(texts[0].contains("question"), "{texts:?}");
    assert!(texts[2].contains("answer"), "{texts:?}");
}

#[test]
fn chat_rows_keep_the_newest_when_the_pane_is_full() {
    let entries: Vec<(String, String, u8)> = (0..20)
        .map(|i| ("> ".to_string(), format!("msg {i}"), 0u8))
        .collect();
    let rows = chat_rows_chronological(&entries, 40, 5);
    let texts: Vec<&str> = rows.iter().map(|(r, _)| r.as_str()).collect();
    assert_eq!(texts.len(), 5);
    assert!(
        texts[0].contains("msg 15") && texts[4].contains("msg 19"),
        "{texts:?}"
    );
}

#[test]
fn run_outcome_lands_in_the_chat_as_a_system_note() {
    let mut app = App::new("m");
    let mut pending = None;
    apply_agent_event(
        &mut app,
        &mut pending,
        &TuiEvent::RunOutcome {
            summary: "⚠️ Task failed (TIMEOUT)\n── Run summary ──".to_string(),
        },
    );
    let last = app.messages.last().expect("outcome message");
    assert!(last.content.contains("Run summary"));
}

#[test]
fn permission_v_pages_the_full_diff_and_never_answers() {
    use crate::safety::confirm_view::{
        ConfirmLine, FullDiffView, LineKind, PermissionPrompt, RiskTag,
    };
    let lines: Vec<ConfirmLine> = (0..30)
        .map(|i| ConfirmLine {
            kind: LineKind::Added,
            text: format!("+{i}"),
        })
        .collect();
    let mut prompt = PermissionPrompt {
        tool_name: "file_edit".to_string(),
        risk: RiskTag::WritesWorkspace,
        reason: None,
        body: vec![],
        allow_always: true,
        shell_rule: None,
        full_view: Some(FullDiffView::new(lines)),
    };
    // `v` is never an answer.
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('v')), None);
    // It opens the view; the modal stays open with no decision.
    assert!(permission_view_key(&mut prompt, KeyCode::Char('v')));
    assert!(prompt.full_view.as_ref().unwrap().open);
    assert!(permission_view_key(&mut prompt, KeyCode::PageDown));
    assert!(permission_view_key(&mut prompt, KeyCode::Down));
    assert_eq!(prompt.full_view.as_ref().unwrap().scroll, 11);
    assert!(permission_view_key(&mut prompt, KeyCode::End));
    assert_eq!(prompt.full_view.as_ref().unwrap().scroll, 29);
    // Esc closes the view (not a deny) while it is open …
    assert!(permission_view_key(&mut prompt, KeyCode::Esc));
    assert!(!prompt.full_view.as_ref().unwrap().open);
    // … and denies again once it is closed.
    assert!(!permission_view_key(&mut prompt, KeyCode::Esc));
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Esc),
        Some(crate::safety::confirm_view::PermissionAnswer::Deny)
    );
    // y/n still answer while the view is open.
    permission_view_key(&mut prompt, KeyCode::Char('v'));
    assert!(!permission_view_key(&mut prompt, KeyCode::Char('y')));
    assert!(!permission_view_key(&mut prompt, KeyCode::Char('n')));

    // Without a full diff `v` does nothing at all.
    prompt.full_view = None;
    assert!(!permission_view_key(&mut prompt, KeyCode::Char('v')));
    assert_eq!(permission_key_answer(&prompt, KeyCode::Char('v')), None);
}

#[test]
fn windows_key_release_events_are_ignored_by_is_key_and_evaluate_quit_key() {
    use crossterm::event::{KeyEvent, KeyEventKind};

    let press_q =
        KeyEvent::new_with_kind(KeyCode::Char('q'), KeyModifiers::NONE, KeyEventKind::Press);
    let release_q = KeyEvent::new_with_kind(
        KeyCode::Char('q'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    assert!(is_key(
        &Event::Key(press_q),
        KeyCode::Char('q'),
        KeyModifiers::NONE
    ));
    assert!(!is_key(
        &Event::Key(release_q),
        KeyCode::Char('q'),
        KeyModifiers::NONE
    ));

    // A single 'q' tap on Windows (Press followed immediately by Release) must
    // only arm quit, never immediately quit on the Release event.
    let mut armed_at = None;
    assert!(matches!(
        evaluate_quit_key(&press_q, true, &mut armed_at),
        QuitDecision::Armed
    ));
    assert!(armed_at.is_some());
    assert!(matches!(
        evaluate_quit_key(&release_q, true, &mut armed_at),
        QuitDecision::None
    ));
    assert!(
        armed_at.is_some(),
        "key release must not clear or consume the armed quit state"
    );
}

#[test]
fn forgiving_slash_commands_resolve_typos_and_aliases() {
    assert_eq!(
        resolve_forgiving_slash_command("/halp"),
        ("/help", Some("/help"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/stauts"),
        ("/status", Some("/status"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/yol"),
        ("/yolo", Some("/yolo"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/rb"),
        ("/rollback", Some("/rollback"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/nudge"),
        ("/unstuck", Some("/unstuck"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/docker"),
        ("/sandbox", Some("/sandbox"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/que"),
        ("/queue", Some("/queue"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/ram"),
        ("/memory", Some("/memory"))
    );
    assert_eq!(resolve_forgiving_slash_command("/help"), ("/help", None));
}

#[test]
fn permission_modal_exclamation_enables_yolo_for_session() {
    use crate::safety::confirm_view::{PermissionAnswer, PermissionPrompt, RiskTag};
    let prompt = PermissionPrompt {
        tool_name: "shell_exec".to_string(),
        risk: RiskTag::WritesWorkspace,
        reason: None,
        body: vec![],
        allow_always: true,
        shell_rule: None,
        full_view: None,
    };
    assert_eq!(
        permission_key_answer(&prompt, KeyCode::Char('!')),
        Some(PermissionAnswer::EnableYolo)
    );
}

#[test]
fn task_control_queues_drops_and_drains_live_messages_and_toggles_sandbox() {
    let control = crate::lifecycle::control::TaskControl::new(std::sync::Arc::new(
        std::sync::atomic::AtomicBool::new(false),
    ));
    assert_eq!(control.queued_message_count(), 0);
    assert!(!control.is_sandbox_enabled());

    control.set_sandbox_enabled(true);
    assert!(control.is_sandbox_enabled());

    assert_eq!(control.enqueue_user_message("first request"), 1);
    assert_eq!(control.enqueue_user_message("second request"), 2);
    assert_eq!(control.enqueue_user_message("   "), 2);
    assert_eq!(
        control.list_queued_messages(),
        vec!["first request".to_string(), "second request".to_string()]
    );

    assert_eq!(
        control.drop_queued_message(0).as_deref(),
        Some("first request")
    );
    assert_eq!(control.queued_message_count(), 1);

    let drained = control.take_queued_messages();
    assert_eq!(drained, vec!["second request".to_string()]);
    assert_eq!(control.queued_message_count(), 0);
}

#[test]
fn dashboard_state_defaults_to_yolo_and_refreshes_memory_footprint() {
    let mut state = DashboardState::default();
    assert_eq!(state.execution_mode, crate::config::ExecutionMode::Yolo);
    state.refresh_memory_stats();
    assert!(
        state.system_mem_total_gb > 0.0,
        "system total RAM should be measured"
    );
}

#[test]
fn call_time_cap_abort_alert_sequence_never_freezes_tui_or_locks_input() {
    use crate::safety::confirm_view::{PermissionPrompt, RiskTag};
    use ratatui::backend::TestBackend;

    let mut app = App::new("qwen3.8-27b");
    let mut dashboard = DashboardState::default();
    let mut pending_permission = Some(PermissionPrompt {
        tool_name: "shell_exec".to_string(),
        risk: RiskTag::RunsCommand,
        reason: None,
        body: vec![],
        allow_always: true,
        shell_rule: None,
        full_view: None,
    });

    // Simulate an active multi-step autonomous run that hits CALL_TIME_CAP.
    let events = vec![
        TuiEvent::AgentStarted,
        TuiEvent::StatusUpdate {
            message: "Step 1".to_string(),
        },
        TuiEvent::ToolStarted {
            name: "shell_exec".to_string(),
        },
        TuiEvent::AssistantDelta {
            text: "Investigating workspace...".to_string(),
        },
        TuiEvent::AgentError {
            message:
                "Step 1 failed: Single model call exceeded agent.max_call_secs=1200s (elapsed 1200s)"
                    .to_string(),
        },
        // Even if a late/out-of-order Step status arrived before RunOutcome,
        // RunOutcome must leave the TUI in Chatting with progress cleared.
        TuiEvent::StatusUpdate {
            message: "Step 1".to_string(),
        },
        TuiEvent::RunOutcome {
            summary: "⚠️ Task aborted (CALL_TIME_CAP): Single model call exceeded agent.max_call_secs=1200s (elapsed 1200s)\n  hint: raise agent.max_call_secs in selfware.toml\n── Run summary ──\n  outcome: failed (CALL_TIME_CAP)".to_string(),
        },
    ];

    for ev in events {
        apply_agent_event(&mut app, &mut pending_permission, &ev);
        dashboard.process_event(ev);
    }

    // 1. State must be Chatting, with no dangling progress, permission modal, or active tools.
    assert_eq!(app.state, AppState::Chatting);
    assert!(app.task_progress.is_none());
    assert!(pending_permission.is_none());
    assert!(dashboard.active_tools.is_empty());
    assert!(
        dashboard.status_message.contains("CALL_TIME_CAP"),
        "dashboard status should surface CALL_TIME_CAP: {}",
        dashboard.status_message
    );

    // 2. Render the chat pane to verify the alert is visible and rendering never panics.
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_chat_pane(frame, Rect::new(0, 0, 100, 30), &app, true))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
    }
    assert!(
        rendered.contains("CALL_TIME_CAP"),
        "rendered chat pane must display the CALL_TIME_CAP alert:\n{rendered}"
    );

    // 3. Pressing Esc after the alert must NEVER lock out keyboard typing, paste, or Enter.
    app.on_escape();
    assert!(
        app.is_input_editable(),
        "input must remain editable even after Esc"
    );
    for ch in "continue".chars() {
        app.on_char(ch);
    }
    assert_eq!(app.state, AppState::Chatting);
    assert_eq!(app.input, "continue");
    app.on_paste(" workflow");
    assert_eq!(app.input, "continue workflow");
    assert_eq!(app.on_enter().as_deref(), Some("continue workflow"));
    assert!(app.input.is_empty());
}

#[test]
fn phi_fox_meta_agent_slash_aliases_and_cross_env_helpers_work() {
    assert_eq!(
        resolve_forgiving_slash_command("/fox"),
        ("/phi", Some("/phi"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/meta"),
        ("/phi", Some("/phi"))
    );
    assert_eq!(
        resolve_forgiving_slash_command("/phi-fox"),
        ("/phi", Some("/phi"))
    );

    assert!(crate::agent::Agent::is_phi_meta_agent_task(
        "You are Phi Fox, the visual screen-reading and multi-terminal meta-agent."
    ));
    assert!(crate::agent::Agent::is_phi_meta_agent_task(
        "Use screen_capture and pty_shell to inspect terminals"
    ));
    assert!(!crate::agent::Agent::is_phi_meta_agent_task(
        "Rename variable foo to bar in src/lib.rs"
    ));

    // Windows drive-letter volume mounts in container validation
    assert!(crate::tools::container::validate_volume_spec(
        r"C:\Users\ivo\project:/workspace:ro"
    ));
    assert!(!crate::tools::container::validate_volume_spec(
        r"C:\Users\ivo\..\secrets:/workspace"
    ));
}
