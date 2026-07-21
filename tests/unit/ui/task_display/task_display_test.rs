use super::*;

// ── format_duration ──────────────────────────────────────────

#[test]
fn test_format_duration_seconds() {
    assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    assert_eq!(format_duration(Duration::from_secs(42)), "42s");
    assert_eq!(format_duration(Duration::from_secs(59)), "59s");
}

#[test]
fn test_format_duration_minutes() {
    assert_eq!(format_duration(Duration::from_secs(60)), "1m 0s");
    assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    assert_eq!(format_duration(Duration::from_secs(192)), "3m 12s");
    assert_eq!(format_duration(Duration::from_secs(3599)), "59m 59s");
}

#[test]
fn test_format_duration_hours() {
    assert_eq!(format_duration(Duration::from_secs(3600)), "1h 0m 0s");
    assert_eq!(format_duration(Duration::from_secs(3930)), "1h 5m 30s");
    assert_eq!(format_duration(Duration::from_secs(7261)), "2h 1m 1s");
}

// ── format_tokens ────────────────────────────────────────────

#[test]
fn test_format_tokens_small() {
    assert_eq!(format_tokens(0), "0");
    assert_eq!(format_tokens(42), "42");
    assert_eq!(format_tokens(999), "999");
}

#[test]
fn test_format_tokens_thousands() {
    assert_eq!(format_tokens(1_000), "1.0K");
    assert_eq!(format_tokens(1_234), "1.2K");
    assert_eq!(format_tokens(8_247), "8.2K");
    assert_eq!(format_tokens(12_345), "12.3K");
    assert_eq!(format_tokens(999_999), "1000.0K");
}

#[test]
fn test_format_tokens_millions() {
    assert_eq!(format_tokens(1_000_000), "1.0M");
    assert_eq!(format_tokens(1_500_000), "1.5M");
    assert_eq!(format_tokens(12_345_678), "12.3M");
}

// ── format_tokens_with_commas ────────────────────────────────

#[test]
fn test_format_tokens_with_commas() {
    assert_eq!(format_tokens_with_commas(0), "0");
    assert_eq!(format_tokens_with_commas(42), "42");
    assert_eq!(format_tokens_with_commas(999), "999");
    assert_eq!(format_tokens_with_commas(1_000), "1,000");
    assert_eq!(format_tokens_with_commas(8_247), "8,247");
    assert_eq!(format_tokens_with_commas(1_234_567), "1,234,567");
}

// ── TaskDisplay construction ─────────────────────────────────

#[test]
fn test_task_display_new() {
    let display = TaskDisplay::new("Test task");
    assert_eq!(display.task_description, "Test task");
    assert_eq!(display.tokens_in.load(Ordering::Relaxed), 0);
    assert_eq!(display.tokens_out.load(Ordering::Relaxed), 0);
    assert_eq!(display.tool_calls.load(Ordering::Relaxed), 0);
    assert_eq!(display.animation_frame.load(Ordering::Relaxed), 0);
}

// ── Token updates ────────────────────────────────────────────

#[test]
fn test_update_tokens() {
    let display = TaskDisplay::new("Tokens");
    display.update_tokens(100, 50);
    assert_eq!(display.tokens_in.load(Ordering::Relaxed), 100);
    assert_eq!(display.tokens_out.load(Ordering::Relaxed), 50);

    display.update_tokens(200, 75);
    assert_eq!(display.tokens_in.load(Ordering::Relaxed), 300);
    assert_eq!(display.tokens_out.load(Ordering::Relaxed), 125);
}

// ── Tool tracking ────────────────────────────────────────────

#[test]
fn test_record_tool_call() {
    let display = TaskDisplay::new("Tools");
    display.record_tool_call("file_read");
    display.record_tool_call("file_read");
    display.record_tool_call("shell_exec");

    assert_eq!(display.tool_calls.load(Ordering::Relaxed), 3);

    let hist = display.tool_histogram.lock().unwrap();
    assert_eq!(hist.get("file_read"), Some(&2));
    assert_eq!(hist.get("shell_exec"), Some(&1));
}

#[test]
fn test_set_current_tool() {
    let display = TaskDisplay::new("Current");
    display.set_current_tool("file_write");
    assert_eq!(*display.current_tool.lock().unwrap(), "file_write");

    display.set_current_tool("shell_exec");
    assert_eq!(*display.current_tool.lock().unwrap(), "shell_exec");

    display.set_current_tool("");
    assert_eq!(*display.current_tool.lock().unwrap(), "");
}

// ── File stats ───────────────────────────────────────────────

#[test]
fn test_record_file_change() {
    let display = TaskDisplay::new("Files");
    display.record_file_change(true);
    display.record_file_change(true);
    display.record_file_change(false);

    let stats = display.file_stats.lock().unwrap();
    assert_eq!(*stats, (2, 1));
}

// ── Animation ────────────────────────────────────────────────

#[test]
fn test_advance_animation() {
    let display = TaskDisplay::new("Anim");
    assert_eq!(display.animation_frame.load(Ordering::Relaxed), 0);

    display.advance_animation();
    assert_eq!(display.animation_frame.load(Ordering::Relaxed), 1);

    display.advance_animation();
    display.advance_animation();
    assert_eq!(display.animation_frame.load(Ordering::Relaxed), 3);
}

#[test]
fn test_render_fox_frame_cycles() {
    let display = TaskDisplay::new("Fox");
    let frame0 = display.render_fox_frame();
    assert!(frame0.contains("/\\___/\\"));

    display.advance_animation();
    let frame1 = display.render_fox_frame();
    assert!(frame1.contains("/\\___/\\"));

    // Frames 0 and 1 differ (open eyes vs closed)
    assert_ne!(frame0, frame1);

    // After 3 advances we wrap back to frame 0
    display.advance_animation();
    display.advance_animation();
    let frame3 = display.render_fox_frame();
    assert_eq!(frame0, frame3);
}

// ── Status line rendering ────────────────────────────────────

#[test]
fn test_render_status_line_contains_description() {
    let display = TaskDisplay::new("Building REST API");
    display.update_tokens(5000, 2000);
    display.record_tool_call("file_write");

    let line = display.render_status_line();
    assert!(line.contains("Building REST API"));
    assert!(line.contains("tokens"));
    assert!(line.contains("tools"));
}

#[test]
fn test_render_status_line_with_current_tool() {
    let display = TaskDisplay::new("Task");
    display.set_current_tool("shell_exec");

    let line = display.render_status_line();
    assert!(line.contains("shell_exec"));
}

#[test]
fn test_render_detailed_status() {
    let display = TaskDisplay::new("Implement auth");
    display.update_tokens(8200, 3100);
    display.record_tool_call("file_write");
    display.set_current_tool("file_write");

    let line = display.render_detailed_status();
    assert!(line.contains("Implement auth"));
    assert!(line.contains("in"));
    assert!(line.contains("out"));
    assert!(line.contains("calls"));
    assert!(line.contains("file_write"));
}

// ── Completion summary ───────────────────────────────────────

#[test]
fn test_render_completion_summary() {
    let display = TaskDisplay::new("Implement user auth");
    display.update_tokens(8247, 3102);
    display.record_tool_call("file_write");
    display.record_tool_call("file_write");
    display.record_tool_call("file_read");
    display.record_tool_call("shell_exec");
    display.record_file_change(true);
    display.record_file_change(false);
    display.record_file_change(false);

    let summary = display.render_completion_summary();
    assert!(summary.contains("Task Complete"));
    assert!(summary.contains("Implement user auth"));
    assert!(summary.contains("Duration"));
    assert!(summary.contains("Tokens"));
    assert!(summary.contains("Tools"));
    assert!(summary.contains("Files"));
    assert!(summary.contains("file_write"));
}

#[test]
fn test_render_completion_summary_no_tools() {
    let display = TaskDisplay::new("Empty task");
    let summary = display.render_completion_summary();
    assert!(summary.contains("Task Complete"));
    assert!(summary.contains("none"));
}

// ── Welcome banner ───────────────────────────────────────────

#[test]
fn test_render_welcome_banner() {
    let banner = render_welcome_banner();
    assert!(banner.contains("Selfware Workshop"));
    assert!(banner.contains(env!("CARGO_PKG_VERSION")));
    assert!(banner.contains("Software that improves itself"));
    assert!(banner.contains("Local-first"));
    assert!(banner.contains("Privacy-owned"));
}
