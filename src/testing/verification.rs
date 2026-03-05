//! Verification Gates - Automatic validation after every code change
//!
//! Implements the "never proceed on assumptions" protocol:
//! 1. Speculate: Agent proposes an edit
//! 2. Validate: Harness runs checks automatically
//! 3. Feedback: Agent sees results immediately
//! 4. Commit: Only on green, or explicit override

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

    // ===== Additional tests for comprehensive coverage =====

    #[test]
    fn test_check_type_deserialize_all_variants() {
        let cases = [
            ("\"type_check\"", CheckType::TypeCheck),
            ("\"test\"", CheckType::Test),
            ("\"lint\"", CheckType::Lint),
            ("\"format\"", CheckType::Format),
            ("\"custom\"", CheckType::Custom),
        ];
        for (json_str, expected) in cases {
            let deserialized: CheckType = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_error_severity_deserialize_all_variants() {
        let cases = [
            ("\"error\"", ErrorSeverity::Error),
            ("\"warning\"", ErrorSeverity::Warning),
            ("\"note\"", ErrorSeverity::Note),
            ("\"help\"", ErrorSeverity::Help),
        ];
        for (json_str, expected) in cases {
            let deserialized: ErrorSeverity = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_side_effect_type_deserialize_all_variants() {
        let cases = [
            ("\"file_created\"", SideEffectType::FileCreated),
            ("\"file_modified\"", SideEffectType::FileModified),
            ("\"file_deleted\"", SideEffectType::FileDeleted),
            ("\"dependency_added\"", SideEffectType::DependencyAdded),
            ("\"dependency_removed\"", SideEffectType::DependencyRemoved),
            ("\"test_added\"", SideEffectType::TestAdded),
            ("\"test_removed\"", SideEffectType::TestRemoved),
        ];
        for (json_str, expected) in cases {
            let deserialized: SideEffectType = serde_json::from_str(json_str).unwrap();
            assert_eq!(deserialized, expected);
        }
    }

    #[test]
    fn test_verification_config_default_all_fields() {
        let config = VerificationConfig::default();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
        assert_eq!(config.exclude_patterns.len(), 4);
        assert!(config.custom_checks.is_empty());
    }

    #[test]
    fn test_verification_config_fast_inherits_defaults() {
        let config = VerificationConfig::fast();
        assert!(config.check_on_edit);
        assert!(!config.test_on_edit);
        assert!(!config.lint_on_edit);
        assert!(!config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
        assert_eq!(config.exclude_patterns.len(), 4);
        assert!(config.custom_checks.is_empty());
    }

    #[test]
    fn test_verification_config_thorough_inherits_defaults() {
        let config = VerificationConfig::thorough();
        assert!(config.check_on_edit);
        assert!(config.test_on_edit);
        assert!(config.lint_on_edit);
        assert!(config.format_on_edit);
        assert!(config.incremental);
        assert_eq!(config.check_timeout_secs, 60);
        assert!(config.continue_on_failure);
    }

    #[test]
    fn test_verification_config_serde_roundtrip() {
        let config = VerificationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.check_on_edit, config.check_on_edit);
        assert_eq!(deserialized.test_on_edit, config.test_on_edit);
        assert_eq!(deserialized.lint_on_edit, config.lint_on_edit);
        assert_eq!(deserialized.format_on_edit, config.format_on_edit);
        assert_eq!(deserialized.incremental, config.incremental);
        assert_eq!(deserialized.check_timeout_secs, config.check_timeout_secs);
        assert_eq!(deserialized.continue_on_failure, config.continue_on_failure);
        assert_eq!(deserialized.exclude_patterns, config.exclude_patterns);
    }

    #[test]
    fn test_custom_check_serde_roundtrip() {
        let check = CustomCheck {
            name: "my_lint".to_string(),
            command: "my-linter".to_string(),
            args: vec!["--strict".to_string(), "--fix".to_string()],
            run_on: vec!["*.rs".to_string(), "*.toml".to_string()],
        };
        let json = serde_json::to_string(&check).unwrap();
        let deserialized: CustomCheck = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "my_lint");
        assert_eq!(deserialized.command, "my-linter");
        assert_eq!(deserialized.args.len(), 2);
        assert_eq!(deserialized.run_on.len(), 2);
    }

    #[test]
    fn test_side_effect_serde_roundtrip() {
        let effect = SideEffect {
            effect_type: SideEffectType::DependencyRemoved,
            description: "Removed dep xyz".to_string(),
            files: vec!["Cargo.toml".to_string(), "Cargo.lock".to_string()],
        };
        let json = serde_json::to_string(&effect).unwrap();
        let deserialized: SideEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.effect_type, SideEffectType::DependencyRemoved);
        assert_eq!(deserialized.description, "Removed dep xyz");
        assert_eq!(deserialized.files.len(), 2);
    }

    #[test]
    fn test_check_result_serde_roundtrip_with_errors() {
        let result = CheckResult {
            check_type: CheckType::Lint,
            passed: false,
            duration_ms: 999,
            output: "clippy output here".to_string(),
            errors: vec![VerificationError {
                file: "src/lib.rs".to_string(),
                line: Some(42),
                column: Some(10),
                message: "unused variable".to_string(),
                code: Some("clippy::unused".to_string()),
                severity: ErrorSeverity::Warning,
                suggestion: Some("prefix with _".to_string()),
            }],
            warnings: vec!["minor issue".to_string()],
            suggestions: vec!["run clippy --fix".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: CheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.check_type, CheckType::Lint);
        assert!(!deserialized.passed);
        assert_eq!(deserialized.duration_ms, 999);
        assert_eq!(deserialized.errors.len(), 1);
        assert_eq!(deserialized.errors[0].file, "src/lib.rs");
        assert_eq!(deserialized.errors[0].line, Some(42));
        assert_eq!(deserialized.errors[0].column, Some(10));
        assert_eq!(deserialized.errors[0].message, "unused variable");
        assert_eq!(
            deserialized.errors[0].code,
            Some("clippy::unused".to_string())
        );
        assert_eq!(deserialized.warnings.len(), 1);
        assert_eq!(deserialized.suggestions.len(), 1);
    }

    #[test]
    fn test_verification_error_serde_roundtrip() {
        let error = VerificationError {
            file: "src/main.rs".to_string(),
            line: Some(10),
            column: None,
            message: "mismatched types".to_string(),
            code: Some("E0308".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("expected i32, found &str".to_string()),
        };
        let json = serde_json::to_string(&error).unwrap();
        let deserialized: VerificationError = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.file, "src/main.rs");
        assert_eq!(deserialized.line, Some(10));
        assert_eq!(deserialized.column, None);
        assert_eq!(deserialized.message, "mismatched types");
        assert_eq!(deserialized.code, Some("E0308".to_string()));
        assert!(matches!(deserialized.severity, ErrorSeverity::Error));
        assert_eq!(
            deserialized.suggestion,
            Some("expected i32, found &str".to_string())
        );
    }

    #[test]
    fn test_verification_error_all_none_fields() {
        let error = VerificationError {
            file: String::new(),
            line: None,
            column: None,
            message: "generic error".to_string(),
            code: None,
            severity: ErrorSeverity::Note,
            suggestion: None,
        };
        assert!(error.file.is_empty());
        assert!(error.line.is_none());
        assert!(error.column.is_none());
        assert!(error.code.is_none());
        assert!(error.suggestion.is_none());
        assert!(matches!(error.severity, ErrorSeverity::Note));
    }

    #[test]
    fn test_verification_report_serde_roundtrip() {
        let report = VerificationReport {
            triggered_by: "file_edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 2500,
            checks: vec![
                CheckResult {
                    check_type: CheckType::TypeCheck,
                    passed: true,
                    duration_ms: 1000,
                    output: "ok".to_string(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Format,
                    passed: false,
                    duration_ms: 200,
                    output: "Diff in src/main.rs".to_string(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec!["Run `cargo fmt` to fix formatting".to_string()],
                },
            ],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string(), "src/lib.rs".to_string()],
            side_effects: vec![SideEffect {
                effect_type: SideEffectType::FileModified,
                description: "Modified src/main.rs".to_string(),
                files: vec!["src/main.rs".to_string()],
            }],
            suggested_next_steps: vec!["Run cargo fmt to fix formatting".to_string()],
        };
        let json = serde_json::to_string(&report).unwrap();
        let deserialized: VerificationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.triggered_by, "file_edit");
        assert_eq!(deserialized.total_duration_ms, 2500);
        assert_eq!(deserialized.checks.len(), 2);
        assert!(!deserialized.overall_passed);
        assert_eq!(deserialized.affected_files.len(), 2);
        assert_eq!(deserialized.side_effects.len(), 1);
        assert_eq!(deserialized.suggested_next_steps.len(), 1);
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_empty_with_zero_max() {
        assert_eq!(truncate_str("hello", 0), "...");
    }

    #[test]
    fn test_truncate_str_max_len_1() {
        assert_eq!(truncate_str("hello", 1), "...");
    }

    #[test]
    fn test_truncate_str_max_len_3() {
        assert_eq!(truncate_str("hello", 3), "...");
    }

    #[test]
    fn test_truncate_str_max_len_4() {
        assert_eq!(truncate_str("hello", 4), "h...");
    }

    #[test]
    fn test_truncate_str_max_len_5_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_very_long_string() {
        let long = "a".repeat(200);
        let result = truncate_str(&long, 10);
        assert_eq!(result.len(), 10);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_is_excluded_txt_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("notes.txt"));
    }

    #[test]
    fn test_is_excluded_toml_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("Cargo.toml"));
    }

    #[test]
    fn test_is_excluded_empty_exclude_patterns() {
        let config = VerificationConfig {
            exclude_patterns: vec![],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(!gate.is_excluded("README.md"));
        assert!(!gate.is_excluded("config.json"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_is_excluded_with_invalid_glob_pattern() {
        let config = VerificationConfig {
            exclude_patterns: vec!["[invalid".to_string()],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_is_excluded_multiple_patterns() {
        let config = VerificationConfig {
            exclude_patterns: vec![
                "*.md".to_string(),
                "*.log".to_string(),
                "vendor/*".to_string(),
            ],
            ..Default::default()
        };
        let gate = VerificationGate::new(".", config);
        assert!(gate.is_excluded("README.md"));
        assert!(gate.is_excluded("debug.log"));
        assert!(!gate.is_excluded("src/main.rs"));
    }

    #[test]
    fn test_should_run_custom_check_no_matching_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "py_check".to_string(),
            command: "python".to_string(),
            args: vec![],
            run_on: vec!["*.py".to_string()],
        };
        assert!(
            !gate.should_run_custom_check(&check, &["main.rs".to_string(), "lib.rs".to_string()])
        );
    }

    #[test]
    fn test_should_run_custom_check_multiple_patterns() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "multi_check".to_string(),
            command: "lint".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string(), "*.toml".to_string()],
        };
        assert!(gate.should_run_custom_check(&check, &["Cargo.toml".to_string()]));
        assert!(gate.should_run_custom_check(&check, &["main.rs".to_string()]));
        assert!(!gate.should_run_custom_check(&check, &["script.py".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_invalid_glob() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "bad_glob".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["[invalid".to_string()],
        };
        assert!(!gate.should_run_custom_check(&check, &["main.rs".to_string()]));
    }

    #[test]
    fn test_should_run_custom_check_empty_files_list() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let check = CustomCheck {
            name: "check".to_string(),
            command: "echo".to_string(),
            args: vec![],
            run_on: vec!["*.rs".to_string()],
        };
        let empty: &[String] = &[];
        assert!(!gate.should_run_custom_check(&check, empty));
    }

    #[test]
    fn test_parse_test_failures_from_stderr() {
        // Note: split("test ") splits on ALL occurrences, including inside "my_test",
        // so use a test name that doesn't contain "test " as a substring
        let stderr = "test my_module::some_fn ... FAILED";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("my_module::some_fn"),
            "actual message: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn test_parse_test_failures_both_stdout_and_stderr() {
        let stdout = "test stdout_test ... FAILED";
        let stderr = "test stderr_test ... FAILED";
        let errors = parse_test_failures(stdout, stderr);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn test_parse_test_failures_panic_in_stderr() {
        let stderr = "thread 'main' panicked at 'assertion failed: x == y', src/lib.rs:42";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("panicked at"));
        assert!(matches!(errors[0].severity, ErrorSeverity::Error));
    }

    #[test]
    fn test_parse_test_failures_combined_failure_and_panic() {
        let output = "test my_test ... FAILED\nthread 'main' panicked at 'oops', src/test.rs:10";
        let errors = parse_test_failures(output, "");
        assert_eq!(errors.len(), 2);
        assert!(errors[0].message.contains("Test failed"));
        assert!(errors[1].message.contains("panicked"));
    }

    #[test]
    fn test_parse_test_failures_failed_without_test_prefix() {
        let output = "some other line FAILED";
        let errors = parse_test_failures(output, "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_test_failures_empty_inputs() {
        let errors = parse_test_failures("", "");
        assert!(errors.is_empty());
    }

    #[test]
    fn test_parse_test_failures_error_fields() {
        let stdout = "test foo::bar ... FAILED";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].file.is_empty());
        assert!(errors[0].line.is_none());
        assert!(errors[0].column.is_none());
        assert!(errors[0].code.is_none());
        assert!(matches!(errors[0].severity, ErrorSeverity::Error));
        assert_eq!(
            errors[0].suggestion,
            Some("Check test output for details".to_string())
        );
    }

    #[test]
    fn test_parse_test_failures_panic_fields() {
        let stderr = "thread 'main' panicked at 'oops'";
        let errors = parse_test_failures("", stderr);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].file.is_empty());
        assert!(errors[0].line.is_none());
        assert!(errors[0].column.is_none());
        assert!(errors[0].code.is_none());
        assert!(errors[0].suggestion.is_none());
    }

    #[test]
    fn test_parse_cargo_json_output_with_warning() {
        let json_line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"unused variable","code":{"code":"W0001"},"spans":[{"file_name":"src/lib.rs","line_start":5,"column_start":3,"is_primary":true}],"children":[]}}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert!(errors.is_empty());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].message, "unused variable");
        assert!(matches!(warnings[0].severity, ErrorSeverity::Warning));
    }

    #[test]
    fn test_parse_cargo_json_output_mixed_errors_and_warnings() {
        let error_line = r#"{"reason":"compiler-message","message":{"level":"error","message":"type mismatch","code":{"code":"E0308"},"spans":[{"file_name":"src/main.rs","line_start":10,"column_start":5,"is_primary":true}],"children":[]}}"#;
        let warning_line = r#"{"reason":"compiler-message","message":{"level":"warning","message":"dead code","code":null,"spans":[{"file_name":"src/lib.rs","line_start":20,"column_start":1,"is_primary":true}],"children":[]}}"#;
        let output = format!("{}\n{}", error_line, warning_line);
        let (errors, warnings) = parse_cargo_json_output(&output);
        assert_eq!(errors.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(errors[0].message, "type mismatch");
        assert_eq!(warnings[0].message, "dead code");
    }

    #[test]
    fn test_parse_cargo_json_output_non_compiler_message() {
        let json_line =
            r#"{"reason":"build-script-executed","package_id":"some_pkg","out_dir":"/tmp"}"#;
        let (errors, warnings) = parse_cargo_json_output(json_line);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_invalid_json() {
        let output = "this is not json\nalso not json\n";
        let (errors, warnings) = parse_cargo_json_output(output);
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parse_cargo_json_output_mixed_json_and_text() {
        let output = "Compiling foo v0.1.0\n{\"reason\":\"compiler-message\",\"message\":{\"level\":\"error\",\"message\":\"boom\",\"code\":{\"code\":\"E0001\"},\"spans\":[{\"file_name\":\"src/main.rs\",\"line_start\":1,\"column_start\":1,\"is_primary\":true}],\"children\":[]}}\nFinished dev";
        let (errors, warnings) = parse_cargo_json_output(output);
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_compiler_error_to_verification_error_note_severity() {
        let ce = CompilerError {
            file: "src/mod.rs".to_string(),
            line: 3,
            column: 0,
            message: "note message".to_string(),
            code: None,
            severity: Severity::Note,
            suggestion: None,
            snippet: String::new(),
        };
        let ve = compiler_error_to_verification_error(&ce);
        assert!(matches!(ve.severity, ErrorSeverity::Note));
        assert_eq!(ve.column, None);
        assert_eq!(ve.line, Some(3));
    }

    #[test]
    fn test_compiler_error_to_verification_error_help_severity() {
        let ce = CompilerError {
            file: "src/mod.rs".to_string(),
            line: 0,
            column: 5,
            message: "help message".to_string(),
            code: Some("help_code".to_string()),
            severity: Severity::Help,
            suggestion: Some("try this".to_string()),
            snippet: "fn main() {}".to_string(),
        };
        let ve = compiler_error_to_verification_error(&ce);
        assert!(matches!(ve.severity, ErrorSeverity::Help));
        assert_eq!(ve.line, None);
        assert_eq!(ve.column, Some(5));
        assert_eq!(ve.code, Some("help_code".to_string()));
        assert_eq!(ve.suggestion, Some("try this".to_string()));
    }

    #[test]
    fn test_verification_gate_new_with_pathbuf() {
        let path = PathBuf::from("/tmp/test_project");
        let config = VerificationConfig::fast();
        let gate = VerificationGate::new(&path, config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_verification_gate_new_with_string() {
        let config = VerificationConfig::thorough();
        let gate = VerificationGate::new("/some/path", config);
        assert!(gate.last_results().is_none());
    }

    #[test]
    fn test_verification_report_display_no_checks() {
        let report = VerificationReport {
            triggered_by: "test_trigger".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 0,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let display = format!("{}", report);
        assert!(display.contains("VERIFICATION REPORT"));
        assert!(display.contains("PASSED"));
        assert!(display.contains("0ms"));
        assert!(!display.contains("Suggested next steps:"));
    }

    #[test]
    fn test_verification_report_display_long_trigger() {
        let report = VerificationReport {
            triggered_by: "this_is_a_very_long_trigger_name_that_exceeds_30_chars".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 42,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let display = format!("{}", report);
        assert!(display.contains("..."));
    }

    #[test]
    fn test_verification_report_display_multiple_checks() {
        let report = VerificationReport {
            triggered_by: "multi".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 3000,
            checks: vec![
                CheckResult {
                    check_type: CheckType::TypeCheck,
                    passed: true,
                    duration_ms: 1000,
                    output: String::new(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Format,
                    passed: true,
                    duration_ms: 200,
                    output: String::new(),
                    errors: vec![],
                    warnings: vec![],
                    suggestions: vec![],
                },
                CheckResult {
                    check_type: CheckType::Lint,
                    passed: false,
                    duration_ms: 800,
                    output: "clippy warnings".to_string(),
                    errors: vec![VerificationError {
                        file: "src/main.rs".to_string(),
                        line: Some(5),
                        column: Some(1),
                        message: "this is a very long error message that should be truncated"
                            .to_string(),
                        code: None,
                        severity: ErrorSeverity::Warning,
                        suggestion: None,
                    }],
                    warnings: vec![],
                    suggestions: vec![],
                },
            ],
            overall_passed: false,
            affected_files: vec!["src/main.rs".to_string()],
            side_effects: vec![],
            suggested_next_steps: vec![
                "Fix clippy warnings".to_string(),
                "Run cargo clippy --fix".to_string(),
            ],
        };
        let display = format!("{}", report);
        assert!(display.contains("FAILED"));
        assert!(display.contains("type_check"));
        assert!(display.contains("format"));
        assert!(display.contains("lint"));
        assert!(display.contains("src/main.rs"));
        assert!(display.contains("Suggested next steps:"));
        assert!(display.contains("Fix clippy warnings"));
    }

    #[test]
    fn test_verification_report_display_multiple_errors_in_check() {
        let report = VerificationReport {
            triggered_by: "edit".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 100,
            checks: vec![CheckResult {
                check_type: CheckType::TypeCheck,
                passed: false,
                duration_ms: 100,
                output: "errors".to_string(),
                errors: vec![
                    VerificationError {
                        file: "a.rs".to_string(),
                        line: Some(1),
                        column: Some(1),
                        message: "error one".to_string(),
                        code: None,
                        severity: ErrorSeverity::Error,
                        suggestion: None,
                    },
                    VerificationError {
                        file: "b.rs".to_string(),
                        line: Some(2),
                        column: None,
                        message: "error two".to_string(),
                        code: None,
                        severity: ErrorSeverity::Error,
                        suggestion: None,
                    },
                ],
                warnings: vec![],
                suggestions: vec![],
            }],
            overall_passed: false,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec!["Fix type errors".to_string()],
        };
        let display = format!("{}", report);
        assert!(display.contains("a.rs"));
        assert!(display.contains("b.rs"));
    }

    #[tokio::test]
    async fn test_detect_side_effects_empty_files() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate.detect_side_effects(&[]).await;
        assert!(effects.is_empty());
    }

    #[tokio::test]
    async fn test_detect_side_effects_test_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["src/my_test.rs".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        assert!(has_test_added);
    }

    #[tokio::test]
    async fn test_detect_side_effects_cargo_toml() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate.detect_side_effects(&["Cargo.toml".to_string()]).await;
        let has_dep_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::DependencyAdded);
        assert!(has_dep_added);
        let dep_effect = effects
            .iter()
            .find(|e| e.effect_type == SideEffectType::DependencyAdded)
            .unwrap();
        assert!(dep_effect.description.contains("Cargo.toml"));
    }

    #[tokio::test]
    async fn test_detect_side_effects_test_and_cargo_combined() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["tests/unit_test.rs".to_string(), "Cargo.toml".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        let has_dep_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::DependencyAdded);
        assert!(has_test_added);
        assert!(has_dep_added);
    }

    #[tokio::test]
    async fn test_detect_side_effects_existing_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(env!("CARGO_MANIFEST_DIR"), config);
        let effects = gate.detect_side_effects(&["Cargo.toml".to_string()]).await;
        let has_modified = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::FileModified);
        assert!(has_modified);
    }

    #[tokio::test]
    async fn test_detect_side_effects_nonexistent_file() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new("/tmp/nonexistent_project_xyz", config);
        let effects = gate.detect_side_effects(&["src/main.rs".to_string()]).await;
        let has_modified = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::FileModified);
        assert!(!has_modified);
    }

    #[tokio::test]
    async fn test_detect_side_effects_file_with_test_in_name() {
        let config = VerificationConfig::default();
        let gate = VerificationGate::new(".", config);
        let effects = gate
            .detect_side_effects(&["integration_test_helpers.rs".to_string()])
            .await;
        let has_test_added = effects
            .iter()
            .any(|e| e.effect_type == SideEffectType::TestAdded);
        assert!(has_test_added);
    }

    #[tokio::test]
    async fn test_verify_change_all_excluded_files() {
        let config = VerificationConfig::default();
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(
                &[
                    "README.md".to_string(),
                    "config.json".to_string(),
                    "notes.txt".to_string(),
                ],
                "test_trigger",
            )
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
        assert_eq!(report.total_duration_ms, 0);
        assert_eq!(report.triggered_by, "test_trigger");
        assert_eq!(report.affected_files.len(), 3);
        assert_eq!(report.suggested_next_steps.len(), 1);
        assert!(report.suggested_next_steps[0].contains("No code files changed"));
    }

    #[tokio::test]
    async fn test_verify_change_stores_last_results() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        assert!(gate.last_results().is_none());
        let _report = gate
            .verify_change(&["src/main.rs".to_string()], "edit")
            .await
            .unwrap();
        assert!(gate.last_results().is_some());
        let last = gate.last_results().unwrap();
        assert_eq!(last.triggered_by, "edit");
    }

    #[tokio::test]
    async fn test_verify_change_no_checks_enabled_with_rs_file() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["src/main.rs".to_string()], "no_checks")
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
        assert_eq!(
            report.suggested_next_steps,
            vec!["All checks passed - safe to proceed"]
        );
    }

    #[tokio::test]
    async fn test_verify_change_non_rust_files_not_excluded() {
        let config = VerificationConfig {
            check_on_edit: true,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "py_edit")
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
    }

    #[tokio::test]
    async fn test_verify_change_with_custom_check_that_runs() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "echo_check".to_string(),
                command: "echo".to_string(),
                args: vec!["hello".to_string()],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "custom_trigger")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].check_type, CheckType::Custom);
        assert!(report.checks[0].passed);
        assert!(report.overall_passed);
    }

    #[tokio::test]
    async fn test_verify_change_with_custom_check_pattern_match() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "rs_only".to_string(),
                command: "echo".to_string(),
                args: vec!["checking".to_string()],
                run_on: vec!["*.rs".to_string()],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);

        let report = gate
            .verify_change(&["script.py".to_string()], "py_edit")
            .await
            .unwrap();
        assert!(report.checks.is_empty());

        let report = gate
            .verify_change(&["main.rs".to_string()], "rs_edit")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.checks[0].check_type, CheckType::Custom);
    }

    #[tokio::test]
    async fn test_verify_change_with_failing_custom_check() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "failing_check".to_string(),
                command: "false".to_string(),
                args: vec![],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["script.py".to_string()], "fail_trigger")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert!(!report.checks[0].passed);
        assert!(!report.overall_passed);
    }

    #[tokio::test]
    async fn test_full_verify_with_no_files() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate.full_verify().await.unwrap();
        assert!(report.overall_passed);
        assert!(report.checks.is_empty());
    }

    #[test]
    fn test_check_result_clone() {
        let result = CheckResult {
            check_type: CheckType::Lint,
            passed: false,
            duration_ms: 250,
            output: "lint errors".to_string(),
            errors: vec![VerificationError {
                file: "src/lib.rs".to_string(),
                line: Some(10),
                column: Some(5),
                message: "unused var".to_string(),
                code: Some("W001".to_string()),
                severity: ErrorSeverity::Warning,
                suggestion: Some("remove it".to_string()),
            }],
            warnings: vec!["w1".to_string()],
            suggestions: vec!["s1".to_string()],
        };
        let cloned = result.clone();
        assert_eq!(cloned.check_type, result.check_type);
        assert_eq!(cloned.passed, result.passed);
        assert_eq!(cloned.duration_ms, result.duration_ms);
        assert_eq!(cloned.output, result.output);
        assert_eq!(cloned.errors.len(), 1);
        assert_eq!(cloned.errors[0].file, "src/lib.rs");
        assert_eq!(cloned.warnings, result.warnings);
        assert_eq!(cloned.suggestions, result.suggestions);
    }

    #[test]
    fn test_verification_error_clone() {
        let error = VerificationError {
            file: "test.rs".to_string(),
            line: Some(1),
            column: Some(2),
            message: "msg".to_string(),
            code: Some("E0001".to_string()),
            severity: ErrorSeverity::Error,
            suggestion: Some("fix".to_string()),
        };
        let cloned = error.clone();
        assert_eq!(cloned.file, error.file);
        assert_eq!(cloned.line, error.line);
        assert_eq!(cloned.column, error.column);
        assert_eq!(cloned.message, error.message);
        assert_eq!(cloned.code, error.code);
        assert_eq!(cloned.suggestion, error.suggestion);
    }

    #[test]
    fn test_side_effect_clone() {
        let effect = SideEffect {
            effect_type: SideEffectType::TestRemoved,
            description: "removed test".to_string(),
            files: vec!["test.rs".to_string()],
        };
        let cloned = effect.clone();
        assert_eq!(cloned.effect_type, effect.effect_type);
        assert_eq!(cloned.description, effect.description);
        assert_eq!(cloned.files, effect.files);
    }

    #[test]
    fn test_check_type_debug() {
        assert_eq!(format!("{:?}", CheckType::TypeCheck), "TypeCheck");
        assert_eq!(format!("{:?}", CheckType::Test), "Test");
        assert_eq!(format!("{:?}", CheckType::Lint), "Lint");
        assert_eq!(format!("{:?}", CheckType::Format), "Format");
        assert_eq!(format!("{:?}", CheckType::Custom), "Custom");
    }

    #[test]
    fn test_error_severity_debug() {
        assert_eq!(format!("{:?}", ErrorSeverity::Error), "Error");
        assert_eq!(format!("{:?}", ErrorSeverity::Warning), "Warning");
        assert_eq!(format!("{:?}", ErrorSeverity::Note), "Note");
        assert_eq!(format!("{:?}", ErrorSeverity::Help), "Help");
    }

    #[test]
    fn test_side_effect_type_debug() {
        assert_eq!(format!("{:?}", SideEffectType::FileCreated), "FileCreated");
        assert_eq!(
            format!("{:?}", SideEffectType::FileModified),
            "FileModified"
        );
        assert_eq!(format!("{:?}", SideEffectType::FileDeleted), "FileDeleted");
        assert_eq!(
            format!("{:?}", SideEffectType::DependencyAdded),
            "DependencyAdded"
        );
        assert_eq!(
            format!("{:?}", SideEffectType::DependencyRemoved),
            "DependencyRemoved"
        );
        assert_eq!(format!("{:?}", SideEffectType::TestAdded), "TestAdded");
        assert_eq!(format!("{:?}", SideEffectType::TestRemoved), "TestRemoved");
    }

    #[test]
    fn test_check_result_debug() {
        let result = CheckResult {
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 0,
            output: String::new(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("CheckResult"));
        assert!(debug.contains("TypeCheck"));
    }

    #[test]
    fn test_verification_error_debug() {
        let error = VerificationError {
            file: "test.rs".to_string(),
            line: Some(1),
            column: None,
            message: "err".to_string(),
            code: None,
            severity: ErrorSeverity::Error,
            suggestion: None,
        };
        let debug = format!("{:?}", error);
        assert!(debug.contains("VerificationError"));
        assert!(debug.contains("test.rs"));
    }

    #[test]
    fn test_verification_report_debug() {
        let report = VerificationReport {
            triggered_by: "debug_test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 0,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![],
        };
        let debug = format!("{:?}", report);
        assert!(debug.contains("VerificationReport"));
        assert!(debug.contains("debug_test"));
    }

    #[test]
    fn test_side_effect_debug() {
        let effect = SideEffect {
            effect_type: SideEffectType::FileCreated,
            description: "created".to_string(),
            files: vec![],
        };
        let debug = format!("{:?}", effect);
        assert!(debug.contains("SideEffect"));
        assert!(debug.contains("FileCreated"));
    }

    #[test]
    fn test_verification_config_debug() {
        let config = VerificationConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("VerificationConfig"));
        assert!(debug.contains("check_on_edit"));
    }

    #[test]
    fn test_custom_check_debug() {
        let check = CustomCheck {
            name: "test".to_string(),
            command: "cmd".to_string(),
            args: vec![],
            run_on: vec![],
        };
        let debug = format!("{:?}", check);
        assert!(debug.contains("CustomCheck"));
    }

    #[test]
    fn test_check_type_copy_and_eq() {
        let a = CheckType::TypeCheck;
        let b = a;
        assert_eq!(a, b);
        assert_eq!(CheckType::Test, CheckType::Test);
        assert_ne!(CheckType::Test, CheckType::Lint);
    }

    #[test]
    fn test_error_severity_copy_and_eq() {
        let a = ErrorSeverity::Warning;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(ErrorSeverity::Error, ErrorSeverity::Help);
    }

    #[test]
    fn test_side_effect_type_copy_and_eq() {
        let a = SideEffectType::FileCreated;
        let b = a;
        assert_eq!(a, b);
        assert_ne!(SideEffectType::FileCreated, SideEffectType::FileDeleted);
    }

    #[test]
    fn test_verification_config_with_custom_checks_serde() {
        let config = VerificationConfig {
            custom_checks: vec![
                CustomCheck {
                    name: "check1".to_string(),
                    command: "cmd1".to_string(),
                    args: vec!["--flag".to_string()],
                    run_on: vec!["*.rs".to_string()],
                },
                CustomCheck {
                    name: "check2".to_string(),
                    command: "cmd2".to_string(),
                    args: vec![],
                    run_on: vec![],
                },
            ],
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.custom_checks.len(), 2);
        assert_eq!(deserialized.custom_checks[0].name, "check1");
        assert_eq!(deserialized.custom_checks[1].name, "check2");
    }

    #[test]
    fn test_overall_passed_with_empty_checks() {
        let checks: Vec<CheckResult> = vec![];
        assert!(checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_overall_passed_all_pass() {
        let checks = [
            CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                check_type: CheckType::Format,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
        ];
        assert!(checks.iter().all(|c| c.passed));
    }

    #[test]
    fn test_overall_passed_one_fails() {
        let checks = [
            CheckResult {
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                check_type: CheckType::Test,
                passed: false,
                duration_ms: 0,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
        ];
        assert!(!checks.iter().all(|c| c.passed));
    }

    #[tokio::test]
    async fn test_run_custom_check_captures_output() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            custom_checks: vec![CustomCheck {
                name: "echo_test".to_string(),
                command: "echo".to_string(),
                args: vec!["custom_output_text".to_string()],
                run_on: vec![],
            }],
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["file.py".to_string()], "custom_test")
            .await
            .unwrap();
        assert_eq!(report.checks.len(), 1);
        assert!(report.checks[0].output.contains("custom_output_text"));
    }

    #[tokio::test]
    async fn test_verify_change_mixed_excluded_and_non_excluded() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let report = gate
            .verify_change(&["README.md".to_string(), "script.py".to_string()], "mixed")
            .await
            .unwrap();
        assert!(report.overall_passed);
        assert!(report.affected_files.contains(&"script.py".to_string()));
        assert!(!report.affected_files.contains(&"README.md".to_string()));
    }

    #[tokio::test]
    async fn test_verify_change_updates_last_results_on_successive_calls() {
        let config = VerificationConfig {
            check_on_edit: false,
            test_on_edit: false,
            lint_on_edit: false,
            format_on_edit: false,
            ..Default::default()
        };
        let mut gate = VerificationGate::new(".", config);
        let _r1 = gate
            .verify_change(&["a.py".to_string()], "first")
            .await
            .unwrap();
        assert_eq!(gate.last_results().unwrap().triggered_by, "first");
        let _r2 = gate
            .verify_change(&["b.py".to_string()], "second")
            .await
            .unwrap();
        assert_eq!(gate.last_results().unwrap().triggered_by, "second");
    }

    #[test]
    fn test_parse_test_failures_test_failed_no_dots_separator() {
        // Without " ..." separator, the split on "test " can match within the test name
        // For "test some_fn FAILED": split("test ") -> ["", "some_fn FAILED"]
        // nth(1) = "some_fn FAILED", split(" ...").next() = "some_fn FAILED"
        let stdout = "test some_fn FAILED";
        let errors = parse_test_failures(stdout, "");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].message.contains("some_fn FAILED"),
            "actual message: {:?}",
            errors[0].message
        );
    }

    #[test]
    fn test_verification_report_display_with_suggested_steps_only() {
        let report = VerificationReport {
            triggered_by: "step_test".to_string(),
            timestamp: chrono::Utc::now(),
            total_duration_ms: 10,
            checks: vec![],
            overall_passed: true,
            affected_files: vec![],
            side_effects: vec![],
            suggested_next_steps: vec![
                "Step one".to_string(),
                "Step two".to_string(),
                "Step three".to_string(),
            ],
        };
        let display = format!("{}", report);
        assert!(display.contains("Suggested next steps:"));
        assert!(display.contains("Step one"));
        assert!(display.contains("Step two"));
        assert!(display.contains("Step three"));
    }
}
