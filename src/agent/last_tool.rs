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
            if s.len() <= max { return; }
            let mut b = max;
            while b > 0 && !s.is_char_boundary(b) { b -= 1; }
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

    #[tokio::test]
    async fn storing_long_output_truncates() {
        let config = crate::config::Config {
            endpoint: "http://localhost:0/v1".to_string(),
            model: "mock-model".to_string(),
            context_length: 500_000,
            max_tokens: 8192,
            execution_mode: crate::config::ExecutionMode::Yolo,
            ..Default::default()
        };
        let mut agent = crate::agent::Agent::new(config).await.expect("agent creation");
        let output = LastToolOutput {
            tool_name: "shell_exec".to_string(),
            summary: "x".repeat(MAX_SUMMARY_LEN + 100),
            full_output: "y".repeat(MAX_FULL_OUTPUT_LEN + 100),
            success: true,
            exit_code: Some(0),
            duration_ms: 42,
        };
        agent.store_last_tool_output(output);
        let stored = agent.retrieve_last_tool_output().unwrap();
        assert_eq!(stored.summary.len(), MAX_SUMMARY_LEN);
        assert_eq!(stored.full_output.len(), MAX_FULL_OUTPUT_LEN);
    }

    // Regression: the byte-index `String::truncate` panics when the limit lands
    // mid-UTF-8-char. "✿" is 3 bytes, so a 16_384-byte cut of 6_000 of them falls
    // inside a character — this stored output must truncate on a char boundary
    // instead of panicking (crashed a live review before the fix).
    #[tokio::test]
    async fn storing_long_multibyte_output_does_not_panic() {
        let config = crate::config::Config {
            endpoint: "http://localhost:0/v1".to_string(),
            model: "mock-model".to_string(),
            context_length: 500_000,
            max_tokens: 8192,
            execution_mode: crate::config::ExecutionMode::Yolo,
            ..Default::default()
        };
        let mut agent = crate::agent::Agent::new(config).await.expect("agent creation");
        let output = LastToolOutput {
            tool_name: "shell_exec".to_string(),
            summary: "é".repeat(MAX_SUMMARY_LEN),
            full_output: "✿".repeat(6_000),
            success: true,
            exit_code: Some(0),
            duration_ms: 1,
        };
        agent.store_last_tool_output(output); // must not panic
        let stored = agent.retrieve_last_tool_output().unwrap();
        assert!(stored.full_output.len() <= MAX_FULL_OUTPUT_LEN);
        assert!(stored.summary.len() <= MAX_SUMMARY_LEN);
        // Still valid UTF-8 (would be impossible if we cut mid-character).
        assert!(std::str::from_utf8(stored.full_output.as_bytes()).is_ok());
    }
}
