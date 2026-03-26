//! Threat Model
//!
//! The main threat model structure that holds assets, threats, controls, and entry points.

use std::collections::HashMap;

use super::types::*;

/// The threat model
#[derive(Debug)]
pub struct ThreatModel {
    /// Model name
    pub name: String,
    /// Description
    pub description: String,
    /// Assets
    pub(crate) assets: HashMap<String, Asset>,
    /// Threats
    pub(crate) threats: HashMap<String, Threat>,
    /// Controls
    pub(crate) controls: HashMap<String, SecurityControl>,
    /// Entry points
    pub(crate) entry_points: Vec<EntryPoint>,
    /// Trust boundaries
    pub(crate) trust_boundaries: Vec<TrustBoundary>,
}

impl ThreatModel {
    /// Create a new threat model
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            assets: HashMap::new(),
            threats: HashMap::new(),
            controls: HashMap::new(),
            entry_points: Vec::new(),
            trust_boundaries: Vec::new(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    /// Add an asset
    pub fn add_asset(&mut self, asset: Asset) -> String {
        let id = asset.id.clone();
        self.assets.insert(id.clone(), asset);
        id
    }

    /// Get an asset
    pub fn get_asset(&self, id: &str) -> Option<&Asset> {
        self.assets.get(id)
    }

    /// Get all assets
    pub fn assets(&self) -> impl Iterator<Item = &Asset> {
        self.assets.values()
    }

    /// Add a threat
    pub fn add_threat(&mut self, threat: Threat) -> String {
        let id = threat.id.clone();
        self.threats.insert(id.clone(), threat);
        id
    }

    /// Get a threat
    pub fn get_threat(&self, id: &str) -> Option<&Threat> {
        self.threats.get(id)
    }

    /// Get mutable threat
    pub fn get_threat_mut(&mut self, id: &str) -> Option<&mut Threat> {
        self.threats.get_mut(id)
    }

    /// Get all threats
    pub fn threats(&self) -> impl Iterator<Item = &Threat> {
        self.threats.values()
    }

    /// Add a control
    pub fn add_control(&mut self, control: SecurityControl) -> String {
        let id = control.id.clone();
        self.controls.insert(id.clone(), control);
        id
    }

    /// Get a control
    pub fn get_control(&self, id: &str) -> Option<&SecurityControl> {
        self.controls.get(id)
    }

    /// Get all controls
    pub fn controls(&self) -> impl Iterator<Item = &SecurityControl> {
        self.controls.values()
    }

    /// Add an entry point
    pub fn add_entry_point(&mut self, entry_point: EntryPoint) {
        self.entry_points.push(entry_point);
    }

    /// Get all entry points
    pub fn entry_points(&self) -> &[EntryPoint] {
        &self.entry_points
    }

    /// Add a trust boundary
    pub fn add_trust_boundary(&mut self, boundary: TrustBoundary) {
        self.trust_boundaries.push(boundary);
    }

    /// Get all trust boundaries
    pub fn trust_boundaries(&self) -> &[TrustBoundary] {
        &self.trust_boundaries
    }

    /// Get threats by category
    pub fn threats_by_category(&self, category: StrideCategory) -> Vec<&Threat> {
        self.threats
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Get threats by status
    pub fn threats_by_status(&self, status: ThreatStatus) -> Vec<&Threat> {
        self.threats
            .values()
            .filter(|t| t.status == status)
            .collect()
    }

    /// Get open threats
    pub fn open_threats(&self) -> Vec<&Threat> {
        self.threats_by_status(ThreatStatus::Open)
    }

    /// Get critical threats
    pub fn critical_threats(&self) -> Vec<&Threat> {
        self.threats
            .values()
            .filter(|t| t.risk_level() == RiskLevel::Critical && t.status == ThreatStatus::Open)
            .collect()
    }

    /// Calculate overall risk score
    pub fn overall_risk_score(&self) -> f32 {
        let open_threats: Vec<_> = self
            .threats
            .values()
            .filter(|t| t.status == ThreatStatus::Open)
            .collect();

        if open_threats.is_empty() {
            return 0.0;
        }

        let total: u32 = open_threats.iter().map(|t| t.risk_score() as u32).sum();

        total as f32 / open_threats.len() as f32
    }

    /// Get risk distribution
    pub fn risk_distribution(&self) -> HashMap<RiskLevel, usize> {
        let mut dist = HashMap::new();

        for threat in self.threats.values() {
            if threat.status == ThreatStatus::Open {
                *dist.entry(threat.risk_level()).or_insert(0) += 1;
            }
        }

        dist
    }

    /// Get STRIDE coverage
    pub fn stride_coverage(&self) -> HashMap<StrideCategory, usize> {
        let mut coverage = HashMap::new();

        for cat in StrideCategory::all() {
            coverage.insert(cat, 0);
        }

        for threat in self.threats.values() {
            *coverage.entry(threat.category).or_insert(0) += 1;
        }

        coverage
    }

    /// Generate risk matrix
    pub fn generate_risk_matrix(&self) -> RiskMatrix {
        let mut matrix = RiskMatrix::new();

        for threat in self.threats.values() {
            if threat.status == ThreatStatus::Open {
                matrix.add_threat(&threat.id, threat.severity, threat.likelihood);
            }
        }

        matrix
    }

    /// Generate report
    pub fn generate_report(&self) -> String {
        let mut report = String::new();

        report.push_str(&format!("# Threat Model: {}\n\n", self.name));

        if !self.description.is_empty() {
            report.push_str(&format!("{}\n\n", self.description));
        }

        report.push_str("## Executive Summary\n\n");
        report.push_str(&format!("- **Total Threats**: {}\n", self.threats.len()));
        report.push_str(&format!(
            "- **Open Threats**: {}\n",
            self.open_threats().len()
        ));
        report.push_str(&format!(
            "- **Critical Threats**: {}\n",
            self.critical_threats().len()
        ));
        report.push_str(&format!(
            "- **Overall Risk Score**: {:.1}\n",
            self.overall_risk_score()
        ));
        report.push('\n');

        // Risk Distribution
        report.push_str("## Risk Distribution\n\n");
        let dist = self.risk_distribution();
        for level in [
            RiskLevel::Critical,
            RiskLevel::High,
            RiskLevel::Moderate,
            RiskLevel::Low,
        ] {
            let count = dist.get(&level).unwrap_or(&0);
            report.push_str(&format!("- **{}**: {}\n", level, count));
        }
        report.push('\n');

        // Assets
        report.push_str("## Assets\n\n");
        for asset in self.assets.values() {
            report.push_str(&format!("### {}\n\n", asset.name));
            report.push_str(&format!("- **Type**: {}\n", asset.asset_type));
            report.push_str(&format!("- **Value**: {}/5\n", asset.value));
            report.push_str(&format!("- **Sensitivity**: {}/5\n", asset.sensitivity));
            if !asset.description.is_empty() {
                report.push_str(&format!("\n{}\n\n", asset.description));
            }
        }

        // Threats by Category
        report.push_str("## Threats by STRIDE Category\n\n");
        for category in StrideCategory::all() {
            let threats = self.threats_by_category(category);
            if !threats.is_empty() {
                report.push_str(&format!("### {} ({})\n\n", category, threats.len()));
                for threat in threats {
                    let risk = threat.risk_level();
                    report.push_str(&format!(
                        "- **{}** [{}] - {} ({} x {})\n",
                        threat.title, threat.status, risk, threat.severity, threat.likelihood
                    ));
                }
                report.push('\n');
            }
        }

        // Controls
        report.push_str("## Security Controls\n\n");
        for control in self.controls.values() {
            report.push_str(&format!("### {}\n\n", control.name));
            report.push_str(&format!("- **Type**: {}\n", control.control_type));
            report.push_str(&format!("- **Status**: {}\n", control.status));
            report.push_str(&format!(
                "- **Effectiveness**: {}/5\n",
                control.effectiveness
            ));
            if !control.description.is_empty() {
                report.push_str(&format!("\n{}\n\n", control.description));
            }
        }

        // Entry Points
        if !self.entry_points.is_empty() {
            report.push_str("## Attack Surface\n\n");
            for entry in &self.entry_points {
                report.push_str(&format!(
                    "- **{}** ({}) - Trust: {}\n",
                    entry.name, entry.entry_type, entry.trust_level
                ));
            }
            report.push('\n');
        }

        report
    }
}
