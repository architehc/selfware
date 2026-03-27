//! Safety Checker Types
//!
//! Core types and configuration for the safety checker.

use crate::config::SafetyConfig;
use regex::Regex;
use std::path::PathBuf;
use std::sync::LazyLock;

/// Guards against dangerous tool calls by validating commands, paths, and content.
///
/// Blocks destructive shell commands, path traversal attacks, secret leakage,
/// force pushes to protected branches, SSRF attempts, and unsafe container mounts.
pub struct SafetyChecker {
    pub(crate) config: SafetyConfig,
    /// Working directory for resolving relative paths
    pub(crate) working_dir: PathBuf,
    /// Security scanner for detecting secrets in file content
    pub(crate) security_scanner: crate::safety::scanner::SecurityScanner,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct UrlSafetyOptions {
    pub allow_file_scheme: bool,
    pub allow_localhost: bool,
}

// Dangerous command patterns with regex for robust matching
// Each tuple contains (regex pattern, human-readable description)
pub(crate) static DANGEROUS_COMMAND_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> =
    LazyLock::new(|| {
        vec![
            // rm -rf / variants (handles multiple slashes, spaces, flags, and parent dir escape)
            (
                Regex::new(r"rm\s+(-[a-z]+\s+)*(/+|\*|/\*|\.\.|\.\./\*)").expect("Invalid regex"),
                "rm -rf / or .. (destructive deletion)",
            ),
            // mkfs - format filesystem
            (
                Regex::new(r"\bmkfs(\.[a-z0-9]+)?\b").expect("Invalid regex"),
                "mkfs (format filesystem)",
            ),
            // dd with dangerous targets
            (
                Regex::new(r"\bdd\s+.*\b(if|of)=\s*/dev/(sd|hd|nvme|vd|xvd)")
                    .expect("Invalid regex"),
                "dd to disk device (data destruction)",
            ),
            // Fork bomb variants
            (
                Regex::new(r":\s*\(\s*\)\s*\{.*:\s*\|.*:\s*&.*\}").expect("Invalid regex"),
                "fork bomb",
            ),
            // Overwrite disk devices
            (
                Regex::new(r">\s*/dev/(sd|hd|nvme|vd|xvd)").expect("Invalid regex"),
                "redirect to disk device",
            ),
            // chmod 777 on root
            (
                Regex::new(r"chmod\s+(-[a-zA-Z]+\s+)*777\s+/+").expect("Invalid regex"),
                "chmod 777 / (remove all file permissions)",
            ),
            // chown -R anywhere
            (
                Regex::new(r"chown\s+(-[a-zA-Z]+\s+)*\S+:\S+\s+/").expect("Invalid regex"),
                "chown on system directory",
            ),
            // Alternative chown -R pattern
            (
                Regex::new(r"chown\s+-[rR]").expect("Invalid regex"),
                "recursive chown",
            ),
            // Pipe to shell (curl/wget to sh/bash)
            (
                Regex::new(r"(curl|wget)\s+[^|]*\|\s*(sh|bash|zsh|ksh|dash)")
                    .expect("Invalid regex"),
                "pipe remote content to shell",
            ),
            // wget -O- piped to shell
            (
                Regex::new(r"wget\s+(-[a-z]+\s+)*-O\s*-\[^|]*\|\s*(sh|bash)")
                    .expect("Invalid regex"),
                "wget -O- | sh",
            ),
            // curl with execution flag
            (
                Regex::new(r"curl\s+.*\|\s*(sh|bash|zsh)").expect("Invalid regex"),
                "curl | sh",
            ),
            // Python/perl/ruby one-liners that execute remote code
            (
                Regex::new(r#"(python|perl|ruby)\s+(-[a-z]+\s+)*-c\s*['"].*import\s+urllib"#)
                    .expect("Invalid regex"),
                "remote code execution via scripting language",
            ),
            // nc (netcat) reverse shells
            (
                Regex::new(r"\bnc\s+.*-e\s+(/bin/)?(sh|bash)").expect("Invalid regex"),
                "netcat reverse shell",
            ),
            // eval with suspicious content
            (
                Regex::new(r#"\beval\s+.*(\$\(|`|curl|wget|nc)"#).expect("Invalid regex"),
                "eval with command substitution",
            ),
        ]
    });

// Pattern to detect base64-encoded command execution
pub(crate) static BASE64_EXEC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(base64\s+(-[a-z]+\s+)*(-d|--decode)|base64\s+-d|--decode\s+<|base64\s+<<<).*\|\s*(sh|bash|zsh|perl|python|exec|ruby)"#)
        .expect("Invalid regex")
});

// Pattern to detect hex-encoded command execution
pub(crate) static HEX_EXEC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(xxd\s+-r|-r\s+xxd|printf\s+.*\\x[0-9a-fA-F]{2}.*\|\s*(sh|bash|zsh|perl|python|ruby))"#,
    )
    .expect("Invalid regex")
});

// Pattern to detect other encoding/obfuscation execution patterns
pub(crate) static ENCODED_EXEC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(uudecode|gunzip|zcat|gzip\s+-d)\s+.*\|\s*(base64|sh|bash|zsh|perl|python)"#)
        .expect("Invalid regex")
});

// Pattern to detect suspicious shell variable substitution
#[allow(dead_code)]
pub(crate) static SUSPICIOUS_SUBSTITUTION_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\$['"][^'"]*['"]|\$\{[^}]+\}|\$[a-zA-Z_][a-zA-Z0-9_]*"#).expect("Invalid regex")
});

// Pattern to detect standalone variable execution
#[allow(dead_code)]
pub(crate) static STANDING_VARIABLE_EXEC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\s*\$\s*\{?\s*[A-Za-z_][A-Za-z0-9_]*\s*\}?\s*$"#).expect("Invalid regex")
});
