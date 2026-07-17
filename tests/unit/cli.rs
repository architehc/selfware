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

/// P1-4/P1-5 regression: `multi-chat --help` must list the optional one-shot
/// task argument and the global `--coordinator` flag (both used to be
/// absent — the flag was even a parse error after the subcommand).
#[test]
fn multi_chat_help_mentions_task_and_coordinator() {
    let mut cmd = assert_cmd::Command::cargo_bin("selfware").unwrap();
    cmd.args(["multi-chat", "--help"])
        .assert()
        .success()
        .stdout(
            predicates::str::contains("[TASK]").and(predicates::str::contains("--coordinator")),
        );
}

/// P1-5 regression: `multi-chat --coordinator` must parse (previously exit 2,
/// "unexpected argument '--coordinator'"). With a missing config file it
/// fails later on config load — not on clap.
#[test]
fn multi_chat_trailing_coordinator_flag_is_not_a_parse_error() {
    let mut cmd = assert_cmd::Command::cargo_bin("selfware").unwrap();
    cmd.args([
        "multi-chat",
        "--coordinator",
        "noop",
        "-c",
        "/nonexistent/definitely-missing.toml",
    ])
    .assert()
    .failure()
    .stderr(predicates::str::contains("unexpected argument").not());
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
