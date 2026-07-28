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

// Dangerous command patterns with regex for robust matching.
// Each tuple contains (regex pattern, human-readable description).
//
// These are matched against the QUOTE-MASKED normalization of the command
// (quoted segments left as opaque placeholders), so quoted prose — commit
// messages, echoed text — cannot trip them. Patterns that must inspect
// quoted PAYLOADS (python -c '…', eval "…") live in
// [`PAYLOAD_COMMAND_PATTERNS`] instead, which is matched against the
// quote-restored form.
pub(crate) static DANGEROUS_COMMAND_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> =
    LazyLock::new(|| {
        vec![
            // rm -rf / variants (handles multiple slashes, spaces, flags, and
            // parent-dir escape to root). Flags match both -rf and
            // --no-preserve-root (the only form that actually executes on GNU
            // coreutils, and the one a naive -[a-z]+ pattern would miss).
            // Targets are deliberately narrow: `/` or `/*` (unanchored, so
            // any absolute rm target still matches, as before) and parent
            // traversals of 2+ levels (`../..`, `../../..`). Bare globs
            // (`rm *.log`, `rm -f *.tmp`) and single-parent operands
            // (`rm ../sibling-file`, `rm -rf ../old-build`) are everyday
            // usage and are NOT matched here.
            (
                Regex::new(r"rm\s+(--?[a-z-]+\s+)*(/\*?|\.\.(/\.\.)+/?)").expect("Invalid regex"),
                "rm -rf / or ../../.. (destructive deletion)",
            ),
            // mkfs - format filesystem. Requires a /dev/ block-device target
            // so text searches (`grep -rn "mkfs" src/`) don't false-positive.
            (
                Regex::new(r"\bmkfs(\.[a-z0-9]+)?\s+(-\S+\s+)*(-t\s+\S+\s+)?/dev/")
                    .expect("Invalid regex"),
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
            // Redirect output to system directories (/etc, /usr, /boot, /sys, /var, /sbin)
            (
                Regex::new(r">\s*/(etc|usr|boot|sys|sbin|var)/").expect("Invalid regex"),
                "redirect to system directory",
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
            // Alternative chown -R pattern: recursive chown targeting root or
            // an absolute system path. Project-relative recursive chown
            // (`chown -R "$USER" node_modules`) is everyday usage and passes.
            (
                Regex::new(
                    r"chown\s+-[rR]\S*\s+\S*\s+(/|/etc|/usr|/var|/bin|/sbin|/lib|/boot|/sys|/proc)",
                )
                .expect("Invalid regex"),
                "recursive chown on system path",
            ),
            // Pipe remote content to a shell. The shell word must be
            // followed by whitespace or end-of-string so checksum
            // verification pipelines (`curl -sL file | shasum -a 256 -c -`,
            // `sha256sum`) don't false-positive, and the `(\|[^|]*)*`
            // middle closes the tee evasion (`curl url | tee x | sh`) for
            // both curl and wget.
            (
                Regex::new(r"(curl|wget)\s+[^|]*(\|[^|]*)*\|\s*(?:ba|z|k|da)?sh(?:\s|$)")
                    .expect("Invalid regex"),
                "pipe remote content to shell",
            ),
            // wget -O- piped to shell (same shell-word boundary as above)
            (
                Regex::new(r"wget\s+(-[a-z]+\s+)*-O\s*-\[^|]*\|\s*(?:ba)?sh(?:\s|$)")
                    .expect("Invalid regex"),
                "wget -O- | sh",
            ),
            // curl with execution flag (same shell-word boundary as above)
            (
                Regex::new(r"curl\s+.*\|\s*(?:ba|z)?sh(?:\s|$)").expect("Invalid regex"),
                "curl | sh",
            ),
            // nc (netcat) reverse shells
            (
                Regex::new(r"\bnc\s+.*-e\s+(/bin/)?(sh|bash)").expect("Invalid regex"),
                "netcat reverse shell",
            ),
        ]
    });

// Patterns that must inspect QUOTED payloads — the dangerous content lives
// inside the quotes (`python3 -c "…"`, `eval "$(…)"`), so they are matched
// against the quote-restored / dequoted forms, unlike
// [`DANGEROUS_COMMAND_PATTERNS`] which match the quote-masked scaffold.
pub(crate) static PAYLOAD_COMMAND_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(
    || {
        vec![
            // Python/perl/ruby one-liners that fetch and execute remote code.
            // Matches python versioned spellings (python3, python3.11) and
            // only flags urllib.request/urllib2 with an actual fetch call
            // (urlopen( or Request( — `request(` after lowercasing), not
            // urllib.parse et al., which are string utilities.
            (
                Regex::new(
                    r#"\b(python[0-9.]*|perl|ruby)\s+(-[a-z]+\s+)*-c\s*['"].*urllib(2|\.request).*(urlopen|request\s*\()"#,
                )
                .expect("Invalid regex"),
                "remote code execution via scripting language",
            ),
            // eval invoking a network tool (word-boundaried so
            // `eval echo sync` passes — `nc` inside `sync` is not a match).
            // Command substitution in eval is handled separately in
            // validation.rs (known-safe program allow-list).
            (
                Regex::new(r"\beval\s+[^;&\n]*\b(curl|wget|nc)\b").expect("Invalid regex"),
                "eval with network tool",
            ),
        ]
    },
);

// Pattern to detect base64-encoded command execution. The piped-to shell
// word must be followed by whitespace or end-of-string so checksum tools
// (`shasum`, `sha256sum`) after a decode pipe don't false-positive.
pub(crate) static BASE64_EXEC_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(base64\s+(-[a-z]+\s+)*(-d|--decode)|base64\s+-d|--decode\s+<|base64\s+<<<).*\|\s*((?:ba|z)?sh|perl|python[0-9.]*|exec|ruby)(\s|$)"#)
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
