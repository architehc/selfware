//! Stores the last tool execution result for progressive disclosure via `/last`.
//!
//! During normal operation, tool output is shown as concise one-liner semantic
//! summaries.  The `/last` command lets users inspect the full output without
//! restarting in `--verbose` mode.
//!
//! The output is stored per-agent (on the `Agent` struct) rather than in
//! process-global state, which makes testing and multi-agent scenarios safe.

/// Maximum characters of full tool output retained for `/last`.
const MAX_FULL_OUTPUT_LEN: usize = 16_384;

/// Maximum characters of the one-line semantic summary retained.
const MAX_SUMMARY_LEN: usize = 512;

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
    ///
    /// Long outputs are truncated to bounded sizes so the agent struct cannot
    /// grow without limit when a tool returns a huge response.
    pub fn store_last_tool_output(&mut self, mut output: LastToolOutput) {
        fn truncate_on_char_boundary(s: &mut String, max: usize) {
            if s.len() <= max {
                return;
            }
            let mut b = max;
            while b > 0 && !s.is_char_boundary(b) {
                b -= 1;
            }
            s.truncate(b);
        }
        truncate_on_char_boundary(&mut output.full_output, MAX_FULL_OUTPUT_LEN);
        truncate_on_char_boundary(&mut output.summary, MAX_SUMMARY_LEN);
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
#[path = "../../tests/unit/agent/last_tool/last_tool_test.rs"]
mod tests;
