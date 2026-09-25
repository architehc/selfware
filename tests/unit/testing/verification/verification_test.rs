use super::*;

#[tokio::test]
async fn run_reaped_times_out_and_does_not_report_success() {
    let dir = tempfile::tempdir().unwrap();
    // A command that runs far longer than the 1s timeout must be killed and
    // reported as not-successful (not hang the verifier forever).
    let start = std::time::Instant::now();
    let out = run_reaped("sleep", &["30"], dir.path(), 1).await.unwrap();
    assert!(
        start.elapsed().as_secs() < 10,
        "should return at the timeout"
    );
    assert!(!out.success, "a timed-out check must not report success");
    assert!(out.timed_out, "timed-out runs must carry the flag");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("timed out"),
        "stderr should note the timeout"
    );
}

/// Regression (review finding P1): verification executes project-controlled
/// programs/linters, so run_reaped children must NOT inherit host
/// credentials. A stub child dumps its env to a file; the synthetic marker
/// must be absent while the shared allowlist (PATH) still reaches it.
#[tokio::test]
async fn run_reaped_drops_inherited_secrets() {
    let _env = crate::test_support::EnvGuard::capture(&["SELFWARE_VERIFY_MARKER"]);
    _env.set("SELFWARE_VERIFY_MARKER", "synthetic-leak-marker");

    let dir = tempfile::tempdir().unwrap();
    let outfile = dir.path().join("child.env");
    let script = format!("env > {}", outfile.display());

    let out = run_reaped("sh", &["-c", &script], dir.path(), 5)
        .await
        .expect("sh -c env must run");
    assert!(out.success, "sh -c env must succeed: {:?}", out.stderr);

    let child_env = std::fs::read_to_string(&outfile).expect("stub child must dump its env");
    assert!(
        !child_env.contains("SELFWARE_VERIFY_MARKER"),
        "synthetic marker leaked to the verification child; saw:\n{child_env}"
    );
    assert!(
        child_env.contains("PATH="),
        "the shared allowlist (PATH) must still reach the child; saw:\n{child_env}"
    );
}

/// Regression (review finding P2): a timed-out verification command must
/// terminate its ENTIRE process group — a backgrounded grandchild
/// (`sleep 30 &`) must be killed, not orphaned to keep holding target/ locks.
#[tokio::test]
#[cfg(unix)]
async fn run_reaped_timeout_reaps_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let pidfile = dir.path().join("gc.pid");

    let start = std::time::Instant::now();
    let script = format!("sleep 30 & echo $! > {}; wait", pidfile.display());
    let out = run_reaped("sh", &["-c", &script], dir.path(), 1)
        .await
        .unwrap();
    assert!(
        start.elapsed().as_secs() < 10,
        "should return at the timeout"
    );
    assert!(out.timed_out, "must be reported as a timeout");

    let gc_pid: i32 = std::fs::read_to_string(&pidfile)
        .expect("grandchild wrote its pid")
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
/// otherwise leave collection awaiting EOF forever — verification would
/// stall past its timeout. `sleep 30 & echo done`: the child exits
/// immediately, the sleeper retains the pipe, and run_reaped must still
/// return in bounded time (reporting the drain timeout honestly).
///
/// Regression (follow-up P1): a drain timeout must be FAIL-CLOSED — the
/// parent exited 0 here, yet the verdict must not be success, because the
/// output was never collected and the group was killed.
#[tokio::test]
#[cfg(unix)]
async fn run_reaped_collection_bounded_when_pipe_held() {
    let dir = tempfile::tempdir().unwrap();

    let start = std::time::Instant::now();
    let out = run_reaped("sh", &["-c", "sleep 30 & echo done"], dir.path(), 5)
        .await
        .unwrap();
    assert!(
        start.elapsed().as_secs() < 10,
        "run_reaped must return in bounded time even when a pipe-holding grandchild lingers"
    );
    assert!(
        out.timed_out,
        "collection exceeded the deadline, so the run must be reported as timed out"
    );
    assert!(
        !out.success,
        "a timed-out run must NEVER report success, even though the parent exited 0"
    );
}

#[tokio::test]
async fn run_reaped_captures_a_normal_command() {
    let dir = tempfile::tempdir().unwrap();
    let out = run_reaped("printf", &["hello"], dir.path(), 5)
        .await
        .unwrap();
    assert!(out.success);
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello");
}

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
            not_run: false,
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
        not_run: false,
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
            not_run: false,
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
        not_run: false,
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
        not_run: false,
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
                not_run: false,
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 1000,
                output: "ok".to_string(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                not_run: false,
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
    assert!(!gate.should_run_custom_check(&check, &["main.rs".to_string(), "lib.rs".to_string()]));
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
                not_run: false,
                check_type: CheckType::TypeCheck,
                passed: true,
                duration_ms: 1000,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                not_run: false,
                check_type: CheckType::Format,
                passed: true,
                duration_ms: 200,
                output: String::new(),
                errors: vec![],
                warnings: vec![],
                suggestions: vec![],
            },
            CheckResult {
                not_run: false,
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
            not_run: false,
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
    // Python files now get language-specific checks (type check runs when check_on_edit is true)
    // The checks may pass or fail depending on environment, but the report should be valid
    assert!(!report.triggered_by.is_empty());
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
        not_run: false,
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
        not_run: false,
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
            not_run: false,
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 0,
            output: String::new(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        },
        CheckResult {
            not_run: false,
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
            not_run: false,
            check_type: CheckType::TypeCheck,
            passed: true,
            duration_ms: 0,
            output: String::new(),
            errors: vec![],
            warnings: vec![],
            suggestions: vec![],
        },
        CheckResult {
            not_run: false,
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

#[test]
fn test_file_hash_cache_detects_changes() {
    let config = VerificationConfig::default();
    let mut gate = VerificationGate::new(".", config);

    // Initially, cache is empty, so files should be considered changed
    assert!(gate.have_files_changed(&["src/lib.rs".to_string()]));

    // Simulate a verification by updating the cache
    // Note: We can't actually read files in this test, so we'll manually populate
    gate.file_hash_cache
        .insert("src/lib.rs".to_string(), 12345u64);

    // Now if we check the same file with same hash, it should not be changed
    // But since we can't actually compute the hash, we'll just test the logic
    // The real hash won't match 12345, so it will still report changed
    assert!(gate.have_files_changed(&["src/lib.rs".to_string()]));
}

#[test]
fn test_file_hash_cache_empty_returns_changed() {
    let config = VerificationConfig::default();
    let gate = VerificationGate::new(".", config);

    // Empty cache should always return true (files changed)
    assert!(gate.have_files_changed(&["src/main.rs".to_string()]));
    assert!(gate.have_files_changed(&["Cargo.toml".to_string()]));
}

#[tokio::test]
async fn test_verify_change_uses_cache_on_unchanged_files() {
    let temp = tempfile::tempdir().unwrap();
    let src_dir = temp.path().join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(src_dir.join("lib.rs"), "pub fn answer() -> i32 { 42 }\n").unwrap();

    let config = VerificationConfig {
        check_on_edit: false,
        test_on_edit: false,
        lint_on_edit: false,
        format_on_edit: false,
        ..Default::default()
    };
    let mut gate = VerificationGate::new(temp.path(), config);

    // First verification with a file
    let report1 = gate
        .verify_change(&["src/lib.rs".to_string()], "first")
        .await
        .unwrap();

    // Verify the file hash was cached
    assert!(gate.file_hash_cache.contains_key("src/lib.rs"));

    // Second verification with same file (will detect as changed because
    // we can't actually read the file in this test, but the cache mechanism is tested)
    let report2 = gate
        .verify_change(&["src/lib.rs".to_string()], "second")
        .await
        .unwrap();

    // Both should pass
    assert!(report1.overall_passed);
    assert!(report2.overall_passed);
}

#[test]
fn infer_repo_language_from_manifests() {
    let tmp = std::env::temp_dir().join(format!(
        "selfware_verify_manifest_test_{}",
        std::process::id()
    ));

    // Python via setup.py
    let py_dir = tmp.join("python_repo");
    std::fs::create_dir_all(&py_dir).unwrap();
    std::fs::write(py_dir.join("setup.py"), "from setuptools import setup\n").unwrap();
    let mut gate = VerificationGate::new(&py_dir, VerificationConfig::default());
    assert_eq!(gate.infer_repo_language(), RepoLanguage::Python);

    // TypeScript via package.json + tsconfig.json
    let ts_dir = tmp.join("ts_repo");
    std::fs::create_dir_all(&ts_dir).unwrap();
    std::fs::write(ts_dir.join("package.json"), "{}").unwrap();
    std::fs::write(ts_dir.join("tsconfig.json"), "{}").unwrap();
    let mut gate = VerificationGate::new(&ts_dir, VerificationConfig::default());
    assert_eq!(gate.infer_repo_language(), RepoLanguage::TypeScript);

    // Go via go.mod
    let go_dir = tmp.join("go_repo");
    std::fs::create_dir_all(&go_dir).unwrap();
    std::fs::write(go_dir.join("go.mod"), "module example\n").unwrap();
    let mut gate = VerificationGate::new(&go_dir, VerificationConfig::default());
    assert_eq!(gate.infer_repo_language(), RepoLanguage::Go);

    // Rust via Cargo.toml
    let rs_dir = tmp.join("rust_repo");
    std::fs::create_dir_all(&rs_dir).unwrap();
    std::fs::write(rs_dir.join("Cargo.toml"), "[package]\n").unwrap();
    let mut gate = VerificationGate::new(&rs_dir, VerificationConfig::default());
    assert_eq!(gate.infer_repo_language(), RepoLanguage::Rust);

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn infer_repo_language_from_extensions() {
    let tmp = std::env::temp_dir().join(format!("selfware_verify_ext_test_{}", std::process::id()));
    let py_dir = tmp.join("py_ext_repo");
    std::fs::create_dir_all(&py_dir).unwrap();
    std::fs::write(py_dir.join("main.py"), "print('hello')\n").unwrap();
    std::fs::write(py_dir.join("lib.py"), "def foo(): pass\n").unwrap();
    std::fs::write(py_dir.join("README.md"), "# hi\n").unwrap();

    let mut gate = VerificationGate::new(&py_dir, VerificationConfig::default());
    assert_eq!(gate.infer_repo_language(), RepoLanguage::Python);

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn infer_repo_language_from_hint() {
    let tmp =
        std::env::temp_dir().join(format!("selfware_verify_hint_test_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut gate = VerificationGate::new(&tmp, VerificationConfig::default());
    gate.set_repo_language_hint("go");
    assert_eq!(gate.infer_repo_language(), RepoLanguage::Go);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn cheap_syntax_check_python() {
    let tmp = std::env::temp_dir().join(format!(
        "selfware_verify_py_syntax_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    let good_py = tmp.join("good.py");
    std::fs::write(&good_py, "def hello():\n    print('world')\n").unwrap();

    let gate = VerificationGate::new(&tmp, VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Python, &["good.py".to_string()])
        .await
        .unwrap();
    assert!(result.passed, "valid python should pass: {}", result.output);

    let bad_py = tmp.join("bad.py");
    std::fs::write(&bad_py, "def hello(\n    print 'world'\n").unwrap();
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Python, &["bad.py".to_string()])
        .await
        .unwrap();
    assert!(!result.passed, "invalid python should fail");
    assert!(
        !result.output.is_empty(),
        "error output should contain details"
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn classify_rustfmt_failure_pure_diff_is_formatting() {
    // A pure `rustfmt --check` diff means the code PARSES — the failure is a
    // formatting difference, not a syntax error (finding C).
    let out = "Diff in /tmp/unformatted.rs:1:\n-fn main(){println!(\"hi\");}\n+fn main() {\n+    println!(\"hi\");\n+}\n \n";
    assert_eq!(
        classify_rustfmt_failure(out),
        RustfmtFailureKind::FormattingDiff
    );
}

#[test]
fn classify_rustfmt_failure_parse_error_is_syntax_failure() {
    // A genuine parse error must NEVER classify as a formatting diff — the
    // invariant that a real syntax error stays red is the whole point of the
    // classifier (finding C).
    let out = "error: expected expression, found `;`\n --> /tmp/broken.rs:2:13\n  |\n2 |     let x = ;\n  |             ^ expected expression\n";
    assert_eq!(
        classify_rustfmt_failure(out),
        RustfmtFailureKind::SyntaxFailure
    );
}

#[test]
fn classify_rustfmt_failure_mixed_diff_and_error_is_syntax_failure() {
    // A run touching several files can print a parse error for one file and a
    // diff for another; the error line must dominate (fail-closed).
    let out = "error: expected expression, found `;`\n --> /tmp/a.rs:2:13\n\nDiff in /tmp/b.rs:1:\n-fn main(){}\n+fn main() {}\n";
    assert_eq!(
        classify_rustfmt_failure(out),
        RustfmtFailureKind::SyntaxFailure
    );
}

#[test]
fn classify_rustfmt_failure_operational_or_empty_is_syntax_failure() {
    // Missing/unreadable files and unclassifiable failures stay blocking.
    assert_eq!(
        classify_rustfmt_failure("Error: file `nonexistent.rs` does not exist"),
        RustfmtFailureKind::SyntaxFailure
    );
    assert_eq!(
        classify_rustfmt_failure(""),
        RustfmtFailureKind::SyntaxFailure
    );
    assert_eq!(
        classify_rustfmt_failure("rustfmt: could not read some files"),
        RustfmtFailureKind::SyntaxFailure
    );
}

#[test]
fn classify_rustfmt_failure_missing_component_is_tool_unavailable() {
    // W7b finding 4: a rustup shim without the rustfmt component never ran
    // the tool — that is check-not-run, NEVER a syntax failure. The
    // error-line arm used to catch it and block verification of valid code.
    assert_eq!(
        classify_rustfmt_failure(
            "error: toolchain 'nightly-2026-09-01-aarch64-apple-darwin' does not have component 'rustfmt'"
        ),
        RustfmtFailureKind::ToolUnavailable
    );
    assert_eq!(
        classify_rustfmt_failure(
            "error: rustfmt is not installed for the toolchain 'stable-aarch64-apple-darwin'\nTo install, run `rustup component add rustfmt`"
        ),
        RustfmtFailureKind::ToolUnavailable
    );
    assert_eq!(
        classify_rustfmt_failure(
            "error: 'rustfmt' is not installed for the toolchain 'stable'\nTo install, run `rustup component add rustfmt`"
        ),
        RustfmtFailureKind::ToolUnavailable
    );
}

#[test]
fn rustfmt_tool_unavailable_result_reports_check_not_run() {
    // The result must say the check did not run (advisory), not that the
    // code failed — and it must not drag the report red.
    let result = rustfmt_unavailable_result(
        RepoLanguage::Rust,
        7,
        "error: toolchain 'x' does not have component 'rustfmt'",
    );
    assert!(
        result.passed,
        "a tool that never ran asserts no failure: {:?}",
        result.errors
    );
    assert!(
        result.errors.is_empty(),
        "no syntax errors were found because none were checked: {:?}",
        result.errors
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("not installed") || w.contains("unavailable")),
        "the warning must name the missing tool: {:?}",
        result.warnings
    );
    assert!(
        result.output.contains("could not run"),
        "the output must state the check did not run: {}",
        result.output
    );
}

#[test]
fn classify_rustfmt_failure_shim_error_does_not_mask_real_errors() {
    // A shim line plus a genuine parse error stays a syntax failure.
    let out = "error: expected expression, found `;`\n --> /tmp/broken.rs:2:13\n";
    assert_eq!(
        classify_rustfmt_failure(out),
        RustfmtFailureKind::SyntaxFailure
    );
}

/// When rustfmt cannot run on this machine — no binary at all
/// (not-run: "`rustfmt` not found"), or a rustup shim whose toolchain lacks
/// the component (e.g. CI's MSRV toolchain) — assert the honest "check did
/// not run" shape instead of the formatter verdicts, and return true so the
/// caller stops. With rustfmt present this returns false and the caller's
/// full assertions run unchanged.
fn rustfmt_absent_path_asserted(result: &CheckResult) -> bool {
    if result.not_run && result.output.contains("`rustfmt` not found") {
        eprintln!("rustfmt not installed — asserting the not-run path");
        assert_eq!(result.check_type, CheckType::TypeCheck);
        assert!(result.errors.is_empty(), "{:?}", result.errors);
        return true;
    }
    if result
        .warnings
        .iter()
        .any(|w| w.contains("rustfmt unavailable"))
    {
        eprintln!("rustfmt component missing — asserting the tool-unavailable path");
        assert!(
            result.passed,
            "an unavailable formatter is advisory, not a failure: {}",
            result.output
        );
        assert!(
            result.errors.is_empty(),
            "no verdict may be claimed when rustfmt never ran: {:?}",
            result.errors
        );
        assert!(
            result.output.contains("could not run"),
            "the output must say the check did not run: {}",
            result.output
        );
        assert!(
            result
                .suggestions
                .iter()
                .any(|s| s.contains("rustup component add rustfmt")),
            "the fix must be suggested: {:?}",
            result.suggestions
        );
        return true;
    }
    false
}

#[tokio::test]
async fn cheap_syntax_check_rust_formatting_diff_is_advisory() {
    // Finding C: `rustfmt --check` exits 1 for formatting diffs too, so a run
    // over unformatted-but-VALID Rust used to fail the syntax gate and block
    // verification even though every test passed. It must now PASS the syntax
    // gate and surface the formatting as an advisory note instead.
    let tmp = std::env::temp_dir().join(format!(
        "selfware_verify_rust_fmt_syntax_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("unformatted.rs"), "fn main(){println!(\"hi\");}\n").unwrap();

    let gate = VerificationGate::new(&tmp, VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["unformatted.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    }

    assert!(
        result.passed,
        "unformatted-but-valid Rust must NOT fail the syntax gate: {}",
        result.output
    );
    assert_eq!(result.check_type, CheckType::TypeCheck);
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.code.as_deref() == Some("FORMATTING_DIFF")),
        "the advisory FORMATTING_DIFF note must be present: {:?}",
        result.errors
    );
    assert!(
        result
            .errors
            .iter()
            .all(|e| !e.message.contains("syntax") || e.message.contains("not a syntax error")),
        "no check may claim a syntax failure for a formatting diff: {:?}",
        result.errors
    );
    assert!(
        result.warnings.iter().any(|w| w.contains("formatting")),
        "a warning must name the formatting difference: {:?}",
        result.warnings
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn cheap_syntax_check_rust_parse_error_still_fails() {
    // Invariant of the format/syntax split (finding C): a REAL parse error
    // must still fail the syntax gate with the syntax message — the split
    // never turns a syntax error green.
    let tmp = std::env::temp_dir().join(format!(
        "selfware_verify_rust_err_syntax_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("bad.rs"), "fn main() {\n    let x = ;\n}\n").unwrap();

    let gate = VerificationGate::new(&tmp, VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["bad.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    }

    assert!(
        !result.passed,
        "a genuine parse error must still fail the syntax gate: {}",
        result.output
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("syntax check failed")),
        "the failure message must still be a syntax failure: {:?}",
        result.errors
    );
    assert!(
        result
            .errors
            .iter()
            .all(|e| e.code.as_deref() != Some("FORMATTING_DIFF")),
        "a parse error must never be labeled a formatting diff: {:?}",
        result.errors
    );
    let _ = std::fs::remove_dir_all(&tmp);
}

#[tokio::test]
async fn targeted_test_command_python() {
    let tmp = std::env::temp_dir().join(format!(
        "selfware_verify_py_test_cmd_test_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&tmp).unwrap();

    // No pytest manifest - if pytest is installed it will be preferred,
    // otherwise falls back to unittest
    let gate = VerificationGate::new(&tmp, VerificationConfig::default());
    let cmd = gate.infer_test_command(RepoLanguage::Python).await;
    assert!(cmd.is_some());
    let (program, _args) = cmd.unwrap();
    assert!(
        program == "pytest" || program == "python3",
        "expected pytest or python3, got {}",
        program
    );

    // With pytest.ini → should use pytest
    std::fs::write(tmp.join("pytest.ini"), "[pytest]\n").unwrap();
    let gate = VerificationGate::new(&tmp, VerificationConfig::default());
    let cmd = gate.infer_test_command(RepoLanguage::Python).await;
    assert!(cmd.is_some());
    let (program, args) = cmd.unwrap();
    assert_eq!(program, "pytest");
    assert!(args.contains(&"--quiet".to_string()));

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn repo_language_from_extension_coverage() {
    assert_eq!(
        RepoLanguage::from_extension(".rs"),
        Some(RepoLanguage::Rust)
    );
    assert_eq!(
        RepoLanguage::from_extension(".py"),
        Some(RepoLanguage::Python)
    );
    assert_eq!(
        RepoLanguage::from_extension(".js"),
        Some(RepoLanguage::JavaScript)
    );
    assert_eq!(
        RepoLanguage::from_extension(".ts"),
        Some(RepoLanguage::TypeScript)
    );
    assert_eq!(RepoLanguage::from_extension(".go"), Some(RepoLanguage::Go));
    assert_eq!(RepoLanguage::from_extension(".txt"), None);
}

#[test]
fn repo_language_from_manifest_coverage() {
    assert_eq!(
        RepoLanguage::from_manifest("Cargo.toml"),
        Some(RepoLanguage::Rust)
    );
    assert_eq!(
        RepoLanguage::from_manifest("pyproject.toml"),
        Some(RepoLanguage::Python)
    );
    assert_eq!(
        RepoLanguage::from_manifest("package.json"),
        Some(RepoLanguage::JavaScript)
    );
    assert_eq!(
        RepoLanguage::from_manifest("go.mod"),
        Some(RepoLanguage::Go)
    );
    assert_eq!(RepoLanguage::from_manifest("random.txt"), None);
}

#[test]
fn language_check_set_default() {
    let set = LanguageCheckSet::default();
    assert!(set.syntax);
    assert!(set.format);
    assert!(set.lint);
    assert!(set.test);
}

#[test]
fn verification_config_language_settings_roundtrip() {
    let mut config = VerificationConfig::default();
    let mut settings = std::collections::HashMap::new();
    settings.insert(
        RepoLanguage::Python,
        LanguageCheckSet {
            syntax: true,
            format: false,
            lint: false,
            test: true,
        },
    );
    config.language_settings = settings;

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: VerificationConfig = serde_json::from_str(&json).unwrap();
    assert!(deserialized
        .language_settings
        .contains_key(&RepoLanguage::Python));
    let py = deserialized
        .language_settings
        .get(&RepoLanguage::Python)
        .unwrap();
    assert!(py.syntax);
    assert!(!py.format);
    assert!(!py.lint);
    assert!(py.test);
}

#[tokio::test]
async fn test_post_edit_test_command_passes() {
    let tmp = tempfile::tempdir().unwrap();
    let config = VerificationConfig {
        check_on_edit: false,
        test_on_edit: false,
        lint_on_edit: false,
        format_on_edit: false,
        post_edit_test_command: Some("echo post_edit_ok".to_string()),
        ..Default::default()
    };
    let mut gate = VerificationGate::new(tmp.path(), config);
    let report = gate
        .verify_change(&["script.py".to_string()], "post_edit_pass_trigger")
        .await
        .unwrap();
    let post_check = report
        .checks
        .iter()
        .find(|c| c.check_type == CheckType::Test);
    assert!(post_check.is_some(), "post-edit test check should run");
    assert!(post_check.unwrap().passed);
    assert!(post_check.unwrap().output.contains("post_edit_ok"));
    assert!(report.overall_passed);
}

#[tokio::test]
async fn test_post_edit_test_command_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let config = VerificationConfig {
        check_on_edit: false,
        test_on_edit: false,
        lint_on_edit: false,
        format_on_edit: false,
        post_edit_test_command: Some("false".to_string()),
        ..Default::default()
    };
    let mut gate = VerificationGate::new(tmp.path(), config);
    let report = gate
        .verify_change(&["script.py".to_string()], "post_edit_fail_trigger")
        .await
        .unwrap();
    let post_check = report
        .checks
        .iter()
        .find(|c| c.check_type == CheckType::Test)
        .expect("post-edit test check should be present");
    assert!(!post_check.passed);
    assert!(!report.overall_passed);
    assert!(report
        .suggested_next_steps
        .iter()
        .any(|s| s.contains("post-edit test command failed")));
}

#[test]
fn test_resolve_file_path_prioritizes_cwd_when_file_exists() {
    let parent_dir = tempfile::tempdir().unwrap();
    let child_dir = parent_dir.path().join("child");
    std::fs::create_dir_all(&child_dir).unwrap();

    let child_file = child_dir.join("calc.py");
    std::fs::write(&child_file, "def add(a, b): return a + b\n").unwrap();

    let gate = VerificationGate::new(parent_dir.path(), VerificationConfig::fast())
        .with_working_dir(&child_dir);
    let resolved = gate.resolve_file_path("calc.py");

    assert_eq!(resolved, child_file);
}

#[test]
fn test_resolve_file_path_falls_back_to_project_root() {
    let parent_dir = tempfile::tempdir().unwrap();
    let parent_file = parent_dir.path().join("root.py");
    std::fs::write(&parent_file, "print('root')\n").unwrap();

    let child_dir = parent_dir.path().join("child");
    std::fs::create_dir_all(&child_dir).unwrap();

    let gate = VerificationGate::new(parent_dir.path(), VerificationConfig::fast())
        .with_working_dir(&child_dir);
    let resolved = gate.resolve_file_path("root.py");

    assert_eq!(resolved, parent_file);
}

#[tokio::test]
async fn test_nested_project_syntax_check_runs_in_child_dir() {
    let parent_dir = tempfile::tempdir().unwrap();
    let child_dir = parent_dir.path().join("child_sub");
    std::fs::create_dir_all(&child_dir).unwrap();

    let child_file = child_dir.join("math_lib.py");
    std::fs::write(&child_file, "def mul(a, b): return a * b\n").unwrap();

    let mut gate = VerificationGate::new(parent_dir.path(), VerificationConfig::fast())
        .with_working_dir(&child_dir);
    let report = gate
        .verify_change(&["math_lib.py".to_string()], "test")
        .await
        .unwrap();

    assert!(report.overall_passed);
    let type_check = report
        .checks
        .iter()
        .find(|c| c.check_type == CheckType::TypeCheck);
    assert!(type_check.is_some());
    assert!(type_check.unwrap().passed);
}

/// Regression: a drain timeout must not discard output the OTHER stream
/// already captured. The parent prints to stdout and exits (stdout EOFs)
/// while a backgrounded sleeper keeps only STDERR open past the deadline:
/// the run is fail-closed (timed out, not success) but keeps the stdout text.
#[tokio::test]
#[cfg(unix)]
async fn run_reaped_drain_timeout_keeps_captured_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let out = run_reaped(
        "sh",
        &[
            "-c",
            "echo early-stdout-marker; sleep 30 >/dev/null & exit 0",
        ],
        dir.path(),
        1,
    )
    .await
    .unwrap();
    assert!(
        start.elapsed().as_secs() < 10,
        "must return in bounded time"
    );
    assert!(out.timed_out, "the stderr drain exceeded the deadline");
    assert!(!out.success, "a timed-out run must not report success");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("early-stdout-marker"),
        "stdout captured before the stderr drain timed out must be kept: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("timed out"),
        "stderr should note the timeout"
    );
}

/// Temp crate `<tmp>/Cargo.toml` + `src/lib.rs` for the edition-aware
/// syntax-check tests. Returns the tempdir guard.
fn rust_crate_fixture(edition_line: &str, lib_rs: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        format!("[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n{edition_line}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), lib_rs).unwrap();
    tmp
}

#[tokio::test]
async fn cheap_syntax_check_rust_async_fn_passes_under_crate_edition() {
    // Regression (expert_async_race): rustfmt run without --edition parsed
    // as Rust 2015 and rejected `async fn` (E0670) — a false syntax failure
    // that failed verification although cargo test and clippy passed.
    for edition in ["2018", "2021", "2024"] {
        let tmp = rust_crate_fixture(
            &format!("edition = \"{edition}\""),
            "pub async fn race() -> u32 {\n    1\n}\n",
        );
        let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
        let result = gate
            .run_cheap_syntax_check(RepoLanguage::Rust, &["src/lib.rs".to_string()])
            .await
            .unwrap();
        if rustfmt_absent_path_asserted(&result) {
            return;
        }
        assert!(
            result.passed,
            "async fn in an edition {edition} crate must pass: {}",
            result.output
        );
        assert!(
            result.output.contains(&format!("edition {edition}")),
            "the output must name the edition parsed as: {}",
            result.output
        );
        assert!(
            result.warnings.iter().all(|w| !w.contains("fallback")),
            "a manifest edition is not a fallback: {:?}",
            result.warnings
        );
    }
}

#[tokio::test]
async fn cheap_syntax_check_rust_explicit_2015_crate_still_rejects_async_fn() {
    // The edition is really passed through: a crate that declares 2015 gets
    // the 2015 parser, which rejects `async fn`.
    let tmp = rust_crate_fixture("edition = \"2015\"", "pub async fn f() {}\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["src/lib.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(
        !result.passed,
        "2015 must reject async fn: {}",
        result.output
    );
}

#[tokio::test]
async fn cheap_syntax_check_rust_parse_error_fails_in_2021_crate() {
    let tmp = rust_crate_fixture(
        "edition = \"2021\"",
        "pub async fn f() {\n    let x = ;\n}\n",
    );
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["src/lib.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(
        !result.passed,
        "a real parse error must fail: {}",
        result.output
    );
    assert!(
        result
            .errors
            .iter()
            .any(|e| e.message.contains("syntax check failed")),
        "{:?}",
        result.errors
    );
}

#[tokio::test]
async fn cheap_syntax_check_rust_workspace_inherited_edition() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"m\"]\n[workspace.package]\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.path().join("m/src")).unwrap();
    std::fs::write(
        tmp.path().join("m/Cargo.toml"),
        "[package]\nname = \"m\"\nversion = \"0.1.0\"\nedition.workspace = true\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("m/src/lib.rs"), "pub async fn f() {}\n").unwrap();
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["m/src/lib.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(result.passed, "{}", result.output);
    assert!(
        result.output.contains("workspace.package.edition"),
        "{}",
        result.output
    );
}

#[tokio::test]
async fn cheap_syntax_check_rust_no_manifest_uses_reported_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("loose.rs"), "async fn f() {}\n").unwrap();
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["loose.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(result.passed, "{}", result.output);
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("fallback") && w.contains("2021")),
        "a guessed edition must be surfaced: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn cheap_syntax_check_rust_checks_edited_file_alone() {
    // `mod child;` pointing at a broken (or not-yet-created) module must not
    // fail the edited parent file: each touched file is checked on its own.
    let tmp = rust_crate_fixture(
        "edition = \"2021\"",
        "mod child;\nmod not_created_yet;\npub async fn f() {}\n",
    );
    std::fs::write(tmp.path().join("src/child.rs"), "fn g() { let x = ; }\n").unwrap();
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["src/lib.rs".to_string()])
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(result.passed, "{}", result.output);

    // The broken child still fails when it is itself a touched file.
    let result = gate
        .run_cheap_syntax_check(RepoLanguage::Rust, &["src/child.rs".to_string()])
        .await
        .unwrap();
    assert!(!result.passed, "{}", result.output);
}

#[tokio::test]
async fn cheap_syntax_check_rust_mixed_edition_files() {
    // Two crates with different editions in one edit: one rustfmt run per
    // edition; a 2015 crate's `async` identifier and a 2021 crate's
    // `async fn` both parse.
    let root = tempfile::tempdir().unwrap();
    for (name, edition, src) in [
        (
            "old",
            "2015",
            "pub fn f() { let async = 1; let _ = async; }\n",
        ),
        ("new", "2021", "pub async fn f() {}\n"),
    ] {
        std::fs::create_dir_all(root.path().join(name).join("src")).unwrap();
        std::fs::write(
            root.path().join(name).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"{edition}\"\n"),
        )
        .unwrap();
        std::fs::write(root.path().join(name).join("src/lib.rs"), src).unwrap();
    }
    let gate = VerificationGate::new(root.path(), VerificationConfig::default());
    let result = gate
        .run_cheap_syntax_check(
            RepoLanguage::Rust,
            &["old/src/lib.rs".to_string(), "new/src/lib.rs".to_string()],
        )
        .await
        .unwrap();
    if rustfmt_absent_path_asserted(&result) {
        return;
    }
    assert!(result.passed, "{}", result.output);
    assert!(result.output.contains("edition 2015"), "{}", result.output);
    assert!(result.output.contains("edition 2021"), "{}", result.output);
}

// ─── Cheap syntax checks: project language level, honest not-run ───
//
// Integration tests below exercise the real toolchain when it is installed
// and SKIP (with a note) otherwise; the resolvers themselves are covered
// toolchain-free in tests/unit/testing/syntax_toolchain.

fn write_file(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(p, text).unwrap();
}

async fn tool_missing(gate: &VerificationGate, tool: &str) -> bool {
    if gate.command_exists(tool).await {
        return false;
    }
    eprintln!("`{tool}` not installed — skipping toolchain integration assertions");
    true
}

/// A not-run result is non-blocking but claims nothing.
fn assert_not_run_shape(r: &CheckResult) {
    assert!(r.not_run, "expected not-run: {}", r.output);
    assert!(r.passed, "not-run must not block: {}", r.output);
    assert!(
        r.errors.is_empty(),
        "not-run claims no verdict: {:?}",
        r.errors
    );
    assert!(r.output.contains("could not run"), "{}", r.output);
    assert!(
        r.warnings.iter().any(|w| w.contains("NOT RUN")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn syntax_tally_outcomes_are_honest() {
    let files = vec!["a".to_string()];
    // Nothing ran → not-run, never a pass.
    let mut t = SyntaxTally::new(RepoLanguage::Java, &files);
    t.record(ToolRun::NotRun("`javac` not found on PATH".into()));
    assert_not_run_shape(&t.finish(1));

    // Partial: a pass that names the part that did not run.
    let mut t = SyntaxTally::new(RepoLanguage::JavaScript, &files);
    t.record(ToolRun::Done {
        success: true,
        output: String::new(),
    });
    t.record(ToolRun::NotRun("no tsc for JSX".into()));
    let r = t.finish(1);
    assert!(r.passed && !r.not_run);
    assert!(r.output.contains("NOT RUN for 1 part(s)"), "{}", r.output);
    assert!(r.warnings.iter().any(|w| w.contains("no tsc for JSX")));

    // Any failure fails, even alongside not-run parts.
    let mut t = SyntaxTally::new(RepoLanguage::Cpp, &files);
    t.record(ToolRun::NotRun("x".into()));
    t.record(ToolRun::Done {
        success: false,
        output: "a.cpp:1:1: error: boom".into(),
    });
    let r = t.finish(1);
    assert!(!r.passed && !r.not_run);
    assert!(r.errors[0]
        .message
        .contains("syntax check failed: a.cpp:1:1: error: boom"));
}

#[test]
fn report_display_marks_not_run_checks() {
    let report = VerificationReport {
        triggered_by: "edit".into(),
        timestamp: chrono::Utc::now(),
        total_duration_ms: 1,
        checks: vec![syntax_not_run(
            RepoLanguage::Go,
            "`gofmt` not found on PATH",
            0,
        )],
        overall_passed: true,
        affected_files: vec![],
        side_effects: vec![],
        suggested_next_steps: vec![],
    };
    let s = report.to_string();
    assert!(s.contains("○ type_check: not run ("), "{s}");
    assert!(s.contains("gofmt"), "the not-run reason is rendered: {s}");
    assert!(
        s.contains("NOT VERIFIED"),
        "an all-not-run report must not say PASSED: {s}"
    );
    assert!(
        !s.contains("✓ type_check"),
        "a not-run check must not render green: {s}"
    );
}

#[tokio::test]
async fn cheap_syntax_check_unknown_language_is_not_run() {
    let tmp = tempfile::tempdir().unwrap();
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::Unknown, &["x.zz".to_string()])
        .await
        .unwrap();
    assert_not_run_shape(&r);
}

#[tokio::test]
async fn cheap_syntax_check_python_never_writes_bytecode() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(tmp.path(), "pkg/good.py", "def f():\n    return 1\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "python3").await {
        return;
    }
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::Python, &["pkg/good.py".to_string()])
        .await
        .unwrap();
    assert!(r.passed && !r.not_run, "{}", r.output);
    assert!(
        !tmp.path().join("pkg/__pycache__").exists(),
        "the syntax check must not write __pycache__ into the workspace"
    );
    assert!(r.output.contains("compiled by python"), "{}", r.output);
}

#[tokio::test]
async fn cheap_syntax_check_python_pin_newer_than_host_is_not_run_not_failure() {
    // The project requires a Python no host has; a (here: genuine) syntax
    // rejection by the older host is "could not verify", never a failure.
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "pyproject.toml",
        "[project]\nname = \"x\"\nrequires-python = \">=3.99\"\n",
    );
    write_file(tmp.path(), "m.py", "def f(:\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "python3").await {
        return;
    }
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::Python, &["m.py".to_string()])
        .await
        .unwrap();
    assert_not_run_shape(&r);
    assert!(
        r.output.contains("older than the project's Python 3.99"),
        "{}",
        r.output
    );
}

#[tokio::test]
async fn cheap_syntax_check_python_modern_syntax_under_pin() {
    // `match` (3.10+) with the project pinned to 3.10: either an interpreter
    // new enough parses it (pass), or the host is too old (not-run) — it is
    // never reported as a syntax failure.
    let tmp = tempfile::tempdir().unwrap();
    write_file(tmp.path(), ".python-version", "3.10\n");
    write_file(
        tmp.path(),
        "m.py",
        "def f(x):\n    match x:\n        case 1:\n            return 'one'\n        case _:\n            return 'other'\n",
    );
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "python3").await {
        return;
    }
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::Python, &["m.py".to_string()])
        .await
        .unwrap();
    assert!(r.passed, "modern syntax must not fail: {}", r.output);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

#[tokio::test]
async fn cheap_syntax_check_javascript_esm_and_every_file() {
    let tmp = tempfile::tempdir().unwrap();
    // Explicit commonjs package: node would reject `import` as a syntax
    // error; the gate parses ESM-shaped files as modules.
    write_file(tmp.path(), "package.json", r#"{"type":"commonjs"}"#);
    write_file(
        tmp.path(),
        "esm.js",
        "import fs from 'node:fs';\nexport const x = await Promise.resolve(fs);\n",
    );
    write_file(tmp.path(), "mod.mjs", "export default 1;\n");
    write_file(tmp.path(), "cjs.js", "module.exports = require('fs');\n");
    write_file(tmp.path(), "broken.js", "function (\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "node").await {
        return;
    }
    let good = ["esm.js", "mod.mjs", "cjs.js"].map(String::from);
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::JavaScript, &good)
        .await
        .unwrap();
    assert!(r.passed && !r.not_run, "{}", r.output);
    assert!(r.output.contains("parsed as an ES module"), "{}", r.output);

    // The broken file is LAST: every file is checked, not just the first.
    let mut with_bad = good.to_vec();
    with_bad.push("broken.js".to_string());
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::JavaScript, &with_bad)
        .await
        .unwrap();
    assert!(
        !r.passed,
        "a syntax error in a later file must fail: {}",
        r.output
    );
}

#[tokio::test]
async fn cheap_syntax_check_jsx_is_never_a_false_failure() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "App.jsx",
        "export default function App() { return <div className=\"a\">hi</div>; }\n",
    );
    write_file(tmp.path(), "Bad.jsx", "const a = <div>;\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::JavaScript, &["App.jsx".to_string()])
        .await
        .unwrap();
    if gate.resolve_tsc(tmp.path()).await.is_none() {
        // No JSX-aware parser: honest not-run (node cannot parse JSX).
        assert_not_run_shape(&r);
        return;
    }
    assert!(r.passed && !r.not_run, "{}", r.output);
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::JavaScript, &["Bad.jsx".to_string()])
        .await
        .unwrap();
    assert!(!r.passed, "unclosed JSX must fail: {}", r.output);
}

#[tokio::test]
async fn cheap_syntax_check_typescript_uses_project_tsconfig() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "tsconfig.json",
        r#"{
  // decorators + ES2022 lib: tsc's own defaults reject both
  "compilerOptions": { "target": "es2022", "experimentalDecorators": true, "strict": true, "composite": true, "outDir": "dist" },
  "include": ["src"],
}"#,
    );
    write_file(
        tmp.path(),
        "src/a.ts",
        "function d(_t: object, _k: string, _i: number) {}\nexport class A { m(@d x: number) { return x; } }\nexport const y = [1].at(-1);\n",
    );
    // A pre-existing error elsewhere in the project must not fail this edit.
    write_file(
        tmp.path(),
        "src/other.ts",
        "export const q: number = 's';\n",
    );
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if gate.resolve_tsc(tmp.path()).await.is_none() {
        let r = gate
            .run_cheap_syntax_check(RepoLanguage::TypeScript, &["src/a.ts".to_string()])
            .await
            .unwrap();
        assert_not_run_shape(&r);
        assert!(r.output.contains("npx is not used"), "{}", r.output);
        return;
    }
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::TypeScript, &["src/a.ts".to_string()])
        .await
        .unwrap();
    assert!(r.passed && !r.not_run, "{}", r.output);
    assert!(r.output.contains("project config"), "{}", r.output);
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(".selfware-syntax-check")
        })
        .collect();
    assert!(leftovers.is_empty(), "temporary tsconfig must be removed");
    assert!(!tmp.path().join("dist").exists(), "check must not emit");

    let r = gate
        .run_cheap_syntax_check(RepoLanguage::TypeScript, &["src/other.ts".to_string()])
        .await
        .unwrap();
    assert!(
        !r.passed,
        "a real type error in the edited file fails: {}",
        r.output
    );
}

#[tokio::test]
async fn cheap_syntax_check_typescript_without_tsconfig_uses_modern_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "a.ts",
        "export const y: number | undefined = [1].at(-1);\n",
    );
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if gate.resolve_tsc(tmp.path()).await.is_none() {
        eprintln!("tsc not installed — skipping");
        return;
    }
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::TypeScript, &["a.ts".to_string()])
        .await
        .unwrap();
    assert!(r.passed, "{}", r.output);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("fallback compiler options")),
        "the fallback must be reported: {:?}",
        r.warnings
    );
}

#[tokio::test]
async fn cheap_syntax_check_cpp_uses_project_standard() {
    let tmp = tempfile::tempdir().unwrap();
    let cxx20 = "#include <concepts>\ntemplate <typename T> concept Num = std::integral<T>;\nauto f(Num auto x) { return x; }\nstruct P { int a; int b; };\nP p{.a = 1, .b = 2};\n";
    write_file(tmp.path(), "loose/a.cpp", cxx20);
    write_file(
        tmp.path(),
        "proj/CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.20)\nproject(p CXX)\nset(CMAKE_CXX_STANDARD 20)\n",
    );
    write_file(tmp.path(), "proj/src/a.cpp", cxx20);
    write_file(tmp.path(), "proj/src/b.cpp", "int main() { return 0 }\n");
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "c++").await {
        return;
    }
    // No config: modern fallback, reported as such.
    let r = gate
        .run_cheap_syntax_check(RepoLanguage::Cpp, &["loose/a.cpp".to_string()])
        .await
        .unwrap();
    assert!(r.passed, "{}", r.output);
    assert!(
        r.warnings.iter().any(|w| w.contains("fallback standard")),
        "{:?}",
        r.warnings
    );

    // CMake project: the declared standard (this used to run `cmake --build .`
    // in the source dir, which fails on every edit).
    let proj = VerificationGate::new(tmp.path().join("proj"), VerificationConfig::default());
    let r = proj
        .run_cheap_syntax_check(RepoLanguage::Cpp, &["src/a.cpp".to_string()])
        .await
        .unwrap();
    assert!(r.passed && !r.not_run, "{}", r.output);
    assert!(r.output.contains("gnu++20"), "{}", r.output);
    let r = proj
        .run_cheap_syntax_check(
            RepoLanguage::Cpp,
            &["src/a.cpp".to_string(), "src/b.cpp".to_string()],
        )
        .await
        .unwrap();
    assert!(!r.passed, "every file is checked: {}", r.output);
}

#[tokio::test]
async fn cheap_syntax_check_java_release_and_clean_output() {
    let tmp = tempfile::tempdir().unwrap();
    write_file(
        tmp.path(),
        "pom.xml",
        "<project><properties><maven.compiler.release>17</maven.compiler.release></properties></project>",
    );
    write_file(
        tmp.path(),
        "src/main/java/com/x/Point.java",
        "package com.x;\npublic record Point(int x, int y) {}\n",
    );
    // References a sibling class: resolved through -sourcepath.
    write_file(
        tmp.path(),
        "src/main/java/com/x/Use.java",
        "package com.x;\npublic class Use {\n  String s = \"\"\"\n    text block\n    \"\"\";\n  Point p = new Point(1, 2);\n}\n",
    );
    write_file(
        tmp.path(),
        "src/main/java/com/x/Bad.java",
        "package com.x;\npublic class Bad { void f( }\n",
    );
    let gate = VerificationGate::new(tmp.path(), VerificationConfig::default());
    if tool_missing(&gate, "javac").await {
        return;
    }
    let r = gate
        .run_cheap_syntax_check(
            RepoLanguage::Java,
            &["src/main/java/com/x/Use.java".to_string()],
        )
        .await
        .unwrap();
    if r.not_run {
        // e.g. a macOS /usr/bin/javac stub with no JDK installed.
        assert_not_run_shape(&r);
        return;
    }
    assert!(r.passed, "{}", r.output);
    assert!(
        r.output.contains("--release 17") || r.warnings.iter().any(|w| w.contains("Java 17")),
        "the project release level must be applied or its absence reported: {} {:?}",
        r.output,
        r.warnings
    );
    let has_class = walk_files(tmp.path())
        .iter()
        .any(|p| p.extension().and_then(|e| e.to_str()) == Some("class"));
    assert!(!has_class, "javac output must not land in the workspace");
    let r = gate
        .run_cheap_syntax_check(
            RepoLanguage::Java,
            &["src/main/java/com/x/Bad.java".to_string()],
        )
        .await
        .unwrap();
    assert!(!r.passed, "{}", r.output);
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// QA not-run carry-through (0.8.2 validation D9 / D12)
// ---------------------------------------------------------------------------

#[test]
fn qa_not_run_stage_becomes_non_blocking_not_run_check_with_reason() {
    use crate::testing::qa_profiles::{QaStage, QaStageResult};
    let c = VerificationGate::qa_stage_to_check_result(QaStageResult::not_run(
        QaStage::Lint,
        "no ESLint configuration",
    ));
    assert!(c.not_run && c.passed, "not-run is non-blocking");
    assert!(c.errors.is_empty());
    assert!(
        c.warnings[0].contains("no ESLint configuration"),
        "{:?}",
        c.warnings
    );
}

#[test]
fn qa_failed_stage_keeps_its_finding() {
    use crate::testing::qa_profiles::{QaStage, QaStageResult};
    let c = VerificationGate::qa_stage_to_check_result(QaStageResult {
        stage: QaStage::Lint,
        passed: false,
        duration_ms: 3,
        output: "\nsrc/a.js 1:7 error 'x' is never used".into(),
        error_count: 1,
        warning_count: 0,
        not_run: None,
    });
    assert!(!c.passed && !c.not_run);
    assert!(
        c.errors[0].message.contains("'x' is never used"),
        "{:?}",
        c.errors
    );
}

#[test]
fn report_renders_not_run_reason_and_warnings() {
    use crate::testing::qa_profiles::{QaStage, QaStageResult};
    let mut ts = CheckResult {
        check_type: CheckType::TypeCheck,
        passed: true,
        not_run: false,
        duration_ms: 415,
        output: "TypeScript syntax check passed".into(),
        errors: vec![],
        warnings: vec!["TypeScript syntax check used fallback compiler options".into()],
        suggestions: vec![],
    };
    ts.warnings.push("second".into());
    let report = VerificationReport {
        triggered_by: "file_edit:src/a.ts".into(),
        timestamp: chrono::Utc::now(),
        total_duration_ms: 1,
        checks: vec![
            ts,
            VerificationGate::qa_stage_to_check_result(QaStageResult::not_run(
                QaStage::Lint,
                "no ESLint configuration",
            )),
        ],
        overall_passed: true,
        affected_files: vec![],
        side_effects: vec![],
        suggested_next_steps: vec![],
    };
    let s = report.to_string();
    assert!(s.contains("○ lint: not run ("), "{s}");
    assert!(s.contains("no ESLint configuration"), "{s}");
    assert!(s.contains("⚠ TypeScript syntax check used fallback"), "{s}");
    assert!(s.contains("PASSED (1 not run)"), "{s}");
    let caveats = report.caveats();
    assert_eq!(caveats.len(), 3, "{caveats:?}");
}

#[test]
fn targeted_test_that_found_nothing_is_not_run() {
    assert!(targeted_test_found_nothing(RepoLanguage::Python, Some(5), "").is_some());
    assert!(
        targeted_test_found_nothing(RepoLanguage::Python, Some(0), "Ran 0 tests\nOK").is_some()
    );
    assert!(targeted_test_found_nothing(RepoLanguage::Python, Some(1), "1 failed").is_none());
    assert!(
        targeted_test_found_nothing(RepoLanguage::Go, Some(0), "?\tm\t[no test files]").is_some()
    );
    assert!(targeted_test_found_nothing(RepoLanguage::Go, Some(0), "ok  \tm\t0.1s").is_none());
}

/// Rule-5 sweep: the targeted-test path had the same classes — an unknown
/// language rendered `✓ test` (a pass for nothing), a JS project without a
/// test script failed on "Missing script", and a missing runner aborted the
/// whole verification.
#[tokio::test]
async fn targeted_test_unconfigured_or_unrunnable_is_not_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("package.json"), r#"{"name":"x"}"#).unwrap();
    let gate = VerificationGate::new(dir.path(), VerificationConfig::default());

    let unknown = gate
        .run_targeted_test(RepoLanguage::Unknown, &[])
        .await
        .unwrap();
    assert!(unknown.not_run && unknown.passed, "{}", unknown.output);

    let js = gate
        .run_targeted_test(RepoLanguage::JavaScript, &["a.js".into()])
        .await
        .unwrap();
    assert!(js.not_run, "no test script → not run: {}", js.output);
    assert!(
        js.warnings[0].contains("no \"test\" script"),
        "{:?}",
        js.warnings
    );

    let err = anyhow::Error::new(SpawnFailed {
        program: "mvn".into(),
        source: std::io::Error::from(std::io::ErrorKind::NotFound),
    });
    let r = not_run_if_spawn_failed(CheckType::Test, &err).expect("spawn failure → not run");
    assert!(r.not_run && r.warnings[0].contains("`mvn` is not installed"));
    assert!(not_run_if_spawn_failed(CheckType::Test, &anyhow::anyhow!("other")).is_none());
}
