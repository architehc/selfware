use super::*;

#[test]
fn test_spinner_tick() {
    let mut spinner = GardenSpinner::new("Loading");
    assert_eq!(spinner.frame, 0);

    spinner.tick();
    assert_eq!(spinner.frame, 1);

    spinner.tick();
    spinner.tick();
    spinner.tick();
    assert_eq!(spinner.frame, 0); // Wrapped around
}

#[test]
fn test_growth_gauge_stages() {
    assert_eq!(GrowthGauge::new(0.1, "").growth_stage(), "Seedling");
    assert_eq!(GrowthGauge::new(0.3, "").growth_stage(), "Sprouting");
    assert_eq!(GrowthGauge::new(0.6, "").growth_stage(), "Growing");
    assert_eq!(GrowthGauge::new(0.9, "").growth_stage(), "Flourishing");
    assert_eq!(GrowthGauge::new(1.0, "").growth_stage(), "Mature");
}

#[test]
fn test_status_indicator_icons() {
    assert_eq!(StatusIndicator::new(StatusType::Success, "").icon(), "✿");
    assert_eq!(StatusIndicator::new(StatusType::Warning, "").icon(), "🥀");
    assert_eq!(StatusIndicator::new(StatusType::Error, "").icon(), "❄️");
}

#[test]
fn test_growth_gauge_clamp() {
    let g1 = GrowthGauge::new(1.5, "test");
    assert_eq!(g1.ratio, 1.0);

    let g2 = GrowthGauge::new(-0.5, "test");
    assert_eq!(g2.ratio, 0.0);
}

#[test]
fn test_spinner_creation() {
    let spinner = GardenSpinner::new("Testing");
    assert_eq!(spinner.frame, 0);
    assert_eq!(spinner.message, "Testing");
}

#[test]
fn test_spinner_chars_all_frames() {
    let mut spinner = GardenSpinner::new("test");
    assert_eq!(spinner.spinner_char(), "🌱");

    spinner.tick();
    assert_eq!(spinner.spinner_char(), "🌿");

    spinner.tick();
    assert_eq!(spinner.spinner_char(), "🍃");

    spinner.tick();
    assert_eq!(spinner.spinner_char(), "🌳");

    spinner.tick();
    assert_eq!(spinner.spinner_char(), "🌱"); // Wrapped
}

#[test]
fn test_growth_gauge_all_stages() {
    // Test boundary values
    assert_eq!(GrowthGauge::new(0.0, "").growth_stage(), "Seedling");
    assert_eq!(GrowthGauge::new(0.25, "").growth_stage(), "Seedling");
    assert_eq!(GrowthGauge::new(0.26, "").growth_stage(), "Sprouting");
    assert_eq!(GrowthGauge::new(0.50, "").growth_stage(), "Sprouting");
    assert_eq!(GrowthGauge::new(0.51, "").growth_stage(), "Growing");
    assert_eq!(GrowthGauge::new(0.75, "").growth_stage(), "Growing");
    assert_eq!(GrowthGauge::new(0.76, "").growth_stage(), "Flourishing");
    assert_eq!(GrowthGauge::new(0.99, "").growth_stage(), "Flourishing");
    assert_eq!(GrowthGauge::new(1.0, "").growth_stage(), "Mature");
}

#[test]
fn test_growth_gauge_bar_chars() {
    let gauge = GrowthGauge::new(0.5, "test");
    let bar = gauge.bar_chars(10);
    assert_eq!(bar.chars().filter(|&c| c == '█').count(), 5);
    assert_eq!(bar.chars().filter(|&c| c == '░').count(), 5);
}

#[test]
fn test_growth_gauge_bar_chars_full() {
    let gauge = GrowthGauge::new(1.0, "test");
    let bar = gauge.bar_chars(10);
    assert_eq!(bar.chars().filter(|&c| c == '█').count(), 10);
    assert_eq!(bar.chars().filter(|&c| c == '░').count(), 0);
}

#[test]
fn test_growth_gauge_bar_chars_empty() {
    let gauge = GrowthGauge::new(0.0, "test");
    let bar = gauge.bar_chars(10);
    assert_eq!(bar.chars().filter(|&c| c == '█').count(), 0);
    assert_eq!(bar.chars().filter(|&c| c == '░').count(), 10);
}

#[test]
fn test_status_indicator_all_icons() {
    assert_eq!(StatusIndicator::new(StatusType::Info, "").icon(), "📋");
    assert_eq!(StatusIndicator::new(StatusType::Loading, "").icon(), "⏳");
}

#[test]
fn test_status_indicator_creation() {
    let indicator = StatusIndicator::new(StatusType::Success, "All good");
    assert_eq!(indicator.label, "All good");
}

#[test]
fn test_status_type_debug() {
    let status = StatusType::Success;
    let debug_str = format!("{:?}", status);
    assert_eq!(debug_str, "Success");
}

#[test]
fn test_tool_output_creation() {
    let output = ToolOutput::new("my_tool", "output data", true);
    assert_eq!(output.tool_name, "my_tool");
    assert_eq!(output.output, "output data");
    assert!(output.success);
}

#[test]
fn test_tool_output_failure() {
    let output = ToolOutput::new("failing_tool", "error message", false);
    assert!(!output.success);
}

#[test]
fn test_growth_gauge_label() {
    let gauge = GrowthGauge::new(0.5, "my_label");
    assert_eq!(gauge.label, "my_label");
    assert_eq!(gauge.ratio, 0.5);
}

#[test]
fn test_spinner_multiple_ticks() {
    let mut spinner = GardenSpinner::new("test");
    for _ in 0..100 {
        spinner.tick();
    }
    // Should be at frame 0 (100 % 4 == 0)
    assert_eq!(spinner.frame, 0);
}

#[test]
fn test_status_indicator_style_returns_style() {
    // Just ensure style() doesn't panic for any variant
    let _ = StatusIndicator::new(StatusType::Success, "").style();
    let _ = StatusIndicator::new(StatusType::Warning, "").style();
    let _ = StatusIndicator::new(StatusType::Error, "").style();
    let _ = StatusIndicator::new(StatusType::Info, "").style();
    let _ = StatusIndicator::new(StatusType::Loading, "").style();
}

#[test]
fn test_status_type_clone() {
    let status = StatusType::Warning;
    let cloned = status;
    assert!(matches!(cloned, StatusType::Warning));
}

#[test]
fn test_growth_gauge_extreme_values() {
    // Values should be clamped
    let g1 = GrowthGauge::new(100.0, "test");
    assert_eq!(g1.ratio, 1.0);

    let g2 = GrowthGauge::new(-100.0, "test");
    assert_eq!(g2.ratio, 0.0);

    let g3 = GrowthGauge::new(f64::INFINITY, "test");
    assert_eq!(g3.ratio, 1.0);
}
