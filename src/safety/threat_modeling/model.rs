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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_asset(id: &str, name: &str) -> Asset {
        Asset {
            id: id.to_string(),
            name: name.to_string(),
            asset_type: AssetType::SourceCode,
            description: "Test asset".to_string(),
            value: 3,
            sensitivity: 3,
            location: None,
            owner: None,
            classification: None,
        }
    }

    fn make_open_threat(
        id: &str,
        category: StrideCategory,
        severity: Severity,
        likelihood: Likelihood,
    ) -> Threat {
        let mut threat = Threat::new(format!("Threat {}", id), category)
            .with_severity(severity)
            .with_likelihood(likelihood);
        threat.id = id.to_string();
        threat
    }

    fn make_control(id: &str, name: &str) -> SecurityControl {
        SecurityControl {
            id: id.to_string(),
            name: name.to_string(),
            control_type: ControlType::Preventive,
            description: "Test control".to_string(),
            status: ControlStatus::Implemented,
            effectiveness: 4,
            mitigates: vec![],
            owner: None,
            notes: None,
        }
    }

    // =========================================================================
    // ThreatModel creation tests
    // =========================================================================

    #[test]
    fn test_new_threat_model() {
        let model = ThreatModel::new("Test Model");
        assert_eq!(model.name, "Test Model");
        assert!(model.description.is_empty());
    }

    #[test]
    fn test_with_description() {
        let model = ThreatModel::new("Test").with_description("A test model");
        assert_eq!(model.description, "A test model");
    }

    // =========================================================================
    // Asset tests
    // =========================================================================

    #[test]
    fn test_add_asset() {
        let mut model = ThreatModel::new("Test");
        let id = model.add_asset(make_asset("asset-1", "User Database"));
        assert_eq!(id, "asset-1");
    }

    #[test]
    fn test_get_asset() {
        let mut model = ThreatModel::new("Test");
        model.add_asset(make_asset("asset-1", "DB"));
        assert!(model.get_asset("asset-1").is_some());
        assert_eq!(model.get_asset("asset-1").unwrap().name, "DB");
    }

    #[test]
    fn test_get_nonexistent_asset() {
        let model = ThreatModel::new("Test");
        assert!(model.get_asset("nonexistent").is_none());
    }

    #[test]
    fn test_assets_iterator() {
        let mut model = ThreatModel::new("Test");
        model.add_asset(make_asset("a1", "Asset 1"));
        model.add_asset(make_asset("a2", "Asset 2"));
        assert_eq!(model.assets().count(), 2);
    }

    // =========================================================================
    // Threat tests
    // =========================================================================

    #[test]
    fn test_add_threat() {
        let mut model = ThreatModel::new("Test");
        let threat = make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        );
        let id = model.add_threat(threat);
        assert_eq!(id, "t1");
    }

    #[test]
    fn test_get_threat() {
        let mut model = ThreatModel::new("Test");
        let threat = make_open_threat(
            "t1",
            StrideCategory::Tampering,
            Severity::Medium,
            Likelihood::Possible,
        );
        model.add_threat(threat);
        assert!(model.get_threat("t1").is_some());
    }

    #[test]
    fn test_get_threat_mut() {
        let mut model = ThreatModel::new("Test");
        let threat = make_open_threat(
            "t1",
            StrideCategory::Tampering,
            Severity::Medium,
            Likelihood::Possible,
        );
        model.add_threat(threat);
        let t = model.get_threat_mut("t1").unwrap();
        t.status = ThreatStatus::Mitigated;
        assert_eq!(
            model.get_threat("t1").unwrap().status,
            ThreatStatus::Mitigated
        );
    }

    #[test]
    fn test_threats_iterator() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::Low,
            Likelihood::Unlikely,
        ));
        model.add_threat(make_open_threat(
            "t2",
            StrideCategory::Tampering,
            Severity::High,
            Likelihood::Likely,
        ));
        assert_eq!(model.threats().count(), 2);
    }

    // =========================================================================
    // Control tests
    // =========================================================================

    #[test]
    fn test_add_control() {
        let mut model = ThreatModel::new("Test");
        let id = model.add_control(make_control("c1", "Input Validation"));
        assert_eq!(id, "c1");
    }

    #[test]
    fn test_get_control() {
        let mut model = ThreatModel::new("Test");
        model.add_control(make_control("c1", "Auth Check"));
        assert!(model.get_control("c1").is_some());
        assert_eq!(model.get_control("c1").unwrap().name, "Auth Check");
    }

    #[test]
    fn test_controls_iterator() {
        let mut model = ThreatModel::new("Test");
        model.add_control(make_control("c1", "C1"));
        model.add_control(make_control("c2", "C2"));
        assert_eq!(model.controls().count(), 2);
    }

    // =========================================================================
    // Entry point and trust boundary tests
    // =========================================================================

    #[test]
    fn test_add_entry_point() {
        let mut model = ThreatModel::new("Test");
        let ep = EntryPoint {
            name: "REST API".to_string(),
            entry_type: EntryPointType::RestApi,
            description: "Main API".to_string(),
            trust_level: TrustLevel::Authenticated,
            threats: vec![],
            data_flows: vec![],
            requires_auth: true,
        };
        model.add_entry_point(ep);
        assert_eq!(model.entry_points().len(), 1);
    }

    #[test]
    fn test_add_trust_boundary() {
        let mut model = ThreatModel::new("Test");
        let tb = TrustBoundary {
            name: "DMZ".to_string(),
            description: "Demilitarized zone".to_string(),
            components: vec!["web-server".to_string()],
            internal_trust: TrustLevel::Authenticated,
            external_trust: TrustLevel::Anonymous,
        };
        model.add_trust_boundary(tb);
        assert_eq!(model.trust_boundaries().len(), 1);
    }

    // =========================================================================
    // Threat filtering tests
    // =========================================================================

    #[test]
    fn test_threats_by_category() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        model.add_threat(make_open_threat(
            "t2",
            StrideCategory::Tampering,
            Severity::Medium,
            Likelihood::Possible,
        ));
        model.add_threat(make_open_threat(
            "t3",
            StrideCategory::Spoofing,
            Severity::Low,
            Likelihood::Unlikely,
        ));

        let spoofing = model.threats_by_category(StrideCategory::Spoofing);
        assert_eq!(spoofing.len(), 2);

        let tampering = model.threats_by_category(StrideCategory::Tampering);
        assert_eq!(tampering.len(), 1);
    }

    #[test]
    fn test_threats_by_status() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        let mut mitigated = make_open_threat(
            "t2",
            StrideCategory::Tampering,
            Severity::Medium,
            Likelihood::Possible,
        );
        mitigated.status = ThreatStatus::Mitigated;
        model.add_threat(mitigated);

        assert_eq!(model.threats_by_status(ThreatStatus::Open).len(), 1);
        assert_eq!(model.threats_by_status(ThreatStatus::Mitigated).len(), 1);
    }

    #[test]
    fn test_open_threats() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        let mut closed = make_open_threat(
            "t2",
            StrideCategory::Tampering,
            Severity::Low,
            Likelihood::Unlikely,
        );
        closed.status = ThreatStatus::Closed;
        model.add_threat(closed);

        assert_eq!(model.open_threats().len(), 1);
    }

    #[test]
    fn test_critical_threats() {
        let mut model = ThreatModel::new("Test");
        // Critical: severity=4 * likelihood=4 = 16 => Critical
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::ElevationOfPrivilege,
            Severity::Critical,
            Likelihood::AlmostCertain,
        ));
        // Low risk
        model.add_threat(make_open_threat(
            "t2",
            StrideCategory::Spoofing,
            Severity::Low,
            Likelihood::Unlikely,
        ));

        let critical = model.critical_threats();
        assert_eq!(critical.len(), 1);
        assert_eq!(critical[0].id, "t1");
    }

    // =========================================================================
    // Risk scoring tests
    // =========================================================================

    #[test]
    fn test_overall_risk_score_no_threats() {
        let model = ThreatModel::new("Test");
        assert_eq!(model.overall_risk_score(), 0.0);
    }

    #[test]
    fn test_overall_risk_score_with_threats() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        let score = model.overall_risk_score();
        assert!(score > 0.0);
    }

    #[test]
    fn test_overall_risk_score_ignores_closed_threats() {
        let mut model = ThreatModel::new("Test");
        let mut closed = make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::Critical,
            Likelihood::AlmostCertain,
        );
        closed.status = ThreatStatus::Closed;
        model.add_threat(closed);
        assert_eq!(model.overall_risk_score(), 0.0);
    }

    // =========================================================================
    // Risk distribution tests
    // =========================================================================

    #[test]
    fn test_risk_distribution_empty() {
        let model = ThreatModel::new("Test");
        let dist = model.risk_distribution();
        assert!(dist.is_empty());
    }

    #[test]
    fn test_risk_distribution_with_threats() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::Critical,
            Likelihood::AlmostCertain,
        ));
        model.add_threat(make_open_threat(
            "t2",
            StrideCategory::Tampering,
            Severity::Low,
            Likelihood::Unlikely,
        ));
        let dist = model.risk_distribution();
        assert!(dist.len() >= 2); // At least 2 different risk levels
    }

    // =========================================================================
    // STRIDE coverage tests
    // =========================================================================

    #[test]
    fn test_stride_coverage_empty() {
        let model = ThreatModel::new("Test");
        let coverage = model.stride_coverage();
        assert_eq!(coverage.len(), 6); // All 6 STRIDE categories
        for count in coverage.values() {
            assert_eq!(*count, 0);
        }
    }

    #[test]
    fn test_stride_coverage_with_threats() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        model.add_threat(make_open_threat(
            "t2",
            StrideCategory::Spoofing,
            Severity::Medium,
            Likelihood::Possible,
        ));
        model.add_threat(make_open_threat(
            "t3",
            StrideCategory::Tampering,
            Severity::Low,
            Likelihood::Unlikely,
        ));
        let coverage = model.stride_coverage();
        assert_eq!(coverage[&StrideCategory::Spoofing], 2);
        assert_eq!(coverage[&StrideCategory::Tampering], 1);
        assert_eq!(coverage[&StrideCategory::Repudiation], 0);
    }

    // =========================================================================
    // Risk matrix tests
    // =========================================================================

    #[test]
    fn test_generate_risk_matrix() {
        let mut model = ThreatModel::new("Test");
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        let matrix = model.generate_risk_matrix();
        let threats = matrix.threats_at(Severity::High, Likelihood::Likely);
        assert_eq!(threats.len(), 1);
    }

    // =========================================================================
    // Report generation tests
    // =========================================================================

    #[test]
    fn test_generate_report_contains_title() {
        let model = ThreatModel::new("My Model");
        let report = model.generate_report();
        assert!(report.contains("My Model"));
    }

    #[test]
    fn test_generate_report_contains_sections() {
        let mut model = ThreatModel::new("Test").with_description("Description");
        model.add_asset(make_asset("a1", "Data"));
        model.add_threat(make_open_threat(
            "t1",
            StrideCategory::Spoofing,
            Severity::High,
            Likelihood::Likely,
        ));
        model.add_control(make_control("c1", "Auth"));
        let report = model.generate_report();
        assert!(report.contains("Executive Summary"));
        assert!(report.contains("Risk Distribution"));
        assert!(report.contains("Assets"));
        assert!(report.contains("STRIDE Category"));
        assert!(report.contains("Security Controls"));
    }

    #[test]
    fn test_generate_report_with_entry_points() {
        let mut model = ThreatModel::new("Test");
        model.add_entry_point(EntryPoint {
            name: "API".to_string(),
            entry_type: EntryPointType::RestApi,
            description: "Main API endpoint".to_string(),
            trust_level: TrustLevel::Authenticated,
            threats: vec![],
            data_flows: vec![],
            requires_auth: true,
        });
        let report = model.generate_report();
        assert!(report.contains("Attack Surface"));
        assert!(report.contains("API"));
    }

    #[test]
    fn test_generate_report_empty_model() {
        let model = ThreatModel::new("Empty");
        let report = model.generate_report();
        assert!(report.contains("Total Threats**: 0"));
        assert!(report.contains("Open Threats**: 0"));
    }
}
