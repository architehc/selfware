//! Terminal input handler with ESC interrupt, fixed input line, and work queuing.
//!
//! Provides a Claude Code-like terminal UI experience where the user can type
//! input at any time (even while the agent is working), interrupt execution
//! with ESC, and queue work items with priority and delay support.

#![allow(dead_code, unused_imports, unused_variables)]

use std::collections::VecDeque;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{self, Stylize};
use crossterm::terminal;
use tokio::sync::{Mutex, Notify};

// ---------------------------------------------------------------------------
// WorkPriority / QueuedWork / WorkQueue
// ---------------------------------------------------------------------------

/// Priority level for queued work items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkPriority {
    /// Standard priority (default).
    Normal,
    /// High priority — prefixed with `!` by the user.
    High,
    /// Delayed execution — prefixed with `@Xm` or `@Xs`.
    Delayed,
}

/// A single work item waiting in the queue.
#[derive(Debug, Clone)]
pub struct QueuedWork {
    /// The user input / command text.
    pub input: String,
    /// When this item was added to the queue.
    pub queued_at: Instant,
    /// Priority of this work item.
    pub priority: WorkPriority,
    /// For delayed items, when they become eligible for execution.
    pub execute_after: Option<Instant>,
}

/// Thread-safe work queue that accepts items from the input handler and
/// dispenses them to the agent loop.
#[derive(Clone)]
pub struct WorkQueue {
    queue: Arc<Mutex<VecDeque<QueuedWork>>>,
    notify: Arc<Notify>,
}

impl WorkQueue {
    /// Create a new empty work queue.
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Push a work item with normal priority.
    pub async fn push(&self, input: String) {
        let mut q = self.queue.lock().await;
        q.push_back(QueuedWork {
            input,
            queued_at: Instant::now(),
            priority: WorkPriority::Normal,
            execute_after: None,
        });
        self.notify.notify_one();
    }

    /// Push a work item with a specific priority and optional delay.
    pub async fn push_with_priority(
        &self,
        input: String,
        priority: WorkPriority,
        execute_after: Option<Instant>,
    ) {
        let mut q = self.queue.lock().await;
        let item = QueuedWork {
            input,
            queued_at: Instant::now(),
            priority,
            execute_after,
        };
        // High-priority items go to the front.
        if priority == WorkPriority::High {
            q.push_front(item);
        } else {
            q.push_back(item);
        }
        self.notify.notify_one();
    }

    /// Try to get the next ready item (non-blocking).
    /// Delayed items whose `execute_after` has not elapsed are skipped.
    pub async fn try_pop(&self) -> Option<QueuedWork> {
        let mut q = self.queue.lock().await;
        let now = Instant::now();
        // Find the first item that is ready.
        let pos = q.iter().position(|item| {
            item.execute_after.is_none_or(|after| now >= after)
        });
        pos.and_then(|i| q.remove(i))
    }

    /// Block until an item is available and ready for execution.
    pub async fn pop(&self) -> QueuedWork {
        loop {
            if let Some(item) = self.try_pop().await {
                return item;
            }
            // Wait for a notification or poll every 250ms for delayed items.
            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(250)) => {}
            }
        }
    }

    /// Peek at the next ready item without removing it.
    pub async fn peek(&self) -> Option<QueuedWork> {
        let q = self.queue.lock().await;
        let now = Instant::now();
        q.iter()
            .find(|item| item.execute_after.is_none_or(|after| now >= after))
            .cloned()
    }

    /// Return the total number of items (including not-yet-ready delayed items).
    pub async fn len(&self) -> usize {
        self.queue.lock().await.len()
    }

    /// Whether the queue is empty.
    pub async fn is_empty(&self) -> bool {
        self.queue.lock().await.is_empty()
    }

    /// Clear all items from the queue. Returns the count removed.
    pub async fn clear(&self) -> usize {
        let mut q = self.queue.lock().await;
        let n = q.len();
        q.clear();
        n
    }

    /// Return a snapshot of all items for display purposes.
    pub async fn snapshot(&self) -> Vec<QueuedWork> {
        self.queue.lock().await.iter().cloned().collect()
    }
}

impl Default for WorkQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// InputHistory
// ---------------------------------------------------------------------------

/// Stores and navigates through input history, with optional disk persistence.
struct InputHistory {
    entries: Vec<String>,
    position: Option<usize>,
    max_entries: usize,
    file_path: Option<PathBuf>,
}

impl InputHistory {
    /// Create a new history that persists to `~/.selfware/history`.
    fn new() -> Self {
        let file_path = dirs::home_dir().map(|h| h.join(".selfware").join("history"));
        let mut history = Self {
            entries: Vec::new(),
            position: None,
            max_entries: 100,
            file_path,
        };
        history.load_from_disk();
        history
    }

    /// Add an entry to history (deduplicating consecutive duplicates).
    fn add(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        // Don't add if identical to last entry.
        if self.entries.last().map(|s| s.as_str()) == Some(trimmed) {
            return;
        }
        self.entries.push(trimmed.to_string());
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.position = None;
        self.save_to_disk();
    }

    /// Navigate up (older) in history. Returns the entry or None.
    fn up(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let new_pos = match self.position {
            None => self.entries.len().saturating_sub(1),
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.position = Some(new_pos);
        self.entries.get(new_pos).map(|s| s.as_str())
    }

    /// Navigate down (newer) in history. Returns the entry or None
    /// (when past the end, resets position so user gets an empty line).
    fn down(&mut self) -> Option<&str> {
        match self.position {
            None => None,
            Some(p) => {
                if p + 1 >= self.entries.len() {
                    self.position = None;
                    None
                } else {
                    let new_pos = p + 1;
                    self.position = Some(new_pos);
                    self.entries.get(new_pos).map(|s| s.as_str())
                }
            }
        }
    }

    /// Load history from disk.
    fn load_from_disk(&mut self) {
        let Some(ref path) = self.file_path else {
            return;
        };
        if let Ok(content) = std::fs::read_to_string(path) {
            self.entries = content
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            // Trim to max.
            if self.entries.len() > self.max_entries {
                let skip = self.entries.len() - self.max_entries;
                self.entries.drain(..skip);
            }
        }
    }

    /// Persist history to disk.
    fn save_to_disk(&self) {
        let Some(ref path) = self.file_path else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let content = self.entries.join("\n");
        let _ = std::fs::write(path, content);
    }
}

// ---------------------------------------------------------------------------
// Delay parsing helpers
// ---------------------------------------------------------------------------

/// Parse a delay prefix like `@5m`, `@30s`, `@2h` from the beginning of input.
/// Returns `(delay_duration, remaining_input)` or `None` if no prefix.
fn parse_delay_prefix(input: &str) -> Option<(Duration, String)> {
    let input = input.trim();
    if !input.starts_with('@') {
        return None;
    }
    // Find the end of the numeric + suffix part.
    let rest = &input[1..];
    let mut num_end = 0;
    for ch in rest.chars() {
        if ch.is_ascii_digit() {
            num_end += ch.len_utf8();
        } else {
            break;
        }
    }
    if num_end == 0 {
        return None;
    }
    let number: u64 = rest[..num_end].parse().ok()?;
    let suffix_start = num_end;
    let suffix_char = rest[suffix_start..].chars().next()?;
    let multiplier = match suffix_char {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        _ => return None,
    };
    let unit_len = suffix_char.len_utf8();
    let remaining = rest[suffix_start + unit_len..].trim();
    if remaining.is_empty() {
        return None; // No command after the delay.
    }
    let duration = Duration::from_secs(number * multiplier);
    Some((duration, remaining.to_string()))
}

/// Parse a high-priority prefix (`!` at the start).
/// Returns `(is_high, cleaned_input)`.
fn parse_priority_prefix(input: &str) -> (bool, String) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix('!') {
        let rest = rest.trim();
        if rest.is_empty() {
            (false, trimmed.to_string())
        } else {
            (true, rest.to_string())
        }
    } else {
        (false, trimmed.to_string())
    }
}

// ---------------------------------------------------------------------------
// InputHandler
// ---------------------------------------------------------------------------

/// Terminal input handler that runs a background key-reading loop, queues
/// user input, and supports ESC interrupt of the running agent.
pub struct InputHandler {
    /// Shared cancellation flag (the same `Arc<AtomicBool>` used by the Agent).
    cancelled: Arc<AtomicBool>,
    /// Whether an interrupt has been flagged since the last `clear_interrupt`.
    interrupted: Arc<AtomicBool>,
    /// The work queue where parsed inputs are deposited.
    work_queue: WorkQueue,
    /// Handle to the background input loop task.
    input_loop_handle: Option<tokio::task::JoinHandle<()>>,
    /// Flag to signal the input loop to shut down.
    shutdown: Arc<AtomicBool>,
}

impl InputHandler {
    /// Create a new `InputHandler` wired to the agent's cancellation token.
    pub fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            interrupted: Arc::new(AtomicBool::new(false)),
            work_queue: WorkQueue::new(),
            input_loop_handle: None,
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a handle to the work queue (for external inspection / management).
    pub fn work_queue(&self) -> &WorkQueue {
        &self.work_queue
    }

    /// Spawn the background input loop that reads terminal keys.
    ///
    /// This enters raw mode on a blocking thread, polls for events, and
    /// deposits completed lines into the work queue. The loop exits when
    /// `shutdown` is set or Ctrl+D is pressed.
    pub fn spawn_input_loop(&mut self, execution_mode: crate::config::ExecutionMode) {
        let cancelled = Arc::clone(&self.cancelled);
        let interrupted = Arc::clone(&self.interrupted);
        let shutdown = Arc::clone(&self.shutdown);
        let work_queue = self.work_queue.clone();

        let handle = tokio::task::spawn_blocking(move || {
            let mut handler = RawInputLoop {
                cancelled,
                interrupted,
                shutdown,
                work_queue,
                buffer: String::new(),
                cursor_pos: 0,
                history: InputHistory::new(),
                execution_mode,
                agent_busy: Arc::new(AtomicBool::new(false)),
            };
            handler.run();
        });

        self.input_loop_handle = Some(handle);
    }

    /// Get the next queued input (non-blocking).
    pub async fn next_input(&self) -> Option<String> {
        self.work_queue.try_pop().await.map(|w| w.input)
    }

    /// Block until the user provides input.
    pub async fn wait_for_input(&self) -> String {
        self.work_queue.pop().await.input
    }

    /// Check if ESC was pressed (interrupt flag is set).
    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
    }

    /// Reset the interrupt flag after it has been handled.
    pub fn clear_interrupt(&self) {
        self.interrupted.store(false, Ordering::Relaxed);
    }

    /// Signal the input loop to shut down and wait for cleanup.
    pub async fn shutdown(mut self) {
        self.shutdown.store(true, Ordering::Release);
        if let Some(handle) = self.input_loop_handle.take() {
            let _ = tokio::time::timeout(Duration::from_millis(500), handle).await;
        }
    }
}

// ---------------------------------------------------------------------------
// RawInputLoop — runs on a blocking thread
// ---------------------------------------------------------------------------

/// The actual key-reading loop that runs in `spawn_blocking`.
struct RawInputLoop {
    cancelled: Arc<AtomicBool>,
    interrupted: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    work_queue: WorkQueue,
    buffer: String,
    cursor_pos: usize,
    history: InputHistory,
    execution_mode: crate::config::ExecutionMode,
    agent_busy: Arc<AtomicBool>,
}

impl RawInputLoop {
    fn run(&mut self) {
        // Enter raw mode.
        if terminal::enable_raw_mode().is_err() {
            return;
        }

        self.render_prompt();

        loop {
            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }

            // Poll with a short timeout so we can check the shutdown flag.
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => {
                    if let Ok(evt) = event::read() {
                        if self.handle_event(evt) {
                            break; // Ctrl+D or shutdown.
                        }
                    }
                }
                Ok(false) => {
                    // No event — loop back.
                }
                Err(_) => break,
            }
        }

        let _ = terminal::disable_raw_mode();
    }

    /// Handle a single terminal event. Returns `true` if the loop should exit.
    fn handle_event(&mut self, evt: Event) -> bool {
        match evt {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                // Multi-line paste support: append everything to the buffer.
                self.buffer.insert_str(self.cursor_pos, &text);
                self.cursor_pos += text.len();
                self.render_prompt();
                false
            }
            _ => false,
        }
    }

    /// Handle a single key event. Returns `true` if the loop should exit.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match (key.code, key.modifiers) {
            // --- ESC: interrupt current execution ---
            (KeyCode::Esc, _) => {
                self.cancelled.store(true, Ordering::Relaxed);
                self.interrupted.store(true, Ordering::Relaxed);
                self.write_raw(b"\r\n\x1b[33m[ESC] Cancelling...\x1b[0m\r\n");
                self.render_prompt();
                false
            }

            // --- Ctrl+C: same as ESC ---
            (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.cancelled.store(true, Ordering::Relaxed);
                self.interrupted.store(true, Ordering::Relaxed);
                self.write_raw(b"\r\n\x1b[33m[Ctrl+C] Cancelling...\x1b[0m\r\n");
                self.render_prompt();
                false
            }

            // --- Ctrl+D: exit ---
            (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.write_raw(b"\r\n");
                true
            }

            // --- Enter: submit current input ---
            (KeyCode::Enter, _) => {
                self.submit_input();
                false
            }

            // --- Backspace: delete char before cursor ---
            (KeyCode::Backspace, _) => {
                if self.cursor_pos > 0 {
                    // Find the byte index of the previous character boundary.
                    let prev = self.buffer[..self.cursor_pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.buffer.remove(prev);
                    self.cursor_pos = prev;
                    self.render_prompt();
                }
                false
            }

            // --- Delete: delete char at cursor ---
            (KeyCode::Delete, _) => {
                if self.cursor_pos < self.buffer.len() {
                    self.buffer.remove(self.cursor_pos);
                    self.render_prompt();
                }
                false
            }

            // --- Left arrow: move cursor left ---
            (KeyCode::Left, _) => {
                if self.cursor_pos > 0 {
                    let prev = self.buffer[..self.cursor_pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.cursor_pos = prev;
                    self.render_prompt();
                }
                false
            }

            // --- Right arrow: move cursor right ---
            (KeyCode::Right, _) => {
                if self.cursor_pos < self.buffer.len() {
                    let next = self.buffer[self.cursor_pos..]
                        .char_indices()
                        .nth(1)
                        .map(|(i, _)| self.cursor_pos + i)
                        .unwrap_or(self.buffer.len());
                    self.cursor_pos = next;
                    self.render_prompt();
                }
                false
            }

            // --- Up arrow: history up ---
            (KeyCode::Up, _) => {
                if let Some(entry) = self.history.up(&self.buffer) {
                    self.buffer = entry.to_string();
                    self.cursor_pos = self.buffer.len();
                    self.render_prompt();
                }
                false
            }

            // --- Down arrow: history down ---
            (KeyCode::Down, _) => {
                match self.history.down() {
                    Some(entry) => {
                        self.buffer = entry.to_string();
                        self.cursor_pos = self.buffer.len();
                    }
                    None => {
                        self.buffer.clear();
                        self.cursor_pos = 0;
                    }
                }
                self.render_prompt();
                false
            }

            // --- Home: move cursor to beginning ---
            (KeyCode::Home, _) => {
                self.cursor_pos = 0;
                self.render_prompt();
                false
            }

            // --- End: move cursor to end ---
            (KeyCode::End, _) => {
                self.cursor_pos = self.buffer.len();
                self.render_prompt();
                false
            }

            // --- Ctrl+U: clear line ---
            (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.buffer.clear();
                self.cursor_pos = 0;
                self.render_prompt();
                false
            }

            // --- Ctrl+W: delete word backwards ---
            (KeyCode::Char('w'), m) if m.contains(KeyModifiers::CONTROL) => {
                if self.cursor_pos > 0 {
                    let before = &self.buffer[..self.cursor_pos];
                    let trimmed = before.trim_end();
                    let new_end = trimmed
                        .rfind(|c: char| c.is_whitespace())
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    self.buffer.drain(new_end..self.cursor_pos);
                    self.cursor_pos = new_end;
                    self.render_prompt();
                }
                false
            }

            // --- Ctrl+A: move to beginning of line ---
            (KeyCode::Char('a'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.cursor_pos = 0;
                self.render_prompt();
                false
            }

            // --- Ctrl+E: move to end of line ---
            (KeyCode::Char('e'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.cursor_pos = self.buffer.len();
                self.render_prompt();
                false
            }

            // --- Ctrl+K: kill to end of line ---
            (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.buffer.truncate(self.cursor_pos);
                self.render_prompt();
                false
            }

            // --- Regular character input ---
            (KeyCode::Char(ch), m) if !m.contains(KeyModifiers::CONTROL) => {
                self.buffer.insert(self.cursor_pos, ch);
                self.cursor_pos += ch.len_utf8();
                self.render_prompt();
                false
            }

            // --- Tab: could be used for completion later ---
            (KeyCode::Tab, _) => {
                // No-op for now; future completion support.
                false
            }

            _ => false,
        }
    }

    /// Submit the current buffer as a work item.
    fn submit_input(&mut self) {
        let input = self.buffer.trim().to_string();
        self.write_raw(b"\r\n");

        if input.is_empty() {
            self.buffer.clear();
            self.cursor_pos = 0;
            self.render_prompt();
            return;
        }

        // Add to history.
        self.history.add(&input);

        // Check for queue management commands handled locally.
        if self.handle_queue_command(&input) {
            self.buffer.clear();
            self.cursor_pos = 0;
            self.render_prompt();
            return;
        }

        // Parse delay prefix: @5m do something
        if let Some((delay, remaining)) = parse_delay_prefix(&input) {
            let execute_after = Instant::now() + delay;
            let wq = self.work_queue.clone();
            let secs = delay.as_secs();
            let label = if secs >= 3600 {
                format!("{}h", secs / 3600)
            } else if secs >= 60 {
                format!("{}m", secs / 60)
            } else {
                format!("{}s", secs)
            };
            self.write_raw(
                format!(
                    "\x1b[36m[Delayed @{}]\x1b[0m {}\r\n",
                    label, remaining
                )
                .as_bytes(),
            );
            // Spawn a background tokio task to add the item after the delay.
            let remaining_clone = remaining.clone();
            tokio::task::block_in_place(move || {
                tokio::runtime::Handle::current().block_on(async {
                    wq.push_with_priority(
                        remaining_clone,
                        WorkPriority::Delayed,
                        Some(execute_after),
                    )
                    .await;
                });
            });
            self.buffer.clear();
            self.cursor_pos = 0;
            self.render_prompt();
            return;
        }

        // Parse high-priority prefix: !do something
        let (is_high, clean_input) = parse_priority_prefix(&input);
        let priority = if is_high {
            WorkPriority::High
        } else {
            WorkPriority::Normal
        };

        // Push to queue.
        let wq = self.work_queue.clone();
        let ci = clean_input.clone();
        tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async {
                wq.push_with_priority(ci, priority, None).await;
            });
        });

        self.buffer.clear();
        self.cursor_pos = 0;
        self.render_prompt();
    }

    /// Handle `/queue`, `/q` commands locally. Returns true if handled.
    fn handle_queue_command(&mut self, input: &str) -> bool {
        let input = input.trim();

        if input == "/queue" || input == "/q" {
            // Show all queued items.
            let items = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { self.work_queue.snapshot().await })
            });
            if items.is_empty() {
                self.write_raw(b"\x1b[36m[Queue] Empty\x1b[0m\r\n");
            } else {
                let header = format!("\x1b[36m[Queue] {} item(s):\x1b[0m\r\n", items.len());
                self.write_raw(header.as_bytes());
                for (i, item) in items.iter().enumerate() {
                    let priority_tag = match item.priority {
                        WorkPriority::High => " \x1b[31m[HIGH]\x1b[0m",
                        WorkPriority::Delayed => " \x1b[33m[DELAYED]\x1b[0m",
                        WorkPriority::Normal => "",
                    };
                    let delay_info = if let Some(after) = item.execute_after {
                        let remaining = after.saturating_duration_since(Instant::now());
                        if remaining.as_secs() > 0 {
                            format!(" (in {}s)", remaining.as_secs())
                        } else {
                            " (ready)".to_string()
                        }
                    } else {
                        String::new()
                    };
                    let preview = if item.input.len() > 60 {
                        format!("{}...", &item.input[..57])
                    } else {
                        item.input.clone()
                    };
                    let line = format!(
                        "  {}. {}{}{}\r\n",
                        i + 1,
                        preview,
                        priority_tag,
                        delay_info,
                    );
                    self.write_raw(line.as_bytes());
                }
            }
            return true;
        }

        if input == "/queue clear" || input == "/q clear" {
            let count = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { self.work_queue.clear().await })
            });
            let msg = format!("\x1b[36m[Queue] Cleared {} item(s)\x1b[0m\r\n", count);
            self.write_raw(msg.as_bytes());
            return true;
        }

        if input == "/queue next" || input == "/q next" {
            let next = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(async { self.work_queue.peek().await })
            });
            match next {
                Some(item) => {
                    let msg = format!("\x1b[36m[Queue] Next:\x1b[0m {}\r\n", item.input);
                    self.write_raw(msg.as_bytes());
                }
                None => {
                    self.write_raw(b"\x1b[36m[Queue] Empty\x1b[0m\r\n");
                }
            }
            return true;
        }

        false
    }

    /// Render the prompt line at the current cursor position.
    fn render_prompt(&self) {
        let mut stderr = io::stderr();

        // Build the prompt prefix.
        let queue_len = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { self.work_queue.len().await })
        });

        let mode_indicator = match self.execution_mode {
            crate::config::ExecutionMode::Normal => "[N]",
            crate::config::ExecutionMode::AutoEdit => "[A]",
            crate::config::ExecutionMode::Yolo => "[Y]",
            crate::config::ExecutionMode::Daemon => "[D]",
        };

        let queue_indicator = if queue_len > 0 {
            format!(" [{} queued]", queue_len)
        } else {
            String::new()
        };

        let prompt = format!(
            "\x1b[2K\r\x1b[36mselfware\x1b[0m \x1b[33m{}\x1b[0m\x1b[90m{}\x1b[0m\x1b[36m>\x1b[0m ",
            mode_indicator, queue_indicator
        );

        // Write prompt + buffer, then position cursor.
        let _ = stderr.write_all(prompt.as_bytes());
        let _ = stderr.write_all(self.buffer.as_bytes());

        // Move cursor to the correct position if not at end.
        let chars_after_cursor = self.buffer[self.cursor_pos..].chars().count();
        if chars_after_cursor > 0 {
            let _ = write!(stderr, "\x1b[{}D", chars_after_cursor);
        }

        let _ = stderr.flush();
    }

    /// Write raw bytes to stderr (used in raw mode where stdout may be
    /// redirected).
    fn write_raw(&self, bytes: &[u8]) {
        let _ = io::stderr().write_all(bytes);
        let _ = io::stderr().flush();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_delay_prefix_minutes() {
        let result = parse_delay_prefix("@5m run tests");
        assert!(result.is_some());
        let (dur, remaining) = result.unwrap();
        assert_eq!(dur, Duration::from_secs(300));
        assert_eq!(remaining, "run tests");
    }

    #[test]
    fn test_parse_delay_prefix_seconds() {
        let result = parse_delay_prefix("@30s check status");
        assert!(result.is_some());
        let (dur, remaining) = result.unwrap();
        assert_eq!(dur, Duration::from_secs(30));
        assert_eq!(remaining, "check status");
    }

    #[test]
    fn test_parse_delay_prefix_hours() {
        let result = parse_delay_prefix("@2h deploy");
        assert!(result.is_some());
        let (dur, remaining) = result.unwrap();
        assert_eq!(dur, Duration::from_secs(7200));
        assert_eq!(remaining, "deploy");
    }

    #[test]
    fn test_parse_delay_prefix_no_command() {
        let result = parse_delay_prefix("@5m");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_delay_prefix_no_prefix() {
        let result = parse_delay_prefix("run tests");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_delay_prefix_invalid_suffix() {
        let result = parse_delay_prefix("@5x run tests");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_priority_prefix_high() {
        let (is_high, text) = parse_priority_prefix("!fix this now");
        assert!(is_high);
        assert_eq!(text, "fix this now");
    }

    #[test]
    fn test_parse_priority_prefix_normal() {
        let (is_high, text) = parse_priority_prefix("do something");
        assert!(!is_high);
        assert_eq!(text, "do something");
    }

    #[test]
    fn test_parse_priority_prefix_bare_bang() {
        let (is_high, text) = parse_priority_prefix("!");
        assert!(!is_high);
        assert_eq!(text, "!");
    }

    #[tokio::test]
    async fn test_work_queue_push_pop() {
        let q = WorkQueue::new();
        q.push("task 1".to_string()).await;
        q.push("task 2".to_string()).await;
        assert_eq!(q.len().await, 2);

        let item = q.try_pop().await.unwrap();
        assert_eq!(item.input, "task 1");
        assert_eq!(q.len().await, 1);
    }

    #[tokio::test]
    async fn test_work_queue_high_priority_goes_front() {
        let q = WorkQueue::new();
        q.push("normal task".to_string()).await;
        q.push_with_priority("urgent task".to_string(), WorkPriority::High, None)
            .await;

        let item = q.try_pop().await.unwrap();
        assert_eq!(item.input, "urgent task");
        assert_eq!(item.priority, WorkPriority::High);
    }

    #[tokio::test]
    async fn test_work_queue_delayed_not_ready() {
        let q = WorkQueue::new();
        let future = Instant::now() + Duration::from_secs(3600);
        q.push_with_priority("future task".to_string(), WorkPriority::Delayed, Some(future))
            .await;

        // Should not be available yet.
        assert!(q.try_pop().await.is_none());
        assert_eq!(q.len().await, 1);
    }

    #[tokio::test]
    async fn test_work_queue_delayed_ready() {
        let q = WorkQueue::new();
        let past = Instant::now() - Duration::from_secs(1);
        q.push_with_priority("past task".to_string(), WorkPriority::Delayed, Some(past))
            .await;

        let item = q.try_pop().await.unwrap();
        assert_eq!(item.input, "past task");
    }

    #[tokio::test]
    async fn test_work_queue_clear() {
        let q = WorkQueue::new();
        q.push("a".to_string()).await;
        q.push("b".to_string()).await;
        q.push("c".to_string()).await;

        let cleared = q.clear().await;
        assert_eq!(cleared, 3);
        assert!(q.is_empty().await);
    }

    #[tokio::test]
    async fn test_work_queue_snapshot() {
        let q = WorkQueue::new();
        q.push("first".to_string()).await;
        q.push("second".to_string()).await;

        let snap = q.snapshot().await;
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].input, "first");
        assert_eq!(snap[1].input, "second");
    }

    #[tokio::test]
    async fn test_work_queue_peek() {
        let q = WorkQueue::new();
        q.push("peeked".to_string()).await;

        let item = q.peek().await.unwrap();
        assert_eq!(item.input, "peeked");
        // Peek should not remove the item.
        assert_eq!(q.len().await, 1);
    }

    #[test]
    fn test_input_history_add_and_navigate() {
        let mut history = InputHistory {
            entries: Vec::new(),
            position: None,
            max_entries: 5,
            file_path: None,
        };

        history.add("first");
        history.add("second");
        history.add("third");

        // Up should go to most recent.
        assert_eq!(history.up(""), Some("third"));
        assert_eq!(history.up(""), Some("second"));
        assert_eq!(history.up(""), Some("first"));
        // Should stay at first.
        assert_eq!(history.up(""), Some("first"));

        // Down should go forward.
        assert_eq!(history.down(), Some("second"));
        assert_eq!(history.down(), Some("third"));
        // Past end resets.
        assert_eq!(history.down(), None);
    }

    #[test]
    fn test_input_history_dedup() {
        let mut history = InputHistory {
            entries: Vec::new(),
            position: None,
            max_entries: 5,
            file_path: None,
        };

        history.add("same");
        history.add("same");
        history.add("same");

        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn test_input_history_max_entries() {
        let mut history = InputHistory {
            entries: Vec::new(),
            position: None,
            max_entries: 3,
            file_path: None,
        };

        history.add("one");
        history.add("two");
        history.add("three");
        history.add("four");

        assert_eq!(history.entries.len(), 3);
        assert_eq!(history.entries[0], "two");
    }

    #[test]
    fn test_input_handler_new() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let handler = InputHandler::new(Arc::clone(&cancelled));

        assert!(!handler.is_interrupted());

        // Setting interrupted externally.
        handler.interrupted.store(true, Ordering::Relaxed);
        assert!(handler.is_interrupted());

        handler.clear_interrupt();
        assert!(!handler.is_interrupted());
    }
}
