use super::*;
use ratatui::{backend::TestBackend, Terminal};

fn buffer_row(terminal: &Terminal<TestBackend>, y: u16) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area().width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn permission_overlay_wraps_details_and_keeps_footer_visible() {
    let backend = TestBackend::new(150, 50);
    let mut terminal = Terminal::new(backend).unwrap();
    let reason = "Write a file at /workspace/a/very/deep/project/path/that/needs/to/remain/visible/configuration.json with arguments: { content: this permission explanation is deliberately long enough to wrap onto another line }";

    terminal
        .draw(|frame| {
            render_permission_overlay(frame, frame.area(), "file_write", reason);
        })
        .unwrap();

    let modal = permission_modal_area(Rect::new(0, 0, 150, 50));
    assert!(modal.width > 60, "modal should expand on a wide terminal");
    assert!(modal.height > 8, "modal should expand on a tall terminal");

    let inner = Rect::new(
        modal.x + 1,
        modal.y + 1,
        modal.width.saturating_sub(2),
        modal.height.saturating_sub(2),
    );
    let rendered_details = ((inner.y + 1)..(inner.y + inner.height.saturating_sub(2)))
        .filter(|&y| !buffer_row(&terminal, y).trim().is_empty())
        .count();
    assert!(rendered_details >= 2, "long details should wrap");

    let footer = buffer_row(&terminal, inner.y + inner.height - 1);
    assert!(footer.contains("[y] allow"));
    assert!(footer.contains("[n/Esc] deny"));
}

#[test]
fn permission_overlay_clears_background_and_preserves_footer_when_compact() {
    let backend = TestBackend::new(40, 12);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            let background = vec![Line::from("X".repeat(40)); 12];
            frame.render_widget(Paragraph::new(background), frame.area());
            render_permission_overlay(
                frame,
                frame.area(),
                "shell",
                "Run a deliberately long command with several arguments that must wrap cleanly",
            );
        })
        .unwrap();

    let modal = permission_modal_area(Rect::new(0, 0, 40, 12));
    let inner = Rect::new(
        modal.x + 1,
        modal.y + 1,
        modal.width.saturating_sub(2),
        modal.height.saturating_sub(2),
    );
    let spacer_y = inner.y + inner.height - 2;
    assert_eq!(
        terminal.backend().buffer()[(inner.x, spacer_y)].symbol(),
        " "
    );

    let footer = buffer_row(&terminal, inner.y + inner.height - 1);
    assert!(footer.contains("[y] allow"));
    assert!(footer.contains("[n/Esc] deny"));
}

#[test]
fn test_dashboard_state_default() {
    let state = DashboardState::default();
    assert_eq!(state.model, "Unknown");
    assert_eq!(state.tokens_used, 0);
    // Honest default: not connected until a response arrives.
    assert!(!state.connected);
    assert!(state.active_tools.is_empty());
    assert!(state.logs.is_empty());
}

#[test]
fn test_dashboard_connected_toggles_on_response_and_error() {
    let mut state = DashboardState::default();
    assert!(!state.connected);
    state.process_event(TuiEvent::AssistantDelta { text: "hi".into() });
    assert!(state.connected, "a response delta means connected");
    state.process_event(TuiEvent::AgentError {
        message: "boom".into(),
    });
    assert!(!state.connected, "an agent error clears connected");
    state.process_event(TuiEvent::AgentCompleted {
        message: "ok".into(),
    });
    assert!(state.connected, "a completion re-marks connected");
}

#[test]
fn test_dashboard_state_new() {
    let state = DashboardState::new("test-model");
    assert_eq!(state.model, "test-model");
}

#[test]
fn test_elapsed_formatted() {
    let state = DashboardState::default();
    // Just check it doesn't panic and returns a reasonable format
    let formatted = state.elapsed_formatted();
    assert!(formatted.contains(":"));
}

#[test]
fn test_log_entry() {
    let mut state = DashboardState::default();
    state.log(LogLevel::Info, "Test message");

    assert_eq!(state.logs.len(), 1);
    assert_eq!(state.logs[0].message, "Test message");
    assert_eq!(state.logs[0].level, LogLevel::Info);
}

#[test]
fn test_log_max_capacity() {
    let mut state = DashboardState::default();
    for i in 0..150 {
        state.log(LogLevel::Info, &format!("Message {}", i));
    }

    // Should only keep last 100
    assert_eq!(state.logs.len(), 100);
    // First message should be "Message 50" (0-49 were removed)
    assert!(state.logs[0].message.contains("50"));
}

#[test]
fn test_tool_tracking() {
    let mut state = DashboardState::default();

    state.tool_start("file_read");
    assert_eq!(state.active_tools.len(), 1);
    assert_eq!(state.active_tools[0].name, "file_read");

    state.tool_progress("file_read", 0.5);
    assert_eq!(state.active_tools[0].progress, 0.5);

    state.tool_complete("file_read");
    assert!(state.active_tools.is_empty());
}

#[test]
fn test_tool_progress_clamp() {
    let mut state = DashboardState::default();
    state.tool_start("test");

    state.tool_progress("test", 1.5);
    assert_eq!(state.active_tools[0].progress, 1.0);

    state.tool_progress("test", -0.5);
    assert_eq!(state.active_tools[0].progress, 0.0);
}

#[test]
fn test_log_level_icons() {
    assert_eq!(LogLevel::Info.icon(), "ℹ");
    assert_eq!(LogLevel::Success.icon(), "✓");
    assert_eq!(LogLevel::Warning.icon(), "⚠");
    assert_eq!(LogLevel::Error.icon(), "✗");
    assert_eq!(LogLevel::Debug.icon(), "◇");
}

#[test]
fn test_log_level_style() {
    // Just ensure they don't panic
    let _ = LogLevel::Info.style();
    let _ = LogLevel::Success.style();
    let _ = LogLevel::Warning.style();
    let _ = LogLevel::Error.style();
    let _ = LogLevel::Debug.style();
}

#[test]
fn test_active_tool_elapsed() {
    let tool = ActiveTool {
        name: "test".to_string(),
        progress: 0.0,
        started: Instant::now(),
    };
    // Just ensure it doesn't panic
    let _ = tool.elapsed();
}

#[test]
fn test_process_event_agent_started() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::AgentStarted);
    assert_eq!(state.status_message, "Agent working...");
    assert_eq!(state.logs.len(), 1);
}

#[test]
fn test_process_event_agent_completed() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::AgentCompleted {
        message: "done".to_string(),
    });
    assert_eq!(state.status_message, "Ready");
    assert_eq!(state.logs.len(), 1);
    assert!(state.logs[0].message.contains("done"));
}

#[test]
fn test_process_event_agent_error() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::AgentError {
        message: "something went wrong".to_string(),
    });
    assert!(state.status_message.contains("Error"));
    assert_eq!(state.logs.len(), 1);
    assert_eq!(state.logs[0].level, LogLevel::Error);
}

#[test]
fn test_process_event_tool_started() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::ToolStarted {
        name: "file_read".to_string(),
    });
    assert_eq!(state.active_tools.len(), 1);
    assert_eq!(state.active_tools[0].name, "file_read");
    assert!(state.status_message.contains("file_read"));
}

#[test]
fn test_process_event_tool_completed_success() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::ToolStarted {
        name: "file_read".to_string(),
    });
    state.process_event(TuiEvent::ToolCompleted {
        name: "file_read".to_string(),
        success: true,
        duration_ms: 100,
    });
    assert!(state.active_tools.is_empty());
    assert!(state.logs.last().unwrap().level == LogLevel::Success);
}

#[test]
fn test_process_event_tool_completed_failure() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::ToolStarted {
        name: "shell_exec".to_string(),
    });
    state.process_event(TuiEvent::ToolCompleted {
        name: "shell_exec".to_string(),
        success: false,
        duration_ms: 200,
    });
    assert!(state.active_tools.is_empty());
    assert!(state.logs.last().unwrap().level == LogLevel::Warning);
}

#[test]
fn test_process_event_token_usage() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::TokenUsage {
        prompt_tokens: 100,
        completion_tokens: 50,
    });
    // Total billed usage: prompt + completion.
    assert_eq!(state.tokens_used, 150);
    assert_eq!(state.logs.len(), 1);
}

#[test]
fn test_process_event_status_update() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::StatusUpdate {
        message: "Processing files".to_string(),
    });
    assert_eq!(state.status_message, "Processing files");
    assert_eq!(state.logs.len(), 1);
}

#[test]
fn test_process_event_garden_health_update() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::GardenHealthUpdate { health: 0.75 });
    assert!((state.garden_health - 0.75).abs() < 0.001);

    // Test clamping
    state.process_event(TuiEvent::GardenHealthUpdate { health: 1.5 });
    assert!((state.garden_health - 1.0).abs() < 0.001);
}

#[test]
fn test_process_event_log() {
    let mut state = DashboardState::default();
    state.process_event(TuiEvent::Log {
        level: LogLevel::Warning,
        message: "low memory".to_string(),
    });
    assert_eq!(state.logs.len(), 1);
    assert_eq!(state.logs[0].level, LogLevel::Warning);
    assert_eq!(state.logs[0].message, "low memory");
}

#[test]
fn test_truncate_for_display() {
    assert_eq!(truncate_for_display("hello", 3), "hel");
    assert_eq!(truncate_for_display("hi", 10), "hi");
    assert_eq!(truncate_for_display("", 5), "");
}
