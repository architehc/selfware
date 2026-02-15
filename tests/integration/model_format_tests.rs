//! Tests that verify the model outputs tool calls in the expected format
//!
//! These tests catch bugs where the model outputs formats the parser doesn't handle.

use std::process::Command;

/// Test that a simple task results in tool calls being parsed and executed
/// This catches format mismatches between model output and parser expectations
#[test]
#[cfg(feature = "integration")]
fn test_model_tool_calls_are_parsed() {
    let output = Command::new("./target/release/selfware")
        .args(["run", "list files in the current directory"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check that tools were actually called (not just warnings about unparsed calls)
    assert!(
        stdout.contains("Tool succeeded") || stdout.contains("✓"),
        "Should have successful tool calls. stdout: {}\nstderr: {}",
        stdout,
        stderr
    );

    // Check for the warning that indicates parsing failed
    assert!(
        !stderr.contains("no valid tool calls were parsed"),
        "Tool calls should be parsed successfully. stderr: {}",
        stderr
    );
}

/// Test that /analyze command works end-to-end with real model
#[test]
#[cfg(feature = "integration")]
fn test_analyze_tool_calls_work() {
    let output = Command::new("./target/release/selfware")
        .args(["analyze", "./src"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should complete successfully
    assert!(
        stdout.contains("completed") || stdout.contains("Tool succeeded"),
        "Analyze should complete. stdout: {}",
        stdout
    );

    // Should not have unparsed tool call warnings
    assert!(
        !stderr
            .contains("Content appears to contain tool-related keywords but no valid tool calls"),
        "All tool calls should be parsed. stderr: {}",
        stderr
    );
}

/// Test that the parser handles whatever format the current model produces
#[test]
#[cfg(feature = "integration")]
fn test_model_format_compatibility() {
    let output = Command::new("./target/release/selfware")
        .args(["run", "read the file Cargo.toml"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Must have at least one successful tool call
    let tool_succeeded = stdout.matches("Tool succeeded").count() + stdout.matches("✓").count();

    assert!(
        tool_succeeded >= 1,
        "Should have at least 1 successful tool call. Got {}. stdout: {}",
        tool_succeeded,
        stdout
    );

    // Count parse warnings
    let parse_warnings = stderr.matches("no valid tool calls were parsed").count()
        + stderr.matches("tool-related keywords but no valid").count();

    assert!(
        parse_warnings == 0,
        "Should have 0 parse warnings, got {}. stderr: {}",
        parse_warnings,
        stderr
    );
}

/// Test interactive mode commands result in proper tool execution
#[test]
#[cfg(feature = "integration")]
fn test_interactive_analyze_parses_tools() {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("./target/release/selfware")
        .arg("chat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"/analyze ./src\nexit\n").ok();
    }

    let output = child.wait_with_output().expect("Failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check tools were executed
    assert!(
        stdout.contains("Tool succeeded") || stdout.contains("✓"),
        "Interactive analyze should execute tools. stdout: {}",
        stdout
    );

    // Check no format mismatch warnings
    assert!(
        !stderr.contains("no valid tool calls were parsed"),
        "Interactive tool calls should parse. stderr: {}",
        stderr
    );
}
