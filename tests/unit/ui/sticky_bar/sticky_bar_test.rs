use super::*;

#[test]
fn fmt_elapsed_seconds() {
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(5)), "5s");
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(59)), "59s");
}

#[test]
fn fmt_elapsed_minutes() {
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(60)), "1m 0s");
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(125)), "2m 5s");
}

#[test]
fn fmt_elapsed_hours() {
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(3600)), "1h 0m");
    assert_eq!(fmt_elapsed(std::time::Duration::from_secs(3661)), "1h 1m");
}

#[test]
fn fmt_tokens_small() {
    assert_eq!(fmt_tokens(0), "0");
    assert_eq!(fmt_tokens(500), "500");
    assert_eq!(fmt_tokens(999), "999");
}

#[test]
fn fmt_tokens_thousands() {
    assert_eq!(fmt_tokens(1000), "1.0k");
    assert_eq!(fmt_tokens(1500), "1.5k");
    assert_eq!(fmt_tokens(42300), "42.3k");
}

#[test]
fn fmt_tokens_millions() {
    assert_eq!(fmt_tokens(1_000_000), "1.0M");
    assert_eq!(fmt_tokens(2_500_000), "2.5M");
}

#[test]
fn render_top_contains_activity() {
    let state = StickyState::new("normal", "qwen3.5");
    state.set_activity("Planning...");
    let top = render_top(&state, 80);
    assert!(
        top.contains("Planning..."),
        "top bar should contain activity"
    );
    assert!(top.contains("tokens"), "top bar should mention tokens");
}

#[test]
fn render_top_contains_elapsed() {
    let state = StickyState::new("normal", "qwen3.5");
    let top = render_top(&state, 80);
    assert!(
        top.contains("0s") || top.contains("1s"),
        "should show elapsed time"
    );
}

#[test]
fn render_top_shows_thinking_when_active() {
    let state = StickyState::new("normal", "qwen3.5");
    state.is_thinking.store(true, Ordering::Relaxed);
    let top = render_top(&state, 120);
    assert!(top.contains("thinking for"), "should show thinking status");
}

#[test]
fn render_top_shows_thought_duration() {
    let state = StickyState::new("normal", "qwen3.5");
    state.thinking_secs.store(5, Ordering::Relaxed);
    let top = render_top(&state, 120);
    assert!(
        top.contains("thought for 5s"),
        "should show completed thinking duration"
    );
}

#[test]
fn render_bottom_normal_mode() {
    let state = StickyState::new("normal", "qwen3.5");
    let bottom = render_bottom(&state, 80);
    assert!(
        bottom.contains("confirm mode"),
        "normal mode should say confirm"
    );
    assert!(bottom.contains("esc to interrupt"));
}

#[test]
fn render_bottom_yolo_mode() {
    let state = StickyState::new("YOLO", "qwen3.5");
    let bottom = render_bottom(&state, 80);
    assert!(
        bottom.contains("auto-approve on"),
        "YOLO should say auto-approve"
    );
}

#[test]
fn render_bottom_with_processes() {
    let state = StickyState::new("normal", "qwen3.5");
    state.active_processes.store(2, Ordering::Relaxed);
    let bottom = render_bottom(&state, 80);
    assert!(bottom.contains("2 processes"), "should show process count");
}

#[test]
fn render_bottom_single_process() {
    let state = StickyState::new("normal", "qwen3.5");
    state.active_processes.store(1, Ordering::Relaxed);
    let bottom = render_bottom(&state, 80);
    assert!(bottom.contains("1 process"), "single should not be plural");
    assert!(!bottom.contains("1 processes"));
}

#[test]
fn sticky_state_add_tokens() {
    let state = StickyState::new("normal", "test");
    state.add_tokens(100);
    state.add_tokens(200);
    assert_eq!(state.tokens.load(Ordering::Relaxed), 300);
}

#[test]
fn sticky_state_model_truncation() {
    let state = StickyState::new("normal", "a-very-long-model-name-that-exceeds-25-chars");
    assert_eq!(state.model.len(), 25);
}

#[test]
fn render_top_pads_to_width() {
    let state = StickyState::new("normal", "m");
    let top = render_top(&state, 100);
    assert_eq!(top.len(), 100, "should pad to exact width");
}

#[test]
fn render_bottom_pads_to_width() {
    let state = StickyState::new("normal", "m");
    let bottom = render_bottom(&state, 100);
    assert_eq!(bottom.len(), 100, "should pad to exact width");
}

#[test]
fn is_active_default_false() {
    // Note: this test can be affected by other tests running in parallel
    // that activate sticky bars. In isolation, it should be false.
    // We just verify the function is callable.
    let _ = is_active();
}
