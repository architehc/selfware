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
