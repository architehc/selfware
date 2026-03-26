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
