//! CLI tests that spawn the real `selfware` binary.

use predicates::boolean::PredicateBooleanExt;

#[test]
fn workflow_codegen_prints_rust_stub() {
    let mut cmd = assert_cmd::Command::cargo_bin("selfware").unwrap();
    cmd.args(["workflow", "codegen", "workflows/bug_investigation.swl"])
        .assert()
        .success()
        .stdout(predicates::str::is_empty().not());
}

/// Regression test for the Windows stack overflow: building the clap
/// command tree plus polling the `cli::run()` dispatch needs more stack
/// than Windows' 1MB main-thread default (debug builds). The binary
/// runs its real entry point on a dedicated large-stack thread, so it
/// must work even when the process main-thread stack is tiny.
#[cfg(unix)]
#[test]
fn binary_runs_with_tiny_main_thread_stack() {
    use std::process::Command;

    let bin = assert_cmd::cargo::cargo_bin("selfware");
    let output = Command::new("bash")
        .arg("-c")
        .arg("ulimit -s 1024; exec \"$0\" workflow codegen workflows/bug_investigation.swl")
        .arg(bin)
        .output()
        .expect("failed to spawn bash wrapper");
    assert!(
        output.status.success(),
        "binary failed under a 1MB main-thread stack: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty());
}
