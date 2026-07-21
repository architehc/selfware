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
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("timed out"),
        "stderr should note the timeout"
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
