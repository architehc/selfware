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

// ---------------------------------------------------------------------------
// Not-run semantics (0.8.2 validation D9 / D9b)
// ---------------------------------------------------------------------------

/// Write an executable stub at `node_modules/.bin/<name>` that prints
/// `stdout` and exits with `code`.
#[cfg(unix)]
fn stub_node_bin(root: &std::path::Path, name: &str, stdout: &str, code: i32) {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("node_modules").join(".bin");
    std::fs::create_dir_all(&bin).unwrap();
    let p = bin.join(name);
    std::fs::write(
        &p,
        format!("#!/bin/sh\ncat <<'OUT'\n{stdout}\nOUT\nexit {code}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// D9b: a program that cannot be spawned (`npm` absent on the host) is a
/// NOT-RUN stage, never `test ✗`.
#[tokio::test]
async fn missing_binary_is_not_run_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    let r = run_stage(
        QaStage::Test,
        "selfware-definitely-missing-tool-7f3a",
        &["test"],
        dir.path(),
        10,
    )
    .await;
    let reason = r
        .not_run
        .as_deref()
        .expect("missing binary must be not_run");
    assert!(reason.contains("not installed"), "{reason}");
    assert!(!r.failed(), "a not-run stage is not a failure");
    assert_eq!(r.error_count, 0);
}

/// D9: eslint installed but the project has no ESLint config → lint is
/// not-run (ESLint >= 6 refuses to run and the old code reported ✗).
#[cfg(unix)]
#[tokio::test]
async fn eslint_without_config_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();
    // Would FAIL if it were run: proves it is not run at all.
    stub_node_bin(
        dir.path(),
        "eslint",
        "ESLint couldn't find a configuration file.",
        2,
    );
    let r = node_lint_stage(dir.path(), 10)
        .await
        .expect("an installed eslint yields a stage");
    let reason = r.not_run.as_deref().expect("no config → not_run");
    assert!(reason.contains("no ESLint configuration"), "{reason}");
    assert!(!r.failed());
}

/// A config the probe misses still ends not-run when ESLint itself says it
/// found no configuration.
#[cfg(unix)]
#[tokio::test]
async fn eslint_reporting_no_config_is_demoted_to_not_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".eslintrc.json"), "{}").unwrap();
    stub_node_bin(
        dir.path(),
        "eslint",
        "Oops! Something went wrong! :( ESLint couldn't find a configuration file.",
        2,
    );
    let r = node_lint_stage(dir.path(), 10).await.unwrap();
    assert!(r.not_run.is_some(), "{}", r.output);
}

/// A lint stage that RAN and reported problems stays a failure.
#[cfg(unix)]
#[tokio::test]
async fn real_eslint_error_stays_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("eslint.config.js"), "export default [];").unwrap();
    stub_node_bin(
        dir.path(),
        "eslint",
        "src/a.js\n  1:7  error  'x' is assigned a value but never used  no-unused-vars",
        1,
    );
    let r = node_lint_stage(dir.path(), 10).await.unwrap();
    assert!(r.not_run.is_none(), "{:?}", r.not_run);
    assert!(r.failed(), "a real lint error must fail: {}", r.output);
    assert!(r.error_count >= 1);
}

/// No `test` script (or the npm init placeholder) → test is not-run.
#[test]
fn missing_or_placeholder_test_script_is_not_configured() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
    let err = npm_test_script(dir.path()).unwrap_err();
    assert!(err.contains("no \"test\" script"), "{err}");

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"scripts":{"test":"echo \"Error: no test specified\" && exit 1"}}"#,
    )
    .unwrap();
    assert!(npm_test_script(dir.path())
        .unwrap_err()
        .contains("placeholder"));

    std::fs::write(
        dir.path().join("package.json"),
        r#"{"scripts":{"test":"node --test"}}"#,
    )
    .unwrap();
    assert_eq!(npm_test_script(dir.path()).unwrap(), "node --test");
}

#[tokio::test]
async fn node_test_stage_without_test_script_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
    // A global vitest would take precedence; only assert when absent.
    if resolve_node_tool(dir.path(), "vitest").await.is_some() {
        return;
    }
    let r = node_test_stage(dir.path(), 10).await;
    assert!(
        r.not_run.as_deref().unwrap_or("").contains("test"),
        "{:?} / {}",
        r.not_run,
        r.output
    );
}

/// npm audit without a lockfile cannot reach a verdict.
#[tokio::test]
async fn npm_audit_without_lockfile_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), "{}").unwrap();
    let r = node_audit_stage(dir.path(), 10).await;
    assert!(r.not_run.as_deref().unwrap().contains("lockfile"));
    assert_eq!(
        npm_audit_not_run_reason("npm ERR! code ENOTFOUND\nnpm ERR! network request failed"),
        Some("npm registry unreachable (ENOTFOUND)".to_string())
    );
    assert_eq!(
        npm_audit_not_run_reason("found 3 high severity vulnerabilities"),
        None
    );
}

/// Rule-5 sweep: "nothing was tested" is not-run for Python and Go too.
#[test]
fn no_tests_collected_is_not_run_for_python_and_go() {
    let ran = |passed: bool, output: &str| QaStageResult {
        stage: QaStage::Test,
        passed,
        duration_ms: 1,
        output: output.into(),
        error_count: 0,
        warning_count: 0,
        not_run: None,
    };
    assert!(
        classify_python_test(ran(false, "no tests ran in 0.01s"), Some(5))
            .not_run
            .is_some()
    );
    assert!(
        classify_python_test(ran(true, "Ran 0 tests in 0.000s\n\nOK"), Some(0))
            .not_run
            .is_some()
    );
    assert!(classify_python_test(ran(false, "1 failed"), Some(1)).failed());
    assert!(classify_go_test(ran(true, "?   \tex/m\t[no test files]"))
        .not_run
        .is_some());
    assert!(
        classify_go_test(ran(true, "?   \tex/a\t[no test files]\nok  \tex/b\t0.01s"))
            .not_run
            .is_none()
    );
}
