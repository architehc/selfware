use viz_progress_bar::bar::ProgressBar;
use viz_progress_bar::style::BarStyle;
use viz_progress_bar::eta::EtaCalculator;
use viz_progress_bar::multi::MultiProgress;

// ─── BarStyle rendering ───

#[test]
fn test_classic_bar_empty() {
    let bar = BarStyle::Classic.render(0.0, 20);
    assert!(bar.starts_with('['), "Bar must start with [");
    assert!(bar.ends_with(']'), "Bar must end with ]");
    assert!(bar.contains('░'), "Empty bar must have empty chars");
    assert!(!bar.contains('█'), "Empty bar must have no filled chars");
}

#[test]
fn test_classic_bar_full() {
    let bar = BarStyle::Classic.render(1.0, 20);
    assert!(bar.contains('█'), "Full bar must have filled chars");
    let filled_count = bar.matches('█').count();
    assert_eq!(
        filled_count, 20,
        "Full bar must have exactly width filled chars, got {}.\nBar: {}",
        filled_count, bar
    );
}

#[test]
fn test_classic_bar_half() {
    let bar = BarStyle::Classic.render(0.5, 20);
    let filled = bar.matches('█').count();
    let empty = bar.matches('░').count();
    assert_eq!(filled, 10, "Half bar should have 10 filled chars, got {}", filled);
    assert_eq!(empty, 10, "Half bar should have 10 empty chars, got {}", empty);
}

#[test]
fn test_arrow_bar() {
    let bar = BarStyle::Arrow.render(0.5, 20);
    assert!(bar.contains('>'), "Arrow bar must have > character");
    assert!(bar.starts_with('['));
    assert!(bar.ends_with(']'));
}

#[test]
fn test_dots_bar() {
    let bar = BarStyle::Dots.render(0.5, 10);
    assert!(bar.contains('●'), "Dots bar must have filled dots");
    assert!(bar.contains('○'), "Dots bar must have empty dots");
    let filled = bar.matches('●').count();
    assert_eq!(filled, 5, "Half dots bar should have 5 filled, got {}", filled);
}

// ─── Spinner ───

#[test]
fn test_spinner_cycles() {
    // Spinner should cycle through all frames without skipping any
    let frames: Vec<char> = (0..10).map(BarStyle::spinner_frame).collect();
    let expected: Vec<char> = vec!['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    assert_eq!(
        frames, expected,
        "Spinner must cycle through all 10 frames"
    );
}

#[test]
fn test_spinner_wraps() {
    // Frame at index 10 should wrap to frame 0
    let f0 = BarStyle::spinner_frame(0);
    let f10 = BarStyle::spinner_frame(10);
    assert_eq!(f0, f10, "Spinner must wrap around after all frames");
}

// ─── Percentage ───

#[test]
fn test_percentage_zero() {
    assert_eq!(BarStyle::format_percentage(0.0), "0%");
}

#[test]
fn test_percentage_full() {
    assert_eq!(BarStyle::format_percentage(1.0), "100%");
}

#[test]
fn test_percentage_rounds() {
    // 0.999 should round to 100%, not truncate to 99%
    assert_eq!(
        BarStyle::format_percentage(0.999),
        "100%",
        "0.999 should round to 100%, not truncate to 99%"
    );
}

// ─── ProgressBar ───

#[test]
fn test_progress_bar_new() {
    let bar = ProgressBar::new(100);
    assert_eq!(bar.position(), 0);
    assert_eq!(bar.total(), 100);
    assert!(!bar.is_finished());
    assert!((bar.ratio() - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_progress_bar_tick_clamps() {
    let mut bar = ProgressBar::new(100);
    bar.tick(150); // More than total
    // Position should be clamped to total
    assert!(
        bar.ratio() <= 1.0,
        "Ratio must not exceed 1.0 after ticking past total. Got: {}",
        bar.ratio()
    );
}

#[test]
fn test_progress_bar_finish_sets_full() {
    let mut bar = ProgressBar::new(100);
    bar.tick(50);
    bar.finish();
    // After finish(), the bar should render as 100% complete
    assert!(bar.is_finished());
    let rendered = bar.render();
    assert!(
        rendered.contains("100%"),
        "Finished bar must show 100%.\nGot: {}",
        rendered
    );
}

#[test]
fn test_progress_bar_render_contains_bar_and_pct() {
    let mut bar = ProgressBar::new(100);
    bar.set_position(50);
    let output = bar.render();
    assert!(output.contains('['), "Render must contain bar start");
    assert!(output.contains(']'), "Render must contain bar end");
    assert!(output.contains("50%"), "Render must contain percentage");
}

#[test]
fn test_progress_bar_message() {
    let mut bar = ProgressBar::new(100);
    bar.set_message("Downloading...");
    let output = bar.render();
    assert!(
        output.contains("Downloading..."),
        "Render must contain message"
    );
}

// ─── MultiProgress ───

#[test]
fn test_multi_progress_order() {
    let mut mp = MultiProgress::new();
    let mut bar1 = ProgressBar::new(100);
    bar1.set_message("First");
    bar1.set_position(25);

    let mut bar2 = ProgressBar::new(200);
    bar2.set_message("Second");
    bar2.set_position(100);

    mp.add(bar1);
    mp.add(bar2);

    let output = mp.render();
    let lines: Vec<&str> = output.lines().collect();

    assert_eq!(lines.len(), 2, "MultiProgress should have 2 lines");

    // First bar added should appear first in output
    assert!(
        lines[0].contains("First"),
        "First line must be the first bar added.\nGot:\n{}",
        output
    );
    assert!(
        lines[1].contains("Second"),
        "Second line must be the second bar added.\nGot:\n{}",
        output
    );
}

#[test]
fn test_multi_progress_all_finished() {
    let mut mp = MultiProgress::new();
    let idx0 = mp.add(ProgressBar::new(10));
    let idx1 = mp.add(ProgressBar::new(20));

    assert!(!mp.all_finished());

    mp.get_mut(idx0).unwrap().finish();
    assert!(!mp.all_finished());

    mp.get_mut(idx1).unwrap().finish();
    assert!(mp.all_finished());
}
