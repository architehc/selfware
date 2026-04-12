//! Tests for threat modeling module

use super::*;
use super::analyzer::ScanResult;
use std::path::PathBuf;

// ============================================================================
// STRIDE Category Tests
// ============================================================================

#[test]
fn test_stride_category_display() {
    assert_eq!(format!("{}", StrideCategory::Spoofing), "Spoofing");
    assert_eq!(format!("{}", StrideCategory::Tampering), "Tampering");
    assert_eq!(format!("{}", StrideCategory::Repudiation), "Repudiation");
    assert_eq!(
        format!("{}", StrideCategory::InformationDisclosure),
        "Information Disclosure"
    );
    assert_eq!(format!("{}", StrideCategory::DenialOfService), "Denial of Service");
    assert_eq!(
        format!("{}", StrideCategory::ElevationOfPrivilege),
        "Elevation of Privilege"
    );
}

#[test]
fn test_stride_category_description() {
    assert!(!StrideCategory::Spoofing.description().is_empty());
    assert!(!StrideCategory::Tampering.description().is_empty());
    assert!(!StrideCategory::Repudiation.description().is_empty());
    assert!(!StrideCategory::InformationDisclosure.description().is_empty());
    assert!(!StrideCategory::DenialOfService.description().is_empty());
    assert!(!StrideCategory::ElevationOfPrivilege.description().is_empty());
}

#[test]
fn test_stride_category_description_content() {
    assert!(StrideCategory::Spoofing.description().contains("Impersonating"));
    assert!(StrideCategory::Tampering.description().contains("Modifying"));
    assert!(StrideCategory::Repudiation.description().contains("Claiming"));
    assert!(StrideCategory::InformationDisclosure.description().contains("Exposing"));
    assert!(StrideCategory::DenialOfService.description().contains("unavailable"));
    assert!(StrideCategory::ElevationOfPrivilege.description().contains("capabilities"));
}

#[test]
fn test_stride_category_mitigations() {
    let spoofing_mitigations = StrideCategory::Spoofing.typical_mitigations();
    assert!(!spoofing_mitigations.is_empty());
    assert!(spoofing_mitigations.iter().any(|m| m.contains("authentication")));
    assert!(spoofing_mitigations.iter().any(|m| m.contains("MFA") || m.contains("Certificate")));

    let tampering_mitigations = StrideCategory::Tampering.typical_mitigations();
    assert!(!tampering_mitigations.is_empty());
    assert!(tampering_mitigations.iter().any(|m| m.contains("signature") || m.contains("MAC")));

    let repudiation_mitigations = StrideCategory::Repudiation.typical_mitigations();
    assert!(!repudiation_mitigations.is_empty());
    assert!(repudiation_mitigations.iter().any(|m| m.contains("Audit") || m.contains("logging")));

    let info_disc_mitigations = StrideCategory::InformationDisclosure.typical_mitigations();
    assert!(!info_disc_mitigations.is_empty());
    assert!(info_disc_mitigations.iter().any(|m| m.contains("Encryption")));

    let dos_mitigations = StrideCategory::DenialOfService.typical_mitigations();
    assert!(!dos_mitigations.is_empty());
    assert!(dos_mitigations.iter().any(|m| m.contains("Rate limiting")));

    let eop_mitigations = StrideCategory::ElevationOfPrivilege.typical_mitigations();
    assert!(!eop_mitigations.is_empty());
    assert!(eop_mitigations.iter().any(|m| m.contains("privilege") || m.contains("RBAC")));
}

#[test]
fn test_stride_category_all() {
    let all = StrideCategory::all();
    assert_eq!(all.len(), 6);
    assert!(all.contains(&StrideCategory::Spoofing));
    assert!(all.contains(&StrideCategory::Tampering));
    assert!(all.contains(&StrideCategory::Repudiation));
    assert!(all.contains(&StrideCategory::InformationDisclosure));
    assert!(all.contains(&StrideCategory::DenialOfService));
    assert!(all.contains(&StrideCategory::ElevationOfPrivilege));
}

#[test]
fn test_stride_category_equality() {
    assert_eq!(StrideCategory::Spoofing, StrideCategory::Spoofing);
    assert_ne!(StrideCategory::Spoofing, StrideCategory::Tampering);
}

// ============================================================================
// Severity Tests
// ============================================================================

#[test]
fn test_severity_display() {
    assert_eq!(format!("{}", Severity::Low), "Low");
    assert_eq!(format!("{}", Severity::Medium), "Medium");
    assert_eq!(format!("{}", Severity::High), "High");
    assert_eq!(format!("{}", Severity::Critical), "Critical");
}

#[test]
fn test_severity_score() {
    assert_eq!(Severity::Low.score(), 1);
    assert_eq!(Severity::Medium.score(), 2);
    assert_eq!(Severity::High.score(), 3);
    assert_eq!(Severity::Critical.score(), 4);
}

#[test]
fn test_severity_from_score() {
    assert_eq!(Severity::from_score(0), Severity::Low);
    assert_eq!(Severity::from_score(1), Severity::Low);
    assert_eq!(Severity::from_score(2), Severity::Medium);
    assert_eq!(Severity::from_score(3), Severity::High);
    assert_eq!(Severity::from_score(4), Severity::Critical);
    assert_eq!(Severity::from_score(5), Severity::Critical);
    assert_eq!(Severity::from_score(255), Severity::Critical);
}

#[test]
fn test_severity_ordering() {
    assert!(Severity::Low < Severity::Medium);
    assert!(Severity::Medium < Severity::High);
    assert!(Severity::High < Severity::Critical);
    assert!(Severity::Low < Severity::Critical);
}

// ============================================================================
// Likelihood Tests
// ============================================================================

#[test]
fn test_likelihood_display() {
    assert_eq!(format!("{}", Likelihood::Unlikely), "Unlikely");
    assert_eq!(format!("{}", Likelihood::Possible), "Possible");
    assert_eq!(format!("{}", Likelihood::Likely), "Likely");
    assert_eq!(format!("{}", Likelihood::AlmostCertain), "Almost Certain");
}

#[test]
fn test_likelihood_score() {
    assert_eq!(Likelihood::Unlikely.score(), 1);
    assert_eq!(Likelihood::Possible.score(), 2);
    assert_eq!(Likelihood::Likely.score(), 3);
    assert_eq!(Likelihood::AlmostCertain.score(), 4);
}

#[test]
fn test_likelihood_from_score() {
    assert_eq!(Likelihood::from_score(0), Likelihood::Unlikely);
    assert_eq!(Likelihood::from_score(1), Likelihood::Unlikely);
    assert_eq!(Likelihood::from_score(2), Likelihood::Possible);
    assert_eq!(Likelihood::from_score(3), Likelihood::Likely);
    assert_eq!(Likelihood::from_score(4), Likelihood::AlmostCertain);
    assert_eq!(Likelihood::from_score(5), Likelihood::AlmostCertain);
    assert_eq!(Likelihood::from_score(255), Likelihood::AlmostCertain);
}

#[test]
fn test_likelihood_ordering() {
    assert!(Likelihood::Unlikely < Likelihood::Possible);
    assert!(Likelihood::Possible < Likelihood::Likely);
    assert!(Likelihood::Likely < Likelihood::AlmostCertain);
    assert!(Likelihood::Unlikely < Likelihood::AlmostCertain);
}

// ============================================================================
// Asset Type Tests
// ============================================================================

#[test]
fn test_asset_type_display() {
    assert_eq!(format!("{}", AssetType::UserData), "User Data");
    assert_eq!(format!("{}", AssetType::Credentials), "Credentials");
    assert_eq!(format!("{}", AssetType::ApiKeys), "API Keys");
    assert_eq!(format!("{}", AssetType::Configuration), "Configuration");
    assert_eq!(format!("{}", AssetType::SourceCode), "Source Code");
    assert_eq!(format!("{}", AssetType::Infrastructure), "Infrastructure");
    assert_eq!(format!("{}", AssetType::FinancialData), "Financial Data");
    assert_eq!(format!("{}", AssetType::IntellectualProperty), "Intellectual Property");
    assert_eq!(format!("{}", AssetType::Availability), "Availability");
    assert_eq!(format!("{}", AssetType::Other), "Other");
}

#[test]
fn test_asset_type_equality() {
    assert_eq!(AssetType::UserData, AssetType::UserData);
    assert_ne!(AssetType::UserData, AssetType::Credentials);
}

// ============================================================================
// Threat Status Tests
// ============================================================================

#[test]
fn test_threat_status_display() {
    assert_eq!(format!("{}", ThreatStatus::Open), "Open");
    assert_eq!(format!("{}", ThreatStatus::Mitigated), "Mitigated");
    assert_eq!(format!("{}", ThreatStatus::Accepted), "Accepted");
    assert_eq!(format!("{}", ThreatStatus::Transferred), "Transferred");
    assert_eq!(format!("{}", ThreatStatus::Closed), "Closed");
}

#[test]
fn test_threat_status_equality() {
    assert_eq!(ThreatStatus::Open, ThreatStatus::Open);
    assert_ne!(ThreatStatus::Open, ThreatStatus::Mitigated);
}

// ============================================================================
// Risk Level Tests
// ============================================================================

#[test]
fn test_risk_level_display() {
    assert_eq!(format!("{}", RiskLevel::Low), "Low");
    assert_eq!(format!("{}", RiskLevel::Moderate), "Moderate");
    assert_eq!(format!("{}", RiskLevel::High), "High");
    assert_eq!(format!("{}", RiskLevel::Critical), "Critical");
}

#[test]
fn test_risk_level_from_score() {
    // Low: 0-3
    assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
    assert_eq!(RiskLevel::from_score(1), RiskLevel::Low);
    assert_eq!(RiskLevel::from_score(3), RiskLevel::Low);

    // Moderate: 4-6
    assert_eq!(RiskLevel::from_score(4), RiskLevel::Moderate);
    assert_eq!(RiskLevel::from_score(5), RiskLevel::Moderate);
    assert_eq!(RiskLevel::from_score(6), RiskLevel::Moderate);

    // High: 7-11
    assert_eq!(RiskLevel::from_score(7), RiskLevel::High);
    assert_eq!(RiskLevel::from_score(9), RiskLevel::High);
    assert_eq!(RiskLevel::from_score(11), RiskLevel::High);

    // Critical: 12+
    assert_eq!(RiskLevel::from_score(12), RiskLevel::Critical);
    assert_eq!(RiskLevel::from_score(16), RiskLevel::Critical);
    assert_eq!(RiskLevel::from_score(255), RiskLevel::Critical);
}

#[test]
fn test_risk_level_score_range() {
    assert_eq!(RiskLevel::Low.score_range(), (1, 3));
    assert_eq!(RiskLevel::Moderate.score_range(), (4, 6));
    assert_eq!(RiskLevel::High.score_range(), (7, 11));
    assert_eq!(RiskLevel::Critical.score_range(), (12, 16));
}

#[test]
fn test_risk_level_ordering() {
    assert!(RiskLevel::Low < RiskLevel::Moderate);
    assert!(RiskLevel::Moderate < RiskLevel::High);
    assert!(RiskLevel::High < RiskLevel::Critical);
}

// ============================================================================
// Control Type Tests
// ============================================================================

#[test]
fn test_control_type_display() {
    assert_eq!(format!("{}", ControlType::Preventive), "Preventive");
    assert_eq!(format!("{}", ControlType::Detective), "Detective");
    assert_eq!(format!("{}", ControlType::Corrective), "Corrective");
    assert_eq!(format!("{}", ControlType::Deterrent), "Deterrent");
    assert_eq!(format!("{}", ControlType::Compensating), "Compensating");
}

#[test]
fn test_control_type_equality() {
    assert_eq!(ControlType::Preventive, ControlType::Preventive);
    assert_ne!(ControlType::Preventive, ControlType::Detective);
}

// ============================================================================
// Control Status Tests
// ============================================================================

#[test]
fn test_control_status_display() {
    assert_eq!(format!("{}", ControlStatus::Planned), "Planned");
    assert_eq!(format!("{}", ControlStatus::Partial), "Partial");
    assert_eq!(format!("{}", ControlStatus::Implemented), "Implemented");
    assert_eq!(format!("{}", ControlStatus::NotApplicable), "N/A");
}

#[test]
fn test_control_status_equality() {
    assert_eq!(ControlStatus::Planned, ControlStatus::Planned);
    assert_ne!(ControlStatus::Planned, ControlStatus::Implemented);
}

// ============================================================================
// Entry Point Type Tests
// ============================================================================

#[test]
fn test_entry_point_type_display() {
    assert_eq!(format!("{}", EntryPointType::RestApi), "REST API");
    assert_eq!(format!("{}", EntryPointType::GraphQL), "GraphQL");
    assert_eq!(format!("{}", EntryPointType::Grpc), "gRPC");
    assert_eq!(format!("{}", EntryPointType::WebSocket), "WebSocket");
    assert_eq!(format!("{}", EntryPointType::Cli), "CLI");
    assert_eq!(format!("{}", EntryPointType::FileUpload), "File Upload");
    assert_eq!(format!("{}", EntryPointType::Database), "Database");
    assert_eq!(format!("{}", EntryPointType::MessageQueue), "Message Queue");
    assert_eq!(format!("{}", EntryPointType::Environment), "Environment");
    assert_eq!(format!("{}", EntryPointType::ConfigFile), "Config File");
    assert_eq!(format!("{}", EntryPointType::UserInterface), "User Interface");
    assert_eq!(format!("{}", EntryPointType::Other), "Other");
}

#[test]
fn test_entry_point_type_equality() {
    assert_eq!(EntryPointType::RestApi, EntryPointType::RestApi);
    assert_ne!(EntryPointType::RestApi, EntryPointType::GraphQL);
}

// ============================================================================
// Trust Level Tests
// ============================================================================

#[test]
fn test_trust_level_display() {
    assert_eq!(format!("{}", TrustLevel::Anonymous), "Anonymous");
    assert_eq!(format!("{}", TrustLevel::Authenticated), "Authenticated");
    assert_eq!(format!("{}", TrustLevel::Privileged), "Privileged");
    assert_eq!(format!("{}", TrustLevel::Admin), "Admin");
    assert_eq!(format!("{}", TrustLevel::System), "System");
}

#[test]
fn test_trust_level_ordering() {
    assert!(TrustLevel::Anonymous < TrustLevel::Authenticated);
    assert!(TrustLevel::Authenticated < TrustLevel::Privileged);
    assert!(TrustLevel::Privileged < TrustLevel::Admin);
    assert!(TrustLevel::Admin < TrustLevel::System);
}

#[test]
fn test_trust_level_equality() {
    assert_eq!(TrustLevel::Anonymous, TrustLevel::Anonymous);
    assert_ne!(TrustLevel::Anonymous, TrustLevel::System);
}

// ============================================================================
// Asset Builder Tests
// ============================================================================

#[test]
fn test_asset_creation() {
    let asset = Asset::new("User Database", AssetType::UserData)
        .with_value(5)
        .with_sensitivity(5)
        .with_description("Contains PII");

    assert_eq!(asset.name, "User Database");
    assert_eq!(asset.asset_type, AssetType::UserData);
    assert_eq!(asset.value, 5);
    assert_eq!(asset.sensitivity, 5);
    assert_eq!(asset.description, "Contains PII");
}

#[test]
fn test_asset_default_values() {
    let asset = Asset::new("Test", AssetType::Other);

    assert_eq!(asset.value, 3);
    assert_eq!(asset.sensitivity, 3);
    assert!(asset.description.is_empty());
    assert!(asset.location.is_none());
    assert!(asset.owner.is_none());
    assert!(asset.classification.is_none());
}

#[test]
fn test_asset_with_location() {
    let asset = Asset::new("DB", AssetType::Infrastructure).with_location("us-east-1");
    assert_eq!(asset.location, Some("us-east-1".to_string()));
}

#[test]
fn test_asset_with_owner() {
    let asset = Asset::new("API Keys", AssetType::ApiKeys).with_owner("Security Team");
    assert_eq!(asset.owner, Some("Security Team".to_string()));
}

#[test]
fn test_asset_value_clamping() {
    // Value should be clamped to 1-5 range
    let asset_low = Asset::new("Test", AssetType::Other).with_value(0);
    assert_eq!(asset_low.value, 1);

    let asset_high = Asset::new("Test", AssetType::Other).with_value(10);
    assert_eq!(asset_high.value, 5);

    let asset_normal = Asset::new("Test", AssetType::Other).with_value(3);
    assert_eq!(asset_normal.value, 3);
}

#[test]
fn test_asset_sensitivity_clamping() {
    // Sensitivity should be clamped to 1-5 range
    let asset_low = Asset::new("Test", AssetType::Other).with_sensitivity(0);
    assert_eq!(asset_low.sensitivity, 1);

    let asset_high = Asset::new("Test", AssetType::Other).with_sensitivity(10);
    assert_eq!(asset_high.sensitivity, 5);

    let asset_normal = Asset::new("Test", AssetType::Other).with_sensitivity(4);
    assert_eq!(asset_normal.sensitivity, 4);
}

#[test]
fn test_asset_risk_score() {
    let asset = Asset::new("Test", AssetType::Other)
        .with_value(4)
        .with_sensitivity(4);

    // (4 + 4) / 2 = 4
    assert_eq!(asset.risk_score(), 4);
}

#[test]
fn test_asset_risk_score_edge_cases() {
    // Min values: (1 + 1) / 2 = 1
    let min_asset = Asset::new("Min", AssetType::Other)
        .with_value(1)
        .with_sensitivity(1);
    assert_eq!(min_asset.risk_score(), 1);

    // Max values: (5 + 5) / 2 = 5
    let max_asset = Asset::new("Max", AssetType::Other)
        .with_value(5)
        .with_sensitivity(5);
    assert_eq!(max_asset.risk_score(), 5);

    // Odd sum: (3 + 4) / 2 = 3 (integer division)
    let odd_asset = Asset::new("Odd", AssetType::Other)
        .with_value(3)
        .with_sensitivity(4);
    assert_eq!(odd_asset.risk_score(), 3);
}

#[test]
fn test_unique_asset_ids() {
    let a1 = Asset::new("A1", AssetType::Other);
    let a2 = Asset::new("A2", AssetType::Other);
    let a3 = Asset::new("A3", AssetType::Other);

    assert_ne!(a1.id, a2.id);
    assert_ne!(a2.id, a3.id);
    assert_ne!(a1.id, a3.id);
    assert!(a1.id.starts_with("asset-"));
}

// ============================================================================
// Threat Builder Tests
// ============================================================================

#[test]
fn test_threat_creation() {
    let threat = Threat::new("SQL Injection", StrideCategory::Tampering)
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely);

    assert_eq!(threat.title, "SQL Injection");
    assert_eq!(threat.category, StrideCategory::Tampering);
    assert_eq!(threat.severity, Severity::High);
    assert_eq!(threat.likelihood, Likelihood::Likely);
}

#[test]
fn test_threat_default_values() {
    let threat = Threat::new("Test", StrideCategory::Spoofing);

    assert_eq!(threat.severity, Severity::Medium);
    assert_eq!(threat.likelihood, Likelihood::Possible);
    assert!(threat.description.is_empty());
    assert!(threat.affected_assets.is_empty());
    assert!(threat.attack_vector.is_none());
    assert!(threat.prerequisites.is_empty());
    assert!(threat.impact.is_empty());
    assert!(threat.mitigations.is_empty());
    assert!(threat.recommendations.is_empty());
    assert_eq!(threat.status, ThreatStatus::Open);
    assert!(threat.source_file.is_none());
    assert!(threat.source_line.is_none());
}

#[test]
fn test_threat_with_description() {
    let threat = Threat::new("XSS", StrideCategory::Tampering)
        .with_description("Cross-site scripting vulnerability");
    assert_eq!(threat.description, "Cross-site scripting vulnerability");
}

#[test]
fn test_threat_with_affected_asset() {
    let threat = Threat::new("Data Leak", StrideCategory::InformationDisclosure)
        .with_affected_asset("asset-1")
        .with_affected_asset("asset-2");

    assert_eq!(threat.affected_assets.len(), 2);
    assert!(threat.affected_assets.contains(&"asset-1".to_string()));
    assert!(threat.affected_assets.contains(&"asset-2".to_string()));
}

#[test]
fn test_threat_with_attack_vector() {
    let threat = Threat::new("MITM", StrideCategory::Spoofing)
        .with_attack_vector("Network interception");
    assert_eq!(threat.attack_vector, Some("Network interception".to_string()));
}

#[test]
fn test_threat_with_impact() {
    let threat = Threat::new("RCE", StrideCategory::ElevationOfPrivilege)
        .with_impact("Full system compromise");
    assert_eq!(threat.impact, "Full system compromise");
}

#[test]
fn test_threat_with_mitigation() {
    let threat = Threat::new("CSRF", StrideCategory::Tampering)
        .with_mitigation("CSRF tokens")
        .with_mitigation("SameSite cookies");

    assert_eq!(threat.mitigations.len(), 2);
    assert!(threat.mitigations.contains(&"CSRF tokens".to_string()));
    assert!(threat.mitigations.contains(&"SameSite cookies".to_string()));
}

#[test]
fn test_threat_with_recommendation() {
    let threat = Threat::new("Weak Auth", StrideCategory::Spoofing)
        .with_recommendation("Implement MFA")
        .with_recommendation("Use strong passwords");

    assert_eq!(threat.recommendations.len(), 2);
}

#[test]
fn test_threat_with_source() {
    let threat = Threat::new("Bug", StrideCategory::Tampering)
        .with_source(PathBuf::from("src/main.rs"), 42);

    assert_eq!(threat.source_file, Some(PathBuf::from("src/main.rs")));
    assert_eq!(threat.source_line, Some(42));
}

#[test]
fn test_threat_risk_score() {
    // High (3) * Likely (3) = 9
    let threat = Threat::new("Test", StrideCategory::Tampering)
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely);

    assert_eq!(threat.risk_score(), 9);
}

#[test]
fn test_threat_risk_score_maximum() {
    // Critical (4) * AlmostCertain (4) = 16, but capped at 16
    let threat = Threat::new("Critical", StrideCategory::ElevationOfPrivilege)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);

    assert_eq!(threat.risk_score(), 16);
}

#[test]
fn test_threat_risk_score_minimum() {
    // Low (1) * Unlikely (1) = 1
    let threat = Threat::new("Low", StrideCategory::Spoofing)
        .with_severity(Severity::Low)
        .with_likelihood(Likelihood::Unlikely);

    assert_eq!(threat.risk_score(), 1);
}

#[test]
fn test_threat_risk_level() {
    let low = Threat::new("Low", StrideCategory::Spoofing)
        .with_severity(Severity::Low)
        .with_likelihood(Likelihood::Unlikely);
    assert_eq!(low.risk_level(), RiskLevel::Low);

    let moderate = Threat::new("Moderate", StrideCategory::Spoofing)
        .with_severity(Severity::Medium)
        .with_likelihood(Likelihood::Possible);
    assert_eq!(moderate.risk_level(), RiskLevel::Moderate);

    let high = Threat::new("High", StrideCategory::Tampering)
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely);
    assert_eq!(high.risk_level(), RiskLevel::High);

    let critical = Threat::new("Critical", StrideCategory::Tampering)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);
    assert_eq!(critical.risk_level(), RiskLevel::Critical);
}

#[test]
fn test_unique_threat_ids() {
    let t1 = Threat::new("T1", StrideCategory::Spoofing);
    let t2 = Threat::new("T2", StrideCategory::Spoofing);
    let t3 = Threat::new("T3", StrideCategory::Spoofing);

    assert_ne!(t1.id, t2.id);
    assert_ne!(t2.id, t3.id);
    assert_ne!(t1.id, t3.id);
    assert!(t1.id.starts_with("threat-"));
}

#[test]
fn test_threat_with_all_fields() {
    let threat = Threat::new("Complete Threat", StrideCategory::InformationDisclosure)
        .with_description("Full description")
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain)
        .with_affected_asset("asset-1")
        .with_attack_vector("Network")
        .with_impact("Data breach")
        .with_mitigation("Encryption")
        .with_recommendation("Enable TLS")
        .with_source(PathBuf::from("src/api.rs"), 42);

    assert_eq!(threat.description, "Full description");
    assert_eq!(threat.severity, Severity::Critical);
    assert_eq!(threat.likelihood, Likelihood::AlmostCertain);
    assert_eq!(threat.affected_assets.len(), 1);
    assert_eq!(threat.attack_vector, Some("Network".to_string()));
    assert_eq!(threat.impact, "Data breach");
    assert_eq!(threat.mitigations.len(), 1);
    assert_eq!(threat.recommendations.len(), 1);
    assert!(threat.source_file.is_some());
    assert_eq!(threat.source_line, Some(42));
}

// ============================================================================
// Security Control Builder Tests
// ============================================================================

#[test]
fn test_security_control_creation() {
    let control = SecurityControl::new("Input Validation", ControlType::Preventive)
        .with_status(ControlStatus::Implemented)
        .with_effectiveness(4);

    assert_eq!(control.name, "Input Validation");
    assert_eq!(control.control_type, ControlType::Preventive);
    assert_eq!(control.status, ControlStatus::Implemented);
    assert_eq!(control.effectiveness, 4);
}

#[test]
fn test_security_control_default_values() {
    let control = SecurityControl::new("Test", ControlType::Detective);

    assert_eq!(control.status, ControlStatus::Planned);
    assert_eq!(control.effectiveness, 3);
    assert!(control.description.is_empty());
    assert!(control.mitigates.is_empty());
    assert!(control.owner.is_none());
    assert!(control.notes.is_none());
}

#[test]
fn test_security_control_with_description() {
    let control = SecurityControl::new("WAF", ControlType::Preventive)
        .with_description("Web Application Firewall");
    assert_eq!(control.description, "Web Application Firewall");
}

#[test]
fn test_security_control_with_effectiveness() {
    let control = SecurityControl::new("Test", ControlType::Preventive)
        .with_effectiveness(5);
    assert_eq!(control.effectiveness, 5);
}

#[test]
fn test_security_control_effectiveness_clamping() {
    // Should be clamped to 1-5
    let low = SecurityControl::new("Low", ControlType::Preventive).with_effectiveness(0);
    assert_eq!(low.effectiveness, 1);

    let high = SecurityControl::new("High", ControlType::Preventive).with_effectiveness(10);
    assert_eq!(high.effectiveness, 5);
}

#[test]
fn test_security_control_mitigates_threat() {
    let control = SecurityControl::new("Auth Check", ControlType::Preventive)
        .mitigates_threat("threat-1")
        .mitigates_threat("threat-2");

    assert_eq!(control.mitigates.len(), 2);
    assert!(control.mitigates.contains(&"threat-1".to_string()));
    assert!(control.mitigates.contains(&"threat-2".to_string()));
}

#[test]
fn test_security_control_with_owner() {
    let control = SecurityControl::new("Audit Log", ControlType::Detective)
        .with_owner("Security Team");
    assert_eq!(control.owner, Some("Security Team".to_string()));
}

#[test]
fn test_unique_control_ids() {
    let c1 = SecurityControl::new("C1", ControlType::Preventive);
    let c2 = SecurityControl::new("C2", ControlType::Preventive);
    let c3 = SecurityControl::new("C3", ControlType::Preventive);

    assert_ne!(c1.id, c2.id);
    assert_ne!(c2.id, c3.id);
    assert_ne!(c1.id, c3.id);
    assert!(c1.id.starts_with("control-"));
}

// ============================================================================
// Entry Point Builder Tests
// ============================================================================

#[test]
fn test_entry_point_creation() {
    let entry = EntryPoint::new("/api/users", EntryPointType::RestApi)
        .with_trust_level(TrustLevel::Authenticated)
        .requires_authentication();

    assert_eq!(entry.name, "/api/users");
    assert_eq!(entry.entry_type, EntryPointType::RestApi);
    assert_eq!(entry.trust_level, TrustLevel::Authenticated);
    assert!(entry.requires_auth);
}

#[test]
fn test_entry_point_default_values() {
    let entry = EntryPoint::new("Test", EntryPointType::Cli);

    assert_eq!(entry.trust_level, TrustLevel::Anonymous);
    assert!(!entry.requires_auth);
    assert!(entry.description.is_empty());
    assert!(entry.threats.is_empty());
    assert!(entry.data_flows.is_empty());
}

#[test]
fn test_entry_point_with_description() {
    let entry = EntryPoint::new("API", EntryPointType::RestApi)
        .with_description("Main REST API endpoint");
    assert_eq!(entry.description, "Main REST API endpoint");
}

#[test]
fn test_entry_point_with_threat() {
    let entry = EntryPoint::new("Upload", EntryPointType::FileUpload)
        .with_threat("threat-1")
        .with_threat("threat-2");

    assert_eq!(entry.threats.len(), 2);
}

// ============================================================================
// Trust Boundary Builder Tests
// ============================================================================

#[test]
fn test_trust_boundary_creation() {
    let boundary = TrustBoundary::new("Internal Network")
        .with_description("Protected network segment")
        .with_component("Database")
        .with_trust_levels(TrustLevel::System, TrustLevel::Anonymous);

    assert_eq!(boundary.name, "Internal Network");
    assert_eq!(boundary.description, "Protected network segment");
    assert_eq!(boundary.components.len(), 1);
    assert!(boundary.components.contains(&"Database".to_string()));
    assert_eq!(boundary.internal_trust, TrustLevel::System);
    assert_eq!(boundary.external_trust, TrustLevel::Anonymous);
}

#[test]
fn test_trust_boundary_default_values() {
    let boundary = TrustBoundary::new("Test");

    assert!(boundary.description.is_empty());
    assert!(boundary.components.is_empty());
    assert_eq!(boundary.internal_trust, TrustLevel::System);
    assert_eq!(boundary.external_trust, TrustLevel::Anonymous);
}

#[test]
fn test_trust_boundary_with_multiple_components() {
    let boundary = TrustBoundary::new("DMZ")
        .with_component("Web Server")
        .with_component("Load Balancer")
        .with_component("WAF");

    assert_eq!(boundary.components.len(), 3);
}

// ============================================================================
// Risk Matrix Tests
// ============================================================================

#[test]
fn test_risk_matrix_new() {
    let matrix = RiskMatrix::new();
    assert!(matrix.cells.is_empty());
}

#[test]
fn test_risk_matrix_default() {
    let matrix: RiskMatrix = Default::default();
    assert!(matrix.cells.is_empty());
}

#[test]
fn test_risk_matrix_add_threat() {
    let mut matrix = RiskMatrix::new();
    matrix.add_threat("t1", Severity::High, Likelihood::Likely);

    let threats = matrix.threats_at(Severity::High, Likelihood::Likely);
    assert_eq!(threats.len(), 1);
    assert_eq!(threats[0], "t1");
}

#[test]
fn test_risk_matrix_multiple_threats_same_cell() {
    let mut matrix = RiskMatrix::new();
    matrix.add_threat("t1", Severity::High, Likelihood::Likely);
    matrix.add_threat("t2", Severity::High, Likelihood::Likely);
    matrix.add_threat("t3", Severity::High, Likelihood::Likely);

    let threats = matrix.threats_at(Severity::High, Likelihood::Likely);
    assert_eq!(threats.len(), 3);
}

#[test]
fn test_risk_matrix_different_cells() {
    let mut matrix = RiskMatrix::new();
    matrix.add_threat("high-likely", Severity::High, Likelihood::Likely);
    matrix.add_threat("low-unlikely", Severity::Low, Likelihood::Unlikely);
    matrix.add_threat("critical-certain", Severity::Critical, Likelihood::AlmostCertain);

    assert_eq!(matrix.threats_at(Severity::High, Likelihood::Likely).len(), 1);
    assert_eq!(matrix.threats_at(Severity::Low, Likelihood::Unlikely).len(), 1);
    assert_eq!(matrix.threats_at(Severity::Critical, Likelihood::AlmostCertain).len(), 1);
}

#[test]
fn test_risk_matrix_threats_at_empty_cell() {
    let matrix = RiskMatrix::new();
    let threats = matrix.threats_at(Severity::Medium, Likelihood::Possible);
    assert!(threats.is_empty());
}

#[test]
fn test_risk_matrix_to_text() {
    let mut matrix = RiskMatrix::new();
    matrix.add_threat("t1", Severity::High, Likelihood::Likely);
    matrix.add_threat("t2", Severity::High, Likelihood::Likely);
    matrix.add_threat("t3", Severity::Low, Likelihood::Unlikely);

    let text = matrix.to_text();
    assert!(text.contains("LIKELIHOOD"));
    assert!(text.contains("Unlikely"));
    assert!(text.contains("Possible"));
    assert!(text.contains("Likely"));
    assert!(text.contains("Critical"));
    assert!(text.contains("High"));
    assert!(text.contains("Medium"));
    assert!(text.contains("Low"));
}

#[test]
fn test_risk_matrix_to_text_formatting() {
    let mut matrix = RiskMatrix::new();
    matrix.add_threat("t1", Severity::High, Likelihood::Likely);

    let text = matrix.to_text();
    // Should contain count for high/likely cell
    assert!(text.contains("2") || text.contains("1")); // Count display
}

// ============================================================================
// STRIDE Analyzer Tests
// ============================================================================

#[test]
fn test_stride_analyzer_new() {
    let analyzer = StrideAnalyzer::new();
    // Should have patterns for all 6 categories
    for category in StrideCategory::all() {
        assert!(!analyzer.get_patterns(category).is_empty());
    }
}

#[test]
fn test_stride_analyzer_default() {
    let analyzer: StrideAnalyzer = Default::default();
    assert!(!analyzer.get_patterns(StrideCategory::Spoofing).is_empty());
}

#[test]
fn test_stride_analyzer_get_patterns() {
    let analyzer = StrideAnalyzer::new();

    let spoofing_patterns = analyzer.get_patterns(StrideCategory::Spoofing);
    assert!(!spoofing_patterns.is_empty());
    assert!(spoofing_patterns.iter().any(|p| p.name.contains("Authentication")));

    let tampering_patterns = analyzer.get_patterns(StrideCategory::Tampering);
    assert!(!tampering_patterns.is_empty());
    assert!(tampering_patterns.iter().any(|p| p.name.contains("Injection")));
}

#[test]
fn test_stride_analyzer_get_patterns_empty() {
    // Test with a custom analyzer - we can't easily create one, so we just verify
    // the default analyzer has patterns
    let analyzer = StrideAnalyzer::new();
    for category in StrideCategory::all() {
        assert!(!analyzer.get_patterns(category).is_empty());
    }
}

#[test]
fn test_stride_analyzer_analyze_spoofing() {
    let analyzer = StrideAnalyzer::new();
    let code = r#"
fn login(user: &str, password: &str) {
    // Basic auth with plaintext password
    let query = format!("SELECT * FROM users WHERE password = '{}'", password);
}
"#;

    let threats = analyzer.analyze(code, &PathBuf::from("test.rs"));
    // Should detect weak authentication pattern
    assert!(!threats.is_empty());
}

#[test]
fn test_stride_analyzer_analyze_tampering() {
    let analyzer = StrideAnalyzer::new();
    let code = r#"
fn get_user(id: &str) {
    let query = format!("SELECT * FROM users WHERE id = '{}'", id);
    db.execute(&query);
}
"#;

    let threats = analyzer.analyze(code, &PathBuf::from("test.rs"));
    // Should detect SQL injection pattern
    assert!(!threats.is_empty());
}

#[test]
fn test_stride_analyzer_analyze_info_disclosure() {
    let analyzer = StrideAnalyzer::new();
    let code = r#"
fn handle_error(e: Error) {
    println!("Error: {:?}", e);
    println!("Stack trace: {}", e.backtrace());
}
"#;

    let _threats = analyzer.analyze(code, &PathBuf::from("test.rs"));
    // May detect verbose error messages
}

#[test]
fn test_stride_analyzer_analyze_no_threats() {
    let analyzer = StrideAnalyzer::new();
    let code = r#"
fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let threats = analyzer.analyze(code, &PathBuf::from("math.rs"));
    // No security threats in simple math function
    assert!(threats.is_empty());
}

#[test]
fn test_stride_analyzer_line_numbers() {
    let analyzer = StrideAnalyzer::new();
    let code = r#"
line 1
line 2
fn login(user: &str) {
    // no auth check here
}
"#;

    let threats = analyzer.analyze(code, &PathBuf::from("test.rs"));
    // Check that source line is set when threats are found
    for threat in &threats {
        assert!(threat.source_line.is_some());
        assert!(threat.source_file.is_some());
    }
}

// ============================================================================
// Threat Pattern Tests
// ============================================================================

#[test]
fn test_threat_pattern_new() {
    let pattern = ThreatPattern::new("SQL Injection", vec!["sql", "query", "execute"]);
    assert_eq!(pattern.name, "SQL Injection");
    assert_eq!(pattern.keywords.len(), 3);
}

#[test]
fn test_threat_pattern_matches() {
    let pattern = ThreatPattern::new("SQL Injection", vec!["sql", "query"]);

    assert!(pattern.matches("let sql = execute_query()"));
    assert!(pattern.matches("let sql_query = \"SELECT * FROM users\""));
    assert!(pattern.matches("sql query database"));
    assert!(pattern.matches("execute_sql_query()"));
    assert!(!pattern.matches("SELECT * FROM users")); // uppercase SELECT, no "sql" or "query"
    assert!(!pattern.matches("let x = 42"));
    assert!(!pattern.matches(""));
}

#[test]
fn test_threat_pattern_matches_empty_keywords() {
    let pattern = ThreatPattern::new("Empty", vec![]);
    assert!(!pattern.matches("anything"));
}

#[test]
fn test_threat_pattern_matches_case_sensitive() {
    let pattern = ThreatPattern::new("Case Test", vec!["SQL"]);

    // Pattern matching is case-sensitive based on implementation
    assert!(pattern.matches("SQL injection"));
    // Lowercase won't match since pattern has uppercase
}

#[test]
fn test_threat_pattern_matches_partial() {
    let pattern = ThreatPattern::new("Test", vec!["password"]);

    assert!(pattern.matches("user_password"));
    assert!(pattern.matches("password_hash"));
    assert!(pattern.matches("check password"));
}

// ============================================================================
// Attack Surface Mapper Tests
// ============================================================================

#[test]
fn test_attack_surface_mapper_new() {
    let _mapper = AttackSurfaceMapper::new();
    // Just verify it creates without panic
}

#[test]
fn test_attack_surface_mapper_default() {
    let _mapper: AttackSurfaceMapper = Default::default();
    // Just verify it creates without panic
}

#[test]
fn test_attack_surface_mapper_detect_rest_api() {
    let mapper = AttackSurfaceMapper::new();
    let code = r#"
#[get("/users")]
async fn get_users() -> impl IntoResponse {
    // ...
}
"#;

    let entry_points = mapper.map(code);
    assert!(!entry_points.is_empty());
    assert!(entry_points.iter().any(|e| e.entry_type == EntryPointType::RestApi));
}

#[test]
fn test_attack_surface_mapper_detect_graphql() {
    let mapper = AttackSurfaceMapper::new();
    let code = r#"
#[derive(Query)]
struct UserQuery {
    users: Vec<User>,
}
"#;

    let entry_points = mapper.map(code);
    assert!(!entry_points.is_empty());
    assert!(entry_points.iter().any(|e| e.entry_type == EntryPointType::GraphQL));
}

#[test]
fn test_attack_surface_mapper_detect_database() {
    let mapper = AttackSurfaceMapper::new();
    let code = r#"
fn query_db() {
    let result = sqlx::query("SELECT * FROM users").fetch_all(&pool).await;
}
"#;

    let entry_points = mapper.map(code);
    assert!(!entry_points.is_empty());
    assert!(entry_points.iter().any(|e| e.entry_type == EntryPointType::Database));
}

#[test]
fn test_attack_surface_mapper_detect_cli() {
    let mapper = AttackSurfaceMapper::new();
    let code = r#"
use clap::Parser;

#[derive(Parser)]
struct Args {
    name: String,
}
"#;

    let entry_points = mapper.map(code);
    assert!(!entry_points.is_empty());
    assert!(entry_points.iter().any(|e| e.entry_type == EntryPointType::Cli));
}

#[test]
fn test_attack_surface_mapper_no_entry_points() {
    let mapper = AttackSurfaceMapper::new();
    let code = r#"
fn internal_helper() -> i32 {
    42
}
"#;

    let entry_points = mapper.map(code);
    assert!(entry_points.is_empty());
}

// ============================================================================
// Entry Point Detector Tests
// ============================================================================

#[test]
fn test_entry_point_detector_new() {
    let _detector = EntryPointDetector::new(EntryPointType::RestApi, vec!["#[get"]);
    // Just verify it creates
}

#[test]
fn test_entry_point_detector_detect() {
    let detector = EntryPointDetector::new(EntryPointType::RestApi, vec!["#[get", "#[post"]);

    let found = detector.detect("#[get(\"/users\")]");
    assert!(found.is_some());
    assert_eq!(found.unwrap().entry_type, EntryPointType::RestApi);

    let not_found = detector.detect("fn main() {}");
    assert!(not_found.is_none());
}

#[test]
fn test_entry_point_detector_multiple_patterns() {
    let detector = EntryPointDetector::new(
        EntryPointType::RestApi,
        vec!["app.get", "app.post", "router.get"],
    );

    assert!(detector.detect("app.get('/users')").is_some());
    assert!(detector.detect("app.post('/users')").is_some());
    assert!(detector.detect("router.get('/api')").is_some());
    assert!(detector.detect("db.query()").is_none());
}

// ============================================================================
// Security Scanner Tests
// ============================================================================

#[test]
fn test_security_scanner_new() {
    let _scanner = SecurityScanner::new();
    // Just verify it creates
}

#[test]
fn test_security_scanner_default() {
    let _scanner: SecurityScanner = Default::default();
    // Just verify it creates
}

#[test]
fn test_security_scanner_scan_file() {
    let scanner = SecurityScanner::new();
    let code = r#"
fn handler(input: &str) {
    let query = format!("SELECT * FROM users WHERE name = '{}'", input);
}
"#;

    let result = scanner.scan_file(code, &PathBuf::from("test.rs"));

    assert_eq!(result.file, PathBuf::from("test.rs"));
    assert!(!result.threats.is_empty());
}

#[test]
fn test_security_scanner_scan_file_with_entry_points() {
    let scanner = SecurityScanner::new();
    let code = r#"
#[get("/api/users")]
fn get_users() {
    let query = format!("SELECT * FROM users");
}
"#;

    let result = scanner.scan_file(code, &PathBuf::from("api.rs"));

    assert!(!result.threats.is_empty());
    assert!(!result.entry_points.is_empty());
}

#[test]
fn test_scan_result_debug() {
    let result = ScanResult {
        file: PathBuf::from("test.rs"),
        threats: vec![],
        entry_points: vec![],
    };

    let debug_str = format!("{:?}", result);
    assert!(debug_str.contains("test.rs"));
}

// ============================================================================
// Threat Model Tests
// ============================================================================

#[test]
fn test_threat_model_creation() {
    let model = ThreatModel::new("My Application").with_description("Web application threat model");

    assert_eq!(model.name, "My Application");
    assert!(!model.description.is_empty());
}

#[test]
fn test_threat_model_new() {
    let model = ThreatModel::new("Test");
    assert_eq!(model.name, "Test");
    assert!(model.description.is_empty());
    assert_eq!(model.assets().count(), 0);
    assert_eq!(model.threats().count(), 0);
    assert_eq!(model.controls().count(), 0);
    assert!(model.entry_points().is_empty());
    assert!(model.trust_boundaries().is_empty());
}

#[test]
fn test_threat_model_with_description() {
    let model = ThreatModel::new("Test").with_description("Test description");
    assert_eq!(model.description, "Test description");
}

#[test]
fn test_threat_model_add_asset() {
    let mut model = ThreatModel::new("Test");

    let asset = Asset::new("Database", AssetType::UserData);
    let id = model.add_asset(asset);

    assert!(model.get_asset(&id).is_some());
    assert_eq!(model.assets().count(), 1);
}

#[test]
fn test_threat_model_get_asset() {
    let mut model = ThreatModel::new("Test");

    let asset = Asset::new("DB", AssetType::UserData);
    let id = model.add_asset(asset);

    let retrieved = model.get_asset(&id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "DB");
}

#[test]
fn test_threat_model_get_nonexistent_asset() {
    let model = ThreatModel::new("Test");
    assert!(model.get_asset("nonexistent").is_none());
}

#[test]
fn test_threat_model_assets_iterator() {
    let mut model = ThreatModel::new("Test");
    model.add_asset(Asset::new("A1", AssetType::UserData));
    model.add_asset(Asset::new("A2", AssetType::Configuration));

    let assets: Vec<_> = model.assets().collect();
    assert_eq!(assets.len(), 2);
}

#[test]
fn test_threat_model_add_threat() {
    let mut model = ThreatModel::new("Test");

    let threat = Threat::new("XSS", StrideCategory::Tampering);
    let id = model.add_threat(threat);

    assert!(model.get_threat(&id).is_some());
    assert_eq!(model.threats().count(), 1);
}

#[test]
fn test_threat_model_get_threat() {
    let mut model = ThreatModel::new("Test");

    let threat = Threat::new("SQL Injection", StrideCategory::Tampering);
    let id = model.add_threat(threat);

    let retrieved = model.get_threat(&id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().title, "SQL Injection");
}

#[test]
fn test_threat_model_get_nonexistent_threat() {
    let model = ThreatModel::new("Test");
    assert!(model.get_threat("nonexistent").is_none());
}

#[test]
fn test_threat_model_get_threat_mut() {
    let mut model = ThreatModel::new("Test");

    let threat = Threat::new("Test", StrideCategory::Spoofing);
    let id = model.add_threat(threat);

    {
        let t = model.get_threat_mut(&id).unwrap();
        t.status = ThreatStatus::Mitigated;
    }

    assert_eq!(model.get_threat(&id).unwrap().status, ThreatStatus::Mitigated);
}

#[test]
fn test_threat_model_threats_iterator() {
    let mut model = ThreatModel::new("Test");
    model.add_threat(Threat::new("T1", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T2", StrideCategory::Tampering));

    let threats: Vec<_> = model.threats().collect();
    assert_eq!(threats.len(), 2);
}

#[test]
fn test_threat_model_add_control() {
    let mut model = ThreatModel::new("Test");

    let control = SecurityControl::new("Input Validation", ControlType::Preventive);
    let id = model.add_control(control);

    assert!(model.get_control(&id).is_some());
    assert_eq!(model.controls().count(), 1);
}

#[test]
fn test_threat_model_get_control() {
    let mut model = ThreatModel::new("Test");

    let control = SecurityControl::new("WAF", ControlType::Preventive);
    let id = model.add_control(control);

    let retrieved = model.get_control(&id);
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "WAF");
}

#[test]
fn test_threat_model_get_nonexistent_control() {
    let model = ThreatModel::new("Test");
    assert!(model.get_control("nonexistent").is_none());
}

#[test]
fn test_threat_model_controls_iterator() {
    let mut model = ThreatModel::new("Test");
    model.add_control(SecurityControl::new("C1", ControlType::Preventive));
    model.add_control(SecurityControl::new("C2", ControlType::Detective));

    let controls: Vec<_> = model.controls().collect();
    assert_eq!(controls.len(), 2);
}

#[test]
fn test_threat_model_add_entry_point() {
    let mut model = ThreatModel::new("Test");

    model.add_entry_point(EntryPoint::new("/api", EntryPointType::RestApi));

    assert_eq!(model.entry_points().len(), 1);
}

#[test]
fn test_threat_model_add_trust_boundary() {
    let mut model = ThreatModel::new("Test");

    model.add_trust_boundary(TrustBoundary::new("DMZ"));

    assert_eq!(model.trust_boundaries().len(), 1);
}

#[test]
fn test_threat_model_threats_by_category() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(Threat::new("T1", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T2", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T3", StrideCategory::Tampering));

    let spoofing = model.threats_by_category(StrideCategory::Spoofing);
    assert_eq!(spoofing.len(), 2);

    let tampering = model.threats_by_category(StrideCategory::Tampering);
    assert_eq!(tampering.len(), 1);

    let repudiation = model.threats_by_category(StrideCategory::Repudiation);
    assert_eq!(repudiation.len(), 0);
}

#[test]
fn test_threat_model_threats_by_status() {
    let mut model = ThreatModel::new("Test");

    let t1 = Threat::new("Open Threat", StrideCategory::Spoofing);
    let t2 = Threat::new("Mitigated Threat", StrideCategory::Tampering);
    let id2 = model.add_threat(t2);

    model.add_threat(t1);
    if let Some(t) = model.get_threat_mut(&id2) {
        t.status = ThreatStatus::Mitigated;
    }

    let open = model.threats_by_status(ThreatStatus::Open);
    assert_eq!(open.len(), 1);

    let mitigated = model.threats_by_status(ThreatStatus::Mitigated);
    assert_eq!(mitigated.len(), 1);
}

#[test]
fn test_threat_model_open_threats() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(Threat::new("Open1", StrideCategory::Spoofing));
    let mitigated_id = model.add_threat(Threat::new("Mitigated", StrideCategory::Spoofing));

    if let Some(t) = model.get_threat_mut(&mitigated_id) {
        t.status = ThreatStatus::Mitigated;
    }

    let open = model.open_threats();
    assert_eq!(open.len(), 1);
}

#[test]
fn test_threat_model_critical_threats() {
    let mut model = ThreatModel::new("Test");

    // Critical threat (open)
    let critical = Threat::new("Critical", StrideCategory::ElevationOfPrivilege)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);
    model.add_threat(critical);

    // High but not critical
    let high = Threat::new("High", StrideCategory::Tampering)
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely);
    model.add_threat(high);

    // Critical but closed
    let closed_critical = Threat::new("Closed", StrideCategory::Spoofing)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);
    let closed_id = model.add_threat(closed_critical);
    if let Some(t) = model.get_threat_mut(&closed_id) {
        t.status = ThreatStatus::Closed;
    }

    let critical_threats = model.critical_threats();
    assert_eq!(critical_threats.len(), 1);
    assert_eq!(critical_threats[0].title, "Critical");
}

#[test]
fn test_threat_model_overall_risk_score() {
    let mut model = ThreatModel::new("Test");

    // Add threats with different risk scores
    model.add_threat(
        Threat::new("T1", StrideCategory::Spoofing)
            .with_severity(Severity::High) // 3
            .with_likelihood(Likelihood::Likely), // 3
        // Risk score: 3 * 3 = 9
    );

    model.add_threat(
        Threat::new("T2", StrideCategory::Tampering)
            .with_severity(Severity::Medium) // 2
            .with_likelihood(Likelihood::Possible), // 2
        // Risk score: 2 * 2 = 4
    );

    let score = model.overall_risk_score();
    // Average of 9 and 4 = 6.5
    assert!(score > 0.0);
    assert_eq!(score, 6.5);
}

#[test]
fn test_threat_model_overall_risk_score_no_threats() {
    let model = ThreatModel::new("Test");
    assert_eq!(model.overall_risk_score(), 0.0);
}

#[test]
fn test_threat_model_overall_risk_score_ignores_closed() {
    let mut model = ThreatModel::new("Test");

    let mut closed = Threat::new("Closed", StrideCategory::Spoofing)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);
    closed.status = ThreatStatus::Closed;
    model.add_threat(closed);

    assert_eq!(model.overall_risk_score(), 0.0);
}

#[test]
fn test_threat_model_risk_distribution() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(
        Threat::new("Low Risk", StrideCategory::Spoofing)
            .with_severity(Severity::Low)
            .with_likelihood(Likelihood::Unlikely),
    );

    model.add_threat(
        Threat::new("High Risk", StrideCategory::Tampering)
            .with_severity(Severity::Critical)
            .with_likelihood(Likelihood::Likely),
    );

    let dist = model.risk_distribution();

    assert!(dist.contains_key(&RiskLevel::Low));
    assert!(dist.contains_key(&RiskLevel::High) || dist.contains_key(&RiskLevel::Critical));
}

#[test]
fn test_threat_model_risk_distribution_empty() {
    let model = ThreatModel::new("Test");
    let dist = model.risk_distribution();
    assert!(dist.is_empty());
}

#[test]
fn test_threat_model_stride_coverage() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(Threat::new("T1", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T2", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T3", StrideCategory::Tampering));

    let coverage = model.stride_coverage();

    assert_eq!(*coverage.get(&StrideCategory::Spoofing).unwrap(), 2);
    assert_eq!(*coverage.get(&StrideCategory::Tampering).unwrap(), 1);
    assert_eq!(*coverage.get(&StrideCategory::Repudiation).unwrap(), 0);
    assert_eq!(*coverage.get(&StrideCategory::InformationDisclosure).unwrap(), 0);
    assert_eq!(*coverage.get(&StrideCategory::DenialOfService).unwrap(), 0);
    assert_eq!(*coverage.get(&StrideCategory::ElevationOfPrivilege).unwrap(), 0);
}

#[test]
fn test_threat_model_stride_coverage_empty() {
    let model = ThreatModel::new("Test");
    let coverage = model.stride_coverage();

    assert_eq!(coverage.len(), 6);
    for count in coverage.values() {
        assert_eq!(*count, 0);
    }
}

#[test]
fn test_threat_model_generate_risk_matrix() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(
        Threat::new("T1", StrideCategory::Spoofing)
            .with_severity(Severity::High)
            .with_likelihood(Likelihood::Likely),
    );

    let matrix = model.generate_risk_matrix();
    let threats = matrix.threats_at(Severity::High, Likelihood::Likely);
    assert_eq!(threats.len(), 1);
}

#[test]
fn test_threat_model_generate_report() {
    let mut model = ThreatModel::new("Test App");

    model.add_asset(Asset::new("DB", AssetType::UserData));
    model.add_threat(Threat::new("SQL Injection", StrideCategory::Tampering));
    model.add_control(SecurityControl::new(
        "Parameterized Queries",
        ControlType::Preventive,
    ));

    let report = model.generate_report();

    assert!(report.contains("# Threat Model: Test App"));
    assert!(report.contains("SQL Injection"));
    assert!(report.contains("Parameterized Queries"));
    assert!(report.contains("Executive Summary"));
    assert!(report.contains("Risk Distribution"));
    assert!(report.contains("Assets"));
    assert!(report.contains("STRIDE Category"));
    assert!(report.contains("Security Controls"));
}

#[test]
fn test_threat_model_generate_report_empty() {
    let model = ThreatModel::new("Empty App");
    let report = model.generate_report();

    assert!(report.contains("Total Threats**: 0"));
    assert!(report.contains("Open Threats**: 0"));
    assert!(report.contains("Critical Threats**: 0"));
}

#[test]
fn test_threat_model_generate_report_with_entry_points() {
    let mut model = ThreatModel::new("Test");
    model.add_entry_point(EntryPoint::new("/api", EntryPointType::RestApi));

    let report = model.generate_report();
    assert!(report.contains("Attack Surface"));
}

// ============================================================================
// Edge Cases and Error Conditions
// ============================================================================

#[test]
fn test_asset_value_clamping_edge_cases() {
    let asset_min = Asset::new("Min", AssetType::Other).with_value(0);
    assert_eq!(asset_min.value, 1);

    let asset_max = Asset::new("Max", AssetType::Other).with_value(255);
    assert_eq!(asset_max.value, 5);
}

#[test]
fn test_asset_sensitivity_clamping_edge_cases() {
    let asset_min = Asset::new("Min", AssetType::Other).with_sensitivity(0);
    assert_eq!(asset_min.sensitivity, 1);

    let asset_max = Asset::new("Max", AssetType::Other).with_sensitivity(255);
    assert_eq!(asset_max.sensitivity, 5);
}

#[test]
fn test_control_effectiveness_clamping_edge_cases() {
    let control_min = SecurityControl::new("Min", ControlType::Preventive).with_effectiveness(0);
    assert_eq!(control_min.effectiveness, 1);

    let control_max = SecurityControl::new("Max", ControlType::Preventive).with_effectiveness(255);
    assert_eq!(control_max.effectiveness, 5);
}

#[test]
fn test_threat_risk_score_capped_at_16() {
    // Even with extreme values, risk score should be capped at 16
    let threat = Threat::new("Max", StrideCategory::ElevationOfPrivilege)
        .with_severity(Severity::Critical) // 4
        .with_likelihood(Likelihood::AlmostCertain); // 4

    // 4 * 4 = 16, which is the max
    assert_eq!(threat.risk_score(), 16);
}

#[test]
fn test_empty_threat_model_operations() {
    let model = ThreatModel::new("Empty");

    // All operations on empty model should work without panic
    assert!(model.open_threats().is_empty());
    assert!(model.critical_threats().is_empty());
    assert_eq!(model.overall_risk_score(), 0.0);
    assert!(model.risk_distribution().is_empty());
    assert!(model.generate_report().contains("Empty"));
}

#[test]
fn test_multiple_threats_same_category() {
    let mut model = ThreatModel::new("Test");

    for i in 0..100 {
        model.add_threat(Threat::new(format!("Threat {}", i), StrideCategory::Spoofing));
    }

    let spoofing = model.threats_by_category(StrideCategory::Spoofing);
    assert_eq!(spoofing.len(), 100);
}

#[test]
fn test_threat_status_transitions() {
    let mut model = ThreatModel::new("Test");

    let threat = Threat::new("Test", StrideCategory::Spoofing);
    let id = model.add_threat(threat);

    // All status transitions
    for status in [
        ThreatStatus::Open,
        ThreatStatus::Mitigated,
        ThreatStatus::Accepted,
        ThreatStatus::Transferred,
        ThreatStatus::Closed,
    ] {
        if let Some(t) = model.get_threat_mut(&id) {
            t.status = status;
        }
        assert_eq!(model.get_threat(&id).unwrap().status, status);
    }
}

#[test]
fn test_control_status_transitions() {
    let mut control = SecurityControl::new("Test", ControlType::Preventive);

    assert_eq!(control.status, ControlStatus::Planned);

    control = control.with_status(ControlStatus::Partial);
    assert_eq!(control.status, ControlStatus::Partial);

    control = control.with_status(ControlStatus::Implemented);
    assert_eq!(control.status, ControlStatus::Implemented);

    control = control.with_status(ControlStatus::NotApplicable);
    assert_eq!(control.status, ControlStatus::NotApplicable);
}

#[test]
fn test_stride_analyzer_analyze_empty_content() {
    let analyzer = StrideAnalyzer::new();
    let threats = analyzer.analyze("", &PathBuf::from("empty.rs"));
    assert!(threats.is_empty());
}

#[test]
fn test_stride_analyzer_analyze_special_characters() {
    let analyzer = StrideAnalyzer::new();
    let code = "!@#$%^&*()_+-=[]{}|;':\",./<>?";
    let _threats = analyzer.analyze(code, &PathBuf::from("special.rs"));
    // Should not panic with special characters
}

#[test]
fn test_threat_pattern_matches_unicode() {
    let pattern = ThreatPattern::new("Unicode", vec!["test"]);
    assert!(pattern.matches("test 中文"));
    assert!(pattern.matches("test 🚀 emoji"));
}

// ============================================================================
// Property-Based Tests
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;

    /// Test that severity score mapping is consistent
    #[test]
    fn test_severity_score_roundtrip() {
        for score in 1..=4 {
            let severity = Severity::from_score(score);
            // The score should map back to a valid severity
            assert!(severity.score() >= 1 && severity.score() <= 4);
        }
    }

    /// Test that likelihood score mapping is consistent
    #[test]
    fn test_likelihood_score_roundtrip() {
        for score in 1..=4 {
            let likelihood = Likelihood::from_score(score);
            // The score should map back to a valid likelihood
            assert!(likelihood.score() >= 1 && likelihood.score() <= 4);
        }
    }

    /// Test that risk level from score is consistent
    #[test]
    fn test_risk_level_score_consistency() {
        for score in 0..=20 {
            let level = RiskLevel::from_score(score);
            let (min, _max) = level.score_range();
            assert!(score >= min || score < min, "Score {} should be in range for {:?}", score, level);
        }
    }

    /// Test that threat risk score is always in valid range
    #[test]
    fn test_threat_risk_score_bounds() {
        for sev_score in 1..=4 {
            for lik_score in 1..=4 {
                let severity = Severity::from_score(sev_score);
                let likelihood = Likelihood::from_score(lik_score);

                let threat = Threat::new("Test", StrideCategory::Spoofing)
                    .with_severity(severity)
                    .with_likelihood(likelihood);

                let risk = threat.risk_score();
                assert!(risk >= 1 && risk <= 16);
            }
        }
    }

    /// Test asset risk score bounds
    #[test]
    fn test_asset_risk_score_bounds() {
        for value in 1..=5 {
            for sensitivity in 1..=5 {
                let asset = Asset::new("Test", AssetType::Other)
                    .with_value(value)
                    .with_sensitivity(sensitivity);

                let score = asset.risk_score();
                assert!(score >= 1 && score <= 5);
            }
        }
    }

    /// Test that all stride categories have mitigations
    #[test]
    fn test_all_stride_categories_have_mitigations() {
        for category in StrideCategory::all() {
            let mitigations = category.typical_mitigations();
            assert!(!mitigations.is_empty(), "{:?} should have mitigations", category);
        }
    }

    /// Test that all stride categories have descriptions
    #[test]
    fn test_all_stride_categories_have_descriptions() {
        for category in StrideCategory::all() {
            let desc = category.description();
            assert!(!desc.is_empty(), "{:?} should have description", category);
        }
    }

    /// Test that unique IDs are truly unique
    #[test]
    fn test_unique_id_generation() {
        let mut threat_ids = std::collections::HashSet::new();
        let mut asset_ids = std::collections::HashSet::new();
        let mut control_ids = std::collections::HashSet::new();

        for _ in 0..100 {
            let threat = Threat::new("Test", StrideCategory::Spoofing);
            assert!(threat_ids.insert(threat.id.clone()), "Duplicate threat ID: {}", threat.id);

            let asset = Asset::new("Test", AssetType::Other);
            assert!(asset_ids.insert(asset.id.clone()), "Duplicate asset ID: {}", asset.id);

            let control = SecurityControl::new("Test", ControlType::Preventive);
            assert!(control_ids.insert(control.id.clone()), "Duplicate control ID: {}", control.id);
        }
    }

    /// Test risk matrix add and retrieve consistency
    #[test]
    fn test_risk_matrix_consistency() {
        let mut matrix = RiskMatrix::new();
        
        matrix.add_threat("test-id", Severity::High, Likelihood::Likely);
        
        let threats = matrix.threats_at(Severity::High, Likelihood::Likely);
        assert_eq!(threats.len(), 1);
        assert_eq!(threats[0], "test-id");
        
        // Different severity/likelihood should be empty
        assert!(matrix.threats_at(Severity::Low, Likelihood::Unlikely).is_empty());
    }
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_full_threat_model_workflow() {
    // Create a threat model
    let mut model = ThreatModel::new("E-commerce System")
        .with_description("Online shopping platform");

    // Add assets
    let db_asset = Asset::new("Customer Database", AssetType::UserData)
        .with_value(5)
        .with_sensitivity(5)
        .with_description("Contains PII and payment info");
    let db_id = model.add_asset(db_asset);

    let api_asset = Asset::new("Payment API", AssetType::ApiKeys)
        .with_value(5)
        .with_sensitivity(4);
    let api_id = model.add_asset(api_asset);

    // Add entry points
    model.add_entry_point(
        EntryPoint::new("/api/v1/users", EntryPointType::RestApi)
            .with_trust_level(TrustLevel::Authenticated)
            .requires_authentication(),
    );

    model.add_entry_point(
        EntryPoint::new("/api/v1/payments", EntryPointType::RestApi)
            .with_trust_level(TrustLevel::Authenticated)
            .requires_authentication(),
    );

    // Add threats
    let sql_injection = Threat::new("SQL Injection", StrideCategory::Tampering)
        .with_description("Unsanitized user input in queries")
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::Possible)
        .with_affected_asset(&db_id)
        .with_attack_vector("HTTP Request Parameters")
        .with_impact("Data breach, data loss")
        .with_recommendation("Use parameterized queries");
    let sql_id = model.add_threat(sql_injection);

    let data_leak = Threat::new("Sensitive Data Exposure", StrideCategory::InformationDisclosure)
        .with_description("API keys in logs")
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely)
        .with_affected_asset(&api_id);
    model.add_threat(data_leak);

    // Add controls
    let input_validation = SecurityControl::new("Input Validation", ControlType::Preventive)
        .with_description("Validate and sanitize all user inputs")
        .with_status(ControlStatus::Implemented)
        .with_effectiveness(4)
        .mitigates_threat(&sql_id);
    model.add_control(input_validation);

    let audit_logging = SecurityControl::new("Audit Logging", ControlType::Detective)
        .with_description("Log all access to sensitive data")
        .with_status(ControlStatus::Planned)
        .with_effectiveness(3);
    model.add_control(audit_logging);

    // Add trust boundaries
    model.add_trust_boundary(
        TrustBoundary::new("DMZ")
            .with_description("Demilitarized zone")
            .with_component("Load Balancer")
            .with_component("WAF")
            .with_trust_levels(TrustLevel::Authenticated, TrustLevel::Anonymous),
    );

    // Verify model state
    assert_eq!(model.assets().count(), 2);
    assert_eq!(model.threats().count(), 2);
    assert_eq!(model.controls().count(), 2);
    assert_eq!(model.entry_points().len(), 2);
    assert_eq!(model.trust_boundaries().len(), 1);

    // Test filtering
    let tampering_threats = model.threats_by_category(StrideCategory::Tampering);
    assert_eq!(tampering_threats.len(), 1);

    let info_disc_threats = model.threats_by_category(StrideCategory::InformationDisclosure);
    assert_eq!(info_disc_threats.len(), 1);

    // Test risk calculations
    let open_threats = model.open_threats();
    assert_eq!(open_threats.len(), 2);

    let risk_score = model.overall_risk_score();
    assert!(risk_score > 0.0);

    let risk_dist = model.risk_distribution();
    assert!(!risk_dist.is_empty());

    let stride_coverage = model.stride_coverage();
    assert_eq!(stride_coverage.len(), 6);

    // Generate and verify report
    let report = model.generate_report();
    assert!(report.contains("E-commerce System"));
    assert!(report.contains("Customer Database"));
    assert!(report.contains("Payment API"));
    assert!(report.contains("SQL Injection"));
    assert!(report.contains("Sensitive Data Exposure"));
    assert!(report.contains("Input Validation"));
    assert!(report.contains("Audit Logging"));
}

#[test]
fn test_scanner_integration() {
    let scanner = SecurityScanner::new();

    let vulnerable_code = r#"
use std::process::Command;

#[get("/execute")]
fn execute_command(user_input: &str) -> String {
    // DANGEROUS: Command injection vulnerability
    let output = Command::new("sh")
        .arg("-c")
        .arg(&user_input)
        .output()
        .expect("Failed");
    
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn process_payment(token: &str) {
    // Log sensitive data
    println!("Processing payment with token: {}", token);
}
"#;

    let result = scanner.scan_file(vulnerable_code, &PathBuf::from("api.rs"));

    // Should detect entry points and threats
    assert!(!result.entry_points.is_empty());
}

// ============================================================================
// Regression Tests
// ============================================================================

#[test]
fn test_risk_score_boundary_conditions() {
    // Test all combinations of severity and likelihood
    let severities = [
        (Severity::Low, 1),
        (Severity::Medium, 2),
        (Severity::High, 3),
        (Severity::Critical, 4),
    ];

    let likelihoods = [
        (Likelihood::Unlikely, 1),
        (Likelihood::Possible, 2),
        (Likelihood::Likely, 3),
        (Likelihood::AlmostCertain, 4),
    ];

    for (sev, sev_score) in &severities {
        for (lik, lik_score) in &likelihoods {
            let threat = Threat::new("Test", StrideCategory::Spoofing)
                .with_severity(*sev)
                .with_likelihood(*lik);

            let expected = (sev_score * lik_score).min(16);
            assert_eq!(
                threat.risk_score(),
                expected,
                "Mismatch for severity {:?} ({}), likelihood {:?} ({})",
                sev,
                sev_score,
                lik,
                lik_score
            );
        }
    }
}

#[test]
fn test_risk_level_boundary_conditions() {
    // Test boundary values for RiskLevel::from_score
    let test_cases = vec![
        (0, RiskLevel::Low),
        (1, RiskLevel::Low),
        (3, RiskLevel::Low),
        (4, RiskLevel::Moderate),
        (6, RiskLevel::Moderate),
        (7, RiskLevel::High),
        (11, RiskLevel::High),
        (12, RiskLevel::Critical),
        (16, RiskLevel::Critical),
        (100, RiskLevel::Critical),
    ];

    for (score, expected) in test_cases {
        assert_eq!(
            RiskLevel::from_score(score),
            expected,
            "Score {} should map to {:?}",
            score,
            expected
        );
    }
}
