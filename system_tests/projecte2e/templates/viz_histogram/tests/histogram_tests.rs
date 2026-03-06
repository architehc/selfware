use viz_histogram::color;
use viz_histogram::histogram::Histogram;

#[test]
fn test_fg_rgb_red() {
    let code = color::fg_rgb(255, 0, 0);
    assert_eq!(
        code, "\x1b[38;2;255;0;0m",
        "fg_rgb(255, 0, 0) should produce red foreground ANSI code"
    );
}

#[test]
fn test_fg_rgb_blue() {
    let code = color::fg_rgb(0, 0, 255);
    assert_eq!(
        code, "\x1b[38;2;0;0;255m",
        "fg_rgb(0, 0, 255) should produce blue foreground ANSI code"
    );
}

#[test]
fn test_bg_rgb_uses_background_code() {
    let code = color::bg_rgb(128, 64, 32);
    assert!(
        code.starts_with("\x1b[48;2;"),
        "bg_rgb must use code 48 (background), not 38 (foreground). Got: {:?}",
        code
    );
    assert_eq!(
        code, "\x1b[48;2;128;64;32m",
        "bg_rgb should produce background ANSI code"
    );
}

#[test]
fn test_reset_produces_ansi_reset() {
    let code = color::reset();
    assert_eq!(
        code, "\x1b[0m",
        "reset() must return the ANSI reset sequence"
    );
}

#[test]
fn test_histogram_empty() {
    let h = Histogram::new();
    assert!(h.render().is_empty(), "Empty histogram should produce empty string");
}

#[test]
fn test_histogram_single_entry() {
    let mut h = Histogram::new();
    h.add("Test", 50.0, "green");
    let output = h.render();
    assert!(output.contains("Test"), "Must contain the label");
    assert!(output.contains("50.0"), "Must contain the value");
    assert!(output.contains('█'), "Must contain bar characters");
}

#[test]
fn test_histogram_sort_ascending() {
    let mut h = Histogram::new();
    h.sort_ascending = true;
    h.add("Big", 100.0, "red");
    h.add("Small", 10.0, "blue");
    h.add("Medium", 50.0, "green");
    let output = h.render();

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 3, "Should have 3 lines");

    // First line should be the smallest value when ascending
    assert!(
        lines[0].contains("Small") || lines[0].contains("10.0"),
        "First line should contain smallest value when sort_ascending=true.\nGot:\n{}",
        output
    );
    // Last line should be the largest
    assert!(
        lines[2].contains("Big") || lines[2].contains("100.0"),
        "Last line should contain largest value when sort_ascending=true.\nGot:\n{}",
        output
    );
}

#[test]
fn test_histogram_zero_values() {
    let mut h = Histogram::new();
    h.add("Zero1", 0.0, "red");
    h.add("Zero2", 0.0, "blue");
    // Should not panic (no division by zero)
    let output = h.render();
    assert!(output.contains("Zero1"), "Must handle zero values without panic");
    assert!(output.contains("0.0"), "Must display zero values");
}

#[test]
fn test_histogram_labels_not_truncated() {
    let mut h = Histogram::new();
    h.add("VeryLongLabelName", 50.0, "green");
    h.add("Short", 30.0, "red");
    let output = h.render();
    assert!(
        output.contains("VeryLongLabelName"),
        "Long labels must NOT be truncated.\nGot:\n{}",
        output
    );
}

#[test]
fn test_histogram_bar_proportional() {
    let mut h = Histogram::new();
    h.max_bar_width = 20;
    h.add("Full", 100.0, "green");
    h.add("Half", 50.0, "yellow");
    let output = h.render();

    // Count bar chars per line
    let lines: Vec<&str> = output.lines().collect();
    for line in &lines {
        let bar_count = line.matches('█').count();
        if line.contains("Full") {
            assert_eq!(bar_count, 20, "Full value should have max_bar_width bars.\nLine: {}", line);
        }
        if line.contains("Half") {
            assert_eq!(bar_count, 10, "Half value should have half the bars.\nLine: {}", line);
        }
    }
}

#[test]
fn test_color_reset_after_each_bar() {
    let mut h = Histogram::new();
    h.add("A", 10.0, "red");
    h.add("B", 20.0, "blue");
    let output = h.render();

    // Each bar should be followed by a color reset
    let reset_count = output.matches("\x1b[0m").count();
    assert!(
        reset_count >= 2,
        "Each bar must be followed by ANSI reset. Found {} resets in output:\n{:?}",
        reset_count,
        output
    );
}

#[test]
fn test_named_fg_colors() {
    // Test that named colors produce valid ANSI codes
    for name in &["red", "green", "blue", "yellow", "cyan", "magenta", "white"] {
        let code = color::named_fg(name);
        assert!(
            code.starts_with("\x1b[38;2;"),
            "Named color '{}' must produce foreground ANSI code. Got: {:?}",
            name,
            code
        );
        assert!(
            code.ends_with('m'),
            "ANSI code must end with 'm'. Got: {:?}",
            code
        );
    }
}
