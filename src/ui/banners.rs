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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/ui/banners/banners_test.rs"]
mod tests;
