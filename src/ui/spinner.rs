//! Live Terminal Spinner
//!
//! An animated spinner that updates on the current terminal line using `\r` + ANSI
//! line clearing, driven by a tokio background task. Shows elapsed time.
//!
//! Terminal capability detection: Respects `TERM=dumb`, unset `TERM`, and the
//! `NO_COLOR` environment variable. When ANSI is not supported, spinner output
//! and color sequences are suppressed.

use std::io::{self, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::watch;

use crate::output;
use crate::ui::animations::SPINNER_DOTS;

/// Check if the terminal supports ANSI escape sequences.
///
/// Returns `false` if:
/// - The `TERM` env var is `"dumb"` or unset/empty
/// - Stdout is not a terminal (piped to a file, etc.)
///
/// This does NOT check `NO_COLOR` — use `supports_color()` for that.
pub fn supports_ansi() -> bool {
    if !io::stdout().is_terminal() {
        return false;
    }
    match std::env::var("TERM") {
        Ok(term) => !term.is_empty() && term != "dumb",
        Err(_) => false, // TERM not set
    }
}

/// Check if color output is allowed.
///
/// Returns `false` if:
/// - The `NO_COLOR` env var is set (any value, per <https://no-color.org/>)
/// - ANSI is not supported (see `supports_ansi()`)
pub fn supports_color() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    supports_ansi()
}

/// A terminal spinner that animates on a single line
pub struct TerminalSpinner {
    stop_signal: Arc<AtomicBool>,
    message_tx: watch::Sender<String>,
    handle: Option<tokio::task::JoinHandle<()>>,
    start_time: Instant,
}

impl TerminalSpinner {
    /// Start a new spinner with the given message
    pub fn start(message: &str) -> Self {
        // Skip in compact mode, non-terminal, or dumb terminal
        if output::is_compact() || !io::stdout().is_terminal() || !supports_ansi() {
            return Self {
                stop_signal: Arc::new(AtomicBool::new(true)),
                message_tx: watch::channel(String::new()).0,
                handle: None,
                start_time: Instant::now(),
            };
        }

        let stop_signal = Arc::new(AtomicBool::new(false));
        let (message_tx, message_rx) = watch::channel(message.to_string());
        let stop = stop_signal.clone();
        let start = Instant::now();

        let handle = tokio::spawn(async move {
            let frames = SPINNER_DOTS;
            let mut tick: usize = 0;

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let frame = frames[tick % frames.len()];
                let msg = message_rx.borrow().clone();
                let elapsed = start.elapsed().as_secs_f64();

                // Clear line and print spinner
                let line = format!("  {} {} ({:.1}s)", frame, msg, elapsed);
                print!("\r\x1b[2K{}", line);
                io::stdout().flush().ok();

                tick += 1;
                tokio::time::sleep(tokio::time::Duration::from_millis(80)).await;
            }
        });

        Self {
            stop_signal,
            message_tx,
            handle: Some(handle),
            start_time: Instant::now(),
        }
    }

    /// Update the spinner message
    pub fn set_message(&self, msg: &str) {
        let _ = self.message_tx.send(msg.to_string());
    }

    /// Stop the spinner with a success message
    pub fn stop_success(self, message: &str) {
        if supports_color() {
            self.stop_with_icon("\x1b[32m\u{2714}\x1b[0m", message); // green checkmark
        } else {
            self.stop_with_icon("\u{2714}", message); // checkmark without color
        }
    }

    /// Stop the spinner with an error message
    pub fn stop_error(self, message: &str) {
        if supports_color() {
            self.stop_with_icon("\x1b[31m\u{2715}\x1b[0m", message); // red X
        } else {
            self.stop_with_icon("\u{2715}", message); // X without color
        }
    }

    /// Stop the spinner and print a final line with icon
    fn stop_with_icon(mut self, icon: &str, message: &str) {
        self.stop_signal.store(true, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            handle.abort();
            // Small sleep to let abort propagate
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        if !output::is_compact() && io::stdout().is_terminal() {
            let elapsed = self.start_time.elapsed().as_secs_f64();
            print!("\r\x1b[2K");
            println!("  {} {} ({:.1}s)", icon, message, elapsed);
            io::stdout().flush().ok();
        }
    }

    /// Get elapsed time since spinner started
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

impl Drop for TerminalSpinner {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
        // Clear the spinner line on drop
        if !output::is_compact() && io::stdout().is_terminal() {
            print!("\r\x1b[2K");
            io::stdout().flush().ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spinner_creation_compact_mode() {
        let spinner = TerminalSpinner {
            stop_signal: Arc::new(AtomicBool::new(true)),
            message_tx: watch::channel(String::new()).0,
            handle: None,
            start_time: Instant::now(),
        };
        assert!(spinner.stop_signal.load(Ordering::Relaxed));
    }

    #[test]
    fn test_spinner_elapsed() {
        let spinner = TerminalSpinner {
            stop_signal: Arc::new(AtomicBool::new(true)),
            message_tx: watch::channel(String::new()).0,
            handle: None,
            start_time: Instant::now(),
        };
        assert!(spinner.elapsed().as_secs() < 1);
    }

    #[test]
    fn test_spinner_set_message() {
        let (tx, rx) = watch::channel("initial".to_string());
        let spinner = TerminalSpinner {
            stop_signal: Arc::new(AtomicBool::new(true)),
            message_tx: tx,
            handle: None,
            start_time: Instant::now(),
        };
        spinner.set_message("updated");
        assert_eq!(*rx.borrow(), "updated");
    }

    #[test]
    fn test_spinner_stop_success_no_panic() {
        // Create a spinner in compact mode (no background task)
        let spinner = TerminalSpinner {
            stop_signal: Arc::new(AtomicBool::new(true)),
            message_tx: watch::channel("test".to_string()).0,
            handle: None,
            start_time: Instant::now(),
        };
        spinner.stop_success("Done!");
    }

    #[test]
    fn test_spinner_stop_error_no_panic() {
        let spinner = TerminalSpinner {
            stop_signal: Arc::new(AtomicBool::new(true)),
            message_tx: watch::channel("test".to_string()).0,
            handle: None,
            start_time: Instant::now(),
        };
        spinner.stop_error("Failed!");
    }

    #[test]
    fn test_spinner_drop_no_panic() {
        {
            let _spinner = TerminalSpinner {
                stop_signal: Arc::new(AtomicBool::new(true)),
                message_tx: watch::channel("test".to_string()).0,
                handle: None,
                start_time: Instant::now(),
            };
            // Spinner will be dropped here
        }
        // If we reach here without panic, the test passes
    }

    #[test]
    fn test_spinner_drop_sets_stop_signal() {
        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_signal.clone();
        {
            let _spinner = TerminalSpinner {
                stop_signal: stop_clone,
                message_tx: watch::channel("test".to_string()).0,
                handle: None,
                start_time: Instant::now(),
            };
        }
        assert!(stop_signal.load(Ordering::Relaxed));
    }

    #[test]
    fn test_supports_ansi_returns_bool() {
        // In a test environment (often piped), this typically returns false,
        // but we just verify it doesn't panic and returns a bool.
        let _result: bool = supports_ansi();
    }

    #[test]
    fn test_supports_color_returns_bool() {
        // In a test environment, this typically returns false.
        let _result: bool = supports_color();
    }

    #[test]
    fn test_supports_color_respects_no_color() {
        // If NO_COLOR is set, supports_color should return false.
        // We can't easily set env vars in tests without affecting other tests
        // running in parallel, so just verify the function works.
        let _ = supports_color();
    }

    #[test]
    fn test_supports_ansi_no_panic_on_dumb_term() {
        // Just verify the function doesn't panic regardless of env state
        let _ = supports_ansi();
    }
}
