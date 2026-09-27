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
            // GitHub classic tokens (ghp_, gho_, ghu_, ghs_, ghr_) — widened to
            // match the scanner's shape (review finding #4: the scanner
            // recognized gho_/ghu_/ghs_/ghr_ that the redactor missed)
            compile_pattern("github_token", r#"(gh[pousr]_[a-zA-Z0-9_]{16,})"#),
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
            // Slack webhook URLs — anyone holding one can post (scanner had
            // this; the redactor missed it — review finding #4)
            compile_pattern("slack_webhook", r#"(hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+)"#),
            // Azure storage account keys (scanner had this; redactor missed it)
            compile_pattern("azure_account_key", r#"(AccountKey=[A-Za-z0-9+/=]{20,})"#),
            // Twilio Account SIDs (AC + 32 hex; scanner had this too)
            compile_pattern("twilio_sid", r#"(AC[0-9a-f]{32})"#),
            // Generic secret/password patterns
            // Value class excludes '[' so earlier patterns' own
            // `name=[REDACTED]` replacements are never re-matched as secrets.
            compile_pattern("password", r#"(?i)(password|passwd|pwd|secret)\s*[=:]\s*["']?([^\s"'\[\\]{6,})["']?"#),
            // Private keys
            compile_pattern("private_key", r#"-----BEGIN\s+(?:[A-Z0-9]+\s+)?PRIVATE\s+KEY-----[\s\S]*?(?:-----END\s+(?:[A-Z0-9]+\s+)?PRIVATE\s+KEY-----|\z)"#),
            // Database connection strings — mongodb+srv and valkey included to
            // match the scanner's shape (review finding #4: a mongodb+srv://
            // URL with credentials sailed through output redaction)
            compile_pattern("db_connection", r#"(?i)(mongodb(\+srv)?|postgres|postgresql|mysql|redis|valkey)://[^\s"'<>]+"#),
            // JWT tokens - full three-part tokens
            compile_pattern("jwt", r#"eyJ[a-zA-Z0-9_-]*\.eyJ[a-zA-Z0-9_-]*\.[a-zA-Z0-9_-]*"#),
            // JWT-like base64 tokens (eyJ prefix is base64 for {"): catch partial/header-only
            compile_pattern("jwt_partial", r#"eyJ[a-zA-Z0-9_/+\-]{30,}"#),
            // Generic tokens in env vars
            compile_pattern("env_token", r#"(?i)([A-Z_]*(?:TOKEN|SECRET|KEY|PASSWORD|CREDENTIAL)[A-Z_]*)\s*[=:]\s*["']?([^\s"'\[\\]{16,})["']?"#),
            // Generic high-entropy base64-encoded strings that look like API keys
            compile_pattern("base64_secret", r#"(?i)(?:key|token|secret|password|credential|auth)\s*[=:]\s*["']?([A-Za-z0-9+/=_\-]{40,})["']?"#),
        ];
        let mut patterns: Vec<SecretPattern> = candidates.into_iter().flatten().collect();
        // Detection is the canonical minimum coverage. Keep the broader log
        // patterns above, and automatically inherit every built-in detector so
        // newly recognized vendor formats cannot silently miss redaction.
        for pattern in super::scanner::SecretScanner::default_patterns() {
            let name = match pattern.name.as_str() {
                // Quoted literal detectors are safe for source too: unlike
                // broad keyword heuristics they do not match function calls.
                "Bearer Token" => "bearer_token",
                "Base64 Secret" => "base64_secret",
                _ => "detected_secret",
            };
            if let Some(regex) = pattern.compiled {
                patterns.push(SecretPattern { name, regex });
            }
        }
        patterns
    })
}

/// What kind of content is being redacted. Rust
/// source gets the conservative carve-out (glm capstone: the generic
/// keyword patterns mangle ordinary code — `let secret = compute()` — so
/// the model reads redacted source and has to reconstruct it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionContext {
    /// Unknown or mixed content — the full pattern set applies.
    Generic,
    /// Rust source (the trust gate's `rust_source`
    /// classification, `tool_dispatch/trust_gate.rs:classification_for`).
    /// Broad keyword heuristics are OFF; quoted secret literals and key formats still
    /// redact everywhere.
    RustSource,
}

/// Pattern names exempted in [`RedactionContext::RustSource`] — the generic
/// keyword patterns that fire on ordinary Rust (`api_key = some_fn()`,
/// `let secret = compute()`, keyword-named consts). Everything else —
/// PEM blocks, AWS/GitHub/GitLab/Google/Stripe/Slack/OpenAI key formats,
/// sk-/key-/token- prefixes, JWTs, DB connection strings — still redacts
/// everywhere, first-party source included.
const RUST_SOURCE_EXEMPT: &[&str] = &[
    "api_key",
    "bearer_token",
    "password",
    "env_token",
    "base64_secret",
];

/// Redact secrets from a string with a content-classification carve-out.
pub fn redact_secrets_with_context(input: &str, context: RedactionContext) -> Cow<'_, str> {
    let mut result = Cow::Borrowed(input);

    for pattern in get_patterns() {
        if context == RedactionContext::RustSource && RUST_SOURCE_EXEMPT.contains(&pattern.name) {
            continue;
        }
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

/// Redact secrets from a string
///
/// Log/telemetry redaction: broad keyword heuristics, whole-match
/// replacement. NEVER use this on text a model reads as file or command
/// content — it rewrites ordinary code (`tokens = text.split(SEP)` became
/// `env_token=[REDACTED]`). Model-facing content goes through
/// [`redact_for_model`].
pub fn redact_secrets(input: &str) -> Cow<'_, str> {
    redact_secrets_with_context(input, RedactionContext::Generic)
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

/// Redact a JSON document whose strings a model will read again — a
/// checkpoint's messages and tool results are restored into the model's
/// context on resume. String leaves go through [`redact_for_model`] (secret
/// values only, lines intact, inline markers); values under a sensitive key
/// name are replaced whole, as in [`redact_json`].
pub fn redact_json_for_model(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            let redaction = redact_for_model(s, RedactionContext::Generic);
            if redaction.redacted > 0 {
                *s = redaction.content;
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                redact_json_for_model(item);
            }
        }
        serde_json::Value::Object(obj) => {
            for (key, val) in obj.iter_mut() {
                if is_sensitive_key(key) && val.is_string() {
                    *val = serde_json::Value::String(REDACTED.to_string());
                } else {
                    redact_json_for_model(val);
                }
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

// ── Model-facing redaction ────────────────────────────────────────────────
//
// What a model reads as file or command content must be the content, byte for
// byte, except for a REAL secret value. The log redactor above is built for
// the opposite trade-off (over-redaction is harmless in a log line) and, run
// on serialized tool results, it rewrote ordinary code the model then
// "repaired" in edits (0.9.4 live finding: `tokens = text.split(SEP)` arrived
// as `env_token=[REDACTED]` with the newline swallowed, because `\n` inside a
// JSON string is two non-space characters the value class consumed).
//
// This redactor:
// - replaces only the secret VALUE span, never the key or the rest of the line;
// - works on real text (JSON results are redacted per string leaf), so a line
//   terminator is never part of a match and the line count is preserved;
// - matches known credential formats, PEM private-key bodies, credentials in
//   connection URLs, and literals assigned to secret-named keys — never an
//   expression (`api_key = get_key()`, `password_field = form["password"]`);
// - marks every replacement inline as `[REDACTED:<kind>]`, so the line itself
//   says it was redacted and must not be "repaired" into the file.

/// Prefix of every model-facing redaction marker (`[REDACTED:<kind>]`).
/// Edit tools refuse to write it into a file that does not already carry it.
pub const MODEL_REDACTION_MARKER_PREFIX: &str = "[REDACTED:";

struct FormatPattern {
    kind: &'static str,
    regex: Regex,
    /// The secret must contain a digit (keeps prose/code identifiers that
    /// happen to share a prefix, e.g. `sk-learn-...`, out).
    needs_digit: bool,
}

static MODEL_FORMAT_PATTERNS: OnceLock<Vec<FormatPattern>> = OnceLock::new();
static MODEL_ASSIGNMENT: OnceLock<Option<Regex>> = OnceLock::new();
static MODEL_PEM_BEGIN: OnceLock<Option<Regex>> = OnceLock::new();
static MODEL_PEM_END: OnceLock<Option<Regex>> = OnceLock::new();

fn build(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .ok()
}

/// Credential formats, each with capture group 1 = the secret span.
fn model_format_patterns() -> &'static Vec<FormatPattern> {
    MODEL_FORMAT_PATTERNS.get_or_init(|| {
        let specs: &[(&str, &str, bool)] = &[
            (
                "openai_key",
                r"(?:^|[^A-Za-z0-9_-])(sk-(?:ant-|proj-)?[A-Za-z0-9_\-]{20,})",
                true,
            ),
            ("github_token", r"\b(gh[pousr]_[A-Za-z0-9_]{16,})", false),
            ("github_token", r"\b(github_pat_[A-Za-z0-9_]{22,})", false),
            ("gitlab_token", r"\b(glpat-[A-Za-z0-9_\-]{20,})", false),
            ("npm_token", r"\b(npm_[A-Za-z0-9]{30,})", false),
            ("aws_access_key", r"\b((?:AKIA|ASIA)[0-9A-Z]{16})\b", false),
            (
                "aws_secret_key",
                r#"(?i)aws.{0,24}?['"]([0-9a-zA-Z/+]{40})['"]"#,
                true,
            ),
            ("google_api_key", r"\b(AIza[A-Za-z0-9_\-]{35})", false),
            (
                "stripe_key",
                r"\b((?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{16,})",
                false,
            ),
            ("slack_token", r"\b(xox[bpsar]-[0-9A-Za-z\-]{10,})", false),
            (
                "slack_webhook",
                r"hooks\.slack\.com/services/(T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+)",
                false,
            ),
            ("azure_account_key", r"AccountKey=([A-Za-z0-9+/=]{20,})", false),
            ("twilio_sid", r"\b(AC[0-9a-f]{32})\b", false),
            (
                "jwt",
                r"\b(eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*)",
                false,
            ),
            ("jwt", r"\b(eyJ[A-Za-z0-9_/+\-]{30,})", false),
            (
                "bearer_token",
                r"(?i)\bbearer[ \t]+([A-Za-z0-9_\-\.=+/]{8,})",
                true,
            ),
            (
                "connection_password",
                r#"(?i)\b(?:postgres(?:ql)?|mysql|mariadb|mongodb(?:\+srv)?|rediss?|valkey|amqps?)://[^:/\s@'"]*:([^@/\s'"]+)@"#,
                false,
            ),
        ];
        specs
            .iter()
            .filter_map(|(kind, pattern, needs_digit)| {
                build(pattern).map(|regex| FormatPattern {
                    kind,
                    regex,
                    needs_digit: *needs_digit,
                })
            })
            .collect()
    })
}

/// `key <op> value` on one line: the key (optionally quoted), the operator,
/// and a quoted literal or a bare token. Backslashes end a bare token, so an
/// escaped `\n` in serialized text is never consumed.
fn model_assignment_regex() -> Option<&'static Regex> {
    MODEL_ASSIGNMENT
        .get_or_init(|| {
            build(
                r#"(?P<key>[A-Za-z_][A-Za-z0-9_.\-]*)["']?[ \t]*(?P<op>:=|=>|=|:)[ \t]*(?P<val>"[^"\\\r\n]*"|'[^'\\\r\n]*'|[^\s"'\\,;(){}\[\]<>`]+)"#,
            )
        })
        .as_ref()
}

fn pem_begin() -> Option<&'static Regex> {
    MODEL_PEM_BEGIN
        .get_or_init(|| build(r"-----BEGIN[ A-Z0-9]*PRIVATE KEY-----"))
        .as_ref()
}

fn pem_end() -> Option<&'static Regex> {
    MODEL_PEM_END
        .get_or_init(|| build(r"-----END[ A-Z0-9]*PRIVATE KEY-----"))
        .as_ref()
}

/// Split an identifier into lowercase words (`_ - .` and camelCase).
fn key_words(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in key.chars() {
        if matches!(ch, '_' | '-' | '.') {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower && !cur.is_empty() {
            words.push(std::mem::take(&mut cur));
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        cur.push(ch.to_ascii_lowercase());
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// The secret kind a key NAMES, judged by its last word(s): `GITHUB_TOKEN`,
/// `client_secret`, `DB_PASSWORD`, `api_key`, `aws_secret_access_key`,
/// `apiKey`. `tokens`, `token_count`, `password_field`, `sort_key` name
/// nothing secret.
fn secret_key_kind(key: &str) -> Option<&'static str> {
    let words = key_words(key);
    let last = words.last()?.as_str();
    let prev = words
        .len()
        .checked_sub(2)
        .and_then(|i| words.get(i))
        .map(String::as_str);
    match last {
        "password" | "passwd" | "pwd" | "passphrase" => Some("password"),
        "pass" if prev.is_some() => Some("password"),
        "secret" => Some("secret"),
        "token" => Some("token"),
        "apikey" => Some("api_key"),
        "credential" | "credentials" => Some("credential"),
        "auth" => Some("auth"),
        "key" => match prev {
            Some(
                "api" | "secret" | "access" | "private" | "signing" | "encryption" | "master"
                | "client" | "app" | "license" | "service",
            ) => Some("api_key"),
            _ => None,
        },
        _ => None,
    }
}

/// Whether a literal assigned to a secret-named key looks like a real secret
/// value rather than a name, placeholder, template or path fragment.
fn looks_like_secret_value(value: &str, password_kind: bool) -> bool {
    let len = value.chars().count();
    if !(8..=1024).contains(&len) || value.chars().any(char::is_whitespace) {
        return false;
    }
    if value.starts_with('$')
        || value.starts_with('{')
        || value.starts_with('<')
        || value.starts_with("%(")
        || value.contains("{{")
    {
        return false;
    }
    let first = value.chars().next().unwrap_or(' ');
    if value.chars().all(|c| c == first) {
        return false; // "********", "xxxxxxxx"
    }
    // Word chains are identifiers, keys, dotted paths: `access_token`,
    // `text.split`, `refresh-token-expired`.
    let is_word_chain = value
        .split(['_', '-', '.'])
        .all(|w| !w.is_empty() && w.chars().all(|c| c.is_ascii_alphabetic()))
        && value.contains(['_', '-', '.']);
    if is_word_chain {
        return false;
    }
    if value.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    if value.chars().all(|c| c.is_ascii_alphabetic()) {
        return password_kind || len >= 16;
    }
    // Symbols without digits (`Ab+Cd/Ef==…`): only long ones.
    len >= 16
}

/// Is `value` an identifier-shaped bare token (a variable, a constant, a
/// dotted attribute chain, a number, a keyword) — i.e. code, not data?
fn is_code_token(value: &str) -> bool {
    let ident = |s: &str| {
        let mut chars = s.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    value.split('.').all(ident) || value.parse::<f64>().is_ok()
}

fn marker(kind: &str) -> String {
    format!("{MODEL_REDACTION_MARKER_PREFIX}{kind}]")
}

/// The redaction outcome for model-facing content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRedaction {
    /// The content with each secret value replaced by `[REDACTED:<kind>]`.
    pub content: String,
    /// Number of secret values replaced.
    pub redacted: usize,
}

/// Redact PEM private-key bodies, one marker per body line, keeping the
/// BEGIN/END lines and every line terminator.
fn redact_pem_bodies(text: &str, count: &mut usize) -> String {
    let (Some(begin), Some(end)) = (pem_begin(), pem_end()) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(b) = begin.find(rest) {
        out.push_str(&rest[..b.end()]);
        let after = &rest[b.end()..];
        let (body, tail) = match end.find(after) {
            Some(e) => (&after[..e.start()], &after[e.start()..]),
            None => (after, ""),
        };
        let mut first = true;
        for segment in body.split('\n') {
            if !first {
                out.push('\n');
            }
            first = false;
            let line = segment.strip_suffix('\r').unwrap_or(segment);
            if line.trim().is_empty() {
                out.push_str(segment);
            } else {
                // Keep any file_read line-number prefix (`   42\t`).
                let prefix_len = crate::tools::line_numbers::numbered_prefix_len(line).unwrap_or(0);
                out.push_str(&line[..prefix_len]);
                out.push_str(&marker("private_key"));
                if segment.ends_with('\r') {
                    out.push('\r');
                }
                *count += 1;
            }
        }
        match end.find(tail) {
            Some(e) => {
                out.push_str(&tail[..e.end()]);
                rest = &tail[e.end()..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Redact one line (no terminator inside); a `file_read` line-number prefix
/// is not part of the line's text.
fn redact_line(line: &str, context: RedactionContext, count: &mut usize) -> String {
    // Collect secret spans, then splice right-to-left.
    let mut spans: Vec<(usize, usize, &'static str)> = Vec::new();
    for pattern in model_format_patterns() {
        for caps in pattern.regex.captures_iter(line) {
            let Some(m) = caps.get(1) else { continue };
            if pattern.needs_digit && !m.as_str().chars().any(|c| c.is_ascii_digit()) {
                continue;
            }
            if m.as_str().starts_with(MODEL_REDACTION_MARKER_PREFIX) {
                continue;
            }
            spans.push((m.start(), m.end(), pattern.kind));
        }
    }
    if context != RedactionContext::RustSource {
        if let Some(re) = model_assignment_regex() {
            let text_start = crate::tools::line_numbers::numbered_prefix_len(line).unwrap_or(0);
            for caps in re.captures_iter(line) {
                let (Some(key), Some(op), Some(val)) =
                    (caps.name("key"), caps.name("op"), caps.name("val"))
                else {
                    continue;
                };
                let Some(kind) = secret_key_kind(key.as_str()) else {
                    continue;
                };
                // `==`, `!=`, `<=`, `>=` are comparisons, not assignments.
                let before_op = line[..op.start()].chars().next_back();
                if op.as_str() == "="
                    && (line[op.end()..].starts_with('=')
                        || matches!(before_op, Some('=' | '!' | '<' | '>')))
                {
                    continue;
                }
                let raw = val.as_str();
                let password_kind = kind == "password";
                let quoted = raw.len() >= 2
                    && (raw.starts_with('"') || raw.starts_with('\''))
                    && raw.ends_with(&raw[..1]);
                let (inner_start, inner) = if quoted {
                    (val.start() + 1, &raw[1..raw.len() - 1])
                } else {
                    (val.start(), raw)
                };
                if !quoted {
                    // A bare token must end the statement: `TOKEN=abc123` or
                    // `password: hunter2secret`, not `token = tok2 + 1`.
                    let rest = line[val.end()..].trim_start();
                    let ends = rest.is_empty()
                        || rest.starts_with('#')
                        || rest.starts_with("//")
                        || rest.starts_with(';')
                        || rest.starts_with("\\n")
                        || rest.starts_with("\\r");
                    if !ends {
                        continue;
                    }
                    // Identifier-shaped tokens are code (`token = next_token2`)
                    // unless the key starts the line unindented (`.env`,
                    // `export X=…`) or it is a YAML-style `key: value`.
                    if is_code_token(inner) {
                        let lead = line
                            .get(text_start..key.start())
                            .unwrap_or("")
                            .trim_start_matches("export ");
                        let env_style = lead.is_empty() && op.as_str() == "=";
                        let yaml_style = lead.trim().is_empty() && op.as_str() == ":";
                        if !(env_style || yaml_style) {
                            continue;
                        }
                    }
                }
                if !looks_like_secret_value(inner, password_kind) {
                    continue;
                }
                spans.push((inner_start, inner_start + inner.len(), kind));
            }
        }
    }
    if spans.is_empty() {
        return line.to_string();
    }
    // Merge overlaps (keep the earliest start, widest end).
    spans.sort_by_key(|s| (s.0, std::cmp::Reverse(s.1)));
    let mut merged: Vec<(usize, usize, &'static str)> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.0 < last.1 => last.1 = last.1.max(span.1),
            _ => merged.push(span),
        }
    }
    let mut out = String::with_capacity(line.len());
    let mut pos = 0;
    for (start, end, kind) in merged {
        out.push_str(&line[pos..start]);
        out.push_str(&marker(kind));
        *count += 1;
        pos = end;
    }
    out.push_str(&line[pos..]);
    out
}

/// Redact a plain text for a model: PEM bodies, then line by line. Every
/// line terminator is preserved.
fn redact_text_for_model(text: &str, context: RedactionContext, count: &mut usize) -> String {
    let text = redact_pem_bodies(text, count);
    let mut out = String::with_capacity(text.len());
    for segment in text.split_inclusive('\n') {
        let (body, term) = match segment.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match segment.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (segment, ""),
            },
        };
        out.push_str(&redact_line(body, context, count));
        out.push_str(term);
    }
    out
}

fn redact_json_leaves_for_model(
    value: &mut serde_json::Value,
    parent_key: Option<&str>,
    context: RedactionContext,
    count: &mut usize,
) {
    match value {
        serde_json::Value::String(s) => {
            // A whole leaf under a secret-named field (`"access_token": "…"`).
            if let Some(kind) = parent_key.and_then(secret_key_kind) {
                if !s.contains('\n')
                    && !s.starts_with(MODEL_REDACTION_MARKER_PREFIX)
                    && looks_like_secret_value(s, kind == "password")
                {
                    *s = marker(kind);
                    *count += 1;
                    return;
                }
            }
            let before = *count;
            let redacted = redact_text_for_model(s, context, count);
            if *count != before {
                *s = redacted;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_leaves_for_model(item, parent_key, context, count);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                redact_json_leaves_for_model(item, Some(key.as_str()), context, count);
            }
        }
        _ => {}
    }
}

/// Redact secret VALUES from content a model will read (tool results, file
/// excerpts, recovery context). Everything that is not a secret value is
/// delivered byte for byte; each replacement is an inline
/// `[REDACTED:<kind>]` marker on the line it came from, and no line
/// terminator is ever consumed.
///
/// A JSON object/array result is redacted per string leaf (so its `\n`
/// escapes are real line breaks while matching) and re-serialized only when
/// something was actually redacted.
pub fn redact_for_model(input: &str, context: RedactionContext) -> ModelRedaction {
    let mut count = 0usize;
    let trimmed = input.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(input) {
            redact_json_leaves_for_model(&mut value, None, context, &mut count);
            if count == 0 {
                return ModelRedaction {
                    content: input.to_string(),
                    redacted: 0,
                };
            }
            let pretty = trimmed.starts_with("{\n") || trimmed.starts_with("[\n");
            let content = if pretty {
                serde_json::to_string_pretty(&value)
            } else {
                serde_json::to_string(&value)
            }
            .unwrap_or_else(|_| input.to_string());
            return ModelRedaction {
                content,
                redacted: count,
            };
        }
    }
    let content = redact_text_for_model(input, context, &mut count);
    if count == 0 {
        return ModelRedaction {
            content: input.to_string(),
            redacted: 0,
        };
    }
    ModelRedaction {
        content,
        redacted: count,
    }
}

/// One-line note naming a model-facing redaction, so the model knows the
/// markers are not file text and must never be written back.
pub fn model_redaction_note(redacted: usize) -> String {
    format!(
        "[redaction: {redacted} secret value(s) in this output were replaced with \
         [REDACTED:<kind>] markers on their own lines; every other byte is the real content. \
         The file still holds the real values — never write a marker into a file.]"
    )
}

/// A wrapper for logging that auto-redacts (test helper)
#[cfg(test)]
pub fn safe_log(message: &str) -> String {
    redact_secrets(message).into_owned()
}

#[cfg(test)]
#[path = "../../tests/unit/safety/redact/redact_test.rs"]
mod tests;
