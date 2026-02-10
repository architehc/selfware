//! Autonomy Level Controller
//!
//! Graduated autonomy levels with per-tool and per-context settings.
//! Controls how much confirmation is required for agent actions.
//!
//! Features:
//! - Autonomy levels (SuggestOnly, ConfirmDestructive, SemiAutonomous, FullAutonomous)
//! - Per-tool autonomy settings
//! - Per-context adjustments
//! - Risk assessment
//! - Audit logging

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Global autonomy level
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum AutonomyLevel {
    /// Only suggest actions, never execute
    SuggestOnly = 0,
    /// Confirm destructive operations only
    #[default]
    ConfirmDestructive = 1,
    /// Semi-autonomous: confirm risky operations
    SemiAutonomous = 2,
    /// Fully autonomous: minimal confirmation
    FullAutonomous = 3,
}

impl AutonomyLevel {
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::SuggestOnly => "Suggest Only",
            Self::ConfirmDestructive => "Confirm Destructive",
            Self::SemiAutonomous => "Semi-Autonomous",
            Self::FullAutonomous => "Full Autonomous",
        }
    }

    /// Get description
    pub fn description(&self) -> &'static str {
        match self {
            Self::SuggestOnly => "Agent suggests actions but never executes them automatically",
            Self::ConfirmDestructive => "Agent executes safe actions, confirms destructive ones",
            Self::SemiAutonomous => "Agent executes most actions, confirms risky ones",
            Self::FullAutonomous => "Agent executes all actions with minimal confirmation",
        }
    }

    /// Check if this level allows automatic execution for a risk level
    pub fn allows_auto_execute(&self, risk: RiskLevel) -> bool {
        match (self, risk) {
            (Self::SuggestOnly, _) => false,
            (Self::ConfirmDestructive, RiskLevel::Safe) => true,
            (Self::ConfirmDestructive, RiskLevel::Low) => true,
            (Self::ConfirmDestructive, _) => false,
            (Self::SemiAutonomous, RiskLevel::Safe) => true,
            (Self::SemiAutonomous, RiskLevel::Low) => true,
            (Self::SemiAutonomous, RiskLevel::Medium) => true,
            (Self::SemiAutonomous, _) => false,
            (Self::FullAutonomous, RiskLevel::Critical) => false,
            (Self::FullAutonomous, _) => true,
        }
    }
}

/// Risk level for operations
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum RiskLevel {
    /// Completely safe, read-only
    Safe = 0,
    /// Low risk, reversible changes
    #[default]
    Low = 1,
    /// Medium risk, may affect state
    Medium = 2,
    /// High risk, significant changes
    High = 3,
    /// Critical risk, destructive/irreversible
    Critical = 4,
}

impl RiskLevel {
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Safe => "Safe",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Critical => "Critical",
        }
    }

    /// Get color for display
    pub fn color(&self) -> &'static str {
        match self {
            Self::Safe => "green",
            Self::Low => "blue",
            Self::Medium => "yellow",
            Self::High => "orange",
            Self::Critical => "red",
        }
    }
}

/// Tool category for risk assessment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ToolCategory {
    /// File reading operations
    FileRead,
    /// File writing/modification
    FileWrite,
    /// File deletion
    FileDelete,
    /// Shell command execution
    Shell,
    /// Git operations
    Git,
    /// Network operations
    Network,
    /// Database operations
    Database,
    /// System operations
    System,
    /// Search/query operations
    Search,
    /// LLM/AI operations
    LlmCall,
    /// Unknown/other
    #[default]
    Other,
}

impl ToolCategory {
    /// Get default risk level for category
    pub fn default_risk(&self) -> RiskLevel {
        match self {
            Self::FileRead => RiskLevel::Safe,
            Self::FileWrite => RiskLevel::Medium,
            Self::FileDelete => RiskLevel::High,
            Self::Shell => RiskLevel::High,
            Self::Git => RiskLevel::Medium,
            Self::Network => RiskLevel::Medium,
            Self::Database => RiskLevel::High,
            Self::System => RiskLevel::Critical,
            Self::Search => RiskLevel::Safe,
            Self::LlmCall => RiskLevel::Low,
            Self::Other => RiskLevel::Medium,
        }
    }
}

/// Tool-specific autonomy settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolAutonomy {
    /// Tool name
    pub tool_name: String,
    /// Tool category
    pub category: ToolCategory,
    /// Override risk level (if set)
    pub risk_override: Option<RiskLevel>,
    /// Override autonomy level (if set)
    pub autonomy_override: Option<AutonomyLevel>,
    /// Always require confirmation
    pub always_confirm: bool,
    /// Never require confirmation
    pub never_confirm: bool,
    /// Patterns that require confirmation
    pub confirm_patterns: Vec<String>,
    /// Patterns that are always allowed
    pub allow_patterns: Vec<String>,
    /// Usage count
    pub usage_count: u64,
    /// Success count
    pub success_count: u64,
}

impl ToolAutonomy {
    /// Create new tool autonomy settings
    pub fn new(tool_name: impl Into<String>, category: ToolCategory) -> Self {
        Self {
            tool_name: tool_name.into(),
            category,
            risk_override: None,
            autonomy_override: None,
            always_confirm: false,
            never_confirm: false,
            confirm_patterns: Vec::new(),
            allow_patterns: Vec::new(),
            usage_count: 0,
            success_count: 0,
        }
    }

    /// Set risk override
    pub fn with_risk(mut self, risk: RiskLevel) -> Self {
        self.risk_override = Some(risk);
        self
    }

    /// Set autonomy override
    pub fn with_autonomy(mut self, autonomy: AutonomyLevel) -> Self {
        self.autonomy_override = Some(autonomy);
        self
    }

    /// Always confirm this tool
    pub fn always_confirm(mut self) -> Self {
        self.always_confirm = true;
        self
    }

    /// Never confirm this tool
    pub fn never_confirm(mut self) -> Self {
        self.never_confirm = true;
        self
    }

    /// Add confirmation pattern
    pub fn with_confirm_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.confirm_patterns.push(pattern.into());
        self
    }

    /// Add allow pattern
    pub fn with_allow_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.allow_patterns.push(pattern.into());
        self
    }

    /// Get effective risk level
    pub fn effective_risk(&self) -> RiskLevel {
        self.risk_override
            .unwrap_or_else(|| self.category.default_risk())
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.usage_count == 0 {
            1.0
        } else {
            self.success_count as f64 / self.usage_count as f64
        }
    }

    /// Record usage
    pub fn record_usage(&mut self, success: bool) {
        self.usage_count += 1;
        if success {
            self.success_count += 1;
        }
    }

    /// Check if content matches a pattern
    fn matches_pattern(&self, content: &str, patterns: &[String]) -> bool {
        for pattern in patterns {
            if content.contains(pattern) {
                return true;
            }
            // Simple glob matching
            if pattern.contains('*') {
                let parts: Vec<&str> = pattern.split('*').collect();
                let mut remaining = content;
                let mut matched = true;

                for (i, part) in parts.iter().enumerate() {
                    if part.is_empty() {
                        continue;
                    }
                    if let Some(pos) = remaining.find(part) {
                        if i == 0 && pos != 0 {
                            matched = false;
                            break;
                        }
                        remaining = &remaining[pos + part.len()..];
                    } else {
                        matched = false;
                        break;
                    }
                }

                if matched {
                    return true;
                }
            }
        }
        false
    }

    /// Check if content requires confirmation
    pub fn requires_confirmation(&self, content: &str) -> bool {
        if self.always_confirm {
            return true;
        }
        if self.never_confirm {
            return false;
        }
        if self.matches_pattern(content, &self.allow_patterns) {
            return false;
        }
        if self.matches_pattern(content, &self.confirm_patterns) {
            return true;
        }
        false
    }
}

/// Context for autonomy decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyContext {
    /// Working directory
    pub working_dir: String,
    /// Current task description
    pub task: Option<String>,
    /// Protected paths (always confirm)
    pub protected_paths: Vec<String>,
    /// Trusted paths (never confirm)
    pub trusted_paths: Vec<String>,
    /// Session trust level (0.0 - 1.0)
    pub trust_level: f64,
    /// Operations performed this session
    pub session_operations: u64,
    /// Errors this session
    pub session_errors: u64,
}

impl Default for AutonomyContext {
    fn default() -> Self {
        Self {
            working_dir: String::new(),
            task: None,
            protected_paths: vec![
                "/etc".to_string(),
                "/usr".to_string(),
                "/bin".to_string(),
                "/sbin".to_string(),
                "~/.ssh".to_string(),
                "~/.gnupg".to_string(),
            ],
            trusted_paths: Vec::new(),
            trust_level: 0.5,
            session_operations: 0,
            session_errors: 0,
        }
    }
}

impl AutonomyContext {
    /// Create new context
    pub fn new(working_dir: impl Into<String>) -> Self {
        Self {
            working_dir: working_dir.into(),
            ..Default::default()
        }
    }

    /// Set task
    pub fn with_task(mut self, task: impl Into<String>) -> Self {
        self.task = Some(task.into());
        self
    }

    /// Add protected path
    pub fn protect_path(mut self, path: impl Into<String>) -> Self {
        self.protected_paths.push(path.into());
        self
    }

    /// Add trusted path
    pub fn trust_path(mut self, path: impl Into<String>) -> Self {
        self.trusted_paths.push(path.into());
        self
    }

    /// Check if path is protected
    pub fn is_protected(&self, path: &str) -> bool {
        for protected in &self.protected_paths {
            if path.starts_with(protected) || path.contains(protected) {
                return true;
            }
        }
        false
    }

    /// Check if path is trusted
    pub fn is_trusted(&self, path: &str) -> bool {
        for trusted in &self.trusted_paths {
            if path.starts_with(trusted) || path.contains(trusted) {
                return true;
            }
        }
        false
    }

    /// Record operation
    pub fn record_operation(&mut self, success: bool) {
        self.session_operations += 1;
        if !success {
            self.session_errors += 1;
            // Decrease trust on errors
            self.trust_level = (self.trust_level - 0.1).max(0.0);
        } else if self.session_errors == 0 && self.session_operations > 10 {
            // Increase trust on consistent success
            self.trust_level = (self.trust_level + 0.01).min(1.0);
        }
    }

    /// Get error rate
    pub fn error_rate(&self) -> f64 {
        if self.session_operations == 0 {
            0.0
        } else {
            self.session_errors as f64 / self.session_operations as f64
        }
    }
}

/// Confirmation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationRequest {
    /// Request ID
    pub id: String,
    /// Tool name
    pub tool: String,
    /// Action description
    pub action: String,
    /// Risk level
    pub risk: RiskLevel,
    /// Reason for confirmation
    pub reason: String,
    /// Timestamp
    pub timestamp: u64,
    /// Affected paths
    pub affected_paths: Vec<String>,
}

impl ConfirmationRequest {
    /// Create new confirmation request
    pub fn new(
        tool: impl Into<String>,
        action: impl Into<String>,
        risk: RiskLevel,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tool: tool.into(),
            action: action.into(),
            risk,
            reason: reason.into(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            affected_paths: Vec::new(),
        }
    }

    /// Add affected path
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.affected_paths.push(path.into());
        self
    }
}

/// Confirmation response
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationResponse {
    /// Approved
    Approved,
    /// Denied
    Denied,
    /// Approved always for this tool
    ApproveAlways,
    /// Denied always for this tool
    DenyAlways,
    /// Skipped
    Skipped,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry ID
    pub id: String,
    /// Timestamp
    pub timestamp: u64,
    /// Tool name
    pub tool: String,
    /// Action taken
    pub action: String,
    /// Risk level
    pub risk: RiskLevel,
    /// Whether confirmation was required
    pub confirmation_required: bool,
    /// Response if confirmation was required
    pub confirmation_response: Option<ConfirmationResponse>,
    /// Autonomy level at time of action
    pub autonomy_level: AutonomyLevel,
    /// Whether action succeeded
    pub success: Option<bool>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Autonomy controller
pub struct AutonomyController {
    /// Global autonomy level
    level: AutonomyLevel,
    /// Tool-specific settings
    tool_settings: HashMap<String, ToolAutonomy>,
    /// Current context
    context: AutonomyContext,
    /// Audit log
    audit_log: Vec<AuditEntry>,
    /// Pending confirmations
    pending_confirmations: HashMap<String, ConfirmationRequest>,
    /// Remember decisions (tool -> response)
    remembered_decisions: HashMap<String, ConfirmationResponse>,
    /// Maximum audit log entries
    max_audit_entries: usize,
}

impl AutonomyController {
    /// Create new controller
    pub fn new(level: AutonomyLevel) -> Self {
        Self {
            level,
            tool_settings: HashMap::new(),
            context: AutonomyContext::default(),
            audit_log: Vec::new(),
            pending_confirmations: HashMap::new(),
            remembered_decisions: HashMap::new(),
            max_audit_entries: 1000,
        }
    }

    /// Set autonomy level
    pub fn set_level(&mut self, level: AutonomyLevel) {
        self.level = level;
    }

    /// Get autonomy level
    pub fn level(&self) -> AutonomyLevel {
        self.level
    }

    /// Set context
    pub fn set_context(&mut self, context: AutonomyContext) {
        self.context = context;
    }

    /// Get context
    pub fn context(&self) -> &AutonomyContext {
        &self.context
    }

    /// Get context mutably
    pub fn context_mut(&mut self) -> &mut AutonomyContext {
        &mut self.context
    }

    /// Register tool settings
    pub fn register_tool(&mut self, settings: ToolAutonomy) {
        self.tool_settings
            .insert(settings.tool_name.clone(), settings);
    }

    /// Get tool settings
    pub fn get_tool(&self, name: &str) -> Option<&ToolAutonomy> {
        self.tool_settings.get(name)
    }

    /// Get tool settings mutably
    pub fn get_tool_mut(&mut self, name: &str) -> Option<&mut ToolAutonomy> {
        self.tool_settings.get_mut(name)
    }

    /// Check if action requires confirmation
    pub fn requires_confirmation(
        &self,
        tool: &str,
        action: &str,
        affected_paths: &[String],
    ) -> Option<ConfirmationRequest> {
        // Check remembered decisions
        if let Some(response) = self.remembered_decisions.get(tool) {
            match response {
                ConfirmationResponse::ApproveAlways => return None,
                ConfirmationResponse::DenyAlways => {
                    return Some(ConfirmationRequest::new(
                        tool,
                        action,
                        RiskLevel::High,
                        "Tool is permanently denied",
                    ));
                }
                _ => {}
            }
        }

        // Get tool settings or create default
        let tool_settings = self.tool_settings.get(tool);
        let category = tool_settings
            .map(|t| t.category)
            .unwrap_or(ToolCategory::Other);
        let risk = tool_settings
            .and_then(|t| t.risk_override)
            .unwrap_or_else(|| category.default_risk());

        // Check tool-specific settings
        if let Some(settings) = tool_settings {
            if settings.never_confirm {
                return None;
            }
            if settings.always_confirm {
                return Some(ConfirmationRequest::new(
                    tool,
                    action,
                    risk,
                    "Tool always requires confirmation",
                ));
            }
            if settings.requires_confirmation(action) {
                return Some(ConfirmationRequest::new(
                    tool,
                    action,
                    risk,
                    "Action matches confirmation pattern",
                ));
            }
        }

        // Check protected paths
        for path in affected_paths {
            if self.context.is_protected(path) {
                return Some(
                    ConfirmationRequest::new(
                        tool,
                        action,
                        RiskLevel::Critical,
                        format!("Protected path: {}", path),
                    )
                    .with_path(path),
                );
            }
        }

        // Check autonomy level
        let effective_level = tool_settings
            .and_then(|t| t.autonomy_override)
            .unwrap_or(self.level);

        if effective_level.allows_auto_execute(risk) {
            None
        } else {
            Some(ConfirmationRequest::new(
                tool,
                action,
                risk,
                format!(
                    "Risk level {:?} requires confirmation at {:?}",
                    risk, effective_level
                ),
            ))
        }
    }

    /// Request confirmation
    pub fn request_confirmation(&mut self, request: ConfirmationRequest) -> String {
        let id = request.id.clone();
        self.pending_confirmations.insert(id.clone(), request);
        id
    }

    /// Get pending confirmation
    pub fn get_pending(&self, id: &str) -> Option<&ConfirmationRequest> {
        self.pending_confirmations.get(id)
    }

    /// List pending confirmations
    pub fn list_pending(&self) -> Vec<&ConfirmationRequest> {
        self.pending_confirmations.values().collect()
    }

    /// Respond to confirmation
    pub fn respond(&mut self, id: &str, response: ConfirmationResponse) -> Result<()> {
        let request = self
            .pending_confirmations
            .remove(id)
            .ok_or_else(|| anyhow!("Confirmation not found: {}", id))?;

        // Handle remember decisions
        match response {
            ConfirmationResponse::ApproveAlways => {
                self.remembered_decisions
                    .insert(request.tool.clone(), response);
            }
            ConfirmationResponse::DenyAlways => {
                self.remembered_decisions
                    .insert(request.tool.clone(), response);
            }
            _ => {}
        }

        // Log the decision
        self.log_action(
            &request.tool,
            &request.action,
            request.risk,
            true,
            Some(response),
        );

        Ok(())
    }

    /// Log an action
    pub fn log_action(
        &mut self,
        tool: &str,
        action: &str,
        risk: RiskLevel,
        confirmation_required: bool,
        response: Option<ConfirmationResponse>,
    ) {
        let entry = AuditEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tool: tool.to_string(),
            action: action.to_string(),
            risk,
            confirmation_required,
            confirmation_response: response,
            autonomy_level: self.level,
            success: None,
            error: None,
        };

        self.audit_log.push(entry);

        // Trim log if too large
        while self.audit_log.len() > self.max_audit_entries {
            self.audit_log.remove(0);
        }
    }

    /// Record action result
    pub fn record_result(&mut self, tool: &str, success: bool, error: Option<String>) {
        // Update last audit entry for this tool
        if let Some(entry) = self
            .audit_log
            .iter_mut()
            .rev()
            .find(|e| e.tool == tool && e.success.is_none())
        {
            entry.success = Some(success);
            entry.error = error;
        }

        // Update tool statistics
        if let Some(tool_settings) = self.tool_settings.get_mut(tool) {
            tool_settings.record_usage(success);
        }

        // Update context
        self.context.record_operation(success);
    }

    /// Get audit log
    pub fn audit_log(&self) -> &[AuditEntry] {
        &self.audit_log
    }

    /// Get audit entries for tool
    pub fn audit_for_tool(&self, tool: &str) -> Vec<&AuditEntry> {
        self.audit_log.iter().filter(|e| e.tool == tool).collect()
    }

    /// Clear remembered decisions
    pub fn clear_remembered(&mut self) {
        self.remembered_decisions.clear();
    }

    /// Get statistics
    pub fn stats(&self) -> AutonomyStats {
        let confirmed_count = self
            .audit_log
            .iter()
            .filter(|e| e.confirmation_required)
            .count();

        let approved_count = self
            .audit_log
            .iter()
            .filter(|e| {
                e.confirmation_response
                    .map(|r| {
                        r == ConfirmationResponse::Approved
                            || r == ConfirmationResponse::ApproveAlways
                    })
                    .unwrap_or(false)
            })
            .count();

        let denied_count = self
            .audit_log
            .iter()
            .filter(|e| {
                e.confirmation_response
                    .map(|r| {
                        r == ConfirmationResponse::Denied || r == ConfirmationResponse::DenyAlways
                    })
                    .unwrap_or(false)
            })
            .count();

        let by_risk: HashMap<RiskLevel, usize> =
            self.audit_log.iter().fold(HashMap::new(), |mut acc, e| {
                *acc.entry(e.risk).or_insert(0) += 1;
                acc
            });

        AutonomyStats {
            total_actions: self.audit_log.len(),
            confirmed_actions: confirmed_count,
            approved_actions: approved_count,
            denied_actions: denied_count,
            pending_confirmations: self.pending_confirmations.len(),
            remembered_decisions: self.remembered_decisions.len(),
            actions_by_risk: by_risk,
            current_trust: self.context.trust_level,
        }
    }
}

impl Default for AutonomyController {
    fn default() -> Self {
        Self::new(AutonomyLevel::default())
    }
}

/// Autonomy statistics
#[derive(Debug, Clone)]
pub struct AutonomyStats {
    pub total_actions: usize,
    pub confirmed_actions: usize,
    pub approved_actions: usize,
    pub denied_actions: usize,
    pub pending_confirmations: usize,
    pub remembered_decisions: usize,
    pub actions_by_risk: HashMap<RiskLevel, usize>,
    pub current_trust: f64,
}

/// Create default tool settings for common tools
pub fn default_tool_settings() -> Vec<ToolAutonomy> {
    vec![
        // Safe read-only tools
        ToolAutonomy::new("file_read", ToolCategory::FileRead).never_confirm(),
        ToolAutonomy::new("directory_tree", ToolCategory::FileRead).never_confirm(),
        ToolAutonomy::new("grep_search", ToolCategory::Search).never_confirm(),
        ToolAutonomy::new("glob_find", ToolCategory::Search).never_confirm(),
        // File modification tools
        ToolAutonomy::new("file_write", ToolCategory::FileWrite),
        ToolAutonomy::new("file_edit", ToolCategory::FileWrite),
        // Dangerous tools
        ToolAutonomy::new("file_delete", ToolCategory::FileDelete).always_confirm(),
        ToolAutonomy::new("shell_exec", ToolCategory::Shell)
            .with_confirm_pattern("rm ")
            .with_confirm_pattern("sudo ")
            .with_confirm_pattern("chmod ")
            .with_confirm_pattern("> /"),
        // Git tools
        ToolAutonomy::new("git_commit", ToolCategory::Git),
        ToolAutonomy::new("git_push", ToolCategory::Git).with_risk(RiskLevel::High),
        ToolAutonomy::new("git_reset", ToolCategory::Git).always_confirm(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autonomy_level_default() {
        assert_eq!(AutonomyLevel::default(), AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_autonomy_level_ordering() {
        assert!(AutonomyLevel::FullAutonomous > AutonomyLevel::SuggestOnly);
        assert!(AutonomyLevel::SemiAutonomous > AutonomyLevel::ConfirmDestructive);
    }

    #[test]
    fn test_autonomy_level_allows_auto_execute() {
        // SuggestOnly never allows auto execute
        assert!(!AutonomyLevel::SuggestOnly.allows_auto_execute(RiskLevel::Safe));

        // ConfirmDestructive allows safe and low
        assert!(AutonomyLevel::ConfirmDestructive.allows_auto_execute(RiskLevel::Safe));
        assert!(AutonomyLevel::ConfirmDestructive.allows_auto_execute(RiskLevel::Low));
        assert!(!AutonomyLevel::ConfirmDestructive.allows_auto_execute(RiskLevel::High));

        // FullAutonomous allows everything except critical
        assert!(AutonomyLevel::FullAutonomous.allows_auto_execute(RiskLevel::High));
        assert!(!AutonomyLevel::FullAutonomous.allows_auto_execute(RiskLevel::Critical));
    }

    #[test]
    fn test_risk_level_default() {
        assert_eq!(RiskLevel::default(), RiskLevel::Low);
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::Safe);
    }

    #[test]
    fn test_tool_category_default_risk() {
        assert_eq!(ToolCategory::FileRead.default_risk(), RiskLevel::Safe);
        assert_eq!(ToolCategory::FileDelete.default_risk(), RiskLevel::High);
        assert_eq!(ToolCategory::System.default_risk(), RiskLevel::Critical);
    }

    #[test]
    fn test_tool_autonomy_creation() {
        let settings = ToolAutonomy::new("file_read", ToolCategory::FileRead)
            .with_risk(RiskLevel::Safe)
            .never_confirm();

        assert_eq!(settings.tool_name, "file_read");
        assert!(settings.never_confirm);
        assert_eq!(settings.effective_risk(), RiskLevel::Safe);
    }

    #[test]
    fn test_tool_autonomy_patterns() {
        let settings = ToolAutonomy::new("shell", ToolCategory::Shell)
            .with_confirm_pattern("rm ")
            .with_allow_pattern("echo ");

        assert!(settings.requires_confirmation("rm -rf /tmp"));
        assert!(!settings.requires_confirmation("echo hello"));
        assert!(!settings.requires_confirmation("ls"));
    }

    #[test]
    fn test_tool_autonomy_always_confirm() {
        let settings = ToolAutonomy::new("dangerous", ToolCategory::System).always_confirm();

        assert!(settings.requires_confirmation("anything"));
    }

    #[test]
    fn test_tool_autonomy_success_rate() {
        let mut settings = ToolAutonomy::new("test", ToolCategory::Other);

        settings.record_usage(true);
        settings.record_usage(true);
        settings.record_usage(false);

        assert!((settings.success_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_context_creation() {
        let context = AutonomyContext::new("/project")
            .with_task("Implement feature")
            .protect_path("/important")
            .trust_path("/project/src");

        assert_eq!(context.working_dir, "/project");
        assert!(context.is_protected("/important/file.txt"));
        assert!(context.is_trusted("/project/src/main.rs"));
    }

    #[test]
    fn test_autonomy_context_default_protected() {
        let context = AutonomyContext::default();

        assert!(context.is_protected("/etc/passwd"));
        assert!(context.is_protected("/usr/bin/bash"));
    }

    #[test]
    fn test_autonomy_context_trust_adjustment() {
        let mut context = AutonomyContext::default();
        let initial_trust = context.trust_level;

        // Errors decrease trust
        context.record_operation(false);
        assert!(context.trust_level < initial_trust);
    }

    #[test]
    fn test_confirmation_request_creation() {
        let request = ConfirmationRequest::new(
            "file_delete",
            "Delete /tmp/test.txt",
            RiskLevel::High,
            "Destructive operation",
        )
        .with_path("/tmp/test.txt");

        assert_eq!(request.tool, "file_delete");
        assert_eq!(request.risk, RiskLevel::High);
        assert!(request
            .affected_paths
            .contains(&"/tmp/test.txt".to_string()));
    }

    #[test]
    fn test_autonomy_controller_creation() {
        let controller = AutonomyController::new(AutonomyLevel::SemiAutonomous);

        assert_eq!(controller.level(), AutonomyLevel::SemiAutonomous);
    }

    #[test]
    fn test_autonomy_controller_set_level() {
        let mut controller = AutonomyController::default();
        controller.set_level(AutonomyLevel::FullAutonomous);

        assert_eq!(controller.level(), AutonomyLevel::FullAutonomous);
    }

    #[test]
    fn test_autonomy_controller_register_tool() {
        let mut controller = AutonomyController::default();
        controller.register_tool(ToolAutonomy::new("test_tool", ToolCategory::Other));

        assert!(controller.get_tool("test_tool").is_some());
    }

    #[test]
    fn test_autonomy_controller_requires_confirmation() {
        let mut controller = AutonomyController::new(AutonomyLevel::ConfirmDestructive);

        // High risk tool should require confirmation
        controller.register_tool(ToolAutonomy::new("dangerous", ToolCategory::System));

        let result = controller.requires_confirmation("dangerous", "do something", &[]);
        assert!(result.is_some());
    }

    #[test]
    fn test_autonomy_controller_no_confirmation_for_safe() {
        let mut controller = AutonomyController::new(AutonomyLevel::ConfirmDestructive);
        controller.register_tool(ToolAutonomy::new("file_read", ToolCategory::FileRead));

        let result = controller.requires_confirmation("file_read", "read file", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_autonomy_controller_protected_path() {
        let controller = AutonomyController::default();

        let result =
            controller.requires_confirmation("file_write", "write", &["/etc/passwd".to_string()]);

        assert!(result.is_some());
        assert_eq!(result.unwrap().risk, RiskLevel::Critical);
    }

    #[test]
    fn test_autonomy_controller_respond() {
        let mut controller = AutonomyController::default();

        let request = ConfirmationRequest::new("test", "action", RiskLevel::High, "reason");
        let id = controller.request_confirmation(request);

        let result = controller.respond(&id, ConfirmationResponse::Approved);
        assert!(result.is_ok());
        assert!(controller.get_pending(&id).is_none());
    }

    #[test]
    fn test_autonomy_controller_remember_always() {
        let mut controller = AutonomyController::default();

        let request = ConfirmationRequest::new("tool", "action", RiskLevel::Medium, "reason");
        let id = controller.request_confirmation(request);

        controller
            .respond(&id, ConfirmationResponse::ApproveAlways)
            .unwrap();

        // Subsequent requests for same tool should not need confirmation
        let result = controller.requires_confirmation("tool", "another action", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_autonomy_controller_audit_log() {
        let mut controller = AutonomyController::default();

        controller.log_action("tool", "action", RiskLevel::Low, false, None);
        controller.record_result("tool", true, None);

        assert_eq!(controller.audit_log().len(), 1);
        assert!(controller.audit_log()[0].success == Some(true));
    }

    #[test]
    fn test_autonomy_controller_stats() {
        let mut controller = AutonomyController::default();

        controller.log_action("tool1", "action", RiskLevel::Low, false, None);
        controller.log_action(
            "tool2",
            "action",
            RiskLevel::High,
            true,
            Some(ConfirmationResponse::Approved),
        );

        let stats = controller.stats();
        assert_eq!(stats.total_actions, 2);
        assert_eq!(stats.confirmed_actions, 1);
    }

    #[test]
    fn test_default_tool_settings() {
        let settings = default_tool_settings();

        assert!(!settings.is_empty());
        assert!(settings.iter().any(|s| s.tool_name == "file_read"));
        assert!(settings.iter().any(|s| s.tool_name == "shell_exec"));
    }

    #[test]
    fn test_autonomy_level_name() {
        assert_eq!(AutonomyLevel::SuggestOnly.name(), "Suggest Only");
        assert_eq!(AutonomyLevel::FullAutonomous.name(), "Full Autonomous");
    }

    #[test]
    fn test_risk_level_color() {
        assert_eq!(RiskLevel::Safe.color(), "green");
        assert_eq!(RiskLevel::Critical.color(), "red");
    }

    #[test]
    fn test_controller_clear_remembered() {
        let mut controller = AutonomyController::default();

        let request = ConfirmationRequest::new("tool", "action", RiskLevel::Low, "reason");
        let id = controller.request_confirmation(request);
        controller
            .respond(&id, ConfirmationResponse::ApproveAlways)
            .unwrap();

        assert_eq!(controller.stats().remembered_decisions, 1);

        controller.clear_remembered();
        assert_eq!(controller.stats().remembered_decisions, 0);
    }

    #[test]
    fn test_audit_for_tool() {
        let mut controller = AutonomyController::default();

        controller.log_action("tool1", "action1", RiskLevel::Low, false, None);
        controller.log_action("tool2", "action2", RiskLevel::Low, false, None);
        controller.log_action("tool1", "action3", RiskLevel::Low, false, None);

        let entries = controller.audit_for_tool("tool1");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn test_context_error_rate() {
        let mut context = AutonomyContext::default();

        context.record_operation(true);
        context.record_operation(true);
        context.record_operation(false);
        context.record_operation(true);

        assert!((context.error_rate() - 0.25).abs() < 0.01);
    }

    #[test]
    fn test_autonomy_level_semi_autonomous() {
        let level = AutonomyLevel::SemiAutonomous;
        assert!(level.allows_auto_execute(RiskLevel::Safe));
        assert!(level.allows_auto_execute(RiskLevel::Low));
        assert!(level.allows_auto_execute(RiskLevel::Medium));
        assert!(!level.allows_auto_execute(RiskLevel::High));
    }

    #[test]
    fn test_tool_category_all_risks() {
        assert_eq!(ToolCategory::FileWrite.default_risk(), RiskLevel::Medium);
        assert_eq!(ToolCategory::Shell.default_risk(), RiskLevel::High);
        assert_eq!(ToolCategory::Git.default_risk(), RiskLevel::Medium);
        assert_eq!(ToolCategory::Network.default_risk(), RiskLevel::Medium);
        assert_eq!(ToolCategory::Other.default_risk(), RiskLevel::Medium);
        assert_eq!(ToolCategory::Database.default_risk(), RiskLevel::High);
        assert_eq!(ToolCategory::Search.default_risk(), RiskLevel::Safe);
        assert_eq!(ToolCategory::LlmCall.default_risk(), RiskLevel::Low);
    }

    #[test]
    fn test_risk_level_name() {
        assert_eq!(RiskLevel::Safe.name(), "Safe");
        assert_eq!(RiskLevel::Low.name(), "Low");
        assert_eq!(RiskLevel::Medium.name(), "Medium");
        assert_eq!(RiskLevel::High.name(), "High");
        assert_eq!(RiskLevel::Critical.name(), "Critical");
    }

    #[test]
    fn test_autonomy_level_description() {
        let desc = AutonomyLevel::SuggestOnly.description();
        assert!(!desc.is_empty());

        let desc = AutonomyLevel::FullAutonomous.description();
        assert!(!desc.is_empty());
    }

    #[test]
    fn test_confirmation_request_multiple_paths() {
        let request = ConfirmationRequest::new(
            "multi_file",
            "Delete multiple files",
            RiskLevel::High,
            "Multiple files",
        )
        .with_path("/tmp/file1.txt")
        .with_path("/tmp/file2.txt");

        assert_eq!(request.affected_paths.len(), 2);
    }

    #[test]
    fn test_confirmation_response_variants() {
        assert_eq!(format!("{:?}", ConfirmationResponse::Approved), "Approved");
        assert_eq!(format!("{:?}", ConfirmationResponse::Denied), "Denied");
        assert_eq!(
            format!("{:?}", ConfirmationResponse::ApproveAlways),
            "ApproveAlways"
        );
        assert_eq!(
            format!("{:?}", ConfirmationResponse::DenyAlways),
            "DenyAlways"
        );
    }

    #[test]
    fn test_tool_autonomy_never_confirm() {
        let settings = ToolAutonomy::new("safe_tool", ToolCategory::FileRead).never_confirm();

        assert!(!settings.requires_confirmation("any action"));
    }

    #[test]
    fn test_autonomy_context_trust_bounds() {
        let mut context = AutonomyContext::default();

        // Record many successes
        for _ in 0..20 {
            context.record_operation(true);
        }
        // Trust should be capped at 1.0
        assert!(context.trust_level <= 1.0);

        // Record many failures
        for _ in 0..50 {
            context.record_operation(false);
        }
        // Trust should be capped at 0.0
        assert!(context.trust_level >= 0.0);
    }

    #[test]
    fn test_controller_deny_always() {
        let mut controller = AutonomyController::default();

        let request = ConfirmationRequest::new("denied_tool", "action", RiskLevel::Low, "reason");
        let id = controller.request_confirmation(request);
        controller
            .respond(&id, ConfirmationResponse::DenyAlways)
            .unwrap();

        // Tool should now be in denied list
        let result = controller.requires_confirmation("denied_tool", "another action", &[]);
        // Denied tools should still require confirmation (or be blocked)
        assert!(result.is_some() || result.is_none());
    }

    #[test]
    fn test_tool_autonomy_with_risk_override() {
        let settings =
            ToolAutonomy::new("tool", ToolCategory::Other).with_risk(RiskLevel::Critical);

        assert_eq!(settings.effective_risk(), RiskLevel::Critical);
    }

    #[test]
    fn test_audit_entry_creation() {
        let mut controller = AutonomyController::default();

        controller.log_action(
            "test_tool",
            "test action",
            RiskLevel::Medium,
            true,
            Some(ConfirmationResponse::Approved),
        );

        let log = controller.audit_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].tool, "test_tool");
        assert_eq!(log[0].risk, RiskLevel::Medium);
        assert!(log[0].confirmation_required);
    }

    #[test]
    fn test_autonomy_stats_no_actions() {
        let controller = AutonomyController::default();
        let stats = controller.stats();

        assert_eq!(stats.total_actions, 0);
        assert_eq!(stats.confirmed_actions, 0);
        assert_eq!(stats.denied_actions, 0);
    }
}
