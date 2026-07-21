use super::*;
use std::process::{Command, Output};

/// Helper: create a CompilationSandbox struct directly for parse_output testing
/// without actually cloning a git repo (which is expensive and requires a real repo).
fn dummy_sandbox() -> CompilationSandbox {
    CompilationSandbox {
        _original_dir: PathBuf::from("/tmp/dummy-original"),
        work_dir: PathBuf::from("/tmp/dummy-sandbox"),
    }
}

/// Helper: produce an Output from running a simple command with known exit code.
fn make_output(exit_code: i32, stdout: &str, stderr: &str) -> Output {
    // We construct Output by running a real shell command that gives us
    // the exact stdout, stderr, and exit code we want.
    let cmd_str = format!(
        "printf '{}'; printf '{}' >&2; exit {}",
        stdout.replace('\'', "'\\''"),
        stderr.replace('\'', "'\\''"),
        exit_code
    );
    Command::new("sh")
        .arg("-c")
        .arg(&cmd_str)
        .output()
        .expect("failed to run helper command")
}

// --- parse_output tests ---

#[test]
fn test_parse_output_success() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "all good", "");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, "all good");
    assert!(result.stderr.is_empty());
}

#[test]
fn test_parse_output_failure() {
    let sandbox = dummy_sandbox();
    let output = make_output(1, "", "error: something broke");
    let result = sandbox.parse_output(output).unwrap();
    assert!(!result.success);
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, "error: something broke");
}

#[test]
fn test_parse_output_empty() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "", "");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[test]
fn test_parse_output_mixed_stdout_stderr() {
    let sandbox = dummy_sandbox();
    let output = make_output(0, "compiled OK", "warning: unused variable");
    let result = sandbox.parse_output(output).unwrap();
    assert!(result.success);
    assert_eq!(result.stdout, "compiled OK");
    assert_eq!(result.stderr, "warning: unused variable");
}

#[test]
fn test_parse_output_nonzero_exit_with_both_streams() {
    let sandbox = dummy_sandbox();
    let output = make_output(42, "partial output", "fatal error");
    let result = sandbox.parse_output(output).unwrap();
    assert!(!result.success);
    assert_eq!(result.stdout, "partial output");
    assert_eq!(result.stderr, "fatal error");
}

// --- CompilationSandbox::new with nonexistent path ---

#[test]
fn test_new_with_nonexistent_path() {
    let result = CompilationSandbox::new("/tmp/nonexistent-selfware-test-path-abc123xyz");
    // Should fail because git clone from a nonexistent dir will fail
    assert!(result.is_err());
}

// --- cleanup with nonexistent dir (should not panic) ---

#[test]
fn test_cleanup_nonexistent_dir_no_panic() {
    let sandbox = CompilationSandbox {
        _original_dir: PathBuf::from("/tmp/does-not-exist-original"),
        work_dir: PathBuf::from("/tmp/does-not-exist-sandbox-xyz123"),
    };
    // cleanup checks `self.work_dir.exists()` before removing, so this should be Ok
    let result = sandbox.cleanup();
    assert!(result.is_ok());
}

// --- CompileResult fields ---

#[test]
fn test_compile_result_debug() {
    let result = CompileResult {
        success: true,
        stdout: "ok".to_string(),
        stderr: String::new(),
    };
    // Verify Debug is derived
    let debug_str = format!("{:?}", result);
    assert!(debug_str.contains("CompileResult"));
    assert!(debug_str.contains("true"));
}
