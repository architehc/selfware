//! Threat Modeling Types
//!
//! Core types and enums for threat modeling including STRIDE categories,
//! severity levels, assets, threats, and security controls.

use std::path::PathBuf;

/// STRIDE threat categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StrideCategory {
    /// Pretending to be something or someone else
    Spoofing,
    /// Modifying data or code
    Tampering,
    /// Denying having performed an action
    Repudiation,
    /// Exposing information to unauthorized parties
    InformationDisclosure,
    /// Making a system unavailable
    DenialOfService,
    /// Gaining unauthorized capabilities
    ElevationOfPrivilege,
}

impl std::fmt::Display for StrideCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrideCategory::Spoofing => write!(f, "Spoofing"),
            StrideCategory::Tampering => write!(f, "Tampering"),
            StrideCategory::Repudiation => write!(f, "Repudiation"),
            StrideCategory::InformationDisclosure => write!(f, "Information Disclosure"),
            StrideCategory::DenialOfService => write!(f, "Denial of Service"),
            StrideCategory::ElevationOfPrivilege => write!(f, "Elevation of Privilege"),
        }
    }
}

impl StrideCategory {
    /// Get description of the threat category
    pub fn description(&self) -> &'static str {
        match self {
            StrideCategory::Spoofing => "Impersonating something or someone else",
            StrideCategory::Tampering => "Modifying data or code without authorization",
            StrideCategory::Repudiation => "Claiming to have not performed an action",
            StrideCategory::InformationDisclosure => "Exposing information to unauthorized parties",
            StrideCategory::DenialOfService => "Making a system or resource unavailable",
            StrideCategory::ElevationOfPrivilege => {
                "Gaining capabilities beyond those initially granted"
            }
        }
    }

    /// Get typical mitigations for this category
    pub fn typical_mitigations(&self) -> Vec<&'static str> {
        match self {
            StrideCategory::Spoofing => vec![
                "Strong authentication (MFA)",
                "Certificate-based authentication",
                "Session tokens with expiration",
                "IP-based restrictions",
            ],
            StrideCategory::Tampering => vec![
                "Digital signatures",
                "Message authentication codes (MAC)",
                "Input validation",
                "Integrity checking",
            ],
            StrideCategory::Repudiation => vec![
                "Audit logging",
                "Digital signatures",
                "Timestamps",
                "Non-repudiation protocols",
            ],
            StrideCategory::InformationDisclosure => vec![
                "Encryption at rest",
                "Encryption in transit (TLS)",
                "Access control lists",
                "Data masking/redaction",
            ],
            StrideCategory::DenialOfService => vec![
                "Rate limiting",
                "Load balancing",
                "Resource quotas",
                "DDoS protection",
            ],
            StrideCategory::ElevationOfPrivilege => vec![
                "Least privilege principle",
                "Role-based access control",
                "Privilege separation",
                "Sandboxing",
            ],
        }
    }

    /// All STRIDE categories
    pub fn all() -> Vec<Self> {
        vec![
            StrideCategory::Spoofing,
            StrideCategory::Tampering,
            StrideCategory::Repudiation,
            StrideCategory::InformationDisclosure,
            StrideCategory::DenialOfService,
            StrideCategory::ElevationOfPrivilege,
        ]
    }
}

/// Threat severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Low => write!(f, "Low"),
            Severity::Medium => write!(f, "Medium"),
            Severity::High => write!(f, "High"),
            Severity::Critical => write!(f, "Critical"),
        }
    }
}

impl Severity {
    /// Get numeric score (1-4)
    pub fn score(&self) -> u8 {
        match self {
            Severity::Low => 1,
            Severity::Medium => 2,
            Severity::High => 3,
            Severity::Critical => 4,
        }
    }

    /// Create from numeric score
    pub fn from_score(score: u8) -> Self {
        match score {
            0 | 1 => Severity::Low,
            2 => Severity::Medium,
            3 => Severity::High,
            _ => Severity::Critical,
        }
    }
}

/// Likelihood of threat occurrence
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Likelihood {
    /// Unlikely to occur
    Unlikely,
    /// Possible
    Possible,
    /// Likely
    Likely,
    /// Almost certain
    AlmostCertain,
}

impl std::fmt::Display for Likelihood {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Likelihood::Unlikely => write!(f, "Unlikely"),
            Likelihood::Possible => write!(f, "Possible"),
            Likelihood::Likely => write!(f, "Likely"),
            Likelihood::AlmostCertain => write!(f, "Almost Certain"),
        }
    }
}

impl Likelihood {
    /// Get numeric score (1-4)
    pub fn score(&self) -> u8 {
        match self {
            Likelihood::Unlikely => 1,
            Likelihood::Possible => 2,
            Likelihood::Likely => 3,
            Likelihood::AlmostCertain => 4,
        }
    }

    /// Create from numeric score
    pub fn from_score(score: u8) -> Self {
        match score {
            0 | 1 => Likelihood::Unlikely,
            2 => Likelihood::Possible,
            3 => Likelihood::Likely,
            _ => Likelihood::AlmostCertain,
        }
    }
}

/// Asset type in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetType {
    /// User data
    UserData,
    /// System credentials
    Credentials,
    /// API keys and secrets
    ApiKeys,
    /// Configuration data
    Configuration,
    /// Source code
    SourceCode,
    /// Infrastructure
    Infrastructure,
    /// Financial data
    FinancialData,
    /// Intellectual property
    IntellectualProperty,
    /// Service availability
    Availability,
    /// Other
    Other,
}

impl std::fmt::Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetType::UserData => write!(f, "User Data"),
            AssetType::Credentials => write!(f, "Credentials"),
            AssetType::ApiKeys => write!(f, "API Keys"),
            AssetType::Configuration => write!(f, "Configuration"),
            AssetType::SourceCode => write!(f, "Source Code"),
            AssetType::Infrastructure => write!(f, "Infrastructure"),
            AssetType::FinancialData => write!(f, "Financial Data"),
            AssetType::IntellectualProperty => write!(f, "Intellectual Property"),
            AssetType::Availability => write!(f, "Availability"),
            AssetType::Other => write!(f, "Other"),
        }
    }
}

/// An asset in the system
#[derive(Debug, Clone)]
pub struct Asset {
    /// Unique identifier
    pub id: String,
    /// Asset name
    pub name: String,
    /// Asset type
    pub asset_type: AssetType,
    /// Description
    pub description: String,
    /// Business value (1-5)
    pub value: u8,
    /// Sensitivity (1-5)
    pub sensitivity: u8,
    /// Location/component
    pub location: Option<String>,
    /// Owner
    pub owner: Option<String>,
    /// Classification level
    pub classification: Option<String>,
}

/// Threat status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThreatStatus {
    /// Open - not yet addressed
    Open,
    /// Mitigated - controls in place
    Mitigated,
    /// Accepted - risk accepted
    Accepted,
    /// Transferred - risk transferred
    Transferred,
    /// Closed - no longer applicable
    Closed,
}

impl std::fmt::Display for ThreatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatStatus::Open => write!(f, "Open"),
            ThreatStatus::Mitigated => write!(f, "Mitigated"),
            ThreatStatus::Accepted => write!(f, "Accepted"),
            ThreatStatus::Transferred => write!(f, "Transferred"),
            ThreatStatus::Closed => write!(f, "Closed"),
        }
    }
}

/// Risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    /// Acceptable risk
    Low,
    /// Moderate risk
    Moderate,
    /// Significant risk
    High,
    /// Unacceptable risk
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "Low"),
            RiskLevel::Moderate => write!(f, "Moderate"),
            RiskLevel::High => write!(f, "High"),
            RiskLevel::Critical => write!(f, "Critical"),
        }
    }
}

impl RiskLevel {
    /// Create from numeric score (1-16)
    pub fn from_score(score: u8) -> Self {
        match score {
            0..=3 => RiskLevel::Low,
            4..=6 => RiskLevel::Moderate,
            7..=11 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }

    /// Get score range
    pub fn score_range(&self) -> (u8, u8) {
        match self {
            RiskLevel::Low => (1, 3),
            RiskLevel::Moderate => (4, 6),
            RiskLevel::High => (7, 11),
            RiskLevel::Critical => (12, 16),
        }
    }
}

/// Control type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlType {
    /// Preventive control
    Preventive,
    /// Detective control
    Detective,
    /// Corrective control
    Corrective,
    /// Deterrent control
    Deterrent,
    /// Compensating control
    Compensating,
}

impl std::fmt::Display for ControlType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlType::Preventive => write!(f, "Preventive"),
            ControlType::Detective => write!(f, "Detective"),
            ControlType::Corrective => write!(f, "Corrective"),
            ControlType::Deterrent => write!(f, "Deterrent"),
            ControlType::Compensating => write!(f, "Compensating"),
        }
    }
}

/// Control implementation status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlStatus {
    /// Planned but not implemented
    Planned,
    /// Partially implemented
    Partial,
    /// Fully implemented
    Implemented,
    /// Not applicable
    NotApplicable,
}

impl std::fmt::Display for ControlStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlStatus::Planned => write!(f, "Planned"),
            ControlStatus::Partial => write!(f, "Partial"),
            ControlStatus::Implemented => write!(f, "Implemented"),
            ControlStatus::NotApplicable => write!(f, "N/A"),
        }
    }
}

/// A threat in the model
#[derive(Debug, Clone)]
pub struct Threat {
    /// Unique identifier
    pub id: String,
    /// Threat title
    pub title: String,
    /// STRIDE category
    pub category: StrideCategory,
    /// Description
    pub description: String,
    /// Severity
    pub severity: Severity,
    /// Likelihood
    pub likelihood: Likelihood,
    /// Affected assets
    pub affected_assets: Vec<String>,
    /// Attack vector
    pub attack_vector: Option<String>,
    /// Prerequisites
    pub prerequisites: Vec<String>,
    /// Potential impact
    pub impact: String,
    /// Existing mitigations
    pub mitigations: Vec<String>,
    /// Recommended controls
    pub recommendations: Vec<String>,
    /// Status
    pub status: ThreatStatus,
    /// Source file (if code-based)
    pub source_file: Option<PathBuf>,
    /// Source line
    pub source_line: Option<usize>,
}

/// Security control
#[derive(Debug, Clone)]
pub struct SecurityControl {
    /// Unique identifier
    pub id: String,
    /// Control name
    pub name: String,
    /// Control type
    pub control_type: ControlType,
    /// Description
    pub description: String,
    /// Implementation status
    pub status: ControlStatus,
    /// Effectiveness (1-5)
    pub effectiveness: u8,
    /// Threats mitigated
    pub mitigates: Vec<String>,
    /// Owner
    pub owner: Option<String>,
    /// Implementation notes
    pub notes: Option<String>,
}

/// Entry point type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryPointType {
    /// HTTP/REST API
    RestApi,
    /// GraphQL API
    GraphQL,
    /// gRPC API
    Grpc,
    /// WebSocket
    WebSocket,
    /// CLI interface
    Cli,
    /// File upload
    FileUpload,
    /// Database connection
    Database,
    /// Message queue
    MessageQueue,
    /// Environment variables
    Environment,
    /// Configuration files
    ConfigFile,
    /// User interface
    UserInterface,
    /// Other
    Other,
}

impl std::fmt::Display for EntryPointType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryPointType::RestApi => write!(f, "REST API"),
            EntryPointType::GraphQL => write!(f, "GraphQL"),
            EntryPointType::Grpc => write!(f, "gRPC"),
            EntryPointType::WebSocket => write!(f, "WebSocket"),
            EntryPointType::Cli => write!(f, "CLI"),
            EntryPointType::FileUpload => write!(f, "File Upload"),
            EntryPointType::Database => write!(f, "Database"),
            EntryPointType::MessageQueue => write!(f, "Message Queue"),
            EntryPointType::Environment => write!(f, "Environment"),
            EntryPointType::ConfigFile => write!(f, "Config File"),
            EntryPointType::UserInterface => write!(f, "User Interface"),
            EntryPointType::Other => write!(f, "Other"),
        }
    }
}

/// Trust level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TrustLevel {
    /// Anonymous user
    Anonymous,
    /// Authenticated user
    Authenticated,
    /// Privileged user
    Privileged,
    /// Administrator
    Admin,
    /// System/Internal
    System,
}

impl std::fmt::Display for TrustLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustLevel::Anonymous => write!(f, "Anonymous"),
            TrustLevel::Authenticated => write!(f, "Authenticated"),
            TrustLevel::Privileged => write!(f, "Privileged"),
            TrustLevel::Admin => write!(f, "Admin"),
            TrustLevel::System => write!(f, "System"),
        }
    }
}

/// An asset in the system
#[derive(Debug, Clone)]
pub struct EntryPoint {
    /// Name
    pub name: String,
    /// Type
    pub entry_type: EntryPointType,
    /// Description
    pub description: String,
    /// Trust level required
    pub trust_level: TrustLevel,
    /// Associated threats
    pub threats: Vec<String>,
    /// Data flows through this point
    pub data_flows: Vec<String>,
    /// Authentication required
    pub requires_auth: bool,
}

/// Trust boundary
#[derive(Debug, Clone)]
pub struct TrustBoundary {
    /// Name
    pub name: String,
    /// Description
    pub description: String,
    /// Components inside the boundary
    pub components: Vec<String>,
    /// Trust level inside
    pub internal_trust: TrustLevel,
    /// Trust level outside
    pub external_trust: TrustLevel,
}

/// Risk matrix for visualization
#[derive(Debug)]
pub struct RiskMatrix {
    /// Threats in each cell (severity x likelihood)
    pub(crate) cells: std::collections::HashMap<(u8, u8), Vec<String>>,
}

impl Default for RiskMatrix {
    fn default() -> Self {
        Self::new()
    }
}

impl RiskMatrix {
    /// Create a new risk matrix
    pub fn new() -> Self {
        Self {
            cells: std::collections::HashMap::new(),
        }
    }

    /// Add a threat to the matrix
    pub fn add_threat(&mut self, threat_id: &str, severity: Severity, likelihood: Likelihood) {
        self.cells
            .entry((severity.score(), likelihood.score()))
            .or_default()
            .push(threat_id.to_string());
    }

    /// Get threats at a cell
    pub fn threats_at(&self, severity: Severity, likelihood: Likelihood) -> &[String] {
        self.cells
            .get(&(severity.score(), likelihood.score()))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Render as text
    pub fn to_text(&self) -> String {
        let mut output = String::new();

        output.push_str("                  LIKELIHOOD\n");
        output.push_str("           Unlikely | Possible | Likely | Certain\n");
        output.push_str("         -----------------------------------------\n");

        let severities = [
            (Severity::Critical, "Critical    "),
            (Severity::High, "High        "),
            (Severity::Medium, "Medium      "),
            (Severity::Low, "Low         "),
        ];

        let likelihoods = [
            Likelihood::Unlikely,
            Likelihood::Possible,
            Likelihood::Likely,
            Likelihood::AlmostCertain,
        ];

        for (sev, sev_label) in &severities {
            output.push_str(&format!(" S {} |", sev_label));
            for lik in &likelihoods {
                let count = self.threats_at(*sev, *lik).len();
                let cell = if count > 0 {
                    format!("   {:>3}   ", count)
                } else {
                    "    -    ".to_string()
                };
                output.push_str(&cell);
                output.push('|');
            }
            output.push('\n');
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // StrideCategory
    // ---------------------------------------------------------------------------

    #[test]
    fn stride_all_returns_six_categories() {
        let all = StrideCategory::all();
        assert_eq!(all.len(), 6, "STRIDE should have exactly 6 categories");
        // Ensure no duplicates
        let mut sorted = all.clone();
        sorted.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "all() must not contain duplicates");
    }

    #[test]
    fn stride_all_contains_every_variant() {
        let all = StrideCategory::all();
        assert!(all.contains(&StrideCategory::Spoofing));
        assert!(all.contains(&StrideCategory::Tampering));
        assert!(all.contains(&StrideCategory::Repudiation));
        assert!(all.contains(&StrideCategory::InformationDisclosure));
        assert!(all.contains(&StrideCategory::DenialOfService));
        assert!(all.contains(&StrideCategory::ElevationOfPrivilege));
    }

    #[test]
    fn stride_display_matches_expected_strings() {
        assert_eq!(StrideCategory::Spoofing.to_string(), "Spoofing");
        assert_eq!(StrideCategory::Tampering.to_string(), "Tampering");
        assert_eq!(StrideCategory::Repudiation.to_string(), "Repudiation");
        assert_eq!(
            StrideCategory::InformationDisclosure.to_string(),
            "Information Disclosure"
        );
        assert_eq!(
            StrideCategory::DenialOfService.to_string(),
            "Denial of Service"
        );
        assert_eq!(
            StrideCategory::ElevationOfPrivilege.to_string(),
            "Elevation of Privilege"
        );
    }

    #[test]
    fn stride_description_is_nonempty_and_unique() {
        let all = StrideCategory::all();
        let descriptions: Vec<&str> = all.iter().map(|c| c.description()).collect();
        for d in &descriptions {
            assert!(!d.is_empty(), "description must not be empty");
        }
        // Descriptions should be unique
        let mut sorted = descriptions.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            descriptions.len(),
            "descriptions must be unique"
        );
    }

    #[test]
    fn stride_typical_mitigations_nonempty_for_all_categories() {
        for cat in StrideCategory::all() {
            let mits = cat.typical_mitigations();
            assert!(
                !mits.is_empty(),
                "{:?} should have at least one typical mitigation",
                cat
            );
            for m in &mits {
                assert!(!m.is_empty(), "mitigation strings must not be empty");
            }
        }
    }

    #[test]
    fn stride_typical_mitigations_are_unique_per_category() {
        for cat in StrideCategory::all() {
            let mits = cat.typical_mitigations();
            let mut sorted = mits.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                mits.len(),
                "{:?} mitigations must be unique",
                cat
            );
        }
    }

    #[test]
    fn stride_copy_and_eq_semantics() {
        let a = StrideCategory::Tampering;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(a, StrideCategory::Spoofing);
    }

    // ---------------------------------------------------------------------------
    // Severity
    // ---------------------------------------------------------------------------

    #[test]
    fn severity_score_values() {
        assert_eq!(Severity::Low.score(), 1);
        assert_eq!(Severity::Medium.score(), 2);
        assert_eq!(Severity::High.score(), 3);
        assert_eq!(Severity::Critical.score(), 4);
    }

    #[test]
    fn severity_from_score_round_trip() {
        for sev in [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            assert_eq!(Severity::from_score(sev.score()), sev);
        }
    }

    #[test]
    fn severity_from_score_boundaries() {
        // 0 and 1 both map to Low
        assert_eq!(Severity::from_score(0), Severity::Low);
        assert_eq!(Severity::from_score(1), Severity::Low);
        assert_eq!(Severity::from_score(2), Severity::Medium);
        assert_eq!(Severity::from_score(3), Severity::High);
        // Anything >= 4 maps to Critical
        assert_eq!(Severity::from_score(4), Severity::Critical);
        assert_eq!(Severity::from_score(255), Severity::Critical);
    }

    #[test]
    fn severity_display() {
        assert_eq!(Severity::Low.to_string(), "Low");
        assert_eq!(Severity::Medium.to_string(), "Medium");
        assert_eq!(Severity::High.to_string(), "High");
        assert_eq!(Severity::Critical.to_string(), "Critical");
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    // ---------------------------------------------------------------------------
    // Likelihood
    // ---------------------------------------------------------------------------

    #[test]
    fn likelihood_score_values() {
        assert_eq!(Likelihood::Unlikely.score(), 1);
        assert_eq!(Likelihood::Possible.score(), 2);
        assert_eq!(Likelihood::Likely.score(), 3);
        assert_eq!(Likelihood::AlmostCertain.score(), 4);
    }

    #[test]
    fn likelihood_from_score_round_trip() {
        for lik in [
            Likelihood::Unlikely,
            Likelihood::Possible,
            Likelihood::Likely,
            Likelihood::AlmostCertain,
        ] {
            assert_eq!(Likelihood::from_score(lik.score()), lik);
        }
    }

    #[test]
    fn likelihood_from_score_boundaries() {
        assert_eq!(Likelihood::from_score(0), Likelihood::Unlikely);
        assert_eq!(Likelihood::from_score(1), Likelihood::Unlikely);
        assert_eq!(Likelihood::from_score(2), Likelihood::Possible);
        assert_eq!(Likelihood::from_score(3), Likelihood::Likely);
        assert_eq!(Likelihood::from_score(4), Likelihood::AlmostCertain);
        assert_eq!(Likelihood::from_score(100), Likelihood::AlmostCertain);
    }

    #[test]
    fn likelihood_display() {
        assert_eq!(Likelihood::Unlikely.to_string(), "Unlikely");
        assert_eq!(Likelihood::Possible.to_string(), "Possible");
        assert_eq!(Likelihood::Likely.to_string(), "Likely");
        assert_eq!(Likelihood::AlmostCertain.to_string(), "Almost Certain");
    }

    #[test]
    fn likelihood_ordering() {
        assert!(Likelihood::Unlikely < Likelihood::Possible);
        assert!(Likelihood::Possible < Likelihood::Likely);
        assert!(Likelihood::Likely < Likelihood::AlmostCertain);
    }

    // ---------------------------------------------------------------------------
    // AssetType
    // ---------------------------------------------------------------------------

    #[test]
    fn asset_type_display_all_variants() {
        assert_eq!(AssetType::UserData.to_string(), "User Data");
        assert_eq!(AssetType::Credentials.to_string(), "Credentials");
        assert_eq!(AssetType::ApiKeys.to_string(), "API Keys");
        assert_eq!(AssetType::Configuration.to_string(), "Configuration");
        assert_eq!(AssetType::SourceCode.to_string(), "Source Code");
        assert_eq!(AssetType::Infrastructure.to_string(), "Infrastructure");
        assert_eq!(AssetType::FinancialData.to_string(), "Financial Data");
        assert_eq!(
            AssetType::IntellectualProperty.to_string(),
            "Intellectual Property"
        );
        assert_eq!(AssetType::Availability.to_string(), "Availability");
        assert_eq!(AssetType::Other.to_string(), "Other");
    }

    // ---------------------------------------------------------------------------
    // ThreatStatus
    // ---------------------------------------------------------------------------

    #[test]
    fn threat_status_display_all_variants() {
        assert_eq!(ThreatStatus::Open.to_string(), "Open");
        assert_eq!(ThreatStatus::Mitigated.to_string(), "Mitigated");
        assert_eq!(ThreatStatus::Accepted.to_string(), "Accepted");
        assert_eq!(ThreatStatus::Transferred.to_string(), "Transferred");
        assert_eq!(ThreatStatus::Closed.to_string(), "Closed");
    }

    // ---------------------------------------------------------------------------
    // RiskLevel
    // ---------------------------------------------------------------------------

    #[test]
    fn risk_level_from_score_boundaries() {
        // Low: 0..=3
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(1), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(3), RiskLevel::Low);
        // Moderate: 4..=6
        assert_eq!(RiskLevel::from_score(4), RiskLevel::Moderate);
        assert_eq!(RiskLevel::from_score(6), RiskLevel::Moderate);
        // High: 7..=11
        assert_eq!(RiskLevel::from_score(7), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(11), RiskLevel::High);
        // Critical: >= 12
        assert_eq!(RiskLevel::from_score(12), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(16), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(255), RiskLevel::Critical);
    }

    #[test]
    fn risk_level_score_range_covers_all_scores() {
        // Every score 0..=16 should map into one of the ranges.
        for s in 0..=16u8 {
            let level = RiskLevel::from_score(s);
            let (lo, hi) = level.score_range();
            // The score should fall within [lo, hi] (with Low starting at 1, but 0 maps to Low)
            assert!(
                s >= lo || (level == RiskLevel::Low && s == 0),
                "score {} maps to {:?} with range {}..={} but is below lo",
                s,
                level,
                lo,
                hi
            );
            assert!(
                s <= hi,
                "score {} maps to {:?} with range {}..={} but is above hi",
                s,
                level,
                lo,
                hi
            );
        }
    }

    #[test]
    fn risk_level_score_range_values() {
        assert_eq!(RiskLevel::Low.score_range(), (1, 3));
        assert_eq!(RiskLevel::Moderate.score_range(), (4, 6));
        assert_eq!(RiskLevel::High.score_range(), (7, 11));
        assert_eq!(RiskLevel::Critical.score_range(), (12, 16));
    }

    #[test]
    fn risk_level_display() {
        assert_eq!(RiskLevel::Low.to_string(), "Low");
        assert_eq!(RiskLevel::Moderate.to_string(), "Moderate");
        assert_eq!(RiskLevel::High.to_string(), "High");
        assert_eq!(RiskLevel::Critical.to_string(), "Critical");
    }

    #[test]
    fn risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Moderate);
        assert!(RiskLevel::Moderate < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    // ---------------------------------------------------------------------------
    // ControlType
    // ---------------------------------------------------------------------------

    #[test]
    fn control_type_display_all_variants() {
        assert_eq!(ControlType::Preventive.to_string(), "Preventive");
        assert_eq!(ControlType::Detective.to_string(), "Detective");
        assert_eq!(ControlType::Corrective.to_string(), "Corrective");
        assert_eq!(ControlType::Deterrent.to_string(), "Deterrent");
        assert_eq!(ControlType::Compensating.to_string(), "Compensating");
    }

    // ---------------------------------------------------------------------------
    // ControlStatus
    // ---------------------------------------------------------------------------

    #[test]
    fn control_status_display_all_variants() {
        assert_eq!(ControlStatus::Planned.to_string(), "Planned");
        assert_eq!(ControlStatus::Partial.to_string(), "Partial");
        assert_eq!(ControlStatus::Implemented.to_string(), "Implemented");
        // N/A is the special case
        assert_eq!(ControlStatus::NotApplicable.to_string(), "N/A");
    }

    // ---------------------------------------------------------------------------
    // EntryPointType
    // ---------------------------------------------------------------------------

    #[test]
    fn entry_point_type_display_all_variants() {
        assert_eq!(EntryPointType::RestApi.to_string(), "REST API");
        assert_eq!(EntryPointType::GraphQL.to_string(), "GraphQL");
        assert_eq!(EntryPointType::Grpc.to_string(), "gRPC");
        assert_eq!(EntryPointType::WebSocket.to_string(), "WebSocket");
        assert_eq!(EntryPointType::Cli.to_string(), "CLI");
        assert_eq!(EntryPointType::FileUpload.to_string(), "File Upload");
        assert_eq!(EntryPointType::Database.to_string(), "Database");
        assert_eq!(EntryPointType::MessageQueue.to_string(), "Message Queue");
        assert_eq!(EntryPointType::Environment.to_string(), "Environment");
        assert_eq!(EntryPointType::ConfigFile.to_string(), "Config File");
        assert_eq!(EntryPointType::UserInterface.to_string(), "User Interface");
        assert_eq!(EntryPointType::Other.to_string(), "Other");
    }

    // ---------------------------------------------------------------------------
    // TrustLevel
    // ---------------------------------------------------------------------------

    #[test]
    fn trust_level_display_all_variants() {
        assert_eq!(TrustLevel::Anonymous.to_string(), "Anonymous");
        assert_eq!(TrustLevel::Authenticated.to_string(), "Authenticated");
        assert_eq!(TrustLevel::Privileged.to_string(), "Privileged");
        assert_eq!(TrustLevel::Admin.to_string(), "Admin");
        assert_eq!(TrustLevel::System.to_string(), "System");
    }

    #[test]
    fn trust_level_ordering() {
        assert!(TrustLevel::Anonymous < TrustLevel::Authenticated);
        assert!(TrustLevel::Authenticated < TrustLevel::Privileged);
        assert!(TrustLevel::Privileged < TrustLevel::Admin);
        assert!(TrustLevel::Admin < TrustLevel::System);
    }

    // ---------------------------------------------------------------------------
    // Asset struct
    // ---------------------------------------------------------------------------

    #[test]
    fn asset_construction_and_clone() {
        let a = Asset {
            id: "asset-1".to_string(),
            name: "User Database".to_string(),
            asset_type: AssetType::UserData,
            description: "Primary user data store".to_string(),
            value: 5,
            sensitivity: 4,
            location: Some("/db/users".to_string()),
            owner: Some("DBA team".to_string()),
            classification: Some("Confidential".to_string()),
        };
        let b = a.clone();
        assert_eq!(a.id, b.id);
        assert_eq!(a.name, b.name);
        assert_eq!(a.asset_type, b.asset_type);
        assert_eq!(a.value, 5);
        assert_eq!(a.sensitivity, 4);
        assert_eq!(a.location.as_deref(), Some("/db/users"));
        assert_eq!(a.owner.as_deref(), Some("DBA team"));
        assert_eq!(a.classification.as_deref(), Some("Confidential"));
    }

    #[test]
    fn asset_with_optional_none() {
        let a = Asset {
            id: "asset-2".to_string(),
            name: "Logs".to_string(),
            asset_type: AssetType::Configuration,
            description: String::new(),
            value: 1,
            sensitivity: 1,
            location: None,
            owner: None,
            classification: None,
        };
        assert!(a.location.is_none());
        assert!(a.owner.is_none());
        assert!(a.classification.is_none());
    }

    // ---------------------------------------------------------------------------
    // Threat struct
    // ---------------------------------------------------------------------------

    #[test]
    fn threat_construction_and_clone() {
        let t = Threat {
            id: "T-001".to_string(),
            title: "SQL Injection".to_string(),
            category: StrideCategory::Tampering,
            description: "Unsanitized input".to_string(),
            severity: Severity::High,
            likelihood: Likelihood::Likely,
            affected_assets: vec!["asset-1".to_string()],
            attack_vector: Some("HTTP POST".to_string()),
            prerequisites: vec!["Network access".to_string()],
            impact: "Data exfiltration".to_string(),
            mitigations: vec!["Parameterized queries".to_string()],
            recommendations: vec!["Add input validation".to_string()],
            status: ThreatStatus::Open,
            source_file: Some(PathBuf::from("/src/db.rs")),
            source_line: Some(42),
        };
        let c = t.clone();
        assert_eq!(t.id, c.id);
        assert_eq!(t.category, c.category);
        assert_eq!(t.severity, Severity::High);
        assert_eq!(t.likelihood, Likelihood::Likely);
        assert_eq!(t.affected_assets, c.affected_assets);
        assert_eq!(
            t.source_file.as_deref(),
            Some(std::path::Path::new("/src/db.rs"))
        );
        assert_eq!(t.source_line, Some(42));
        assert_eq!(t.status, ThreatStatus::Open);
    }

    #[test]
    fn threat_with_no_source() {
        let t = Threat {
            id: "T-002".to_string(),
            title: "DoS".to_string(),
            category: StrideCategory::DenialOfService,
            description: String::new(),
            severity: Severity::Medium,
            likelihood: Likelihood::Possible,
            affected_assets: vec![],
            attack_vector: None,
            prerequisites: vec![],
            impact: String::new(),
            mitigations: vec![],
            recommendations: vec![],
            status: ThreatStatus::Mitigated,
            source_file: None,
            source_line: None,
        };
        assert!(t.source_file.is_none());
        assert!(t.source_line.is_none());
        assert!(t.affected_assets.is_empty());
        assert_eq!(t.status, ThreatStatus::Mitigated);
    }

    // ---------------------------------------------------------------------------
    // SecurityControl struct
    // ---------------------------------------------------------------------------

    #[test]
    fn security_control_construction_and_clone() {
        let sc = SecurityControl {
            id: "C-1".to_string(),
            name: "WAF".to_string(),
            control_type: ControlType::Preventive,
            description: "Web Application Firewall".to_string(),
            status: ControlStatus::Implemented,
            effectiveness: 4,
            mitigates: vec!["T-001".to_string()],
            owner: Some("SecOps".to_string()),
            notes: Some("Deployed at edge".to_string()),
        };
        let c = sc.clone();
        assert_eq!(sc.id, c.id);
        assert_eq!(sc.control_type, ControlType::Preventive);
        assert_eq!(sc.status, ControlStatus::Implemented);
        assert_eq!(sc.effectiveness, 4);
        assert_eq!(sc.mitigates, c.mitigates);
        assert_eq!(sc.owner.as_deref(), Some("SecOps"));
    }

    #[test]
    fn security_control_with_no_optional_fields() {
        let sc = SecurityControl {
            id: "C-2".to_string(),
            name: "Logging".to_string(),
            control_type: ControlType::Detective,
            description: String::new(),
            status: ControlStatus::Planned,
            effectiveness: 0,
            mitigates: vec![],
            owner: None,
            notes: None,
        };
        assert!(sc.owner.is_none());
        assert!(sc.notes.is_none());
        assert!(sc.mitigates.is_empty());
    }

    // ---------------------------------------------------------------------------
    // EntryPoint struct
    // ---------------------------------------------------------------------------

    #[test]
    fn entry_point_construction_and_clone() {
        let ep = EntryPoint {
            name: "Public API".to_string(),
            entry_type: EntryPointType::RestApi,
            description: "Public REST endpoint".to_string(),
            trust_level: TrustLevel::Anonymous,
            threats: vec!["T-001".to_string()],
            data_flows: vec!["HTTP request".to_string()],
            requires_auth: false,
        };
        let c = ep.clone();
        assert_eq!(ep.name, c.name);
        assert_eq!(ep.entry_type, EntryPointType::RestApi);
        assert_eq!(ep.trust_level, TrustLevel::Anonymous);
        assert!(!ep.requires_auth);
        assert_eq!(ep.threats, c.threats);
        assert_eq!(ep.data_flows, c.data_flows);
    }

    // ---------------------------------------------------------------------------
    // TrustBoundary struct
    // ---------------------------------------------------------------------------

    #[test]
    fn trust_boundary_construction_and_clone() {
        let tb = TrustBoundary {
            name: "DMZ".to_string(),
            description: "Demilitarized zone".to_string(),
            components: vec!["web-server".to_string(), "api-gw".to_string()],
            internal_trust: TrustLevel::Authenticated,
            external_trust: TrustLevel::Anonymous,
        };
        let c = tb.clone();
        assert_eq!(tb.name, c.name);
        assert_eq!(tb.components, c.components);
        assert_eq!(tb.internal_trust, TrustLevel::Authenticated);
        assert_eq!(tb.external_trust, TrustLevel::Anonymous);
        assert!(tb.internal_trust > tb.external_trust);
    }

    // ---------------------------------------------------------------------------
    // RiskMatrix
    // ---------------------------------------------------------------------------

    #[test]
    fn risk_matrix_new_is_empty() {
        let m = RiskMatrix::new();
        // No threats in any cell
        for sev in [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            for lik in [
                Likelihood::Unlikely,
                Likelihood::Possible,
                Likelihood::Likely,
                Likelihood::AlmostCertain,
            ] {
                assert!(m.threats_at(sev, lik).is_empty());
            }
        }
    }

    #[test]
    fn risk_matrix_default_equals_new() {
        let d = RiskMatrix::default();
        let n = RiskMatrix::new();
        // Both should be empty
        assert!(d
            .threats_at(Severity::Critical, Likelihood::AlmostCertain)
            .is_empty());
        assert!(n
            .threats_at(Severity::Critical, Likelihood::AlmostCertain)
            .is_empty());
    }

    #[test]
    fn risk_matrix_add_and_retrieve_single_threat() {
        let mut m = RiskMatrix::new();
        m.add_threat("T-1", Severity::High, Likelihood::Likely);
        let cell = m.threats_at(Severity::High, Likelihood::Likely);
        assert_eq!(cell.len(), 1);
        assert_eq!(cell[0], "T-1");
    }

    #[test]
    fn risk_matrix_add_multiple_threats_same_cell() {
        let mut m = RiskMatrix::new();
        m.add_threat("T-1", Severity::Critical, Likelihood::AlmostCertain);
        m.add_threat("T-2", Severity::Critical, Likelihood::AlmostCertain);
        m.add_threat("T-3", Severity::Critical, Likelihood::AlmostCertain);
        let cell = m.threats_at(Severity::Critical, Likelihood::AlmostCertain);
        assert_eq!(cell.len(), 3);
        assert_eq!(cell, &["T-1", "T-2", "T-3"]);
    }

    #[test]
    fn risk_matrix_add_threats_different_cells() {
        let mut m = RiskMatrix::new();
        m.add_threat("T-low", Severity::Low, Likelihood::Unlikely);
        m.add_threat("T-high", Severity::High, Likelihood::Possible);
        m.add_threat("T-crit", Severity::Critical, Likelihood::AlmostCertain);

        assert_eq!(
            m.threats_at(Severity::Low, Likelihood::Unlikely),
            &["T-low"]
        );
        assert_eq!(
            m.threats_at(Severity::High, Likelihood::Possible),
            &["T-high"]
        );
        assert_eq!(
            m.threats_at(Severity::Critical, Likelihood::AlmostCertain),
            &["T-crit"]
        );
        // Other cells still empty
        assert!(m
            .threats_at(Severity::Medium, Likelihood::Likely)
            .is_empty());
    }

    #[test]
    fn risk_matrix_threats_at_empty_cell_returns_empty_slice() {
        let m = RiskMatrix::new();
        assert!(m.threats_at(Severity::Low, Likelihood::Unlikely).is_empty());
        assert!(m
            .threats_at(Severity::Critical, Likelihood::AlmostCertain)
            .is_empty());
    }

    #[test]
    fn risk_matrix_add_threat_preserves_insertion_order() {
        let mut m = RiskMatrix::new();
        for i in 0..10 {
            m.add_threat(&format!("T-{}", i), Severity::Medium, Likelihood::Possible);
        }
        let cell = m.threats_at(Severity::Medium, Likelihood::Possible);
        assert_eq!(cell.len(), 10);
        for (i, id) in cell.iter().enumerate() {
            assert_eq!(id, &format!("T-{}", i));
        }
    }

    #[test]
    fn risk_matrix_to_text_contains_headers_and_counts() {
        let mut m = RiskMatrix::new();
        m.add_threat("T-1", Severity::High, Likelihood::Likely);
        m.add_threat("T-2", Severity::High, Likelihood::Likely);
        m.add_threat("T-3", Severity::Critical, Likelihood::AlmostCertain);

        let text = m.to_text();
        assert!(
            text.contains("LIKELIHOOD"),
            "text should contain LIKELIHOOD header"
        );
        assert!(
            text.contains("Unlikely"),
            "text should contain Unlikely column header"
        );
        assert!(
            text.contains("Possible"),
            "text should contain Possible column header"
        );
        assert!(
            text.contains("Likely"),
            "text should contain Likely column header"
        );
        assert!(
            text.contains("Certain"),
            "text should contain Certain column header"
        );
        assert!(
            text.contains("Critical"),
            "text should contain Critical severity row"
        );
        assert!(
            text.contains("High"),
            "text should contain High severity row"
        );
        assert!(
            text.contains("Medium"),
            "text should contain Medium severity row"
        );
        assert!(text.contains("Low"), "text should contain Low severity row");
        // The count 2 should appear for the High/Likely cell
        assert!(
            text.contains("  2 "),
            "text should contain count 2 for High/Likely"
        );
        // The count 1 should appear for the Critical/AlmostCertain cell
        assert!(
            text.contains("  1 "),
            "text should contain count 1 for Critical/Certain"
        );
    }

    #[test]
    fn risk_matrix_to_text_empty_shows_dashes() {
        let m = RiskMatrix::new();
        let text = m.to_text();
        // Empty cells are rendered as "    -    "
        assert!(
            text.contains('-'),
            "empty matrix text should contain dashes for empty cells"
        );
        // No numeric counts should appear
        assert!(
            !text.contains("  1 "),
            "empty matrix should not show count 1"
        );
        assert!(
            !text.contains("  2 "),
            "empty matrix should not show count 2"
        );
    }

    #[test]
    fn risk_matrix_to_text_has_four_severity_rows_and_one_header_block() {
        let m = RiskMatrix::new();
        let text = m.to_text();
        // 4 severity rows each prefixed with " S "
        let row_count = text.lines().filter(|l| l.contains(" S ")).count();
        assert_eq!(row_count, 4, "should have 4 severity rows");
    }

    #[test]
    fn risk_matrix_to_text_nonempty_for_all_cells_filled() {
        let mut m = RiskMatrix::new();
        // Fill every cell with one threat
        for sev in [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            for lik in [
                Likelihood::Unlikely,
                Likelihood::Possible,
                Likelihood::Likely,
                Likelihood::AlmostCertain,
            ] {
                m.add_threat(&format!("T-{:?}-{:?}", sev, lik), sev, lik);
            }
        }
        let text = m.to_text();
        // Every severity row should contain at least one "  1 " count
        for label in ["Critical", "High", "Medium", "Low"] {
            let row_line = text
                .lines()
                .find(|l| l.contains(label))
                .unwrap_or_else(|| panic!("row for {} not found", label));
            assert!(
                row_line.contains("  1 "),
                "row for {} should contain count 1",
                label
            );
        }
    }

    // ---------------------------------------------------------------------------
    // Cross-cutting: Severity x Likelihood used by RiskMatrix
    // ---------------------------------------------------------------------------

    #[test]
    fn risk_matrix_uses_score_pairing_correctly() {
        let mut m = RiskMatrix::new();
        // Severity::High.score() == 3, Likelihood::Possible.score() == 2
        m.add_threat("X", Severity::High, Likelihood::Possible);
        // Retrieving with the same severity/likelihood should find it
        assert_eq!(m.threats_at(Severity::High, Likelihood::Possible), &["X"]);
        // Retrieving with a different severity or likelihood should not
        assert!(m
            .threats_at(Severity::Medium, Likelihood::Possible)
            .is_empty());
        assert!(m
            .threats_at(Severity::High, Likelihood::Unlikely)
            .is_empty());
    }
}
