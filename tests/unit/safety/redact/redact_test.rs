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

#[test]
fn redacts_scanner_shapes_review_finding_4() {
    // External review of 6e231e2e, finding #4: the secret scanner recognized
    // credential shapes the output redactor missed. Each synthetic below
    // matched NONE of the redactor's patterns before the fix.
    // NOTE: the Slack webhook and Twilio SID literals are assembled from
    // fragments — a full literal in source trips GitHub push protection
    // (GH013) even though every value here is synthetic.
    let slack_webhook = concat!(
        "SLACK_WEBHOOK=https://hooks.slack.com/",
        "services/T01ABCDEF23/B04CDEFGH67/",
        "abcdefABCDEF1234567890ab"
    );
    let twilio_sid = concat!("TWILIO_SID=AC", "0123456789abcdef0123456789abcdef");
    let cases = [
        // MongoDB SRV connection string with credentials
        "uri = \"mongodb+srv://admin:Sup3rSecretPass@cluster0.ab1cd.mongodb.net/prod\"",
        // PostgreSQL (explicit scheme alias)
        "DATABASE_URL=postgresql://svc:Sup3rSecretPass@db.internal:5432/app",
        // Slack webhook URL — anyone holding one can post
        slack_webhook,
        // Azure storage account key
        "DefaultEndpointsProtocol=https;AccountName=st;AccountKey=Ab1Cd2Ef3Gh4Ij5Kl6Mn7Op8Qr9St0Uv==;EndpointSuffix=core.windows.net",
        // Twilio Account SID
        twilio_sid,
        // GitHub OAuth / server-to-server / user-to-server / refresh tokens
        "token = gho_a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
        "token = ghs_a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6",
    ];
    for input in cases {
        let output = redact_secrets(input);
        assert!(
            output.contains("[REDACTED]"),
            "not redacted: {input} -> {output}"
        );
        assert!(
            !output.contains("Sup3rSecretPass"),
            "password survived: {output}"
        );
    }
    // The webhook path itself must not survive either.
    let output = redact_secrets(cases[2]);
    assert!(
        !output.contains("abcdefABCDEF1234567890ab"),
        "webhook secret survived: {output}"
    );
}

#[test]
fn every_builtin_secret_detector_has_redaction_coverage() {
    // Independent examples force review when a new detector is added; sharing
    // regexes alone must not make this assertion pass without a real fixture.
    // Construct these synthetic values at runtime so GitHub push protection
    // does not mistake the test source for committed credentials.
    let gitlab_token = ["glpat", "A1b2C3d4E5f6G7h8I9j0"].join("-");
    let twilio_sid = format!("AC{}", "0123456789abcdef".repeat(2));
    let cases = [
        (
            "AWS Access Key",
            "AKIA7H3M9Q2V6N8C4R5T",
            "AKIA7H3M9Q2V6N8C4R5T",
        ),
        (
            "AWS Secret Key",
            "AWS_SECRET_ACCESS_KEY = \"Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A\"",
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A",
        ),
        (
            "GitHub Token",
            "ghp_A1b2C3d4E5f6G7h8I9j0",
            "ghp_A1b2C3d4E5f6G7h8I9j0",
        ),
        (
            "GitHub Fine-Grained Token",
            "github_pat_A1b2C3d4E5f6G7h8I9j0K1L2",
            "github_pat_A1b2C3d4E5f6G7h8I9j0K1L2",
        ),
        ("GitLab Token", gitlab_token.as_str(), gitlab_token.as_str()),
        (
            "npm Token",
            "npm_H9vz3E8Kq5X2Mf7Yb6Cd4Nr8Q2Az5W7P",
            "npm_H9vz3E8Kq5X2Mf7Yb6Cd4Nr8Q2Az5W7P",
        ),
        (
            "Generic API Key",
            "api_key = \"A1b2C3d4E5f6G7h8I9j0\"",
            "A1b2C3d4E5f6G7h8I9j0",
        ),
        (
            "Private Key",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nprivate_key_material_without_end_marker",
            "private_key_material_without_end_marker",
        ),
        (
            "Google API Key",
            "AIzaAb3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx",
            "AIzaAb3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx",
        ),
        (
            "Stripe Key",
            "sk_test_H9vz3E8Kq5X2Mf7Yb",
            "sk_test_H9vz3E8Kq5X2Mf7Yb",
        ),
        (
            "Password in Code",
            "password = \"H9vz3E8Kq5X2\"",
            "H9vz3E8Kq5X2",
        ),
        ("Bearer Token", "Bearer B7q2", "B7q2"),
        (
            "JWT Token",
            "eyJhbGciOiJub25lIn0.eyJ1c2VyIjoiYSJ9.c2ln",
            "eyJhbGciOiJub25lIn0.eyJ1c2VyIjoiYSJ9.c2ln",
        ),
        (
            "Database URL",
            "mongodb+srv://reader:H9vz3E8Kq5@db.invalid/data",
            "H9vz3E8Kq5",
        ),
        (
            "Slack Token",
            "xoxb-H9vz3E8Kq5X2Mf7Yb",
            "xoxb-H9vz3E8Kq5X2Mf7Yb",
        ),
        (
            "JWT Partial",
            "eyJhbGciOiJub25lIiwidXNlciI6ImFiY2RlZiJ9",
            "eyJhbGciOiJub25lIiwidXNlciI6ImFiY2RlZiJ9",
        ),
        (
            "Slack Webhook",
            "hooks.slack.com/services/T12ABC/B34DEF/H9vz3E8Kq5X2",
            "H9vz3E8Kq5X2",
        ),
        (
            "Azure Account Key",
            "AccountKey=Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv",
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv",
        ),
        ("Twilio SID", twilio_sid.as_str(), twilio_sid.as_str()),
        (
            "Base64 Secret",
            "auth=Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A",
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A",
        ),
    ];
    for pattern in crate::safety::scanner::SecretScanner::default_patterns() {
        let (_, input, secret) = cases
            .iter()
            .find(|(name, _, _)| *name == pattern.name)
            .expect("new detector needs an independent redaction fixture");
        assert!(
            pattern.compiled.as_ref().unwrap().is_match(input),
            "invalid fixture for {}",
            pattern.name
        );
        let output = redact_secrets(input);
        assert!(
            output.contains("[REDACTED]"),
            "{} must redact",
            pattern.name
        );
        assert!(
            !output.contains(*secret),
            "{} secret survived",
            pattern.name
        );
    }
}

#[test]
fn npm_and_all_stripe_modes_are_redacted_in_source_and_output() {
    for context in [RedactionContext::Generic, RedactionContext::RustSource] {
        for kind in ["sk", "rk", "pk"] {
            for mode in ["live", "test"] {
                let key = format!("{kind}_{mode}_H9vz3E8Kq5X2Mf7Yb");
                let output = redact_secrets_with_context(&key, context);
                assert!(!output.contains(&key));
                assert!(output.contains("[REDACTED]"));
            }
        }
        let key = "npm_H9vz3E8Kq5X2Mf7Yb6Cd4Nr8Q2Az5W7P";
        assert!(!redact_secrets_with_context(key, context).contains(key));
    }
}

// ── Model-facing redaction (0.9.4: redaction false positives on ordinary
// code reached the model, which "repaired" the line into the file) ──

/// Code-shaped lines that must reach the model verbatim.
const CODE_SHAPED: &[&str] = &[
    "    tokens = text.split(DEFAULT_SEPARATOR)",
    "    api_key = get_key()",
    "    password_field = form[\"password\"]",
    "    token_count = len(tokens)",
    "    secret = compute_hash()",
    "    auth_token = request.headers.get(\"Authorization\")",
    "    MAX_TOKENS = 4096",
    "    if token == expected_token_2:",
    "    token = next_token2 + 1",
    "    x < y && y > z",
    "    s = \"a &amp; b <div>\"",
    "def slugify(text, api_key=None, password=None):",
    "    headers = {\"Authorization\": f\"Bearer {token}\"}",
    "    PASSWORD_PROMPT = \"Enter your password:\"",
    "    kind = \"access_token\"",
    "    token: Optional[str] = None",
    "    self.api_key = config.api_key",
    "    SECRET_KEY = os.environ[\"SECRET_KEY\"]",
    "    api_key: ${{ secrets.API_KEY }}",
    "    password = \"********\"",
    "    GITHUB_TOKEN=$GITHUB_TOKEN",
    "    url = \"postgres://localhost/db\"",
    "    use scikit-learn-extra-long-name-here",
];

#[test]
fn model_redaction_keeps_code_shaped_lines_verbatim() {
    let text = CODE_SHAPED.join("\n") + "\n";
    for context in [RedactionContext::Generic, RedactionContext::RustSource] {
        let plain = redact_for_model(&text, context);
        assert_eq!(plain.content, text);
        assert_eq!(plain.redacted, 0);
        // Serialized (file_read-shaped, numbered) — the shape the old
        // redactor ran on, where `\n` was two non-space characters.
        let numbered = crate::tools::line_numbers::number_lines(&text, 70);
        let json = serde_json::json!({"content": numbered, "line_numbers": true}).to_string();
        let out = redact_for_model(&json, context);
        assert_eq!(out.content, json);
        assert_eq!(out.redacted, 0);
    }
}

#[test]
fn log_redactor_no_longer_swallows_escaped_newlines() {
    // The exact live shape: serialized content, value class ran through `\n`.
    let json = serde_json::json!({
        "content": "    tokens = text.split(DEFAULT_SEPARATOR)0123456789\n    if word_boundary:\n"
    })
    .to_string();
    let out = redact_secrets(&json);
    assert!(out.contains("\\n    if word_boundary:"), "{out}");
}

#[test]
fn model_redaction_replaces_only_secret_values_and_keeps_lines() {
    let openai = ["sk", "ant", "api03", "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0"].join("-");
    let text = format!(
        "API_KEY={openai}\n\
         config:\n\
         \x20 password: hunter2secret\n\
         \x20 db: postgres://app:H9vz3E8Kq5@db.internal/app\n\
         Authorization: Bearer abcdef1234567890ghijkl\n\
         const ACCESS_TOKEN = \"Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0\";\n\
         -----BEGIN PRIVATE KEY-----\n\
         MIIBVQIBADANBgkqhkiG9w0BAQEFAASCAT8wggE7AgEAAkEA\n\
         -----END PRIVATE KEY-----\n\
         done\n"
    );
    let out = redact_for_model(&text, RedactionContext::Generic);
    let lines: Vec<&str> = out.content.lines().collect();
    assert_eq!(lines.len(), text.lines().count());
    assert_eq!(lines[0], "API_KEY=[REDACTED:openai_key]");
    assert_eq!(lines[1], "config:");
    assert_eq!(lines[2], "  password: [REDACTED:password]");
    assert_eq!(
        lines[3],
        "  db: postgres://app:[REDACTED:connection_password]@db.internal/app"
    );
    assert_eq!(lines[4], "Authorization: Bearer [REDACTED:bearer_token]");
    assert_eq!(lines[5], "const ACCESS_TOKEN = \"[REDACTED:token]\";");
    assert_eq!(lines[6], "-----BEGIN PRIVATE KEY-----");
    assert_eq!(lines[7], "[REDACTED:private_key]");
    assert_eq!(lines[8], "-----END PRIVATE KEY-----");
    assert_eq!(lines[9], "done");
    assert_eq!(out.redacted, 6);
    assert!(out.content.ends_with('\n'));

    // A JSON result is redacted per leaf, including secret-named fields.
    let json = serde_json::json!({
        "access_token": "Zx9Yw8Vu7Ts6Rq5Po4Nm3Lk2",
        "token_type": "bearer",
        "content": format!("line one\nAPI_KEY={openai}\nline three\n"),
    })
    .to_string();
    let out = redact_for_model(&json, RedactionContext::Generic);
    let value: serde_json::Value = serde_json::from_str(&out.content).unwrap();
    assert_eq!(value["access_token"], "[REDACTED:token]");
    assert_eq!(value["token_type"], "bearer");
    assert_eq!(
        value["content"],
        "line one\nAPI_KEY=[REDACTED:openai_key]\nline three\n"
    );
}

#[test]
fn every_builtin_secret_detector_has_model_redaction_coverage() {
    // Same independent fixtures as the log-redaction coverage test (built
    // at runtime for push protection); a new scanner detector fails here
    // until the model-facing redactor covers it too.
    let gitlab_token = ["glpat", "A1b2C3d4E5f6G7h8I9j0"].join("-");
    let twilio_sid = format!("AC{}", "0123456789abcdef".repeat(2));
    let cases: Vec<(&str, String, String)> = vec![
        (
            "AWS Access Key",
            "AKIA7H3M9Q2V6N8C4R5T".into(),
            "AKIA7H3M9Q2V6N8C4R5T".into(),
        ),
        (
            "AWS Secret Key",
            "AWS_SECRET_ACCESS_KEY = \"Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A\"".into(),
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A".into(),
        ),
        (
            "GitHub Token",
            "ghp_A1b2C3d4E5f6G7h8I9j0".into(),
            "ghp_A1b2C3d4E5f6G7h8I9j0".into(),
        ),
        (
            "GitHub Fine-Grained Token",
            "github_pat_A1b2C3d4E5f6G7h8I9j0K1L2".into(),
            "github_pat_A1b2C3d4E5f6G7h8I9j0K1L2".into(),
        ),
        ("GitLab Token", gitlab_token.clone(), gitlab_token.clone()),
        (
            "npm Token",
            "npm_H9vz3E8Kq5X2Mf7Yb6Cd4Nr8Q2Az5W7P".into(),
            "npm_H9vz3E8Kq5X2Mf7Yb6Cd4Nr8Q2Az5W7P".into(),
        ),
        (
            "Generic API Key",
            "api_key = \"A1b2C3d4E5f6G7h8I9j0\"".into(),
            "A1b2C3d4E5f6G7h8I9j0".into(),
        ),
        (
            "Private Key",
            "-----BEGIN OPENSSH PRIVATE KEY-----\nprivate_key_material_without_end_marker".into(),
            "private_key_material_without_end_marker".into(),
        ),
        (
            "Google API Key",
            "AIzaAb3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx".into(),
            "AIzaAb3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx".into(),
        ),
        (
            "Stripe Key",
            "sk_test_H9vz3E8Kq5X2Mf7Yb".into(),
            "sk_test_H9vz3E8Kq5X2Mf7Yb".into(),
        ),
        (
            "Password in Code",
            "password = \"H9vz3E8Kq5X2\"".into(),
            "H9vz3E8Kq5X2".into(),
        ),
        // A realistic bearer credential (the log fixture's 4-char `B7q2` is
        // below what a model-facing redactor can tell from prose).
        (
            "Bearer Token",
            "Bearer B7q2Xk9Lm3Np8Qr4".into(),
            "B7q2Xk9Lm3Np8Qr4".into(),
        ),
        (
            "JWT Token",
            "eyJhbGciOiJub25lIn0.eyJ1c2VyIjoiYSJ9.c2ln".into(),
            "eyJhbGciOiJub25lIn0.eyJ1c2VyIjoiYSJ9.c2ln".into(),
        ),
        (
            "Database URL",
            "mongodb+srv://reader:H9vz3E8Kq5@db.invalid/data".into(),
            "H9vz3E8Kq5".into(),
        ),
        (
            "Slack Token",
            "xoxb-H9vz3E8Kq5X2Mf7Yb".into(),
            "xoxb-H9vz3E8Kq5X2Mf7Yb".into(),
        ),
        (
            "JWT Partial",
            "eyJhbGciOiJub25lIiwidXNlciI6ImFiY2RlZiJ9".into(),
            "eyJhbGciOiJub25lIiwidXNlciI6ImFiY2RlZiJ9".into(),
        ),
        (
            "Slack Webhook",
            "hooks.slack.com/services/T12ABC/B34DEF/H9vz3E8Kq5X2".into(),
            "H9vz3E8Kq5X2".into(),
        ),
        (
            "Azure Account Key",
            "AccountKey=Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv".into(),
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv".into(),
        ),
        ("Twilio SID", twilio_sid.clone(), twilio_sid.clone()),
        (
            "Base64 Secret",
            "auth=Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A".into(),
            "Ab3Cd4Ef5Gh6Ij7Kl8Mn9Op0Qr1St2Uv3Wx4Yz5A".into(),
        ),
    ];
    for pattern in crate::safety::scanner::SecretScanner::default_patterns() {
        let (_, input, secret) = cases
            .iter()
            .find(|(name, _, _)| *name == pattern.name)
            .expect("new detector needs a model-facing redaction fixture");
        let out = redact_for_model(input, RedactionContext::Generic);
        assert!(
            !out.content.contains(secret.as_str()),
            "{} secret survived: {}",
            pattern.name,
            out.content
        );
        assert!(
            out.content.contains(MODEL_REDACTION_MARKER_PREFIX),
            "{}",
            pattern.name
        );
        assert_eq!(out.content.lines().count(), input.lines().count());
    }
}
