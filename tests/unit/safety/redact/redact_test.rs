use super::*;

#[test]
fn test_redact_api_key() {
    let input = "api_key=sk_test_FAKEFAKEFAKEFAKE1234";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("sk_test"));
}

#[test]
fn test_redact_bearer_token() {
    let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.test.test";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_aws_access_key() {
    let input = "Found key: AKIAIOSFODNN7EXAMPLE";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("AKIAIOSFODNN7EXAMPLE"));
}

#[test]
fn test_redact_github_token() {
    let input = "GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_openai_key() {
    let input = "openai_key: sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_password() {
    let input = "password=mysupersecretpassword123";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("mysupersecret"));
}

#[test]
fn test_redact_private_key() {
    let input = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7
-----END PRIVATE KEY-----"#;
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_db_connection() {
    let input = "DATABASE_URL=postgres://user:password@localhost:5432/mydb";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_no_redaction_needed() {
    let input = "This is a normal message with no secrets";
    let output = redact_secrets(input);
    assert_eq!(output, input);
}

#[test]
fn test_redact_json() {
    let mut json = serde_json::json!({
        "name": "test",
        "api_key": "sk-secretkey12345678901234567890",
        "nested": {
            "password": "secret123"
        }
    });

    redact_json(&mut json);

    assert_eq!(json["api_key"], "[REDACTED]");
    assert_eq!(json["nested"]["password"], "[REDACTED]");
    assert_eq!(json["name"], "test");
}

#[test]
fn test_is_sensitive_key() {
    assert!(is_sensitive_key("password"));
    assert!(is_sensitive_key("API_KEY"));
    assert!(is_sensitive_key("auth_token"));
    assert!(is_sensitive_key("secret_value"));

    assert!(!is_sensitive_key("username"));
    assert!(!is_sensitive_key("email"));
    assert!(!is_sensitive_key("name"));
}

#[test]
fn test_redact_path() {
    assert!(redact_path("/home/user/.env").contains("SENSITIVE_PATH"));
    assert!(redact_path("/root/.ssh/id_rsa").contains("SENSITIVE_PATH"));
    assert_eq!(
        redact_path("/home/user/code/main.rs"),
        "/home/user/code/main.rs"
    );
}

#[test]
fn test_redact_jwt() {
    let input = "token: eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_slack_token() {
    let input = "SLACK_TOKEN=xoxb-FAKE-FAKE-FAKEFAKEFAKEFAKE";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_safe_log() {
    let message = "Connecting with api_key=secret12345678901234567890";
    let safe = safe_log(message);
    assert!(!safe.contains("secret123"));
}

#[test]
fn test_redact_empty_string() {
    let input = "";
    let output = redact_secrets(input);
    assert_eq!(output, "");
}

#[test]
fn test_redact_multiple_secrets() {
    let input = "api_key=secret12345678901234567890 and password=anothersecretpassword";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("secret123"));
    assert!(!output.contains("anothersecretpassword"));
}

#[test]
fn test_is_sensitive_key_edge_cases() {
    // Case insensitive
    assert!(is_sensitive_key("PASSWORD"));
    assert!(is_sensitive_key("PaSsWoRd"));
    assert!(is_sensitive_key("API_KEY"));
    assert!(is_sensitive_key("ApiKey"));

    // Contains patterns
    assert!(is_sensitive_key("user_password_hash"));
    assert!(is_sensitive_key("my_secret_value"));
    assert!(is_sensitive_key("jwt_token"));
    assert!(is_sensitive_key("session_cookie"));
    assert!(is_sensitive_key("private_key_path"));
    assert!(is_sensitive_key("bearer_token"));
    assert!(is_sensitive_key("authorization_header"));
    assert!(is_sensitive_key("credential_file"));
}

#[test]
fn test_is_sensitive_key_non_sensitive() {
    assert!(!is_sensitive_key("user_id"));
    assert!(!is_sensitive_key("timestamp"));
    assert!(!is_sensitive_key("count"));
    assert!(!is_sensitive_key("description"));
    assert!(!is_sensitive_key("created_at"));
}

#[test]
fn test_redact_json_array() {
    let mut json = serde_json::json!([
        {"api_key": "secret123456789012345678901"},
        {"name": "test"},
        {"password": "mysecret"}
    ]);

    redact_json(&mut json);

    assert_eq!(json[0]["api_key"], "[REDACTED]");
    assert_eq!(json[1]["name"], "test");
    assert_eq!(json[2]["password"], "[REDACTED]");
}

#[test]
fn test_redact_json_nested_array() {
    let mut json = serde_json::json!({
        "users": [
            {"name": "alice", "auth_token": "token12345678901234567890"},
            {"name": "bob", "auth_token": "token09876543210987654321"}
        ]
    });

    redact_json(&mut json);

    assert_eq!(json["users"][0]["name"], "alice");
    assert_eq!(json["users"][0]["auth_token"], "[REDACTED]");
    assert_eq!(json["users"][1]["auth_token"], "[REDACTED]");
}

#[test]
fn test_redact_json_primitives() {
    // Numbers and bools should not be changed
    let mut json = serde_json::json!({
        "count": 42,
        "active": true,
        "rate": 3.15
    });

    redact_json(&mut json);

    assert_eq!(json["count"], 42);
    assert_eq!(json["active"], true);
    assert_eq!(json["rate"], 3.15);
}

#[test]
fn test_redact_json_null_value() {
    let mut json = serde_json::json!({
        "api_key": null,
        "password": null
    });

    redact_json(&mut json);

    // null values remain null (not strings to redact)
    assert!(json["api_key"].is_null());
    assert!(json["password"].is_null());
}

#[test]
fn test_redact_json_string_with_pattern() {
    let mut json = serde_json::json!({
        "log": "Connection with api_key=secret12345678901234567890 established"
    });

    redact_json(&mut json);

    let log = json["log"].as_str().unwrap();
    assert!(log.contains("[REDACTED]"));
    assert!(!log.contains("secret12345"));
}

#[test]
fn test_redact_path_all_sensitive() {
    assert!(redact_path("/home/user/.env").contains("SENSITIVE_PATH:.env"));
    assert!(redact_path("/etc/credentials").contains("SENSITIVE_PATH:credentials"));
    assert!(redact_path("/var/secrets/app").contains("SENSITIVE_PATH:secrets"));
    assert!(redact_path("/home/user/.netrc").contains("SENSITIVE_PATH:.netrc"));
    assert!(redact_path("/home/user/.npmrc").contains("SENSITIVE_PATH:.npmrc"));
    assert!(redact_path("/home/user/.ssh/id_rsa").contains("SENSITIVE_PATH:id_rsa"));
    assert!(redact_path("/home/user/.ssh/id_ed25519").contains("SENSITIVE_PATH:id_ed25519"));
}

#[test]
fn test_redact_path_non_sensitive() {
    let paths = [
        "/home/user/code/main.rs",
        "/var/log/app.log",
        "/etc/nginx/nginx.conf",
        "/usr/local/bin/app",
    ];
    for path in paths {
        assert_eq!(redact_path(path), path);
    }
}

#[test]
fn test_redact_aws_secret_key() {
    let input = "aws_secret_access_key=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("wJalrXUtnFEMI"));
}

#[test]
fn test_redact_github_pat() {
    let input = "token=github_pat_abcdefghijklmnopqrstuv";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("github_pat_"));
}

#[test]
fn test_redact_mongodb_connection() {
    let input = "mongodb://user:password123@localhost:27017/mydb";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("password123"));
}

#[test]
fn test_redact_mysql_connection() {
    let input = "mysql://root:supersecret@localhost:3306/db";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_redis_connection() {
    let input = "redis://default:mypassword@localhost:6379";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_env_token() {
    let input = "MY_SECRET_TOKEN=abcdefghijklmnop1234";
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_rsa_private_key() {
    let input = r#"-----BEGIN RSA PRIVATE KEY-----
MIIBOgIBAAJBALRiMLAj+6y3uqsVLr
-----END RSA PRIVATE KEY-----"#;
    let output = redact_secrets(input);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("MIIBOgI"));
}

#[test]
fn test_cow_borrowed_no_secrets() {
    let input = "Normal text without any secrets";
    let output = redact_secrets(input);
    // Should be Borrowed since no changes needed
    assert!(matches!(output, Cow::Borrowed(_)));
}

#[test]
fn test_cow_owned_with_secrets() {
    let input = "api_key=secret12345678901234567890";
    let output = redact_secrets(input);
    // Should be Owned since changes were made
    assert!(matches!(output, Cow::Owned(_)));
}

#[test]
fn test_get_patterns_returns_vec() {
    let patterns = get_patterns();
    assert!(!patterns.is_empty());
    // Should have at least the patterns we defined
    assert!(patterns.len() >= 10);
}

#[test]
fn test_redact_secrets_preserves_surrounding_text() {
    let input = "Before api_key=secret12345678901234567890 After";
    let output = redact_secrets(input);
    assert!(output.contains("Before"));
    assert!(output.contains("After"));
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn test_redact_openssh_private_key() {
    // The modern ssh-keygen default (BEGIN OPENSSH PRIVATE KEY) was missed
    // by the old (RSA )? prefix; obviously-fake key material below.
    let input = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
                     abc123fakekeymaterialnotreal\n\
                     -----END OPENSSH PRIVATE KEY-----";
    let output = redact_secrets(input);
    assert!(
        output.contains("[REDACTED]"),
        "OpenSSH private key block should be redacted"
    );
    assert!(
        !output.contains("fakekeymaterial"),
        "fake key material must not survive redaction"
    );
}

// ── First-party rust_source carve-out (glm capstone: generic keyword
// patterns mangle ordinary workspace code the model must read verbatim) ──

#[test]
fn rust_source_keeps_ordinary_code_verbatim() {
    // The exact mangle shapes from the capstone: keyword-named bindings
    // whose "value" is a function call or a benign const.
    let source = concat!(
        "let secret = compute_hash();\n",
        "let api_key = fetch_remote_api_key();\n",
        "const TIMEOUT_KEY: &str = \"timeout\";\n",
        "const API_TOKEN: &str = \"abcdefghijklmnop\";\n",
        "let auth_token = \"dGVzdCB0b2tlbiBmb3IgZXhhbXBsZSBwdXJwb3Nl\";\n",
    );
    let output = redact_secrets_with_context(source, RedactionContext::RustSource);
    assert_eq!(output, source, "workspace Rust must survive verbatim");
}

#[test]
fn generic_context_still_redacts_the_same_code() {
    // Proof the carve-out is what saves the code: the full pattern set
    // mangles these exact lines (the pre-fix behavior).
    let source = "let secret = compute_hash();\nlet api_key = fetch_remote_api_key();\n";
    let output = redact_secrets_with_context(source, RedactionContext::Generic);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("secret = compute_hash()"));
}

#[test]
fn rust_source_still_redacts_high_signal_key_formats() {
    // An actual sk- key in first-party source must redact — the carve-out
    // only covers generic keyword patterns, never real key formats.
    let source = "let key = \"sk-abcdefghijklmnopqrstuvwxyz123456\";\n";
    let output = redact_secrets_with_context(source, RedactionContext::RustSource);
    assert!(output.contains("[REDACTED]"), "got: {output}");
    assert!(!output.contains("sk-abcdefghijklmnopqrstuvwxyz123456"));

    // PEM blocks redact everywhere too.
    let pem = "-----BEGIN PRIVATE KEY-----\nMIIBog==\n-----END PRIVATE KEY-----";
    let output = redact_secrets_with_context(pem, RedactionContext::RustSource);
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("MIIBog=="));

    // AWS access key ids redact everywhere.
    let output = redact_secrets_with_context("AKIAIOSFODNN7EXAMPLE", RedactionContext::RustSource);
    assert!(output.contains("[REDACTED]"));
}

#[test]
fn plain_redact_secrets_unchanged_for_non_rust_content() {
    // Backward compatibility: the context-free entry point keeps the full
    // pattern set for non-workspace / unknown content.
    let output = redact_secrets("let secret = compute_hash();");
    assert!(output.contains("[REDACTED]"));
}
