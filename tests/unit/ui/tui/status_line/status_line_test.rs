use super::*;

#[test]
fn test_status_line_default() {
    let sl = StatusLine::default();
    assert_eq!(sl.model, "Unknown");
    assert_eq!(sl.tokens_used, (0, 0));
    assert_eq!(sl.context_percent, 0.0);
    assert_eq!(sl.mode, StatusMode::Normal);
    assert!(!sl.connected);
}

#[test]
fn test_status_line_new() {
    let sl = StatusLine::new("qwen3-5-27b");
    assert_eq!(sl.model, "qwen3-5-27b");
    assert_eq!(sl.total_tokens(), 0);
}

#[test]
fn test_total_tokens() {
    let sl = StatusLine {
        tokens_used: (150, 75),
        ..Default::default()
    };
    assert_eq!(sl.total_tokens(), 225);
}

#[test]
fn test_fmt_tokens() {
    assert_eq!(StatusLine::fmt_tokens(500), "500");
    assert_eq!(StatusLine::fmt_tokens(1500), "1.5K");
    assert_eq!(StatusLine::fmt_tokens(1_500_000), "1.5M");
}

#[test]
fn test_mode_labels() {
    assert_eq!(StatusMode::Normal.label(), "NORMAL");
    assert_eq!(StatusMode::Plan.label(), "PLAN");
    assert_eq!(StatusMode::Auto.label(), "AUTO");
    assert_eq!(StatusMode::Yolo.label(), "YOLO");
}

#[test]
fn test_context_style_thresholds() {
    let low = StatusLine {
        context_percent: 50.0,
        ..Default::default()
    };
    let mid = StatusLine {
        context_percent: 80.0,
        ..Default::default()
    };
    let high = StatusLine {
        context_percent: 95.0,
        ..Default::default()
    };
    // Just ensure they don't panic
    let _ = low.context_style();
    let _ = mid.context_style();
    let _ = high.context_style();
}
