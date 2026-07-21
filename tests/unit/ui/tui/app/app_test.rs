use super::*;

#[test]
fn test_app_creation() {
    let app = App::new("test-model");
    assert_eq!(app.model, "test-model");
    assert_eq!(app.state, AppState::Chatting);
    // Honest default: not connected until the model responds.
    assert!(!app.connected);
}

#[test]
fn test_app_connected_after_model_output() {
    let mut app = App::new("test-model");
    assert!(!app.connected);
    app.append_streaming("hello");
    assert!(app.connected, "streamed bytes must mark the app connected");

    let mut app2 = App::new("test-model");
    app2.add_assistant_message("done");
    assert!(app2.connected, "an assistant message must mark connected");
}

#[test]
fn test_app_initial_state() {
    let app = App::new("test");
    assert!(app.input.is_empty());
    assert_eq!(app.cursor, 0);
    assert_eq!(app.scroll, 0);
    assert_eq!(app.selected, 0);
    assert_eq!(app.status, "Ready");
}

#[test]
fn test_app_has_welcome_message() {
    let app = App::new("test");
    assert!(!app.messages.is_empty());
    assert_eq!(app.messages[0].role, MessageRole::System);
}

#[test]
fn test_add_messages() {
    let mut app = App::new("test");
    app.add_user_message("Hello");
    app.add_assistant_message("Hi there!");

    assert_eq!(app.messages.len(), 3); // 1 system + 2 new
    assert_eq!(app.messages[1].role, MessageRole::User);
    assert_eq!(app.messages[2].role, MessageRole::Assistant);
}

#[test]
fn test_add_user_message() {
    let mut app = App::new("test");
    app.add_user_message("Test message");

    assert_eq!(app.messages.last().unwrap().role, MessageRole::User);
    assert_eq!(app.messages.last().unwrap().content, "Test message");
}

#[test]
fn test_add_assistant_message() {
    let mut app = App::new("test");
    app.add_assistant_message("Assistant response");

    assert_eq!(app.messages.last().unwrap().role, MessageRole::Assistant);
    assert_eq!(app.messages.last().unwrap().content, "Assistant response");
}

#[test]
fn test_add_tool_message() {
    let mut app = App::new("test");
    app.add_tool_message("file_read", "File contents here");

    assert_eq!(app.messages.last().unwrap().role, MessageRole::Tool);
    assert!(app.messages.last().unwrap().content.contains("file_read"));
    assert!(app
        .messages
        .last()
        .unwrap()
        .content
        .contains("File contents here"));
}

#[test]
fn test_message_has_timestamp() {
    let mut app = App::new("test");
    app.add_user_message("Test");

    assert!(!app.messages.last().unwrap().timestamp.is_empty());
}

#[test]
fn test_input_handling() {
    let mut app = App::new("test");

    app.on_char('h');
    app.on_char('i');
    assert_eq!(app.input, "hi");
    assert_eq!(app.cursor, 2);

    app.on_backspace();
    assert_eq!(app.input, "h");
    assert_eq!(app.cursor, 1);
}

#[test]
fn test_input_char_inserts_at_cursor() {
    let mut app = App::new("test");
    app.on_char('a');
    app.on_char('c');
    app.on_left();
    app.on_char('b');

    assert_eq!(app.input, "abc");
}

#[test]
fn test_backspace_at_start() {
    let mut app = App::new("test");
    app.on_char('a');
    app.on_left();
    app.on_backspace();

    // Should not change anything
    assert_eq!(app.input, "a");
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_backspace_empty() {
    let mut app = App::new("test");
    app.on_backspace();

    // Should not panic
    assert!(app.input.is_empty());
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_on_left() {
    let mut app = App::new("test");
    app.on_char('a');
    app.on_char('b');
    assert_eq!(app.cursor, 2);

    app.on_left();
    assert_eq!(app.cursor, 1);

    app.on_left();
    assert_eq!(app.cursor, 0);

    app.on_left();
    assert_eq!(app.cursor, 0); // Can't go below 0
}

#[test]
fn test_on_right() {
    let mut app = App::new("test");
    app.input = "abc".into();
    app.cursor = 0;

    app.on_right();
    assert_eq!(app.cursor, 1);

    app.on_right();
    app.on_right();
    assert_eq!(app.cursor, 3);

    app.on_right();
    assert_eq!(app.cursor, 3); // Can't go beyond length
}

#[test]
fn test_on_up_scroll() {
    let mut app = App::new("test");
    // Add enough messages to scroll
    for i in 0..10 {
        app.add_user_message(&format!("Message {}", i));
    }

    assert_eq!(app.scroll, 0);
    app.on_up();
    assert_eq!(app.scroll, 1);
    app.on_up();
    assert_eq!(app.scroll, 2);
}

#[test]
fn test_on_down_scroll() {
    let mut app = App::new("test");
    for i in 0..10 {
        app.add_user_message(&format!("Message {}", i));
    }

    app.scroll = 5;
    app.on_down();
    assert_eq!(app.scroll, 4);

    app.scroll = 0;
    app.on_down();
    assert_eq!(app.scroll, 0); // Can't go below 0
}

#[test]
fn test_toggle_palette() {
    let mut app = App::new("test");
    assert_eq!(app.state, AppState::Chatting);

    app.toggle_palette();
    assert_eq!(app.state, AppState::Palette);

    app.toggle_palette();
    assert_eq!(app.state, AppState::Chatting);
}

#[test]
fn test_on_enter() {
    let mut app = App::new("test");
    app.input = "hello world".into();
    app.cursor = 11;

    let result = app.on_enter();
    assert_eq!(result, Some("hello world".into()));
    assert!(app.input.is_empty());
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_on_enter_empty() {
    let mut app = App::new("test");
    let result = app.on_enter();
    assert!(result.is_none());
}

#[test]
fn test_on_escape_from_palette() {
    let mut app = App::new("test");
    app.state = AppState::Palette;

    app.on_escape();
    assert_eq!(app.state, AppState::Chatting);
}

#[test]
fn test_on_escape_from_confirming() {
    let mut app = App::new("test");
    app.state = AppState::Confirming("test action".into());

    app.on_escape();
    assert_eq!(app.state, AppState::Chatting);
}

#[test]
fn test_on_escape_from_chatting_shows_confirmation() {
    let mut app = App::new("test");
    app.state = AppState::Chatting;

    app.on_escape();
    // When in clean Chatting state, Esc shows exit confirmation
    assert!(matches!(app.state, AppState::Confirming(_)));
}

#[test]
fn test_on_escape_from_chatting_clears_input() {
    let mut app = App::new("test");
    app.state = AppState::Chatting;
    app.input = "some text".into();
    app.cursor = 9;

    app.on_escape();
    // First escape clears input
    assert_eq!(app.state, AppState::Chatting);
    assert!(app.input.is_empty());
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_on_escape_closes_help() {
    let mut app = App::new("test");
    app.state = AppState::Help;

    app.on_escape();
    assert_eq!(app.state, AppState::Chatting);
}

#[test]
fn test_on_escape_closes_file_browser() {
    let mut app = App::new("test");
    app.state = AppState::FileBrowser;

    app.on_escape();
    assert_eq!(app.state, AppState::Chatting);
}

#[test]
fn test_on_escape_cancels_task() {
    let mut app = App::new("test");
    app.state = AppState::RunningTask;
    app.task_progress = Some(TaskProgress {
        description: "Test task".into(),
        current_step: 1,
        total_steps: None,
        current_action: "Testing".into(),
        elapsed_secs: 0,
    });

    app.on_escape();
    assert_eq!(app.state, AppState::Chatting);
    assert!(app.task_progress.is_none());
}

#[test]
fn test_on_escape_resets_palette() {
    let mut app = App::new("test");
    app.state = AppState::Palette;
    // Type some text in the palette
    app.palette.on_char('t');
    app.palette.on_char('e');
    app.palette.on_char('s');
    app.palette.on_char('t');

    app.on_escape();
    // Should return to chatting state
    assert_eq!(app.state, AppState::Chatting);
    // Palette is reset (we verify by checking it works correctly when reopened)
    app.toggle_palette();
    assert_eq!(app.state, AppState::Palette);
}

#[test]
fn test_set_progress() {
    let mut app = App::new("test");
    let progress = TaskProgress {
        description: "Test task".into(),
        current_step: 3,
        total_steps: Some(10),
        current_action: "Testing".into(),
        elapsed_secs: 120,
    };

    app.set_progress(progress);
    assert_eq!(app.state, AppState::RunningTask);
    assert!(app.task_progress.is_some());
}

#[test]
fn test_clear_progress() {
    let mut app = App::new("test");
    let progress = TaskProgress {
        description: "Test".into(),
        current_step: 1,
        total_steps: None,
        current_action: "".into(),
        elapsed_secs: 0,
    };
    app.set_progress(progress);

    app.clear_progress();
    assert_eq!(app.state, AppState::Chatting);
    assert!(app.task_progress.is_none());
}

#[test]
fn test_input_in_palette_mode() {
    let mut app = App::new("test");
    app.state = AppState::Palette;

    app.on_char('a');
    // Input should go to palette, not main input
    assert!(app.input.is_empty());
}

#[test]
fn test_up_down_in_palette_mode() {
    let mut app = App::new("test");
    app.toggle_palette();
    assert_eq!(app.state, AppState::Palette);

    // Navigation in palette mode should work without panic
    app.on_down();
    app.on_up();
    // Navigation is handled by palette internally
}

#[test]
fn test_message_role_equality() {
    assert_eq!(MessageRole::User, MessageRole::User);
    assert_ne!(MessageRole::User, MessageRole::Assistant);
}

#[test]
fn test_app_state_equality() {
    assert_eq!(AppState::Chatting, AppState::Chatting);
    assert_ne!(AppState::Chatting, AppState::Palette);
    assert_eq!(
        AppState::Confirming("a".into()),
        AppState::Confirming("a".into())
    );
}

#[test]
fn test_animation_speed_default() {
    let app = App::new("test");
    assert!((app.animation_speed - ANIMATION_SPEED_DEFAULT).abs() < 0.001);
}

#[test]
fn test_on_plus_increases_speed() {
    let mut app = App::new("test");
    let original = app.animation_speed;
    app.on_plus();
    assert!(app.animation_speed > original);
}

#[test]
fn test_on_minus_decreases_speed() {
    let mut app = App::new("test");
    app.on_plus(); // First increase so we can decrease
    let speed_after_plus = app.animation_speed;
    app.on_minus();
    assert!(app.animation_speed < speed_after_plus);
}

#[test]
fn test_animation_speed_max_cap() {
    let mut app = App::new("test");
    // Increase many times
    for _ in 0..20 {
        app.on_plus();
    }
    assert!(app.animation_speed <= ANIMATION_SPEED_MAX);
}

#[test]
fn test_animation_speed_min_cap() {
    let mut app = App::new("test");
    // Decrease many times
    for _ in 0..20 {
        app.on_minus();
    }
    assert!(app.animation_speed >= ANIMATION_SPEED_MIN);
}

#[test]
fn test_animation_delay_inversely_proportional() {
    let mut app = App::new("test");
    let normal_delay = app.animation_delay_ms();

    app.animation_speed = 2.0;
    let fast_delay = app.animation_delay_ms();

    // Faster speed should mean shorter delay
    assert!(fast_delay < normal_delay);
}

#[test]
fn test_animation_speed_display() {
    let mut app = App::new("test");
    app.animation_speed = 1.5;
    assert_eq!(app.animation_speed_display(), "150%");
}

#[test]
fn multibyte_input_does_not_panic_and_edits_correctly() {
    let mut app = App::new("m");
    app.state = AppState::Chatting;
    // Type an emoji then ASCII — byte offsets must stay on char boundaries.
    for c in "a🦊b".chars() {
        app.on_char(c);
    }
    assert_eq!(app.input, "a🦊b");
    assert_eq!(app.cursor, 3);
    // Backspace removes the ASCII 'b', then the multibyte fox — no panic.
    app.on_backspace();
    assert_eq!(app.input, "a🦊");
    app.on_backspace();
    assert_eq!(app.input, "a");
    assert_eq!(app.cursor, 1);
}

#[test]
fn streaming_assistant_accumulates_then_clears() {
    let mut app = App::new("m");
    assert!(app.streaming_assistant.is_none());
    app.append_streaming("Hel");
    app.append_streaming("lo");
    assert_eq!(app.streaming_assistant.as_deref(), Some("Hello"));
    let n = app.messages.len();
    app.clear_streaming();
    assert!(app.streaming_assistant.is_none());
    app.add_assistant_message("Hello");
    assert_eq!(app.messages.len(), n + 1);
}

#[test]
fn update_step_progress_drives_gauge_and_header() {
    let mut app = App::new("glm-5.2");
    // Idle: header has no step segment.
    assert!(!app.header_title().contains("step"));

    // A live "Step 3/200" signal advances the gauge from nothing.
    app.update_step_progress(3, 200);
    let p = app.task_progress.as_ref().expect("progress created");
    assert_eq!(p.current_step, 3);
    assert_eq!(p.total_steps, Some(200));
    assert_eq!(app.state, AppState::RunningTask);
    let title = app.header_title();
    assert!(title.contains("step 3/200"), "header shows step: {title}");
    assert!(title.contains("glm-5.2"));

    // A later signal updates in place (no duplicate progress entry).
    app.update_step_progress(4, 200);
    assert_eq!(app.task_progress.as_ref().unwrap().current_step, 4);
    assert!(app.header_title().contains("step 4/200"));
}

#[test]
fn clear_chat_resets_task_progress_and_usage() {
    let mut app = App::new("m");
    // Simulate an active task with accumulated usage.
    app.set_progress(TaskProgress {
        description: "task".into(),
        current_step: 1,
        total_steps: Some(3),
        current_action: "working".into(),
        elapsed_secs: 5,
    });
    app.status_line.tokens_used = (1234, 567);
    app.status_line.context_percent = 42.0;
    app.append_streaming("partial");
    assert!(app.task_progress.is_some());
    assert_eq!(app.state, AppState::RunningTask);

    app.clear_chat();

    // Everything tied to the (now-reset) conversation is gone.
    assert!(app.task_progress.is_none());
    assert_eq!(app.state, AppState::Chatting);
    assert!(app.streaming_assistant.is_none());
    assert_eq!(app.status_line.tokens_used, (0, 0));
    assert_eq!(app.status_line.context_percent, 0.0);
    // Only the "Chat cleared." system message remains.
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.scroll, 0);
}
