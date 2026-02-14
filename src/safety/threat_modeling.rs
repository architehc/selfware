//! Threat Modeling Assistant
//!
//! STRIDE analysis, attack surface mapping, security architecture review,
//! and risk assessment for software systems.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Attack surface entry point
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

/// The threat model
#[derive(Debug)]
pub struct ThreatModel {
    /// Model name
    pub name: String,
    /// Description
    pub description: String,
    /// Assets
    assets: HashMap<String, Asset>,
    /// Threats
    threats: HashMap<String, Threat>,
    /// Controls
    controls: HashMap<String, SecurityControl>,
    /// Entry points
    entry_points: Vec<EntryPoint>,
    /// Trust boundaries
    trust_boundaries: Vec<TrustBoundary>,
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

/// Risk matrix for visualization
#[derive(Debug)]
pub struct RiskMatrix {
    /// Threats in each cell (severity x likelihood)
    cells: HashMap<(u8, u8), Vec<String>>,
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
            cells: HashMap::new(),
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
            (Severity::Critical, "Critical"),
            (Severity::High, "High    "),
            (Severity::Medium, "Medium  "),
            (Severity::Low, "Low     "),
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

/// STRIDE analyzer
#[derive(Debug)]
pub struct StrideAnalyzer {
    /// Threat patterns
    patterns: HashMap<StrideCategory, Vec<ThreatPattern>>,
}

impl Default for StrideAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl StrideAnalyzer {
    /// Create a new analyzer with default patterns
    pub fn new() -> Self {
        let mut patterns = HashMap::new();

        // Spoofing patterns
        patterns.insert(
            StrideCategory::Spoofing,
            vec![
                ThreatPattern::new(
                    "Missing Authentication",
                    vec!["no auth", "unauthenticated", "anonymous"],
                ),
                ThreatPattern::new(
                    "Weak Authentication",
                    vec!["basic auth", "plaintext password"],
                ),
                ThreatPattern::new("Session Hijacking", vec!["session", "cookie", "token"]),
            ],
        );

        // Tampering patterns
        patterns.insert(
            StrideCategory::Tampering,
            vec![
                ThreatPattern::new(
                    "Missing Input Validation",
                    vec!["user input", "form", "request"],
                ),
                ThreatPattern::new("SQL Injection", vec!["sql", "query", "database"]),
                ThreatPattern::new("Command Injection", vec!["exec", "shell", "command"]),
            ],
        );

        // Repudiation patterns
        patterns.insert(
            StrideCategory::Repudiation,
            vec![
                ThreatPattern::new("Missing Audit Log", vec!["log", "audit", "track"]),
                ThreatPattern::new("No Transaction Records", vec!["transaction", "payment"]),
            ],
        );

        // Information Disclosure patterns
        patterns.insert(
            StrideCategory::InformationDisclosure,
            vec![
                ThreatPattern::new(
                    "Sensitive Data Exposure",
                    vec!["password", "secret", "key", "token"],
                ),
                ThreatPattern::new(
                    "Verbose Error Messages",
                    vec!["error", "exception", "stack trace"],
                ),
                ThreatPattern::new("Information Leakage", vec!["debug", "verbose", "print"]),
            ],
        );

        // Denial of Service patterns
        patterns.insert(
            StrideCategory::DenialOfService,
            vec![
                ThreatPattern::new("Resource Exhaustion", vec!["loop", "memory", "cpu"]),
                ThreatPattern::new("Missing Rate Limiting", vec!["api", "endpoint", "request"]),
            ],
        );

        // Elevation of Privilege patterns
        patterns.insert(
            StrideCategory::ElevationOfPrivilege,
            vec![
                ThreatPattern::new("Missing Authorization", vec!["admin", "role", "permission"]),
                ThreatPattern::new("Privilege Escalation", vec!["sudo", "root", "elevated"]),
            ],
        );

        Self { patterns }
    }

    /// Analyze code for threats
    pub fn analyze(&self, content: &str, file_path: &Path) -> Vec<Threat> {
        let mut threats = Vec::new();
        let lower_content = content.to_lowercase();
        let lines: Vec<&str> = content.lines().collect();

        for (category, category_patterns) in &self.patterns {
            for pattern in category_patterns {
                if pattern.matches(&lower_content) {
                    // Find line numbers where pattern matches
                    for (line_num, line) in lines.iter().enumerate() {
                        let lower_line = line.to_lowercase();
                        if pattern.keywords.iter().any(|kw| lower_line.contains(kw)) {
                            let threat = Threat::new(&pattern.name, *category)
                                .with_description(format!(
                                    "Potential {} vulnerability detected",
                                    pattern.name
                                ))
                                .with_source(file_path.to_path_buf(), line_num + 1);
                            threats.push(threat);
                            break; // One threat per pattern per file
                        }
                    }
                }
            }
        }

        threats
    }

    /// Get patterns for a category
    pub fn get_patterns(&self, category: StrideCategory) -> &[ThreatPattern] {
        self.patterns
            .get(&category)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Threat pattern for detection
#[derive(Debug, Clone)]
pub struct ThreatPattern {
    /// Pattern name
    pub name: String,
    /// Keywords to match
    pub keywords: Vec<String>,
}

impl ThreatPattern {
    /// Create a new pattern
    pub fn new(name: impl Into<String>, keywords: Vec<&str>) -> Self {
        Self {
            name: name.into(),
            keywords: keywords.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Check if pattern matches content
    pub fn matches(&self, content: &str) -> bool {
        self.keywords.iter().any(|kw| content.contains(kw))
    }
}

/// Attack surface mapper
#[derive(Debug)]
pub struct AttackSurfaceMapper {
    /// Entry point detectors
    detectors: Vec<EntryPointDetector>,
}

impl Default for AttackSurfaceMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl AttackSurfaceMapper {
    /// Create a new mapper
    pub fn new() -> Self {
        let detectors = vec![
            EntryPointDetector::new(
                EntryPointType::RestApi,
                vec![
                    "#[get",
                    "#[post",
                    "#[put",
                    "#[delete",
                    "app.get",
                    "app.post",
                    "router.get",
                    "HttpGet",
                    "HttpPost",
                    "@GetMapping",
                    "@PostMapping",
                ],
            ),
            EntryPointDetector::new(
                EntryPointType::GraphQL,
                vec!["graphql", "Query", "Mutation", "Resolver"],
            ),
            EntryPointDetector::new(
                EntryPointType::Database,
                vec!["query", "execute", "SELECT", "INSERT", "UPDATE", "DELETE"],
            ),
            EntryPointDetector::new(
                EntryPointType::FileUpload,
                vec!["upload", "multipart", "file_field", "save_file"],
            ),
            EntryPointDetector::new(
                EntryPointType::Cli,
                vec!["clap", "structopt", "argparse", "cli", "args"],
            ),
        ];

        Self { detectors }
    }

    /// Map attack surface from code
    pub fn map(&self, content: &str) -> Vec<EntryPoint> {
        let mut entry_points = Vec::new();

        for detector in &self.detectors {
            if let Some(entry) = detector.detect(content) {
                entry_points.push(entry);
            }
        }

        entry_points
    }
}

/// Entry point detector
#[derive(Debug)]
pub struct EntryPointDetector {
    /// Entry point type
    entry_type: EntryPointType,
    /// Patterns to detect
    patterns: Vec<String>,
}

impl EntryPointDetector {
    /// Create a new detector
    pub fn new(entry_type: EntryPointType, patterns: Vec<&str>) -> Self {
        Self {
            entry_type,
            patterns: patterns.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Detect entry point
    pub fn detect(&self, content: &str) -> Option<EntryPoint> {
        for pattern in &self.patterns {
            if content.contains(pattern) {
                return Some(EntryPoint::new(
                    format!("{} endpoint", self.entry_type),
                    self.entry_type,
                ));
            }
        }
        None
    }
}

/// Code scanner for security issues
#[derive(Debug)]
pub struct SecurityScanner {
    /// STRIDE analyzer
    stride_analyzer: StrideAnalyzer,
    /// Attack surface mapper
    surface_mapper: AttackSurfaceMapper,
}

impl Default for SecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityScanner {
    /// Create a new scanner
    pub fn new() -> Self {
        Self {
            stride_analyzer: StrideAnalyzer::new(),
            surface_mapper: AttackSurfaceMapper::new(),
        }
    }

    /// Scan a file
    pub fn scan_file(&self, content: &str, file_path: &Path) -> ScanResult {
        let threats = self.stride_analyzer.analyze(content, file_path);
        let entry_points = self.surface_mapper.map(content);

        ScanResult {
            file: file_path.to_path_buf(),
            threats,
            entry_points,
        }
    }
}

/// Scan result
#[derive(Debug)]
pub struct ScanResult {
    /// File scanned
    pub file: PathBuf,
    /// Threats found
    pub threats: Vec<Threat>,
    /// Entry points found
    pub entry_points: Vec<EntryPoint>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_asset_value_clamping() {
        let asset = Asset::new("Test", AssetType::Other)
            .with_value(10)
            .with_sensitivity(0);

        assert_eq!(asset.value, 5); // Clamped to max
        assert_eq!(asset.sensitivity, 1); // Clamped to min
    }

    #[test]
    fn test_entry_point_with_data_flow() {
        let mut entry = EntryPoint::new("API", EntryPointType::RestApi);
        entry.data_flows.push("user input".to_string());

        assert_eq!(entry.data_flows.len(), 1);
    }
}
