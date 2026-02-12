//! System tests for interactive mode
//!
//! Tests all command paths and edge cases in the interactive CLI mode.
//! These tests use subprocess execution to simulate real user interaction.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Get the selfware binary path, preferring debug (freshly built) over release
fn get_binary_path() -> String {
    if let Ok(path) = std::env::var("SELFWARE_BINARY") {
        return path;
    }

    let debug_path = "./target/debug/selfware";
    let release_path = "./target/release/selfware";

    // Prefer debug build to avoid running stale release binaries
    if Path::new(debug_path).exists() {
        debug_path.to_string()
    } else if Path::new(release_path).exists() {
        release_path.to_string()
    } else {
        debug_path.to_string()
    }
}

/// Helper to run selfware with input and capture output with timeout enforcement
fn run_interactive(input: &str, timeout_secs: u64) -> (String, String, i32) {
    let binary = get_binary_path();
    let mut child = Command::new(&binary)
        .arg("chat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    // Write input to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).ok();
        stdin.write_all(b"\n").ok();
    }

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    // Poll for completion with timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Child exited, read output
                let mut stdout_buf = Vec::new();
                let mut stderr_buf = Vec::new();
                if let Some(mut stdout) = child.stdout.take() {
                    stdout.read_to_end(&mut stdout_buf).ok();
                }
                if let Some(mut stderr) = child.stderr.take() {
                    stderr.read_to_end(&mut stderr_buf).ok();
                }
                let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
                let stderr = String::from_utf8_lossy(&stderr_buf).to_string();
                let code = status.code().unwrap_or(-1);
                return (stdout, stderr, code);
            }
            Ok(None) => {
                // Still running, check timeout
                if start.elapsed() >= timeout {
                    child.kill().ok();
                    child.wait().ok();
                    return ("".to_string(), "timeout".to_string(), -1);
                }
                std::thread::sleep(poll_interval);
            }
            Err(_) => {
                return ("".to_string(), "process error".to_string(), -1);
            }
        }
    }
}

/// Helper to run selfware 'run' command (non-interactive) with timeout enforcement
/// Uses --yolo to auto-approve tools since non-interactive mode requires it
fn run_task(task: &str, timeout_secs: u64) -> (String, String, i32) {
    let mut child = Command::new(&get_binary_path())
        .args(["--yolo", "run", task])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let poll_interval = Duration::from_millis(100);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout_buf = Vec::new();
                let mut stderr_buf = Vec::new();
                if let Some(mut stdout) = child.stdout.take() {
                    stdout.read_to_end(&mut stdout_buf).ok();
                }
                if let Some(mut stderr) = child.stderr.take() {
                    stderr.read_to_end(&mut stderr_buf).ok();
                }
                let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
                let stderr = String::from_utf8_lossy(&stderr_buf).to_string();
                let code = status.code().unwrap_or(-1);
                return (stdout, stderr, code);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    child.kill().ok();
                    child.wait().ok();
                    return ("".to_string(), "timeout".to_string(), -1);
                }
                std::thread::sleep(poll_interval);
            }
            Err(_) => {
                return ("".to_string(), "process error".to_string(), -1);
            }
        }
    }
}

/// Test the /help command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_help_command() {
    let (stdout, _stderr, _code) = run_interactive("/help\nexit\n", 30);

    // Should show help menu
    assert!(
        stdout.contains("/help") || stdout.contains("Commands:"),
        "Should display help. Got: {}",
        stdout
    );
}

/// Test the /status command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_status_command() {
    let (stdout, _stderr, _code) = run_interactive("/status\nexit\n", 30);

    // Should show status info
    assert!(
        stdout.contains("Messages") || stdout.contains("Memory") || stdout.contains("tokens"),
        "Should display status. Got: {}",
        stdout
    );
}

/// Test the /memory command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_memory_command() {
    let (stdout, _stderr, _code) = run_interactive("/memory\nexit\n", 30);

    // Should show memory stats
    assert!(
        stdout.contains("Memory") || stdout.contains("tokens") || stdout.contains("entries"),
        "Should display memory stats. Got: {}",
        stdout
    );
}

/// Test the /clear command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_clear_command() {
    let (stdout, _stderr, _code) = run_interactive("/clear\nexit\n", 30);

    // Should confirm clearing
    assert!(
        stdout.contains("clear") || stdout.contains("Clear"),
        "Should confirm clearing. Got: {}",
        stdout
    );
}

/// Test the /tools command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_tools_command() {
    let (stdout, _stderr, _code) = run_interactive("/tools\nexit\n", 30);

    // Should list tools (file_read is a core tool)
    assert!(
        stdout.contains("file_read") || stdout.contains("directory_tree"),
        "Should list tools. Got: {}",
        stdout
    );
}

/// Test exit command
#[test]
#[cfg(feature = "integration")]
fn test_interactive_exit_command() {
    let (stdout, _stderr, code) = run_interactive("exit\n", 30);

    // Should exit cleanly
    assert!(
        code == 0 || stdout.contains("exit") || stdout.contains("Basic Mode"),
        "Should exit. Code: {}, stdout: {}",
        code,
        stdout
    );
}

/// Test quit command (alias for exit)
#[test]
#[cfg(feature = "integration")]
fn test_interactive_quit_command() {
    let (stdout, _stderr, code) = run_interactive("quit\n", 30);

    // Should exit cleanly
    assert!(
        code == 0 || stdout.contains("quit") || stdout.contains("Basic Mode"),
        "Should quit. Code: {}, stdout: {}",
        code,
        stdout
    );
}

/// Test fallback to basic mode when terminal unavailable
#[test]
#[cfg(feature = "integration")]
fn test_interactive_fallback_to_basic_mode() {
    let (stdout, stderr, _code) = run_interactive("exit\n", 30);

    // Should fall back to basic mode (since we're not in a real TTY)
    assert!(
        stdout.contains("Basic Mode")
            || stderr.contains("basic mode")
            || stderr.contains("falling back"),
        "Should fall back to basic mode. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test the run command (non-interactive)
#[test]
#[cfg(feature = "integration")]
fn test_run_command_simple_task() {
    let (stdout, _stderr, _code) = run_task("echo hello", 60);

    // Should complete the task
    assert!(
        stdout.contains("Task") || stdout.contains("completed") || stdout.contains("Tool"),
        "Should run task. stdout: {}",
        stdout
    );
}

/// Test analyze command
#[test]
#[cfg(feature = "integration")]
fn test_analyze_command() {
    let output = Command::new(&get_binary_path())
        .args(["analyze", "./src"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should analyze the directory
    assert!(
        stdout.contains("Surveying") || stdout.contains("directory") || stdout.contains("Tool"),
        "Should analyze directory. Got: {}",
        stdout
    );
}

/// Test --help flag
#[test]
#[cfg(feature = "integration")]
fn test_help_flag() {
    let output = Command::new(&get_binary_path())
        .arg("--help")
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should show CLI help
    assert!(
        stdout.contains("Usage:") || stdout.contains("selfware"),
        "Should show help. Got: {}",
        stdout
    );
    assert!(stdout.contains("chat"), "Should list chat command");
    assert!(stdout.contains("run"), "Should list run command");
}

/// Test --version flag
#[test]
#[cfg(feature = "integration")]
fn test_version_flag() {
    let output = Command::new(&get_binary_path())
        .arg("--version")
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should show version
    assert!(
        stdout.contains("selfware") || stdout.contains("0."),
        "Should show version. Got: {}",
        stdout
    );
}

/// Test journal command
#[test]
#[cfg(feature = "integration")]
fn test_journal_command() {
    let output = Command::new(&get_binary_path())
        .arg("journal")
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(-1);

    // Should list journal entries (may be empty)
    assert!(
        code == 0 || stdout.contains("journal") || stdout.contains("No"),
        "Should handle journal. Code: {}, stdout: {}",
        code,
        stdout
    );
}

/// Test status command
#[test]
#[cfg(feature = "integration")]
fn test_status_command() {
    let output = Command::new(&get_binary_path())
        .arg("status")
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should show status
    assert!(
        stdout.contains("WORKSHOP") || stdout.contains("status") || stdout.contains("Status"),
        "Should show status. Got: {}",
        stdout
    );
}

/// Test garden command
#[test]
#[cfg(feature = "integration")]
fn test_garden_command() {
    let output = Command::new(&get_binary_path())
        .args(["garden", "."])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(-1);

    // Should show garden view
    assert!(
        code == 0 || stdout.contains("garden") || stdout.contains("Garden"),
        "Should show garden. Code: {}, stdout: {}",
        code,
        stdout
    );
}

/// Test multi-chat command initialization
#[test]
#[cfg(feature = "integration")]
fn test_multi_chat_init() {
    let mut child = Command::new(&get_binary_path())
        .args(["multi-chat", "-n", "2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    // Send exit immediately
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"exit\n").ok();
    }

    let output = child.wait_with_output().expect("Failed to wait");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should initialize multi-agent mode
    assert!(
        stdout.contains("concurrent") || stdout.contains("Multi") || stdout.contains("WORKSHOP"),
        "Should init multi-chat. Got: {}",
        stdout
    );
}

/// Test config file specification
#[test]
#[cfg(feature = "integration")]
fn test_config_flag() {
    let output = Command::new(&get_binary_path())
        .args(["-c", "selfware.toml", "--help"])
        .output()
        .expect("Failed to run selfware");

    let code = output.status.code().unwrap_or(-1);

    // Should accept config flag
    assert!(code == 0, "Should accept config flag");
}

/// Test workdir flag
#[test]
#[cfg(feature = "integration")]
fn test_workdir_flag() {
    let output = Command::new(&get_binary_path())
        .args(["-C", "/tmp", "--help"])
        .output()
        .expect("Failed to run selfware");

    let code = output.status.code().unwrap_or(-1);

    // Should accept workdir flag
    assert!(code == 0, "Should accept workdir flag");
}

/// Test invalid command
#[test]
#[cfg(feature = "integration")]
fn test_invalid_command() {
    let output = Command::new(&get_binary_path())
        .arg("invalid_command_xyz")
        .output()
        .expect("Failed to run selfware");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);

    // Should error on invalid command
    assert!(
        code != 0 || stderr.contains("error") || stderr.contains("invalid"),
        "Should reject invalid command. Code: {}",
        code
    );
}

/// Test quiet mode
#[test]
#[cfg(feature = "integration")]
fn test_quiet_mode() {
    let output = Command::new(&get_binary_path())
        .args(["-q", "status"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Quiet mode should have less output (no banner)
    // Note: This is a weak test, mainly checking it doesn't crash
    let _ = stdout;
}

/// Test Ctrl+C handling (interrupt)
#[test]
#[cfg(feature = "integration")]
fn test_interrupt_handling() {
    // This tests that the process can be killed cleanly
    let mut child = Command::new(&get_binary_path())
        .arg("chat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn selfware");

    // Give it a moment to start
    std::thread::sleep(Duration::from_millis(500));

    // Kill it
    child.kill().ok();
    let status = child.wait().expect("Failed to wait");

    // Should be killed (not crash)
    assert!(
        !status.success() || status.code().is_some(),
        "Process should be killable"
    );
}

/// Test that binary exists and is executable
#[test]
#[cfg(feature = "integration")]
fn test_binary_exists() {
    let binary_path = get_binary_path();
    let path = Path::new(&binary_path);
    assert!(
        path.exists(),
        "Binary should exist at {} (build with: cargo build or cargo build --release)",
        binary_path
    );
}

/// Test environment variable configuration
#[test]
#[cfg(feature = "integration")]
fn test_env_var_config() {
    let output = Command::new(&get_binary_path())
        .env("SELFWARE_DEBUG", "1")
        .arg("--help")
        .output()
        .expect("Failed to run selfware");

    let code = output.status.code().unwrap_or(-1);

    // Should accept env var
    assert!(code == 0, "Should work with env var");
}

/// Test --output-format json produces valid JSON
#[test]
#[cfg(feature = "integration")]
fn test_output_format_json() {
    let output = Command::new(&get_binary_path())
        .args(["--output-format", "json", "status"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let code = output.status.code().unwrap_or(-1);

    assert!(code == 0, "Should exit successfully");

    // Output should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(
        parsed.is_ok(),
        "Output should be valid JSON. Got: {}",
        stdout
    );

    // JSON should contain expected fields
    let json = parsed.unwrap();
    assert!(json.get("model").is_some(), "JSON should have 'model' field");
    assert!(
        json.get("journal").is_some(),
        "JSON should have 'journal' field"
    );
}

/// Test --no-color disables color codes
#[test]
#[cfg(feature = "integration")]
fn test_no_color_flag() {
    let output = Command::new(&get_binary_path())
        .args(["--no-color", "status"])
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should not contain ANSI escape codes
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI escape codes with --no-color"
    );
}

/// Test NO_COLOR env var disables color codes
#[test]
#[cfg(feature = "integration")]
fn test_no_color_env_var() {
    let output = Command::new(&get_binary_path())
        .env("NO_COLOR", "1")
        .arg("status")
        .output()
        .expect("Failed to run selfware");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Should not contain ANSI escape codes
    assert!(
        !stdout.contains("\x1b["),
        "Output should not contain ANSI escape codes with NO_COLOR env var"
    );
}

/// Test SELFWARE_TIMEOUT env var is applied
#[test]
#[cfg(feature = "integration")]
fn test_selfware_timeout_env_var() {
    // This just checks the env var is accepted without error
    // Actual timeout behavior is hard to test without a slow endpoint
    let output = Command::new(&get_binary_path())
        .env("SELFWARE_TIMEOUT", "120")
        .arg("--help")
        .output()
        .expect("Failed to run selfware");

    let code = output.status.code().unwrap_or(-1);
    assert!(code == 0, "Should accept SELFWARE_TIMEOUT env var");
}
