//! Security Sandbox
//!
//! Defense in depth:
//! - Filesystem sandboxing
//! - Network firewall rules
//! - Resource limits
//! - Audit logging
//! - Autonomy levels

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Autonomy level for agent operations
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum AutonomyLevel {
    /// Agent can only suggest, human must approve and execute
    SuggestOnly,
    /// Agent can execute safe operations, must confirm destructive ones
    #[default]
    ConfirmDestructive,
    /// Agent can execute all operations except explicitly forbidden ones
    SemiAutonomous,
    /// Agent has full control (use with caution)
    FullAutonomous,
}

impl AutonomyLevel {
    /// Icon for display
    pub fn icon(&self) -> &'static str {
        match self {
            AutonomyLevel::SuggestOnly => "🔒",
            AutonomyLevel::ConfirmDestructive => "⚠️",
            AutonomyLevel::SemiAutonomous => "🔓",
            AutonomyLevel::FullAutonomous => "🔥",
        }
    }

    /// Description
    pub fn description(&self) -> &'static str {
        match self {
            AutonomyLevel::SuggestOnly => "Agent can only suggest actions, human executes",
            AutonomyLevel::ConfirmDestructive => {
                "Agent executes safe ops, confirms destructive ones"
            }
            AutonomyLevel::SemiAutonomous => "Agent executes most ops, respects explicit denials",
            AutonomyLevel::FullAutonomous => "Agent has full control (dangerous)",
        }
    }

    /// Is this a restricted level?
    pub fn is_restricted(&self) -> bool {
        matches!(
            self,
            AutonomyLevel::SuggestOnly | AutonomyLevel::ConfirmDestructive
        )
    }

    /// Can auto-execute safe operations?
    pub fn can_auto_execute_safe(&self) -> bool {
        !matches!(self, AutonomyLevel::SuggestOnly)
    }

    /// Can auto-execute destructive operations?
    pub fn can_auto_execute_destructive(&self) -> bool {
        matches!(
            self,
            AutonomyLevel::SemiAutonomous | AutonomyLevel::FullAutonomous
        )
    }

    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "suggest" | "suggest_only" | "suggestonly" => Some(AutonomyLevel::SuggestOnly),
            "confirm" | "confirm_destructive" | "confirmdestructive" => {
                Some(AutonomyLevel::ConfirmDestructive)
            }
            "semi" | "semi_autonomous" | "semiautonomous" => Some(AutonomyLevel::SemiAutonomous),
            "full" | "full_autonomous" | "fullautonomous" => Some(AutonomyLevel::FullAutonomous),
            _ => None,
        }
    }
}

impl std::fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            AutonomyLevel::SuggestOnly => "SuggestOnly",
            AutonomyLevel::ConfirmDestructive => "ConfirmDestructive",
            AutonomyLevel::SemiAutonomous => "SemiAutonomous",
            AutonomyLevel::FullAutonomous => "FullAutonomous",
        };
        write!(f, "{}", name)
    }
}

/// Operation risk level
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum RiskLevel {
    /// Safe operation (read, list)
    #[default]
    Safe,
    /// Low risk (write to allowed paths)
    Low,
    /// Medium risk (modify existing files)
    Medium,
    /// High risk (delete files, system changes)
    High,
    /// Critical risk (system destruction potential)
    Critical,
}

impl RiskLevel {
    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "✓",
            RiskLevel::Low => "⚡",
            RiskLevel::Medium => "⚠️",
            RiskLevel::High => "🔥",
            RiskLevel::Critical => "💀",
        }
    }

    /// Is destructive?
    pub fn is_destructive(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Critical)
    }

    /// Color code
    pub fn color(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "\x1b[32m",
            RiskLevel::Low => "\x1b[33m",
            RiskLevel::Medium => "\x1b[33m",
            RiskLevel::High => "\x1b[31m",
            RiskLevel::Critical => "\x1b[91m",
        }
    }
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            RiskLevel::Safe => "Safe",
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
            RiskLevel::Critical => "Critical",
        };
        write!(f, "{}", name)
    }
}

/// File access type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileAccess {
    Read,
    Write,
    Create,
    Delete,
    Execute,
    List,
}

impl FileAccess {
    /// Risk level for this access type
    pub fn risk_level(&self) -> RiskLevel {
        match self {
            FileAccess::Read | FileAccess::List => RiskLevel::Safe,
            FileAccess::Create => RiskLevel::Low,
            FileAccess::Write => RiskLevel::Medium,
            FileAccess::Delete => RiskLevel::High,
            FileAccess::Execute => RiskLevel::High,
        }
    }
}

impl std::fmt::Display for FileAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            FileAccess::Read => "read",
            FileAccess::Write => "write",
            FileAccess::Create => "create",
            FileAccess::Delete => "delete",
            FileAccess::Execute => "execute",
            FileAccess::List => "list",
        };
        write!(f, "{}", name)
    }
}

/// Filesystem sandbox policy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilesystemPolicy {
    /// Allowed paths (whitelist)
    pub allowed_paths: Vec<PathBuf>,
    /// Denied paths (blacklist, takes precedence)
    pub denied_paths: Vec<PathBuf>,
    /// Allowed extensions
    pub allowed_extensions: Option<HashSet<String>>,
    /// Denied extensions
    pub denied_extensions: HashSet<String>,
    /// Max file size for write (bytes)
    pub max_write_size: Option<u64>,
    /// Allow symlinks
    pub allow_symlinks: bool,
    /// Allow hidden files (starting with .)
    pub allow_hidden: bool,
}

impl FilesystemPolicy {
    /// Create new policy
    pub fn new() -> Self {
        Self {
            allow_symlinks: false,
            allow_hidden: true,
            ..Default::default()
        }
    }

    /// Allow a path
    pub fn allow_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.allowed_paths.push(path.into());
        self
    }

    /// Deny a path
    pub fn deny_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.denied_paths.push(path.into());
        self
    }

    /// Deny an extension
    pub fn deny_extension(mut self, ext: &str) -> Self {
        self.denied_extensions.insert(ext.to_string());
        self
    }

    /// Set max write size
    pub fn max_size(mut self, size: u64) -> Self {
        self.max_write_size = Some(size);
        self
    }

    /// Check if path is allowed
    pub fn is_allowed(&self, path: &Path, access: FileAccess) -> Result<()> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Check denied paths first (blacklist takes precedence)
        for denied in &self.denied_paths {
            if path.starts_with(denied) {
                return Err(anyhow!("Path is in denied list: {}", path.display()));
            }
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if self.denied_extensions.contains(ext) {
                return Err(anyhow!("Extension .{} is denied", ext));
            }
            if let Some(allowed) = &self.allowed_extensions {
                if !allowed.contains(ext) {
                    return Err(anyhow!("Extension .{} is not in allowed list", ext));
                }
            }
        }

        // Check hidden files
        if !self.allow_hidden {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    return Err(anyhow!("Hidden files are not allowed"));
                }
            }
        }

        // Check allowed paths (whitelist)
        if !self.allowed_paths.is_empty() {
            let in_allowed = self
                .allowed_paths
                .iter()
                .any(|allowed| path.starts_with(allowed));
            if !in_allowed {
                return Err(anyhow!("Path is not in allowed list: {}", path.display()));
            }
        }

        // For write/create, check if safe based on access type
        if matches!(
            access,
            FileAccess::Write | FileAccess::Create | FileAccess::Delete
        ) {
            // Additional checks could go here
        }

        Ok(())
    }

    /// Check if symlinks are allowed
    pub fn check_symlink(&self, path: &Path) -> Result<()> {
        if !self.allow_symlinks && path.is_symlink() {
            return Err(anyhow!("Symlinks are not allowed"));
        }
        Ok(())
    }

    /// Check write size
    pub fn check_size(&self, size: u64) -> Result<()> {
        if let Some(max) = self.max_write_size {
            if size > max {
                return Err(anyhow!("Write size {} exceeds limit {}", size, max));
            }
        }
        Ok(())
    }
}

/// Network access type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NetworkAccess {
    Connect,
    Listen,
    Dns,
}

impl std::fmt::Display for NetworkAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            NetworkAccess::Connect => "connect",
            NetworkAccess::Listen => "listen",
            NetworkAccess::Dns => "dns",
        };
        write!(f, "{}", name)
    }
}

/// Network rule action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RuleAction {
    Allow,
    #[default]
    Deny,
    Log,
}

/// Network firewall rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRule {
    /// Rule name
    pub name: String,
    /// Action
    pub action: RuleAction,
    /// Target (host pattern)
    pub host: Option<String>,
    /// Port or port range
    pub port: Option<PortSpec>,
    /// Protocol
    pub protocol: Option<String>,
    /// Access type
    pub access: Option<NetworkAccess>,
}

/// Port specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PortSpec {
    Single(u16),
    Range(u16, u16),
    List(Vec<u16>),
}

impl PortSpec {
    /// Check if port matches
    pub fn matches(&self, port: u16) -> bool {
        match self {
            PortSpec::Single(p) => *p == port,
            PortSpec::Range(start, end) => port >= *start && port <= *end,
            PortSpec::List(ports) => ports.contains(&port),
        }
    }
}

impl NetworkRule {
    /// Create new rule
    pub fn new(name: &str, action: RuleAction) -> Self {
        Self {
            name: name.to_string(),
            action,
            host: None,
            port: None,
            protocol: None,
            access: None,
        }
    }

    /// Set host pattern
    pub fn host(mut self, host: &str) -> Self {
        self.host = Some(host.to_string());
        self
    }

    /// Set port
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(PortSpec::Single(port));
        self
    }

    /// Set port range
    pub fn port_range(mut self, start: u16, end: u16) -> Self {
        self.port = Some(PortSpec::Range(start, end));
        self
    }

    /// Set access type
    pub fn access(mut self, access: NetworkAccess) -> Self {
        self.access = Some(access);
        self
    }

    /// Check if rule matches
    pub fn matches(&self, host: &str, port: u16, access: NetworkAccess) -> bool {
        // Check host pattern
        if let Some(pattern) = &self.host {
            if !Self::host_matches(pattern, host) {
                return false;
            }
        }

        // Check port
        if let Some(port_spec) = &self.port {
            if !port_spec.matches(port) {
                return false;
            }
        }

        // Check access type
        if let Some(acc) = &self.access {
            if *acc != access {
                return false;
            }
        }

        true
    }

    /// Check if host matches pattern
    fn host_matches(pattern: &str, host: &str) -> bool {
        if pattern == "*" {
            return true;
        }
        if pattern.starts_with("*.") {
            // Wildcard subdomain
            let suffix = &pattern[1..];
            return host.ends_with(suffix);
        }
        pattern == host
    }
}

/// Network policy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Rules (evaluated in order)
    pub rules: Vec<NetworkRule>,
    /// Default action
    pub default_action: RuleAction,
    /// Allow localhost
    pub allow_localhost: bool,
}

impl NetworkPolicy {
    /// Create new policy
    pub fn new() -> Self {
        Self {
            default_action: RuleAction::Deny,
            allow_localhost: true,
            ..Default::default()
        }
    }

    /// Add rule
    pub fn add_rule(mut self, rule: NetworkRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// Check access
    pub fn check(&self, host: &str, port: u16, access: NetworkAccess) -> RuleAction {
        // Localhost exception
        if self.allow_localhost && (host == "localhost" || host == "127.0.0.1" || host == "::1") {
            return RuleAction::Allow;
        }

        // Check rules in order
        for rule in &self.rules {
            if rule.matches(host, port, access) {
                return rule.action;
            }
        }

        self.default_action
    }

    /// Is allowed
    pub fn is_allowed(&self, host: &str, port: u16, access: NetworkAccess) -> bool {
        matches!(self.check(host, port, access), RuleAction::Allow)
    }
}

/// Resource limits
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Max CPU time (seconds)
    pub max_cpu_time: Option<u64>,
    /// Max memory (bytes)
    pub max_memory: Option<u64>,
    /// Max file descriptors
    pub max_fds: Option<u32>,
    /// Max processes
    pub max_processes: Option<u32>,
    /// Max output size (bytes)
    pub max_output_size: Option<u64>,
    /// Execution timeout
    pub timeout: Option<Duration>,
}

impl ResourceLimits {
    /// Create new limits
    pub fn new() -> Self {
        Self::default()
    }

    /// Set CPU time limit
    pub fn cpu_time(mut self, seconds: u64) -> Self {
        self.max_cpu_time = Some(seconds);
        self
    }

    /// Set memory limit
    pub fn memory(mut self, bytes: u64) -> Self {
        self.max_memory = Some(bytes);
        self
    }

    /// Set memory limit in MB
    pub fn memory_mb(self, mb: u64) -> Self {
        self.memory(mb * 1024 * 1024)
    }

    /// Set timeout
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Set timeout in seconds
    pub fn timeout_secs(self, seconds: u64) -> Self {
        self.timeout(Duration::from_secs(seconds))
    }

    /// Set max processes
    pub fn max_procs(mut self, count: u32) -> Self {
        self.max_processes = Some(count);
        self
    }

    /// Check if memory is within limits
    pub fn check_memory(&self, bytes: u64) -> Result<()> {
        if let Some(max) = self.max_memory {
            if bytes > max {
                return Err(anyhow!("Memory usage {} exceeds limit {}", bytes, max));
            }
        }
        Ok(())
    }

    /// Check if output size is within limits
    pub fn check_output(&self, bytes: u64) -> Result<()> {
        if let Some(max) = self.max_output_size {
            if bytes > max {
                return Err(anyhow!("Output size {} exceeds limit {}", bytes, max));
            }
        }
        Ok(())
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Action type
    pub action: String,
    /// Subject (who)
    pub subject: String,
    /// Object (what)
    pub object: String,
    /// Result
    pub result: AuditResult,
    /// Details
    pub details: Option<String>,
    /// Risk level
    pub risk: RiskLevel,
}

/// Audit result
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditResult {
    Allowed,
    Denied,
    Prompted,
    Failed,
}

impl AuditResult {
    /// Icon
    pub fn icon(&self) -> &'static str {
        match self {
            AuditResult::Allowed => "✓",
            AuditResult::Denied => "✗",
            AuditResult::Prompted => "?",
            AuditResult::Failed => "!",
        }
    }
}

impl std::fmt::Display for AuditResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            AuditResult::Allowed => "Allowed",
            AuditResult::Denied => "Denied",
            AuditResult::Prompted => "Prompted",
            AuditResult::Failed => "Failed",
        };
        write!(f, "{}", name)
    }
}

impl AuditEntry {
    /// Create new entry
    pub fn new(action: &str, subject: &str, object: &str, result: AuditResult) -> Self {
        Self {
            timestamp: Utc::now(),
            action: action.to_string(),
            subject: subject.to_string(),
            object: object.to_string(),
            result,
            details: None,
            risk: RiskLevel::Safe,
        }
    }

    /// With details
    pub fn with_details(mut self, details: &str) -> Self {
        self.details = Some(details.to_string());
        self
    }

    /// With risk level
    pub fn with_risk(mut self, risk: RiskLevel) -> Self {
        self.risk = risk;
        self
    }

    /// Format for display
    pub fn display(&self) -> String {
        format!(
            "[{}] {} {} {} on {} - {}",
            self.timestamp.format("%Y-%m-%d %H:%M:%S"),
            self.risk.icon(),
            self.subject,
            self.action,
            self.object,
            self.result
        )
    }
}

/// Audit logger
#[derive(Debug, Default)]
pub struct AuditLogger {
    /// Log entries
    entries: Vec<AuditEntry>,
    /// Max entries to keep
    max_entries: usize,
    /// Log to file
    log_file: Option<PathBuf>,
    /// Log level (minimum risk to log)
    min_risk: RiskLevel,
}

impl AuditLogger {
    /// Create new logger
    pub fn new() -> Self {
        Self {
            max_entries: 10000,
            min_risk: RiskLevel::Safe,
            ..Default::default()
        }
    }

    /// Set log file
    pub fn with_file(mut self, path: PathBuf) -> Self {
        self.log_file = Some(path);
        self
    }

    /// Set minimum risk level to log
    pub fn with_min_risk(mut self, risk: RiskLevel) -> Self {
        self.min_risk = risk;
        self
    }

    /// Log an entry
    pub fn log(&mut self, entry: AuditEntry) {
        if entry.risk >= self.min_risk {
            self.entries.push(entry);

            // Limit size
            if self.entries.len() > self.max_entries {
                self.entries.remove(0);
            }
        }
    }

    /// Log a simple action
    pub fn log_action(
        &mut self,
        action: &str,
        subject: &str,
        object: &str,
        result: AuditResult,
        risk: RiskLevel,
    ) {
        self.log(AuditEntry::new(action, subject, object, result).with_risk(risk));
    }

    /// Get recent entries
    pub fn recent(&self, limit: usize) -> Vec<&AuditEntry> {
        self.entries.iter().rev().take(limit).collect()
    }

    /// Get entries by result
    pub fn by_result(&self, result: AuditResult) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| e.result == result).collect()
    }

    /// Get denied entries
    pub fn denied(&self) -> Vec<&AuditEntry> {
        self.by_result(AuditResult::Denied)
    }

    /// Get entries by risk level
    pub fn by_risk(&self, risk: RiskLevel) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| e.risk == risk).collect()
    }

    /// Count entries
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Clear entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get summary
    pub fn summary(&self) -> AuditSummary {
        AuditSummary {
            total: self.entries.len(),
            allowed: self.by_result(AuditResult::Allowed).len(),
            denied: self.by_result(AuditResult::Denied).len(),
            prompted: self.by_result(AuditResult::Prompted).len(),
            high_risk: self
                .entries
                .iter()
                .filter(|e| e.risk >= RiskLevel::High)
                .count(),
        }
    }
}

/// Audit summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditSummary {
    pub total: usize,
    pub allowed: usize,
    pub denied: usize,
    pub prompted: usize,
    pub high_risk: usize,
}

impl AuditSummary {
    /// Display
    pub fn display(&self) -> String {
        format!(
            "{} actions: {} allowed, {} denied, {} prompted ({} high-risk)",
            self.total, self.allowed, self.denied, self.prompted, self.high_risk
        )
    }
}

/// Security sandbox combining all policies
#[derive(Debug)]
pub struct SecuritySandbox {
    /// Autonomy level
    pub autonomy: AutonomyLevel,
    /// Filesystem policy
    pub filesystem: FilesystemPolicy,
    /// Network policy
    pub network: NetworkPolicy,
    /// Resource limits
    pub resources: ResourceLimits,
    /// Audit logger
    pub audit: AuditLogger,
    /// Enabled
    pub enabled: bool,
}

impl Default for SecuritySandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl SecuritySandbox {
    /// Create new sandbox with default policies
    pub fn new() -> Self {
        Self {
            autonomy: AutonomyLevel::ConfirmDestructive,
            filesystem: FilesystemPolicy::new(),
            network: NetworkPolicy::new(),
            resources: ResourceLimits::new(),
            audit: AuditLogger::new(),
            enabled: true,
        }
    }

    /// Create a strict sandbox
    pub fn strict() -> Self {
        Self {
            autonomy: AutonomyLevel::SuggestOnly,
            filesystem: FilesystemPolicy::new()
                .deny_path("/etc")
                .deny_path("/usr")
                .deny_path("/bin")
                .deny_path("/sbin")
                .deny_extension("exe")
                .deny_extension("sh"),
            network: NetworkPolicy {
                default_action: RuleAction::Deny,
                allow_localhost: true,
                rules: vec![],
            },
            resources: ResourceLimits::new()
                .memory_mb(512)
                .timeout_secs(300)
                .max_procs(10),
            audit: AuditLogger::new().with_min_risk(RiskLevel::Low),
            enabled: true,
        }
    }

    /// Create a permissive sandbox
    pub fn permissive() -> Self {
        Self {
            autonomy: AutonomyLevel::SemiAutonomous,
            filesystem: FilesystemPolicy::new()
                .deny_path("/etc/shadow")
                .deny_path("/etc/passwd"),
            network: NetworkPolicy {
                default_action: RuleAction::Allow,
                allow_localhost: true,
                rules: vec![],
            },
            resources: ResourceLimits::new(),
            audit: AuditLogger::new().with_min_risk(RiskLevel::High),
            enabled: true,
        }
    }

    /// Set autonomy level
    pub fn with_autonomy(mut self, level: AutonomyLevel) -> Self {
        self.autonomy = level;
        self
    }

    /// Enable/disable
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check file access
    pub fn check_file_access(&mut self, path: &Path, access: FileAccess) -> Result<bool> {
        if !self.enabled {
            return Ok(true);
        }

        let risk = access.risk_level();
        let result = self.filesystem.is_allowed(path, access);

        let audit_result = match &result {
            Ok(()) => {
                if risk.is_destructive() && !self.autonomy.can_auto_execute_destructive() {
                    AuditResult::Prompted
                } else {
                    AuditResult::Allowed
                }
            }
            Err(_) => AuditResult::Denied,
        };

        self.audit.log_action(
            &format!("file_{}", access),
            "agent",
            &path.display().to_string(),
            audit_result,
            risk,
        );

        match result {
            Ok(()) => {
                if risk.is_destructive() && !self.autonomy.can_auto_execute_destructive() {
                    Ok(false) // Needs confirmation
                } else {
                    Ok(true)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Check network access
    pub fn check_network_access(
        &mut self,
        host: &str,
        port: u16,
        access: NetworkAccess,
    ) -> Result<bool> {
        if !self.enabled {
            return Ok(true);
        }

        let action = self.network.check(host, port, access);
        let result = match action {
            RuleAction::Allow => AuditResult::Allowed,
            RuleAction::Deny => AuditResult::Denied,
            RuleAction::Log => AuditResult::Allowed,
        };

        self.audit.log_action(
            &format!("net_{}", access),
            "agent",
            &format!("{}:{}", host, port),
            result,
            if matches!(access, NetworkAccess::Listen) {
                RiskLevel::Medium
            } else {
                RiskLevel::Low
            },
        );

        match action {
            RuleAction::Allow | RuleAction::Log => Ok(true),
            RuleAction::Deny => Err(anyhow!("Network access denied: {}:{}", host, port)),
        }
    }

    /// Check if operation needs confirmation
    pub fn needs_confirmation(&self, risk: RiskLevel) -> bool {
        if !self.enabled {
            return false;
        }

        match self.autonomy {
            AutonomyLevel::SuggestOnly => true,
            AutonomyLevel::ConfirmDestructive => risk.is_destructive(),
            AutonomyLevel::SemiAutonomous => risk == RiskLevel::Critical,
            AutonomyLevel::FullAutonomous => false,
        }
    }

    /// Get security status
    pub fn status(&self) -> SandboxStatus {
        SandboxStatus {
            enabled: self.enabled,
            autonomy: self.autonomy,
            audit_count: self.audit.count(),
            denied_count: self.audit.denied().len(),
        }
    }
}

/// Sandbox status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStatus {
    pub enabled: bool,
    pub autonomy: AutonomyLevel,
    pub audit_count: usize,
    pub denied_count: usize,
}

impl SandboxStatus {
    /// Display
    pub fn display(&self) -> String {
        format!(
            "Sandbox: {} | Autonomy: {} | Actions: {} ({} denied)",
            if self.enabled { "ON" } else { "OFF" },
            self.autonomy,
            self.audit_count,
            self.denied_count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomy_level_default() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_autonomy_level_parse() {
        assert_eq!(
            AutonomyLevel::parse("suggest"),
            Some(AutonomyLevel::SuggestOnly)
        );
        assert_eq!(
            AutonomyLevel::parse("confirm"),
            Some(AutonomyLevel::ConfirmDestructive)
        );
        assert_eq!(
            AutonomyLevel::parse("semi"),
            Some(AutonomyLevel::SemiAutonomous)
        );
        assert_eq!(
            AutonomyLevel::parse("full"),
            Some(AutonomyLevel::FullAutonomous)
        );
        assert_eq!(AutonomyLevel::parse("invalid"), None);
    }

    #[test]
    fn test_autonomy_level_icon() {
        assert_eq!(AutonomyLevel::SuggestOnly.icon(), "🔒");
        assert_eq!(AutonomyLevel::FullAutonomous.icon(), "🔥");
    }

    #[test]
    fn test_autonomy_level_permissions() {
        assert!(!AutonomyLevel::SuggestOnly.can_auto_execute_safe());
        assert!(AutonomyLevel::ConfirmDestructive.can_auto_execute_safe());
        assert!(!AutonomyLevel::ConfirmDestructive.can_auto_execute_destructive());
        assert!(AutonomyLevel::FullAutonomous.can_auto_execute_destructive());
    }

    #[test]
    fn test_autonomy_level_display() {
        assert_eq!(format!("{}", AutonomyLevel::SuggestOnly), "SuggestOnly");
    }

    #[test]
    fn test_risk_level_is_destructive() {
        assert!(!RiskLevel::Safe.is_destructive());
        assert!(!RiskLevel::Low.is_destructive());
        assert!(!RiskLevel::Medium.is_destructive());
        assert!(RiskLevel::High.is_destructive());
        assert!(RiskLevel::Critical.is_destructive());
    }

    #[test]
    fn test_risk_level_icon() {
        assert_eq!(RiskLevel::Safe.icon(), "✓");
        assert_eq!(RiskLevel::Critical.icon(), "💀");
    }

    #[test]
    fn test_file_access_risk() {
        assert_eq!(FileAccess::Read.risk_level(), RiskLevel::Safe);
        assert_eq!(FileAccess::Create.risk_level(), RiskLevel::Low);
        assert_eq!(FileAccess::Write.risk_level(), RiskLevel::Medium);
        assert_eq!(FileAccess::Delete.risk_level(), RiskLevel::High);
    }

    #[test]
    fn test_filesystem_policy_new() {
        let policy = FilesystemPolicy::new();
        assert!(!policy.allow_symlinks);
        assert!(policy.allow_hidden);
    }

    #[test]
    fn test_filesystem_policy_builder() {
        let policy = FilesystemPolicy::new()
            .allow_path("/home/user")
            .deny_path("/etc")
            .deny_extension("exe")
            .max_size(1024);

        assert_eq!(policy.allowed_paths.len(), 1);
        assert_eq!(policy.denied_paths.len(), 1);
        assert!(policy.denied_extensions.contains("exe"));
        assert_eq!(policy.max_write_size, Some(1024));
    }

    #[test]
    fn test_filesystem_policy_denied() {
        let policy = FilesystemPolicy::new().deny_path("/etc");

        let result = policy.is_allowed(Path::new("/etc/passwd"), FileAccess::Read);
        assert!(result.is_err());
    }

    #[test]
    fn test_filesystem_policy_extension_denied() {
        let policy = FilesystemPolicy::new().deny_extension("exe");

        let result = policy.is_allowed(Path::new("/tmp/virus.exe"), FileAccess::Read);
        assert!(result.is_err());
    }

    #[test]
    fn test_filesystem_policy_check_size() {
        let policy = FilesystemPolicy::new().max_size(100);
        assert!(policy.check_size(50).is_ok());
        assert!(policy.check_size(150).is_err());
    }

    #[test]
    fn test_port_spec_matches() {
        assert!(PortSpec::Single(80).matches(80));
        assert!(!PortSpec::Single(80).matches(443));

        assert!(PortSpec::Range(80, 90).matches(85));
        assert!(!PortSpec::Range(80, 90).matches(100));

        assert!(PortSpec::List(vec![80, 443]).matches(443));
        assert!(!PortSpec::List(vec![80, 443]).matches(8080));
    }

    #[test]
    fn test_network_rule_new() {
        let rule = NetworkRule::new("allow_http", RuleAction::Allow)
            .host("*.example.com")
            .port(80);

        assert_eq!(rule.name, "allow_http");
        assert!(rule.matches("api.example.com", 80, NetworkAccess::Connect));
        assert!(!rule.matches("api.example.com", 443, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_rule_host_pattern() {
        let rule = NetworkRule::new("wildcard", RuleAction::Allow).host("*.google.com");

        assert!(rule.matches("www.google.com", 443, NetworkAccess::Connect));
        assert!(!rule.matches("google.com", 443, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_policy_localhost() {
        let policy = NetworkPolicy::new();
        assert!(policy.is_allowed("localhost", 8080, NetworkAccess::Connect));
        assert!(policy.is_allowed("127.0.0.1", 8080, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_policy_default_deny() {
        let policy = NetworkPolicy::new();
        assert!(!policy.is_allowed("example.com", 80, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_policy_rules() {
        let policy = NetworkPolicy::new()
            .add_rule(NetworkRule::new("allow_http", RuleAction::Allow).port(80));

        assert!(policy.is_allowed("example.com", 80, NetworkAccess::Connect));
        assert!(!policy.is_allowed("example.com", 443, NetworkAccess::Connect));
    }

    #[test]
    fn test_resource_limits_new() {
        let limits = ResourceLimits::new();
        assert!(limits.max_memory.is_none());
        assert!(limits.timeout.is_none());
    }

    #[test]
    fn test_resource_limits_builder() {
        let limits = ResourceLimits::new()
            .memory_mb(512)
            .timeout_secs(60)
            .max_procs(10);

        assert_eq!(limits.max_memory, Some(512 * 1024 * 1024));
        assert_eq!(limits.timeout, Some(Duration::from_secs(60)));
        assert_eq!(limits.max_processes, Some(10));
    }

    #[test]
    fn test_resource_limits_check_memory() {
        let limits = ResourceLimits::new().memory_mb(1);
        assert!(limits.check_memory(500_000).is_ok());
        assert!(limits.check_memory(2_000_000).is_err());
    }

    #[test]
    fn test_audit_result_icon() {
        assert_eq!(AuditResult::Allowed.icon(), "✓");
        assert_eq!(AuditResult::Denied.icon(), "✗");
    }

    #[test]
    fn test_audit_entry_new() {
        let entry = AuditEntry::new("file_read", "agent", "/tmp/file", AuditResult::Allowed);
        assert_eq!(entry.action, "file_read");
        assert_eq!(entry.result, AuditResult::Allowed);
    }

    #[test]
    fn test_audit_entry_builder() {
        let entry = AuditEntry::new("delete", "agent", "/tmp/file", AuditResult::Denied)
            .with_details("Permission denied")
            .with_risk(RiskLevel::High);

        assert!(entry.details.is_some());
        assert_eq!(entry.risk, RiskLevel::High);
    }

    #[test]
    fn test_audit_logger_new() {
        let logger = AuditLogger::new();
        assert_eq!(logger.count(), 0);
    }

    #[test]
    fn test_audit_logger_log() {
        let mut logger = AuditLogger::new();
        logger.log(AuditEntry::new(
            "test",
            "agent",
            "object",
            AuditResult::Allowed,
        ));

        assert_eq!(logger.count(), 1);
    }

    #[test]
    fn test_audit_logger_log_action() {
        let mut logger = AuditLogger::new();
        logger.log_action(
            "file_read",
            "agent",
            "/tmp/file",
            AuditResult::Allowed,
            RiskLevel::Safe,
        );

        assert_eq!(logger.count(), 1);
    }

    #[test]
    fn test_audit_logger_denied() {
        let mut logger = AuditLogger::new();
        logger.log_action(
            "delete",
            "agent",
            "/etc/passwd",
            AuditResult::Denied,
            RiskLevel::High,
        );

        assert_eq!(logger.denied().len(), 1);
    }

    #[test]
    fn test_audit_logger_min_risk() {
        let mut logger = AuditLogger::new().with_min_risk(RiskLevel::High);
        logger.log_action(
            "read",
            "agent",
            "/tmp/file",
            AuditResult::Allowed,
            RiskLevel::Safe,
        );

        assert_eq!(logger.count(), 0); // Not logged because risk is below minimum
    }

    #[test]
    fn test_audit_logger_summary() {
        let mut logger = AuditLogger::new();
        logger.log_action("a", "agent", "x", AuditResult::Allowed, RiskLevel::Safe);
        logger.log_action("b", "agent", "y", AuditResult::Denied, RiskLevel::High);

        let summary = logger.summary();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.allowed, 1);
        assert_eq!(summary.denied, 1);
    }

    #[test]
    fn test_security_sandbox_new() {
        let sandbox = SecuritySandbox::new();
        assert!(sandbox.enabled);
        assert_eq!(sandbox.autonomy, AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_security_sandbox_strict() {
        let sandbox = SecuritySandbox::strict();
        assert_eq!(sandbox.autonomy, AutonomyLevel::SuggestOnly);
    }

    #[test]
    fn test_security_sandbox_permissive() {
        let sandbox = SecuritySandbox::permissive();
        assert_eq!(sandbox.autonomy, AutonomyLevel::SemiAutonomous);
    }

    #[test]
    fn test_security_sandbox_needs_confirmation() {
        let sandbox = SecuritySandbox::new();
        assert!(!sandbox.needs_confirmation(RiskLevel::Safe));
        assert!(sandbox.needs_confirmation(RiskLevel::High));
    }

    #[test]
    fn test_security_sandbox_disabled() {
        let mut sandbox = SecuritySandbox::new();
        sandbox.set_enabled(false);

        assert!(!sandbox.needs_confirmation(RiskLevel::Critical));
    }

    #[test]
    fn test_security_sandbox_status() {
        let sandbox = SecuritySandbox::new();
        let status = sandbox.status();

        assert!(status.enabled);
        assert_eq!(status.autonomy, AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_sandbox_status_display() {
        let status = SandboxStatus {
            enabled: true,
            autonomy: AutonomyLevel::ConfirmDestructive,
            audit_count: 100,
            denied_count: 5,
        };

        let display = status.display();
        assert!(display.contains("ON"));
        assert!(display.contains("ConfirmDestructive"));
        assert!(display.contains("100"));
    }

    #[test]
    fn test_security_sandbox_check_network() {
        let mut sandbox = SecuritySandbox::new();

        // Localhost should be allowed
        let result = sandbox.check_network_access("localhost", 8080, NetworkAccess::Connect);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // External should be denied by default
        let result = sandbox.check_network_access("example.com", 80, NetworkAccess::Connect);
        assert!(result.is_err());
    }

    // ================== Additional Coverage Tests ==================

    #[test]
    fn test_autonomy_level_description() {
        assert!(AutonomyLevel::SuggestOnly.description().contains("suggest"));
        assert!(AutonomyLevel::ConfirmDestructive
            .description()
            .contains("confirms"));
        assert!(AutonomyLevel::SemiAutonomous
            .description()
            .contains("executes"));
        assert!(AutonomyLevel::FullAutonomous.description().contains("full"));
    }

    #[test]
    fn test_autonomy_level_is_restricted() {
        assert!(AutonomyLevel::SuggestOnly.is_restricted());
        assert!(AutonomyLevel::ConfirmDestructive.is_restricted());
        assert!(!AutonomyLevel::SemiAutonomous.is_restricted());
        assert!(!AutonomyLevel::FullAutonomous.is_restricted());
    }

    #[test]
    fn test_autonomy_level_parse_all_variants() {
        // SuggestOnly variants
        assert_eq!(
            AutonomyLevel::parse("suggest_only"),
            Some(AutonomyLevel::SuggestOnly)
        );
        assert_eq!(
            AutonomyLevel::parse("SUGGESTONLY"),
            Some(AutonomyLevel::SuggestOnly)
        );

        // ConfirmDestructive variants
        assert_eq!(
            AutonomyLevel::parse("confirm_destructive"),
            Some(AutonomyLevel::ConfirmDestructive)
        );
        assert_eq!(
            AutonomyLevel::parse("confirmdestructive"),
            Some(AutonomyLevel::ConfirmDestructive)
        );

        // SemiAutonomous variants
        assert_eq!(
            AutonomyLevel::parse("semi_autonomous"),
            Some(AutonomyLevel::SemiAutonomous)
        );
        assert_eq!(
            AutonomyLevel::parse("semiautonomous"),
            Some(AutonomyLevel::SemiAutonomous)
        );

        // FullAutonomous variants
        assert_eq!(
            AutonomyLevel::parse("full_autonomous"),
            Some(AutonomyLevel::FullAutonomous)
        );
        assert_eq!(
            AutonomyLevel::parse("fullautonomous"),
            Some(AutonomyLevel::FullAutonomous)
        );
    }

    #[test]
    fn test_risk_level_color() {
        assert!(RiskLevel::Safe.color().contains("32")); // green
        assert!(RiskLevel::Low.color().contains("33")); // yellow
        assert!(RiskLevel::Medium.color().contains("33")); // yellow
        assert!(RiskLevel::High.color().contains("31")); // red
        assert!(RiskLevel::Critical.color().contains("91")); // bright red
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(format!("{}", RiskLevel::Safe), "Safe");
        assert_eq!(format!("{}", RiskLevel::Low), "Low");
        assert_eq!(format!("{}", RiskLevel::Medium), "Medium");
        assert_eq!(format!("{}", RiskLevel::High), "High");
        assert_eq!(format!("{}", RiskLevel::Critical), "Critical");
    }

    #[test]
    fn test_file_access_all_variants() {
        let accesses = vec![
            (FileAccess::Read, RiskLevel::Safe),
            (FileAccess::List, RiskLevel::Safe),
            (FileAccess::Create, RiskLevel::Low),
            (FileAccess::Write, RiskLevel::Medium),
            (FileAccess::Delete, RiskLevel::High),
            (FileAccess::Execute, RiskLevel::High),
        ];
        for (access, expected_risk) in accesses {
            assert_eq!(access.risk_level(), expected_risk);
        }
    }

    #[test]
    fn test_file_access_display() {
        assert_eq!(format!("{}", FileAccess::Read), "read");
        assert_eq!(format!("{}", FileAccess::Write), "write");
        assert_eq!(format!("{}", FileAccess::Create), "create");
        assert_eq!(format!("{}", FileAccess::Delete), "delete");
        assert_eq!(format!("{}", FileAccess::Execute), "execute");
        assert_eq!(format!("{}", FileAccess::List), "list");
    }

    #[test]
    fn test_network_access_display() {
        assert_eq!(format!("{}", NetworkAccess::Connect), "connect");
        assert_eq!(format!("{}", NetworkAccess::Listen), "listen");
        assert_eq!(format!("{}", NetworkAccess::Dns), "dns");
    }

    #[test]
    fn test_rule_action_serde() {
        let actions = vec![RuleAction::Allow, RuleAction::Deny, RuleAction::Log];
        for action in actions {
            let json = serde_json::to_string(&action).unwrap();
            let parsed: RuleAction = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, action);
        }
    }

    #[test]
    fn test_rule_action_default() {
        assert_eq!(RuleAction::default(), RuleAction::Deny);
    }

    #[test]
    fn test_network_rule_port_range() {
        let rule = NetworkRule::new("range_rule", RuleAction::Allow).port_range(8000, 9000);

        assert!(rule.matches("example.com", 8500, NetworkAccess::Connect));
        assert!(!rule.matches("example.com", 7999, NetworkAccess::Connect));
        assert!(!rule.matches("example.com", 9001, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_rule_access_filter() {
        let rule = NetworkRule::new("listen_only", RuleAction::Allow).access(NetworkAccess::Listen);

        assert!(rule.matches("example.com", 80, NetworkAccess::Listen));
        assert!(!rule.matches("example.com", 80, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_rule_wildcard_all() {
        let rule = NetworkRule::new("all", RuleAction::Allow).host("*");

        assert!(rule.matches("anything.com", 80, NetworkAccess::Connect));
        assert!(rule.matches("localhost", 8080, NetworkAccess::Connect));
    }

    #[test]
    fn test_network_policy_check_log_action() {
        let policy =
            NetworkPolicy::new().add_rule(NetworkRule::new("log_rule", RuleAction::Log).port(8080));

        // Log action should return Log
        let result = policy.check("example.com", 8080, NetworkAccess::Connect);
        assert_eq!(result, RuleAction::Log);
    }

    #[test]
    fn test_network_policy_ipv6_localhost() {
        let policy = NetworkPolicy::new();
        assert!(policy.is_allowed("::1", 8080, NetworkAccess::Connect));
    }

    #[test]
    fn test_resource_limits_cpu_time() {
        let limits = ResourceLimits::new().cpu_time(120);
        assert_eq!(limits.max_cpu_time, Some(120));
    }

    #[test]
    fn test_resource_limits_memory() {
        let limits = ResourceLimits::new().memory(1_000_000);
        assert_eq!(limits.max_memory, Some(1_000_000));
    }

    #[test]
    fn test_resource_limits_timeout() {
        let limits = ResourceLimits::new().timeout(Duration::from_secs(30));
        assert_eq!(limits.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_resource_limits_check_output() {
        let limits = ResourceLimits::new();
        // No limit set - should pass
        assert!(limits.check_output(1_000_000).is_ok());

        // With limit
        let mut limits2 = ResourceLimits::new();
        limits2.max_output_size = Some(1000);
        assert!(limits2.check_output(500).is_ok());
        assert!(limits2.check_output(2000).is_err());
    }

    #[test]
    fn test_audit_result_all_variants() {
        let results = vec![
            (AuditResult::Allowed, "✓"),
            (AuditResult::Denied, "✗"),
            (AuditResult::Prompted, "?"),
            (AuditResult::Failed, "!"),
        ];
        for (result, icon) in results {
            assert_eq!(result.icon(), icon);
        }
    }

    #[test]
    fn test_audit_result_display() {
        assert_eq!(format!("{}", AuditResult::Allowed), "Allowed");
        assert_eq!(format!("{}", AuditResult::Denied), "Denied");
        assert_eq!(format!("{}", AuditResult::Prompted), "Prompted");
        assert_eq!(format!("{}", AuditResult::Failed), "Failed");
    }

    #[test]
    fn test_audit_entry_display() {
        let entry = AuditEntry::new("file_read", "agent", "/tmp/test", AuditResult::Allowed)
            .with_risk(RiskLevel::Safe);
        let display = entry.display();
        assert!(display.contains("agent"));
        assert!(display.contains("file_read"));
        assert!(display.contains("/tmp/test"));
        assert!(display.contains("Allowed"));
    }

    #[test]
    fn test_audit_logger_with_file() {
        let logger = AuditLogger::new().with_file(PathBuf::from("/tmp/audit.log"));
        assert!(logger.log_file.is_some());
    }

    #[test]
    fn test_audit_logger_recent() {
        let mut logger = AuditLogger::new();
        logger.log_action("a1", "agent", "x", AuditResult::Allowed, RiskLevel::Safe);
        logger.log_action("a2", "agent", "y", AuditResult::Denied, RiskLevel::High);
        logger.log_action("a3", "agent", "z", AuditResult::Allowed, RiskLevel::Low);

        let recent = logger.recent(2);
        assert_eq!(recent.len(), 2);
        // Should be in reverse order
        assert_eq!(recent[0].action, "a3");
        assert_eq!(recent[1].action, "a2");
    }

    #[test]
    fn test_audit_logger_by_result() {
        let mut logger = AuditLogger::new();
        logger.log_action("a1", "agent", "x", AuditResult::Allowed, RiskLevel::Safe);
        logger.log_action("a2", "agent", "y", AuditResult::Prompted, RiskLevel::Medium);
        logger.log_action("a3", "agent", "z", AuditResult::Allowed, RiskLevel::Low);

        let prompted = logger.by_result(AuditResult::Prompted);
        assert_eq!(prompted.len(), 1);
        assert_eq!(prompted[0].action, "a2");
    }

    #[test]
    fn test_audit_logger_by_risk() {
        let mut logger = AuditLogger::new();
        logger.log_action("a1", "agent", "x", AuditResult::Allowed, RiskLevel::High);
        logger.log_action("a2", "agent", "y", AuditResult::Allowed, RiskLevel::Low);
        logger.log_action("a3", "agent", "z", AuditResult::Allowed, RiskLevel::High);

        let high_risk = logger.by_risk(RiskLevel::High);
        assert_eq!(high_risk.len(), 2);
    }

    #[test]
    fn test_audit_logger_clear() {
        let mut logger = AuditLogger::new();
        logger.log_action("a1", "agent", "x", AuditResult::Allowed, RiskLevel::Safe);
        logger.log_action("a2", "agent", "y", AuditResult::Denied, RiskLevel::High);
        assert_eq!(logger.count(), 2);

        logger.clear();
        assert_eq!(logger.count(), 0);
    }

    #[test]
    fn test_audit_summary_display() {
        let summary = AuditSummary {
            total: 100,
            allowed: 80,
            denied: 15,
            prompted: 5,
            high_risk: 10,
        };
        let display = summary.display();
        assert!(display.contains("100"));
        assert!(display.contains("80"));
        assert!(display.contains("15"));
        assert!(display.contains("5"));
        assert!(display.contains("10"));
    }

    #[test]
    fn test_security_sandbox_default() {
        let sandbox = SecuritySandbox::default();
        assert!(sandbox.enabled);
        assert_eq!(sandbox.autonomy, AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_security_sandbox_with_autonomy() {
        let sandbox = SecuritySandbox::new().with_autonomy(AutonomyLevel::FullAutonomous);
        assert_eq!(sandbox.autonomy, AutonomyLevel::FullAutonomous);
    }

    #[test]
    fn test_security_sandbox_needs_confirmation_all_levels() {
        // SuggestOnly - always needs confirmation
        let sandbox = SecuritySandbox::new().with_autonomy(AutonomyLevel::SuggestOnly);
        assert!(sandbox.needs_confirmation(RiskLevel::Safe));
        assert!(sandbox.needs_confirmation(RiskLevel::Critical));

        // ConfirmDestructive - only destructive needs confirmation
        let sandbox = SecuritySandbox::new().with_autonomy(AutonomyLevel::ConfirmDestructive);
        assert!(!sandbox.needs_confirmation(RiskLevel::Safe));
        assert!(!sandbox.needs_confirmation(RiskLevel::Medium));
        assert!(sandbox.needs_confirmation(RiskLevel::High));

        // SemiAutonomous - only critical needs confirmation
        let sandbox = SecuritySandbox::new().with_autonomy(AutonomyLevel::SemiAutonomous);
        assert!(!sandbox.needs_confirmation(RiskLevel::High));
        assert!(sandbox.needs_confirmation(RiskLevel::Critical));

        // FullAutonomous - never needs confirmation
        let sandbox = SecuritySandbox::new().with_autonomy(AutonomyLevel::FullAutonomous);
        assert!(!sandbox.needs_confirmation(RiskLevel::Critical));
    }

    #[test]
    fn test_sandbox_status_serde() {
        let status = SandboxStatus {
            enabled: true,
            autonomy: AutonomyLevel::SemiAutonomous,
            audit_count: 50,
            denied_count: 3,
        };
        let json = serde_json::to_string(&status).unwrap();
        let parsed: SandboxStatus = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.autonomy, AutonomyLevel::SemiAutonomous);
        assert_eq!(parsed.audit_count, 50);
    }

    #[test]
    fn test_port_spec_serde() {
        let specs = vec![
            PortSpec::Single(80),
            PortSpec::Range(8000, 9000),
            PortSpec::List(vec![80, 443, 8080]),
        ];
        for spec in specs {
            let json = serde_json::to_string(&spec).unwrap();
            let _: PortSpec = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_filesystem_policy_allowed_extensions() {
        let mut policy = FilesystemPolicy::new();
        policy.allowed_extensions = Some(["rs", "txt"].iter().map(|s| s.to_string()).collect());

        // Test uses /tmp which should exist
        let allowed_path = std::env::temp_dir().join("test.rs");
        assert!(policy.is_allowed(&allowed_path, FileAccess::Read).is_ok());

        let denied_path = std::env::temp_dir().join("test.exe");
        assert!(policy.is_allowed(&denied_path, FileAccess::Read).is_err());
    }

    #[test]
    fn test_filesystem_policy_hidden_files() {
        let mut policy = FilesystemPolicy::new();
        policy.allow_hidden = false;

        let hidden_path = std::env::temp_dir().join(".hidden");
        assert!(policy.is_allowed(&hidden_path, FileAccess::Read).is_err());
    }

    #[test]
    fn test_network_rule_serde() {
        let rule = NetworkRule::new("test_rule", RuleAction::Allow)
            .host("*.example.com")
            .port(443)
            .access(NetworkAccess::Connect);

        let json = serde_json::to_string(&rule).unwrap();
        let parsed: NetworkRule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test_rule");
        assert_eq!(parsed.action, RuleAction::Allow);
    }

    #[test]
    fn test_resource_limits_serde() {
        let limits = ResourceLimits::new().memory_mb(512).timeout_secs(60);

        let json = serde_json::to_string(&limits).unwrap();
        let parsed: ResourceLimits = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_memory, Some(512 * 1024 * 1024));
    }

    #[test]
    fn test_audit_entry_serde() {
        let entry = AuditEntry::new("test", "agent", "/path", AuditResult::Allowed)
            .with_details("details")
            .with_risk(RiskLevel::Medium);

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: AuditEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.action, "test");
        assert_eq!(parsed.risk, RiskLevel::Medium);
    }

    #[test]
    fn test_security_sandbox_check_network_log_action() {
        let mut sandbox = SecuritySandbox::new();
        sandbox.network =
            NetworkPolicy::new().add_rule(NetworkRule::new("log_http", RuleAction::Log).port(80));

        let result = sandbox.check_network_access("example.com", 80, NetworkAccess::Connect);
        assert!(result.is_ok());
        assert!(result.unwrap()); // Log action allows
    }

    #[test]
    fn test_security_sandbox_disabled_network() {
        let mut sandbox = SecuritySandbox::new();
        sandbox.set_enabled(false);

        // When disabled, even external hosts should be allowed
        let result = sandbox.check_network_access("malicious.com", 80, NetworkAccess::Connect);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }
}
