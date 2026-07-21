//! YOLO Mode - Fully autonomous operation without confirmations
//!
//! Enables the agent to run for extended periods (hours/days) without
//! requiring user intervention. All confirmations are auto-approved
//! with comprehensive audit logging.

// Feature-gated module

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::RwLock;

/// YOLO mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoloConfig {
    /// Whether YOLO mode is enabled
    pub enabled: bool,
    /// Maximum operations before requiring check-in (0 = unlimited)
    pub max_operations: usize,
    /// Maximum time in hours before requiring check-in (0 = unlimited)
    pub max_hours: f64,
    /// Operations that are NEVER auto-approved even in YOLO mode
    pub forbidden_operations: Vec<String>,
    /// Paths that should never be modified
    pub protected_paths: Vec<String>,
    /// Deny-glob patterns (from the safety config's `denied_paths`) that also
    /// apply to `shell_exec` command strings: a shell command that reads a
    /// path matching one of these is gated just like a file operation would be.
    /// File-op path validation already honors these; this closes the gap where
    /// `cat`/`grep`/etc. could exfiltrate a denied path via the shell.
    #[serde(default)]
    pub denied_paths: Vec<String>,
    /// Whether to allow git push operations
    pub allow_git_push: bool,
    /// Whether to allow destructive shell commands
    pub allow_destructive_shell: bool,
    /// Audit log file path
    pub audit_log_path: Option<PathBuf>,
    /// Send periodic status updates (every N operations)
    pub status_interval: usize,
}

impl Default for YoloConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_operations: 0, // Unlimited
            max_hours: 0.0,    // Unlimited
            forbidden_operations: vec![
                // These are NEVER auto-approved
                "rm -rf /".to_string(),
                "rm -rf /*".to_string(),
                "dd if=/dev/zero".to_string(),
                "mkfs".to_string(),
                "> /dev/sda".to_string(),
                "chmod -R 777 /".to_string(),
            ],
            protected_paths: vec![
                "/etc".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                "/boot".to_string(),
                "/root".to_string(),
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
            ],
            denied_paths: Vec::new(),
            allow_git_push: true,
            // SAFETY: Default to false - destructive commands require explicit opt-in
            allow_destructive_shell: false,
            audit_log_path: None,
            status_interval: 100,
        }
    }
}

impl YoloConfig {
    /// Create a YOLO config with sensible defaults for autonomous coding
    ///
    /// This enables autonomous operation for most coding tasks while
    /// requiring confirmation for destructive shell commands.
    pub fn for_coding() -> Self {
        Self {
            enabled: true,
            allow_git_push: false,          // Require explicit push
            allow_destructive_shell: false, // Safer default - require confirmation for rm, etc.
            status_interval: 50,
            ..Default::default()
        }
    }

    /// Create a fully autonomous config for long-running unattended operations
    ///
    /// IMPORTANT: This still disallows destructive shell commands by default.
    /// Use `with_destructive_shell(true)` if you explicitly need that capability.
    ///
    /// # Safety
    /// Even in fully autonomous mode, certain operations are never auto-approved:
    /// - Commands in the `forbidden_operations` list
    /// - Modifications to `protected_paths`
    pub fn fully_autonomous() -> Self {
        Self {
            enabled: true,
            allow_git_push: true,
            allow_destructive_shell: false, // Safer default - use with_destructive_shell() to enable
            status_interval: 100,
            ..Default::default()
        }
    }

    /// Builder method to explicitly enable destructive shell commands
    ///
    /// # Warning
    /// This allows commands like `rm -rf`, `git reset --hard`, etc.
    /// Only use this if you understand the risks and have proper backups.
    pub fn with_destructive_shell(mut self, allow: bool) -> Self {
        self.allow_destructive_shell = allow;
        self
    }

    /// Builder method to enable/disable git push
    pub fn with_git_push(mut self, allow: bool) -> Self {
        self.allow_git_push = allow;
        self
    }

    /// Check if an operation is forbidden
    ///
    /// Uses regex word-boundary matching with whitespace normalization to prevent
    /// bypass via extra whitespace, backslash escapes, or other trivial variations.
    pub fn is_forbidden(&self, operation: &str) -> bool {
        let normalized = normalize_input(operation);
        self.forbidden_operations.iter().any(|f| {
            let pattern = build_boundary_pattern(f);
            Regex::new(&pattern)
                .map(|re| re.is_match(&normalized))
                .unwrap_or(false)
        })
    }

    /// Check if a path is protected
    pub fn is_protected_path(&self, path: &str) -> bool {
        let expanded = expand_home(path);
        self.protected_paths.iter().any(|p| {
            let protected = expand_home(p);
            expanded.starts_with(&protected) || expanded == protected
        })
    }
}

/// Normalize input for security matching.
///
/// Collapses multiple whitespace characters into a single space and trims
/// leading/trailing whitespace. This prevents bypass via extra spacing.
fn normalize_input(input: &str) -> String {
    static WS_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
    WS_RE.replace_all(input.trim(), " ").to_lowercase()
}

/// Build a regex pattern with word boundaries from a literal forbidden string.
///
/// - Splits the pattern on whitespace and escapes each token for regex
/// - Joins tokens with `\s+` for flexible whitespace matching
/// - Adds `\b` word boundaries at start/end when the boundary character is
///   a word character (alphanumeric or underscore), preventing partial-word matches
fn build_boundary_pattern(pattern: &str) -> String {
    let trimmed = pattern.trim().to_lowercase();
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }

    // Escape each token individually, then join with flexible whitespace
    let escaped_tokens: Vec<String> = tokens.iter().map(|t| regex::escape(t)).collect();
    let flexible = escaped_tokens.join(r"\s+");

    // Add word boundaries where appropriate based on the original (unescaped) tokens
    let first_char = tokens.first().and_then(|t| t.chars().next());
    let last_char = tokens.last().and_then(|t| t.chars().last());

    let prefix = if first_char.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        r"\b"
    } else {
        ""
    };
    let suffix = if last_char.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        r"\b"
    } else {
        ""
    };

    format!("(?i){}{}{}", prefix, flexible, suffix)
}

/// Expand ~ to home directory
fn expand_home(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}

/// Audit log entry for YOLO mode operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub operation_id: usize,
    pub tool_name: String,
    pub arguments_summary: String,
    pub auto_approved: bool,
    pub result: AuditResult,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditResult {
    Success,
    Failed(String),
    Blocked(String),
}

/// YOLO mode manager
pub struct YoloManager {
    config: YoloConfig,
    enabled: AtomicBool,
    operation_count: AtomicUsize,
    start_time: RwLock<Option<std::time::Instant>>,
    audit_log: RwLock<Vec<AuditEntry>>,
}

impl YoloManager {
    /// Create a new YOLO manager
    pub fn new(config: YoloConfig) -> Self {
        let enabled = config.enabled;
        Self {
            config,
            enabled: AtomicBool::new(enabled),
            operation_count: AtomicUsize::new(0),
            start_time: RwLock::new(if enabled {
                Some(std::time::Instant::now())
            } else {
                None
            }),
            audit_log: RwLock::new(Vec::new()),
        }
    }

    /// Check if YOLO mode is currently active
    pub fn is_active(&self) -> bool {
        if !self.enabled.load(Ordering::SeqCst) {
            return false;
        }

        // Check operation limit
        if self.config.max_operations > 0
            && self.operation_count.load(Ordering::SeqCst) >= self.config.max_operations
        {
            return false;
        }

        // Check time limit
        if self.config.max_hours > 0.0 {
            if let Ok(start) = self.start_time.read() {
                if let Some(start_time) = *start {
                    let hours = start_time.elapsed().as_secs_f64() / 3600.0;
                    if hours >= self.config.max_hours {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Enable YOLO mode
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::SeqCst);
        if let Ok(mut start) = self.start_time.write() {
            *start = Some(std::time::Instant::now());
        }
        self.operation_count.store(0, Ordering::SeqCst);
    }

    /// Disable YOLO mode
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::SeqCst);
    }

    /// Check if an operation should be auto-approved
    pub fn should_auto_approve(&self, tool_name: &str, args: &serde_json::Value) -> YoloDecision {
        if !self.is_active() {
            return YoloDecision::RequireConfirmation("YOLO mode not active".to_string());
        }

        // Check forbidden operations
        let args_str = serde_json::to_string(args).unwrap_or_default();
        if self.config.is_forbidden(&args_str) {
            return YoloDecision::Block("Operation is in forbidden list".to_string());
        }

        // Check protected paths
        if let Some(path) = extract_path(args) {
            if self.config.is_protected_path(&path) {
                return YoloDecision::Block(format!("Path '{}' is protected", path));
            }
        }

        // Check container_run volume mounts for dangerous host paths
        if tool_name == "container_run" {
            if let Some(volumes) = args.get("volumes").and_then(|v| v.as_array()) {
                for vol in volumes {
                    if let Some(mount) = vol.as_str() {
                        if let Some(reason) = check_volume_mount(mount) {
                            return YoloDecision::Block(reason);
                        }
                    }
                }
            }
        }

        // Check git push
        if tool_name == "git_push" && !self.config.allow_git_push {
            return YoloDecision::RequireConfirmation("Git push requires confirmation".to_string());
        }

        // Check destructive shell commands + best-effort sensitive-path reads.
        if tool_name == "shell_exec" {
            if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                if !self.config.allow_destructive_shell {
                    if is_destructive_command(cmd) {
                        return YoloDecision::RequireConfirmation(
                            "Destructive shell command requires confirmation".to_string(),
                        );
                    }
                    if let Some(secret) = reads_sensitive_path(cmd) {
                        return YoloDecision::RequireConfirmation(format!(
                            "Shell command reads a sensitive path ({secret}) — requires confirmation."
                        ));
                    }
                    if let Some(pattern) = reads_denied_path(cmd, &self.config.denied_paths) {
                        return YoloDecision::RequireConfirmation(format!(
                            "Shell command reads a path matching a denied glob ('{pattern}') — \
                             requires confirmation."
                        ));
                    }
                }
            }
        }

        // Check any tool classified as destructive (file_delete, container_remove, etc.).
        // Tools with a more specific per-argument check above (shell_exec,
        // pty_shell, git_push) are excluded here: those tools are broadly
        // classified `destructive: true` in tool_metadata regardless of the
        // actual command/args, so without this exclusion every single
        // shell_exec call -- including harmless ones like `ls` -- would
        // require confirmation whenever `allow_destructive_shell` is false
        // (the default), which is exactly the setting every production
        // config in this repo uses for headless/unattended runs.
        if !matches!(tool_name, "shell_exec" | "pty_shell" | "git_push") {
            let metadata = crate::safety::default_tool_metadata(tool_name);
            if metadata.destructive && !self.config.allow_destructive_shell {
                return YoloDecision::RequireConfirmation(
                    "Destructive operation requires confirmation".to_string(),
                );
            }
        }

        YoloDecision::AutoApprove
    }

    /// Record an operation in the audit log
    pub fn record_operation(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        auto_approved: bool,
        result: AuditResult,
        duration_ms: u64,
    ) {
        let op_id = self.operation_count.fetch_add(1, Ordering::SeqCst);

        let entry = AuditEntry {
            timestamp: Utc::now(),
            operation_id: op_id,
            tool_name: tool_name.to_string(),
            arguments_summary: summarize_args(args),
            auto_approved,
            result,
            duration_ms,
        };

        // Acquire file lock so in-memory push and file write are atomic.
        let _file_guard = if self.config.audit_log_path.is_some() {
            Some(AUDIT_FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner()))
        } else {
            None
        };

        // Add to in-memory log
        if let Ok(mut log) = self.audit_log.write() {
            log.push(entry.clone());
        }

        // Write to file if configured
        if let Some(ref path) = self.config.audit_log_path {
            let _ = append_to_audit_file(path, &entry);
        }

        // Print status update at intervals
        if self.config.status_interval > 0
            && op_id > 0
            && op_id.is_multiple_of(self.config.status_interval)
        {
            self.print_status();
        }
    }

    /// Get the current operation count
    pub fn operation_count(&self) -> usize {
        self.operation_count.load(Ordering::SeqCst)
    }

    /// Get elapsed time in hours
    pub fn elapsed_hours(&self) -> f64 {
        if let Ok(start) = self.start_time.read() {
            if let Some(start_time) = *start {
                return start_time.elapsed().as_secs_f64() / 3600.0;
            }
        }
        0.0
    }

    /// Print a status update
    pub fn print_status(&self) {
        let ops = self.operation_count();
        let hours = self.elapsed_hours();
        let success_count = self
            .audit_log
            .read()
            .map(|log| {
                log.iter()
                    .filter(|e| matches!(e.result, AuditResult::Success))
                    .count()
            })
            .unwrap_or(0);
        let failed_count = self
            .audit_log
            .read()
            .map(|log| {
                log.iter()
                    .filter(|e| matches!(e.result, AuditResult::Failed(_)))
                    .count()
            })
            .unwrap_or(0);

        eprintln!("\n╔══════════════════════════════════════╗");
        eprintln!("║      YOLO MODE STATUS UPDATE         ║");
        eprintln!("╠══════════════════════════════════════╣");
        eprintln!("║ Operations: {:<6} | Time: {:.1}h      ║", ops, hours);
        eprintln!(
            "║ Success: {:<4} | Failed: {:<4}         ║",
            success_count, failed_count
        );
        eprintln!("╚══════════════════════════════════════╝\n");
    }

    /// Get audit log summary
    pub fn audit_summary(&self) -> AuditSummary {
        let log = self.audit_log.read().unwrap_or_else(|e| e.into_inner());

        let mut tools_used: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut success = 0;
        let mut failed = 0;
        let mut blocked = 0;
        let mut total_duration_ms = 0u64;

        for entry in log.iter() {
            *tools_used.entry(entry.tool_name.clone()).or_insert(0) += 1;
            total_duration_ms += entry.duration_ms;
            match &entry.result {
                AuditResult::Success => success += 1,
                AuditResult::Failed(_) => failed += 1,
                AuditResult::Blocked(_) => blocked += 1,
            }
        }

        AuditSummary {
            total_operations: log.len(),
            success,
            failed,
            blocked,
            tools_used,
            total_duration_ms,
            elapsed_hours: self.elapsed_hours(),
        }
    }

    /// Export audit log to file
    pub fn export_audit_log(&self, path: &std::path::Path) -> std::io::Result<()> {
        let log = self.audit_log.read().unwrap_or_else(|e| e.into_inner());
        let json = serde_json::to_string_pretty(&*log).unwrap_or_default();
        fs::write(path, json)
    }
}

/// Decision from YOLO mode check
#[derive(Debug, Clone, PartialEq)]
pub enum YoloDecision {
    /// Auto-approve the operation
    AutoApprove,
    /// Require user confirmation with reason
    RequireConfirmation(String),
    /// Block the operation entirely
    Block(String),
}

/// Summary of audit log
#[derive(Debug, Clone, Serialize)]
pub struct AuditSummary {
    pub total_operations: usize,
    pub success: usize,
    pub failed: usize,
    pub blocked: usize,
    pub tools_used: std::collections::HashMap<String, usize>,
    pub total_duration_ms: u64,
    pub elapsed_hours: f64,
}

impl std::fmt::Display for AuditSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "YOLO Mode Audit Summary")?;
        writeln!(f, "======================")?;
        writeln!(f, "Total Operations: {}", self.total_operations)?;
        writeln!(f, "  - Success: {}", self.success)?;
        writeln!(f, "  - Failed: {}", self.failed)?;
        writeln!(f, "  - Blocked: {}", self.blocked)?;
        writeln!(f, "Elapsed Time: {:.2} hours", self.elapsed_hours)?;
        writeln!(
            f,
            "Total Duration: {:.1}s",
            self.total_duration_ms as f64 / 1000.0
        )?;
        writeln!(f, "\nTools Used:")?;
        for (tool, count) in &self.tools_used {
            writeln!(f, "  - {}: {}", tool, count)?;
        }
        Ok(())
    }
}

/// Extract path from tool arguments (recursively)
fn extract_path(args: &serde_json::Value) -> Option<String> {
    match args {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                if k == "path" || k == "file" || k == "directory" {
                    if let Some(s) = v.as_str() {
                        return Some(s.to_string());
                    }
                }
                if let Some(res) = extract_path(v) {
                    return Some(res);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(res) = extract_path(v) {
                    return Some(res);
                }
            }
            None
        }
        _ => None,
    }
}

/// Check a container volume mount for dangerous host paths.
///
/// Mirrors `SafetyChecker::check_volume_mount` from `src/safety/checker/validation.rs`.
/// Returns `Some(reason)` if the mount should be blocked.
fn check_volume_mount(mount: &str) -> Option<String> {
    let host_path = mount.split(':').next().unwrap_or("");
    let expanded = expand_home(host_path);

    // Block SSH directory mounts (including ~/.ssh and relative .ssh)
    if expanded.contains("/.ssh") || expanded == ".ssh" || expanded.starts_with(".ssh/") {
        return Some(format!("Volume mount '{}' is not allowed", mount));
    }

    // Block dangerous system mounts
    let dangerous_mounts = [
        "/", "/etc", "/boot", "/usr", "/var", "/root", "/sys", "/proc", "/lib", "/lib64", "/opt",
        "/run",
    ];
    for dm in &dangerous_mounts {
        if expanded == *dm
            || (expanded.starts_with(dm) && expanded.as_bytes().get(dm.len()) == Some(&b'/'))
        {
            return Some(format!("Volume mount '{}' is not allowed", mount));
        }
    }

    None
}

/// Pre-compiled regexes for destructive command detection.
///
/// Each pattern uses word-boundary matching and flexible whitespace to prevent
/// bypass via extra spacing, backslash insertion, or other trivial variations.
static DESTRUCTIVE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    let patterns = [
        "rm -rf",
        "rm -r",
        "rmdir",
        "git push -f",
        "git push --force",
        "git reset --hard",
        "git clean -f",
        "DROP TABLE",
        "DROP DATABASE",
        "DELETE FROM",
        "TRUNCATE",
        "> /dev/",
        "dd if=",
    ];
    patterns
        .iter()
        .filter_map(|p| {
            let re_pattern = build_boundary_pattern(p);
            Regex::new(&re_pattern).ok()
        })
        .collect()
});

/// Check if a shell command is destructive
fn is_destructive_command(cmd: &str) -> bool {
    let normalized = normalize_input(cmd);
    DESTRUCTIVE_PATTERNS
        .iter()
        .any(|re| re.is_match(&normalized))
}

/// Best-effort detection of a shell command that READS a sensitive path
/// (SSH/private keys, cloud/git credentials, .env). Defense-in-depth only — a
/// determined command can evade this; see docs/limitations.md. Returns the
/// matched sensitive token so the confirmation reason can name it.
fn reads_sensitive_path(cmd: &str) -> Option<&'static str> {
    let lower = cmd.to_lowercase();
    // High-signal sensitive path tokens.
    const SENSITIVE: &[&str] = &[
        ".ssh/",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        ".aws/credentials",
        ".netrc",
        ".git-credentials",
        "private_key",
        ".pem",
        "/secrets/",
        ".env",
    ];
    // Commands that read file contents (as opposed to merely listing).
    const READERS: &[&str] = &[
        "cat", "less", "more", "head", "tail", "bat", "nl", "tac", "xxd", "od", "strings",
        "base64", "grep", "awk", "sed", "cp", "rsync", "scp", "curl", "dd",
    ];
    let secret = SENSITIVE.iter().copied().find(|s| lower.contains(s))?;
    if READERS.iter().any(|r| lower.contains(r)) {
        Some(secret)
    } else {
        None
    }
}

/// Shell reader commands that consume file *contents* (as opposed to merely
/// listing). Shared by the sensitive-path and denied-glob heuristics.
const SHELL_READERS: &[&str] = &[
    "cat", "less", "more", "head", "tail", "bat", "nl", "tac", "xxd", "od", "strings", "base64",
    "grep", "awk", "sed", "cp", "rsync", "scp", "curl", "dd",
];

/// Split a shell command into candidate path tokens: whitespace and common
/// shell metacharacters/quotes are separators, and a leading `./` is stripped
/// so `./secret.env` matches a `*.env` glob.
fn shell_path_tokens(cmd: &str) -> Vec<&str> {
    cmd.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '"' | '\'' | '=' | '|' | ';' | '&' | '(' | ')' | '<' | '>' | '`' | ','
            )
    })
    .map(|t| t.trim_start_matches("./"))
    .filter(|t| !t.is_empty())
    .collect()
}

/// Check whether a shell command *reads* a path matching one of the operator's
/// configured `denied_paths` globs. This extends the same deny-globs that guard
/// file operations to `shell_exec` command strings, closing the gap where a
/// reader like `cat`/`grep` could exfiltrate a denied path via the shell.
///
/// Defense-in-depth only (like [`reads_sensitive_path`]) — it flags reads, not
/// listings, and a determined command can still evade token-level matching.
/// Returns the matched glob pattern so the confirmation reason can name it.
fn reads_denied_path(cmd: &str, denied_paths: &[String]) -> Option<String> {
    if denied_paths.is_empty() {
        return None;
    }
    let lower = cmd.to_lowercase();
    if !SHELL_READERS.iter().any(|r| lower.contains(r)) {
        return None;
    }
    let tokens = shell_path_tokens(cmd);
    for pattern in denied_paths {
        let compiled = match glob::Pattern::new(pattern) {
            Ok(p) => p,
            Err(_) => continue,
        };
        // A filename-only pattern (no '/', e.g. ".env") should match by the
        // token's basename; a path pattern (e.g. "**/.ssh/**") matches the
        // whole token.
        let filename_only = !pattern.contains('/');
        for tok in &tokens {
            if compiled.matches(tok) {
                return Some(pattern.clone());
            }
            if filename_only {
                if let Some(base) = std::path::Path::new(tok)
                    .file_name()
                    .and_then(|s| s.to_str())
                {
                    if compiled.matches(base) {
                        return Some(pattern.clone());
                    }
                }
            }
        }
    }
    None
}

/// Summarize arguments for audit log (truncate long values)
fn summarize_args(args: &serde_json::Value) -> String {
    let mut summary = serde_json::Map::new();

    if let Some(obj) = args.as_object() {
        for (key, value) in obj {
            let summarized = match value {
                serde_json::Value::String(s) if s.len() > 100 => {
                    serde_json::Value::String(format!(
                        "{}... ({} chars)",
                        s.chars().take(100).collect::<String>(),
                        s.len()
                    ))
                }
                other => other.clone(),
            };
            summary.insert(key.clone(), summarized);
        }
    }

    serde_json::to_string(&summary).unwrap_or_else(|_| "{}".to_string())
}

/// Append an audit entry to file.
///
/// Thread-safety: the caller serialises access through AUDIT_FILE_LOCK.
fn append_to_audit_file(path: &PathBuf, entry: &AuditEntry) -> std::io::Result<()> {
    let json = serde_json::to_string(entry).unwrap_or_default();
    let line = format!("{}\n", json);

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(line.as_bytes())?;
    file.flush()
}

/// Global mutex protecting audit file writes from concurrent threads.
static AUDIT_FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
#[path = "../../tests/unit/safety/yolo/yolo_test.rs"]
mod tests;
