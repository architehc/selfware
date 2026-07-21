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

impl Default for BashGuard {
    fn default() -> Self {
        Self::new()
    }
}

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
pub fn fmt_elapsed(d: std::time::Duration) -> String {
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
pub fn fmt_tokens(n: u64) -> String {
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
    let activity = state.activity.lock().map(|a| a.clone()).unwrap_or_default();
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
        parts.push(format!(
            "{} process{}",
            procs,
            if procs == 1 { "" } else { "es" }
        ));
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
/// Call `update()` periodically (e.g. on each streaming chunk) to refresh
/// the displayed information.  On [`drop`], the scrolling region is reset.
pub struct StickyBar {
    state: StickyState,
    height: u16,
    width: u16,
}

impl StickyBar {
    /// Activate the status bar.  No scroll regions — just tracks state and
    /// renders inline via stderr on update().
    pub fn activate(state: StickyState) -> Option<Self> {
        let (width, height) = terminal::size().ok()?;
        if height < 3 {
            return None;
        }
        STICKY_ACTIVE.store(true, Ordering::Relaxed);
        Some(Self {
            state,
            height,
            width,
        })
    }

    /// Refresh: print the bottom status line on stderr (doesn't interfere
    /// with stdout streaming).  Uses `\r\x1b[K` to overwrite the current
    /// stderr line so it stays in place between calls.
    pub fn update(&self) {
        self.state
            .active_bash
            .store(active_bash_count(), Ordering::Relaxed);

        let w = terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(self.width as usize);
        let bottom = render_bottom(&self.state, w);
        let _top = render_top(&self.state, w);

        // Overwrite a single line on stderr with the combined status
        let mut err = io::stderr().lock();
        // Save cursor, move to column 0, clear line, print, restore cursor
        write!(
            err,
            "\x1b7\x1b[{};1H\x1b[48;5;236m\x1b[38;5;245m{}\x1b[0m\x1b8",
            self.height, bottom
        )
        .ok();
        err.flush().ok();
    }

    /// Print the final status summary inline (called once when generation ends).
    pub fn finish(&self) {
        self.state
            .active_bash
            .store(active_bash_count(), Ordering::Relaxed);

        let w = terminal::size()
            .map(|(w, _)| w as usize)
            .unwrap_or(self.width as usize);
        let top = render_top(&self.state, w);

        // Clear the bottom bar we've been overwriting
        let mut err = io::stderr().lock();
        write!(err, "\x1b[{};1H\x1b[2K\x1b[A", self.height).ok();
        err.flush().ok();

        // Print final summary inline on stdout
        let mut out = io::stdout();
        write!(out, "\x1b[90m{}\x1b[0m", top.trim()).ok();
        out.flush().ok();
    }

    pub fn state(&self) -> &StickyState {
        &self.state
    }

    fn teardown(&self) {
        // Clear the bottom bar line
        let mut err = io::stderr().lock();
        write!(err, "\x1b[{};1H\x1b[2K", self.height).ok();
        err.flush().ok();
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
#[path = "../../tests/unit/ui/sticky_bar/sticky_bar_test.rs"]
mod tests;
