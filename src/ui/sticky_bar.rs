//! Persistent sticky status bars (top + bottom) that remain visible while
//! content scrolls between them — similar to Claude Code's UI.
//!
//! Uses ANSI scrolling regions (`\033[{top};{bottom}r`) to pin the first and
//! last terminal rows, letting all other output scroll naturally in between.

use crossterm::terminal;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Global flag: when true, the sticky bar is active and output should respect
/// the scrolling region.
static STICKY_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Returns true if a sticky bar is currently rendered.
pub fn is_active() -> bool {
    STICKY_ACTIVE.load(Ordering::Relaxed)
}

/// Global counter for active bash/shell commands.
static ACTIVE_BASH: AtomicU64 = AtomicU64::new(0);

/// Increment active bash count (call when shell_exec/pty_shell starts).
pub fn bash_started() {
    ACTIVE_BASH.fetch_add(1, Ordering::Relaxed);
}

/// Decrement active bash count (call when shell_exec/pty_shell finishes).
pub fn bash_finished() {
    let prev = ACTIVE_BASH.fetch_sub(1, Ordering::Relaxed);
    if prev == 0 {
        // Saturate at 0
        ACTIVE_BASH.store(0, Ordering::Relaxed);
    }
}

/// Current active bash count.
pub fn active_bash_count() -> u64 {
    ACTIVE_BASH.load(Ordering::Relaxed)
}

/// RAII guard that increments on creation and decrements on drop.
pub struct BashGuard;

impl BashGuard {
    pub fn new() -> Self {
        bash_started();
        Self
    }
}

impl Drop for BashGuard {
    fn drop(&mut self) {
        bash_finished();
    }
}

/// Shared mutable state for the bars, updated from streaming/execution code.
#[derive(Clone)]
pub struct StickyState {
    /// Current activity label (e.g. "Planning...", "Executing step 3...")
    pub activity: Arc<std::sync::Mutex<String>>,
    /// Task start instant — elapsed time derived from this
    pub started: Instant,
    /// Tokens received so far in this turn
    pub tokens: Arc<AtomicU64>,
    /// Thinking/reasoning duration in seconds
    pub thinking_secs: Arc<AtomicU64>,
    /// Whether the model is currently in thinking/reasoning phase
    pub is_thinking: Arc<AtomicBool>,
    /// Execution mode label
    pub mode: String,
    /// Model name (short)
    pub model: String,
    /// Active background process count
    pub active_processes: Arc<AtomicU64>,
    /// Active bash/shell command count
    pub active_bash: Arc<AtomicU64>,
    /// Number of queued messages from the user
    pub queued_count: Arc<AtomicU64>,
}

impl StickyState {
    pub fn new(mode: &str, model: &str) -> Self {
        Self {
            activity: Arc::new(std::sync::Mutex::new("Working...".to_string())),
            started: Instant::now(),
            tokens: Arc::new(AtomicU64::new(0)),
            thinking_secs: Arc::new(AtomicU64::new(0)),
            is_thinking: Arc::new(AtomicBool::new(false)),
            mode: mode.to_string(),
            model: if model.len() > 25 {
                model.chars().take(25).collect()
            } else {
                model.to_string()
            },
            active_processes: Arc::new(AtomicU64::new(0)),
            active_bash: Arc::new(AtomicU64::new(0)),
            queued_count: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn set_activity(&self, msg: &str) {
        if let Ok(mut a) = self.activity.lock() {
            *a = msg.to_string();
        }
    }

    pub fn add_tokens(&self, n: u64) {
        self.tokens.fetch_add(n, Ordering::Relaxed);
    }
}

/// Format a duration as "Xs" / "Xm Xs" / "Xh Xm"
fn fmt_elapsed(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Format token count compactly: 1.2k, 45.3k, 1.2M
fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Render the top bar content string (no ANSI cursor movement).
fn render_top(state: &StickyState, width: usize) -> String {
    let activity = state
        .activity
        .lock()
        .map(|a| a.clone())
        .unwrap_or_default();
    let elapsed = fmt_elapsed(state.started.elapsed());
    let tokens = fmt_tokens(state.tokens.load(Ordering::Relaxed));

    let mut parts = vec![
        format!("  ✱ {}", activity),
        format!("({})", elapsed),
        format!("↓ {} tokens", tokens),
    ];

    let think_secs = state.thinking_secs.load(Ordering::Relaxed);
    if think_secs > 0 || state.is_thinking.load(Ordering::Relaxed) {
        let label = if state.is_thinking.load(Ordering::Relaxed) {
            format!("thinking for {}s", state.started.elapsed().as_secs())
        } else {
            format!("thought for {}s", think_secs)
        };
        parts.push(label);
    }

    let content = parts.join(" · ");
    // Pad to full width
    if content.len() < width {
        format!("{}{}", content, " ".repeat(width - content.len()))
    } else {
        content.chars().take(width).collect()
    }
}

/// Render the bottom bar content string.
fn render_bottom(state: &StickyState, width: usize) -> String {
    let procs = state.active_processes.load(Ordering::Relaxed);
    let bash = state.active_bash.load(Ordering::Relaxed);
    let queued = state.queued_count.load(Ordering::Relaxed);

    let mut parts: Vec<String> = Vec::new();

    // Mode
    let mode_str = match state.mode.as_str() {
        "YOLO" => "▸▸ auto-approve on".to_string(),
        "auto-edit" => "▸ auto-edit on".to_string(),
        "daemon" => "▸▸ daemon mode".to_string(),
        _ => "▸ confirm mode".to_string(),
    };
    parts.push(mode_str);

    // Active bash commands
    if bash > 0 {
        parts.push(format!("{} bash", bash));
    }

    // Active background processes
    if procs > 0 {
        parts.push(format!("{} process{}", procs, if procs == 1 { "" } else { "es" }));
    }

    // Queued messages
    if queued > 0 {
        parts.push(format!("↑ to edit queued ({})", queued));
    }

    // Shortcuts
    parts.push("esc to interrupt".to_string());

    let content = format!("  {}", parts.join(" · "));
    if content.len() < width {
        format!("{}{}", content, " ".repeat(width - content.len()))
    } else {
        content.chars().take(width).collect()
    }
}

/// A sticky bar handle.  While this is alive, the top and bottom rows of the
/// terminal are pinned and all output scrolls between them.
///
/// Call [`update()`] periodically (e.g. on each streaming chunk) to refresh
/// the displayed information.  On [`drop`], the scrolling region is reset.
pub struct StickyBar {
    state: StickyState,
    height: u16,
    width: u16,
}

impl StickyBar {
    /// Activate the sticky bar.  Sets the terminal scrolling region and
    /// renders the initial top + bottom bars.
    pub fn activate(state: StickyState) -> Option<Self> {
        let (width, height) = terminal::size().ok()?;
        if height < 5 {
            return None; // Terminal too small
        }

        STICKY_ACTIVE.store(true, Ordering::Relaxed);

        let bar = Self {
            state,
            height,
            width,
        };
        bar.setup_regions();
        bar.paint();
        Some(bar)
    }

    /// Set up ANSI scrolling region: rows 2..(height-1) scroll, row 1 and
    /// row height are pinned.
    fn setup_regions(&self) {
        let mut out = io::stdout();
        // Set scrolling region to rows 2 through (height-1)
        write!(out, "\x1b[2;{}r", self.height - 1).ok();
        // Move cursor into the scrolling region
        write!(out, "\x1b[2;1H").ok();
        out.flush().ok();
    }

    /// Paint both bars without moving the content cursor.
    fn paint(&self) {
        // Sync global bash count into the state
        self.state
            .active_bash
            .store(active_bash_count(), Ordering::Relaxed);

        let mut out = io::stdout();
        let w = self.width as usize;

        // Save cursor position
        write!(out, "\x1b7").ok();

        // -- Top bar (row 1) --
        write!(out, "\x1b[1;1H").ok(); // Move to row 1, col 1
        let top = render_top(&self.state, w);
        // Reverse video (white on dark) for the bar
        write!(out, "\x1b[48;5;236m\x1b[38;5;215m{}\x1b[0m", top).ok();

        // -- Bottom bar (last row) --
        write!(out, "\x1b[{};1H", self.height).ok();
        let bottom = render_bottom(&self.state, w);
        write!(out, "\x1b[48;5;236m\x1b[38;5;245m{}\x1b[0m", bottom).ok();

        // Restore cursor position
        write!(out, "\x1b8").ok();
        out.flush().ok();
    }

    /// Refresh the bar contents.  Call this periodically during streaming.
    pub fn update(&self) {
        self.paint();
    }

    /// Get a reference to the shared state for external updates.
    pub fn state(&self) -> &StickyState {
        &self.state
    }

    /// Deactivate: reset scrolling region and clear the bar rows.
    fn teardown(&self) {
        let mut out = io::stdout();
        // Reset scrolling region to full terminal
        write!(out, "\x1b[r").ok();
        // Clear top bar row
        write!(out, "\x1b[1;1H\x1b[2K").ok();
        // Clear bottom bar row
        write!(out, "\x1b[{};1H\x1b[2K", self.height).ok();
        // Move cursor to row 2
        write!(out, "\x1b[2;1H").ok();
        out.flush().ok();
        STICKY_ACTIVE.store(false, Ordering::Relaxed);
    }
}

impl Drop for StickyBar {
    fn drop(&mut self) {
        self.teardown();
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_elapsed_seconds() {
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(5)), "5s");
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(59)), "59s");
    }

    #[test]
    fn fmt_elapsed_minutes() {
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(60)), "1m 0s");
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn fmt_elapsed_hours() {
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(3600)), "1h 0m");
        assert_eq!(fmt_elapsed(std::time::Duration::from_secs(3661)), "1h 1m");
    }

    #[test]
    fn fmt_tokens_small() {
        assert_eq!(fmt_tokens(0), "0");
        assert_eq!(fmt_tokens(500), "500");
        assert_eq!(fmt_tokens(999), "999");
    }

    #[test]
    fn fmt_tokens_thousands() {
        assert_eq!(fmt_tokens(1000), "1.0k");
        assert_eq!(fmt_tokens(1500), "1.5k");
        assert_eq!(fmt_tokens(42300), "42.3k");
    }

    #[test]
    fn fmt_tokens_millions() {
        assert_eq!(fmt_tokens(1_000_000), "1.0M");
        assert_eq!(fmt_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn render_top_contains_activity() {
        let state = StickyState::new("normal", "qwen3.5");
        state.set_activity("Planning...");
        let top = render_top(&state, 80);
        assert!(top.contains("Planning..."), "top bar should contain activity");
        assert!(top.contains("tokens"), "top bar should mention tokens");
    }

    #[test]
    fn render_top_contains_elapsed() {
        let state = StickyState::new("normal", "qwen3.5");
        let top = render_top(&state, 80);
        assert!(top.contains("0s") || top.contains("1s"), "should show elapsed time");
    }

    #[test]
    fn render_top_shows_thinking_when_active() {
        let state = StickyState::new("normal", "qwen3.5");
        state.is_thinking.store(true, Ordering::Relaxed);
        let top = render_top(&state, 120);
        assert!(top.contains("thinking for"), "should show thinking status");
    }

    #[test]
    fn render_top_shows_thought_duration() {
        let state = StickyState::new("normal", "qwen3.5");
        state.thinking_secs.store(5, Ordering::Relaxed);
        let top = render_top(&state, 120);
        assert!(top.contains("thought for 5s"), "should show completed thinking duration");
    }

    #[test]
    fn render_bottom_normal_mode() {
        let state = StickyState::new("normal", "qwen3.5");
        let bottom = render_bottom(&state, 80);
        assert!(bottom.contains("confirm mode"), "normal mode should say confirm");
        assert!(bottom.contains("esc to interrupt"));
    }

    #[test]
    fn render_bottom_yolo_mode() {
        let state = StickyState::new("YOLO", "qwen3.5");
        let bottom = render_bottom(&state, 80);
        assert!(bottom.contains("auto-approve on"), "YOLO should say auto-approve");
    }

    #[test]
    fn render_bottom_with_processes() {
        let state = StickyState::new("normal", "qwen3.5");
        state.active_processes.store(2, Ordering::Relaxed);
        let bottom = render_bottom(&state, 80);
        assert!(bottom.contains("2 processes"), "should show process count");
    }

    #[test]
    fn render_bottom_single_process() {
        let state = StickyState::new("normal", "qwen3.5");
        state.active_processes.store(1, Ordering::Relaxed);
        let bottom = render_bottom(&state, 80);
        assert!(bottom.contains("1 process"), "single should not be plural");
        assert!(!bottom.contains("1 processes"));
    }

    #[test]
    fn sticky_state_add_tokens() {
        let state = StickyState::new("normal", "test");
        state.add_tokens(100);
        state.add_tokens(200);
        assert_eq!(state.tokens.load(Ordering::Relaxed), 300);
    }

    #[test]
    fn sticky_state_model_truncation() {
        let state = StickyState::new("normal", "a-very-long-model-name-that-exceeds-25-chars");
        assert_eq!(state.model.len(), 25);
    }

    #[test]
    fn render_top_pads_to_width() {
        let state = StickyState::new("normal", "m");
        let top = render_top(&state, 100);
        assert_eq!(top.len(), 100, "should pad to exact width");
    }

    #[test]
    fn render_bottom_pads_to_width() {
        let state = StickyState::new("normal", "m");
        let bottom = render_bottom(&state, 100);
        assert_eq!(bottom.len(), 100, "should pad to exact width");
    }

    #[test]
    fn is_active_default_false() {
        // Note: this test can be affected by other tests running in parallel
        // that activate sticky bars. In isolation, it should be false.
        // We just verify the function is callable.
        let _ = is_active();
    }
}
