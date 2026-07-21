use super::*;

#[test]
fn test_security_severity_ordering() {
    assert!(SecuritySeverity::Critical > SecuritySeverity::High);
    assert!(SecuritySeverity::High > SecuritySeverity::Medium);
    assert!(SecuritySeverity::Medium > SecuritySeverity::Low);
    assert!(SecuritySeverity::Low > SecuritySeverity::Info);
}

#[test]
fn test_security_severity_scores() {
    assert_eq!(SecuritySeverity::Info.score(), 0.0);
    assert_eq!(SecuritySeverity::Low.score(), 3.0);
    assert_eq!(SecuritySeverity::Medium.score(), 5.5);
    assert_eq!(SecuritySeverity::High.score(), 7.5);
    assert_eq!(SecuritySeverity::Critical.score(), 9.5);
}

#[test]
fn test_security_severity_as_str() {
    assert_eq!(SecuritySeverity::Info.as_str(), "info");
    assert_eq!(SecuritySeverity::Low.as_str(), "low");
    assert_eq!(SecuritySeverity::Medium.as_str(), "medium");
    assert_eq!(SecuritySeverity::High.as_str(), "high");
    assert_eq!(SecuritySeverity::Critical.as_str(), "critical");
}

#[test]
fn test_security_category_as_str() {
    assert_eq!(
        SecurityCategory::HardcodedSecret.as_str(),
        "hardcoded_secret"
    );
    assert_eq!(SecurityCategory::Injection.as_str(), "injection");
    assert_eq!(SecurityCategory::Authentication.as_str(), "authentication");
    assert_eq!(SecurityCategory::Authorization.as_str(), "authorization");
    assert_eq!(SecurityCategory::DataExposure.as_str(), "data_exposure");
    assert_eq!(SecurityCategory::Cryptography.as_str(), "cryptography");
    assert_eq!(SecurityCategory::Configuration.as_str(), "configuration");
    assert_eq!(SecurityCategory::Dependency.as_str(), "dependency");
    assert_eq!(SecurityCategory::Compliance.as_str(), "compliance");
    assert_eq!(SecurityCategory::CodeQuality.as_str(), "code_quality");
    assert_eq!(
        SecurityCategory::Custom("custom_cat".to_string()).as_str(),
        "custom_cat"
    );
}

#[test]
fn test_execution_mode_default() {
    let default: ExecutionMode = Default::default();
    assert_eq!(default, ExecutionMode::Normal);
}

#[test]
fn test_risk_level_variants() {
    let low = RiskLevel::Low;
    let medium = RiskLevel::Medium;
    let high = RiskLevel::High;

    assert!(matches!(low, RiskLevel::Low));
    assert!(matches!(medium, RiskLevel::Medium));
    assert!(matches!(high, RiskLevel::High));
}

#[test]
fn test_risk_level_as_str() {
    assert_eq!(RiskLevel::Low.as_str(), "low");
    assert_eq!(RiskLevel::Medium.as_str(), "medium");
    assert_eq!(RiskLevel::High.as_str(), "high");
}

#[test]
fn test_permission_result_variants() {
    let allow = PermissionResult::Allow;
    let deny = PermissionResult::Deny {
        reason: "test".to_string(),
    };
    let prompt = PermissionResult::Prompt {
        reason: "confirm".to_string(),
    };

    assert!(matches!(allow, PermissionResult::Allow));
    assert!(matches!(deny, PermissionResult::Deny { .. }));
    assert!(matches!(prompt, PermissionResult::Prompt { .. }));
}
