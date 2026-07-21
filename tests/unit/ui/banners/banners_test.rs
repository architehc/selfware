use super::*;

#[test]
fn test_banner_creation() {
    let banner = Banner::new(vec!["Hello", "World"]);
    assert_eq!(banner.height(), 2);
    assert_eq!(banner.width(), 5);
}

#[test]
fn test_banner_lines() {
    let banner = Banner::new(vec!["Line 1", "Line 2"]);
    assert_eq!(banner.lines().len(), 2);
    assert_eq!(banner.lines()[0], "Line 1");
}

#[test]
fn test_banner_centered() {
    let banner = Banner::new(vec!["Hi"]);
    let centered = banner.centered(10);
    assert_eq!(centered.len(), 1);
    // "Hi" is 2 chars, 10 - 2 = 8, padding = 4
    assert!(centered[0].starts_with("    "));
}

#[test]
fn test_banner_boxed() {
    let banner = Banner::new(vec!["Test"]);
    let boxed = banner.boxed();
    assert!(boxed.height() > banner.height());
    assert!(boxed.lines()[0].contains("╔"));
}

#[test]
fn test_banner_display() {
    let banner = Banner::new(vec!["Hello"]);
    let displayed = format!("{}", banner);
    assert!(displayed.contains("Hello"));
}

#[test]
fn test_selfware_logo() {
    let logo = selfware_logo();
    assert!(logo.height() > 0);
    assert!(logo.width() > 0);
}

#[test]
fn test_selfware_compact() {
    let logo = selfware_compact();
    assert!(logo.height() > 0);
}

#[test]
fn test_selfware_badge() {
    let badge = selfware_badge();
    assert!(badge.height() == 3);
}

#[test]
fn test_selfware_logo_alt() {
    let logo = selfware_logo_alt();
    assert!(logo.height() > 0);
}

#[test]
fn test_selfware_with_tagline_alt() {
    let logo = selfware_with_tagline_alt();
    assert!(logo.lines().iter().any(|l| l.contains("Companion")));
}

#[test]
fn test_success_banner() {
    let banner = success_banner();
    assert!(banner.lines().iter().any(|l| l.contains("SUCCESS")));
}

#[test]
fn test_error_banner() {
    let banner = error_banner();
    assert!(banner.lines().iter().any(|l| l.contains("ERROR")));
}

#[test]
fn test_warning_banner() {
    let banner = warning_banner();
    assert!(banner.lines().iter().any(|l| l.contains("WARNING")));
}

#[test]
fn test_welcome_banner() {
    let banner = welcome_banner();
    assert!(banner.lines().iter().any(|l| l.contains("Welcome")));
}

#[test]
fn test_goodbye_banner() {
    let banner = goodbye_banner();
    assert!(banner.lines().iter().any(|l| l.contains("next time")));
}

#[test]
fn test_divider_simple() {
    let div = divider_simple(10);
    assert_eq!(div.chars().count(), 10);
    assert!(div.chars().all(|c| c == '─'));
}

#[test]
fn test_divider_double() {
    let div = divider_double(10);
    assert_eq!(div.chars().count(), 10);
    assert!(div.chars().all(|c| c == '═'));
}

#[test]
fn test_divider_dotted() {
    let div = divider_dotted(5);
    assert_eq!(div.chars().count(), 5);
}

#[test]
fn test_divider_dashed() {
    let div = divider_dashed(5);
    assert_eq!(div.chars().count(), 5);
}

#[test]
fn test_divider_with_text() {
    let div = divider_with_text("Test", 20);
    assert!(div.contains("Test"));
    assert!(div.contains("─"));
}

#[test]
fn test_divider_with_text_too_narrow() {
    let div = divider_with_text("Test", 4);
    assert!(div.contains("Test"));
}

#[test]
fn test_section_header() {
    let header = section_header("Title");
    assert!(header.lines().iter().any(|l| l.contains("Title")));
}

#[test]
fn test_step_indicator() {
    let indicator = step_indicator(2, 5);
    assert!(indicator.contains("●●"));
    assert!(indicator.contains("○○○"));
}

#[test]
fn test_step_indicator_overflow() {
    let indicator = step_indicator(10, 5);
    // Should cap at total
    assert!(!indicator.contains("○")); // All filled
}

#[test]
fn test_task_progress() {
    let progress = task_progress("Task", 5, 10, "Running");
    assert!(progress.lines().iter().any(|l| l.contains("Progress")));
}

#[test]
fn test_metric_box() {
    let metric = metric_box("CPU", "85", "%");
    assert!(metric.lines().iter().any(|l| l.contains("CPU")));
    assert!(metric.lines().iter().any(|l| l.contains("85")));
}

#[test]
fn test_growth_stage() {
    assert!(growth_stage(0).contains("Seedling"));
    assert!(growth_stage(1).contains("Sprouting"));
    assert!(growth_stage(2).contains("Growing"));
    assert!(growth_stage(3).contains("Maturing"));
    assert!(growth_stage(4).contains("Flourishing"));
    assert!(growth_stage(100).contains("Flourishing"));
}

#[test]
fn test_garden_border() {
    let border = garden_border(20);
    assert_eq!(border.height(), 2);
}

#[test]
fn test_seasonal_icon() {
    assert!(seasonal_icon(3).contains("🌸")); // Spring
    assert!(seasonal_icon(6).contains("☀")); // Summer (partial match for emoji)
    assert!(seasonal_icon(10).contains("🍂")); // Autumn
    assert!(seasonal_icon(1).contains("❄")); // Winter
}

#[test]
fn test_tool_output_frame_success() {
    let (top, bottom) = tool_output_frame("my_tool", true);
    assert!(top.contains("✓"));
    assert!(top.contains("my_tool"));
    assert!(bottom.contains("╯"));
}

#[test]
fn test_tool_output_frame_failure() {
    let (top, _) = tool_output_frame("my_tool", false);
    assert!(top.contains("✗"));
}

#[test]
fn test_code_frame() {
    let (top, bottom) = code_frame("rust");
    assert_eq!(top, "```rust");
    assert_eq!(bottom, "```");
}

#[test]
fn test_wrap_text() {
    let wrapped = wrap_text("Hello world this is a test", 10);
    assert!(wrapped.len() > 1);
    for line in &wrapped {
        assert!(line.len() <= 10 || line.split_whitespace().count() == 1);
    }
}

#[test]
fn test_wrap_text_single_word() {
    let wrapped = wrap_text("Superlongword", 5);
    assert_eq!(wrapped.len(), 1);
}

#[test]
fn test_wrap_text_empty() {
    let wrapped = wrap_text("", 10);
    assert!(wrapped.is_empty());
}

#[test]
fn test_text_box() {
    let box_ = text_box("Hello world", 20);
    assert!(box_.height() >= 3); // Top, content, bottom
}

#[test]
fn test_banner_empty() {
    let banner = Banner::new(vec![]);
    assert_eq!(banner.height(), 0);
    assert_eq!(banner.width(), 0);
}

#[test]
fn test_banner_centered_narrow() {
    let banner = Banner::new(vec!["Very long line here"]);
    let centered = banner.centered(5); // Narrower than content
    assert_eq!(centered[0], "Very long line here"); // Unchanged
}
