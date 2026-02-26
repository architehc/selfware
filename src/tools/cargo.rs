use super::analyzer::ErrorAnalyzer;
use super::Tool;
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::instrument;

/// Maximum output buffer size from a cargo command (16 MB).
/// Prevents a runaway cargo process from consuming unlimited memory.
const MAX_CARGO_OUTPUT_SIZE: usize = 16 * 1024 * 1024;

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
        let mut cmd = tokio::process::Command::new("cargo");
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

        let output = cmd.output().await.context("Failed to execute cargo test")?;
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
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("check");
        cmd.arg("--message-format=json");

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

        let output = cmd
            .output()
            .await
            .context("Failed to execute cargo check")?;
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
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("clippy");
        cmd.arg("--message-format=json");

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

        if args
            .get("deny_warnings")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            cmd.arg("--").arg("-D").arg("warnings");
        }

        cmd.args([
            "--",
            "-D",
            "clippy::unwrap_used",
            "-D",
            "clippy::expect_used",
        ]);

        let output = cmd
            .output()
            .await
            .context("Failed to execute cargo clippy")?;
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
        let mut cmd = tokio::process::Command::new("cargo");
        cmd.arg("fmt");

        if args.get("all").and_then(|v| v.as_bool()).unwrap_or(true) {
            cmd.arg("--all");
        }

        if args.get("check").and_then(|v| v.as_bool()).unwrap_or(false) {
            cmd.arg("--").arg("--check");
        }

        let output = cmd.output().await.context("Failed to execute cargo fmt")?;

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
mod tests {
    use super::*;

    #[test]
    fn test_cargo_test_name() {
        let tool = CargoTest;
        assert_eq!(tool.name(), "cargo_test");
    }

    #[test]
    fn test_cargo_test_description() {
        let tool = CargoTest;
        assert!(tool.description().contains("test"));
    }

    #[test]
    fn test_cargo_test_schema() {
        let tool = CargoTest;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["package"].is_object());
        assert!(schema["properties"]["test_name"].is_object());
    }

    #[test]
    fn test_cargo_check_name() {
        let tool = CargoCheck;
        assert_eq!(tool.name(), "cargo_check");
    }

    #[test]
    fn test_cargo_check_description() {
        let tool = CargoCheck;
        assert!(tool.description().contains("check"));
    }

    #[test]
    fn test_cargo_check_schema() {
        let tool = CargoCheck;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["all_targets"].is_object());
        assert!(schema["properties"]["all_features"].is_object());
    }

    #[test]
    fn test_cargo_clippy_name() {
        let tool = CargoClippy;
        assert_eq!(tool.name(), "cargo_clippy");
    }

    #[test]
    fn test_cargo_clippy_description() {
        let tool = CargoClippy;
        assert!(tool.description().contains("clippy"));
    }

    #[test]
    fn test_cargo_clippy_schema() {
        let tool = CargoClippy;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["fix"].is_object());
        assert!(schema["properties"]["deny_warnings"].is_object());
    }

    #[test]
    fn test_cargo_fmt_name() {
        let tool = CargoFmt;
        assert_eq!(tool.name(), "cargo_fmt");
    }

    #[test]
    fn test_cargo_fmt_description() {
        let tool = CargoFmt;
        assert!(tool.description().contains("fmt"));
    }

    #[test]
    fn test_cargo_fmt_schema() {
        let tool = CargoFmt;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["check"].is_object());
        assert!(schema["properties"]["all"].is_object());
    }

    #[test]
    fn test_parse_test_output_basic() {
        let stdout = "test tests::test_basic ... ok\ntest tests::test_fail ... FAILED\ntest tests::test_ignore ... ignored";
        let (tests, _failures) = parse_test_output(stdout, "");

        assert_eq!(tests.len(), 3);
        assert_eq!(tests[0].status, TestStatus::Passed);
        assert_eq!(tests[1].status, TestStatus::Failed);
        assert_eq!(tests[2].status, TestStatus::Ignored);
    }

    #[test]
    fn test_parse_test_output_with_failure() {
        let stdout = r#"
test tests::test_fail ... FAILED

---- tests::test_fail stdout ----
thread 'tests::test_fail' panicked at 'assertion failed', src/lib.rs:10:5
----
"#;
        let (tests, failures) = parse_test_output(stdout, "");

        assert_eq!(tests.len(), 1);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message.contains("panicked"));
    }

    #[test]
    fn test_parse_compiler_message() {
        let json = serde_json::json!({
            "level": "error",
            "message": "cannot find value `foo` in this scope",
            "code": {"code": "E0425"},
            "spans": [{
                "file_name": "src/main.rs",
                "line_start": 10,
                "column_start": 5,
                "is_primary": true,
                "text": [{"text": "    foo;"}]
            }],
            "children": [{
                "level": "help",
                "message": "consider using `bar` instead"
            }]
        });

        let error = parse_compiler_message(&json).unwrap();
        assert_eq!(error.code, Some("E0425".to_string()));
        assert_eq!(error.severity, Severity::Error);
        assert_eq!(error.file, "src/main.rs");
        assert_eq!(error.line, 10);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_test_status_serde() {
        let passed = TestStatus::Passed;
        let json = serde_json::to_string(&passed).unwrap();
        assert_eq!(json, "\"passed\"");

        let parsed: TestStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, TestStatus::Passed);
    }

    #[test]
    fn test_severity_serde() {
        let error = Severity::Error;
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, "\"error\"");

        let warning = Severity::Warning;
        let json = serde_json::to_string(&warning).unwrap();
        assert_eq!(json, "\"warning\"");
    }

    #[test]
    fn test_lint_level_serde() {
        let deny = LintLevel::Deny;
        let json = serde_json::to_string(&deny).unwrap();
        assert_eq!(json, "\"deny\"");

        let warn = LintLevel::Warn;
        let json = serde_json::to_string(&warn).unwrap();
        assert_eq!(json, "\"warn\"");
    }

    #[test]
    fn test_parse_test_output_empty() {
        let (tests, failures) = parse_test_output("", "");
        assert!(tests.is_empty());
        assert!(failures.is_empty());
    }

    #[test]
    fn test_parse_test_output_only_passed() {
        let stdout = "test foo::bar ... ok\ntest baz::qux ... ok";
        let (tests, failures) = parse_test_output(stdout, "");
        assert_eq!(tests.len(), 2);
        assert!(tests.iter().all(|t| t.status == TestStatus::Passed));
        assert!(failures.is_empty());
    }

    #[test]
    fn test_parse_test_output_only_ignored() {
        let stdout = "test skip_me ... ignored\ntest skip_too ... ignored";
        let (tests, _) = parse_test_output(stdout, "");
        assert_eq!(tests.len(), 2);
        assert!(tests.iter().all(|t| t.status == TestStatus::Ignored));
    }

    #[test]
    fn test_parse_compiler_message_warning() {
        let json = serde_json::json!({
            "level": "warning",
            "message": "unused variable `x`",
            "code": {"code": "unused_variables"},
            "spans": [{
                "file_name": "src/lib.rs",
                "line_start": 5,
                "column_start": 9,
                "is_primary": true,
                "text": [{"text": "    let x = 1;"}]
            }],
            "children": []
        });

        let error = parse_compiler_message(&json).unwrap();
        assert_eq!(error.severity, Severity::Warning);
        assert_eq!(error.line, 5);
    }

    #[test]
    fn test_parse_compiler_message_no_span() {
        let json = serde_json::json!({
            "level": "note",
            "message": "some note",
            "spans": [],
            "children": []
        });

        let error = parse_compiler_message(&json);
        assert!(error.is_some());
        assert_eq!(error.unwrap().severity, Severity::Note);
    }

    #[test]
    fn test_parse_compiler_message_help_level() {
        let json = serde_json::json!({
            "level": "help",
            "message": "try this instead",
            "spans": [],
            "children": []
        });

        let error = parse_compiler_message(&json);
        assert!(error.is_some());
        assert_eq!(error.unwrap().severity, Severity::Help);
    }

    #[test]
    fn test_parse_compiler_message_unknown_level() {
        let json = serde_json::json!({
            "level": "unknown_level",
            "message": "something",
            "spans": [],
            "children": []
        });

        let error = parse_compiler_message(&json);
        assert!(error.is_none());
    }

    #[test]
    fn test_parse_cargo_json_messages_empty() {
        let (errors, warnings) = parse_cargo_json_messages("");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_messages_non_json() {
        let (errors, warnings) = parse_cargo_json_messages("this is not json\nneither is this");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_messages_with_error() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"error","message":"test error","code":{"code":"E0001"},"spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true,"text":[]}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_messages(json_line);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_messages_with_warning() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"test warning","code":{"code":"W0001"},"spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true,"text":[]}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_messages(json_line);
        assert!(errors.is_empty());
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn test_parse_clippy_json_messages_empty() {
        let lints = parse_clippy_json_messages("");
        assert!(lints.is_empty());
    }

    #[test]
    fn test_test_result_struct() {
        let result = TestResult {
            name: "test_foo".to_string(),
            status: TestStatus::Passed,
            duration_ms: Some(100),
            failure_message: None,
            failure_location: None,
        };
        assert_eq!(result.name, "test_foo");
        assert!(result.duration_ms.is_some());
    }

    #[test]
    fn test_failure_detail_struct() {
        let detail = FailureDetail {
            test_name: "test_bar".to_string(),
            message: "assertion failed".to_string(),
            location: Some("src/lib.rs:10".to_string()),
            stdout: Some("output".to_string()),
        };
        assert_eq!(detail.test_name, "test_bar");
        assert!(detail.location.is_some());
    }

    #[test]
    fn test_compiler_error_struct() {
        let error = CompilerError {
            code: Some("E0001".to_string()),
            message: "error message".to_string(),
            file: "src/main.rs".to_string(),
            line: 10,
            column: 5,
            snippet: "let x = 1;".to_string(),
            suggestion: Some("try this".to_string()),
            severity: Severity::Error,
        };
        assert_eq!(error.code, Some("E0001".to_string()));
        assert_eq!(error.line, 10);
    }

    #[test]
    fn test_clippy_lint_struct() {
        let lint = ClippyLint {
            name: "clippy::unwrap_used".to_string(),
            message: "used unwrap".to_string(),
            file: "src/lib.rs".to_string(),
            line: 20,
            severity: LintLevel::Warn,
            suggestion: Some("use expect instead".to_string()),
        };
        assert!(lint.name.starts_with("clippy::"));
    }

    #[test]
    fn test_test_summary_struct() {
        let summary = TestSummary {
            passed: 10,
            failed: 2,
            ignored: 3,
            total: 15,
        };
        assert_eq!(
            summary.passed + summary.failed + summary.ignored,
            summary.total
        );
    }

    #[test]
    fn test_cargo_test_output_struct() {
        let output = CargoTestOutput {
            success: true,
            summary: TestSummary {
                passed: 5,
                failed: 0,
                ignored: 1,
                total: 6,
            },
            tests: vec![],
            failures: vec![],
            stdout: "output".to_string(),
            stderr: "".to_string(),
            exit_code: Some(0),
        };
        assert!(output.success);
        assert_eq!(output.summary.total, 6);
    }

    #[test]
    fn test_cargo_check_output_struct() {
        let output = CargoCheckOutput {
            success: true,
            errors: vec![],
            warnings: vec![],
            by_file: HashMap::new(),
            first_error: None,
            error_count: 0,
            warning_count: 0,
            output: "".to_string(),
            exit_code: Some(0),
        };
        assert!(output.success);
        assert!(output.first_error.is_none());
    }

    #[test]
    fn test_cargo_clippy_output_struct() {
        let output = CargoClippyOutput {
            success: true,
            lints: vec![],
            by_category: HashMap::new(),
            fixable: 0,
            error_count: 0,
            warning_count: 0,
            output: "".to_string(),
        };
        assert!(output.success);
        assert_eq!(output.fixable, 0);
    }

    #[test]
    fn test_severity_note_serde() {
        let note = Severity::Note;
        let json = serde_json::to_string(&note).unwrap();
        assert_eq!(json, "\"note\"");
    }

    #[test]
    fn test_severity_help_serde() {
        let help = Severity::Help;
        let json = serde_json::to_string(&help).unwrap();
        assert_eq!(json, "\"help\"");
    }

    #[test]
    fn test_lint_level_allow_serde() {
        let allow = LintLevel::Allow;
        let json = serde_json::to_string(&allow).unwrap();
        assert_eq!(json, "\"allow\"");
    }

    #[test]
    fn test_lint_level_forbid_serde() {
        let forbid = LintLevel::Forbid;
        let json = serde_json::to_string(&forbid).unwrap();
        assert_eq!(json, "\"forbid\"");
    }

    #[test]
    fn test_test_status_failed_serde() {
        let failed = TestStatus::Failed;
        let json = serde_json::to_string(&failed).unwrap();
        assert_eq!(json, "\"failed\"");
    }

    #[test]
    fn test_test_status_ignored_serde() {
        let ignored = TestStatus::Ignored;
        let json = serde_json::to_string(&ignored).unwrap();
        assert_eq!(json, "\"ignored\"");
    }

    // Additional parsing tests for improved coverage

    #[test]
    fn test_parse_test_output_basic_pass() {
        let stdout = "running 2 tests\ntest test_one ... ok\ntest test_two ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored";
        let (tests, failures) = parse_test_output(stdout, "");

        assert_eq!(tests.len(), 2);
        assert!(tests.iter().all(|t| t.status == TestStatus::Passed));
        assert!(failures.is_empty());
    }

    #[test]
    fn test_parse_test_output_with_failure_detailed() {
        // The parser needs a closing "----" line to end the failure block
        let stdout = r#"running 1 test
test test_failing ... FAILED

failures:

---- test_failing stdout ----
thread 'test_failing' panicked at 'assertion failed', src/lib.rs:10:5
---- end ----

failures:
    test_failing

test result: FAILED. 0 passed; 1 failed; 0 ignored"#;

        let (tests, failures) = parse_test_output(stdout, "");

        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].status, TestStatus::Failed);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message.contains("panicked"));
    }

    #[test]
    fn test_parse_test_output_with_ignored() {
        let stdout = "running 1 test\ntest test_ignored ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored";
        let (tests, _) = parse_test_output(stdout, "");

        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].status, TestStatus::Ignored);
    }

    #[test]
    fn test_parse_test_output_empty_input() {
        let (tests, failures) = parse_test_output("", "");
        assert!(tests.is_empty());
        assert!(failures.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_empty() {
        let (errors, warnings) = parse_cargo_json_messages("");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_invalid_json() {
        let (errors, warnings) = parse_cargo_json_messages("not valid json\nalso invalid");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_mixed_content() {
        // Lines that aren't compiler messages should be skipped
        let mixed = r#"
{"reason":"compiler-artifact","target":{"name":"test"}}
{"reason":"build-script-executed"}
"#;
        let (errors, warnings) = parse_cargo_json_messages(mixed);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_compiler_message_complete() {
        let message = serde_json::json!({
            "level": "error",
            "message": "test error message",
            "code": {"code": "E0001"},
            "spans": [{
                "file_name": "src/main.rs",
                "line_start": 10,
                "column_start": 5,
                "is_primary": true,
                "text": [{"text": "let x = 1;"}]
            }],
            "rendered": "error[E0001]: test error\n --> src/main.rs:10:5"
        });

        let error = parse_compiler_message(&message).unwrap();
        assert_eq!(error.code, Some("E0001".to_string()));
        assert_eq!(error.message, "test error message");
        assert_eq!(error.file, "src/main.rs");
        assert_eq!(error.line, 10);
    }

    #[test]
    fn test_parse_compiler_message_no_primary_span() {
        let message = serde_json::json!({
            "level": "error",
            "message": "general error",
            "spans": []
        });

        let error = parse_compiler_message(&message);
        // Messages without primary spans return empty file/line
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.file, "");
        assert_eq!(err.line, 0);
    }

    #[test]
    fn test_parse_clippy_json_empty() {
        let lints = parse_clippy_json_messages("");
        assert!(lints.is_empty());
    }

    #[test]
    fn test_parse_clippy_json_invalid() {
        let lints = parse_clippy_json_messages("invalid json content");
        assert!(lints.is_empty());
    }

    #[test]
    fn test_parse_clippy_lint_complete() {
        let message = serde_json::json!({
            "code": {"code": "clippy::unwrap_used"},
            "message": "used `unwrap()` on an `Option` value",
            "level": "warning",
            "spans": [{
                "file_name": "src/main.rs",
                "line_start": 15
            }],
            "rendered": "warning: used `unwrap()` on an `Option` value"
        });

        let lint = parse_clippy_lint(&message).unwrap();
        assert_eq!(lint.name, "clippy::unwrap_used");
        assert!(lint.message.contains("unwrap"));
    }

    #[test]
    fn test_compiler_error_severity_warning() {
        let error = CompilerError {
            code: None,
            message: "unused variable".to_string(),
            file: "src/lib.rs".to_string(),
            line: 5,
            column: 1,
            snippet: "let unused = 1;".to_string(),
            suggestion: Some("prefix with _".to_string()),
            severity: Severity::Warning,
        };

        assert_eq!(error.severity, Severity::Warning);
        assert!(error.suggestion.is_some());
    }

    #[test]
    fn test_clippy_lint_severity() {
        let lint = ClippyLint {
            name: "clippy::complexity".to_string(),
            message: "complex code".to_string(),
            file: "src/main.rs".to_string(),
            line: 20,
            severity: LintLevel::Warn,
            suggestion: None,
        };

        assert_eq!(lint.severity, LintLevel::Warn);
    }

    #[test]
    fn test_test_result_with_duration() {
        let result = TestResult {
            name: "test_with_timing".to_string(),
            status: TestStatus::Passed,
            duration_ms: Some(150),
            failure_message: None,
            failure_location: None,
        };

        assert_eq!(result.duration_ms, Some(150));
    }

    #[test]
    fn test_failure_detail_with_location() {
        let detail = FailureDetail {
            test_name: "failing_test".to_string(),
            message: "assertion failed".to_string(),
            location: Some("src/lib.rs:42".to_string()),
            stdout: None,
        };

        assert!(detail.location.is_some());
        assert!(detail.message.contains("assertion"));
    }

    #[test]
    fn test_test_summary_totals() {
        let summary = TestSummary {
            passed: 10,
            failed: 2,
            ignored: 1,
            total: 13,
        };

        assert_eq!(
            summary.passed + summary.failed + summary.ignored,
            summary.total
        );
    }

    #[test]
    fn test_cargo_test_output_with_failures() {
        let output = CargoTestOutput {
            success: false,
            summary: TestSummary {
                passed: 5,
                failed: 2,
                ignored: 0,
                total: 7,
            },
            tests: vec![TestResult {
                name: "test1".to_string(),
                status: TestStatus::Failed,
                duration_ms: None,
                failure_message: Some("assertion error".to_string()),
                failure_location: Some("src/lib.rs:10".to_string()),
            }],
            failures: vec![FailureDetail {
                test_name: "test1".to_string(),
                message: "assertion error".to_string(),
                location: Some("src/lib.rs:10".to_string()),
                stdout: None,
            }],
            stdout: "test output".to_string(),
            stderr: "".to_string(),
            exit_code: Some(101),
        };

        assert!(!output.success);
        assert_eq!(output.summary.failed, 2);
        assert_eq!(output.failures.len(), 1);
    }

    #[test]
    fn test_cargo_check_output_with_errors() {
        let error = CompilerError {
            code: Some("E0425".to_string()),
            message: "cannot find value".to_string(),
            file: "src/main.rs".to_string(),
            line: 10,
            column: 5,
            snippet: "let x = undefined;".to_string(),
            suggestion: None,
            severity: Severity::Error,
        };

        let mut by_file = HashMap::new();
        by_file.insert("src/main.rs".to_string(), vec![error.clone()]);

        let output = CargoCheckOutput {
            success: false,
            errors: vec![error.clone()],
            warnings: vec![],
            by_file,
            first_error: Some(error),
            error_count: 1,
            warning_count: 0,
            output: "error output".to_string(),
            exit_code: Some(101),
        };

        assert!(!output.success);
        assert_eq!(output.error_count, 1);
        assert!(output.first_error.is_some());
    }

    #[test]
    fn test_cargo_clippy_output_with_lints() {
        let lint = ClippyLint {
            name: "clippy::unwrap_used".to_string(),
            message: "used unwrap".to_string(),
            file: "src/main.rs".to_string(),
            line: 15,
            severity: LintLevel::Warn,
            suggestion: Some("use expect instead".to_string()),
        };

        let mut by_category = HashMap::new();
        by_category.insert("correctness".to_string(), 1usize);

        let output = CargoClippyOutput {
            success: true,
            lints: vec![lint],
            by_category,
            fixable: 1,
            error_count: 0,
            warning_count: 1,
            output: "clippy output".to_string(),
        };

        assert!(output.success);
        assert_eq!(output.warning_count, 1);
        assert_eq!(output.fixable, 1);
    }
}
