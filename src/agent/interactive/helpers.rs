use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::{PendingMessage, PendingMessageOrigin};

/// Check if input is an exit command (shared with the TUI and multi-chat
/// loops — see [`crate::input::command_registry::is_exit_command`]).
pub(crate) use crate::input::command_registry::is_exit_command;

/// True when REPL input looks like a slash command (`/word` optionally
/// followed by arguments) rather than plain chat or an absolute path.
/// `/mode yolo` and `/analyze` are commands; `/tmp/foo.rs` and `/` are not
/// (a second `/` before any whitespace means it's a path).
pub(crate) fn looks_like_slash_command(input: &str) -> bool {
    let Some(rest) = input.strip_prefix('/') else {
        return false;
    };
    let head = rest.split_whitespace().next().unwrap_or("");
    !head.is_empty()
        && !head.contains('/')
        && head.chars().any(|c| c.is_alphanumeric())
        && head
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Reject a slash-prefixed input that matched no built-in command and no
/// registered skill, instead of forwarding it to the LLM as a paid chat
/// message. Commands advertised by the registry but with no REPL handler
/// (`/mode yolo`, `/analyze`, `/garden`, `/journal`, `/palette`) otherwise
/// burn tokens doing nothing.
pub(crate) fn reject_unknown_slash_command(input: &str) {
    use colored::*;
    let head = input
        .strip_prefix('/')
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or(input);
    println!(
        "{} Unknown command '{}'. Type {} to list available commands.",
        "✗".bright_red(),
        format!("/{}", head).bright_white(),
        "/help".bright_cyan()
    );
}

/// Truncate a string at a char boundary, avoiding panics on multi-byte UTF-8.
pub(crate) fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub(crate) const QUEUE_NOTICE_PREVIEW_BYTES: usize = 120;
pub(crate) const QUEUE_LIST_PREVIEW_BYTES: usize = 120;
pub(crate) const QUEUE_DROP_PREVIEW_BYTES: usize = 80;
pub(crate) const QUEUE_DRAIN_PREVIEW_BYTES: usize = 120;
pub(crate) const TOOL_DEBUG_RESULT_PREVIEW_LINES: usize = 20;
pub(crate) const INTERACTIVE_QUEUE_COALESCE_WINDOW: Duration = Duration::from_millis(150);

pub(crate) fn strip_trailing_submission_newlines(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

pub(crate) fn is_effectively_empty_message(s: &str) -> bool {
    s.trim().is_empty()
}

pub(crate) fn flatten_preview_text(s: &str) -> String {
    let mut flattened = String::with_capacity(s.len());
    let mut last_was_space = false;

    for ch in s.chars() {
        let mapped = match ch {
            '\r' | '\n' | '\t' => ' ',
            _ => ch,
        };

        if mapped.is_whitespace() {
            if !last_was_space {
                flattened.push(' ');
                last_was_space = true;
            }
        } else {
            flattened.push(mapped);
            last_was_space = false;
        }
    }

    flattened.trim().to_string()
}

pub(crate) fn preview_with_ellipsis(s: &str, max_bytes: usize) -> String {
    let flattened = flatten_preview_text(s);
    let preview = safe_truncate(&flattened, max_bytes);
    if flattened.len() > preview.len() {
        format!("{}...", preview)
    } else {
        preview.to_string()
    }
}

pub(crate) fn print_debug_args_block(args: &str) {
    println!("     Args:");
    if args.is_empty() {
        println!("       <none>");
        return;
    }

    for line in args.lines() {
        println!("       {}", line);
    }
}

pub(crate) fn print_debug_result_block(result: Option<&str>, full: bool) {
    println!("     Result:");
    let Some(result) = result else {
        println!("       <none>");
        return;
    };

    let lines: Vec<&str> = result.lines().collect();
    let show = if full {
        lines.len()
    } else {
        lines.len().min(TOOL_DEBUG_RESULT_PREVIEW_LINES)
    };
    for line in &lines[..show] {
        println!("       {}", line);
    }
    if !full && lines.len() > show {
        println!("       ... ({} more lines)", lines.len() - show);
    }
}

pub(crate) fn render_inline_queue_prompt(input: &str) {
    let preview = preview_with_ellipsis(input, QUEUE_NOTICE_PREVIEW_BYTES);
    let prompt = format!("\r\x1b[2K\x1b[90m  ▸ \x1b[0m{}", preview);
    let _ = std::io::stderr().write_all(prompt.as_bytes());
    let _ = std::io::stderr().flush();
}

pub(crate) fn coalesce_pending_messages<I>(messages: I) -> Vec<PendingMessage>
where
    I: IntoIterator<Item = PendingMessage>,
{
    let mut coalesced: Vec<PendingMessage> = Vec::new();

    for msg in messages {
        if is_effectively_empty_message(&msg.content) {
            continue;
        }

        if let Some(current) = coalesced.last_mut() {
            let within_window = msg.queued_at.saturating_duration_since(current.queued_at)
                <= INTERACTIVE_QUEUE_COALESCE_WINDOW;
            if current.origin == PendingMessageOrigin::InteractiveQueue
                && msg.origin == PendingMessageOrigin::InteractiveQueue
                && within_window
            {
                if !current.content.is_empty() {
                    current.content.push('\n');
                }
                current.content.push_str(&msg.content);
                current.queued_at = msg.queued_at;
                continue;
            }
        }

        coalesced.push(msg);
    }

    coalesced
}

/// Shared queue for messages typed during generation.
pub(crate) type InputQueue = Arc<std::sync::Mutex<Vec<PendingMessage>>>;

/// Handle returned by [`spawn_input_listener`] so the caller can signal the
/// listener to stop and wait for terminal raw-mode to be restored.
pub(crate) struct EscListenerGuard {
    stop: Arc<AtomicBool>,
    handle: tokio::task::JoinHandle<()>,
    /// Messages queued by the user while the model was generating.
    pub queued: InputQueue,
}

impl EscListenerGuard {
    /// Signal the background listener to exit and wait (briefly) for it to
    /// restore terminal raw-mode before returning control to reedline.
    pub(crate) async fn stop(self) -> Vec<PendingMessage> {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::Relaxed);
        let _ = tokio::time::timeout(std::time::Duration::from_millis(200), self.handle).await;
        self.queued
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}

/// Cap on events drained per pause episode, so a pathological event source
/// (a held-down key, a runaway paste) cannot stop the listener from acking
/// the confirmation prompt's pause request.
pub(crate) const MAX_PAUSE_DRAIN_EVENTS: usize = 256;

/// What produced a task-cancel from the listener — used for the notice text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelNotice {
    Esc,
    CtrlC,
    /// An exit command (`/quit`, `quit`, ...) typed mid-task. Acts as a task
    /// cancel; exiting the session still happens at the REPL prompt.
    ExitCommand,
}

/// What the listener loop must do after processing one terminal event. Pure
/// decision — the loop performs the actual I/O, which keeps the state machine
/// deterministic and testable without a TTY.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ListenerAction {
    /// Unhandled event / nothing to do.
    None,
    /// Re-render the inline "▸" queue prompt for the current buffer.
    RenderPrompt,
    /// Erase the inline prompt line.
    ClearPrompt,
    /// A completed non-command line: enqueue it as a follow-up message.
    QueueMessage(String),
    /// Cancel the running task (ESC / Ctrl+C / exit command).
    Cancel(CancelNotice),
    /// A slash command typed mid-task: never enqueued as task prose; the user
    /// is told to re-enter it at the next prompt.
    RejectSlashCommand(String),
    /// Input captured while a confirmation prompt owns the input stream:
    /// counted and discarded so it can never be replayed as a queued message.
    Trapped,
}

/// Line-buffer state for the inline queue prompt, plus the prompt-handoff
/// discard counter. Pure: no I/O, no threads — tests drive it with scripted
/// events and assert on the returned actions.
#[derive(Default)]
pub(crate) struct ListenerInputState {
    input_buf: String,
    showing_prompt: bool,
    trapped_during_pause: u64,
}

impl ListenerInputState {
    /// Process one terminal event.
    ///
    /// `prompt_owns_input` MUST be the pause flag re-checked AFTER the event
    /// was read, not before the poll — that recheck is what stops an event
    /// that raced the confirmation prompt's pause request from being buffered
    /// here and replayed later as a fake queued user message. Events caught
    /// mid-handoff are discarded (counted) instead.
    pub(crate) fn on_event(
        &mut self,
        event: &crossterm::event::Event,
        prompt_owns_input: bool,
    ) -> ListenerAction {
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

        match event {
            Event::Key(KeyEvent {
                code, modifiers, ..
            }) => {
                // Explicit cancels are honoured even mid-handoff — never
                // swallow the user's abort.
                if *code == KeyCode::Esc {
                    return ListenerAction::Cancel(CancelNotice::Esc);
                }
                if *code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    return ListenerAction::Cancel(CancelNotice::CtrlC);
                }
                if (*code == KeyCode::Char('j') || *code == KeyCode::Char('m'))
                    && modifiers.contains(KeyModifiers::CONTROL)
                {
                    if prompt_owns_input {
                        self.trapped_during_pause += 1;
                        return ListenerAction::Trapped;
                    }
                    return self.on_enter();
                }
                if prompt_owns_input {
                    self.trapped_during_pause += 1;
                    return ListenerAction::Trapped;
                }
                if modifiers.contains(KeyModifiers::CONTROL)
                    || modifiers.contains(KeyModifiers::ALT)
                {
                    return ListenerAction::None;
                }
                match code {
                    KeyCode::Enter => self.on_enter(),
                    KeyCode::Backspace => {
                        if self.input_buf.pop().is_some() {
                            if self.input_buf.is_empty() {
                                self.showing_prompt = false;
                                ListenerAction::ClearPrompt
                            } else {
                                ListenerAction::RenderPrompt
                            }
                        } else {
                            ListenerAction::None
                        }
                    }
                    KeyCode::Char(c) => {
                        self.showing_prompt = true;
                        self.input_buf.push(*c);
                        ListenerAction::RenderPrompt
                    }
                    _ => ListenerAction::None,
                }
            }
            Event::Paste(text) => {
                if prompt_owns_input {
                    self.trapped_during_pause += 1;
                    return ListenerAction::Trapped;
                }
                self.showing_prompt = true;
                self.input_buf.push_str(text);
                ListenerAction::RenderPrompt
            }
            _ => ListenerAction::None,
        }
    }

    /// Handle the submission (Enter) of the current buffer. Exit commands
    /// cancel the running task instead of being enqueued; other slash
    /// commands are rejected outright — the queue carries follow-up TASK
    /// input only, and a `/quit`-style string reaching the model as prose is
    /// the failure this guards.
    fn on_enter(&mut self) -> ListenerAction {
        if self.input_buf.is_empty() {
            return ListenerAction::None;
        }
        let msg = strip_trailing_submission_newlines(&self.input_buf).to_string();
        if is_effectively_empty_message(&msg) {
            self.input_buf.clear();
            self.showing_prompt = false;
            return ListenerAction::ClearPrompt;
        }
        if is_exit_command(msg.trim()) {
            // Buffer dismissal happens in `apply`, which owns the visible line.
            return ListenerAction::Cancel(CancelNotice::ExitCommand);
        }
        self.input_buf.clear();
        self.showing_prompt = false;
        if looks_like_slash_command(msg.trim_start()) {
            ListenerAction::RejectSlashCommand(msg)
        } else {
            ListenerAction::QueueMessage(msg)
        }
    }

    pub(crate) fn buffer(&self) -> &str {
        &self.input_buf
    }

    pub(crate) fn is_showing_prompt(&self) -> bool {
        self.showing_prompt
    }

    /// Load a queued message back into the edit buffer (Up-arrow recall).
    pub(crate) fn load_buffer(&mut self, content: String) {
        self.input_buf = content;
        self.showing_prompt = true;
    }

    /// Clear the buffer and the prompt-display flag (after cancel).
    fn dismiss(&mut self) {
        self.input_buf.clear();
        self.showing_prompt = false;
    }

    /// Events discarded during prompt handoffs since the last report.
    /// Resets the counter.
    pub(crate) fn take_trapped_count(&mut self) -> u64 {
        std::mem::take(&mut self.trapped_during_pause)
    }
}

/// Terminal event source abstraction. Production reads crossterm; tests drive
/// the listener loop with a scripted source, so no TTY is involved.
pub(crate) trait InputEventSource {
    /// Wait up to `timeout` for an event; `Ok(true)` means one is ready.
    fn poll_ready(&mut self, timeout: Duration) -> std::io::Result<bool>;
    fn read_event(&mut self) -> std::io::Result<crossterm::event::Event>;
    /// Enter/leave raw mode around event reads (no-op for scripted sources).
    fn set_raw_mode(&mut self, enable: bool) -> std::io::Result<()>;
}

pub(crate) struct CrosstermEventSource;

impl InputEventSource for CrosstermEventSource {
    fn poll_ready(&mut self, timeout: Duration) -> std::io::Result<bool> {
        crossterm::event::poll(timeout)
    }

    fn read_event(&mut self) -> std::io::Result<crossterm::event::Event> {
        crossterm::event::read()
    }

    fn set_raw_mode(&mut self, enable: bool) -> std::io::Result<()> {
        if enable {
            crossterm::terminal::enable_raw_mode()
        } else {
            crossterm::terminal::disable_raw_mode()
        }
    }
}

/// The listener loop, generic over the event source. Owns the input stream
/// while the agent runs, EXCEPT during confirmation prompts: while `paused`
/// is set it drains what it already captured, acknowledges, and then stops
/// reading until unpaused — the prompt is the sole reader for that window
/// (see `execution::read_line_pausing_esc`).
pub(crate) struct EscListenerLoop<S: InputEventSource> {
    source: S,
    state: ListenerInputState,
    cancel_token: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    pause_ack: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    queued: InputQueue,
    /// Lifetime count of events discarded during prompt handoffs — returned
    /// from [`run`](Self::run) for tests and diagnostics.
    total_trapped: u64,
}

impl<S: InputEventSource> EscListenerLoop<S> {
    pub(crate) fn new(
        source: S,
        cancel_token: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        pause_ack: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        queued: InputQueue,
    ) -> Self {
        Self {
            source,
            state: ListenerInputState::default(),
            cancel_token,
            paused,
            pause_ack,
            stop,
            queued,
            total_trapped: 0,
        }
    }

    /// Run until stopped/cancelled or the event source fails. Returns the
    /// total number of input events discarded during prompt handoffs.
    pub(crate) fn run(&mut self) -> u64 {
        use std::sync::atomic::Ordering;

        loop {
            if self.stop.load(Ordering::Relaxed) || self.cancel_token.load(Ordering::Relaxed) {
                break;
            }

            if self.paused.load(Ordering::Relaxed) {
                // Acknowledge only AFTER draining everything already captured:
                // the prompt waits for the ack before reading, so this drain
                // is the last time the listener touches the input stream until
                // the prompt closes — the prompt then owns it atomically.
                if !self.pause_ack.load(Ordering::Acquire) {
                    let cancel = self.drain_trapped_events();
                    let trapped = self.state.take_trapped_count();
                    if trapped > 0 {
                        self.total_trapped += trapped;
                        tracing::warn!(
                            "discarded {trapped} input event(s) that raced the confirmation-prompt handoff"
                        );
                        let note = format!(
                            "\r\n\x1b[90m  [input] {trapped} keystroke(s) raced the prompt handoff and were discarded — please retype at the prompt.\x1b[0m\r\n"
                        );
                        let _ = std::io::stderr().write_all(note.as_bytes());
                        let _ = std::io::stderr().flush();
                    }
                    self.pause_ack.store(true, Ordering::Release);
                    if cancel {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            self.pause_ack.store(false, Ordering::Release);

            if self.source.set_raw_mode(true).is_err() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            let poll_result = self.source.poll_ready(Duration::from_millis(50));
            let event_result = match poll_result {
                Ok(true) => Some(self.source.read_event()),
                Ok(false) => None,
                Err(_) => {
                    let _ = self.source.set_raw_mode(false);
                    break;
                }
            };

            let _ = self.source.set_raw_mode(false);

            if let Some(read_result) = event_result {
                match read_result {
                    Ok(event) => {
                        // RE-CHECK the pause flag after the read: an event that
                        // raced the pause request belongs to the confirmation
                        // prompt, never to the queue buffer.
                        let prompt_owns_input = self.paused.load(Ordering::Relaxed);
                        let action = if is_up_arrow(&event) && !prompt_owns_input {
                            self.recall_queued_message()
                        } else {
                            self.state.on_event(&event, prompt_owns_input)
                        };
                        if self.apply(action) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }

        if self.state.is_showing_prompt() {
            let _ = std::io::stderr().write_all(b"\r\x1b[2K");
            let _ = std::io::stderr().flush();
        }

        // Flush any handoff discards that never reached a pause-episode report
        // (e.g. stop raced the pause), so the count is never silently lost.
        let leftover = self.state.take_trapped_count();
        if leftover > 0 {
            self.total_trapped += leftover;
            tracing::warn!(
                "discarded {leftover} input event(s) that raced the confirmation-prompt handoff"
            );
        }

        self.pause_ack.store(false, Ordering::Release);
        let _ = self.source.set_raw_mode(false);
        self.total_trapped
    }

    /// Drain every event the source already has buffered, routing each through
    /// the paused handler (discard + count). Returns true if a cancel was
    /// requested while draining.
    fn drain_trapped_events(&mut self) -> bool {
        for _ in 0..MAX_PAUSE_DRAIN_EVENTS {
            match self.source.poll_ready(Duration::ZERO) {
                Ok(true) => match self.source.read_event() {
                    Ok(event) => {
                        let action = self.state.on_event(&event, true);
                        if self.apply(action) {
                            return true;
                        }
                    }
                    Err(_) => return false,
                },
                _ => return false,
            }
        }
        false
    }

    /// Up-arrow recall: pop the most recent queued message back into the
    /// edit buffer.
    fn recall_queued_message(&mut self) -> ListenerAction {
        let popped = match self.queued.lock() {
            Ok(mut q) => q.pop(),
            Err(_) => None,
        };
        match popped {
            Some(msg) => {
                self.state.load_buffer(msg.content);
                ListenerAction::RenderPrompt
            }
            None => ListenerAction::None,
        }
    }

    /// Perform the side effects of a decided action. Returns true when the
    /// loop should exit (any cancel).
    fn apply(&mut self, action: ListenerAction) -> bool {
        use std::sync::atomic::Ordering;
        match action {
            ListenerAction::None | ListenerAction::Trapped => false,
            ListenerAction::RenderPrompt => {
                render_inline_queue_prompt(self.state.buffer());
                false
            }
            ListenerAction::ClearPrompt => {
                let _ = std::io::stderr().write_all(b"\r\x1b[2K");
                let _ = std::io::stderr().flush();
                false
            }
            ListenerAction::QueueMessage(msg) => {
                let count = match self.queued.lock() {
                    Ok(mut q) => {
                        q.push(PendingMessage::new(
                            msg.clone(),
                            PendingMessageOrigin::InteractiveQueue,
                            Instant::now(),
                        ));
                        q.len()
                    }
                    Err(_) => 0,
                };
                let notice = format!(
                    "\r\x1b[2K\x1b[36m  ▸ Queued: \x1b[0m{}\x1b[90m ({})\x1b[0m\r\n",
                    preview_with_ellipsis(&msg, QUEUE_NOTICE_PREVIEW_BYTES),
                    count,
                );
                let _ = std::io::stderr().write_all(notice.as_bytes());
                let _ = std::io::stderr().flush();
                false
            }
            ListenerAction::RejectSlashCommand(cmd) => {
                let note = format!(
                    "\r\x1b[2K\x1b[33m  Slash commands aren't queued while a task is running — re-enter at the next prompt: {}\x1b[0m\r\n",
                    preview_with_ellipsis(&cmd, QUEUE_NOTICE_PREVIEW_BYTES),
                );
                let _ = std::io::stderr().write_all(note.as_bytes());
                let _ = std::io::stderr().flush();
                false
            }
            ListenerAction::Cancel(notice) => {
                if self.state.is_showing_prompt() {
                    let _ = std::io::stderr().write_all(b"\r\x1b[2K");
                }
                self.state.dismiss();
                self.cancel_token.store(true, Ordering::Relaxed);
                let text: &[u8] = match notice {
                    CancelNotice::Esc => b"\r\n\x1b[33m[ESC] Cancelling...\x1b[0m\r\n",
                    CancelNotice::CtrlC => b"\r\n\x1b[33m[Ctrl+C] Cancelling...\x1b[0m\r\n",
                    CancelNotice::ExitCommand => {
                        b"\r\n\x1b[33m[exit command] Cancelling the current task - type /quit at the prompt to exit selfware.\x1b[0m\r\n"
                    }
                };
                let _ = std::io::stderr().write_all(text);
                let _ = std::io::stderr().flush();
                true
            }
        }
    }
}

fn is_up_arrow(event: &crossterm::event::Event) -> bool {
    matches!(
        event,
        crossterm::event::Event::Key(crossterm::event::KeyEvent {
            code: crossterm::event::KeyCode::Up,
            ..
        })
    )
}

/// Spawn a background input listener that runs during model execution.
pub(crate) fn spawn_esc_listener(
    cancel_token: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    pause_ack: Arc<AtomicBool>,
) -> EscListenerGuard {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let queued: InputQueue = Arc::new(std::sync::Mutex::new(Vec::new()));
    let queued_clone = Arc::clone(&queued);

    let handle = tokio::task::spawn_blocking(move || {
        #[cfg(unix)]
        {
            // Ignore SIGTTOU so tcsetattr (called by terminal::enable_raw_mode /
            // disable_raw_mode) does not cause the OS to stop/suspend the process
            // when selfware is running in a background process group or test runner.
            use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
            let action = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());
            let _ = unsafe { sigaction(Signal::SIGTTOU, &action) };
        }

        let mut listener = EscListenerLoop::new(
            CrosstermEventSource,
            cancel_token,
            paused,
            pause_ack,
            stop_clone,
            queued_clone,
        );
        listener.run();
    });

    EscListenerGuard {
        stop,
        handle,
        queued,
    }
}

// Inline tests for the interactive-REPL pure helpers. The 780k-token
// architecture review (2026-09-02) flagged agent::interactive as the
// second-most-complex module with zero inline tests — helpers.rs is the
// pure-function core, so it gets the first thorough coverage.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{PendingMessage, PendingMessageOrigin};
    use std::time::Instant;

    // ── is_exit_command ──────────────────────────────────────────────

    #[test]
    fn exit_commands_recognized() {
        for cmd in ["exit", "quit", "/exit", "/quit", "/q"] {
            assert!(is_exit_command(cmd), "{cmd} must be an exit command");
        }
    }

    #[test]
    fn exit_commands_reject_lookalikes() {
        for cmd in ["", "Exit", " exit", "exit now", "/exitall", "/qq", "quite"] {
            assert!(!is_exit_command(cmd), "{cmd} must NOT be an exit command");
        }
    }

    // ── looks_like_slash_command ─────────────────────────────────────

    #[test]
    fn slash_command_detected() {
        assert!(looks_like_slash_command("/mode yolo"));
        assert!(looks_like_slash_command("/analyze"));
        assert!(looks_like_slash_command("/journal-entry 42"));
        assert!(looks_like_slash_command("/under_score"));
    }

    #[test]
    fn slash_command_rejects_paths_and_plain_text() {
        assert!(!looks_like_slash_command("/tmp/foo.rs"));
        assert!(!looks_like_slash_command("/"));
        assert!(!looks_like_slash_command(""));
        assert!(!looks_like_slash_command("no slash"));
        assert!(!looks_like_slash_command("//comment"));
        assert!(!looks_like_slash_command("/?"));
    }

    // ── safe_truncate ────────────────────────────────────────────────

    #[test]
    fn safe_truncate_short_strings_pass_through() {
        assert_eq!(safe_truncate("abc", 10), "abc");
        assert_eq!(safe_truncate("", 0), "");
        assert_eq!(safe_truncate("abc", 3), "abc");
    }

    #[test]
    fn safe_truncate_respects_char_boundaries() {
        // 'é' is 2 bytes — truncating at byte 1 must back off to the boundary.
        let s = "aéb";
        assert_eq!(safe_truncate(s, 2), "a");
        assert_eq!(safe_truncate(s, 3), "aé");
        // 4-byte emoji: never split inside it.
        let e = "x🦀y";
        assert_eq!(safe_truncate(e, 2), "x");
        assert_eq!(safe_truncate(e, 5), "x🦀");
    }

    // ── flatten_preview_text / preview_with_ellipsis ─────────────────

    #[test]
    fn flatten_collapses_whitespace_runs() {
        assert_eq!(flatten_preview_text("a\nb\tc  d\re"), "a b c d e");
        assert_eq!(flatten_preview_text("  padded  "), "padded");
        assert_eq!(flatten_preview_text("\n\t\r "), "");
    }

    #[test]
    fn preview_adds_ellipsis_only_when_truncated() {
        assert_eq!(preview_with_ellipsis("short", 10), "short");
        let long = "x".repeat(200);
        let preview = preview_with_ellipsis(&long, 10);
        assert!(preview.ends_with("..."));
        assert_eq!(preview.len(), 13);
        // Newlines flatten before truncation, so a multi-line input previews
        // as one line.
        let multiline = preview_with_ellipsis("line one\nline two", 8);
        assert_eq!(multiline, "line one...");
    }

    // ── empty-message / newline stripping ────────────────────────────

    #[test]
    fn effectively_empty_detection() {
        assert!(is_effectively_empty_message(""));
        assert!(is_effectively_empty_message("   \n\t\r\n  "));
        assert!(!is_effectively_empty_message(" x "));
    }

    #[test]
    fn strip_trailing_newlines_only() {
        assert_eq!(strip_trailing_submission_newlines("abc\r\n\r\n"), "abc");
        assert_eq!(strip_trailing_submission_newlines("\nabc\n"), "\nabc");
        assert_eq!(strip_trailing_submission_newlines("abc"), "abc");
    }

    // ── coalesce_pending_messages ────────────────────────────────────

    fn queued(content: &str, at: Instant) -> PendingMessage {
        PendingMessage::new(content, PendingMessageOrigin::InteractiveQueue, at)
    }

    #[test]
    fn coalesce_merges_interactive_within_window() {
        let t0 = Instant::now();
        let out = coalesce_pending_messages(vec![
            queued("hello", t0),
            queued("world", t0 + Duration::from_millis(50)),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "hello\nworld");
        // The merged message keeps the LATEST timestamp.
        assert_eq!(out[0].queued_at, t0 + Duration::from_millis(50));
    }

    #[test]
    fn coalesce_does_not_merge_outside_window() {
        let t0 = Instant::now();
        let out = coalesce_pending_messages(vec![
            queued("a", t0),
            queued("b", t0 + Duration::from_millis(500)),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn coalesce_does_not_merge_across_origins() {
        let t0 = Instant::now();
        let out = coalesce_pending_messages(vec![
            queued("a", t0),
            PendingMessage::new("b", PendingMessageOrigin::ManualQueue, t0),
        ]);
        assert_eq!(out.len(), 2);
        // Manual first, then interactive — still no merge.
        let out = coalesce_pending_messages(vec![
            PendingMessage::new("a", PendingMessageOrigin::ManualQueue, t0),
            queued("b", t0),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn coalesce_drops_empty_messages() {
        let t0 = Instant::now();
        let out = coalesce_pending_messages(vec![
            queued("   ", t0),
            queued("real", t0),
            queued("\n\t", t0),
        ]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].content, "real");
    }

    #[test]
    fn coalesce_empty_input_gives_empty_output() {
        assert!(coalesce_pending_messages(Vec::new()).is_empty());
    }
}
