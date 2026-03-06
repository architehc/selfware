use viz_sparkline::{sparkline, sparkline_with_stats, trend};
use viz_sparkline::scale::normalize;
use viz_sparkline::render::{render_blocks, render_bar};

// ─── sparkline() ───

#[test]
fn test_sparkline_empty() {
    assert_eq!(sparkline(&[]), "");
}

#[test]
fn test_sparkline_single_value() {
    let s = sparkline(&[42.0]);
    assert!(!s.is_empty(), "Single value should produce a character");
    // Single value should map to middle block (▄ or ▅)
    assert!(
        s.contains('▄') || s.contains('▅'),
        "Single value should map to a middle block character, got: {:?}",
        s
    );
}

#[test]
fn test_sparkline_ascending() {
    let s = sparkline(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    let chars: Vec<char> = s.chars().collect();
    assert_eq!(chars.len(), 8, "Should have 8 characters");
    // First char should be lowest block, last should be highest
    assert_eq!(chars[0], '▁', "First char should be lowest block (▁)");
    assert_eq!(chars[7], '█', "Last char should be highest block (█)");
}

#[test]
fn test_sparkline_descending() {
    let s = sparkline(&[8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]);
    let chars: Vec<char> = s.chars().collect();
    assert_eq!(chars[0], '█', "First char should be highest block");
    assert_eq!(chars[7], '▁', "Last char should be lowest block");
}

#[test]
fn test_sparkline_all_same() {
    let s = sparkline(&[5.0, 5.0, 5.0, 5.0]);
    let chars: Vec<char> = s.chars().collect();
    assert_eq!(chars.len(), 4, "Should have 4 characters");
    // All same values → all should map to middle block
    let first = chars[0];
    for ch in &chars {
        assert_eq!(*ch, first, "All-same values should produce identical blocks");
    }
}

// ─── normalize() ───

#[test]
fn test_normalize_ascending() {
    let result = normalize(&[0.0, 50.0, 100.0]);
    assert_eq!(result.len(), 3);
    assert!((result[0] - 0.0).abs() < 0.01, "Min should normalize to 0.0");
    assert!((result[1] - 0.5).abs() < 0.01, "Mid should normalize to 0.5");
    assert!((result[2] - 1.0).abs() < 0.01, "Max should normalize to 1.0");
}

#[test]
fn test_normalize_single() {
    let result = normalize(&[42.0]);
    assert_eq!(result.len(), 1);
    assert!(
        (result[0] - 0.5).abs() < 0.01,
        "Single element should normalize to 0.5, got {}",
        result[0]
    );
}

#[test]
fn test_normalize_all_same() {
    let result = normalize(&[7.0, 7.0, 7.0]);
    assert_eq!(result.len(), 3);
    for v in &result {
        assert!(
            (v - 0.5).abs() < 0.01,
            "All-same values should normalize to 0.5, got {}",
            v
        );
        assert!(!v.is_nan(), "Normalize must not produce NaN");
    }
}

#[test]
fn test_normalize_empty() {
    assert!(normalize(&[]).is_empty());
}

// ─── render_blocks() ───

#[test]
fn test_render_blocks_min_max() {
    let result = render_blocks(&[0.0, 1.0]);
    let chars: Vec<char> = result.chars().collect();
    assert_eq!(chars[0], '▁', "0.0 should map to lowest block (▁), got {:?}", chars[0]);
    assert_eq!(chars[1], '█', "1.0 should map to highest block (█), got {:?}", chars[1]);
}

#[test]
fn test_render_blocks_middle() {
    let result = render_blocks(&[0.5]);
    let chars: Vec<char> = result.chars().collect();
    // 0.5 * 7 = 3.5, rounds to 4 → BLOCKS[4] = '▅'
    assert!(
        chars[0] == '▄' || chars[0] == '▅',
        "0.5 should map to middle block, got {:?}",
        chars[0]
    );
}

// ─── render_bar() ───

#[test]
fn test_render_bar_full() {
    let bar = render_bar(100.0, 100.0, 10);
    let count = bar.chars().count();
    assert_eq!(count, 10, "Full value should produce full-width bar");
}

#[test]
fn test_render_bar_half() {
    let bar = render_bar(50.0, 100.0, 10);
    let count = bar.chars().count();
    assert_eq!(count, 5, "Half value should produce half-width bar");
}

// ─── sparkline_with_stats() ───

#[test]
fn test_sparkline_with_stats() {
    let s = sparkline_with_stats(&[10.0, 20.0, 30.0]);
    assert!(s.contains("min: 10.0"), "Must show min value. Got: {}", s);
    assert!(s.contains("max: 30.0"), "Must show max value. Got: {}", s);
}

// ─── trend() ───

#[test]
fn test_trend_upward() {
    assert_eq!(trend(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]), "↑");
}

#[test]
fn test_trend_downward() {
    assert_eq!(trend(&[9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0, 2.0, 1.0]), "↓");
}

#[test]
fn test_trend_stable() {
    assert_eq!(trend(&[5.0, 5.0, 5.0, 5.0, 5.0, 5.0]), "→");
}
