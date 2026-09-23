use super::*;

#[test]
fn test_qa_language_display() {
    assert_eq!(QaLanguage::Rust.to_string(), "Rust");
    assert_eq!(QaLanguage::Python.to_string(), "Python");
    assert_eq!(QaLanguage::Node.to_string(), "Node");
    assert_eq!(QaLanguage::Go.to_string(), "Go");
    assert_eq!(QaLanguage::Unknown.to_string(), "Unknown");
}

#[test]
fn test_count_pattern() {
    assert_eq!(count_pattern("error: foo\nerror: bar", "error"), 2);
    assert_eq!(count_pattern("Warning: something", "warning"), 1);
    assert_eq!(count_pattern("all good", "error"), 0);
}

#[test]
fn test_qa_language_detect() {
    // Can't easily test without creating temp dirs, but verify the logic path
    let tmp = std::env::temp_dir().join("nonexistent_qa_test");
    let lang = QaLanguage::detect(&tmp);
    assert_eq!(lang, QaLanguage::Unknown);
}

/// Regression (review finding P1): `run_stage` executes project-controlled
/// programs, so its child must NOT inherit host credentials. A stub child
/// dumps its environment to a file; the synthetic marker must be absent while
/// the shared allowlist (PATH) still reaches it.
#[tokio::test]
async fn run_stage_sanitizes_env_before_spawning() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_QA_MARKER"]);
    _env.set("SELFWARE_QA_MARKER", "synthetic-leak-marker");

    let dir = tempfile::tempdir().unwrap();
    let outfile = dir.path().join("child.env");

    let result = run_stage(
        QaStage::Syntax,
        "sh",
        &["-c", &format!("env > {}", outfile.display())],
        dir.path(),
        10,
    )
    .await;

    assert!(result.passed, "sh -c env must succeed: {}", result.output);
    let child_env = std::fs::read_to_string(&outfile).expect("stub child must dump its env");
    assert!(
        !child_env.contains("SELFWARE_QA_MARKER"),
        "synthetic marker leaked to the QA child; saw:\n{child_env}"
    );
    assert!(
        child_env.contains("PATH="),
        "the shared allowlist (PATH) must still reach the child; saw:\n{child_env}"
    );
}

/// Regression (review finding P2): a timed-out QA stage must TERMINATE the
/// child and its whole process group, not just drop the future. A stub
/// backgrounds a sleeper and records its pid; after the timeout the sleeper
/// must be dead — otherwise a timed-out stage keeps running and can mutate
/// project files while later stages work.
#[tokio::test]
#[cfg(unix)]
async fn run_stage_timeout_kills_whole_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("gc.pid");

    let start = std::time::Instant::now();
    let result = run_stage(
        QaStage::Test,
        "sh",
        &[
            "-c",
            &format!("sleep 30 & echo $! > {}; wait", pidfile.display()),
        ],
        dir.path(),
        1, // floored to 5s by run_stage
    )
    .await;
    assert!(
        start.elapsed().as_secs() < 10,
        "stage must return at the timeout"
    );
    assert!(!result.passed, "a timed-out stage must not report success");
    assert!(
        result.output.contains("timed out"),
        "output should report the timeout: {}",
        result.output
    );

    let gc_pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("backgrounded sleeper wrote its pid")
        .trim()
        .parse()
        .expect("valid pid");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    let alive = kill(Pid::from_raw(gc_pid), None).is_ok();
    assert!(
        !alive,
        "backgrounded grandchild pid {gc_pid} must be reaped after timeout"
    );
}

/// Regression (follow-up finding, P2): the output-collection phase must be
/// bounded by the SAME deadline as the child wait. A parent that EXITS
/// normally while a backgrounded grandchild keeps the stdout pipe open would
/// otherwise leave the collection awaiting EOF forever — a QA stage that
/// hangs well past its timeout. `sleep 30 & echo done`: the child exits
/// immediately, the sleeper retains the pipe, and the stage must still
/// return in bounded time (reporting the drain timeout honestly).
#[tokio::test]
#[cfg(unix)]
async fn run_stage_collection_bounded_when_grandchild_holds_pipe() {
    let dir = tempfile::tempdir().unwrap();

    let start = std::time::Instant::now();
    let result = run_stage(
        QaStage::Test,
        "sh",
        &["-c", "sleep 30 & echo done"],
        dir.path(),
        1, // floored to 5s by run_stage
    )
    .await;
    assert!(
        start.elapsed().as_secs() < 10,
        "stage must return in bounded time even when a pipe-holding grandchild lingers"
    );
    assert!(
        result.output.contains("timed out"),
        "collection exceeded the deadline, so the stage must be reported as timed out: {}",
        result.output
    );
    assert!(!result.passed, "a timed-out stage must not report success");
}

/// Regression: a drain timeout must not discard output the OTHER stream
/// already captured. The parent writes to stdout and exits (stdout EOFs),
/// while a backgrounded sleeper keeps only STDERR open past the deadline.
/// The stage times out, but the stdout text must survive in the output.
#[tokio::test]
#[cfg(unix)]
async fn run_stage_drain_timeout_keeps_captured_stdout() {
    let dir = tempfile::tempdir().unwrap();

    let start = std::time::Instant::now();
    let result = run_stage(
        QaStage::Test,
        "sh",
        &[
            "-c",
            "echo early-stdout-marker; sleep 30 >/dev/null & exit 0",
        ],
        dir.path(),
        1, // floored to 5s by run_stage
    )
    .await;
    assert!(
        start.elapsed().as_secs() < 12,
        "stage must return in bounded time"
    );
    assert!(!result.passed, "a timed-out stage must not report success");
    assert!(
        result.output.contains("timed out"),
        "drain timeout must still be reported: {}",
        result.output
    );
    assert!(
        result.output.contains("early-stdout-marker"),
        "stdout captured before the stderr drain timed out must be kept: {}",
        result.output
    );
}
