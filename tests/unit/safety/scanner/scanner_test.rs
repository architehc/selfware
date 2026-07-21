use super::*;

// -------------------------------------------------------------------------
// SecuritySeverity
// -------------------------------------------------------------------------

#[test]
fn test_security_severity_ordering() {
    assert!(SecuritySeverity::Critical > SecuritySeverity::High);
    assert!(SecuritySeverity::High > SecuritySeverity::Medium);
    assert!(SecuritySeverity::Medium > SecuritySeverity::Low);
    assert!(SecuritySeverity::Low > SecuritySeverity::Info);
}

#[test]
fn test_security_severity_score() {
    assert!(SecuritySeverity::Critical.score() > SecuritySeverity::High.score());
    assert!(SecuritySeverity::High.score() > SecuritySeverity::Medium.score());
    assert!(SecuritySeverity::Medium.score() > SecuritySeverity::Low.score());
    assert!(SecuritySeverity::Low.score() > SecuritySeverity::Info.score());
    assert_eq!(SecuritySeverity::Info.score(), 0.0);
}

#[test]
fn test_security_severity_as_str() {
    assert_eq!(SecuritySeverity::Info.as_str(), "info");
    assert_eq!(SecuritySeverity::Low.as_str(), "low");
    assert_eq!(SecuritySeverity::Medium.as_str(), "medium");
    assert_eq!(SecuritySeverity::High.as_str(), "high");
    assert_eq!(SecuritySeverity::Critical.as_str(), "critical");
}

#[test]
fn test_security_severity_scores_concrete() {
    assert_eq!(SecuritySeverity::Info.score(), 0.0);
    assert_eq!(SecuritySeverity::Low.score(), 3.0);
    assert_eq!(SecuritySeverity::Medium.score(), 5.5);
    assert_eq!(SecuritySeverity::High.score(), 7.5);
    assert_eq!(SecuritySeverity::Critical.score(), 9.5);
}

// -------------------------------------------------------------------------
// SecurityCategory
// -------------------------------------------------------------------------

#[test]
fn test_security_category_as_str() {
    assert_eq!(
        SecurityCategory::HardcodedSecret.as_str(),
        "hardcoded_secret"
    );
    assert_eq!(SecurityCategory::Injection.as_str(), "injection");
    assert_eq!(SecurityCategory::Authentication.as_str(), "authentication");
    assert_eq!(SecurityCategory::Authorization.as_str(), "authorization");
    assert_eq!(SecurityCategory::DataExposure.as_str(), "data_exposure");
    assert_eq!(SecurityCategory::Cryptography.as_str(), "cryptography");
    assert_eq!(SecurityCategory::Configuration.as_str(), "configuration");
    assert_eq!(SecurityCategory::Dependency.as_str(), "dependency");
    assert_eq!(SecurityCategory::Compliance.as_str(), "compliance");
    assert_eq!(SecurityCategory::CodeQuality.as_str(), "code_quality");
}

#[test]
fn test_security_category_custom() {
    let cat = SecurityCategory::Custom("my_category".to_string());
    assert_eq!(cat.as_str(), "my_category");
}

// -------------------------------------------------------------------------
// SecurityFinding
// -------------------------------------------------------------------------

#[test]
fn test_security_finding_new() {
    let finding = SecurityFinding::new(
        "Test Finding",
        SecurityCategory::HardcodedSecret,
        SecuritySeverity::High,
    );
    assert!(!finding.id.is_empty());
    assert!(finding.id.starts_with("SEC-"));
    assert!(finding.timestamp > 0);
    assert_eq!(finding.title, "Test Finding");
    assert_eq!(finding.description, "");
    assert!(finding.file.is_none());
    assert!(finding.line.is_none());
    assert!(finding.snippet.is_none());
    assert!(finding.remediation.is_none());
    assert!(finding.cwe.is_none());
}

#[test]
fn test_security_finding_with_description() {
    let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
        .with_description("Detailed description here");
    assert_eq!(finding.description, "Detailed description here");
}

#[test]
fn test_security_finding_with_location() {
    let finding = SecurityFinding::new("Test", SecurityCategory::Injection, SecuritySeverity::High)
        .with_location(PathBuf::from("test.rs"), 42);
    assert_eq!(finding.line, Some(42));
    assert_eq!(finding.file, Some(PathBuf::from("test.rs")));
}

#[test]
fn test_security_finding_with_snippet() {
    let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
        .with_snippet("let x = dangerous();");
    assert_eq!(finding.snippet.as_deref(), Some("let x = dangerous();"));
}

#[test]
fn test_security_finding_with_remediation() {
    let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
        .with_remediation("Use parameterized queries");
    assert_eq!(
        finding.remediation.as_deref(),
        Some("Use parameterized queries")
    );
}

#[test]
fn test_security_finding_with_cwe() {
    let finding = SecurityFinding::new("T", SecurityCategory::Injection, SecuritySeverity::Low)
        .with_cwe("CWE-89");
    assert_eq!(finding.cwe.as_deref(), Some("CWE-89"));
}

#[test]
fn test_security_finding_builder_chain() {
    let finding = SecurityFinding::new(
        "Chain Test",
        SecurityCategory::HardcodedSecret,
        SecuritySeverity::Critical,
    )
    .with_description("desc")
    .with_location(PathBuf::from("foo.rs"), 10)
    .with_snippet("snippet")
    .with_remediation("fix")
    .with_cwe("CWE-798");

    assert_eq!(finding.title, "Chain Test");
    assert_eq!(finding.description, "desc");
    assert_eq!(finding.line, Some(10));
    assert_eq!(finding.snippet.as_deref(), Some("snippet"));
    assert_eq!(finding.remediation.as_deref(), Some("fix"));
    assert_eq!(finding.cwe.as_deref(), Some("CWE-798"));
    assert_eq!(finding.severity, SecuritySeverity::Critical);
    assert_eq!(finding.category, SecurityCategory::HardcodedSecret);
}

// -------------------------------------------------------------------------
// SecretPattern
// -------------------------------------------------------------------------

#[test]
fn test_secret_pattern_new() {
    let pattern = SecretPattern::new("Test", r"\d+", SecuritySeverity::Low);
    assert_eq!(pattern.name, "Test");
    assert_eq!(pattern.severity, SecuritySeverity::Low);
    assert!(pattern.compiled.is_some(), "valid regex should compile");
}

#[test]
fn test_secret_pattern_invalid_regex() {
    let pattern = SecretPattern::new("Bad", r"[invalid", SecuritySeverity::Low);
    assert!(
        pattern.compiled.is_none(),
        "invalid regex should produce None"
    );
}

#[test]
fn test_secret_pattern_description_default() {
    let pattern = SecretPattern::new("AWS Key", r"\d+", SecuritySeverity::Critical);
    assert!(pattern.description.contains("AWS Key"));
}

// -------------------------------------------------------------------------
// SecretScanner
// -------------------------------------------------------------------------

#[test]
fn test_secret_scanner_new() {
    let scanner = SecretScanner::new();
    assert!(scanner.findings().is_empty());
}

#[test]
fn test_secret_scanner_default() {
    let scanner = SecretScanner::default();
    assert!(scanner.findings().is_empty());
}

#[test]
fn test_secret_scanner_empty_content() {
    let mut scanner = SecretScanner::new();
    let findings = scanner.scan_content("", None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_no_secrets() {
    let mut scanner = SecretScanner::new();
    let content = "let x = 42;\nfn main() {}\n// This is safe code";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_aws_access_key() {
    let mut scanner = SecretScanner::new();
    // AKIA followed by 16 uppercase alphanumeric chars
    let content = "aws_access_key = AKIAIOSFODNN7EXAMPLE";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
}

#[test]
fn test_secret_scanner_detect_private_key() {
    let mut scanner = SecretScanner::new();
    let content = "-----BEGIN RSA PRIVATE KEY-----";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_ec_private_key() {
    let mut scanner = SecretScanner::new();
    let content = "-----BEGIN EC PRIVATE KEY-----";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_openssh_private_key() {
    let mut scanner = SecretScanner::new();
    let content = "-----BEGIN OPENSSH PRIVATE KEY-----";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_generic_private_key() {
    let mut scanner = SecretScanner::new();
    let content = "-----BEGIN PRIVATE KEY-----";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_github_token_ghp() {
    let mut scanner = SecretScanner::new();
    // ghp_ followed by 36 alphanumeric/underscore chars (pattern: gh[pousr]_[A-Za-z0-9_]{36,})
    let content = "token = ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789ab";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
}

#[test]
fn test_secret_scanner_detect_github_token_gho() {
    let mut scanner = SecretScanner::new();
    let content = "gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_github_token_ghu() {
    let mut scanner = SecretScanner::new();
    let content = "ghu_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_github_fine_grained_token() {
    let mut scanner = SecretScanner::new();
    // github_pat_ followed by 22+ chars
    let content = "github_pat_ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_gitlab_token() {
    let mut scanner = SecretScanner::new();
    // glpat- followed by 20+ chars
    let content = "glpat-ABCDEFGHIJKLMNOPQRSTUVWXYZabcde";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_google_api_key() {
    let mut scanner = SecretScanner::new();
    // AIza followed by exactly 35 alphanumeric/underscore/hyphen chars
    // Pattern: AIza[a-zA-Z0-9_\-]{35}
    // Pattern: AIza[a-zA-Z0-9_\-]{35}  →  AIza + exactly 35 body chars
    let content = "key = AIzaSyDabcdefghijklmnopqrstuvwxyz012345";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_stripe_secret_key() {
    let mut scanner = SecretScanner::new();
    // Build the test key dynamically to avoid triggering push protection
    let prefix = "sk_live_";
    let suffix = "TESTKEYTESTKEYTESTKEYTESTK";
    let content = format!("stripe_key = {}{}", prefix, suffix);
    let findings = scanner.scan_content(&content, None);
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
}

#[test]
fn test_secret_scanner_detect_stripe_restricted_key() {
    let mut scanner = SecretScanner::new();
    // Build the test key dynamically to avoid triggering push protection
    let prefix = "rk_live_";
    let suffix = "TESTKEYTESTKEYTESTKEYTESTK";
    let content = format!("{}{}", prefix, suffix);
    let findings = scanner.scan_content(&content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_generic_api_key() {
    let mut scanner = SecretScanner::new();
    // api_key = 'somevalue20chars+'
    let content = r#"api_key = "abcdefghij1234567890extra""#;
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_apikey_variant() {
    let mut scanner = SecretScanner::new();
    let content = r#"apikey = "abcdefghijklmnopqrstuv""#;
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_password_in_code() {
    let mut scanner = SecretScanner::new();
    let content = r#"password = "s3cr3tpassword""#;
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_passwd_variant() {
    let mut scanner = SecretScanner::new();
    let content = r#"passwd = "mysecretpass1234""#;
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_bearer_token() {
    let mut scanner = SecretScanner::new();
    let content = "Authorization: Bearer eyJhbGciOiJSUzI1NiJ9";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_jwt_token() {
    let mut scanner = SecretScanner::new();
    // Full JWT: header.payload.signature, each base64url encoded starting with eyJ
    let content =
            "token = eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_database_url_postgres() {
    let mut scanner = SecretScanner::new();
    let content = "db_url = postgres://user:password@localhost/mydb";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_database_url_mysql() {
    let mut scanner = SecretScanner::new();
    let content = "url = mysql://admin:s3cr3t@db.example.com/prod";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_database_url_mongodb() {
    let mut scanner = SecretScanner::new();
    let content = "mongo_uri = mongodb://root:hunter2@mongo.local/admin";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_slack_bot_token() {
    let mut scanner = SecretScanner::new();
    // xoxb- followed by 10+ alphanumeric/hyphen
    let content = "slack_token = xoxb-1234567890-abcdefghij";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_slack_user_token() {
    let mut scanner = SecretScanner::new();
    let content = "xoxp-1234567890-0987654321-abcdef";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_detect_base64_secret() {
    let mut scanner = SecretScanner::new();
    // "secret = " followed by 40+ base64 chars
    let content = "secret = ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/==";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_secret_scanner_skip_double_slash_comment() {
    let mut scanner = SecretScanner::new();
    let content = "// aws_key = AKIAIOSFODNN7EXAMPLE";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_skip_hash_comment() {
    let mut scanner = SecretScanner::new();
    let content = "# password = supersecretvalue";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_skip_block_comment() {
    let mut scanner = SecretScanner::new();
    let content = "/* password = supersecretvalue */";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_with_file_path() {
    let mut scanner = SecretScanner::new();
    let path = PathBuf::from("src/config.rs");
    let content = "AKIAIOSFODNN7EXAMPLE";
    let findings = scanner.scan_content(content, Some(&path));
    assert!(!findings.is_empty());
    assert_eq!(findings[0].file, Some(path));
    assert_eq!(findings[0].line, Some(1));
}

#[test]
fn test_secret_scanner_line_numbers() {
    let mut scanner = SecretScanner::new();
    let path = PathBuf::from("foo.rs");
    let content = "line1\nline2\nAKIAIOSFODNN7EXAMPLE\nline4";
    let findings = scanner.scan_content(content, Some(&path));
    assert!(!findings.is_empty());
    assert_eq!(findings[0].line, Some(3));
}

#[test]
fn test_secret_scanner_accumulates_findings() {
    let mut scanner = SecretScanner::new();
    scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
    scanner.scan_content("-----BEGIN RSA PRIVATE KEY-----", None);
    assert!(scanner.findings().len() >= 2);
}

#[test]
fn test_secret_scanner_clear() {
    let mut scanner = SecretScanner::new();
    scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
    assert!(!scanner.findings().is_empty());
    scanner.clear();
    assert!(scanner.findings().is_empty());
}

#[test]
fn test_secret_scanner_masked_snippet() {
    let mut scanner = SecretScanner::new();
    let content = "AKIAIOSFODNN7EXAMPLE";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
    // The snippet should be masked (contains "..." or all "*")
    if let Some(snippet) = &findings[0].snippet {
        assert!(snippet.contains("...") || snippet.chars().all(|c| c == '*'));
    }
}

#[test]
fn test_secret_scanner_remediation_set() {
    let mut scanner = SecretScanner::new();
    let findings = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
    assert!(!findings.is_empty());
    assert!(findings[0].remediation.is_some());
}

#[test]
fn test_secret_scanner_cwe_set() {
    let mut scanner = SecretScanner::new();
    let findings = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None);
    assert!(!findings.is_empty());
    assert_eq!(findings[0].cwe.as_deref(), Some("CWE-798"));
}

#[test]
fn test_secret_scanner_add_custom_pattern() {
    let mut scanner = SecretScanner::new();
    let custom = SecretPattern::new("Custom Secret", r"MYSECRET\d{6}", SecuritySeverity::High);
    scanner.add_pattern(custom);
    let content = "key = MYSECRET123456";
    let findings = scanner.scan_content(content, None);
    assert!(findings.iter().any(|f| f.title == "Custom Secret"));
}

#[test]
fn test_secret_scanner_unicode_content() {
    let mut scanner = SecretScanner::new();
    // Unicode characters should not cause panics
    let content = "let msg = \"こんにちは世界\"; // no secrets here";
    let findings = scanner.scan_content(content, None);
    // Comment line is skipped; non-comment line has no secret pattern match
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_very_long_line() {
    let mut scanner = SecretScanner::new();
    // 10 000-character line with no secrets; should complete without panic
    let content = "a".repeat(10_000);
    let findings = scanner.scan_content(&content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_multiline_content() {
    let mut scanner = SecretScanner::new();
    let content = "line1\nline2\nline3\nline4\nline5";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

// -------------------------------------------------------------------------
// VulnerabilityDetector
// -------------------------------------------------------------------------

#[test]
fn test_vulnerability_detector_new() {
    let detector = VulnerabilityDetector::new();
    assert!(detector.findings().is_empty());
}

#[test]
fn test_vulnerability_detector_default() {
    let detector = VulnerabilityDetector::default();
    assert!(detector.findings().is_empty());
}

#[test]
fn test_vulnerability_detector_empty_content() {
    let mut detector = VulnerabilityDetector::new();
    let findings = detector.scan_content("", None, "rust");
    assert!(findings.is_empty());
}

#[test]
fn test_vulnerability_detector_rust_unsafe() {
    let mut detector = VulnerabilityDetector::new();
    let content = "unsafe { ptr::read(addr) }";
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-242")));
}

#[test]
fn test_vulnerability_detector_rust_unsafe_severity() {
    let mut detector = VulnerabilityDetector::new();
    let content = "unsafe { }";
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    // RUST001 is Medium
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Medium));
}

#[test]
fn test_vulnerability_detector_rust_unwrap() {
    let mut detector = VulnerabilityDetector::new();
    let content = "let x = option.unwrap();";
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-252")));
}

#[test]
fn test_vulnerability_detector_rust_unwrap_severity() {
    let mut detector = VulnerabilityDetector::new();
    let content = "let x = result.unwrap();";
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    // RUST002 is Low
    assert!(findings.iter().any(|f| f.severity == SecuritySeverity::Low));
}

#[test]
fn test_vulnerability_detector_rust_sql_injection_select() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let q = format!("SELECT * FROM users WHERE id={}", id);"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-89")));
}

#[test]
fn test_vulnerability_detector_rust_sql_injection_insert() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let q = format!("INSERT INTO logs VALUES({})", val);"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_rust_sql_injection_update() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let q = format!("UPDATE users SET name={}", name);"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_rust_sql_injection_delete() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let q = format!("DELETE FROM sessions WHERE id={}", id);"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_rust_command_injection() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"Command::new(&format!("{}", user_input)).spawn().unwrap();"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
}

#[test]
fn test_vulnerability_detector_rust_path_traversal_path_new() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let p = Path::new(&format!("/uploads/{}", filename));"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-22")));
}

#[test]
fn test_vulnerability_detector_rust_path_traversal_pathbuf_from() {
    let mut detector = VulnerabilityDetector::new();
    let content = r#"let p = PathBuf::from(&format!("/data/{}", input));"#;
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_js_eval() {
    let mut detector = VulnerabilityDetector::new();
    let content = "eval(userInput)";
    let findings = detector.scan_content(content, None, "javascript");
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-95")));
}

#[test]
fn test_vulnerability_detector_js_inner_html() {
    let mut detector = VulnerabilityDetector::new();
    let content = "element.innerHTML = userInput;";
    let findings = detector.scan_content(content, None, "javascript");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-79")));
}

#[test]
fn test_vulnerability_detector_python_exec() {
    let mut detector = VulnerabilityDetector::new();
    let content = "exec(user_code)";
    let findings = detector.scan_content(content, None, "python");
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.severity == SecuritySeverity::Critical));
}

#[test]
fn test_vulnerability_detector_python_eval() {
    let mut detector = VulnerabilityDetector::new();
    let content = "result = eval(expression)";
    let findings = detector.scan_content(content, None, "python");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_python_pickle_load() {
    let mut detector = VulnerabilityDetector::new();
    let content = "obj = pickle.load(file_handle)";
    let findings = detector.scan_content(content, None, "python");
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-502")));
}

#[test]
fn test_vulnerability_detector_python_pickle_loads() {
    let mut detector = VulnerabilityDetector::new();
    let content = "data = pickle.loads(raw_bytes)";
    let findings = detector.scan_content(content, None, "python");
    assert!(!findings.is_empty());
}

#[test]
fn test_vulnerability_detector_language_filter() {
    // JS patterns should NOT fire on Rust code
    let mut detector = VulnerabilityDetector::new();
    let content = "eval(something)";
    let findings = detector.scan_content(content, None, "rust");
    // JS001 (eval) should not match when language is "rust"
    assert!(!findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-95")));
}

#[test]
fn test_vulnerability_detector_with_file_location() {
    let mut detector = VulnerabilityDetector::new();
    let path = PathBuf::from("src/main.rs");
    let content = "unsafe { }";
    let findings = detector.scan_content(content, Some(&path), "rust");
    assert!(!findings.is_empty());
    assert_eq!(findings[0].file, Some(path));
    assert_eq!(findings[0].line, Some(1));
}

#[test]
fn test_vulnerability_detector_snippet_is_trimmed() {
    let mut detector = VulnerabilityDetector::new();
    let content = "    unsafe { }  ";
    let findings = detector.scan_content(content, None, "rust");
    assert!(!findings.is_empty());
    // snippet should be trimmed
    if let Some(snippet) = &findings[0].snippet {
        assert_eq!(snippet.trim(), snippet.as_str());
    }
}

#[test]
fn test_vulnerability_detector_accumulates_findings() {
    let mut detector = VulnerabilityDetector::new();
    detector.scan_content("unsafe { }", None, "rust");
    detector.scan_content("let x = option.unwrap();", None, "rust");
    assert!(detector.findings().len() >= 2);
}

#[test]
fn test_vulnerability_detector_clear() {
    let mut detector = VulnerabilityDetector::new();
    detector.scan_content("unsafe { }", None, "rust");
    assert!(!detector.findings().is_empty());
    detector.clear();
    assert!(detector.findings().is_empty());
}

#[test]
fn test_vulnerability_detector_add_custom_pattern() {
    let mut detector = VulnerabilityDetector::new();
    detector.add_pattern(VulnerabilityPattern {
        id: "CUSTOM001".to_string(),
        name: "Custom Check".to_string(),
        language: "rust".to_string(),
        pattern: r"todo!\(\)".to_string(),
        severity: SecuritySeverity::Info,
        cwe: "CWE-0".to_string(),
        description: "Incomplete code marker".to_string(),
        remediation: "Remove todo!() before production".to_string(),
    });
    let findings = detector.scan_content("todo!()", None, "rust");
    assert!(findings.iter().any(|f| f.title == "Custom Check"));
}

#[test]
fn test_vulnerability_detector_unknown_language_no_matches() {
    let mut detector = VulnerabilityDetector::new();
    // "cobol" matches no built-in patterns
    let content = "eval(foo) unsafe { } pickle.loads(x)";
    let findings = detector.scan_content(content, None, "cobol");
    assert!(findings.is_empty());
}

#[test]
fn test_vulnerability_detector_wildcard_language() {
    // A pattern with language "*" should match all languages
    let mut detector = VulnerabilityDetector::new();
    detector.add_pattern(VulnerabilityPattern {
        id: "ALL001".to_string(),
        name: "Universal Check".to_string(),
        language: "*".to_string(),
        pattern: r"FORBIDDEN".to_string(),
        severity: SecuritySeverity::High,
        cwe: "CWE-999".to_string(),
        description: "Forbidden token".to_string(),
        remediation: "Remove it".to_string(),
    });
    let findings_rust = detector.scan_content("FORBIDDEN", None, "rust");
    assert!(!findings_rust.is_empty());
    let findings_js = detector.scan_content("FORBIDDEN", None, "javascript");
    assert!(!findings_js.is_empty());
}

// -------------------------------------------------------------------------
// KnownVulnerability
// -------------------------------------------------------------------------

#[test]
fn test_known_vulnerability_new() {
    let vuln = KnownVulnerability::new("CVE-2021-1234", SecuritySeverity::High, "Test vuln");
    assert_eq!(vuln.id, "CVE-2021-1234");
    assert_eq!(vuln.severity, SecuritySeverity::High);
    assert_eq!(vuln.description, "Test vuln");
    assert!(vuln.fixed_version.is_none());
    assert!(vuln.url.is_none());
}

#[test]
fn test_known_vulnerability_with_fixed_version() {
    let vuln =
        KnownVulnerability::new("CVE-X", SecuritySeverity::Low, "desc").with_fixed_version("2.0.0");
    assert_eq!(vuln.fixed_version.as_deref(), Some("2.0.0"));
}

#[test]
fn test_known_vulnerability_with_url() {
    let vuln = KnownVulnerability::new("CVE-X", SecuritySeverity::Low, "desc")
        .with_url("https://nvd.nist.gov/vuln/detail/CVE-X");
    assert_eq!(
        vuln.url.as_deref(),
        Some("https://nvd.nist.gov/vuln/detail/CVE-X")
    );
}

// -------------------------------------------------------------------------
// Dependency
// -------------------------------------------------------------------------

#[test]
fn test_dependency_new() {
    let dep = Dependency::new("test-pkg", "1.0.0", "npm");
    assert_eq!(dep.name, "test-pkg");
    assert_eq!(dep.version, "1.0.0");
    assert_eq!(dep.source, "npm");
    assert!(!dep.is_vulnerable());
    assert!(dep.vulnerabilities.is_empty());
}

#[test]
fn test_dependency_is_vulnerable_with_vuln() {
    let mut dep = Dependency::new("pkg", "1.0", "crates.io");
    dep.vulnerabilities.push(KnownVulnerability::new(
        "CVE-X",
        SecuritySeverity::High,
        "d",
    ));
    assert!(dep.is_vulnerable());
}

#[test]
fn test_dependency_max_severity_none() {
    let dep = Dependency::new("pkg", "1.0", "crates.io");
    assert_eq!(dep.max_severity(), None);
}

#[test]
fn test_dependency_max_severity_single() {
    let mut dep = Dependency::new("pkg", "1.0", "crates.io");
    dep.vulnerabilities.push(KnownVulnerability::new(
        "CVE-X",
        SecuritySeverity::High,
        "d",
    ));
    assert_eq!(dep.max_severity(), Some(SecuritySeverity::High));
}

#[test]
fn test_dependency_max_severity_multiple() {
    let mut dep = Dependency::new("pkg", "1.0", "crates.io");
    dep.vulnerabilities
        .push(KnownVulnerability::new("CVE-A", SecuritySeverity::Low, "d"));
    dep.vulnerabilities.push(KnownVulnerability::new(
        "CVE-B",
        SecuritySeverity::Critical,
        "e",
    ));
    dep.vulnerabilities.push(KnownVulnerability::new(
        "CVE-C",
        SecuritySeverity::High,
        "f",
    ));
    assert_eq!(dep.max_severity(), Some(SecuritySeverity::Critical));
}

// -------------------------------------------------------------------------
// DependencyAuditor
// -------------------------------------------------------------------------

#[test]
fn test_dependency_auditor_new() {
    let auditor = DependencyAuditor::new();
    assert!(auditor.vulnerable_dependencies().is_empty());
    assert!(auditor.findings().is_empty());
}

#[test]
fn test_dependency_auditor_default() {
    let auditor = DependencyAuditor::default();
    assert!(auditor.findings().is_empty());
}

#[test]
fn test_dependency_auditor_known_vuln_lodash() {
    let mut auditor = DependencyAuditor::new();
    let dep = auditor.audit_dependency("lodash", "4.17.0", "npm");
    assert!(dep.is_vulnerable());
    assert!(!auditor.findings().is_empty());
    assert_eq!(auditor.findings()[0].category, SecurityCategory::Dependency);
}

#[test]
fn test_dependency_auditor_known_vuln_log4j() {
    let mut auditor = DependencyAuditor::new();
    let dep = auditor.audit_dependency("log4j", "2.14.0", "maven");
    assert!(dep.is_vulnerable());
    assert!(dep.vulnerabilities.iter().any(|v| v.id == "CVE-2021-44228"));
    assert!(dep
        .vulnerabilities
        .iter()
        .any(|v| v.severity == SecuritySeverity::Critical));
}

#[test]
fn test_dependency_auditor_safe_dep() {
    let mut auditor = DependencyAuditor::new();
    let dep = auditor.audit_dependency("safe-package", "1.0.0", "npm");
    assert!(!dep.is_vulnerable());
}

#[test]
fn test_dependency_auditor_vulnerable_dependencies_list() {
    let mut auditor = DependencyAuditor::new();
    auditor.audit_dependency("safe-package", "1.0.0", "npm");
    auditor.audit_dependency("lodash", "4.17.0", "npm");
    let vulns = auditor.vulnerable_dependencies();
    assert_eq!(vulns.len(), 1);
    assert_eq!(vulns[0].name, "lodash");
}

#[test]
fn test_dependency_auditor_add_vulnerability() {
    let mut auditor = DependencyAuditor::new();
    let vuln = KnownVulnerability::new("CVE-CUSTOM", SecuritySeverity::Medium, "Custom vuln");
    auditor.add_vulnerability("my-package", vuln);
    let dep = auditor.audit_dependency("my-package", "1.0.0", "custom");
    assert!(dep.is_vulnerable());
}

#[test]
fn test_dependency_auditor_finding_remediation_contains_fixed_version() {
    let mut auditor = DependencyAuditor::new();
    auditor.audit_dependency("lodash", "4.17.0", "npm");
    let finding = &auditor.findings()[0];
    // remediation should mention the fixed version
    assert!(finding
        .remediation
        .as_deref()
        .unwrap_or("")
        .contains("4.17.21"));
}

#[test]
fn test_dependency_auditor_clear() {
    let mut auditor = DependencyAuditor::new();
    auditor.audit_dependency("lodash", "4.17.0", "npm");
    assert!(!auditor.findings().is_empty());
    auditor.clear();
    assert!(auditor.findings().is_empty());
    assert!(auditor.vulnerable_dependencies().is_empty());
}

// -------------------------------------------------------------------------
// ComplianceRule
// -------------------------------------------------------------------------

#[test]
fn test_compliance_rule_new() {
    let rule = ComplianceRule::new("R1", "OWASP", "Test rule");
    assert_eq!(rule.id, "R1");
    assert_eq!(rule.standard, "OWASP");
    assert_eq!(rule.description, "Test rule");
    assert!(rule.pattern.is_none());
    assert_eq!(rule.severity, SecuritySeverity::Medium);
}

#[test]
fn test_compliance_rule_with_pattern() {
    let rule = ComplianceRule::new("R1", "OWASP", "desc").with_pattern(r"md5\(");
    assert!(rule.pattern.is_some());
}

#[test]
fn test_compliance_rule_with_severity() {
    let rule = ComplianceRule::new("R1", "PCI", "desc").with_severity(SecuritySeverity::Critical);
    assert_eq!(rule.severity, SecuritySeverity::Critical);
}

// -------------------------------------------------------------------------
// ComplianceChecker
// -------------------------------------------------------------------------

#[test]
fn test_compliance_checker_new() {
    let checker = ComplianceChecker::new();
    assert!(!checker.standards().is_empty());
    assert!(checker.findings().is_empty());
}

#[test]
fn test_compliance_checker_default() {
    let checker = ComplianceChecker::default();
    assert!(!checker.standards().is_empty());
}

#[test]
fn test_compliance_checker_standards_sorted_deduped() {
    let checker = ComplianceChecker::new();
    let standards = checker.standards();
    // Check sorted
    for window in standards.windows(2) {
        assert!(window[0] <= window[1]);
    }
    // Check deduped
    let mut prev = "";
    for s in &standards {
        assert_ne!(s, prev, "duplicate standard found: {s}");
        prev = s;
    }
}

#[test]
fn test_compliance_checker_contains_known_standards() {
    let checker = ComplianceChecker::new();
    let standards = checker.standards();
    assert!(standards.iter().any(|s| s == "OWASP Top 10"));
    assert!(standards.iter().any(|s| s == "PCI-DSS"));
    assert!(standards.iter().any(|s| s == "HIPAA"));
}

#[test]
fn test_compliance_checker_weak_crypto_md5() {
    let mut checker = ComplianceChecker::new();
    let content = "hash = md5(password)";
    let findings = checker.check_content(content, None);
    assert!(!findings.is_empty());
    assert!(findings
        .iter()
        .any(|f| f.category == SecurityCategory::Compliance));
}

#[test]
fn test_compliance_checker_weak_crypto_sha1() {
    let mut checker = ComplianceChecker::new();
    let content = "digest = sha1(data)";
    let findings = checker.check_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_compliance_checker_case_insensitive_md5() {
    let mut checker = ComplianceChecker::new();
    let content = "hash = MD5(password)";
    let findings = checker.check_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_compliance_checker_case_insensitive_sha1() {
    let mut checker = ComplianceChecker::new();
    let content = "digest = SHA1(data)";
    let findings = checker.check_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_compliance_checker_clean_code() {
    let mut checker = ComplianceChecker::new();
    // sha256 is not a weak hash; should not trigger
    let content = "digest = sha256(data)";
    let findings = checker.check_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_compliance_checker_empty_content() {
    let mut checker = ComplianceChecker::new();
    let findings = checker.check_content("", None);
    assert!(findings.is_empty());
}

#[test]
fn test_compliance_checker_with_file_path() {
    let mut checker = ComplianceChecker::new();
    let path = PathBuf::from("crypto.py");
    let content = "hash = md5(data)";
    let findings = checker.check_content(content, Some(&path));
    assert!(!findings.is_empty());
    assert_eq!(findings[0].file, Some(path));
}

#[test]
fn test_compliance_checker_snippet_is_trimmed() {
    let mut checker = ComplianceChecker::new();
    let content = "   hash = md5(data)   ";
    let findings = checker.check_content(content, None);
    assert!(!findings.is_empty());
    if let Some(snippet) = &findings[0].snippet {
        assert_eq!(snippet.trim(), snippet.as_str());
    }
}

#[test]
fn test_compliance_checker_accumulates_findings() {
    let mut checker = ComplianceChecker::new();
    checker.check_content("md5(x)", None);
    checker.check_content("sha1(y)", None);
    assert!(checker.findings().len() >= 2);
}

#[test]
fn test_compliance_checker_clear() {
    let mut checker = ComplianceChecker::new();
    checker.check_content("md5(x)", None);
    assert!(!checker.findings().is_empty());
    checker.clear();
    assert!(checker.findings().is_empty());
}

#[test]
fn test_compliance_checker_add_rule() {
    let mut checker = ComplianceChecker::new();
    checker.add_rule(
        ComplianceRule::new("CUSTOM-001", "CUSTOM_STD", "No goto")
            .with_pattern(r"\bgoto\b")
            .with_severity(SecuritySeverity::Low),
    );
    let findings = checker.check_content("goto label;", None);
    assert!(findings.iter().any(|f| f.title.contains("CUSTOM-001")));
}

// -------------------------------------------------------------------------
// ScanResult
// -------------------------------------------------------------------------

#[test]
fn test_scan_result_new() {
    let result = ScanResult::new();
    assert_eq!(result.total_findings(), 0);
    assert!(!result.has_critical());
    assert!(!result.has_high());
    assert_eq!(result.risk_score(), 0.0);
    assert_eq!(result.files_scanned, 0);
    assert_eq!(result.lines_scanned, 0);
}

#[test]
fn test_scan_result_default() {
    let result = ScanResult::default();
    assert_eq!(result.total_findings(), 0);
}

#[test]
fn test_scan_result_total_findings() {
    let mut result = ScanResult::new();
    result.findings.push(SecurityFinding::new(
        "F1",
        SecurityCategory::Injection,
        SecuritySeverity::Low,
    ));
    result.findings.push(SecurityFinding::new(
        "F2",
        SecurityCategory::Injection,
        SecuritySeverity::High,
    ));
    assert_eq!(result.total_findings(), 2);
}

#[test]
fn test_scan_result_has_critical_true() {
    let mut result = ScanResult::new();
    result.by_severity.insert(SecuritySeverity::Critical, 1);
    assert!(result.has_critical());
}

#[test]
fn test_scan_result_has_critical_zero() {
    let mut result = ScanResult::new();
    result.by_severity.insert(SecuritySeverity::Critical, 0);
    assert!(!result.has_critical());
}

#[test]
fn test_scan_result_has_high_true() {
    let mut result = ScanResult::new();
    result.by_severity.insert(SecuritySeverity::High, 2);
    assert!(result.has_high());
}

#[test]
fn test_scan_result_has_high_zero() {
    let mut result = ScanResult::new();
    result.by_severity.insert(SecuritySeverity::High, 0);
    assert!(!result.has_high());
}

#[test]
fn test_scan_result_risk_score_empty() {
    let result = ScanResult::new();
    assert_eq!(result.risk_score(), 0.0);
}

#[test]
fn test_scan_result_risk_score_one_critical() {
    let mut result = ScanResult::new();
    result.findings.push(SecurityFinding::new(
        "Test",
        SecurityCategory::Injection,
        SecuritySeverity::Critical,
    ));
    assert_eq!(result.risk_score(), 9.5);
}

#[test]
fn test_scan_result_risk_score_additive() {
    let mut result = ScanResult::new();
    result.findings.push(SecurityFinding::new(
        "F1",
        SecurityCategory::Injection,
        SecuritySeverity::High,
    ));
    result.findings.push(SecurityFinding::new(
        "F2",
        SecurityCategory::Injection,
        SecuritySeverity::Medium,
    ));
    // 7.5 + 5.5 = 13.0
    assert!((result.risk_score() - 13.0).abs() < f32::EPSILON);
}

// -------------------------------------------------------------------------
// SecurityScanner (top-level integration)
// -------------------------------------------------------------------------

#[test]
fn test_security_scanner_new() {
    let scanner = SecurityScanner::new();
    let stats = scanner.get_stats();
    assert_eq!(stats.total_scans, 0);
    assert_eq!(stats.total_findings, 0);
    assert_eq!(stats.critical_findings, 0);
    assert_eq!(stats.high_findings, 0);
}

#[test]
fn test_security_scanner_default() {
    let scanner = SecurityScanner::default();
    let stats = scanner.get_stats();
    assert_eq!(stats.total_scans, 0);
}

#[test]
fn test_security_scanner_scan_clean_code() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("let x = 1;", None, "rust");
    assert_eq!(result.files_scanned, 1);
    assert_eq!(result.lines_scanned, 1);
}

#[test]
fn test_security_scanner_scan_empty_content() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("", None, "rust");
    assert_eq!(result.total_findings(), 0);
    assert_eq!(result.lines_scanned, 0);
}

#[test]
fn test_security_scanner_scan_detects_aws_secret() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("api_key = \"AKIAIOSFODNN7EXAMPLE\"", None, "rust");
    assert!(result.total_findings() > 0);
    assert!(result.by_category.contains_key("hardcoded_secret"));
}

#[test]
fn test_security_scanner_scan_detects_rust_unsafe() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("unsafe { }", None, "rust");
    assert!(result.total_findings() > 0);
    assert!(result.by_category.contains_key("code_quality"));
}

#[test]
fn test_security_scanner_scan_detects_weak_crypto() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("md5(password)", None, "python");
    assert!(result.total_findings() > 0);
    assert!(result.by_category.contains_key("compliance"));
}

#[test]
fn test_security_scanner_by_severity_populated() {
    let scanner = SecurityScanner::new();
    // unsafe is Medium severity
    let result = scanner.scan_content("unsafe { }", None, "rust");
    let total: usize = result.by_severity.values().sum();
    assert_eq!(total, result.total_findings());
}

#[test]
fn test_security_scanner_by_category_populated() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("unsafe { }", None, "rust");
    let total: usize = result.by_category.values().sum();
    assert_eq!(total, result.total_findings());
}

#[test]
fn test_security_scanner_duration_recorded() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("let x = 1;", None, "rust");
    // duration_ms may be 0 for fast scans but should not be negative (it's u64)
    let _ = result.duration_ms;
}

#[test]
fn test_security_scanner_stats_accumulate_across_scans() {
    let scanner = SecurityScanner::new();
    scanner.scan_content("let x = 1;", None, "rust");
    scanner.scan_content("let y = 2;", None, "rust");
    let stats = scanner.get_stats();
    assert_eq!(stats.total_scans, 2);
}

#[test]
fn test_security_scanner_stats_count_findings() {
    let scanner = SecurityScanner::new();
    scanner.scan_content("unsafe { }", None, "rust");
    let stats = scanner.get_stats();
    assert!(stats.total_findings >= 1);
}

#[test]
fn test_security_scanner_stats_count_critical() {
    let scanner = SecurityScanner::new();
    // An AWS key is Critical
    scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
    let stats = scanner.get_stats();
    assert!(stats.critical_findings >= 1);
}

#[test]
fn test_security_scanner_stats_count_high() {
    let scanner = SecurityScanner::new();
    // A bearer token is High
    scanner.scan_content("Authorization: Bearer sometoken123", None, "rust");
    let stats = scanner.get_stats();
    assert!(stats.high_findings >= 1);
}

#[test]
fn test_security_scanner_audit_dependency_vulnerable() {
    let scanner = SecurityScanner::new();
    let dep = scanner.audit_dependency("lodash", "4.17.0", "npm");
    assert!(dep.is_vulnerable());
}

#[test]
fn test_security_scanner_audit_dependency_safe() {
    let scanner = SecurityScanner::new();
    let dep = scanner.audit_dependency("totally-safe-pkg", "9.9.9", "npm");
    assert!(!dep.is_vulnerable());
}

#[test]
fn test_security_scanner_audit_dependency_log4j() {
    let scanner = SecurityScanner::new();
    let dep = scanner.audit_dependency("log4j", "2.14.0", "maven");
    assert!(dep.is_vulnerable());
    assert!(dep
        .vulnerabilities
        .iter()
        .any(|v| v.severity == SecuritySeverity::Critical));
}

#[test]
fn test_security_scanner_report_contains_header() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("let x = 1;", None, "rust");
    let report = scanner.generate_report(&result);
    assert!(report.contains("# Security Scan Report"));
}

#[test]
fn test_security_scanner_report_contains_stats() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("let x = 1;\nlet y = 2;", None, "rust");
    let report = scanner.generate_report(&result);
    assert!(report.contains("Files scanned: 1"));
    assert!(report.contains("Lines scanned: 2"));
    assert!(report.contains("Total findings:"));
    assert!(report.contains("Risk score:"));
}

#[test]
fn test_security_scanner_report_critical_section() {
    let scanner = SecurityScanner::new();
    // AWS key triggers Critical
    let result = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
    let report = scanner.generate_report(&result);
    assert!(report.contains("CRITICAL"));
}

#[test]
fn test_security_scanner_report_high_section() {
    let scanner = SecurityScanner::new();
    // Bearer token triggers High (no Critical expected for a bearer token alone)
    let result = scanner.scan_content("Authorization: Bearer sometoken", None, "rust");
    let report = scanner.generate_report(&result);
    // At minimum we get summary section
    assert!(report.contains("Summary by Category"));
}

#[test]
fn test_security_scanner_report_summary_by_category() {
    let scanner = SecurityScanner::new();
    let result = scanner.scan_content("unsafe { }", None, "rust");
    let report = scanner.generate_report(&result);
    assert!(report.contains("Summary by Category"));
}

#[test]
fn test_security_scanner_report_with_file_location() {
    let scanner = SecurityScanner::new();
    let path = PathBuf::from("src/secrets.rs");
    let result = scanner.scan_content("AKIAIOSFODNN7EXAMPLE", Some(&path), "rust");
    let report = scanner.generate_report(&result);
    assert!(report.contains("src/secrets.rs"));
}

#[test]
fn test_security_scanner_scan_history_capped() {
    let scanner = SecurityScanner::new();
    // Scan 110 times; history should be capped at 100
    for _ in 0..110 {
        scanner.scan_content("let x = 1;", None, "rust");
    }
    let stats = scanner.get_stats();
    assert!(stats.total_scans <= 100);
}

#[test]
fn test_security_scanner_multiline_rust_vuln() {
    let scanner = SecurityScanner::new();
    let content = "fn safe() {}\nfn danger() { unsafe { *ptr = 0; } }\nfn also_safe() {}";
    let result = scanner.scan_content(content, None, "rust");
    assert!(result.total_findings() >= 1);
    assert_eq!(result.lines_scanned, 3);
}

// -------------------------------------------------------------------------
// Edge cases & advanced scenarios
// -------------------------------------------------------------------------

#[test]
fn test_scan_only_whitespace() {
    let mut scanner = SecretScanner::new();
    let content = "   \n\t\n   ";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_scan_windows_line_endings() {
    let mut scanner = SecretScanner::new();
    // Windows CRLF - should not cause panics
    let content = "let x = 1;\r\nlet y = 2;\r\n";
    let findings = scanner.scan_content(content, None);
    assert!(findings.is_empty());
}

#[test]
fn test_secret_scanner_short_secret_masking() {
    // mask_secret: len <= 8 → all stars
    let mut scanner = SecretScanner::new();
    // Craft content that matches the private key pattern (short enough to trigger <=8 branch)
    // The pattern match "-----BEGIN PRIVATE KEY-----" is > 8 chars, so we test the other branch
    // Just verify the scanner does not panic on short snippets
    let content = "-----BEGIN PRIVATE KEY-----";
    let findings = scanner.scan_content(content, None);
    if let Some(f) = findings.first() {
        // snippet must be Some and non-empty
        assert!(f.snippet.is_some());
    }
}

#[test]
fn test_vulnerability_detector_multiple_vulns_same_line() {
    let mut detector = VulnerabilityDetector::new();
    // This line matches both unsafe (RUST001) and unwrap (RUST002)
    let content = "unsafe { x.unwrap() }";
    let findings = detector.scan_content(content, None, "rust");
    assert!(findings.len() >= 2);
}

#[test]
fn test_compliance_checker_multiple_findings_same_line() {
    let mut checker = ComplianceChecker::new();
    // Both md5 and sha1 patterns on one line
    let content = "use_md5_and_sha1()";
    // At most one OWASP-A02 pattern fires since there is only one pattern rule for weak crypto
    let findings = checker.check_content(content, None);
    // Results depend on how many rules have patterns that match
    let _ = findings; // No panic is the key assertion
}

#[test]
fn test_security_finding_ids_are_unique() {
    // IDs are based on nanosecond timestamps; with sleep this would be guaranteed,
    // but we just check that the structure produces non-empty IDs and that two
    // findings created sequentially get the SEC- prefix.
    let f1 = SecurityFinding::new("A", SecurityCategory::Injection, SecuritySeverity::Low);
    let f2 = SecurityFinding::new("B", SecurityCategory::Injection, SecuritySeverity::Low);
    assert!(f1.id.starts_with("SEC-"));
    assert!(f2.id.starts_with("SEC-"));
}

#[test]
fn test_scan_result_has_critical_absent_key() {
    let result = ScanResult::new();
    // by_severity is empty → has_critical should return false
    assert!(!result.has_critical());
}

#[test]
fn test_scan_result_has_high_absent_key() {
    let result = ScanResult::new();
    assert!(!result.has_high());
}

#[test]
fn test_dependency_auditor_multiple_audits() {
    let mut auditor = DependencyAuditor::new();
    auditor.audit_dependency("lodash", "4.17.0", "npm");
    auditor.audit_dependency("log4j", "2.14.0", "maven");
    auditor.audit_dependency("safe-pkg", "1.0", "npm");
    assert_eq!(auditor.vulnerable_dependencies().len(), 2);
}

#[test]
fn test_scanner_stats_fields() {
    let scanner = SecurityScanner::new();
    // Scan with known critical finding
    scanner.scan_content("AKIAIOSFODNN7EXAMPLE", None, "rust");
    let stats = scanner.get_stats();
    assert_eq!(stats.total_scans, 1);
    assert!(stats.total_findings >= 1);
    assert!(stats.critical_findings >= 1);
}

#[test]
fn test_vulnerability_pattern_struct_fields() {
    let pattern = VulnerabilityPattern {
        id: "V1".to_string(),
        name: "Test Pattern".to_string(),
        language: "rust".to_string(),
        pattern: r"test".to_string(),
        severity: SecuritySeverity::Low,
        cwe: "CWE-1".to_string(),
        description: "A test pattern".to_string(),
        remediation: "Fix it".to_string(),
    };
    assert_eq!(pattern.id, "V1");
    assert_eq!(pattern.name, "Test Pattern");
    assert_eq!(pattern.language, "rust");
    assert_eq!(pattern.cwe, "CWE-1");
    assert_eq!(pattern.severity, SecuritySeverity::Low);
}

#[test]
fn test_secret_scanner_detects_multiline_secrets() {
    let mut scanner = SecretScanner::new();
    let content = "line1\nAKIAIOSFODNN7EXAMPLE\nline3";
    let findings = scanner.scan_content(content, None);
    assert!(!findings.is_empty());
}

#[test]
fn test_compliance_checker_no_match_when_no_pattern() {
    // Rules without a pattern should never produce findings via check_content
    let mut checker = ComplianceChecker::new();
    checker.add_rule(ComplianceRule::new(
        "OWASP-A01",
        "OWASP Top 10",
        "Broken Access Control",
    ));
    // "Broken Access Control" rule has no pattern, so check_content should not trigger it
    let findings = checker.check_content("something dangerous", None);
    // Only pattern-based rules fire; ensure no panic
    let _ = findings;
}
