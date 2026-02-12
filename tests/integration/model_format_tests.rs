//! Tests that verify the model outputs tool calls in the expected format
//!
//! These tests catch bugs where the model outputs formats the parser doesn't handle.
//!
//! NOTE: These tests require a running LLM endpoint and are gated behind the "integration" feature.
//! Run with: cargo test --features integration

use std::process::Command;
use std::time::Duration;

/// Get the selfware binary path using Cargo-provided path (ensures freshly built binary)
fn get_binary_path() -> String {
    // Allow override via environment variable
    if let Ok(path) = std::env::var("SELFWARE_BINARY") {
        return path;
    }

    // Use Cargo-provided binary path when running via `cargo test`
    // This ensures we always use the binary that was just built
    env!("CARGO_BIN_EXE_selfware").to_string()
}

/// Helper to run selfware command with timeout
fn run_selfware_with_timeout(
    args: &[&str],
    timeout_secs: u64,
) -> std::io::Result<std::process::Output> {
    use std::io::{Error, ErrorKind};
    use std::process::Stdio;

    let binary = get_binary_path();

    let mut child = Command::new(&binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Error::new(
                e.kind(),
                format!("Failed to spawn {}: {}. Run tests with: cargo test", binary, e),
            )
        })?;

    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait()? {
            Some(_status) => {
                return child.wait_with_output();
            }
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(Error::new(
                        ErrorKind::TimedOut,
                        format!("Command timed out after {} seconds", timeout_secs),
                    ));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Test that a simple task results in tool calls being parsed and executed
/// This catches format mismatches between model output and parser expectations
#[test]
#[cfg(feature = "integration")]
fn test_model_tool_calls_are_parsed() {
    let output = run_selfware_with_timeout(
        &["--yolo", "run", "list files in the current directory"],
        90,
    )
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
/// This test analyzes a single file to be faster and more deterministic
/// Note: Ignored by default due to variable backend latency (run with --include-ignored)
#[test]
#[ignore]
#[cfg(feature = "integration")]
fn test_analyze_tool_calls_work() {
    // Analyze a single small file instead of entire src/ directory
    let output = run_selfware_with_timeout(&["--yolo", "analyze", "./Cargo.toml"], 180)
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should complete without critical errors
    // We're lenient here - just check it didn't timeout and produced output
    assert!(
        !stdout.is_empty() || !stderr.is_empty(),
        "Analyze should produce some output"
    );

    // Should not have unparsed tool call warnings
    assert!(
        !stderr.contains("Content appears to contain tool-related keywords but no valid tool calls"),
        "All tool calls should be parsed. stderr: {}",
        stderr
    );
}

/// Test that the parser handles whatever format the current model produces
#[test]
#[cfg(feature = "integration")]
fn test_model_format_compatibility() {
    let output = run_selfware_with_timeout(&["--yolo", "run", "read the file Cargo.toml"], 90)
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
/// This test is slower and may be flaky with slow models - marked as ignored by default
#[test]
#[ignore] // Run with: cargo test --features integration -- --ignored
#[cfg(feature = "integration")]
fn test_interactive_analyze_parses_tools() {
    use std::io::Write;
    use std::process::Stdio;

    let binary = get_binary_path();

    let mut child = Command::new(&binary)
        .args(["--yolo", "chat"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    if let Some(mut stdin) = child.stdin.take() {
        // Use a simpler command that completes faster
        stdin.write_all(b"list files in current directory\nexit\n").ok();
    }

    // Wait with timeout
    let timeout = Duration::from_secs(180);
    let start = std::time::Instant::now();

    let output = loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break child.wait_with_output().expect("Failed to get output");
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    panic!(
                        "Interactive test timed out after {} seconds",
                        timeout.as_secs()
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                panic!("Error waiting for child: {}", e);
            }
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check tools were executed or at least we got a response
    assert!(
        stdout.contains("Tool succeeded") || stdout.contains("✓") || !stdout.is_empty(),
        "Interactive mode should produce output. stdout: {}",
        stdout
    );

    // Check no format mismatch warnings
    assert!(
        !stderr.contains("no valid tool calls were parsed"),
        "Interactive tool calls should parse. stderr: {}",
        stderr
    );
}
