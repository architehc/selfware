use super::analyzer::ErrorAnalyzer;
use super::Tool;
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tracing::instrument;

/// Resolve the `cargo` binary. Prefers PATH, but falls back to the standard
/// rustup location (`~/.cargo/bin/cargo`) so the cargo tools work even when the
/// agent process was launched without cargo on PATH (observed: cargo_check
/// returned "No such file or directory" and the model then looped trying to
/// verify via full-path shell commands that weren't credited as verification).
fn cargo_program() -> std::path::PathBuf {
    let exe = if cfg!(windows) { "cargo.exe" } else { "cargo" };
    if let Some(path) = std::env::var_os("PATH") {
        if std::env::split_paths(&path).any(|dir| dir.join(exe).is_file()) {
            return std::path::PathBuf::from("cargo");
        }
    }
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(".cargo").join("bin").join(exe);
        if candidate.is_file() {
            return candidate;
        }
    }
    std::path::PathBuf::from("cargo")
}

/// Maximum output buffer size from a cargo command (16 MB).
/// Prevents a runaway cargo process from consuming unlimited memory.
const MAX_CARGO_OUTPUT_SIZE: usize = 16 * 1024 * 1024;

/// Timeout for cargo commands to prevent indefinite hangs in TUI mode.
/// This prevents the TUI from getting stuck when cargo commands take too long.
const CARGO_TIMEOUT_SECS: u64 = 300; // 5 minutes

/// Truncate a byte buffer to a safe maximum size, returning a lossy UTF-8 string.
/// Truncation happens at a valid UTF-8 boundary to avoid partial characters.
fn safe_truncate_output(bytes: &[u8], max_size: usize) -> String {
    if bytes.len() <= max_size {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let truncated = String::from_utf8_lossy(&bytes[..max_size]).into_owned();
    format!(
        "{}\n[OUTPUT TRUNCATED: {} bytes total, showing first {}]",
        truncated,
        bytes.len(),
        max_size
    )
}

pub struct CargoTest;
pub struct CargoCheck;
pub struct CargoClippy;
pub struct CargoFmt;

/// Represents a single test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub status: TestStatus,
    pub duration_ms: Option<u64>,
    pub failure_message: Option<String>,
    pub failure_location: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Ignored,
}

/// Structured output from cargo test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoTestOutput {
    pub success: bool,
    pub summary: TestSummary,
    pub tests: Vec<TestResult>,
    pub failures: Vec<FailureDetail>,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureDetail {
    pub test_name: String,
    pub message: String,
    pub location: Option<String>,
    pub stdout: Option<String>,
}

/// Represents a compiler error or warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompilerError {
    pub code: Option<String>,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
    pub suggestion: Option<String>,
    pub severity: Severity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
    Help,
}

/// Structured output from cargo check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoCheckOutput {
    pub success: bool,
    pub errors: Vec<CompilerError>,
    pub warnings: Vec<CompilerError>,
    pub by_file: HashMap<String, Vec<CompilerError>>,
    pub first_error: Option<CompilerError>,
    pub error_count: usize,
    pub warning_count: usize,
    pub output: String,
    pub exit_code: Option<i32>,
}

/// Represents a clippy lint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClippyLint {
    pub name: String,
    pub message: String,
    pub file: String,
    pub line: u32,
    pub severity: LintLevel,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LintLevel {
    Allow,
    Warn,
    Deny,
    Forbid,
}

/// Structured output from cargo clippy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CargoClippyOutput {
    pub success: bool,
    pub lints: Vec<ClippyLint>,
    pub by_category: HashMap<String, usize>,
    pub fixable: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub output: String,
}

#[async_trait]
impl Tool for CargoTest {
    fn name(&self) -> &str {
        "cargo_test"
    }

    fn description(&self) -> &str {
        "Run cargo test with structured output parsing. Returns detailed test results including pass/fail status, failure messages, and locations."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "package": {"type": "string", "description": "Specific package to test"},
                "test_name": {"type": "string", "description": "Specific test to run (substring match)"},
                "release": {"type": "boolean", "default": false, "description": "Run tests in release mode"},
                "no_fail_fast": {"type": "boolean", "default": true, "description": "Run all tests even if some fail"}
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new(cargo_program());
        cmd.arg("test");

        if let Some(pkg) = args.get("package").and_then(|v| v.as_str()) {
            cmd.arg("-p").arg(pkg);
        }

        if let Some(name) = args.get("test_name").and_then(|v| v.as_str()) {
            cmd.arg(name);
        }

        if args
            .get("release")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("--release");
        }

        if args
            .get("no_fail_fast")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            cmd.arg("--no-fail-fast");
        }

        cmd.env("RUST_BACKTRACE", "1");
        cmd.kill_on_drop(true);

        let timeout_duration = Duration::from_secs(CARGO_TIMEOUT_SECS);
        let output_result = tokio::time::timeout(timeout_duration, cmd.output()).await;

        let output = match output_result {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => anyhow::bail!("Failed to execute cargo test: {}", e),
            Err(_) => {
                anyhow::bail!("cargo test timed out after {} seconds", CARGO_TIMEOUT_SECS)
            }
        };

        let stdout = safe_truncate_output(&output.stdout, MAX_CARGO_OUTPUT_SIZE);
        let stderr = safe_truncate_output(&output.stderr, MAX_CARGO_OUTPUT_SIZE);

        // Parse test results from output
        let (tests, failures) = parse_test_output(&stdout, &stderr);

        let passed = tests
            .iter()
            .filter(|t| t.status == TestStatus::Passed)
            .count();
        let failed = tests
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .count();
        let ignored = tests
            .iter()
            .filter(|t| t.status == TestStatus::Ignored)
            .count();

        let result = CargoTestOutput {
            success: output.status.success() && failed == 0,
            summary: TestSummary {
                passed,
                failed,
                ignored,
                total: tests.len(),
            },
            tests,
            failures,
            stdout: stdout.chars().take(8000).collect(),
            stderr: stderr.chars().take(4000).collect(),
            exit_code: output.status.code(),
        };

        Ok(serde_json::to_value(result)?)
    }
}

#[async_trait]
impl Tool for CargoCheck {
    fn name(&self) -> &str {
        "cargo_check"
    }

    fn description(&self) -> &str {
        "Run cargo check with structured error parsing. Returns detailed compiler errors with file locations, error codes, and suggestions."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "all_targets": {"type": "boolean", "default": true, "description": "Check all targets including tests"},
                "all_features": {"type": "boolean", "default": true, "description": "Check with all features enabled"},
                "release": {"type": "boolean", "default": false}
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new(cargo_program());
        cmd.arg("check");
        cmd.arg("--message-format=json");
        cmd.kill_on_drop(true);

        if args
            .get("all_targets")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            cmd.arg("--all-targets");
        }

        if args
            .get("all_features")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            cmd.arg("--all-features");
        }

        if args
            .get("release")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            cmd.arg("--release");
        }

        let timeout_duration = Duration::from_secs(CARGO_TIMEOUT_SECS);
        let output_result = tokio::time::timeout(timeout_duration, cmd.output()).await;

        let output = match output_result {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => anyhow::bail!("Failed to execute cargo check: {}", e),
            Err(_) => {
                anyhow::bail!("cargo check timed out after {} seconds", CARGO_TIMEOUT_SECS)
            }
        };

        let stdout = safe_truncate_output(&output.stdout, MAX_CARGO_OUTPUT_SIZE);
        let stderr = safe_truncate_output(&output.stderr, MAX_CARGO_OUTPUT_SIZE);

        // Parse JSON messages from stdout
        let (mut errors, warnings) = parse_cargo_json_messages(&stdout);

        // Enrich errors with fix suggestions from ErrorAnalyzer
        for error in &mut errors {
            if error.suggestion.is_none() {
                error.suggestion = ErrorAnalyzer::suggest_fix(error);
            }
        }

        // Group by file
        let mut by_file: HashMap<String, Vec<CompilerError>> = HashMap::new();
        for error in errors.iter().chain(warnings.iter()) {
            by_file
                .entry(error.file.clone())
                .or_default()
                .push(error.clone());
        }

        let first_error = errors.first().cloned();

        let result = CargoCheckOutput {
            success: output.status.success(),
            error_count: errors.len(),
            warning_count: warnings.len(),
            errors,
            warnings,
            by_file,
            first_error,
            output: stderr.chars().take(6000).collect(),
            exit_code: output.status.code(),
        };

        // Add error analysis summary to the output
        let mut result_value = serde_json::to_value(&result)?;
        if !result.errors.is_empty() {
            let category_summary = ErrorAnalyzer::summarize_by_category(&result.errors);
            let most_actionable =
                ErrorAnalyzer::most_actionable(&result.errors).map(|e| e.message.clone());
            result_value["analysis"] = serde_json::json!({
                "most_actionable": most_actionable,
                "by_category": category_summary,
            });
        }

        Ok(result_value)
    }
}

#[async_trait]
impl Tool for CargoClippy {
    fn name(&self) -> &str {
        "cargo_clippy"
    }

    fn description(&self) -> &str {
        "Run cargo clippy with structured lint parsing. Returns categorized lints with severity levels and fix suggestions."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "all_targets": {"type": "boolean", "default": true},
                "fix": {"type": "boolean", "default": false, "description": "Automatically apply safe fixes"},
                "deny_warnings": {"type": "boolean", "default": true}
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new(cargo_program());
        cmd.arg("clippy");
        cmd.arg("--message-format=json");
        cmd.kill_on_drop(true);

        if args
            .get("all_targets")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            cmd.arg("--all-targets");
        }

        if args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--fix").arg("--allow-staged").arg("--allow-dirty");
        }

        let mut lint_args: Vec<&str> = Vec::new();
        if args
            .get("deny_warnings")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            lint_args.extend(["-D", "warnings"]);
        }
        lint_args.extend(["-D", "clippy::unwrap_used", "-D", "clippy::expect_used"]);

        cmd.arg("--").args(lint_args);

        let timeout_duration = Duration::from_secs(CARGO_TIMEOUT_SECS);
        let output_result = tokio::time::timeout(timeout_duration, cmd.output()).await;

        let output = match output_result {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => anyhow::bail!("Failed to execute cargo clippy: {}", e),
            Err(_) => {
                anyhow::bail!(
                    "cargo clippy timed out after {} seconds",
                    CARGO_TIMEOUT_SECS
                )
            }
        };

        let stdout = safe_truncate_output(&output.stdout, MAX_CARGO_OUTPUT_SIZE);
        let stderr = safe_truncate_output(&output.stderr, MAX_CARGO_OUTPUT_SIZE);

        // Parse clippy lints from JSON output
        let lints = parse_clippy_json_messages(&stdout);

        // Count by category
        let mut by_category: HashMap<String, usize> = HashMap::new();
        for lint in &lints {
            let category = lint
                .name
                .split("::")
                .next()
                .unwrap_or("unknown")
                .to_string();
            *by_category.entry(category).or_default() += 1;
        }

        let fixable = lints.iter().filter(|l| l.suggestion.is_some()).count();
        let error_count = lints
            .iter()
            .filter(|l| l.severity == LintLevel::Deny || l.severity == LintLevel::Forbid)
            .count();
        let warning_count = lints
            .iter()
            .filter(|l| l.severity == LintLevel::Warn)
            .count();

        let result = CargoClippyOutput {
            success: output.status.success(),
            lints,
            by_category,
            fixable,
            error_count,
            warning_count,
            output: stderr.chars().take(6000).collect(),
        };

        Ok(serde_json::to_value(result)?)
    }
}

#[async_trait]
impl Tool for CargoFmt {
    fn name(&self) -> &str {
        "cargo_fmt"
    }

    fn description(&self) -> &str {
        "Run cargo fmt to format code. Use --check to verify formatting without changing."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "check": {"type": "boolean", "default": false, "description": "Check formatting without modifying"},
                "all": {"type": "boolean", "default": true, "description": "Format all targets"}
            }
        })
    }

    #[instrument(level = "info", skip(self, args), fields(tool_name = self.name()))]
    async fn execute(&self, args: Value) -> Result<Value> {
        let mut cmd = tokio::process::Command::new(cargo_program());
        cmd.arg("fmt");
        cmd.kill_on_drop(true);

        if args.get("all").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all");
        }

        if args.get("check").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--").arg("--check");
        }

        let timeout_duration = Duration::from_secs(CARGO_TIMEOUT_SECS);
        let output_result = tokio::time::timeout(timeout_duration, cmd.output()).await;

        let output = match output_result {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => anyhow::bail!("Failed to execute cargo fmt: {}", e),
            Err(_) => {
                anyhow::bail!("cargo fmt timed out after {} seconds", CARGO_TIMEOUT_SECS)
            }
        };

        Ok(serde_json::json!({
            "success": output.status.success(),
            "diff": String::from_utf8_lossy(&output.stderr).into_owned(),
            "exit_code": output.status.code()
        }))
    }
}

/// Parse test output into structured results
fn parse_test_output(stdout: &str, stderr: &str) -> (Vec<TestResult>, Vec<FailureDetail>) {
    let mut tests = Vec::new();
    let mut failures = Vec::new();
    let mut current_failure: Option<FailureDetail> = None;
    let mut in_failure_block = false;
    let mut failure_output = String::new();

    // Combine stdout and stderr for parsing
    let combined = format!("{}\n{}", stdout, stderr);

    for line in combined.lines() {
        // Parse test results: "test module::test_name ... ok"
        if line.starts_with("test ")
            && (line.contains(" ... ok")
                || line.contains(" ... FAILED")
                || line.contains(" ... ignored"))
        {
            let parts: Vec<&str> = line.split(" ... ").collect();
            if parts.len() >= 2 {
                let name = parts[0]
                    .strip_prefix("test ")
                    .unwrap_or(parts[0])
                    .to_string();
                let status = if parts[1].contains("ok") {
                    TestStatus::Passed
                } else if parts[1].contains("FAILED") {
                    TestStatus::Failed
                } else {
                    TestStatus::Ignored
                };

                tests.push(TestResult {
                    name: name.clone(),
                    status: status.clone(),
                    duration_ms: None,
                    failure_message: None,
                    failure_location: None,
                });

                if status == TestStatus::Failed {
                    current_failure = Some(FailureDetail {
                        test_name: name,
                        message: String::new(),
                        location: None,
                        stdout: None,
                    });
                }
            }
        }

        // Detect failure block start
        if line.contains("---- ") && line.contains(" stdout ----") {
            in_failure_block = true;
            failure_output.clear();
            continue;
        }

        // Collect failure output
        if in_failure_block {
            if line.starts_with("----") {
                in_failure_block = false;
                if let Some(ref mut failure) = current_failure {
                    failure.stdout = Some(failure_output.clone());
                    // Extract panic message
                    if let Some(panic_line) =
                        failure_output.lines().find(|l| l.contains("panicked at"))
                    {
                        failure.message = panic_line.to_string();
                        // Try to extract location
                        if let Some(loc_start) = panic_line.find('\'') {
                            if let Some(loc_end) = panic_line.rfind('\'') {
                                failure.location =
                                    Some(panic_line[loc_start + 1..loc_end].to_string());
                            }
                        }
                    }
                    failures.push(failure.clone());
                    current_failure = None;
                }
            } else {
                failure_output.push_str(line);
                failure_output.push('\n');
            }
        }
    }

    // Handle any remaining failure
    if let Some(mut failure) = current_failure {
        if !failure_output.is_empty() {
            failure.stdout = Some(failure_output);
        }
        failures.push(failure);
    }

    (tests, failures)
}

/// Parse cargo JSON messages into compiler errors and warnings
/// This function is public to allow reuse by the verification module
pub fn parse_cargo_json_messages(output: &str) -> (Vec<CompilerError>, Vec<CompilerError>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        // Try to parse as JSON
        if let Ok(json) = serde_json::from_str::<Value>(line) {
            // Look for compiler messages
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                if let Some(message) = json.get("message") {
                    if let Some(error) = parse_compiler_message(message) {
                        match error.severity {
                            Severity::Error => errors.push(error),
                            Severity::Warning => warnings.push(error),
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    (errors, warnings)
}

/// Parse a single compiler message into a CompilerError
fn parse_compiler_message(message: &Value) -> Option<CompilerError> {
    let level = message.get("level")?.as_str()?;
    let msg = message.get("message")?.as_str()?;

    let severity = match level {
        "error" => Severity::Error,
        "warning" => Severity::Warning,
        "note" => Severity::Note,
        "help" => Severity::Help,
        _ => return None,
    };

    // Get code if present
    let code = message
        .get("code")
        .and_then(|c| c.get("code"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string());

    // Get primary span
    let spans = message.get("spans")?.as_array()?;
    let primary_span = spans.iter().find(|s| {
        s.get("is_primary")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
    });

    let (file, line, column, snippet) = if let Some(span) = primary_span {
        let file = span
            .get("file_name")
            .and_then(|f| f.as_str())
            .unwrap_or("")
            .to_string();
        let line = span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
        let column = span
            .get("column_start")
            .and_then(|c| c.as_u64())
            .unwrap_or(0) as u32;
        let snippet = span
            .get("text")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.first())
            .and_then(|t| t.get("text"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        (file, line, column, snippet)
    } else {
        (String::new(), 0, 0, String::new())
    };

    // Get suggestion if available
    let suggestion = message
        .get("children")
        .and_then(|c| c.as_array())
        .and_then(|children| {
            children.iter().find_map(|child| {
                if child.get("level").and_then(|l| l.as_str()) == Some("help") {
                    child
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        });

    Some(CompilerError {
        code,
        message: msg.to_string(),
        file,
        line,
        column,
        snippet,
        suggestion,
        severity,
    })
}

/// Parse clippy JSON messages into lints
fn parse_clippy_json_messages(output: &str) -> Vec<ClippyLint> {
    let mut lints = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<Value>(line) {
            if json.get("reason").and_then(|r| r.as_str()) == Some("compiler-message") {
                if let Some(message) = json.get("message") {
                    if let Some(lint) = parse_clippy_lint(message) {
                        lints.push(lint);
                    }
                }
            }
        }
    }

    lints
}

/// Parse a clippy message into a ClippyLint
fn parse_clippy_lint(message: &Value) -> Option<ClippyLint> {
    let level = message.get("level")?.as_str()?;
    let msg = message.get("message")?.as_str()?;

    // Get lint name from code
    let lint_name = message
        .get("code")
        .and_then(|c| c.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Skip non-clippy messages
    if !lint_name.starts_with("clippy::") && level != "error" && level != "warning" {
        return None;
    }

    let severity = match level {
        "deny" | "error" => LintLevel::Deny,
        "forbid" => LintLevel::Forbid,
        "warn" | "warning" => LintLevel::Warn,
        _ => LintLevel::Allow,
    };

    // Get location
    let spans = message.get("spans")?.as_array()?;
    let primary_span = spans.iter().find(|s| {
        s.get("is_primary")
            .and_then(|p| p.as_bool())
            .unwrap_or(false)
    });

    let (file, line) = if let Some(span) = primary_span {
        let file = span
            .get("file_name")
            .and_then(|f| f.as_str())
            .unwrap_or("")
            .to_string();
        let line = span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(0) as u32;
        (file, line)
    } else {
        (String::new(), 0)
    };

    // Get suggestion
    let suggestion = message
        .get("children")
        .and_then(|c| c.as_array())
        .and_then(|children| {
            children.iter().find_map(|child| {
                if child.get("level").and_then(|l| l.as_str()) == Some("help") {
                    child
                        .get("message")
                        .and_then(|m| m.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
        });

    Some(ClippyLint {
        name: lint_name,
        message: msg.to_string(),
        file,
        line,
        severity,
        suggestion,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/tools/cargo/cargo_test.rs"]
mod tests;
