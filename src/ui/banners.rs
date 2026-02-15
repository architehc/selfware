//! ASCII Art Banners for Selfware UI
//!
//! Pre-designed ASCII art banners, logos, and decorative elements.
//! All banners follow the Selfware aesthetic: warm, organic, personal.

use std::fmt;

// ============================================================================
// Banner Types
// ============================================================================

/// A banner that can be displayed in the terminal
pub struct Banner {
    lines: Vec<String>,
    width: usize,
}

impl Banner {
    /// Create a new banner from lines
    pub fn new(lines: Vec<&str>) -> Self {
        let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
        Self {
            lines: lines.into_iter().map(String::from).collect(),
            width,
        }
    }

    /// Get the banner width
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get the banner height (number of lines)
    pub fn height(&self) -> usize {
        self.lines.len()
    }

    /// Get the lines
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Center the banner within a given width
    pub fn centered(&self, total_width: usize) -> Vec<String> {
        self.lines
            .iter()
            .map(|line| {
                if total_width > line.chars().count() {
                    let padding = (total_width - line.chars().count()) / 2;
                    format!("{}{}", " ".repeat(padding), line)
                } else {
                    line.clone()
                }
            })
            .collect()
    }

    /// Add a box around the banner
    pub fn boxed(&self) -> Self {
        let inner_width = self.width + 2;
        let mut new_lines = Vec::with_capacity(self.lines.len() + 2);

        // Top border
        new_lines.push(format!("╔{}╗", "═".repeat(inner_width)));

        // Content with padding
        for line in &self.lines {
            let padding = self.width - line.chars().count();
            new_lines.push(format!("║ {}{} ║", line, " ".repeat(padding)));
        }

        // Bottom border
        new_lines.push(format!("╚{}╝", "═".repeat(inner_width)));

        Banner::new(new_lines.iter().map(|s| s.as_str()).collect())
    }
}

impl fmt::Display for Banner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.lines {
            writeln!(f, "{}", line)?;
        }
        Ok(())
    }
}

// ============================================================================
// Selfware Logo Banners
// ============================================================================

/// Main Selfware logo banner
pub fn selfware_logo() -> Banner {
    Banner::new(vec![
        "███████╗███████╗██╗     ███████╗██╗    ██╗ █████╗ ██████╗ ███████╗",
        "██╔════╝██╔════╝██║     ██╔════╝██║    ██║██╔══██╗██╔══██╗██╔════╝",
        "███████╗█████╗  ██║     █████╗  ██║ █╗ ██║███████║██████╔╝█████╗  ",
        "╚════██║██╔══╝  ██║     ██╔══╝  ██║███╗██║██╔══██║██╔══██╗██╔══╝  ",
        "███████║███████╗███████╗██║     ╚███╔███╔╝██║  ██║██║  ██║███████╗",
        "╚══════╝╚══════╝╚══════╝╚═╝      ╚══╝╚══╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚══════╝",
    ])
}

/// Compact Selfware logo
pub fn selfware_compact() -> Banner {
    Banner::new(vec![
        "╭───────────────╮",
        "│  SELFWARE    │",
        "│  ╭─────────╮  │",
        "│  │ 🌱 → 🌳 │  │",
        "│  ╰─────────╯  │",
        "╰───────────────╯",
    ])
}

/// Small Selfware badge
pub fn selfware_badge() -> Banner {
    Banner::new(vec![
        "┌──────────────┐",
        "│ ✿ SELFWARE ✿ │",
        "└──────────────┘",
    ])
}

// ============================================================================
// Alternate Logo Banners
// ============================================================================

/// Selfware agent logo (alternate)
pub fn selfware_logo_alt() -> Banner {
    Banner::new(vec![
        "██╗  ██╗██╗███╗   ███╗██╗",
        "██║ ██╔╝██║████╗ ████║██║",
        "█████╔╝ ██║██╔████╔██║██║",
        "██╔═██╗ ██║██║╚██╔╝██║██║",
        "██║  ██╗██║██║ ╚═╝ ██║██║",
        "╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚═╝",
    ])
}

/// Selfware with tagline (alternate)
pub fn selfware_with_tagline_alt() -> Banner {
    Banner::new(vec![
        "╭─────────────────────────────────╮",
        "│    ██╗  ██╗██╗███╗   ███╗██╗    │",
        "│    ██║ ██╔╝██║████╗ ████║██║    │",
        "│    █████╔╝ ██║██╔████╔██║██║    │",
        "│    ██╔═██╗ ██║██║╚██╔╝██║██║    │",
        "│    ██║  ██╗██║██║ ╚═╝ ██║██║    │",
        "│    ╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚═╝    │",
        "│                                 │",
        "│    Your Coding Companion 🌱     │",
        "╰─────────────────────────────────╯",
    ])
}

// ============================================================================
// Status Banners
// ============================================================================

/// Success banner
pub fn success_banner() -> Banner {
    Banner::new(vec![
        "╔════════════════════════════════╗",
        "║    ✓ SUCCESS                   ║",
        "║    ═══════════════════════     ║",
        "║    Task completed!  🌳         ║",
        "╚════════════════════════════════╝",
    ])
}

/// Error banner
pub fn error_banner() -> Banner {
    Banner::new(vec![
        "╔════════════════════════════════╗",
        "║    ✗ ERROR                     ║",
        "║    ═══════════════════════     ║",
        "║    Something went wrong ❄️     ║",
        "╚════════════════════════════════╝",
    ])
}

/// Warning banner
pub fn warning_banner() -> Banner {
    Banner::new(vec![
        "╔════════════════════════════════╗",
        "║    ⚠ WARNING                   ║",
        "║    ═══════════════════════     ║",
        "║    Please review carefully 🥀  ║",
        "╚════════════════════════════════╝",
    ])
}

/// Welcome banner
pub fn welcome_banner() -> Banner {
    Banner::new(vec![
        "╭────────────────────────────────────────╮",
        "│                                        │",
        "│   Welcome to Selfware Workshop 🌱      │",
        "│   Your Personal AI Coding Companion    │",
        "│                                        │",
        "│   Type your request to begin...        │",
        "│                                        │",
        "╰────────────────────────────────────────╯",
    ])
}

/// Goodbye banner
pub fn goodbye_banner() -> Banner {
    Banner::new(vec![
        "╭────────────────────────────────────────╮",
        "│                                        │",
        "│   Until next time! 🌳                  │",
        "│   Your digital garden grows stronger   │",
        "│                                        │",
        "╰────────────────────────────────────────╯",
    ])
}

// ============================================================================
// Decorative Elements
// ============================================================================

/// Horizontal divider (simple)
pub fn divider_simple(width: usize) -> String {
    "─".repeat(width)
}

/// Horizontal divider (double line)
pub fn divider_double(width: usize) -> String {
    "═".repeat(width)
}

/// Horizontal divider (dotted)
pub fn divider_dotted(width: usize) -> String {
    "·".repeat(width)
}

/// Horizontal divider (dashed)
pub fn divider_dashed(width: usize) -> String {
    "╌".repeat(width)
}

/// Horizontal divider with center text
pub fn divider_with_text(text: &str, width: usize) -> String {
    let text_len = text.chars().count() + 2; // +2 for spaces
    if width <= text_len {
        return format!(" {} ", text);
    }
    let side_len = (width - text_len) / 2;
    let left = "─".repeat(side_len);
    let right = "─".repeat(width - side_len - text_len);
    format!("{} {} {}", left, text, right)
}

/// Section header
pub fn section_header(title: &str) -> Banner {
    let width = title.chars().count() + 4;
    Banner::new(vec![
        &format!("╭{}╮", "─".repeat(width)),
        &format!("│  {}  │", title),
        &format!("╰{}╯", "─".repeat(width)),
    ])
}

/// Step indicator
pub fn step_indicator(current: usize, total: usize) -> String {
    let filled = "●".repeat(current.min(total));
    let empty = "○".repeat(total.saturating_sub(current));
    format!("[ {} {} ]", filled, empty)
}

// ============================================================================
// Progress and Status Elements
// ============================================================================

/// Task progress box
pub fn task_progress(title: &str, current: usize, total: usize, status: &str) -> Banner {
    let progress_width = 20;
    let filled = ((current as f64 / total as f64) * progress_width as f64) as usize;
    let empty = progress_width - filled;
    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));
    let percent = format!("{:.0}%", (current as f64 / total as f64) * 100.0);

    Banner::new(vec![
        &format!("╭─ {} ─────────────────────────╮", title),
        "│                                    │",
        &format!("│  Progress: {} {}       │", bar, percent),
        &format!("│  Status: {}                        │", status)
            .get(0..40)
            .unwrap_or("│  Status: ..."),
        "│                                    │",
        "╰────────────────────────────────────╯",
    ])
}

/// Metric display
pub fn metric_box(label: &str, value: &str, unit: &str) -> Banner {
    Banner::new(vec![
        "┌──────────────────┐",
        &format!("│ {:<16} │", label),
        &format!("│ {:>12} {} │", value, unit),
        "└──────────────────┘",
    ])
}

// ============================================================================
// Garden Theme Elements
// ============================================================================

/// Garden growth stages
pub fn growth_stage(stage: usize) -> &'static str {
    match stage {
        0 => "🌱 Seedling",
        1 => "🌿 Sprouting",
        2 => "🍃 Growing",
        3 => "🌲 Maturing",
        _ => "🌳 Flourishing",
    }
}

/// Garden border with vines
pub fn garden_border(width: usize) -> Banner {
    let vine_top = "🌿".to_string() + &"─".repeat(width.saturating_sub(4)) + "🌿";
    let vine_bottom = "🌱".to_string() + &"─".repeat(width.saturating_sub(4)) + "🌱";
    Banner::new(vec![&vine_top, &vine_bottom])
}

/// Seasonal decorators
pub fn seasonal_icon(month: u32) -> &'static str {
    match month {
        3..=5 => "🌸",  // Spring
        6..=8 => "☀️",  // Summer
        9..=11 => "🍂", // Autumn
        _ => "❄️",      // Winter
    }
}

// ============================================================================
// Tool Output Frames
// ============================================================================

/// Frame for tool output
pub fn tool_output_frame(tool_name: &str, success: bool) -> (String, String) {
    let icon = if success { "✓" } else { "✗" };
    let top = format!("╭─ {} {} ─────────────────────────────╮", icon, tool_name);
    let bottom = "╰─────────────────────────────────────────╯".to_string();
    (top, bottom)
}

/// Code block frame
pub fn code_frame(language: &str) -> (String, String) {
    let top = format!("```{}", language);
    let bottom = "```".to_string();
    (top, bottom)
}

// ============================================================================
// Multi-line Text Formatting
// ============================================================================

/// Wrap text to fit width
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    #[allow(clippy::int_plus_one)]
    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + word.len() + 1 <= width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

/// Create a text box with wrapped text
pub fn text_box(text: &str, width: usize) -> Banner {
    let inner_width = width.saturating_sub(4);
    let wrapped = wrap_text(text, inner_width);

    let mut lines = Vec::new();
    lines.push(format!("╭{}╮", "─".repeat(width - 2)));

    for line in wrapped {
        let padding = inner_width.saturating_sub(line.chars().count());
        lines.push(format!("│ {}{} │", line, " ".repeat(padding)));
    }

    lines.push(format!("╰{}╯", "─".repeat(width - 2)));

    Banner::new(lines.iter().map(|s| s.as_str()).collect())
}

// ============================================================================
// Quick Helpers
// ============================================================================

/// Print a banner to stdout
pub fn print_banner(banner: &Banner) {
    print!("{}", banner);
}

/// Print a boxed banner
pub fn print_boxed(banner: &Banner) {
    print!("{}", banner.boxed());
}

/// Print centered banner
pub fn print_centered(banner: &Banner, width: usize) {
    for line in banner.centered(width) {
        println!("{}", line);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
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
}
