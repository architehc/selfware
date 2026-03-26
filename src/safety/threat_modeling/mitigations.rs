//! Mitigations and Builders
//!
//! Provides builder patterns and mitigations for threat modeling types.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::types::*;

/// Atomic counters for unique IDs
static THREAT_COUNTER: AtomicU64 = AtomicU64::new(0);
static ASSET_COUNTER: AtomicU64 = AtomicU64::new(0);
static CONTROL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate unique threat ID
fn generate_threat_id() -> String {
    format!("threat-{}", THREAT_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique asset ID
fn generate_asset_id() -> String {
    format!("asset-{}", ASSET_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Generate unique control ID
fn generate_control_id() -> String {
    format!("control-{}", CONTROL_COUNTER.fetch_add(1, Ordering::SeqCst))
}

impl Asset {
    /// Create a new asset
    pub fn new(name: impl Into<String>, asset_type: AssetType) -> Self {
        Self {
            id: generate_asset_id(),
            name: name.into(),
            asset_type,
            description: String::new(),
            value: 3,
            sensitivity: 3,
            location: None,
            owner: None,
            classification: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set value
    pub fn with_value(mut self, value: u8) -> Self {
        self.value = value.clamp(1, 5);
        self
    }

    /// Set sensitivity
    pub fn with_sensitivity(mut self, sensitivity: u8) -> Self {
        self.sensitivity = sensitivity.clamp(1, 5);
        self
    }

    /// Set location
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Set owner
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Calculate risk score based on value and sensitivity
    pub fn risk_score(&self) -> u8 {
        (self.value + self.sensitivity) / 2
    }
}

impl Threat {
    /// Create a new threat
    pub fn new(title: impl Into<String>, category: StrideCategory) -> Self {
        Self {
            id: generate_threat_id(),
            title: title.into(),
            category,
            description: String::new(),
            severity: Severity::Medium,
            likelihood: Likelihood::Possible,
            affected_assets: Vec::new(),
            attack_vector: None,
            prerequisites: Vec::new(),
            impact: String::new(),
            mitigations: Vec::new(),
            recommendations: Vec::new(),
            status: ThreatStatus::Open,
            source_file: None,
            source_line: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set severity
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// Set likelihood
    pub fn with_likelihood(mut self, likelihood: Likelihood) -> Self {
        self.likelihood = likelihood;
        self
    }

    /// Add affected asset
    pub fn with_affected_asset(mut self, asset_id: impl Into<String>) -> Self {
        self.affected_assets.push(asset_id.into());
        self
    }

    /// Set attack vector
    pub fn with_attack_vector(mut self, vector: impl Into<String>) -> Self {
        self.attack_vector = Some(vector.into());
        self
    }

    /// Set impact
    pub fn with_impact(mut self, impact: impl Into<String>) -> Self {
        self.impact = impact.into();
        self
    }

    /// Add mitigation
    pub fn with_mitigation(mut self, mitigation: impl Into<String>) -> Self {
        self.mitigations.push(mitigation.into());
        self
    }

    /// Add recommendation
    pub fn with_recommendation(mut self, recommendation: impl Into<String>) -> Self {
        self.recommendations.push(recommendation.into());
        self
    }

    /// Set source location
    pub fn with_source(mut self, file: PathBuf, line: usize) -> Self {
        self.source_file = Some(file);
        self.source_line = Some(line);
        self
    }

    /// Calculate risk score
    pub fn risk_score(&self) -> u8 {
        (self.severity.score() * self.likelihood.score()).min(16)
    }

    /// Get risk level from score
    pub fn risk_level(&self) -> RiskLevel {
        RiskLevel::from_score(self.risk_score())
    }
}

impl SecurityControl {
    /// Create a new security control
    pub fn new(name: impl Into<String>, control_type: ControlType) -> Self {
        Self {
            id: generate_control_id(),
            name: name.into(),
            control_type,
            description: String::new(),
            status: ControlStatus::Planned,
            effectiveness: 3,
            mitigates: Vec::new(),
            owner: None,
            notes: None,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set status
    pub fn with_status(mut self, status: ControlStatus) -> Self {
        self.status = status;
        self
    }

    /// Set effectiveness
    pub fn with_effectiveness(mut self, effectiveness: u8) -> Self {
        self.effectiveness = effectiveness.clamp(1, 5);
        self
    }

    /// Add threat that this control mitigates
    pub fn mitigates_threat(mut self, threat_id: impl Into<String>) -> Self {
        self.mitigates.push(threat_id.into());
        self
    }

    /// Set owner
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }
}

impl EntryPoint {
    /// Create a new entry point
    pub fn new(name: impl Into<String>, entry_type: EntryPointType) -> Self {
        Self {
            name: name.into(),
            entry_type,
            description: String::new(),
            trust_level: TrustLevel::Anonymous,
            threats: Vec::new(),
            data_flows: Vec::new(),
            requires_auth: false,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Set trust level
    pub fn with_trust_level(mut self, level: TrustLevel) -> Self {
        self.trust_level = level;
        self
    }

    /// Add threat
    pub fn with_threat(mut self, threat_id: impl Into<String>) -> Self {
        self.threats.push(threat_id.into());
        self
    }

    /// Set authentication required
    pub fn requires_authentication(mut self) -> Self {
        self.requires_auth = true;
        self
    }
}

impl TrustBoundary {
    /// Create a new trust boundary
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            components: Vec::new(),
            internal_trust: TrustLevel::System,
            external_trust: TrustLevel::Anonymous,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add component
    pub fn with_component(mut self, component: impl Into<String>) -> Self {
        self.components.push(component.into());
        self
    }

    /// Set trust levels
    pub fn with_trust_levels(mut self, internal: TrustLevel, external: TrustLevel) -> Self {
        self.internal_trust = internal;
        self.external_trust = external;
        self
    }
}
