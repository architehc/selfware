//! Stores the last tool execution result for progressive disclosure via `/last`.
//!
//! During normal operation, tool output is shown as concise one-liner semantic
//! summaries.  The `/last` command lets users inspect the full output without
//! restarting in `--verbose` mode.

use std::sync::Mutex;

/// Captured output from the most recent tool execution.
#[derive(Debug, Clone, Default)]
pub struct LastToolOutput {
    /// Name of the tool that was executed (e.g. `shell_exec`, `file_read`).
    pub tool_name: String,
    /// One-line semantic summary shown in the spinner.
    pub summary: String,
    /// Full result string returned by the tool.
    pub full_output: String,
    /// Whether the tool execution succeeded.
    pub success: bool,
    /// Exit code, when applicable (e.g. for shell_exec).
    pub exit_code: Option<i32>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

static LAST_OUTPUT: Mutex<Option<LastToolOutput>> = Mutex::new(None);

/// Store the output of the most recent tool execution.
pub fn store(output: LastToolOutput) {
    if let Ok(mut guard) = LAST_OUTPUT.lock() {
        *guard = Some(output);
    }
}

/// Retrieve the stored output (cloned).  Returns `None` if no tool has been
/// executed yet or if the lock is poisoned.
pub fn retrieve() -> Option<LastToolOutput> {
    LAST_OUTPUT.lock().ok().and_then(|g| g.clone())
}

/// Clear the stored output (mainly useful for tests).
#[cfg(test)]
pub fn clear() {
    if let Ok(mut guard) = LAST_OUTPUT.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_retrieve_round_trip() {
        clear();

        assert!(retrieve().is_none(), "should start empty");

        store(LastToolOutput {
            tool_name: "shell_exec".into(),
            summary: "Ran: cargo check (exit 0)".into(),
            full_output: r#"{"exit_code":0,"stdout":"ok","stderr":""}"#.into(),
            success: true,
            exit_code: Some(0),
            duration_ms: 123,
        });

        let out = retrieve().expect("should have stored output");
        assert_eq!(out.tool_name, "shell_exec");
        assert_eq!(out.summary, "Ran: cargo check (exit 0)");
        assert!(out.success);
        assert_eq!(out.exit_code, Some(0));
        assert_eq!(out.duration_ms, 123);
    }

    #[test]
    fn latest_store_wins() {
        clear();

        store(LastToolOutput {
            tool_name: "file_read".into(),
            summary: "Read src/main.rs".into(),
            full_output: "first".into(),
            success: true,
            exit_code: None,
            duration_ms: 10,
        });

        store(LastToolOutput {
            tool_name: "shell_exec".into(),
            summary: "Ran: ls".into(),
            full_output: "second".into(),
            success: true,
            exit_code: Some(0),
            duration_ms: 20,
        });

        let out = retrieve().expect("should have stored output");
        assert_eq!(out.tool_name, "shell_exec");
        assert_eq!(out.full_output, "second");
    }

    #[test]
    fn retrieve_clones_not_takes() {
        clear();

        store(LastToolOutput {
            tool_name: "grep_search".into(),
            summary: "Searched 'foo'".into(),
            full_output: "matches".into(),
            success: true,
            exit_code: None,
            duration_ms: 5,
        });

        let _first = retrieve();
        let second = retrieve();
        assert!(
            second.is_some(),
            "retrieve should not consume the stored value"
        );
    }

    #[test]
    fn clear_resets_state() {
        store(LastToolOutput {
            tool_name: "test".into(),
            ..Default::default()
        });
        clear();
        assert!(retrieve().is_none());
    }

    #[test]
    fn default_last_tool_output() {
        let output = LastToolOutput::default();
        assert!(output.tool_name.is_empty());
        assert!(output.summary.is_empty());
        assert!(output.full_output.is_empty());
        assert!(!output.success);
        assert!(output.exit_code.is_none());
        assert_eq!(output.duration_ms, 0);
    }
}
