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
    let mut agent = crate::agent::Agent::new(config)
        .await
        .expect("agent creation");
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
    let mut agent = crate::agent::Agent::new(config)
        .await
        .expect("agent creation");
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
