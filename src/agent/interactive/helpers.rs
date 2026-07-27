use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::{PendingMessage, PendingMessageOrigin};

/// Check if input is an exit command.
pub(crate) fn is_exit_command(input: &str) -> bool {
    matches!(input, "exit" | "quit" | "/exit" | "/quit" | "/q")
}

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
        use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
        use crossterm::terminal;
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        let mut input_buf = String::new();
        let mut showing_prompt = false;

        loop {
            if stop_clone.load(Ordering::Relaxed) || cancel_token.load(Ordering::Relaxed) {
                break;
            }

            if paused.load(Ordering::Relaxed) {
                pause_ack.store(true, Ordering::Release);
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            pause_ack.store(false, Ordering::Release);

            if terminal::enable_raw_mode().is_err() {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }

            let poll_result = event::poll(Duration::from_millis(50));
            let event_result = match poll_result {
                Ok(true) => Some(event::read()),
                Ok(false) => None,
                Err(_) => {
                    let _ = terminal::disable_raw_mode();
                    break;
                }
            };

            let _ = terminal::disable_raw_mode();

            if let Some(read_result) = event_result {
                match read_result {
                    Ok(Event::Key(KeyEvent {
                        code, modifiers, ..
                    })) => match code {
                        KeyCode::Esc => {
                            if showing_prompt {
                                let _ = std::io::stderr().write_all(b"\r\x1b[2K");
                                let _ = std::io::stderr().flush();
                            }
                            input_buf.clear();
                            showing_prompt = false;
                            cancel_token.store(true, Ordering::Relaxed);
                            let _ = std::io::stderr()
                                .write_all(b"\r\n\x1b[33m[ESC] Cancelling...\x1b[0m\r\n");
                            break;
                        }
                        KeyCode::Enter if !input_buf.is_empty() => {
                            let msg = strip_trailing_submission_newlines(&input_buf).to_string();
                            input_buf.clear();
                            showing_prompt = false;
                            if !is_effectively_empty_message(&msg) {
                                let count = {
                                    match queued_clone.lock() {
                                        Ok(mut q) => {
                                            q.push(PendingMessage::new(
                                                msg.clone(),
                                                PendingMessageOrigin::InteractiveQueue,
                                                Instant::now(),
                                            ));
                                            q.len()
                                        }
                                        Err(_) => 0,
                                    }
                                };
                                let notice = format!(
                                            "\r\x1b[2K\x1b[36m  ▸ Queued: \x1b[0m{}\x1b[90m ({})\x1b[0m\r\n",
                                            preview_with_ellipsis(&msg, QUEUE_NOTICE_PREVIEW_BYTES),
                                            count,
                                        );
                                let _ = std::io::stderr().write_all(notice.as_bytes());
                                let _ = std::io::stderr().flush();
                            } else {
                                let _ = std::io::stderr().write_all(b"\r\x1b[2K");
                                let _ = std::io::stderr().flush();
                            }
                        }
                        KeyCode::Backspace if input_buf.pop().is_some() => {
                            render_inline_queue_prompt(&input_buf);
                            if input_buf.is_empty() {
                                let _ = std::io::stderr().write_all(b"\r\x1b[2K");
                                let _ = std::io::stderr().flush();
                                showing_prompt = false;
                            }
                        }
                        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                            cancel_token.store(true, Ordering::Relaxed);
                            let _ = std::io::stderr()
                                .write_all(b"\r\n\x1b[33m[Ctrl+C] Cancelling...\x1b[0m\r\n");
                            break;
                        }
                        KeyCode::Up => {
                            let popped = match queued_clone.lock() {
                                Ok(mut q) => q.pop(),
                                Err(_) => None,
                            };
                            if let Some(msg) = popped {
                                input_buf = msg.content;
                                showing_prompt = true;
                                render_inline_queue_prompt(&input_buf);
                            }
                        }
                        KeyCode::Char(c) => {
                            if !showing_prompt {
                                let _ = std::io::stderr()
                                    .write_all(b"\r\x1b[2K\x1b[90m  \xe2\x96\xb8 \x1b[0m");
                                showing_prompt = true;
                            }
                            input_buf.push(c);
                            render_inline_queue_prompt(&input_buf);
                        }
                        _ => {}
                    },
                    Ok(Event::Paste(text)) => {
                        if !showing_prompt {
                            let _ = std::io::stderr()
                                .write_all(b"\r\x1b[2K\x1b[90m  \xe2\x96\xb8 \x1b[0m");
                            showing_prompt = true;
                        }
                        input_buf.push_str(&text);
                        render_inline_queue_prompt(&input_buf);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }

        if showing_prompt {
            let _ = std::io::stderr().write_all(b"\r\x1b[2K");
            let _ = std::io::stderr().flush();
        }

        pause_ack.store(false, Ordering::Release);
        let _ = terminal::disable_raw_mode();
    });

    EscListenerGuard {
        stop,
        handle,
        queued,
    }
}
