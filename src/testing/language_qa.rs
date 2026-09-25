#![allow(dead_code, unused_imports, unused_variables)]
//! Language-specific QA runners.
//!
//! Each runner executes the standard QA pipeline stages (syntax, format, lint,
//! type check, test, security) using language-appropriate tools.

use anyhow::Result;
use std::path::Path;
use std::time::Instant;
use tokio::process::Command;
use tracing::{debug, info, warn};

use super::qa_profiles::{QaStage, QaStageResult};

/// Maximum command output to capture per stage.
const MAX_OUTPUT_BYTES: usize = 32 * 1024;

/// Detected language for QA dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QaLanguage {
    Rust,
    Python,
    Node,
    Go,
    Unknown,
}

impl QaLanguage {
    /// Detect the project language from the working directory.
    pub fn detect(project_root: &Path) -> Self {
        if project_root.join("Cargo.toml").exists() {
            Self::Rust
        } else if project_root.join("package.json").exists() {
            Self::Node
        } else if project_root.join("pyproject.toml").exists()
            || project_root.join("setup.py").exists()
            || project_root.join("requirements.txt").exists()
        {
            Self::Python
        } else if project_root.join("go.mod").exists() {
            Self::Go
        } else {
            Self::Unknown
        }
    }
}

impl std::fmt::Display for QaLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::Python => write!(f, "Python"),
            Self::Node => write!(f, "Node"),
            Self::Go => write!(f, "Go"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Run a shell command and capture output, returning a QA stage result.
///
/// The child runs with a SANITIZED environment (see `safety::process_env`):
/// QA stages execute project-controlled programs (`build.rs`, `setup.py`,
/// `npm install`, test fixtures), and an unsanitized child would inherit
/// every credential on the box. The child also runs in its own process group
/// with `kill_on_drop`, and a timeout KILLS AND REAPS the whole group — a
/// plain `timeout(Command::output())` only drops the future, leaving the
/// timed-out process alive to keep mutating files or holding locks.
async fn run_stage(
    stage: QaStage,
    program: &str,
    args: &[&str],
    project_root: &Path,
    timeout_secs: u64,
) -> QaStageResult {
    run_stage_with_code(stage, program, args, project_root, timeout_secs)
        .await
        .0
}

/// [`run_stage`] plus the child's exit code (`None` when it did not exit
/// normally or never started), for stages whose runner encodes "nothing to
/// check" in a dedicated code (pytest / unittest exit 5 = no tests ran).
///
/// A program that cannot be SPAWNED (not installed: ENOENT; not executable)
/// yields a NOT-RUN stage, never a failure: the check asserted nothing about
/// the code (AGENTS.md Rule 3; 0.8.2 validation D9b, where a missing `npm`
/// was reported as `test ✗` and blocked completion seven times).
async fn run_stage_with_code(
    stage: QaStage,
    program: &str,
    args: &[&str],
    project_root: &Path,
    timeout_secs: u64,
) -> (QaStageResult, Option<i32>) {
    use tokio::io::AsyncReadExt;

    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs.max(5));
    // ONE absolute deadline covers BOTH the child wait and the output
    // collection: a descendant that retained a pipe keeps it open even after
    // the group was reaped, so the collection must be bounded too or the QA
    // stage can stall indefinitely past its timeout.
    let deadline = start + timeout;

    let mut cmd = Command::new(program);
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.kill_on_drop(true);
    cmd.args(args).current_dir(project_root);
    #[cfg(unix)]
    cmd.process_group(0);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let reason = if e.kind() == std::io::ErrorKind::NotFound {
                format!("`{}` is not installed ({})", program, e)
            } else {
                format!("`{}` could not be started ({})", program, e)
            };
            let mut r = QaStageResult::not_run(stage, reason);
            r.duration_ms = start.elapsed().as_millis() as u64;
            return (r, None);
        }
    };
    let child_pid = child.id();

    // Drain stdout/stderr concurrently so a chatty stage can't deadlock on a
    // full pipe; keep only the display cap (matching the previous truncation).
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let mut stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 8192];
        if let Some(mut pipe) = stdout_pipe {
            loop {
                match pipe.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if bytes.len() < MAX_OUTPUT_BYTES {
                            let take = n.min(MAX_OUTPUT_BYTES - bytes.len());
                            bytes.extend_from_slice(&buf[..take]);
                        }
                        // Beyond the cap: keep draining so the child never blocks.
                    }
                }
            }
        }
        bytes
    });
    let mut stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 8192];
        if let Some(mut pipe) = stderr_pipe {
            loop {
                match pipe.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if bytes.len() < MAX_OUTPUT_BYTES {
                            let take = n.min(MAX_OUTPUT_BYTES - bytes.len());
                            bytes.extend_from_slice(&buf[..take]);
                        }
                        // Beyond the cap: keep draining so the child never blocks.
                    }
                }
            }
        }
        bytes
    });

    let wait_result = tokio::time::timeout(timeout, child.wait()).await;
    let mut exit_code = None;
    let (passed, wait_timed_out) = match wait_result {
        Ok(Ok(status)) => {
            exit_code = status.code();
            (status.success(), false)
        }
        Ok(Err(e)) => {
            return (
                QaStageResult {
                    stage,
                    passed: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: format!("Failed to run {} {:?}: {}", program, args, e),
                    error_count: 1,
                    warning_count: 0,
                    not_run: None,
                },
                None,
            )
        }
        Err(_) => {
            // Timed out: kill the ENTIRE process group, then reap the child
            // so no descendant survives to touch project files afterwards.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            (false, true)
        }
    };

    // Bound the collection phase with the SAME absolute deadline. In the
    // normal case the pipes EOF right after the child exits; but a
    // backgrounded descendant that escaped the process-group kill still holds
    // a write end, and an unbounded await here would stall the stage forever
    // (review finding: QA can hang beyond its timeout). On collection
    // timeout, re-kill the group best-effort and abort only the drains that
    // never finished: each drain has its own result slot (shared helper with
    // `run_command_bounded`), so output one stream ALREADY captured is kept.
    let (stdout_bytes, stderr_bytes, drain_timed_out) = {
        use crate::tools::process_guard::{collect_drains_until, DRAIN_AFTER_KILL};
        let mut stdout_slot: Option<Vec<u8>> = None;
        let mut stderr_slot: Option<Vec<u8>> = None;
        collect_drains_until(
            tokio::time::Instant::from_std(deadline),
            (&mut stdout_task, &mut stdout_slot),
            (&mut stderr_task, &mut stderr_slot),
        )
        .await;
        let drain_timed_out = stdout_slot.is_none() || stderr_slot.is_none();
        if drain_timed_out {
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
            }
            collect_drains_until(
                tokio::time::Instant::now() + DRAIN_AFTER_KILL,
                (&mut stdout_task, &mut stdout_slot),
                (&mut stderr_task, &mut stderr_slot),
            )
            .await;
            if stdout_slot.is_none() {
                stdout_task.abort();
            }
            if stderr_slot.is_none() {
                stderr_task.abort();
            }
        }
        (
            stdout_slot.unwrap_or_default(),
            stderr_slot.unwrap_or_default(),
            drain_timed_out,
        )
    };
    let timed_out = wait_timed_out || drain_timed_out;
    let duration_ms = start.elapsed().as_millis() as u64;

    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    if timed_out {
        return (
            QaStageResult {
                stage,
                passed: false,
                duration_ms,
                // Keep whatever was captured before the kill after the notice.
                output: if combined.trim().is_empty() {
                    format!("{} {:?} timed out after {}s", program, args, timeout_secs)
                } else {
                    format!(
                        "{} {:?} timed out after {}s\n{}",
                        program, args, timeout_secs, combined
                    )
                },
                error_count: 1,
                warning_count: 0,
                not_run: None,
            },
            None,
        );
    }

    let error_count = count_pattern(&combined, "error");
    let warning_count = count_pattern(&combined, "warning");

    (
        QaStageResult {
            stage,
            passed,
            duration_ms,
            output: combined,
            error_count,
            warning_count,
            not_run: None,
        },
        exit_code,
    )
}

/// Rough count of a pattern in output (case-insensitive).
fn count_pattern(text: &str, pattern: &str) -> usize {
    text.to_lowercase().matches(&pattern.to_lowercase()).count()
}

/// Try to run a command, returning None if the command is not found.
async fn try_run_stage(
    stage: QaStage,
    program: &str,
    args: &[&str],
    project_root: &Path,
    timeout_secs: u64,
) -> Option<QaStageResult> {
    // Quick check if the program exists
    let mut which = Command::new("which");
    crate::safety::process_env::sanitize_command_env(&mut which);
    let check = which.arg(program).output().await;
    if check.is_err() || !check.unwrap().status.success() {
        debug!("{} not found, skipping {} stage", program, stage);
        return None;
    }
    Some(run_stage(stage, program, args, project_root, timeout_secs).await)
}

// ============================================================================
// Tool resolution
// ============================================================================

/// Byte-compile-free syntax check of every path in argv (single-quote free:
/// it is embedded in a `sh -c '...'` string).
const PY_COMPILE_ALL: &str = r#"import sys
rc = 0
for p in sys.argv[1:]:
    try:
        compile(open(p, "rb").read(), p, "exec", dont_inherit=True)
    except SyntaxError as e:
        rc = 1
        print("%s:%s: SyntaxError: %s" % (p, e.lineno, e.msg))
sys.exit(rc)"#;

/// `pythonX.Y` matching the project's pinned version when that interpreter
/// is installed, else `python3`.
async fn python_for_project(project_root: &Path) -> String {
    if let Some(pin) = super::syntax_toolchain::resolve_python_pin(project_root, project_root) {
        let exact = format!("python{}.{}", pin.min.0, pin.min.1);
        if on_path(&exact).await {
            return exact;
        }
    }
    "python3".to_string()
}

/// True when `program` resolves on the (sanitized) PATH.
async fn on_path(program: &str) -> bool {
    let mut which = Command::new("which");
    crate::safety::process_env::sanitize_command_env(&mut which);
    matches!(which.arg(program).output().await, Ok(o) if o.status.success())
}

/// A Node CLI tool resolved WITHOUT `npx`: the project-local
/// `node_modules/.bin/<name>` first, then `<name>` on PATH. `npx` silently
/// DOWNLOADS a package that is not installed (and for `tsc` it fetches the
/// unrelated `tsc` npm stub), so a missing tool is reported as a skipped
/// stage instead of a network install or a bogus failure.
async fn resolve_node_tool(project_root: &Path, name: &str) -> Option<String> {
    if let Some(p) = super::syntax_toolchain::find_local_node_bin(project_root, name) {
        return Some(p.to_string_lossy().to_string());
    }
    if on_path(name).await {
        Some(name.to_string())
    } else {
        debug!(
            "{} not installed (no node_modules/.bin, not on PATH); stage skipped",
            name
        );
        None
    }
}

// ============================================================================
// Stage-configuration probes (shared by the runners)
// ============================================================================
//
// A stage runs only when it can say something about THIS project. Two cases
// are NOT-RUN rather than a pass or a failure (AGENTS.md Rule 3; 0.8.2
// validation D9/D9b, where `eslint .` with no config and `npm test` without
// npm blocked completion of correct code):
//
// - the tool is missing: not resolvable via `node_modules/.bin` / PATH, or
//   the spawn fails (ENOENT) — see `run_stage_with_code`;
// - the project does not configure the stage: an opinionated tool the
//   project never opted into (no ESLint/Prettier/flake8/mypy/bandit config,
//   no formatter config), no `test` script, no lockfile for an audit, no
//   tests collected, or a network-dependent audit that could not reach its
//   database.
//
// Optional tools that are neither installed nor configured stay silent (as
// before): they were never part of the pipeline for this project. A stage
// that RAN and reported problems is still a failure.

/// Read `package.json` at the project root.
fn read_package_json(project_root: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(project_root.join("package.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Top-level `package.json` key present (e.g. `eslintConfig`, `prettier`).
fn package_json_has_key(project_root: &Path, key: &str) -> bool {
    read_package_json(project_root)
        .and_then(|v| v.get(key).cloned())
        .is_some()
}

/// First existing file among `names` in `dir`.
fn first_existing(dir: &Path, names: &[&str]) -> Option<std::path::PathBuf> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file())
}

const ESLINT_CONFIG_FILES: &[&str] = &[
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.yaml",
    ".eslintrc.yml",
    ".eslintrc.json",
];

/// The project's ESLint configuration, if any. ESLint resolves config files
/// by walking UP from the working directory (both flat and legacy formats),
/// so ancestors count too; `eslintConfig` in package.json is the legacy
/// in-manifest form.
pub(crate) fn eslint_config(project_root: &Path) -> Option<String> {
    for dir in project_root.ancestors() {
        if let Some(p) = first_existing(dir, ESLINT_CONFIG_FILES) {
            return Some(p.to_string_lossy().to_string());
        }
    }
    package_json_has_key(project_root, "eslintConfig").then(|| "package.json#eslintConfig".into())
}

/// ESLint ran but reported that it has no configuration (our probe missed a
/// config form, or the config resolved away): not a lint finding.
fn eslint_output_is_unconfigured(output: &str) -> bool {
    let lower = output.to_lowercase();
    lower.contains("couldn't find a configuration file")
        || lower.contains("could not find a configuration file")
        || lower.contains("couldn't find an eslint.config")
        || lower.contains("could not find config file")
}

/// Prettier is opted into by a config file, a `prettier` key in
/// package.json, or a project-local install.
fn prettier_configured(project_root: &Path) -> bool {
    const FILES: &[&str] = &[
        ".prettierrc",
        ".prettierrc.json",
        ".prettierrc.json5",
        ".prettierrc.yaml",
        ".prettierrc.yml",
        ".prettierrc.toml",
        ".prettierrc.js",
        ".prettierrc.cjs",
        ".prettierrc.mjs",
        ".prettierrc.ts",
        "prettier.config.js",
        "prettier.config.cjs",
        "prettier.config.mjs",
        "prettier.config.ts",
    ];
    first_existing(project_root, FILES).is_some()
        || package_json_has_key(project_root, "prettier")
        || super::syntax_toolchain::find_local_node_bin(project_root, "prettier").is_some()
}

/// The `test` script from package.json, or why the test stage cannot run.
/// `npm init`'s placeholder (`echo "Error: no test specified" && exit 1`)
/// is not a test suite: running it "fails" by design.
pub(crate) fn npm_test_script(project_root: &Path) -> std::result::Result<String, String> {
    let Some(pkg) = read_package_json(project_root) else {
        return Err("package.json is missing or not valid JSON".into());
    };
    match pkg
        .get("scripts")
        .and_then(|s| s.get("test"))
        .and_then(|t| t.as_str())
    {
        None => Err("package.json defines no \"test\" script".into()),
        Some(t) if t.trim().is_empty() => Err("package.json \"test\" script is empty".into()),
        Some(t) if t.contains("no test specified") => {
            Err("package.json \"test\" script is the npm init placeholder".into())
        }
        Some(t) => Ok(t.to_string()),
    }
}

/// `npm audit` could not reach a verdict (no lockfile, no network, registry
/// error) — not a vulnerability finding.
fn npm_audit_not_run_reason(output: &str) -> Option<String> {
    const MARKERS: &[(&str, &str)] = &[
        ("enolock", "npm audit needs a lockfile (ENOLOCK)"),
        ("enotfound", "npm registry unreachable (ENOTFOUND)"),
        ("eai_again", "npm registry unreachable (EAI_AGAIN)"),
        ("econnrefused", "npm registry unreachable (ECONNREFUSED)"),
        ("econnreset", "npm registry unreachable (ECONNRESET)"),
        ("etimedout", "npm registry unreachable (ETIMEDOUT)"),
        ("enetunreach", "npm registry unreachable (ENETUNREACH)"),
        (
            "audit endpoint returned an error",
            "npm audit endpoint returned an error",
        ),
    ];
    let lower = output.to_lowercase();
    MARKERS
        .iter()
        .find(|(m, _)| lower.contains(m))
        .map(|(_, reason)| reason.to_string())
}

/// Re-label a stage that ran but could not reach a verdict as NOT-RUN,
/// keeping its output (so the reason is inspectable).
fn demote_to_not_run(mut r: QaStageResult, reason: String) -> QaStageResult {
    r.passed = false;
    r.error_count = 0;
    r.warning_count = 0;
    r.output = format!("{} stage not run: {}\n{}", r.stage, reason, r.output);
    r.not_run = Some(reason);
    r
}

/// Contents of a text file, or empty.
fn read_or_empty(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

/// A Python tool is configured by a dedicated file or a section in
/// pyproject.toml / setup.cfg / tox.ini.
fn python_tool_configured(
    project_root: &Path,
    files: &[&str],
    pyproject_section: Option<&str>,
    ini_section: Option<&str>,
) -> bool {
    if first_existing(project_root, files).is_some() {
        return true;
    }
    if let Some(sec) = pyproject_section {
        if read_or_empty(&project_root.join("pyproject.toml")).contains(sec) {
            return true;
        }
    }
    if let Some(sec) = ini_section {
        for f in ["setup.cfg", "tox.ini"] {
            if read_or_empty(&project_root.join(f)).contains(sec) {
                return true;
            }
        }
    }
    false
}

/// Run `program` for `stage` only when it is on PATH AND the project
/// configures it. Installed-but-unconfigured → not-run with a reason;
/// configured-but-missing → not-run; neither → `None` (an optional tool the
/// project never used stays silent, as before).
async fn run_if_configured(
    stage: QaStage,
    program: &str,
    args: &[&str],
    configured: bool,
    project_root: &Path,
    timeout_secs: u64,
) -> Option<QaStageResult> {
    let installed = on_path(program).await;
    match (installed, configured) {
        (true, true) => Some(run_stage(stage, program, args, project_root, timeout_secs).await),
        (true, false) => Some(QaStageResult::not_run(
            stage,
            format!("the project does not configure {}", program),
        )),
        (false, true) => Some(QaStageResult::not_run(
            stage,
            format!("{} is configured but not installed", program),
        )),
        (false, false) => {
            debug!(
                "{} neither installed nor configured; {} stage skipped",
                program, stage
            );
            None
        }
    }
}

// ============================================================================
// Python QA Runner
// ============================================================================

pub async fn run_python_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();

    // Syntax: compile() every .py file (up to 50). NOT `py_compile`, which
    // writes `__pycache__/*.pyc` next to every file it checks (workspace
    // pollution); `-B` + `compile()` writes nothing. The interpreter matching
    // the project's pinned version is used when installed (an older host
    // python3 rejects newer syntax as a false syntax error).
    //
    // The pipeline runs under `sh -c`, so a missing interpreter would surface
    // as a shell exit 127 (a bogus failure), not a spawn error: probe first.
    let interp = python_for_project(project_root).await;
    if on_path(&interp).await {
        let syntax_cmd = format!(
            "find . -name '*.py' -not -path './.*' -not -path '*/node_modules/*' | head -50 | xargs {} -B -c '{}' 2>&1",
            interp, PY_COMPILE_ALL
        );
        results.push(
            run_stage(
                QaStage::Syntax,
                "sh",
                &["-c", &syntax_cmd],
                project_root,
                timeout_secs,
            )
            .await,
        );
    } else {
        results.push(QaStageResult::not_run(
            QaStage::Syntax,
            format!("`{}` is not installed", interp),
        ));
    }

    // Format: ruff format --check (falls back to black) — only for a project
    // that configures the formatter; an unconfigured project was never
    // formatted by it, so a diff is not a finding.
    let ruff_configured = python_tool_configured(
        project_root,
        &["ruff.toml", ".ruff.toml"],
        Some("[tool.ruff"),
        None,
    );
    let black_configured = python_tool_configured(project_root, &[], Some("[tool.black"), None);
    if on_path("ruff").await {
        if let Some(r) = run_if_configured(
            QaStage::Format,
            "ruff",
            &["format", "--check", "."],
            ruff_configured || black_configured,
            project_root,
            timeout_secs,
        )
        .await
        {
            results.push(r);
        }
    } else if let Some(r) = run_if_configured(
        QaStage::Format,
        "black",
        &["--check", "."],
        black_configured,
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(r);
    }

    // Lint: `ruff check` runs whenever ruff is installed — its DEFAULT rule
    // set (pyflakes F + E4/E7/E9) is correctness-only (undefined names,
    // syntax-level errors), so it is meaningful without project config.
    // flake8's defaults include pycodestyle style rules (E501 line length,
    // ...), so it runs only when the project configures it.
    if let Some(lint) = try_run_stage(
        QaStage::Lint,
        "ruff",
        &["check", "."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(lint);
    } else if let Some(lint) = run_if_configured(
        QaStage::Lint,
        "flake8",
        &["."],
        python_tool_configured(project_root, &[".flake8"], None, Some("[flake8")),
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(lint);
    }

    // TypeCheck: mypy, only when configured — unconfigured mypy on untyped
    // code reports missing stubs / untyped imports, not defects.
    if let Some(tc) = run_if_configured(
        QaStage::TypeCheck,
        "mypy",
        &["."],
        python_tool_configured(
            project_root,
            &["mypy.ini", ".mypy.ini"],
            Some("[tool.mypy"),
            Some("[mypy"),
        ),
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(tc);
    }

    // Test: pytest (falls back to python -m unittest). Exit status 5 means
    // "no tests were collected/ran" for both runners (unittest since 3.12);
    // older unittest exits 0 after "Ran 0 tests". Neither is a pass or a
    // failure: nothing was tested.
    let (test, code) = if on_path("pytest").await {
        run_stage_with_code(
            QaStage::Test,
            "pytest",
            &["--quiet", "--tb=short"],
            project_root,
            timeout_secs * 2,
        )
        .await
    } else {
        run_stage_with_code(
            QaStage::Test,
            "python3",
            &["-m", "unittest", "discover", "-s", ".", "-q"],
            project_root,
            timeout_secs * 2,
        )
        .await
    };
    results.push(classify_python_test(test, code));

    // Security: bandit, only when configured — its defaults flag every
    // `assert` (B101), which fails any pytest suite.
    if let Some(sec) = run_if_configured(
        QaStage::Security,
        "bandit",
        &["-r", ".", "-q"],
        python_tool_configured(
            project_root,
            &[".bandit", "bandit.yaml", "bandit.yml"],
            Some("[tool.bandit"),
            None,
        ),
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(sec);
    }

    results
}

/// Python test stage verdict: "no tests" is not-run, not a pass/failure.
pub(crate) fn classify_python_test(r: QaStageResult, exit_code: Option<i32>) -> QaStageResult {
    if r.not_run.is_some() {
        return r;
    }
    if exit_code == Some(5) {
        return demote_to_not_run(r, "no tests were collected (exit status 5)".into());
    }
    if r.passed && r.output.contains("Ran 0 tests") {
        return demote_to_not_run(r, "no tests were found (Ran 0 tests)".into());
    }
    r
}

// ============================================================================
// Node.js / TypeScript QA Runner
// ============================================================================

pub async fn run_node_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();
    let has_ts = project_root.join("tsconfig.json").exists();

    // TypeCheck: tsc --noEmit, when the project configures TypeScript.
    if has_ts {
        if let Some(tsc) = resolve_node_tool(project_root, "tsc").await {
            results.push(
                run_stage(
                    QaStage::TypeCheck,
                    &tsc,
                    &["--noEmit"],
                    project_root,
                    timeout_secs,
                )
                .await,
            );
        } else {
            results.push(QaStageResult::not_run(
                QaStage::TypeCheck,
                "tsconfig.json present but tsc is not installed (no node_modules/.bin/tsc, not on PATH)",
            ));
        }
    }

    if let Some(r) = node_format_stage(project_root, timeout_secs).await {
        results.push(r);
    }
    if let Some(r) = node_lint_stage(project_root, timeout_secs).await {
        results.push(r);
    }
    results.push(node_test_stage(project_root, timeout_secs * 2).await);
    results.push(node_audit_stage(project_root, timeout_secs).await);

    results
}

/// Format: prettier --check, only for a project that opted into Prettier.
pub(crate) async fn node_format_stage(
    project_root: &Path,
    timeout_secs: u64,
) -> Option<QaStageResult> {
    let prettier = resolve_node_tool(project_root, "prettier").await;
    let configured = prettier_configured(project_root);
    match (prettier, configured) {
        (Some(p), true) => Some(
            run_stage(
                QaStage::Format,
                &p,
                &["--check", "."],
                project_root,
                timeout_secs,
            )
            .await,
        ),
        (Some(_), false) => Some(QaStageResult::not_run(
            QaStage::Format,
            "the project does not configure Prettier (no .prettierrc*/prettier.config.*, no package.json \"prettier\", not a local dependency)",
        )),
        (None, true) => Some(QaStageResult::not_run(
            QaStage::Format,
            "Prettier is configured but not installed",
        )),
        (None, false) => None,
    }
}

/// Lint: eslint, only with an ESLint configuration. ESLint >= 6 refuses to
/// run without one ("ESLint couldn't find a configuration file"), which the
/// 0.8.2 validation (D9) reported as a lint FAILURE that blocked correct code.
pub(crate) async fn node_lint_stage(
    project_root: &Path,
    timeout_secs: u64,
) -> Option<QaStageResult> {
    let eslint = resolve_node_tool(project_root, "eslint").await;
    let config = eslint_config(project_root);
    match (eslint, config) {
        (Some(e), Some(_)) => {
            let r = run_stage(QaStage::Lint, &e, &["."], project_root, timeout_secs).await;
            if r.failed() && eslint_output_is_unconfigured(&r.output) {
                Some(demote_to_not_run(
                    r,
                    "ESLint found no usable configuration file".into(),
                ))
            } else {
                Some(r)
            }
        }
        (Some(_), None) => Some(QaStageResult::not_run(
            QaStage::Lint,
            "no ESLint configuration (eslint.config.*, .eslintrc*, package.json \"eslintConfig\")",
        )),
        (None, Some(cfg)) => Some(QaStageResult::not_run(
            QaStage::Lint,
            format!("ESLint is configured ({cfg}) but not installed"),
        )),
        (None, None) => None,
    }
}

/// Test: vitest when installed, else the package.json `test` script via npm.
pub(crate) async fn node_test_stage(project_root: &Path, timeout_secs: u64) -> QaStageResult {
    if let Some(vitest) = resolve_node_tool(project_root, "vitest").await {
        return classify_node_test(
            run_stage(
                QaStage::Test,
                &vitest,
                &["run", "--reporter=verbose"],
                project_root,
                timeout_secs,
            )
            .await,
        );
    }
    if let Err(reason) = npm_test_script(project_root) {
        return QaStageResult::not_run(QaStage::Test, reason);
    }
    // `npm test` (not `npm test -- --if-present`: arguments after `--` go to
    // the test SCRIPT, where `--if-present` is an unknown flag). A missing
    // npm is a not-run stage via the spawn error. The script's runner (jest,
    // mocha, node --test, ava, …) finding no tests is not-run too.
    classify_node_test(run_stage(QaStage::Test, "npm", &["test"], project_root, timeout_secs).await)
}

/// JS test stage verdict: a runner that found no tests is not-run, not a
/// pass or a (blocking) failure — vitest / jest `No test(s) found`, mocha
/// `No test files found` / `0 passing`, node `--test` `# tests 0`, ava
/// `Couldn't find any files to test`. The markers are the shared
/// zero-execution detector (`runner_output_proves_no_tests_ran`), so a
/// failure marker or an executed test vetoes the demotion.
pub(crate) fn classify_node_test(r: QaStageResult) -> QaStageResult {
    if r.not_run.is_some() {
        return r;
    }
    if crate::agent::tool_dispatch::helpers::runner_output_proves_no_tests_ran(&r.output) {
        return demote_to_not_run(r, "the test runner found no tests".into());
    }
    r
}

/// Security: npm audit, which needs a lockfile and the registry.
pub(crate) async fn node_audit_stage(project_root: &Path, timeout_secs: u64) -> QaStageResult {
    let has_lock = project_root.join("package-lock.json").is_file()
        || project_root.join("npm-shrinkwrap.json").is_file();
    if !has_lock {
        return QaStageResult::not_run(
            QaStage::Security,
            "no package-lock.json / npm-shrinkwrap.json (npm audit needs a lockfile)",
        );
    }
    let r = run_stage(
        QaStage::Security,
        "npm",
        &["audit", "--omit=dev"],
        project_root,
        timeout_secs,
    )
    .await;
    if r.failed() {
        if let Some(reason) = npm_audit_not_run_reason(&r.output) {
            return demote_to_not_run(r, reason);
        }
    }
    r
}

// ============================================================================
// Go QA Runner
// ============================================================================

pub async fn run_go_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();

    // Syntax + TypeCheck: go build (a missing `go` is a not-run stage via
    // the spawn error in run_stage_with_code).
    results.push(
        run_stage(
            QaStage::Syntax,
            "go",
            &["build", "./..."],
            project_root,
            timeout_secs,
        )
        .await,
    );

    // Format: gofmt -l. Runs under `sh -c` with stderr discarded, so a
    // missing gofmt would print nothing and PASS: probe it first.
    if on_path("gofmt").await {
        results.push(
            run_stage(
                QaStage::Format,
                "sh",
                &["-c", "test -z \"$(gofmt -l . 2>/dev/null)\""],
                project_root,
                timeout_secs,
            )
            .await,
        );
    } else {
        results.push(QaStageResult::not_run(
            QaStage::Format,
            "`gofmt` is not installed",
        ));
    }

    // Lint: go vet
    results.push(
        run_stage(
            QaStage::Lint,
            "go",
            &["vet", "./..."],
            project_root,
            timeout_secs,
        )
        .await,
    );

    // Test: go test
    let test = run_stage(
        QaStage::Test,
        "go",
        &["test", "-v", "./..."],
        project_root,
        timeout_secs * 2,
    )
    .await;
    results.push(classify_go_test(test));

    // Security: govulncheck (if available). It downloads the vulnerability
    // database, so an unreachable network is not-run, not a finding.
    if let Some(sec) = try_run_stage(
        QaStage::Security,
        "govulncheck",
        &["./..."],
        project_root,
        timeout_secs,
    )
    .await
    {
        let lower = sec.output.to_lowercase();
        let offline = lower.contains("dial tcp")
            || lower.contains("no such host")
            || lower.contains("connection refused")
            || lower.contains("i/o timeout");
        results.push(if sec.failed() && offline {
            demote_to_not_run(
                sec,
                "govulncheck could not reach the vulnerability database".into(),
            )
        } else {
            sec
        });
    }

    results
}

/// `go test` over packages that all report `[no test files]` exits 0 having
/// tested nothing: not-run, not a pass.
pub(crate) fn classify_go_test(r: QaStageResult) -> QaStageResult {
    if r.passed
        && r.output.contains("[no test files]")
        && !r
            .output
            .lines()
            .any(|l| l.starts_with("ok ") || l.starts_with("ok\t") || l.starts_with("PASS"))
    {
        return demote_to_not_run(r, "no Go test files".into());
    }
    r
}

#[cfg(test)]
#[path = "../../tests/unit/testing/language_qa/language_qa_test.rs"]
mod tests;
