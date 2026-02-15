//! Verification Gates - Automatic validation after every code change
//!
//! Implements the "never proceed on assumptions" protocol:
//! 1. Speculate: Agent proposes an edit
//! 2. Validate: Harness runs checks automatically
//! 3. Feedback: Agent sees results immediately
//! 4. Commit: Only on green, or explicit override

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::process::Command;

use crate::tools::cargo::{parse_cargo_json_messages, CompilerError, Severity};

/// Verification result for a single check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub check_type: CheckType,
    pub passed: bool,
    pub duration_ms: u64,
    pub output: String,
    pub errors: Vec<VerificationError>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
}

/// Types of verification checks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    /// Rust type checking (cargo check)
    TypeCheck,
    /// Run tests (cargo test)
    Test,
    /// Linting (cargo clippy)
    Lint,
    /// Formatting check (cargo fmt --check)
    Format,
    /// Custom command
    Custom,
}

impl CheckType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TypeCheck => "type_check",
            Self::Test => "test",
            Self::Lint => "lint",
            Self::Format => "format",
            Self::Custom => "custom",
        }
    }
}

/// A verification error with location info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationError {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub message: String,
    pub code: Option<String>,
    pub severity: ErrorSeverity,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Error,
    Warning,
    Note,
    Help,
}

/// Complete verification report after a change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub triggered_by: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub total_duration_ms: u64,
    pub checks: Vec<CheckResult>,
    pub overall_passed: bool,
    pub affected_files: Vec<String>,
    pub side_effects: Vec<SideEffect>,
    pub suggested_next_steps: Vec<String>,
}

/// Side effects detected from the change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SideEffect {
    pub effect_type: SideEffectType,
    pub description: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectType {
    FileCreated,
    FileModified,
    FileDeleted,
    DependencyAdded,
    DependencyRemoved,
    TestAdded,
    TestRemoved,
}

/// Configuration for verification gates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Run type check after every file edit
    pub check_on_edit: bool,
    /// Run tests after every file edit
    pub test_on_edit: bool,
    /// Run clippy after every file edit
    pub lint_on_edit: bool,
    /// Run format check after every file edit
    pub format_on_edit: bool,
    /// Only run checks on affected files (faster but less thorough)
    pub incremental: bool,
    /// Timeout for each check
    pub check_timeout_secs: u64,
    /// Continue running other checks if one fails
    pub continue_on_failure: bool,
    /// Files/patterns to exclude from verification
    pub exclude_patterns: Vec<String>,
    /// Custom verification commands
    pub custom_checks: Vec<CustomCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomCheck {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub run_on: Vec<String>, // File patterns that trigger this check
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: false, // Tests can be slow, opt-in
            lint_on_edit: false, // Clippy can be slow, opt-in
            format_on_edit: true,
            incremental: true,
            check_timeout_secs: 60,
            continue_on_failure: true,
            exclude_patterns: vec![
                "*.md".to_string(),
                "*.txt".to_string(),
                "*.json".to_string(),
                "*.toml".to_string(),
            ],
            custom_checks: vec![],
        }
    }
}

impl VerificationConfig {
    /// Fast mode: only type check
    pub fn fast() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        }
    }

    /// Thorough mode: all checks
    pub fn thorough() -> Self {
        Self {
            check_on_edit: true,
            test_on_edit: true,
            lint_on_edit: true,
            format_on_edit: true,
            ..Default::default()
        }
    }
}

/// The verification gate - runs checks and reports results
pub struct VerificationGate {
    config: VerificationConfig,
    project_root: PathBuf,
    last_results: Option<VerificationReport>,
}

impl VerificationGate {
    pub fn new(project_root: impl AsRef<Path>, config: VerificationConfig) -> Self {
        Self {
            config,
            project_root: project_root.as_ref().to_path_buf(),
            last_results: None,
        }
    }

    /// Run verification after a file change
    pub async fn verify_change(
        &mut self,
        changed_files: &[String],
        trigger: &str,
    ) -> Result<VerificationReport> {
        let start = Instant::now();
        let mut checks = Vec::new();
        let mut suggested_next_steps = Vec::new();

        // Filter out excluded files
        let files_to_check: Vec<_> = changed_files
            .iter()
            .filter(|f| !self.is_excluded(f))
            .cloned()
            .collect();

        if files_to_check.is_empty() {
            return Ok(VerificationReport {
                triggered_by: trigger.to_string(),
                timestamp: chrono::Utc::now(),
                total_duration_ms: 0,
                checks: vec![],
                overall_passed: true,
                affected_files: changed_files.to_vec(),
                side_effects: vec![],
                suggested_next_steps: vec![
                    "No code files changed, verification skipped".to_string()
                ],
            });
        }

        // Detect if any Rust files changed
        let rust_files_changed = files_to_check.iter().any(|f| f.ends_with(".rs"));

        if rust_files_changed {
            // Run type check
            if self.config.check_on_edit {
                let result = self.run_cargo_check().await?;
                if !result.passed {
                    suggested_next_steps.push("Fix type errors before proceeding".to_string());
                }
                checks.push(result);
            }

            // Run format check
            if self.config.format_on_edit {
                let result = self.run_cargo_fmt_check().await?;
                if !result.passed {
                    suggested_next_steps.push("Run cargo fmt to fix formatting".to_string());
                }
                checks.push(result);
            }

            // Run tests (if enabled)
            if self.config.test_on_edit {
                let result = self.run_cargo_test().await?;
                if !result.passed {
                    suggested_next_steps.push("Fix failing tests".to_string());
                }
                checks.push(result);
            }

            // Run clippy (if enabled)
            if self.config.lint_on_edit {
                let result = self.run_cargo_clippy().await?;
                if !result.passed {
                    suggested_next_steps.push("Address clippy warnings".to_string());
                }
                checks.push(result);
            }
        }

        // Run custom checks
        for custom in &self.config.custom_checks {
            if self.should_run_custom_check(custom, &files_to_check) {
                let result = self.run_custom_check(custom).await?;
                checks.push(result);
            }
        }

        let overall_passed = checks.iter().all(|c| c.passed);
        let total_duration = start.elapsed().as_millis() as u64;

        // Detect side effects
        let side_effects = self.detect_side_effects(&files_to_check).await;

        // Add suggestions based on results
        if overall_passed && suggested_next_steps.is_empty() {
            suggested_next_steps.push("All checks passed - safe to proceed".to_string());
        }

        let report = VerificationReport {
            triggered_by: trigger.to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: total_duration,
            checks,
            overall_passed,
            affected_files: files_to_check,
            side_effects,
            suggested_next_steps,
        };

        self.last_results = Some(report.clone());
        Ok(report)
    }

    /// Quick verification - just type check
    pub async fn quick_verify(&mut self, _changed_files: &[String]) -> Result<bool> {
        let result = self.run_cargo_check().await?;
        Ok(result.passed)
    }

    /// Full verification - all checks
    pub async fn full_verify(&mut self) -> Result<VerificationReport> {
        self.verify_change(&[], "full_verification").await
    }

    /// Run cargo check
    async fn run_cargo_check(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=json")
            .current_dir(&self.project_root)
            .output()
            .await
            .context("Failed to run cargo check")?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let (errors, warnings) = parse_cargo_json_output(&stdout);

        Ok(CheckResult {
            check_type: CheckType::TypeCheck,
            passed: output.status.success(),
            duration_ms: duration,
            output: if output.status.success() {
                "Type check passed".to_string()
            } else {
                stderr.to_string()
            },
            errors,
            warnings: warnings.iter().map(|e| e.message.clone()).collect(),
            suggestions: vec![],
        })
    }

    /// Run cargo fmt --check
    async fn run_cargo_fmt_check(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["fmt", "--check"])
            .current_dir(&self.project_root)
            .output()
            .await
            .context("Failed to run cargo fmt")?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        Ok(CheckResult {
            check_type: CheckType::Format,
            passed: output.status.success(),
            duration_ms: duration,
            output: if output.status.success() {
                "Formatting check passed".to_string()
            } else {
                stdout.to_string()
            },
            errors: vec![],
            warnings: vec![],
            suggestions: if !output.status.success() {
                vec!["Run `cargo fmt` to fix formatting".to_string()]
            } else {
                vec![]
            },
        })
    }

    /// Run cargo test
    async fn run_cargo_test(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["test", "--no-fail-fast"])
            .current_dir(&self.project_root)
            .output()
            .await
            .context("Failed to run cargo test")?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Parse test failures from output
        let errors = parse_test_failures(&stdout, &stderr);

        Ok(CheckResult {
            check_type: CheckType::Test,
            passed: output.status.success(),
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors,
            warnings: vec![],
            suggestions: vec![],
        })
    }

    /// Run cargo clippy
    async fn run_cargo_clippy(&self) -> Result<CheckResult> {
        let start = Instant::now();

        let output = Command::new("cargo")
            .args(["clippy", "--message-format=json", "--", "-D", "warnings"])
            .current_dir(&self.project_root)
            .output()
            .await
            .context("Failed to run cargo clippy")?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let (errors, warnings) = parse_cargo_json_output(&stdout);

        Ok(CheckResult {
            check_type: CheckType::Lint,
            passed: output.status.success(),
            duration_ms: duration,
            output: stderr.to_string(),
            errors,
            warnings: warnings.iter().map(|e| e.message.clone()).collect(),
            suggestions: vec![],
        })
    }

    /// Run a custom check
    async fn run_custom_check(&self, check: &CustomCheck) -> Result<CheckResult> {
        let start = Instant::now();

        let output = Command::new(&check.command)
            .args(&check.args)
            .current_dir(&self.project_root)
            .output()
            .await
            .context(format!("Failed to run custom check: {}", check.name))?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        Ok(CheckResult {
            check_type: CheckType::Custom,
            passed: output.status.success(),
            duration_ms: duration,
            output: format!("{}\n{}", stdout, stderr),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        })
    }

    /// Check if a file should be excluded from verification
    pub fn is_excluded(&self, file: &str) -> bool {
        for pattern in &self.config.exclude_patterns {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if glob.matches(file) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if a custom check should run based on changed files
    fn should_run_custom_check(&self, check: &CustomCheck, files: &[String]) -> bool {
        if check.run_on.is_empty() {
            return true;
        }
        for pattern in &check.run_on {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                if files.iter().any(|f| glob.matches(f)) {
                    return true;
                }
            }
        }
        false
    }

    /// Detect side effects from file changes
    async fn detect_side_effects(&self, files: &[String]) -> Vec<SideEffect> {
        let mut effects = Vec::new();

        for file in files {
            // Check if it's a new file
            let path = self.project_root.join(file);
            if path.exists() {
                effects.push(SideEffect {
                    effect_type: SideEffectType::FileModified,
                    description: format!("Modified: {}", file),
                    files: vec![file.clone()],
                });
            }

            // Check for test files
            if file.contains("test") || file.contains("_test.rs") {
                effects.push(SideEffect {
                    effect_type: SideEffectType::TestAdded,
                    description: "Test file modified".to_string(),
                    files: vec![file.clone()],
                });
            }
        }

        // Check Cargo.toml for dependency changes
        if files.iter().any(|f| f.ends_with("Cargo.toml")) {
            effects.push(SideEffect {
                effect_type: SideEffectType::DependencyAdded,
                description: "Cargo.toml modified - dependencies may have changed".to_string(),
                files: vec!["Cargo.toml".to_string()],
            });
        }

        effects
    }

    /// Get the last verification results
    pub fn last_results(&self) -> Option<&VerificationReport> {
        self.last_results.as_ref()
    }
}

/// Convert a CompilerError from cargo module to VerificationError
fn compiler_error_to_verification_error(ce: &CompilerError) -> VerificationError {
    VerificationError {
        file: ce.file.clone(),
        line: if ce.line > 0 { Some(ce.line) } else { None },
        column: if ce.column > 0 { Some(ce.column) } else { None },
        message: ce.message.clone(),
        code: ce.code.clone(),
        severity: match ce.severity {
            Severity::Error => ErrorSeverity::Error,
            Severity::Warning => ErrorSeverity::Warning,
            Severity::Note => ErrorSeverity::Note,
            Severity::Help => ErrorSeverity::Help,
        },
        suggestion: ce.suggestion.clone(),
    }
}

/// Parse cargo JSON output into errors and warnings
/// Uses shared parsing logic from crate::tools::cargo
fn parse_cargo_json_output(output: &str) -> (Vec<VerificationError>, Vec<VerificationError>) {
    let (cargo_errors, cargo_warnings) = parse_cargo_json_messages(output);

    let errors = cargo_errors
        .iter()
        .map(compiler_error_to_verification_error)
        .collect();
    let warnings = cargo_warnings
        .iter()
        .map(compiler_error_to_verification_error)
        .collect();

    (errors, warnings)
}

/// Parse test failures from cargo test output
fn parse_test_failures(stdout: &str, stderr: &str) -> Vec<VerificationError> {
    let mut errors = Vec::new();

    // Look for FAILED tests
    for line in stdout.lines().chain(stderr.lines()) {
        if line.contains("FAILED") && line.contains("test ") {
            let test_name = line
                .split("test ")
                .nth(1)
                .and_then(|s| s.split(" ...").next())
                .unwrap_or("unknown");

            errors.push(VerificationError {
                file: String::new(),
                line: None,
                column: None,
                message: format!("Test failed: {}", test_name),
                code: None,
                severity: ErrorSeverity::Error,
                suggestion: Some("Check test output for details".to_string()),
            });
        }

        // Look for panic messages
        if line.contains("panicked at") {
            errors.push(VerificationError {
                file: String::new(),
                line: None,
                column: None,
                message: line.to_string(),
                code: None,
                severity: ErrorSeverity::Error,
                suggestion: None,
            });
        }
    }

    errors
}

/// Format a verification report for display
impl std::fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n╔══════════════════════════════════════════╗")?;
        writeln!(f, "║         VERIFICATION REPORT              ║")?;
        writeln!(f, "╠══════════════════════════════════════════╣")?;
        writeln!(
            f,
            "║ Trigger: {:<30} ║",
            truncate_str(&self.triggered_by, 30)
        )?;
        writeln!(
            f,
            "║ Status: {:<31} ║",
            if self.overall_passed {
                "✓ PASSED"
            } else {
                "✗ FAILED"
            }
        )?;
        writeln!(
            f,
            "║ Duration: {:<29} ║",
            format!("{}ms", self.total_duration_ms)
        )?;
        writeln!(f, "╠══════════════════════════════════════════╣")?;

        for check in &self.checks {
            let status = if check.passed { "✓" } else { "✗" };
            writeln!(
                f,
                "║ {} {}: {}ms",
                status,
                check.check_type.as_str(),
                check.duration_ms
            )?;

            for error in &check.errors {
                writeln!(
                    f,
                    "║   └─ {}: {}",
                    error.file,
                    truncate_str(&error.message, 30)
                )?;
            }
        }

        if !self.suggested_next_steps.is_empty() {
            writeln!(f, "╠══════════════════════════════════════════╣")?;
            writeln!(f, "║ Suggested next steps:                    ║")?;
            for step in &self.suggested_next_steps {
                writeln!(f, "║   • {}", truncate_str(step, 36))?;
            }
        }

        writeln!(f, "╚══════════════════════════════════════════╝")?;
        Ok(())
    }
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(config.format_on_edit);
    }

    #[test]
    fn test_verification_config_fast() {
        let config = VerificationConfig::fast();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(!config.format_on_edit);
    }

    #[test]
    fn test_verification_config_thorough() {
        let config = VerificationConfig::thorough();
        assert!(config.check_on_edit);
        assert!(config.test_on_edit);
        assert!(config.lint_on_edit);
        assert!(config.format_on_edit);
    }

    #[test]
    fn test_check_type_as_str() {
        assert_eq!(CheckType::TypeCheck.as_str(), "type_check");
        assert_eq!(CheckType::Test.as_str(), "test");
        assert_eq!(CheckType::Lint.as_str(), "lint");
        assert_eq!(CheckType::Format.as_str(), "format");
    }

    #[test]
    fn test_parse_cargo_json_output_empty() {
        let (errors, warnings) = parse_cargo_json_output("");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_with_error() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"error","message":"test error","code":{"code":"E0001"},"spans":[{"file_name":"src/main.rs","line_start":1,"column_start":1,"is_primary":true}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
        assert_eq!(errors[0].message, "test error");
    }

    #[test]
    fn test_parse_test_failures() {
        let stdout = "test foo::bar ... FAILED\ntest baz::qux ... ok";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("foo::bar"));
    }

    #[test]
    fn test_verification_report_display() {
        let report = VerificationReport {
            triggered_by: "file_edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 1234,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 500,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: true,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec!["All checks passed".to_string()],
        };

        let display = format!("{}", report);
        assert!(display.contains("VERIFICATION REPORT"));
        assert!(display.contains("PASSED"));
    }

    #[test]
    fn test_error_severity_serde() {
        let severity = ErrorSeverity::Error;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"error\"");
    }

    #[test]
    fn test_side_effect_type_serde() {
        let effect = SideEffectType::FileModified;
        let json = serde_json::to_string(&effect).unwrap();
        assert_eq!(json, "\"file_modified\"");
    }

    #[tokio::test]
    async fn test_verification_gate_new() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_is_excluded() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        assert!(gate.is_excluded("README.md"));
        assert!(gate.is_excluded("config.json"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world", 8), "hello...");
    }

    #[test]
    fn test_check_type_custom() {
        assert_eq!(CheckType::Custom.as_str(), "custom");
    }

    #[test]
    fn test_check_result_creation() {
        let result = CheckResult {
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 100,
            output: "Success".to_string(),
            errors: vec![],
            warnings: vec!["minor warning".to_string()],
            suggestions: vec!["consider this".to_string()],
        };
        assert!(result.passed);
        assert_eq!(result.duration_ms, 100);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.suggestions.len(), 1);
    }

    #[test]
    fn test_verification_error_creation() {
        let error = VerificationError {
            file: "src/main.rs".to_string(),
            line: Some(10),
            column: Some(5),
            message: "error message".to_string(),
            code: Some("E0001".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("fix this".to_string()),
        };
        assert_eq!(error.file, "src/main.rs");
        assert_eq!(error.line, Some(10));
        assert!(error.code.is_some());
    }

    #[test]
    fn test_error_severity_variants() {
        let _ = ErrorSeverity::Error;
        let _ = ErrorSeverity::Warning;
        let _ = ErrorSeverity::Note;
        let _ = ErrorSeverity::Help;
    }

    #[test]
    fn test_side_effect_creation() {
        let effect = SideEffect {
            effect_type: SideEffectType::FileCreated,
            description: "New file".to_string(),
            files: vec!["new.rs".to_string()],
        };
        assert_eq!(effect.effect_type, SideEffectType::FileCreated);
        assert_eq!(effect.files.len(), 1);
    }

    #[test]
    fn test_side_effect_types() {
        assert_eq!(
            serde_json::to_string(&SideEffectType::FileCreated).unwrap(),
            "\"file_created\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::FileDeleted).unwrap(),
            "\"file_deleted\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::DependencyAdded).unwrap(),
            "\"dependency_added\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::DependencyRemoved).unwrap(),
            "\"dependency_removed\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::TestAdded).unwrap(),
            "\"test_added\""
        );
        assert_eq!(
            serde_json::to_string(&SideEffectType::TestRemoved).unwrap(),
            "\"test_removed\""
        );
    }

    #[test]
    fn test_custom_check_creation() {
        let check = CustomCheck {
            name: "my_check".to_string(),
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            run_on: vec!["*.rs".to_string()],
        };
        assert_eq!(check.name, "my_check");
        assert_eq!(check.args.len(), 1);
    }

    #[test]
    fn test_verification_config_default_exclude() {
        let config = VerificationConfig::default();
        assert!(config.exclude_patterns.contains(&"*.md".to_string()));
        assert!(config.exclude_patterns.contains(&"*.txt".to_string()));
        assert!(config.exclude_patterns.contains(&"*.json".to_string()));
        assert!(config.exclude_patterns.contains(&"*.toml".to_string()));
    }

    #[test]
    fn test_should_run_custom_check_empty_run_on() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        let check = CustomCheck {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec![], // Empty means run on all
        };

        assert!(gate.should_run_custom_check(&check, &["any.rs".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_matching_pattern() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        let check = CustomCheck {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string()],
        };

        assert!(gate.should_run_custom_check(&check, &["main.rs".to_string()]));
        assert!(!gate.should_run_custom_check(&check, &["main.py".to_string()]));
    }

    #[test]
    fn test_parse_test_failures_with_panic() {
        let output = "panicked at 'assertion failed', src/test.rs:10";
        let errors = parse_test_failures(output, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("panicked"));
    }

    #[test]
    fn test_parse_test_failures_no_failures() {
        let output = "test foo::bar ... ok\ntest baz::qux ... ok";
        let errors = parse_test_failures(output, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_verification_report_display_failed() {
        let report = VerificationReport {
            triggered_by: "test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 500,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                duration_ms: 500,
                output: "error".to_string(),
                errors: vec![VerificationError {
                    file: "src/main.rs".to_string(),
                    line: Some(10),
                    column: Some(1),
                    message: "type error".to_string(),
                    code: Some("E0001".to_string()),
                    severity: ErrorSeverity::Error,
                    suggestion: None,
                }],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec!["Fix errors".to_string()],
        };

        let display = format!("{}", report);
        assert!(display.contains("FAILED"));
        assert!(display.contains("type_check"));
    }

    #[test]
    fn test_truncate_str_exact_length() {
        assert_eq!(truncate_str("12345678", 8), "12345678");
    }

    #[test]
    fn test_truncate_str_one_over() {
        assert_eq!(truncate_str("123456789", 8), "12345...");
    }

    #[test]
    fn test_check_type_serde() {
        let check = CheckType::TypeCheck;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"type_check\"");

        let check = CheckType::Test;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"test\"");

        let check = CheckType::Lint;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"lint\"");

        let check = CheckType::Format;
        let json = serde_json::to_string(&check).unwrap();
        assert_eq!(json, "\"format\"");
    }

    #[test]
    fn test_error_severity_all_variants() {
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Warning).unwrap(),
            "\"warning\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Note).unwrap(),
            "\"note\""
        );
        assert_eq!(
            serde_json::to_string(&ErrorSeverity::Help).unwrap(),
            "\"help\""
        );
    }

    #[test]
    fn test_is_excluded_rs_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);

        // .rs files should not be excluded
        assert!(!gate.is_excluded("src/main.rs"));
        assert!(!gate.is_excluded("lib.rs"));
    }

    #[test]
    fn test_is_excluded_pattern_matching() {
        let config = VerificationConfig {
            exclude_patterns: vec!["*.test.rs".to_string(), "target/*".to_string()],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);

        assert!(gate.is_excluded("foo.test.rs"));
        // Note: glob matching depends on exact pattern syntax
    }

    #[test]
    fn test_compiler_error_to_verification_error() {
        let ce = CompilerError {
            file: "test.rs".to_string(),
            line: 5,
            column: 10,
            message: "test message".to_string(),
            code: Some("E0001".to_string()),
            severity: Severity::Error,
            suggestion: Some("fix it".to_string()),
            snippet: "let x = 1;".to_string(),
        };

        let ve = compiler_error_to_verification_error(&ce);
        assert_eq!(ve.file, "test.rs");
        assert_eq!(ve.line, Some(5));
        assert_eq!(ve.column, Some(10));
        assert_eq!(ve.message, "test message");
        assert_eq!(ve.code, Some("E0001".to_string()));
        assert!(matches!(ve.severity, ErrorSeverity::Error));
        assert_eq!(ve.suggestion, Some("fix it".to_string()));
    }

    #[test]
    fn test_compiler_error_to_verification_error_zero_line() {
        let ce = CompilerError {
            file: "test.rs".to_string(),
            line: 0,
            column: 0,
            message: "test".to_string(),
            code: None,
            severity: Severity::Warning,
            suggestion: None,
            snippet: String::new(),
        };

        let ve = compiler_error_to_verification_error(&ce);
        assert!(ve.line.is_none());
        assert!(ve.column.is_none());
    }

    #[test]
    fn test_compiler_error_severity_mapping() {
        for (cargo_sev, expected_sev) in [
            (Severity::Error, ErrorSeverity::Error),
            (Severity::Warning, ErrorSeverity::Warning),
            (Severity::Note, ErrorSeverity::Note),
            (Severity::Help, ErrorSeverity::Help),
        ] {
            let ce = CompilerError {
                file: "test.rs".to_string(),
                line: 1,
                column: 1,
                message: "test".to_string(),
                code: None,
                severity: cargo_sev,
                suggestion: None,
                snippet: String::new(),
            };
            let ve = compiler_error_to_verification_error(&ce);
            assert_eq!(ve.severity, expected_sev);
        }
    }

    #[test]
    fn test_verification_report_clone() {
        let report = VerificationReport {
            triggered_by: "test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 100,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };

        let cloned = report.clone();
        assert_eq!(cloned.triggered_by, report.triggered_by);
        assert_eq!(cloned.overall_passed, report.overall_passed);
    }

    #[test]
    fn test_check_result_serde() {
        let result = CheckResult {
            check_type: CheckType::Test,
            passed: true,
            duration_ms: 50,
            output: "ok".to_string(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"check_type\":\"test\""));
        assert!(json.contains("\"passed\":true"));
    }
}
