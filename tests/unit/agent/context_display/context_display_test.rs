use super::*;

#[test]
fn label_names_both_the_window_and_the_compaction_threshold() {
    // 0.9.1 field test: 21742 tokens read "13.3% (21.7k/164k)" on the
    // status bar and "21742 / 106496 (20.4%)" in /stats.
    assert_eq!(
        context_usage_label(21_742, 163_840, 106_496),
        "21.7k of 164k context (13%) · compaction at 106k"
    );
}

#[test]
fn label_handles_unknown_window_and_tiny_usage() {
    assert_eq!(
        context_usage_label(500, 0, 0),
        "500 context (window unknown)"
    );
    assert_eq!(
        context_usage_label(10, 200_000, 150_000),
        "10 of 200k context (<1%) · compaction at 150k"
    );
    assert_eq!(context_usage_label(0, 8_000, 0), "0 of 8.0k context (0%)");
    // A threshold at or above the window adds nothing.
    assert_eq!(
        context_usage_label(4_000, 8_000, 8_000),
        "4.0k of 8.0k context (50%)"
    );
}

#[test]
fn context_pct_is_bounded() {
    assert_eq!(context_pct(1, 0), 0.0);
    assert_eq!(context_pct(300, 100), 100.0);
    assert!((context_pct(21_742, 163_840) - 13.27).abs() < 0.01);
}

#[test]
fn truncation_marks_the_cut() {
    assert_eq!(
        truncate_with_ellipsis("qwen38-flash-next", 40),
        "qwen38-flash-next"
    );
    assert_eq!(
        truncate_with_ellipsis("qwen38-flash-next", 10),
        "qwen38-fl…"
    );
    assert_eq!(truncate_with_ellipsis("ab", 0), "");
}

#[test]
fn status_bar_keeps_the_full_model_name_when_it_fits() {
    let left = "[normal] ? for shortcuts";
    let middle = "21.7k of 164k context (13%) · compaction at 106k";
    let layout = layout_status_bar(left, middle, "qwen38-flash-next", 160);
    assert!(layout.show_bar);
    assert_eq!(layout.model, "qwen38-flash-next");
    let total = 1 + left.len() + layout.padding + 2 + 11 + middle.chars().count() + 3 + 17;
    assert_eq!(total, 160, "line fills the terminal exactly");
}

#[test]
fn status_bar_drops_the_bar_before_shortening_the_model() {
    let left = "[normal] ? for shortcuts";
    let middle = "21.7k of 164k context (13%) · compaction at 106k";
    // 1 + 24 + 2 + 49 + 3 + 17 = 96 columns without the bar.
    let layout = layout_status_bar(left, middle, "qwen38-flash-next", 100);
    assert!(!layout.show_bar);
    assert_eq!(layout.model, "qwen38-flash-next");

    let narrow = layout_status_bar(left, middle, "qwen38-flash-next", 80);
    assert!(!narrow.show_bar);
    assert!(narrow.model.ends_with('…'), "{}", narrow.model);
    assert!(narrow.model.chars().count() >= 12);
    assert_eq!(narrow.padding, 1);
}
