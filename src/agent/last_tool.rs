//! Stores the last tool execution result for progressive disclosure via `/last`.
//!
//! During normal operation, tool output is shown as concise one-liner semantic
//! summaries.  The `/last` command lets users inspect the full output without
//! restarting in `--verbose` mode.
//!
//! The output is stored per-agent (on the `Agent` struct) rather than in
//! process-global state, which makes testing and multi-agent scenarios safe.

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

impl super::Agent {
    /// Store the output of the most recent tool execution.
    pub fn store_last_tool_output(&mut self, output: LastToolOutput) {
        self.last_tool_output = Some(output);
    }

    /// Retrieve the stored output (cloned).  Returns `None` if no tool has been
    /// executed yet.
    pub fn retrieve_last_tool_output(&self) -> Option<LastToolOutput> {
        self.last_tool_output.clone()
    }

    /// Clear the stored output.
    pub fn clear_last_tool_output(&mut self) {
        self.last_tool_output = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
