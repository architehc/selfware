//! YOLO Mode - Fully autonomous operation without confirmations
//!
//! Enables the agent to run for extended periods (hours/days) without
//! requiring user intervention. All confirmations are auto-approved
//! with comprehensive audit logging.

// Feature-gated module

use chrono::{DateTime, Utc};
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
    pub fn is_forbidden(&self, operation: &str) -> bool {
        let op_lower = operation.to_lowercase();
        self.forbidden_operations
            .iter()
            .any(|f| op_lower.contains(&f.to_lowercase()))
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

        // Check git push
        if tool_name == "git_push" && !self.config.allow_git_push {
            return YoloDecision::RequireConfirmation("Git push requires confirmation".to_string());
        }

        // Check destructive shell commands
        if tool_name == "shell_exec" {
            if let Some(cmd) = args.get("command").and_then(|c| c.as_str()) {
                if is_destructive_command(cmd) && !self.config.allow_destructive_shell {
                    return YoloDecision::RequireConfirmation(
                        "Destructive shell command requires confirmation".to_string(),
                    );
                }
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

/// Extract path from tool arguments
fn extract_path(args: &serde_json::Value) -> Option<String> {
    args.get("path")
        .or_else(|| args.get("file"))
        .or_else(|| args.get("directory"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Check if a shell command is destructive
fn is_destructive_command(cmd: &str) -> bool {
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
    let cmd_lower = cmd.to_lowercase();
    patterns
        .iter()
        .any(|p| cmd_lower.contains(&p.to_lowercase()))
}

/// Summarize arguments for audit log (truncate long values)
fn summarize_args(args: &serde_json::Value) -> String {
    let mut summary = serde_json::Map::new();

    if let Some(obj) = args.as_object() {
        for (key, value) in obj {
            let summarized = match value {
                serde_json::Value::String(s) if s.len() > 100 => {
                    serde_json::Value::String(format!("{}... ({} chars)", s.chars().take(100).collect::<String>(), s.len()))
                }
                other => other.clone(),
            };
            summary.insert(key.clone(), summarized);
        }
    }

    serde_json::to_string(&summary).unwrap_or_else(|_| "{}".to_string())
}

/// Append an audit entry to file
fn append_to_audit_file(path: &PathBuf, entry: &AuditEntry) -> std::io::Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;

    let json = serde_json::to_string(entry).unwrap_or_default();
    writeln!(file, "{}", json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yolo_config_default() {
        let config = YoloConfig::default();
        assert!(!config.enabled);
        assert!(config.allow_git_push);
    }

    #[test]
    fn test_yolo_config_for_coding() {
        let config = YoloConfig::for_coding();
        assert!(config.enabled);
        assert!(!config.allow_git_push); // Safer default
    }

    #[test]
    fn test_is_forbidden() {
        let config = YoloConfig::default();
        assert!(config.is_forbidden("rm -rf /"));
        assert!(config.is_forbidden("sudo rm -rf /"));
        assert!(!config.is_forbidden("rm file.txt"));
    }

    #[test]
    fn test_is_protected_path() {
        let config = YoloConfig::default();
        assert!(config.is_protected_path("/etc/passwd"));
        assert!(config.is_protected_path("/usr/bin/bash"));
        assert!(!config.is_protected_path("/home/user/project"));
    }

    #[test]
    fn test_yolo_manager_inactive_by_default() {
        let config = YoloConfig::default();
        let manager = YoloManager::new(config);
        assert!(!manager.is_active());
    }

    #[test]
    fn test_yolo_manager_enable_disable() {
        let config = YoloConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = YoloManager::new(config);

        assert!(manager.is_active());
        manager.disable();
        assert!(!manager.is_active());
        manager.enable();
        assert!(manager.is_active());
    }

    #[test]
    fn test_auto_approve_when_active() {
        let config = YoloConfig::fully_autonomous();
        let manager = YoloManager::new(config);

        let args = serde_json::json!({"path": "/home/user/test.txt"});
        let decision = manager.should_auto_approve("file_read", &args);

        assert_eq!(decision, YoloDecision::AutoApprove);
    }

    #[test]
    fn test_block_forbidden_operation() {
        let config = YoloConfig::fully_autonomous();
        let manager = YoloManager::new(config);

        let args = serde_json::json!({"command": "rm -rf /"});
        let decision = manager.should_auto_approve("shell_exec", &args);

        assert!(matches!(decision, YoloDecision::Block(_)));
    }

    #[test]
    fn test_block_protected_path() {
        let config = YoloConfig::fully_autonomous();
        let manager = YoloManager::new(config);

        let args = serde_json::json!({"path": "/etc/passwd"});
        let decision = manager.should_auto_approve("file_write", &args);

        assert!(matches!(decision, YoloDecision::Block(_)));
    }

    #[test]
    fn test_require_confirmation_git_push() {
        let config = YoloConfig::for_coding(); // git push disabled
        let manager = YoloManager::new(config);

        let args = serde_json::json!({"branch": "main"});
        let decision = manager.should_auto_approve("git_push", &args);

        assert!(matches!(decision, YoloDecision::RequireConfirmation(_)));
    }

    #[test]
    fn test_operation_counting() {
        let config = YoloConfig::fully_autonomous();
        let manager = YoloManager::new(config);

        assert_eq!(manager.operation_count(), 0);

        manager.record_operation(
            "file_read",
            &serde_json::json!({"path": "test.txt"}),
            true,
            AuditResult::Success,
            100,
        );

        assert_eq!(manager.operation_count(), 1);
    }

    #[test]
    fn test_max_operations_limit() {
        let mut config = YoloConfig::fully_autonomous();
        config.max_operations = 2;
        let manager = YoloManager::new(config);

        assert!(manager.is_active());

        manager.record_operation("t1", &serde_json::json!({}), true, AuditResult::Success, 0);
        assert!(manager.is_active());

        manager.record_operation("t2", &serde_json::json!({}), true, AuditResult::Success, 0);
        assert!(!manager.is_active()); // Limit reached
    }

    #[test]
    fn test_audit_summary() {
        let config = YoloConfig::fully_autonomous();
        let manager = YoloManager::new(config);

        manager.record_operation(
            "file_read",
            &serde_json::json!({}),
            true,
            AuditResult::Success,
            50,
        );
        manager.record_operation(
            "file_write",
            &serde_json::json!({}),
            true,
            AuditResult::Success,
            100,
        );
        manager.record_operation(
            "shell_exec",
            &serde_json::json!({}),
            true,
            AuditResult::Failed("error".to_string()),
            200,
        );

        let summary = manager.audit_summary();

        assert_eq!(summary.total_operations, 3);
        assert_eq!(summary.success, 2);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.total_duration_ms, 350);
    }

    #[test]
    fn test_is_destructive_command() {
        assert!(is_destructive_command("rm -rf /tmp/test"));
        assert!(is_destructive_command("git push --force"));
        assert!(is_destructive_command("DROP TABLE users"));
        assert!(!is_destructive_command("ls -la"));
        assert!(!is_destructive_command("cargo test"));
    }

    #[test]
    fn test_summarize_args_truncates() {
        let long_content = "x".repeat(200);
        let args = serde_json::json!({"content": long_content});
        let summary = summarize_args(&args);

        assert!(summary.len() < 250);
        assert!(summary.contains("200 chars"));
    }

    #[test]
    fn test_expand_home() {
        // This test depends on HOME being set
        if std::env::var("HOME").is_ok() {
            let expanded = expand_home("~/test");
            assert!(!expanded.starts_with("~"));
            assert!(expanded.ends_with("/test"));
        }
    }

    #[test]
    fn test_yolo_config_default_values() {
        let config = YoloConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_operations, 0);
        assert!((config.max_hours - 0.0).abs() < f64::EPSILON);
        assert!(config.allow_git_push);
        assert!(!config.allow_destructive_shell);
        assert!(config.audit_log_path.is_none());
        assert_eq!(config.status_interval, 100);
    }

    #[test]
    fn test_yolo_config_for_coding_values() {
        let config = YoloConfig::for_coding();
        assert!(config.enabled);
        assert!(!config.allow_git_push);
        assert!(!config.allow_destructive_shell);
        assert_eq!(config.status_interval, 50);
    }

    #[test]
    fn test_yolo_config_fully_autonomous() {
        let config = YoloConfig::fully_autonomous();
        assert!(config.enabled);
        assert!(config.allow_git_push);
        assert!(!config.allow_destructive_shell);
    }

    #[test]
    fn test_yolo_config_with_destructive_shell() {
        let config = YoloConfig::for_coding().with_destructive_shell(true);
        assert!(config.allow_destructive_shell);

        let config2 = YoloConfig::for_coding().with_destructive_shell(false);
        assert!(!config2.allow_destructive_shell);
    }

    #[test]
    fn test_yolo_config_with_git_push() {
        let config = YoloConfig::for_coding().with_git_push(true);
        assert!(config.allow_git_push);

        let config2 = YoloConfig::fully_autonomous().with_git_push(false);
        assert!(!config2.allow_git_push);
    }

    #[test]
    fn test_is_forbidden_case_insensitive() {
        let config = YoloConfig::default();
        assert!(config.is_forbidden("RM -RF /"));
        assert!(config.is_forbidden("DD IF=/DEV/ZERO"));
        assert!(!config.is_forbidden("ls -la"));
    }

    #[test]
    fn test_yolo_decision_eq() {
        assert_eq!(YoloDecision::AutoApprove, YoloDecision::AutoApprove);
        assert_ne!(
            YoloDecision::AutoApprove,
            YoloDecision::Block("x".to_string())
        );
    }

    #[test]
    fn test_yolo_decision_debug() {
        let decision = YoloDecision::RequireConfirmation("test".to_string());
        let debug_str = format!("{:?}", decision);
        assert!(debug_str.contains("RequireConfirmation"));
    }

    #[test]
    fn test_audit_result_variants() {
        let success = AuditResult::Success;
        let failed = AuditResult::Failed("error".to_string());
        let blocked = AuditResult::Blocked("protected".to_string());

        let _ = format!("{:?}", success);
        let _ = format!("{:?}", failed);
        let _ = format!("{:?}", blocked);
    }

    #[test]
    fn test_audit_entry_clone() {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            operation_id: 1,
            tool_name: "test".to_string(),
            arguments_summary: "args".to_string(),
            auto_approved: true,
            result: AuditResult::Success,
            duration_ms: 100,
        };

        let cloned = entry.clone();
        assert_eq!(entry.operation_id, cloned.operation_id);
        assert_eq!(entry.tool_name, cloned.tool_name);
    }

    #[test]
    fn test_audit_entry_serde() {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            operation_id: 1,
            tool_name: "file_read".to_string(),
            arguments_summary: "path: test.txt".to_string(),
            auto_approved: true,
            result: AuditResult::Success,
            duration_ms: 50,
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("file_read"));
        assert!(json.contains("operation_id"));

        let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tool_name, entry.tool_name);
    }

    #[test]
    fn test_yolo_config_clone() {
        let config = YoloConfig::fully_autonomous();
        let cloned = config.clone();
        assert_eq!(config.enabled, cloned.enabled);
        assert_eq!(config.allow_git_push, cloned.allow_git_push);
    }

    #[test]
    fn test_yolo_config_serde() {
        let config = YoloConfig::for_coding();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("enabled"));

        let parsed: YoloConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enabled, config.enabled);
    }

    #[test]
    fn test_audit_summary_fields() {
        let summary = AuditSummary {
            total_operations: 10,
            success: 8,
            failed: 1,
            blocked: 1,
            tools_used: std::collections::HashMap::new(),
            total_duration_ms: 5000,
            elapsed_hours: 1.5,
        };

        let debug_str = format!("{:?}", summary);
        assert!(debug_str.contains("total_operations"));
    }

    #[test]
    fn test_require_confirmation_destructive_shell() {
        let config = YoloConfig::fully_autonomous().with_destructive_shell(false);
        let manager = YoloManager::new(config);

        let args = serde_json::json!({"command": "rm -rf ./test"});
        let decision = manager.should_auto_approve("shell_exec", &args);

        assert!(matches!(decision, YoloDecision::RequireConfirmation(_)));
    }

    #[test]
    fn test_allow_destructive_shell_when_enabled() {
        let config = YoloConfig::fully_autonomous().with_destructive_shell(true);
        let manager = YoloManager::new(config);

        // Safe destructive command (not in forbidden list)
        let args = serde_json::json!({"command": "rm -rf ./test_dir"});
        let decision = manager.should_auto_approve("shell_exec", &args);

        // Should auto-approve since destructive shell is enabled
        // and it's not in the forbidden list
        assert_eq!(decision, YoloDecision::AutoApprove);
    }

    #[test]
    fn test_yolo_manager_with_audit_log_path() {
        let config = YoloConfig {
            enabled: true,
            audit_log_path: Some(PathBuf::from("/tmp/test_audit.log")),
            ..Default::default()
        };
        let manager = YoloManager::new(config);
        assert!(manager.is_active());
    }

    #[test]
    fn test_protected_paths_include_ssh() {
        let config = YoloConfig::default();
        // SSH directory should be protected
        if std::env::var("HOME").is_ok() {
            let expanded = expand_home("~/.ssh/id_rsa");
            assert!(
                config.is_protected_path(&expanded) || config.is_protected_path("~/.ssh/id_rsa")
            );
        }
    }

    #[test]
    fn test_expand_home_no_tilde() {
        let path = "/absolute/path";
        let expanded = expand_home(path);
        assert_eq!(expanded, path);
    }
}
