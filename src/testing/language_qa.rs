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
async fn run_stage(
    stage: QaStage,
    program: &str,
    args: &[&str],
    project_root: &Path,
    timeout_secs: u64,
) -> QaStageResult {
    let start = Instant::now();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs.max(5)),
        Command::new(program).args(args).current_dir(project_root).output(),
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(Ok(output)) => {
            let stdout =
                String::from_utf8_lossy(&output.stdout[..output.stdout.len().min(MAX_OUTPUT_BYTES)])
                    .to_string();
            let stderr =
                String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(MAX_OUTPUT_BYTES)])
                    .to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else {
                format!("{}\n{}", stdout, stderr)
            };

            let error_count = count_pattern(&combined, "error");
            let warning_count = count_pattern(&combined, "warning");

            QaStageResult {
                stage,
                passed: output.status.success(),
                duration_ms,
                output: combined,
                error_count,
                warning_count,
            }
        }
        Ok(Err(e)) => QaStageResult {
            stage,
            passed: false,
            duration_ms,
            output: format!("Failed to run {} {:?}: {}", program, args, e),
            error_count: 1,
            warning_count: 0,
        },
        Err(_) => QaStageResult {
            stage,
            passed: false,
            duration_ms,
            output: format!("{} {:?} timed out after {}s", program, args, timeout_secs),
            error_count: 1,
            warning_count: 0,
        },
    }
}

/// Rough count of a pattern in output (case-insensitive).
fn count_pattern(text: &str, pattern: &str) -> usize {
    text.to_lowercase()
        .matches(&pattern.to_lowercase())
        .count()
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
    let check = Command::new("which").arg(program).output().await;
    if check.is_err() || !check.unwrap().status.success() {
        debug!("{} not found, skipping {} stage", program, stage);
        return None;
    }
    Some(run_stage(stage, program, args, project_root, timeout_secs).await)
}

// ============================================================================
// Rust QA Runner
// ============================================================================

pub async fn run_rust_qa(project_root: &Path, timeout_secs: u64) -> Vec<QaStageResult> {
    let mut results = Vec::new();

    // Syntax: cargo check
    results.push(run_stage(QaStage::Syntax, "cargo", &["check", "--quiet"], project_root, timeout_secs).await);

    // Format: cargo fmt --check
    results.push(run_stage(QaStage::Format, "cargo", &["fmt", "--check"], project_root, timeout_secs).await);

    // Lint: cargo clippy
    results.push(
        run_stage(
            QaStage::Lint,
            "cargo",
            &["clippy", "--quiet", "--", "-D", "warnings"],
            project_root,
            timeout_secs,
        )
        .await,
    );

    // TypeCheck: same as syntax for Rust (cargo check covers both)
    // Skip duplicate — Syntax already covers it

    // Test: cargo test
    results.push(
        run_stage(
            QaStage::Test,
            "cargo",
            &["test", "--quiet"],
            project_root,
            timeout_secs * 2, // Tests get double timeout
        )
        .await,
    );

    // Security: cargo audit (if available)
    if let Some(audit) = try_run_stage(
        QaStage::Security,
        "cargo",
        &["audit"],
        project_root,
        timeout_secs,
    )
    .await
    {
        results.push(audit);
    }

    results
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
    } else if let Some(lint) = try_run_stage(
        QaStage::Lint,
        "flake8",
        &["."],
        project_root,
        timeout_secs,
    )
    .await
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

/// Dispatch QA to the appropriate language runner.
pub async fn run_qa(
    language: QaLanguage,
    project_root: &Path,
    timeout_secs: u64,
) -> Vec<QaStageResult> {
    match language {
        QaLanguage::Rust => run_rust_qa(project_root, timeout_secs).await,
        QaLanguage::Python => run_python_qa(project_root, timeout_secs).await,
        QaLanguage::Node => run_node_qa(project_root, timeout_secs).await,
        QaLanguage::Go => run_go_qa(project_root, timeout_secs).await,
        QaLanguage::Unknown => {
            warn!("Unknown project language, skipping QA");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
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
}
