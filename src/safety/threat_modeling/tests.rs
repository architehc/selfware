//! Tests for threat modeling module

use super::*;
use std::path::PathBuf;

#[test]
fn test_stride_category_display() {
    assert_eq!(format!("{}", StrideCategory::Spoofing), "Spoofing");
    assert_eq!(
        format!("{}", StrideCategory::InformationDisclosure),
        "Information Disclosure"
    );
}

#[test]
fn test_stride_category_description() {
    assert!(!StrideCategory::Spoofing.description().is_empty());
    assert!(!StrideCategory::Tampering.description().is_empty());
}

#[test]
fn test_stride_category_mitigations() {
    let mitigations = StrideCategory::Spoofing.typical_mitigations();
    assert!(!mitigations.is_empty());
    assert!(mitigations.iter().any(|m| m.contains("authentication")));
}

#[test]
fn test_stride_category_all() {
    let all = StrideCategory::all();
    assert_eq!(all.len(), 6);
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
    assert_eq!(Severity::from_score(1), Severity::Low);
    assert_eq!(Severity::from_score(2), Severity::Medium);
    assert_eq!(Severity::from_score(3), Severity::High);
    assert_eq!(Severity::from_score(4), Severity::Critical);
}

#[test]
fn test_likelihood_score() {
    assert_eq!(Likelihood::Unlikely.score(), 1);
    assert_eq!(Likelihood::Possible.score(), 2);
    assert_eq!(Likelihood::Likely.score(), 3);
    assert_eq!(Likelihood::AlmostCertain.score(), 4);
}

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
}

#[test]
fn test_asset_risk_score() {
    let asset = Asset::new("Test", AssetType::Other)
        .with_value(4)
        .with_sensitivity(4);

    assert_eq!(asset.risk_score(), 4);
}

#[test]
fn test_threat_creation() {
    let threat = Threat::new("SQL Injection", StrideCategory::Tampering)
        .with_severity(Severity::High)
        .with_likelihood(Likelihood::Likely);

    assert_eq!(threat.title, "SQL Injection");
    assert_eq!(threat.category, StrideCategory::Tampering);
    assert_eq!(threat.severity, Severity::High);
}

#[test]
fn test_threat_risk_score() {
    let threat = Threat::new("Test", StrideCategory::Tampering)
        .with_severity(Severity::High) // 3
        .with_likelihood(Likelihood::Likely); // 3

    assert_eq!(threat.risk_score(), 9);
}

#[test]
fn test_threat_risk_level() {
    let low = Threat::new("Low", StrideCategory::Spoofing)
        .with_severity(Severity::Low)
        .with_likelihood(Likelihood::Unlikely);

    let critical = Threat::new("Critical", StrideCategory::Tampering)
        .with_severity(Severity::Critical)
        .with_likelihood(Likelihood::AlmostCertain);

    assert_eq!(low.risk_level(), RiskLevel::Low);
    assert_eq!(critical.risk_level(), RiskLevel::Critical);
}

#[test]
fn test_security_control_creation() {
    let control = SecurityControl::new("Input Validation", ControlType::Preventive)
        .with_status(ControlStatus::Implemented)
        .with_effectiveness(4);

    assert_eq!(control.name, "Input Validation");
    assert_eq!(control.control_type, ControlType::Preventive);
    assert_eq!(control.status, ControlStatus::Implemented);
}

#[test]
fn test_entry_point_creation() {
    let entry = EntryPoint::new("/api/users", EntryPointType::RestApi)
        .with_trust_level(TrustLevel::Authenticated)
        .requires_authentication();

    assert_eq!(entry.name, "/api/users");
    assert_eq!(entry.entry_type, EntryPointType::RestApi);
    assert!(entry.requires_auth);
}

#[test]
fn test_threat_model_creation() {
    let model =
        ThreatModel::new("My Application").with_description("Web application threat model");

    assert_eq!(model.name, "My Application");
    assert!(!model.description.is_empty());
}

#[test]
fn test_threat_model_add_asset() {
    let mut model = ThreatModel::new("Test");

    let asset = Asset::new("Database", AssetType::UserData);
    let id = model.add_asset(asset);

    assert!(model.get_asset(&id).is_some());
}

#[test]
fn test_threat_model_add_threat() {
    let mut model = ThreatModel::new("Test");

    let threat = Threat::new("XSS", StrideCategory::Tampering);
    let id = model.add_threat(threat);

    assert!(model.get_threat(&id).is_some());
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
fn test_threat_model_overall_risk_score() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(
        Threat::new("T1", StrideCategory::Spoofing)
            .with_severity(Severity::High)
            .with_likelihood(Likelihood::Likely),
    );

    let score = model.overall_risk_score();
    assert!(score > 0.0);
}

#[test]
fn test_threat_model_stride_coverage() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(Threat::new("T1", StrideCategory::Spoofing));
    model.add_threat(Threat::new("T2", StrideCategory::Tampering));

    let coverage = model.stride_coverage();

    assert_eq!(*coverage.get(&StrideCategory::Spoofing).unwrap(), 1);
    assert_eq!(*coverage.get(&StrideCategory::Tampering).unwrap(), 1);
    assert_eq!(*coverage.get(&StrideCategory::Repudiation).unwrap(), 0);
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
}

#[test]
fn test_trust_boundary_creation() {
    let boundary = TrustBoundary::new("Internal Network")
        .with_description("Protected network segment")
        .with_component("Database")
        .with_trust_levels(TrustLevel::System, TrustLevel::Anonymous);

    assert_eq!(boundary.name, "Internal Network");
    assert_eq!(boundary.components.len(), 1);
    assert_eq!(boundary.internal_trust, TrustLevel::System);
}

#[test]
fn test_risk_matrix() {
    let mut matrix = RiskMatrix::new();

    matrix.add_threat("t1", Severity::High, Likelihood::Likely);
    matrix.add_threat("t2", Severity::High, Likelihood::Likely);
    matrix.add_threat("t3", Severity::Low, Likelihood::Unlikely);

    let high_likely = matrix.threats_at(Severity::High, Likelihood::Likely);
    assert_eq!(high_likely.len(), 2);

    let text = matrix.to_text();
    assert!(text.contains("LIKELIHOOD"));
}

#[test]
fn test_risk_level_from_score() {
    assert_eq!(RiskLevel::from_score(1), RiskLevel::Low);
    assert_eq!(RiskLevel::from_score(5), RiskLevel::Moderate);
    assert_eq!(RiskLevel::from_score(9), RiskLevel::High);
    assert_eq!(RiskLevel::from_score(14), RiskLevel::Critical);
}

#[test]
fn test_stride_analyzer() {
    let analyzer = StrideAnalyzer::new();

    let code = r#"
fn login(user: &str, password: &str) {
    // Basic auth with plaintext password
    let query = format!("SELECT * FROM users WHERE password = '{}'", password);
}
"#;

    let threats = analyzer.analyze(code, &PathBuf::from("test.rs"));

    // Should detect potential issues
    assert!(!threats.is_empty());
}

#[test]
fn test_stride_analyzer_patterns() {
    let analyzer = StrideAnalyzer::new();

    let spoofing_patterns = analyzer.get_patterns(StrideCategory::Spoofing);
    assert!(!spoofing_patterns.is_empty());
}

#[test]
fn test_threat_pattern_matches() {
    let pattern = ThreatPattern::new("SQL Injection", vec!["sql", "query"]);

    assert!(pattern.matches("let sql = execute_query()"));
    assert!(!pattern.matches("let x = 42"));
}

#[test]
fn test_attack_surface_mapper() {
    let mapper = AttackSurfaceMapper::new();

    let code = r#"
#[get("/users")]
async fn get_users() -> impl IntoResponse {
    // ...
}
"#;

    let entry_points = mapper.map(code);
    assert!(!entry_points.is_empty());
}

#[test]
fn test_security_scanner() {
    let scanner = SecurityScanner::new();

    let code = r#"
fn handler(input: &str) {
    let query = format!("SELECT * FROM users WHERE name = '{}'", input);
}
"#;

    let result = scanner.scan_file(code, &PathBuf::from("test.rs"));

    assert!(!result.threats.is_empty());
}

#[test]
fn test_unique_threat_ids() {
    let t1 = Threat::new("T1", StrideCategory::Spoofing);
    let t2 = Threat::new("T2", StrideCategory::Spoofing);

    assert_ne!(t1.id, t2.id);
}

#[test]
fn test_unique_asset_ids() {
    let a1 = Asset::new("A1", AssetType::Other);
    let a2 = Asset::new("A2", AssetType::Other);

    assert_ne!(a1.id, a2.id);
}

#[test]
fn test_unique_control_ids() {
    let c1 = SecurityControl::new("C1", ControlType::Preventive);
    let c2 = SecurityControl::new("C2", ControlType::Preventive);

    assert_ne!(c1.id, c2.id);
}

#[test]
fn test_asset_type_display() {
    assert_eq!(format!("{}", AssetType::UserData), "User Data");
    assert_eq!(format!("{}", AssetType::ApiKeys), "API Keys");
}

#[test]
fn test_entry_point_type_display() {
    assert_eq!(format!("{}", EntryPointType::RestApi), "REST API");
    assert_eq!(format!("{}", EntryPointType::GraphQL), "GraphQL");
}

#[test]
fn test_control_type_display() {
    assert_eq!(format!("{}", ControlType::Preventive), "Preventive");
    assert_eq!(format!("{}", ControlType::Detective), "Detective");
}

#[test]
fn test_control_status_display() {
    assert_eq!(format!("{}", ControlStatus::Implemented), "Implemented");
    assert_eq!(format!("{}", ControlStatus::Planned), "Planned");
}

#[test]
fn test_trust_level_display() {
    assert_eq!(format!("{}", TrustLevel::Anonymous), "Anonymous");
    assert_eq!(format!("{}", TrustLevel::Admin), "Admin");
}

#[test]
fn test_threat_status_display() {
    assert_eq!(format!("{}", ThreatStatus::Open), "Open");
    assert_eq!(format!("{}", ThreatStatus::Mitigated), "Mitigated");
}

#[test]
fn test_risk_level_score_range() {
    assert_eq!(RiskLevel::Low.score_range(), (1, 3));
    assert_eq!(RiskLevel::Critical.score_range(), (12, 16));
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

    assert_eq!(threat.affected_assets.len(), 1);
    assert!(threat.attack_vector.is_some());
    assert!(!threat.mitigations.is_empty());
    assert!(threat.source_file.is_some());
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
fn test_threat_model_critical_threats() {
    let mut model = ThreatModel::new("Test");

    model.add_threat(
        Threat::new("Critical Threat", StrideCategory::Tampering)
            .with_severity(Severity::Critical)
            .with_likelihood(Likelihood::AlmostCertain),
    );

    model.add_threat(
        Threat::new("Low Threat", StrideCategory::Spoofing)
            .with_severity(Severity::Low)
            .with_likelihood(Likelihood::Unlikely),
    );

    let critical = model.critical_threats();
    assert_eq!(critical.len(), 1);
}
