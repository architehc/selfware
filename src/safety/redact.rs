//! Secrets redaction to prevent sensitive data from leaking to logs/checkpoints

use regex::{Regex, RegexBuilder};
use std::borrow::Cow;
use std::sync::OnceLock;

/// Placeholder for redacted content
const REDACTED: &str = "[REDACTED]";

/// Maximum compiled regex size to mitigate ReDoS (catastrophic backtracking).
const REGEX_SIZE_LIMIT: usize = 1 << 20; // 1 MB

/// Common secret patterns to redact
static SECRET_PATTERNS: OnceLock<Vec<SecretPattern>> = OnceLock::new();

struct SecretPattern {
    name: &'static str,
    regex: Regex,
}

/// Try to compile a regex with a size limit to prevent ReDoS.
/// Returns `None` (and logs a warning) if the pattern fails to compile.
fn compile_pattern(name: &'static str, pattern: &str) -> Option<SecretPattern> {
    match RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
    {
        Ok(regex) => Some(SecretPattern { name, regex }),
        Err(e) => {
            // Log at module level; tracing may not be initialised during OnceLock
            // init, so also use eprintln as a fallback visible in tests.
            eprintln!(
                "[redact] WARNING: secret pattern '{}' failed to compile (skipping): {}",
                name, e
            );
            None
        }
    }
}

fn get_patterns() -> &'static Vec<SecretPattern> {
    SECRET_PATTERNS.get_or_init(|| {
        let candidates: Vec<Option<SecretPattern>> = vec![
            // API Keys (generic)
            compile_pattern("api_key", r#"(?i)(api[_-]?key|apikey)\s*[=:]\s*["']?([a-zA-Z0-9_\-]{20,})["']?"#),
            // Bearer tokens
            compile_pattern("bearer_token", r#"(?i)(bearer\s+)([a-zA-Z0-9_\-\.]{20,})"#),
            // AWS credentials
            compile_pattern("aws_access_key", r#"(?i)(AKIA[A-Z0-9]{16})"#),
            compile_pattern("aws_secret_key", r#"(?i)(aws[_-]?secret[_-]?access[_-]?key)\s*[=:]\s*["']?([a-zA-Z0-9/+=]{40})["']?"#),
            // GitHub classic tokens (ghp_)
            compile_pattern("github_token", r#"(ghp_[a-zA-Z0-9]{36})"#),
            // GitHub fine-grained personal access tokens (github_pat_)
            compile_pattern("github_fine_grained_token", r#"(github_pat_[a-zA-Z0-9_]{22,})"#),
            // GitLab tokens (glpat-)
            compile_pattern("gitlab_token", r#"(glpat-[a-zA-Z0-9_\-]{20,})"#),
            // OpenAI/Anthropic API keys
            compile_pattern("openai_key", r#"(?:^|[^A-Za-z0-9])(sk-[a-zA-Z0-9_-]{20,})"#),
            // Telemetry/log shapes: sk-, key-, token- prefixed secrets (any
            // length ≥8 — the observability layer's log-redaction shapes)
            compile_pattern("prefixed_secret", r#"(?i)(sk-|key-|token-)[A-Za-z0-9_\-]{8,}"#),
            // Bearer tokens in Authorization headers
            compile_pattern("bearer_token", r#"(?i)bearer\s+[A-Za-z0-9_\-\.]{8,}"#),
            // Google API keys
            compile_pattern("google_api_key", r#"(AIza[a-zA-Z0-9_\-]{35})"#),
            // Stripe API keys (secret, restricted, and publishable)
            compile_pattern("stripe_key", r#"(sk_live_[a-zA-Z0-9]{24,}|rk_live_[a-zA-Z0-9]{24,}|pk_live_[a-zA-Z0-9]{24,})"#),
            // Slack tokens (xoxb-, xoxp-, xoxs-, xoxa-, xoxr-)
            compile_pattern("slack_token", r#"(xox[bpsar]-[a-zA-Z0-9\-]+)"#),
            // Generic secret/password patterns
            // Value class excludes '[' so earlier patterns' own
            // `name=[REDACTED]` replacements are never re-matched as secrets.
            compile_pattern("password", r#"(?i)(password|passwd|pwd|secret)\s*[=:]\s*["']?([^\s"'\[]{6,})["']?"#),
            // Private keys
            compile_pattern("private_key", r#"-----BEGIN\s+(?:[A-Z0-9]+\s+)?PRIVATE\s+KEY-----[\s\S]*?-----END\s+(?:[A-Z0-9]+\s+)?PRIVATE\s+KEY-----"#),
            // Database connection strings
            compile_pattern("db_connection", r#"(?i)(mongodb|postgres|mysql|redis)://[^\s"'<>]+"#),
            // JWT tokens - full three-part tokens
            compile_pattern("jwt", r#"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*"#),
            // JWT-like base64 tokens (eyJ prefix is base64 for {"): catch partial/header-only
            compile_pattern("jwt_partial", r#"eyJ[a-zA-Z0-9_/+\-]{30,}"#),
            // Generic tokens in env vars
            compile_pattern("env_token", r#"(?i)([A-Z_]*(?:TOKEN|SECRET|KEY|PASSWORD|CREDENTIAL)[A-Z_]*)\s*[=:]\s*["']?([^\s"'\[]{16,})["']?"#),
            // Generic high-entropy base64-encoded strings that look like API keys
            compile_pattern("base64_secret", r#"(?i)(?:key|token|secret|password|credential|auth)\s*[=:]\s*["']?([A-Za-z0-9+/=_\-]{40,})["']?"#),
        ];
        candidates.into_iter().flatten().collect()
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
#[path = "../../tests/unit/safety/redact/redact_test.rs"]
mod tests;
