//! Secrets redaction to prevent sensitive data from leaking to logs/checkpoints


use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// Placeholder for redacted content
const REDACTED: &str = "[REDACTED]";

/// Common secret patterns to redact
static SECRET_PATTERNS: OnceLock<Vec<SecretPattern>> = OnceLock::new();

struct SecretPattern {
    name: &'static str,
    regex: Regex,
}

fn get_patterns() -> &'static Vec<SecretPattern> {
    SECRET_PATTERNS.get_or_init(|| {
        vec![
            // API Keys (generic)
            SecretPattern {
                name: "api_key",
                regex: Regex::new(r#"(?i)(api[_-]?key|apikey)\s*[=:]\s*["']?([a-zA-Z0-9_\-]{20,})["']?"#).unwrap(),
            },
            // Bearer tokens
            SecretPattern {
                name: "bearer_token",
                regex: Regex::new(r#"(?i)(bearer\s+)([a-zA-Z0-9_\-\.]{20,})"#).unwrap(),
            },
            // AWS credentials
            SecretPattern {
                name: "aws_access_key",
                regex: Regex::new(r#"(?i)(AKIA[A-Z0-9]{16})"#).unwrap(),
            },
            SecretPattern {
                name: "aws_secret_key",
                regex: Regex::new(r#"(?i)(aws[_-]?secret[_-]?access[_-]?key)\s*[=:]\s*["']?([a-zA-Z0-9/+=]{40})["']?"#).unwrap(),
            },
            // GitHub tokens
            SecretPattern {
                name: "github_token",
                regex: Regex::new(r#"(ghp_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{22,})"#).unwrap(),
            },
            // OpenAI/Anthropic API keys
            SecretPattern {
                name: "openai_key",
                regex: Regex::new(r#"(sk-[a-zA-Z0-9]{32,})"#).unwrap(),
            },
            // Generic secret/password patterns
            SecretPattern {
                name: "password",
                regex: Regex::new(r#"(?i)(password|passwd|pwd|secret)\s*[=:]\s*["']?([^\s"']{8,})["']?"#).unwrap(),
            },
            // Private keys
            SecretPattern {
                name: "private_key",
                regex: Regex::new(r#"-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(RSA\s+)?PRIVATE\s+KEY-----"#).unwrap(),
            },
            // Database connection strings
            SecretPattern {
                name: "db_connection",
                regex: Regex::new(r#"(?i)(mongodb|postgres|mysql|redis)://[^\s"'<>]+"#).unwrap(),
            },
            // JWT tokens (basic pattern)
            SecretPattern {
                name: "jwt",
                regex: Regex::new(r#"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*"#).unwrap(),
            },
            // Slack tokens
            SecretPattern {
                name: "slack_token",
                regex: Regex::new(r#"xox[baprs]-[a-zA-Z0-9-]+"#).unwrap(),
            },
            // Generic tokens in env vars
            SecretPattern {
                name: "env_token",
                regex: Regex::new(r#"(?i)([A-Z_]*(?:TOKEN|SECRET|KEY|PASSWORD|CREDENTIAL)[A-Z_]*)\s*[=:]\s*["']?([^\s"']{16,})["']?"#).unwrap(),
            },
        ]
    })
}

/// Redact secrets from a string
pub fn redact_secrets(input: &str) -> Cow<'_, str> {
    let mut result = Cow::Borrowed(input);

    for pattern in get_patterns() {
        if pattern.regex.is_match(&result) {
            let replacement = format!("{}={}", pattern.name, REDACTED);
            result = Cow::Owned(
                pattern
                    .regex
                    .replace_all(&result, &replacement)
                    .into_owned(),
            );
        }
    }

    result
}

/// Redact secrets from a JSON value (recursively)
pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            let redacted = redact_secrets(s);
            if redacted != *s {
                *s = redacted.into_owned();
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                redact_json(item);
            }
        }
        serde_json::Value::Object(obj) => {
            // Check if key suggests sensitive data
            let sensitive_keys: Vec<String> = obj
                .keys()
                .filter(|k| is_sensitive_key(k))
                .cloned()
                .collect();

            for key in sensitive_keys {
                if let Some(val) = obj.get_mut(&key) {
                    if val.is_string() {
                        *val = serde_json::Value::String(REDACTED.to_string());
                    }
                }
            }

            // Recursively check all values
            for (_, val) in obj.iter_mut() {
                redact_json(val);
            }
        }
        _ => {}
    }
}

/// Check if a key name suggests sensitive data
fn is_sensitive_key(key: &str) -> bool {
    let key_lower = key.to_lowercase();
    let sensitive_patterns = [
        "password",
        "passwd",
        "pwd",
        "secret",
        "token",
        "api_key",
        "apikey",
        "auth",
        "credential",
        "private",
        "key",
        "bearer",
        "jwt",
        "session",
        "cookie",
        "authorization",
    ];

    sensitive_patterns.iter().any(|p| key_lower.contains(p))
}

/// Redact file paths that might contain sensitive info
pub fn redact_path(path: &str) -> Cow<'_, str> {
    let sensitive_files = [
        ".env",
        "credentials",
        "secrets",
        ".netrc",
        ".npmrc",
        "id_rsa",
        "id_ed25519",
    ];

    for sensitive in &sensitive_files {
        if path.contains(sensitive) {
            return Cow::Owned(format!("[SENSITIVE_PATH:{}]", sensitive));
        }
    }

    Cow::Borrowed(path)
}

/// A wrapper for logging that auto-redacts (test helper)
#[cfg(test)]
pub fn safe_log(message: &str) -> String {
    redact_secrets(message).into_owned()
}

#[cfg(test)]
mod tests {
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
}
