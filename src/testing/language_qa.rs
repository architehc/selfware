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
            return QaStageResult {
                stage,
                passed: false,
                duration_ms: start.elapsed().as_millis() as u64,
                output: format!("Failed to run {} {:?}: {}", program, args, e),
                error_count: 1,
                warning_count: 0,
            }
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
    let (passed, wait_timed_out) = match wait_result {
        Ok(Ok(status)) => (status.success(), false),
        Ok(Err(e)) => {
            return QaStageResult {
                stage,
                passed: false,
                duration_ms: start.elapsed().as_millis() as u64,
                output: format!("Failed to run {} {:?}: {}", program, args, e),
                error_count: 1,
                warning_count: 0,
            }
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
    // timeout, abort the drains and re-kill the group best-effort.
    let remaining = deadline.saturating_duration_since(Instant::now());
    let (stdout_bytes, stderr_bytes, drain_timed_out) =
        match tokio::time::timeout(remaining, async {
            tokio::join!(&mut stdout_task, &mut stderr_task)
        })
        .await
        {
            Ok((out, err)) => (out.unwrap_or_default(), err.unwrap_or_default(), false),
            Err(_) => {
                stdout_task.abort();
                stderr_task.abort();
                #[cfg(unix)]
                if let Some(pid) = child_pid {
                    use nix::sys::signal::{killpg, Signal};
                    use nix::unistd::Pid;
                    let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
                }
                (Vec::new(), Vec::new(), true)
            }
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
        return QaStageResult {
            stage,
            passed: false,
            duration_ms,
            output: format!("{} {:?} timed out after {}s", program, args, timeout_secs),
            error_count: 1,
            warning_count: 0,
        };
    }

    let error_count = count_pattern(&combined, "error");
    let warning_count = count_pattern(&combined, "warning");

    QaStageResult {
        stage,
        passed,
        duration_ms,
        output: combined,
        error_count,
        warning_count,
    }
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
// Python QA Runner
// ============================================================================

pub async fn run_python_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();

    // Syntax: python -m py_compile (check all .py files)
    results.push(
        run_stage(
            QaStage::Syntax,
            "python3",
            &["-m", "py_compile", "--help"], // placeholder - we'll use a find command
            project_root,
            timeout_secs,
        )
        .await,
    );
    // Override with a more comprehensive syntax check
    results.pop();
    results.push(
        run_stage(
            QaStage::Syntax,
            "sh",
            &["-c", "find . -name '*.py' -not -path './.*' -not -path '*/node_modules/*' | head -50 | xargs python3 -m py_compile 2>&1"],
            project_root,
            timeout_secs,
        )
        .await,
    );

    // Format: ruff format --check (falls back to black)
    if let Some(fmt) = try_run_stage(
        QaStage::Format,
        "ruff",
        &["format", "--check", "."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(fmt);
    } else if let Some(fmt) = try_run_stage(
        QaStage::Format,
        "black",
        &["--check", "."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(fmt);
    }

    // Lint: ruff check (falls back to flake8)
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
    } else if let Some(lint) =
        try_run_stage(QaStage::Lint, "flake8", &["."], project_root, timeout_secs).await
    {
        results.push(lint);
    }

    // TypeCheck: mypy (if available)
    if let Some(tc) = try_run_stage(
        QaStage::TypeCheck,
        "mypy",
        &["."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(tc);
    }

    // Test: pytest (falls back to python -m unittest)
    if let Some(test) = try_run_stage(
        QaStage::Test,
        "pytest",
        &["--quiet", "--tb=short"],
        project_root,
        timeout_secs * 2,
    )
    .await
    {
        results.push(test);
    } else {
        results.push(
            run_stage(
                QaStage::Test,
                "python3",
                &["-m", "unittest", "discover", "-s", ".", "-q"],
                project_root,
                timeout_secs * 2,
            )
            .await,
        );
    }

    // Security: bandit (if available)
    if let Some(sec) = try_run_stage(
        QaStage::Security,
        "bandit",
        &["-r", ".", "-q"],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(sec);
    }

    results
}

// ============================================================================
// Node.js / TypeScript QA Runner
// ============================================================================

pub async fn run_node_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();
    let has_ts = project_root.join("tsconfig.json").exists();

    // Syntax / TypeCheck: tsc --noEmit (for TS) or node --check (for JS)
    if has_ts {
        results.push(
            run_stage(
                QaStage::TypeCheck,
                "npx",
                &["tsc", "--noEmit"],
                project_root,
                timeout_secs,
            )
            .await,
        );
    }

    // Format: prettier --check
    if let Some(fmt) = try_run_stage(
        QaStage::Format,
        "npx",
        &["prettier", "--check", "."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(fmt);
    }

    // Lint: eslint
    if let Some(lint) = try_run_stage(
        QaStage::Lint,
        "npx",
        &["eslint", "."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(lint);
    }

    // Test: npm test (or vitest / jest)
    if let Some(test) = try_run_stage(
        QaStage::Test,
        "npx",
        &["vitest", "run", "--reporter=verbose"],
        project_root,
        timeout_secs * 2,
    )
    .await
    {
        results.push(test);
    } else {
        results.push(
            run_stage(
                QaStage::Test,
                "npm",
                &["test", "--", "--if-present"],
                project_root,
                timeout_secs * 2,
            )
            .await,
        );
    }

    // Security: npm audit
    results.push(
        run_stage(
            QaStage::Security,
            "npm",
            &["audit", "--omit=dev"],
            project_root,
            timeout_secs,
        )
        .await,
    );

    results
}

// ============================================================================
// Go QA Runner
// ============================================================================

pub async fn run_go_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();

    // Syntax + TypeCheck: go build
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

    // Format: gofmt -l
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
    results.push(
        run_stage(
            QaStage::Test,
            "go",
            &["test", "-v", "./..."],
            project_root,
            timeout_secs * 2,
        )
        .await,
    );

    // Security: govulncheck (if available)
    if let Some(sec) = try_run_stage(
        QaStage::Security,
        "govulncheck",
        &["./..."],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(sec);
    }

    results
}

#[cfg(test)]
#[path = "../../tests/unit/testing/language_qa/language_qa_test.rs"]
mod tests;
