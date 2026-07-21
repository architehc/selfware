//! Task Display System
//!
//! A live task display with animated fox mascot, elapsed time tracking,
//! token counting, and tool call statistics. Inspired by Claude Code's
//! compact status line, but with Selfware's workshop personality.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use colored::Colorize;

use super::style::{Glyphs, SelfwareStyle};
use super::theme::current_theme;

// ============================================================================
// Fox Animation Frames
// ============================================================================

/// Animated fox frames for task execution display.
const FOX_FRAMES: [&[&str]; 3] = [
    // Frame 0: Eyes open
    &[
        r"  /\___/\  ",
        r" ( o   o ) ",
        r" (  =^=  ) ",
        r"  )     (  ",
        r" (       ) ",
    ],
    // Frame 1: Eyes closed (blink)
    &[
        r"  /\___/\  ",
        r" ( -   - ) ",
        r" (  =^=  ) ",
        r"  )     (  ",
        r" (       ) ",
    ],
    // Frame 2: Tail wag
    &[
        r"  /\___/\  ",
        r" ( o   o ) ",
        r" (  =^=  )~",
        r"  )     (  ",
        r" (       ) ",
    ],
];

/// Single-line fox for compact status display.
const FOX_INLINE: &str = "/\\___/\\";

// ============================================================================
// TaskDisplay
// ============================================================================

/// Live task display with animation, timing, and token tracking.
///
/// All counters use atomics so they can be updated from any thread
/// without holding a lock. The `current_tool` field uses a `Mutex`
/// because it stores a variable-length `String`.
pub struct TaskDisplay {
    /// Short description of the current task.
    pub task_description: String,
    /// When the task started.
    pub start_time: Instant,
    /// Cumulative input tokens consumed.
    pub tokens_in: AtomicU64,
    /// Cumulative output tokens generated.
    pub tokens_out: AtomicU64,
    /// Number of tool calls made so far.
    pub tool_calls: AtomicU64,
    /// Name of the currently executing tool (empty if idle).
    pub current_tool: Mutex<String>,
    /// Current animation frame index.
    pub animation_frame: AtomicU64,
    /// Per-tool call counts for the completion summary.
    tool_histogram: Mutex<HashMap<String, u64>>,
    /// File-change tallies: (created, modified).
    file_stats: Mutex<(u64, u64)>,
}

impl TaskDisplay {
    /// Create a new task display with the given description.
    pub fn new(description: &str) -> Self {
        Self {
            task_description: description.to_string(),
            start_time: Instant::now(),
            tokens_in: AtomicU64::new(0),
            tokens_out: AtomicU64::new(0),
            tool_calls: AtomicU64::new(0),
            current_tool: Mutex::new(String::new()),
            animation_frame: AtomicU64::new(0),
            tool_histogram: Mutex::new(HashMap::new()),
            file_stats: Mutex::new((0, 0)),
        }
    }

    // ────────────────────────────────────────────────────────────────
    // Mutation helpers
    // ────────────────────────────────────────────────────────────────

    /// Increment token counters.
    pub fn update_tokens(&self, input: u64, output: u64) {
        self.tokens_in.fetch_add(input, Ordering::Relaxed);
        self.tokens_out.fetch_add(output, Ordering::Relaxed);
    }

    /// Record a tool call and update the histogram.
    pub fn record_tool_call(&self, tool_name: &str) {
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut hist) = self.tool_histogram.lock() {
            *hist.entry(tool_name.to_string()).or_insert(0) += 1;
        }
    }

    /// Update the currently executing tool name.
    pub fn set_current_tool(&self, name: &str) {
        if let Ok(mut current) = self.current_tool.lock() {
            current.clear();
            current.push_str(name);
        }
    }

    /// Record file creation or modification.
    pub fn record_file_change(&self, created: bool) {
        if let Ok(mut stats) = self.file_stats.lock() {
            if created {
                stats.0 += 1;
            } else {
                stats.1 += 1;
            }
        }
    }

    /// Advance the animation frame (call periodically from a timer).
    pub fn advance_animation(&self) {
        self.animation_frame.fetch_add(1, Ordering::Relaxed);
    }

    // ────────────────────────────────────────────────────────────────
    // Rendering
    // ────────────────────────────────────────────────────────────────

    /// Render the current fox animation frame as a multi-line string.
    pub fn render_fox_frame(&self) -> String {
        let idx = self.animation_frame.load(Ordering::Relaxed) as usize % FOX_FRAMES.len();
        let theme = current_theme();
        FOX_FRAMES[idx]
            .iter()
            .map(|line| format!("  {}", line.custom_color(theme.primary)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Render a compact, single-line status suitable for the bottom
    /// of the terminal while a task is running.
    ///
    /// Example output (without ANSI codes):
    /// ```text
    /// /\___/\ Building REST API server ─── 2m 34s │ 12.4K tokens │ 7 tools
    /// ```
    pub fn render_status_line(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let time_str = format_duration(elapsed);
        let total_tokens =
            self.tokens_in.load(Ordering::Relaxed) + self.tokens_out.load(Ordering::Relaxed);
        let token_str = format_tokens(total_tokens);
        let calls = self.tool_calls.load(Ordering::Relaxed);

        let current = self
            .current_tool
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();

        let mut line = format!(
            "{} {} {} {} {} {} tokens {} {} tools",
            FOX_INLINE.custom_color(current_theme().primary),
            self.task_description.as_str().emphasis(),
            Glyphs::horiz().repeat(3).muted(),
            time_str.as_str().timestamp(),
            Glyphs::vert().muted(),
            token_str.as_str().garden_healthy(),
            Glyphs::vert().muted(),
            calls.to_string().as_str().tool_name(),
        );

        if !current.is_empty() {
            line.push_str(&format!(
                " {} {}",
                Glyphs::vert().muted(),
                current.as_str().craftsman_voice(),
            ));
        }

        line
    }

    /// Render a detailed status line with input/output token breakdown.
    ///
    /// Example output (without ANSI codes):
    /// ```text
    /// 🦊 Task: Implement auth │ ⏱ 3m 12s │ 📊 8.2K in / 3.1K out │ 🔧 12 calls │ Currently: file_write
    /// ```
    pub fn render_detailed_status(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let time_str = format_duration(elapsed);
        let tin = self.tokens_in.load(Ordering::Relaxed);
        let tout = self.tokens_out.load(Ordering::Relaxed);
        let calls = self.tool_calls.load(Ordering::Relaxed);

        let current = self
            .current_tool
            .lock()
            .map(|c| c.clone())
            .unwrap_or_default();

        let mut line = format!(
            "{} Task: {} {} {} {} {} {} in / {} out {} {} {} calls",
            Glyphs::sprout(),
            self.task_description.as_str().emphasis(),
            Glyphs::vert().muted(),
            Glyphs::gear(),
            time_str.as_str().timestamp(),
            Glyphs::vert().muted(),
            format_tokens(tin).as_str().garden_healthy(),
            format_tokens(tout).as_str().garden_wilting(),
            Glyphs::vert().muted(),
            Glyphs::wrench(),
            calls.to_string().as_str().tool_name(),
        );

        if !current.is_empty() {
            line.push_str(&format!(
                " {} Currently: {}",
                Glyphs::vert().muted(),
                current.as_str().craftsman_voice(),
            ));
        }

        line
    }

    /// Render a boxed completion summary.
    pub fn render_completion_summary(&self) -> String {
        let elapsed = self.start_time.elapsed();
        let time_str = format_duration(elapsed);
        let tin = self.tokens_in.load(Ordering::Relaxed);
        let tout = self.tokens_out.load(Ordering::Relaxed);
        let calls = self.tool_calls.load(Ordering::Relaxed);

        // Build tool histogram string
        let hist_str = if let Ok(hist) = self.tool_histogram.lock() {
            if hist.is_empty() {
                "none".to_string()
            } else {
                let mut pairs: Vec<_> = hist.iter().collect();
                pairs.sort_by(|a, b| b.1.cmp(a.1));
                let parts: Vec<String> = pairs
                    .iter()
                    .take(6)
                    .map(|(name, count)| format!("{} x{}", name, count))
                    .collect();
                parts.join(", ")
            }
        } else {
            "unknown".to_string()
        };

        let (files_created, files_modified) = self.file_stats.lock().map(|s| *s).unwrap_or((0, 0));

        // Determine the box inner width
        let desc_line = format!("{} {}", Glyphs::bloom(), self.task_description);
        let dur_line = format!("Duration: {}", time_str);
        let tok_line = format!(
            "Tokens:   {} in / {} out",
            format_tokens_with_commas(tin),
            format_tokens_with_commas(tout),
        );
        let tool_line = format!("Tools:    {} calls ({})", calls, hist_str);
        let file_line = format!(
            "Files:    {} created, {} modified",
            files_created, files_modified
        );

        let content_lines = [&desc_line, &dur_line, &tok_line, &tool_line, &file_line];
        let inner_width = content_lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(40)
            .max(40)
            + 2; // padding

        let h = Glyphs::horiz();
        let v = Glyphs::vert();

        let top = format!(
            "{} Task Complete {}{}",
            Glyphs::corner_tl(),
            h.repeat(inner_width.saturating_sub(15)),
            Glyphs::corner_tr(),
        );
        let bottom = format!(
            "{}{}{}",
            Glyphs::corner_bl(),
            h.repeat(inner_width + 2),
            Glyphs::corner_br(),
        );

        let pad = |s: &str| -> String {
            let chars = s.chars().count();
            let remaining = inner_width.saturating_sub(chars);
            format!("{} {}{} {}", v, s, " ".repeat(remaining), v)
        };
        let empty_line = pad("");

        let mut result = String::new();
        result.push_str(&format!("{}\n", top.as_str().muted()));
        result.push_str(&format!("{}\n", pad(&desc_line).as_str().garden_healthy()));
        result.push_str(&format!("{}\n", empty_line.as_str().muted()));
        result.push_str(&format!("{}\n", pad(&dur_line).as_str().muted()));
        result.push_str(&format!("{}\n", pad(&tok_line).as_str().muted()));
        result.push_str(&format!("{}\n", pad(&tool_line).as_str().muted()));
        result.push_str(&format!("{}\n", pad(&file_line).as_str().muted()));
        result.push_str(&format!("{}", bottom.as_str().muted()));

        result
    }
}

// ============================================================================
// Welcome Banner
// ============================================================================

/// Render the Selfware welcome banner with version and tagline.
pub fn render_welcome_banner() -> String {
    let h = Glyphs::horiz();
    let v = Glyphs::vert();
    let width = 43;

    let top = format!(
        "{}{}{}",
        Glyphs::corner_tl(),
        h.repeat(width),
        Glyphs::corner_tr(),
    );
    let bottom = format!(
        "{}{}{}",
        Glyphs::corner_bl(),
        h.repeat(width),
        Glyphs::corner_br(),
    );

    let title = format!("Selfware Workshop v{}", env!("CARGO_PKG_VERSION"));
    let line1 = format!(
        "{}  {} {}{}{}",
        v,
        Glyphs::sprout(),
        title,
        " ".repeat(width.saturating_sub(6 + title.len())),
        v,
    );
    let line2 = format!(
        "{}  Software that improves itself.{}{}",
        v,
        " ".repeat(width - 34),
        v,
    );
    let line3 = format!(
        "{}  Local-first. Privacy-owned.{}{}",
        v,
        " ".repeat(width - 31),
        v,
    );

    format!(
        "{}\n{}\n{}\n{}\n{}",
        top.as_str().muted(),
        line1.as_str().emphasis(),
        line2.as_str().craftsman_voice(),
        line3.as_str().craftsman_voice(),
        bottom.as_str().muted(),
    )
}

// ============================================================================
// Formatting helpers
// ============================================================================

/// Format a duration as a human-readable string.
///
/// - Less than 60 s: `"42s"`
/// - 60 s or more:   `"3m 12s"`
/// - 60 min or more: `"1h 5m 30s"`
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs < 60 {
        format!("{}s", total_secs)
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}m {}s", mins, secs)
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;
        format!("{}h {}m {}s", hours, mins, secs)
    }
}

/// Format a token count in compact notation.
///
/// - Under 1000: the raw number, e.g. `"847"`
/// - 1000+:      one decimal, e.g. `"1.2K"`, `"12.3K"`, `"1.5M"`
pub fn format_tokens(count: u64) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 1_000_000 {
        let k = count as f64 / 1_000.0;
        format!("{:.1}K", k)
    } else {
        let m = count as f64 / 1_000_000.0;
        format!("{:.1}M", m)
    }
}

/// Format a token count with comma separators for the summary box.
///
/// e.g. `8247` -> `"8,247"`, `1234567` -> `"1,234,567"`
pub fn format_tokens_with_commas(count: u64) -> String {
    let s = count.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/ui/task_display/task_display_test.rs"]
mod tests;
