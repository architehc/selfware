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
            // A backslash is part of the value (`password=ab\cdefgh`) unless
            // it starts a JSON escape (`\n`, `\r`, `\t`, `\"`), so serialized
            // text never has its next line swallowed.
            compile_pattern("password", r#"(?i)(password|passwd|pwd|secret)\s*[=:]\s*["']?((?:[^\s"'\[\\]|\\[^nrt"\s]){6,})["']?"#),
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
            compile_pattern("env_token", r#"(?i)([A-Z_]*(?:TOKEN|SECRET|KEY|PASSWORD|CREDENTIAL)[A-Z_]*)\s*[=:]\s*["']?((?:[^\s"'\[\\]|\\[^nrt"\s]){16,})["']?"#),
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
    /// classification, `safety/source_context.rs:classification_for`).
    /// Log path: broad keyword heuristics are OFF. Model path: a bare
    /// `key = value` token is code and stays; QUOTED secret literals
    /// (`let password = "hunter2secret";`), key formats, PEM bodies,
    /// connection-string passwords and every scanner detector still redact.
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
// - matches known credential formats, every built-in scanner detector
//   (`SecretScanner::default_patterns`, inherited below), PEM/PGP/PuTTY
//   private-key bodies, credentials in any `scheme://user:pass@host` URL, and
//   literals assigned to secret-named keys — never an expression or a code
//   identifier (`api_key = get_key()`, `password_field = form["password"]`);
// - finds `KEY=value` pairs ANYWHERE on a line (0.9.5 review): after a grep
//   `path:line:` prefix, a diff marker, a `jq -R` quote, several pairs on one
//   command line, a `--flag=`; a bare value ends at whitespace;
// - marks every replacement inline as `[REDACTED:<kind>]`, so the line itself
//   says it was redacted and must not be "repaired" into the file.

/// Prefix of every model-facing redaction marker (`[REDACTED:<kind>]`).
/// Edit tools refuse to write a marker into a file that does not already carry it.
pub const MODEL_REDACTION_MARKER_PREFIX: &str = "[REDACTED:";

/// How a matched value is judged before it is redacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueCheck {
    /// The format alone is proof (vendor prefixes, JWTs).
    Always,
    /// The secret must contain a digit (keeps prose/code identifiers that
    /// happen to share a prefix, e.g. `sk-learn-...`, out).
    Digit,
    /// Opaque credential after a scheme word (`Bearer …`): see [`token_like`].
    TokenLike,
    /// `Basic <b64>`: decodes to `user:password`.
    BasicCredential,
    /// Any literal that is not a template/placeholder or UI prose.
    Literal,
    /// A literal that is also not a code identifier/expression.
    NotCode,
}

struct FormatPattern {
    kind: &'static str,
    regex: Regex,
    check: ValueCheck,
}

/// A secret span on one line: byte range and marker kind.
type Span = (usize, usize, &'static str);

static MODEL_FORMAT_PATTERNS: OnceLock<Vec<FormatPattern>> = OnceLock::new();
static INHERITED_PATTERNS: OnceLock<Vec<FormatPattern>> = OnceLock::new();
static MODEL_PEM_BEGIN: OnceLock<Option<Regex>> = OnceLock::new();
static MODEL_PEM_END: OnceLock<Option<Regex>> = OnceLock::new();
static GREP_PREFIX: OnceLock<Option<Regex>> = OnceLock::new();
static ENV_CHAIN: OnceLock<Option<Regex>> = OnceLock::new();
static CLI_FLAG_TAIL: OnceLock<Option<Regex>> = OnceLock::new();
static NEXT_PAIR: OnceLock<Option<Regex>> = OnceLock::new();
static TYPE_ANNOTATION: OnceLock<Option<Regex>> = OnceLock::new();
static PUTTY_PRIVATE_LINES: OnceLock<Option<Regex>> = OnceLock::new();

fn build(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .ok()
}

fn cached(cell: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    cell.get_or_init(|| build(pattern)).as_ref()
}

/// Credential formats. The secret span is the first capture group that
/// participated in the match.
fn model_format_patterns() -> &'static Vec<FormatPattern> {
    use ValueCheck::*;
    MODEL_FORMAT_PATTERNS.get_or_init(|| {
        let specs: &[(&str, &str, ValueCheck)] = &[
            (
                "openai_key",
                r"(?:^|[^A-Za-z0-9_-])(sk-(?:ant-|proj-)?[A-Za-z0-9_\-]{20,})",
                Digit,
            ),
            ("github_token", r"\b(gh[pousr]_[A-Za-z0-9_]{16,})", Always),
            ("github_token", r"\b(github_pat_[A-Za-z0-9_]{22,})", Always),
            ("gitlab_token", r"\b(glpat-[A-Za-z0-9_\-]{20,})", Always),
            ("npm_token", r"\b(npm_[A-Za-z0-9]{30,})", Always),
            ("pypi_token", r"\b(pypi-AgE[A-Za-z0-9_\-]{20,})", Always),
            ("huggingface_token", r"\b(hf_[A-Za-z0-9]{30,})", Always),
            ("aws_access_key", r"\b((?:AKIA|ASIA)[0-9A-Z]{16})\b", Always),
            (
                "aws_secret_key",
                r#"(?i)aws.{0,24}?['"]([0-9a-zA-Z/+]{40})['"]"#,
                Digit,
            ),
            ("google_api_key", r"\b(AIza[A-Za-z0-9_\-]{35})", Always),
            (
                "stripe_key",
                r"\b((?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{16,})",
                Always,
            ),
            ("webhook_secret", r"\b(whsec_[A-Za-z0-9+/=]{20,})", Always),
            (
                "slack_token",
                r"\b(xox[abcdeprs](?:\.xox[abcdeprs])?-[0-9A-Za-z\-]{10,})",
                Digit,
            ),
            ("slack_token", r"\b(xapp-[0-9]+-[0-9A-Za-z\-]{10,})", Always),
            (
                "slack_webhook",
                r"hooks\.slack\.com/services/(T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+)",
                Always,
            ),
            ("azure_account_key", r"AccountKey=([A-Za-z0-9+/=]{20,})", Always),
            ("twilio_sid", r"\b(AC[0-9a-f]{32})\b", Always),
            (
                "jwt",
                r"\b(eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*)",
                Always,
            ),
            ("jwt", r"\b(eyJ[A-Za-z0-9_/+\-]{30,})", Always),
            (
                "bearer_token",
                r"(?i)\bbearer[ \t]+([A-Za-z0-9_\-\.=+/~]{8,})",
                TokenLike,
            ),
            (
                "basic_auth",
                r"(?i)\bbasic[ \t]+([A-Za-z0-9+/]{8,}={0,2})",
                BasicCredential,
            ),
            (
                "authorization",
                r#"(?i)\b(?:proxy-)?authorization["']?[ \t]*[:=][ \t]*["']?(?:(?:basic|token|digest|negotiate|ntlm|apikey|sso-key)[ \t]+)?([A-Za-z0-9_\-\.=+/~]{8,})"#,
                TokenLike,
            ),
            // PuTTY key files: the MAC over the private blob.
            (
                "private_key",
                r"\bPrivate-(?:MAC|Hash):[ \t]*([0-9A-Fa-f]{16,})",
                Always,
            ),
            // `mysql -pSECRET` (the password glued to -p).
            (
                "password",
                r#"(?:^|[\s;&|(])(?:mysql|mysqldump|mysqladmin|mysqlimport|mysqlshow|mysqlcheck|mariadb)\b[^|;&\r\n]*?[ \t]-p(?:'([^'\s]+)'|"([^"\s]+)"|([^\s'"-][^\s'"]*))"#,
                Literal,
            ),
            // `aws configure set aws_secret_access_key SECRET`.
            (
                "aws_secret_key",
                r"(?i)\baws[ \t]+configure[ \t]+set[ \t]+(?:[\w.-]+\.)?(?:aws_secret_access_key|aws_session_token)[ \t]+([^\s'\x22]+)",
                Literal,
            ),
            // `curl -u user:SECRET`.
            (
                "password",
                r#"\bcurl\b[^\r\n]*?[ \t](?:-u|--user)[ \t=]*["']?[^\s:'"]+:([^\s'"]+)"#,
                Literal,
            ),
        ];
        specs
            .iter()
            .filter_map(|(kind, pattern, check)| {
                build(pattern).map(|regex| FormatPattern {
                    kind,
                    regex,
                    check: *check,
                })
            })
            .collect()
    })
}

/// Every built-in scanner detector, applied on the model path too, so a
/// newly recognized format cannot be detected-but-delivered. The value span
/// is extracted from each match ([`inherited_value_span`]); PEM headers are
/// left to [`redact_key_blocks_in_line`], which redacts the body instead.
fn inherited_patterns() -> &'static Vec<FormatPattern> {
    use ValueCheck::*;
    INHERITED_PATTERNS.get_or_init(|| {
        super::scanner::SecretScanner::default_patterns()
            .into_iter()
            .filter_map(|pattern| {
                let (kind, check): (&'static str, ValueCheck) = match pattern.name.as_str() {
                    "Private Key" => return None,
                    "AWS Access Key" => ("aws_access_key", Always),
                    "AWS Secret Key" => ("aws_secret_key", Literal),
                    "GitHub Token" | "GitHub Fine-Grained Token" => ("github_token", Always),
                    "GitLab Token" => ("gitlab_token", Always),
                    "npm Token" => ("npm_token", Always),
                    "Generic API Key" => ("api_key", Literal),
                    "Google API Key" => ("google_api_key", Always),
                    "Stripe Key" => ("stripe_key", Always),
                    "Password in Code" => ("password", Literal),
                    "Bearer Token" => ("bearer_token", TokenLike),
                    "JWT Token" | "JWT Partial" => ("jwt", Always),
                    "Database URL" => ("connection_password", Literal),
                    // Real Slack tokens carry digits; `xoxb-your-token` is docs.
                    "Slack Token" => ("slack_token", Digit),
                    "Slack Webhook" => ("slack_webhook", Always),
                    "Azure Account Key" => ("azure_account_key", Always),
                    "Twilio SID" => ("twilio_sid", Always),
                    "Base64 Secret" => ("secret", NotCode),
                    other => {
                        // A detector added later: redact it under its own
                        // name (leaked once, at init).
                        let kind = other.to_ascii_lowercase().replace([' ', '-'], "_");
                        (&*Box::leak(kind.into_boxed_str()), Literal)
                    }
                };
                pattern
                    .compiled
                    .map(|regex| FormatPattern { kind, regex, check })
            })
            .collect()
    })
}

/// The secret value inside a scanner match: a quoted literal at the end of
/// the match (`password = "…"`), else what follows the last `:`/`=`/space
/// (`auth=…`, `Bearer …`, `user:…@`), else the whole match (a bare token).
fn inherited_value_span(m: regex::Match<'_>) -> Option<(usize, usize)> {
    let s = m.as_str();
    let base = m.start();
    if let Some(q) = s.chars().next_back().filter(|c| matches!(c, '"' | '\'')) {
        let inner_end = s.len() - 1;
        let open = s[..inner_end].rfind(q)?;
        return (open + 1 < inner_end).then_some((base + open + 1, base + inner_end));
    }
    let body = s.strip_suffix('@').unwrap_or(s);
    let mut start = body
        .rfind(|c: char| c.is_whitespace() || c == ':' || c == '=')
        .map_or(0, |i| i + 1);
    if body[start..].starts_with(['"', '\'']) {
        start += 1;
    }
    (start < body.len()).then_some((base + start, base + body.len()))
}

fn pem_begin() -> Option<&'static Regex> {
    cached(
        &MODEL_PEM_BEGIN,
        r"-----BEGIN[ A-Z0-9]*PRIVATE KEY(?: BLOCK)?-----",
    )
}

fn pem_end() -> Option<&'static Regex> {
    cached(
        &MODEL_PEM_END,
        r"-----END[ A-Z0-9]*PRIVATE KEY(?: BLOCK)?-----",
    )
}

/// Where a line's own text starts: after a `file_read` line-number prefix
/// (`   42\t`) and a grep/rg `path:line:` / `path:line:col:` match prefix or
/// `path-line-` context prefix.
fn logical_start(line: &str) -> usize {
    let n = crate::tools::line_numbers::numbered_prefix_len(line).unwrap_or(0);
    match cached(&GREP_PREFIX, r"^(?:[^\s:=]+:\d+:(?:\d+:)?|[^\s:=]+?-\d+-)")
        .and_then(|re| re.find(&line[n..]))
    {
        Some(m) => n + m.end(),
        None => n,
    }
}

/// Split an identifier into lowercase words (`_ - .` and camelCase).
fn key_words(key: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for ch in key.chars() {
        if matches!(ch, '_' | '-' | '.' | ':') {
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

/// Last words that make a secret-named key describe something ABOUT the
/// secret (`password_field`, `token_count`, `api_key_env`, `auth_url`)
/// rather than hold it.
const KEY_METADATA_WORDS: &[&str] = &[
    "field",
    "fields",
    "label",
    "placeholder",
    "hint",
    "prompt",
    "message",
    "msg",
    "error",
    "err",
    "errors",
    "name",
    "names",
    "path",
    "paths",
    "file",
    "filename",
    "files",
    "dir",
    "directory",
    "url",
    "uri",
    "urls",
    "endpoint",
    "host",
    "type",
    "types",
    "kind",
    "count",
    "len",
    "length",
    "size",
    "env",
    "var",
    "vars",
    "header",
    "headers",
    "param",
    "params",
    "policy",
    "regex",
    "re",
    "pattern",
    "min",
    "max",
    "limit",
    "budget",
    "required",
    "enabled",
    "enable",
    "disabled",
    "expiry",
    "expires",
    "expiration",
    "exp",
    "ttl",
    "timeout",
    "lifetime",
    "format",
    "fmt",
    "mode",
    "strength",
    "id",
    "ids",
    "index",
    "idx",
    "col",
    "column",
    "prefix",
    "suffix",
    "algorithm",
    "algo",
    "alg",
    "sock",
    "socket",
    "helper",
    "cmd",
    "command",
    "provider",
    "source",
    "method",
    "methods",
    "backend",
    "flag",
    "flags",
    "option",
    "options",
    "opts",
    "validator",
    "validation",
    "valid",
    "changed",
    "updated",
    "created",
    "at",
    "time",
    "date",
    "version",
    "usage",
    "used",
    "scope",
    "scopes",
    "issuer",
    "audience",
    "user",
    "username",
    "login",
    "ref",
    "arn",
    "location",
    "help",
    "description",
    "desc",
    "title",
    "text",
    "link",
    "button",
    "btn",
    "input",
    "form",
    "class",
    "style",
    "icon",
    "rule",
    "rules",
    "check",
    "schema",
    "store",
    "manager",
    "service",
    "script",
];

/// Words that follow an api/secret-ish qualifier to name a key (`api_key`,
/// `client-key-data`, `SECRET_KEY_BASE`, `HMAC_KEY`); `sort_key`,
/// `primary_key`, `public_key` name nothing secret.
const SECRET_KEY_QUALIFIERS: &[&str] = &[
    "api",
    "secret",
    "access",
    "private",
    "signing",
    "sign",
    "encryption",
    "enc",
    "master",
    "client",
    "app",
    "license",
    "service",
    "hmac",
    "session",
    "auth",
    "account",
    "shared",
    "deploy",
    "ssh",
    "gpg",
    "pgp",
    "crypto",
    "cipher",
    "storage",
    "admin",
    "root",
    "webhook",
    "jwt",
    "aes",
    "rsa",
    "subscription",
    "consumer",
    "decryption",
    "server",
    "mac",
    "x",
];

/// The secret kind a key NAMES: a secret word anywhere in it (`HMAC_KEY`,
/// `SECRET_KEY_BASE`, `API_KEY_PROD`, `_authToken`, `client-key-data`),
/// unless its last word says it describes the secret (`password_field`,
/// `token_count`). `tokens`, `sort_key`, `PWD`/`OLDPWD` name nothing secret.
fn secret_key_kind(key: &str) -> Option<&'static str> {
    let words = key_words(key);
    let last = words.last()?.as_str();
    if KEY_METADATA_WORDS.contains(&last) {
        return None;
    }
    let last_idx = words.len() - 1;
    let mut kind = None;
    for (idx, word) in words.iter().enumerate() {
        let prev = idx.checked_sub(1).map(|p| words[p].as_str());
        let found = match word.as_str() {
            "password" | "passwd" | "passphrase" | "passwords" => Some("password"),
            "pwd" if prev.is_some_and(|p| p != "old") => Some("password"),
            "pass" if prev.is_some() && idx == last_idx => Some("password"),
            "secret" | "clientsecret" => Some("secret"),
            "token" | "authtoken" | "accesstoken" | "refreshtoken" | "apitoken" | "idtoken" => {
                Some("token")
            }
            "apikey" | "accesskey" | "secretkey" | "privatekey" | "sessionkey" | "signingkey"
            | "masterkey" | "hmac" => Some("api_key"),
            "credential" | "credentials" | "creds" => Some("credential"),
            "auth" | "authorization" => Some("auth"),
            "bearer" | "jwt" => Some("token"),
            "cookie" => Some("cookie"),
            "key" if prev.is_some_and(|p| SECRET_KEY_QUALIFIERS.contains(&p)) => Some("api_key"),
            _ => None,
        };
        if found.is_some() {
            kind = found;
        }
    }
    kind
}

/// A chain of words (`access_token`, `text.split`, `phi.state.v1`,
/// `base64_secret`): each segment letters with at most three trailing
/// digits, and at most one segment carrying digits — a name, not a secret.
fn is_word_chain(value: &str) -> bool {
    if !value.contains(['_', '-', '.']) {
        return false;
    }
    let mut with_digits = 0;
    value.split(['_', '-', '.']).all(|w| {
        let letters = w.trim_end_matches(|c: char| c.is_ascii_digit());
        let digits = w.len() - letters.len();
        if digits > 0 {
            with_digits += 1;
        }
        !w.is_empty() && letters.chars().all(|c| c.is_ascii_alphabetic()) && digits <= 3
    }) && with_digits <= 1
}

/// A template, placeholder, masked or elided value — never a real secret.
fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let first = value.chars().next().unwrap_or(' ');
    value.starts_with(['$', '{', '<', '%'])
        || value.contains("{{")
        || value.contains("${")
        || value.contains("$(")
        || value.contains("****")
        || value.contains("...")
        || value.contains('…')
        || lower.contains("xxxx")
        || value.contains(MODEL_REDACTION_MARKER_PREFIX)
        || value.chars().all(|c| c == first)
        // Documentation stand-ins (`scheme://user:pass@host`).
        || matches!(
            lower.as_str(),
            "pass" | "password" | "passwd" | "pwd" | "secret" | "changeme" | "token"
        )
}

/// UI text, not a passphrase (`"Enter your password"`).
fn is_prose(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "enter ",
        "your ",
        "please",
        "invalid",
        "must ",
        " is ",
        "the ",
        " not ",
        "incorrect",
        "wrong",
        "required",
        "%s",
        "%d",
        "{",
    ]
    .iter()
    .any(|w| lower.contains(w))
        || value.ends_with([':', '?', '.', '!'])
}

/// A filesystem path (`~/.ssh/id_rsa.pub`, `/run/secrets/db`).
fn is_path_like(value: &str) -> bool {
    if value.starts_with("~/") || value.starts_with("./") || value.starts_with("../") {
        return true;
    }
    value.starts_with('/')
        && !value.contains(['+', '='])
        && value.matches('/').count() >= 2
        && value[1..].split('/').all(|seg| {
            seg.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'))
        })
}

/// Is `value` code — an identifier, a dotted/`::` chain of identifiers, a
/// number or a keyword (`get_key`, `config.api_key`, `API_KEY`,
/// `next_token2`, `sha256_hex`, `None`) — rather than a literal secret? Each
/// word of an identifier is letters with at most three trailing digits, and
/// at most one word carries digits (`Zq8Xw3Lm9…` is random, not a name).
fn is_code_identifier(value: &str) -> bool {
    if value.parse::<f64>().is_ok() {
        return true;
    }
    let ident = |seg: &str| {
        let starts_ok = seg
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !starts_ok || !seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return false;
        }
        let mut words_with_digits = 0;
        key_words(seg).iter().all(|w| {
            let letters = w.trim_end_matches(|c: char| c.is_ascii_digit());
            let digits = w.len() - letters.len();
            if digits > 0 {
                words_with_digits += 1;
            }
            letters.chars().all(|c| c.is_ascii_lowercase()) && digits <= 3
        }) && words_with_digits <= 1
    };
    value.split("::").all(|part| part.split('.').all(ident))
}

/// An opaque credential after a scheme word: long, or with a digit, mixed
/// case or base64 symbols — not prose (`Bearer authentication`) or an
/// identifier chain (`Bearer token_value`).
fn token_like(value: &str) -> bool {
    let len = value.chars().count();
    if len < 8 || is_placeholder(value) || is_word_chain(value) {
        return false;
    }
    let has_upper = value.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = value.chars().any(|c| c.is_ascii_lowercase());
    value.chars().any(|c| c.is_ascii_digit())
        || len >= 20
        || (len >= 12 && has_upper && has_lower)
        || value.contains(['+', '/', '=', '~'])
}

/// `Basic <b64>` whose payload is `user:password` (prose like `Basic
/// search/replace` does not decode to that; `Authorization: Basic …` is
/// redacted by the authorization pattern regardless).
fn basic_credential(value: &str) -> bool {
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value));
    decoded
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .is_some_and(|s| s.contains(':'))
}

fn accepts(check: ValueCheck, value: &str) -> bool {
    if value.is_empty() || value.contains(MODEL_REDACTION_MARKER_PREFIX) {
        return false;
    }
    let literal =
        || !(is_placeholder(value) || (value.contains(char::is_whitespace) && is_prose(value)));
    match check {
        ValueCheck::Always => true,
        ValueCheck::Digit => value.chars().any(|c| c.is_ascii_digit()),
        ValueCheck::TokenLike => token_like(value),
        ValueCheck::BasicCredential => basic_credential(value),
        ValueCheck::Literal => literal(),
        ValueCheck::NotCode => literal() && !is_code_identifier(value) && !is_word_chain(value),
    }
}

/// Whether a literal assigned to a secret-named key looks like a real secret
/// value rather than a name, placeholder, template, path or UI text.
/// `quoted`: a string literal, where a secret-named passphrase may contain
/// spaces.
fn looks_like_secret_value(value: &str, kind: &str, quoted: bool) -> bool {
    let len = value.chars().count();
    if !(8..=4096).contains(&len)
        || is_placeholder(value)
        || is_path_like(value)
        || value.contains("://")
    {
        return false;
    }
    let password_kind = kind == "password";
    if value.chars().any(char::is_whitespace) {
        return quoted
            && (password_kind || kind == "secret")
            && len <= 256
            && !value.contains(['\t', '\n', '\r'])
            && !is_prose(value);
    }
    // Word chains are identifiers, keys, dotted paths: `access_token`,
    // `text.split`, `refresh-token-expired`.
    if is_word_chain(value) {
        return false;
    }
    if value.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    password_kind || len >= 16
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

/// Whether the rest of a BEGIN line carries key material (an escaped
/// `\nMIIE…` in a JSON string) rather than code punctuation (`";`).
fn carries_key_material(segment: &str) -> bool {
    let mut run = 0;
    segment.chars().any(|c| {
        run = if c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=') {
            run + 1
        } else {
            0
        };
        run >= 8
    })
}

/// A line that can belong to an armored key body: base64-ish without
/// spaces, an armor header (`Proc-Type: …`, `Version: …`), or blank.
fn is_key_body_line(line: &str) -> bool {
    let text = line[logical_start(line)..].trim();
    text.is_empty()
        || text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '_' | '-'))
        || text.split_once(": ").is_some_and(|(k, _)| {
            !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

/// Multi-line private-key state carried from line to line — and from one
/// JSON leaf to the next under the same key, since grep matches split a PEM
/// block into one leaf per line.
#[derive(Debug, Clone, Copy, Default)]
struct KeyBlockState {
    /// Inside a PEM/PGP body (BEGIN seen, END not yet).
    in_pem: bool,
    /// PuTTY `Private-Lines` still to redact.
    putty_remaining: usize,
}

/// Redact private-key material on one line: PEM/PGP bodies (BEGIN/END kept;
/// a body inside one JSON string becomes one marker) and PuTTY private
/// lines, one marker per body line. Without an END (truncated output) a
/// body runs while lines look like key material, so code after a BEGIN
/// string literal stays readable.
fn redact_key_blocks_in_line(line: &str, state: &mut KeyBlockState, count: &mut usize) -> String {
    let (Some(begin), Some(end)) = (pem_begin(), pem_end()) else {
        return line.to_string();
    };
    let prefix = logical_start(line);
    let whole_line = |count: &mut usize| {
        if line[prefix..].trim().is_empty() {
            line.to_string()
        } else {
            *count += 1;
            format!("{}{}", &line[..prefix], marker("private_key"))
        }
    };
    if state.putty_remaining > 0 {
        state.putty_remaining -= 1;
        return whole_line(count);
    }
    if let Some(n) = putty_private_lines(line) {
        state.putty_remaining = n;
    }
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    if state.in_pem {
        match end.find(rest) {
            Some(e) => {
                let body = &rest[..e.start()];
                let keep = prefix.min(body.len());
                out.push_str(&body[..keep]);
                if body[keep..].trim().is_empty() {
                    out.push_str(&body[keep..]);
                } else {
                    out.push_str(&marker("private_key"));
                    *count += 1;
                }
                out.push_str(e.as_str());
                rest = &rest[e.end()..];
                state.in_pem = false;
            }
            None if is_key_body_line(line) => return whole_line(count),
            // Truncated body: this line is not key material.
            None => state.in_pem = false,
        }
    }
    while let Some(b) = begin.find(rest) {
        out.push_str(&rest[..b.end()]);
        let after = &rest[b.end()..];
        let (body, closing) = match end.find(after) {
            Some(e) => (&after[..e.start()], Some(e)),
            None => (after, None),
        };
        if carries_key_material(body) {
            out.push_str(&marker("private_key"));
            *count += 1;
        } else {
            out.push_str(body);
        }
        match closing {
            Some(e) => {
                out.push_str(e.as_str());
                rest = &after[e.end()..];
            }
            None => {
                state.in_pem = true;
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// A quoted literal starting at `v` (optionally `r`/`b`/`u`/`br` prefixed,
/// or a Rust raw `r#"…"#`): the inner byte range. `f`-strings are
/// expressions and are not literals.
fn quoted_literal(line: &str, v: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut p = v;
    while p < bytes.len()
        && p - v < 2
        && matches!(bytes[p], b'r' | b'b' | b'u' | b'R' | b'B' | b'U')
    {
        p += 1;
    }
    let raw = bytes[v..p].iter().any(|b| matches!(b, b'r' | b'R'));
    let mut hashes = 0;
    if raw {
        while p < bytes.len() && bytes[p] == b'#' {
            hashes += 1;
            p += 1;
        }
    }
    let q = *bytes.get(p)?;
    if !matches!(q, b'"' | b'\'' | b'`') || (p > v && q == b'`') {
        return None;
    }
    let start = p + 1;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' && !raw {
            i += 2;
            continue;
        }
        if c == q && bytes[i + 1..].iter().take(hashes).all(|b| *b == b'#') {
            return (bytes.len() >= i + 1 + hashes).then_some((start, i));
        }
        i += 1;
    }
    None
}

/// A bare value starting at `v`: up to whitespace or a quote, or a `;`/`&`/`,`
/// that starts the next `key=` pair (`?token=…&x=1`, ADO `Password=…;Encrypt=…`),
/// minus trailing punctuation.
fn bare_value(line: &str, v: usize) -> (usize, usize) {
    let bytes = line.as_bytes();
    let next_pair = cached(&NEXT_PAIR, r"^[ \t]*[A-Za-z_][A-Za-z0-9_. \-]{0,40}?=");
    let mut e = v;
    while e < bytes.len() {
        let c = bytes[e];
        if c.is_ascii_whitespace() || matches!(c, b'"' | b'\'' | b'`') {
            break;
        }
        if matches!(c, b';' | b'&' | b',')
            && next_pair.is_some_and(|re| re.is_match(&line[e + 1..]))
        {
            break;
        }
        e += 1;
    }
    while e > v && matches!(bytes[e - 1], b';' | b',' | b')' | b']' | b'}' | b':' | b'.') {
        e -= 1;
    }
    (v, e)
}

/// Whether the text before a key makes `key=value` DATA (a `.env`/shell/
/// config line, a CLI flag, a diff/grep line, a `jq -R` string) rather than
/// a statement in code, for identifier-shaped values.
fn lead_is_data(line: &str, logical: usize, key_start: usize, op: &str) -> bool {
    let base = if logical <= key_start {
        logical
    } else {
        crate::tools::line_numbers::numbered_prefix_len(line)
            .unwrap_or(0)
            .min(key_start)
    };
    let lead = &line[base..key_start];
    // `--password=…`, `-password …`: a CLI flag.
    if lead == "-" || lead.ends_with("--") || lead.ends_with(" -") {
        return true;
    }
    let mut t = lead;
    // One diff marker (`+`, `-`, ` `, `<`, `>`).
    if let Some(first) = t
        .chars()
        .next()
        .filter(|c| matches!(c, '+' | '-' | ' ' | '<' | '>'))
    {
        t = &t[1..];
        if matches!(first, '<' | '>') {
            t = t.strip_prefix(' ').unwrap_or(t);
        }
    }
    t = t.strip_prefix(['"', '\'']).unwrap_or(t);
    if op == ":" {
        t = t.trim_start();
        t = t.strip_prefix("- ").unwrap_or(t).trim_start();
    } else if t.starts_with([' ', '\t']) {
        // Indented `key = value` is a statement in code.
        return false;
    }
    let t = t.trim_end();
    t.is_empty()
        || matches!(
            t,
            "export" | "set" | "setenv" | "env" | "declare -x" | "typeset -x" | "ENV" | "ARG"
                | "local" | "readonly" | "$" | "#" | "//" | ";" | "--"
        )
        || cached(
            &ENV_CHAIN,
            r"^(?:(?:export|env|set|declare\s+-x)\s+)?[A-Za-z_][A-Za-z0-9_]*=\S*(?:\s+[A-Za-z_][A-Za-z0-9_]*=\S*)*$",
        )
        .is_some_and(|re| re.is_match(t))
        || cached(&CLI_FLAG_TAIL, r"(?:^|\s)--?[A-Za-z][A-Za-z0-9_-]*$")
            .is_some_and(|re| re.is_match(t))
}

/// The secret value of `key <op> value` for a secret-named key at
/// `key_start..key_end`, if there is one.
fn assignment_value(
    line: &str,
    logical: usize,
    key_start: usize,
    key_end: usize,
    kind: &'static str,
    context: RedactionContext,
) -> Option<Span> {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut p = key_end;
    let key_closed_by_quote = p < len && matches!(bytes[p], b'"' | b'\'');
    if key_closed_by_quote {
        p += 1;
    }
    let ws_start = p;
    while p < len && matches!(bytes[p], b' ' | b'\t') {
        p += 1;
    }
    let spaced_before = p > ws_start;
    let rest = &line[p..];
    let (mut op, op_len) = if rest.starts_with(":=") {
        (":=", 2)
    } else if rest.starts_with("=>") {
        ("=>", 2)
    } else if rest.starts_with('=') {
        ("=", 1)
    } else if rest.starts_with(':') {
        (":", 1)
    } else {
        return None;
    };
    let after_op = p + op_len;
    let tail = &line[after_op..];
    if (op == "=" && tail.starts_with(['=', '~'])) || (op == ":" && tail.starts_with([':', '/'])) {
        return None; // `==`, `=~`, `Type::path`, `scheme://`
    }
    let mut v = after_op;
    while v < len && matches!(bytes[v], b' ' | b'\t') {
        v += 1;
    }
    let mut spaced_after = v > after_op;
    // `NAME: Type = value` (Rust/TS/Python annotations).
    if op == ":" {
        if let Some(m) = cached(
            &TYPE_ANNOTATION,
            r#"^[A-Za-z_&'\[(<][^=;"'`]{0,80}?[ \t]=[ \t]*"#,
        )
        .and_then(|re| re.find(&line[v..]))
        {
            if !line[v + m.end()..].starts_with('=') {
                v += m.end();
                op = "=";
                spaced_after = true;
            }
        }
    }
    if v >= len {
        return None;
    }
    if let Some((inner_start, inner_end)) = quoted_literal(line, v) {
        let inner = &line[inner_start..inner_end];
        return looks_like_secret_value(inner, kind, true).then_some((
            inner_start,
            inner_end,
            kind,
        ));
    }
    // Code files: only quoted literals are data; a bare token is code.
    if context == RedactionContext::RustSource {
        return None;
    }
    let (bs, be) = bare_value(line, v);
    let value = &line[bs..be];
    if value.is_empty() || value.contains(['(', ')', '[', ']', '{', '}', '<', '>', '\\']) {
        return None; // an expression, call, index, generic type or escape sequence
    }
    let compact = op == "=" && !spaced_before && !spaced_after;
    if !compact && op != ":" {
        // A spaced bare statement must end at the value: `token = tok2 + 1`
        // is an expression.
        let rest = line[be..].trim_start_matches([';', ',', ' ', '\t']);
        let ends = rest.is_empty()
            || rest.starts_with('#')
            || rest.starts_with("//")
            || rest.starts_with("\\n")
            || rest.starts_with("\\r");
        if !ends {
            return None;
        }
    }
    if is_code_identifier(value) {
        let before_key = line[..key_start].chars().next_back();
        let header_in_string = matches!(before_key, Some('"' | '\'')) && !key_closed_by_quote;
        if !(header_in_string || lead_is_data(line, logical, key_start, op)) {
            return None;
        }
    }
    looks_like_secret_value(value, kind, false).then_some((bs, be, kind))
}

/// Every `key <op> value` with a secret-named key on the line, wherever it
/// starts (overlapping candidates are merged by the caller).
fn assignment_spans(line: &str, context: RedactionContext, spans: &mut Vec<Span>) {
    let bytes = line.as_bytes();
    let logical = logical_start(line);
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    for i in 0..bytes.len() {
        let b = bytes[i];
        if !(b.is_ascii_alphabetic() || b == b'_') || (i > 0 && word(bytes[i - 1])) {
            continue;
        }
        let mut j = i;
        while j < bytes.len() && j - i < 128 && (word(bytes[j]) || matches!(bytes[j], b'.' | b'-'))
        {
            j += 1;
        }
        while j > i && matches!(bytes[j - 1], b'.' | b'-') {
            j -= 1;
        }
        // Cheap pre-check: an operator must follow the key.
        let after = line[j..]
            .trim_start_matches(['"', '\''])
            .trim_start_matches([' ', '\t']);
        if !after.starts_with([':', '=']) {
            continue;
        }
        let Some(kind) = secret_key_kind(&line[i..j]) else {
            continue;
        };
        if let Some(span) = assignment_value(line, logical, i, j, kind, context) {
            spans.push(span);
        }
    }
}

/// `scheme://user:PASSWORD@host` in any scheme. The userinfo runs to the
/// LAST `@` before whitespace/quotes, so a password containing `/` or `@`
/// is redacted whole.
fn url_userinfo_spans(line: &str, spans: &mut Vec<Span>) {
    let mut from = 0;
    while let Some(rel) = line[from..].find("://") {
        let sep = from + rel;
        from = sep + 3;
        if !line[..sep]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            continue;
        }
        let rest_start = sep + 3;
        let rest_end = line[rest_start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | '`'))
            .map_or(line.len(), |i| rest_start + i);
        let rest = &line[rest_start..rest_end];
        let Some(at) = rest.rfind('@') else { continue };
        let userinfo = &rest[..at];
        let Some(colon) = userinfo.find(':') else {
            continue;
        };
        if userinfo[..colon].contains(['/', '?', '#']) {
            continue;
        }
        let password = &userinfo[colon + 1..];
        let host_ok = rest[at + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '[');
        let port_like = {
            let digits = password.len()
                - password
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .len();
            (1..=5).contains(&digits)
                && (password.len() == digits || password[digits..].starts_with('/'))
        };
        if password.is_empty() || !host_ok || port_like || !accepts(ValueCheck::Literal, password) {
            continue;
        }
        let start = rest_start + colon + 1;
        spans.push((start, start + password.len(), "connection_password"));
    }
}

/// Redact one line (no terminator inside).
fn redact_line(line: &str, context: RedactionContext, count: &mut usize) -> String {
    // Collect secret spans (order = marker-kind priority), then splice.
    let mut spans: Vec<Span> = Vec::new();
    for pattern in model_format_patterns() {
        for caps in pattern.regex.captures_iter(line) {
            let Some(m) = caps.iter().skip(1).flatten().next() else {
                continue;
            };
            if accepts(pattern.check, m.as_str()) {
                spans.push((m.start(), m.end(), pattern.kind));
            }
        }
    }
    url_userinfo_spans(line, &mut spans);
    for pattern in inherited_patterns() {
        for m in pattern.regex.find_iter(line) {
            let Some((s, e)) = inherited_value_span(m) else {
                continue;
            };
            if accepts(pattern.check, &line[s..e]) {
                spans.push((s, e, pattern.kind));
            }
        }
    }
    assignment_spans(line, context, &mut spans);
    if spans.is_empty() {
        return line.to_string();
    }
    // Merge overlaps (keep the earliest start, widest end).
    spans.sort_by_key(|s| (s.0, std::cmp::Reverse(s.1)));
    let mut merged: Vec<Span> = Vec::new();
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

/// `Private-Lines: N` in a PuTTY key file: the next N lines are the key.
fn putty_private_lines(line: &str) -> Option<usize> {
    let re = cached(
        &PUTTY_PRIVATE_LINES,
        r"^Private-Lines:[ \t]*(\d{1,5})[ \t]*$",
    )?;
    re.captures(&line[logical_start(line)..])
        .and_then(|c| c[1].parse().ok())
}

/// Redact a plain text for a model, line by line: private-key blocks, then
/// secret values. Every line terminator is preserved.
fn redact_text_for_model(
    text: &str,
    context: RedactionContext,
    state: &mut KeyBlockState,
    count: &mut usize,
) -> String {
    let mut out = String::with_capacity(text.len());
    for segment in text.split_inclusive('\n') {
        let (body, term) = match segment.strip_suffix("\r\n") {
            Some(b) => (b, "\r\n"),
            None => match segment.strip_suffix('\n') {
                Some(b) => (b, "\n"),
                None => (segment, ""),
            },
        };
        let keyed = redact_key_blocks_in_line(body, state, count);
        out.push_str(&redact_line(&keyed, context, count));
        out.push_str(term);
    }
    out
}

fn redact_json_leaves_for_model(
    value: &mut serde_json::Value,
    parent_key: Option<&str>,
    context: RedactionContext,
    states: &mut std::collections::HashMap<String, KeyBlockState>,
    count: &mut usize,
) {
    match value {
        serde_json::Value::String(s) => {
            // A whole leaf under a secret-named field (`"access_token": "…"`).
            if let Some(kind) = parent_key.and_then(secret_key_kind) {
                if !s.contains('\n') && looks_like_secret_value(s, kind, true) {
                    *s = marker(kind);
                    *count += 1;
                    return;
                }
            }
            let state = states
                .entry(parent_key.unwrap_or_default().to_string())
                .or_default();
            let before = *count;
            let redacted = redact_text_for_model(s, context, state, count);
            if *count != before {
                *s = redacted;
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json_leaves_for_model(item, parent_key, context, states, count);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map.iter_mut() {
                redact_json_leaves_for_model(item, Some(key.as_str()), context, states, count);
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
            let mut states = std::collections::HashMap::new();
            redact_json_leaves_for_model(&mut value, None, context, &mut states, &mut count);
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
    let content = redact_text_for_model(input, context, &mut KeyBlockState::default(), &mut count);
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

/// Every `[REDACTED:<kind>]` marker kind the model-facing redactor can emit
/// (edit tools refuse to write these into a file that lacks them).
pub fn model_redaction_kinds() -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = model_format_patterns()
        .iter()
        .chain(inherited_patterns().iter())
        .map(|p| p.kind)
        .chain([
            "private_key",
            "connection_password",
            "password",
            "secret",
            "token",
            "api_key",
            "credential",
            "auth",
            "cookie",
        ])
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
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
pub(crate) mod tests;
