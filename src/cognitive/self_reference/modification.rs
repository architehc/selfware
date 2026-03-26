//! Self-Modification System

use super::types::{ModificationType, ProposedModification, ReferenceConfig, ReferenceLevel, ReferenceState, RiskLevel, SelfReferenceRecord, ValidationResult, ReferenceOperation};
use std::collections::HashSet;

/// Modification engine for self-modification
pub struct ModificationEngine {
    state: ReferenceState,
    config: ReferenceConfig,
    protected_targets: HashSet<String>,
}

impl ModificationEngine {
    pub fn new(state: ReferenceState, config: ReferenceConfig) -> Self {
        let mut protected_targets = HashSet::new();
        protected_targets.insert("core".to_string());
        protected_targets.insert("safety".to_string());
        protected_targets.insert("memory_hierarchy".to_string());

        Self {
            state,
            config,
            protected_targets,
        }
    }

    pub fn with_default_config(state: ReferenceState) -> Self {
        Self::new(state, ReferenceConfig::default())
    }

    /// Propose a modification
    pub async fn propose(
        &self,
        modification_type: ModificationType,
        target: &str,
        description: &str,
        reasoning: &str,
    ) -> anyhow::Result<ProposedModification> {
        if self.protected_targets.contains(target) {
            return Err(anyhow::anyhow!("Target '{}' is protected", target));
        }

        let id = self.state.next_id();
        let risk_level = self.assess_risk(modification_type, target);

        let proposal = ProposedModification {
            id,
            modification_type,
            target: target.to_string(),
            description: description.to_string(),
            reasoning: reasoning.to_string(),
            affected_capabilities: Vec::new(),
            risk_level,
        };

        Ok(proposal)
    }

    /// Validate a proposed modification
    pub async fn validate(&self, proposal: &ProposedModification) -> ValidationResult {
        let mut issues = Vec::new();
        let mut warnings = Vec::new();

        if self.protected_targets.contains(&proposal.target) {
            issues.push(format!("Target '{}' is protected", proposal.target));
        }

        if proposal.risk_level > self.config.risk_threshold {
            issues.push(format!(
                "Risk level {:?} exceeds threshold {:?}",
                proposal.risk_level, self.config.risk_threshold
            ));
        }

        if proposal.affected_capabilities.len() > 5 {
            warnings.push("Many capabilities affected".to_string());
        }

        if proposal.description.len() < 10 {
            warnings.push("Description is very short".to_string());
        }

        ValidationResult {
            valid: issues.is_empty(),
            issues,
            warnings,
        }
    }

    /// Execute a validated modification
    pub async fn execute(&self, proposal: &ProposedModification) -> anyhow::Result<ModificationResult> {
        let validation = self.validate(proposal).await;
        if !validation.valid {
            return Err(anyhow::anyhow!(
                "Validation failed: {:?}",
                validation.issues
            ));
        }

        if self.config.auto_validate && proposal.risk_level >= RiskLevel::High {
            return Err(anyhow::anyhow!("High risk modification requires manual approval"));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let result = ModificationResult {
            id: proposal.id,
            success: true,
            applied_at: timestamp,
            rollback_available: true,
        };

        let record = SelfReferenceRecord {
            id: proposal.id,
            timestamp,
            operation: ReferenceOperation::Propose { modification: proposal.clone() },
            level: ReferenceLevel::Meta,
        };
        self.state.record(record).await;

        Ok(result)
    }

    fn assess_risk(&self, modification_type: ModificationType, target: &str) -> RiskLevel {
        if self.protected_targets.contains(target) {
            return RiskLevel::Critical;
        }

        match modification_type {
            ModificationType::Add => RiskLevel::Low,
            ModificationType::Remove => RiskLevel::High,
            ModificationType::Modify => RiskLevel::Medium,
            ModificationType::Reorder => RiskLevel::Low,
            ModificationType::Conditional => RiskLevel::Medium,
        }
    }

    /// Add a protected target
    pub fn protect(&mut self, target: &str) {
        self.protected_targets.insert(target.to_string());
    }

    /// Remove protection from a target
    pub fn unprotect(&mut self, target: &str) {
        self.protected_targets.remove(target);
    }

    /// Check if a target is protected
    pub fn is_protected(&self, target: &str) -> bool {
        self.protected_targets.contains(target)
    }
}

/// Result of a modification
#[derive(Debug, Clone)]
pub struct ModificationResult {
    pub id: u64,
    pub success: bool,
    pub applied_at: u64,
    pub rollback_available: bool,
}

/// Builder for modifications
pub struct ModificationBuilder {
    modification_type: Option<ModificationType>,
    target: Option<String>,
    description: Option<String>,
    reasoning: Option<String>,
    capabilities: Vec<String>,
}

impl ModificationBuilder {
    pub fn new() -> Self {
        Self {
            modification_type: None,
            target: None,
            description: None,
            reasoning: None,
            capabilities: Vec::new(),
        }
    }

    pub fn modification_type(mut self, t: ModificationType) -> Self {
        self.modification_type = Some(t);
        self
    }

    pub fn target(mut self, target: &str) -> Self {
        self.target = Some(target.to_string());
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = Some(reasoning.to_string());
        self
    }

    pub fn affects(mut self, capability: &str) -> Self {
        self.capabilities.push(capability.to_string());
        self
    }

    pub fn build(self) -> anyhow::Result<(ModificationType, String, String, String, Vec<String>)> {
        let t = self.modification_type.ok_or_else(|| anyhow::anyhow!("Missing modification_type"))?;
        let target = self.target.ok_or_else(|| anyhow::anyhow!("Missing target"))?;
        let desc = self.description.ok_or_else(|| anyhow::anyhow!("Missing description"))?;
        let reasoning = self.reasoning.ok_or_else(|| anyhow::anyhow!("Missing reasoning"))?;
        Ok((t, target, desc, reasoning, self.capabilities))
    }
}

impl Default for ModificationBuilder {
    fn default() -> Self {
        Self::new()
    }
}
