//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Threat modeling
//! - Sandboxing
//! - Execution control modes

pub mod audit;
pub mod autonomy;
pub mod checker;
pub mod path_validator;
pub mod permissions;
pub mod redact;
pub mod sandbox;
pub mod scanner;
pub mod threat_modeling;
pub mod tool_metadata;

#[cfg(feature = "execution-modes")]
pub mod confirm;
#[cfg(feature = "execution-modes")]
pub mod dry_run;
#[cfg(feature = "execution-modes")]
pub mod yolo;

// Re-exports for convenience
pub use autonomy::{AutonomyContext, AutonomyController, AutonomyLevel};
pub use checker::validation::{
    is_private_or_internal, normalize_shell_command, split_shell_commands, PinnedDnsResolver,
};
pub use checker::SafetyChecker;
pub use sandbox::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
pub use scanner::{
    SecretScanner, SecurityCategory, SecurityFinding, SecurityScanner, SecuritySeverity,
};
pub use threat_modeling::{Asset, SecurityControl, StrideCategory, Threat};
pub use tool_metadata::{
    default_tool_metadata, ExecutionMode, PermissionChecker, PermissionResult, RiskLevel,
    ToolMetadata,
};

#[cfg(test)]
mod tests {
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
        assert_eq!(SecurityCategory::HardcodedSecret.as_str(), "hardcoded_secret");
        assert_eq!(SecurityCategory::Injection.as_str(), "injection");
        assert_eq!(SecurityCategory::Authentication.as_str(), "authentication");
        assert_eq!(SecurityCategory::Authorization.as_str(), "authorization");
        assert_eq!(SecurityCategory::DataExposure.as_str(), "data_exposure");
        assert_eq!(SecurityCategory::Cryptography.as_str(), "cryptography");
        assert_eq!(SecurityCategory::Configuration.as_str(), "configuration");
        assert_eq!(SecurityCategory::Dependency.as_str(), "dependency");
        assert_eq!(SecurityCategory::Compliance.as_str(), "compliance");
        assert_eq!(SecurityCategory::CodeQuality.as_str(), "code_quality");
        assert_eq!(SecurityCategory::Custom("custom_cat".to_string()).as_str(), "custom_cat");
    }

    #[test]
    fn test_autonomy_level_default() {
        let default: AutonomyLevel = Default::default();
        assert_eq!(default, AutonomyLevel::ConfirmAll);
    }

    #[test]
    fn test_autonomy_level_from_str() {
        assert_eq!(AutonomyLevel::from_str("yolo"), Some(AutonomyLevel::Yolo));
        assert_eq!(AutonomyLevel::from_str("YOLO"), Some(AutonomyLevel::Yolo));
        assert_eq!(AutonomyLevel::from_str("confirm_all"), Some(AutonomyLevel::ConfirmAll));
        assert_eq!(AutonomyLevel::from_str("dry_run"), Some(AutonomyLevel::DryRun));
        assert_eq!(AutonomyLevel::from_str("invalid"), None);
    }

    #[test]
    fn test_execution_mode_default() {
        let default: ExecutionMode = Default::default();
        assert_eq!(default, ExecutionMode::ConfirmAll);
    }

    #[test]
    fn test_risk_level_as_str() {
        assert_eq!(RiskLevel::None.as_str(), "none");
        assert_eq!(RiskLevel::Low.as_str(), "low");
        assert_eq!(RiskLevel::Medium.as_str(), "medium");
        assert_eq!(RiskLevel::High.as_str(), "high");
        assert_eq!(RiskLevel::Critical.as_str(), "critical");
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Critical > RiskLevel::High);
        assert!(RiskLevel::High > RiskLevel::Medium);
        assert!(RiskLevel::Medium > RiskLevel::Low);
        assert!(RiskLevel::Low > RiskLevel::None);
    }

    #[test]
    fn test_permission_result_default() {
        let default = PermissionResult::default();
        assert!(!default.granted);
        assert!(default.expires_at.is_none());
        assert!(default.reason.is_none());
    }

    #[test]
    fn test_permission_result_granted() {
        let result = PermissionResult::granted();
        assert!(result.granted);
        assert!(result.expires_at.is_none());
    }

    #[test]
    fn test_permission_result_denied() {
        let result = PermissionResult::denied("test reason");
        assert!(!result.granted);
        assert_eq!(result.reason, Some("test reason".to_string()));
    }

    #[test]
    fn test_permission_result_is_expired() {
        let mut result = PermissionResult::granted();
        assert!(!result.is_expired());
        
        // Set expiration in the past
        result.expires_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
        assert!(result.is_expired());
    }

    #[test]
    fn test_filesystem_policy_default() {
        let policy = FilesystemPolicy::default();
        assert!(policy.read_allowed);
        assert!(!policy.write_allowed);
        assert!(!policy.delete_allowed);
        assert!(policy.allowed_paths.is_empty());
        assert!(policy.denied_paths.is_empty());
    }

    #[test]
    fn test_network_policy_default() {
        let policy = NetworkPolicy::default();
        assert!(!policy.outbound_allowed);
        assert!(policy.allowed_hosts.is_empty());
        assert!(policy.denied_hosts.is_empty());
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_memory_mb, 1024);
        assert_eq!(limits.max_cpu_percent, 50.0);
        assert_eq!(limits.max_file_size_mb, 100);
        assert_eq!(limits.max_open_files, 100);
    }
}
