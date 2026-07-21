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
