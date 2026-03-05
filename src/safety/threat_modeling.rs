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

    // ── StrideCategory comprehensive tests ────────────────────────────────────

    #[test]
    fn test_stride_category_display_all_variants() {
        assert_eq!(format!("{}", StrideCategory::Spoofing), "Spoofing");
        assert_eq!(format!("{}", StrideCategory::Tampering), "Tampering");
        assert_eq!(format!("{}", StrideCategory::Repudiation), "Repudiation");
        assert_eq!(
            format!("{}", StrideCategory::InformationDisclosure),
            "Information Disclosure"
        );
        assert_eq!(
            format!("{}", StrideCategory::DenialOfService),
            "Denial of Service"
        );
        assert_eq!(
            format!("{}", StrideCategory::ElevationOfPrivilege),
            "Elevation of Privilege"
        );
    }

    #[test]
    fn test_stride_category_description_all_variants() {
        assert!(!StrideCategory::Spoofing.description().is_empty());
        assert!(!StrideCategory::Tampering.description().is_empty());
        assert!(!StrideCategory::Repudiation.description().is_empty());
        assert!(!StrideCategory::InformationDisclosure
            .description()
            .is_empty());
        assert!(!StrideCategory::DenialOfService.description().is_empty());
        assert!(!StrideCategory::ElevationOfPrivilege
            .description()
            .is_empty());
    }

    #[test]
    fn test_stride_category_mitigations_all_variants() {
        let spoofing_m = StrideCategory::Spoofing.typical_mitigations();
        assert!(!spoofing_m.is_empty());
        // At least one mitigation mentions authentication for Spoofing
        assert!(spoofing_m.iter().any(|m| m.to_lowercase().contains("auth")));

        let tampering_m = StrideCategory::Tampering.typical_mitigations();
        assert!(!tampering_m.is_empty());

        let repudiation_m = StrideCategory::Repudiation.typical_mitigations();
        assert!(!repudiation_m.is_empty());
        assert!(repudiation_m
            .iter()
            .any(|m| m.to_lowercase().contains("log")));

        let info_m = StrideCategory::InformationDisclosure.typical_mitigations();
        assert!(!info_m.is_empty());
        assert!(info_m.iter().any(|m| m.to_lowercase().contains("encrypt")));

        let dos_m = StrideCategory::DenialOfService.typical_mitigations();
        assert!(!dos_m.is_empty());
        assert!(dos_m.iter().any(|m| m.to_lowercase().contains("rate")));

        let eop_m = StrideCategory::ElevationOfPrivilege.typical_mitigations();
        assert!(!eop_m.is_empty());
        assert!(eop_m.iter().any(|m| m.to_lowercase().contains("privilege")));
    }

    #[test]
    fn test_stride_category_all_contains_all_six_variants() {
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
    fn test_stride_category_equality_and_hash() {
        use std::collections::HashSet;
        let mut set: HashSet<StrideCategory> = HashSet::new();
        set.insert(StrideCategory::Spoofing);
        set.insert(StrideCategory::Spoofing); // duplicate
        set.insert(StrideCategory::Tampering);
        assert_eq!(set.len(), 2);
    }

    // ── Severity comprehensive tests ──────────────────────────────────────────

    #[test]
    fn test_severity_display_all_variants() {
        assert_eq!(format!("{}", Severity::Low), "Low");
        assert_eq!(format!("{}", Severity::Medium), "Medium");
        assert_eq!(format!("{}", Severity::High), "High");
        assert_eq!(format!("{}", Severity::Critical), "Critical");
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
        assert!(Severity::Critical > Severity::Low);
    }

    #[test]
    fn test_severity_from_score_boundary_values() {
        // Score 0 → Low
        assert_eq!(Severity::from_score(0), Severity::Low);
        // Score 1 → Low
        assert_eq!(Severity::from_score(1), Severity::Low);
        // Score 2 → Medium
        assert_eq!(Severity::from_score(2), Severity::Medium);
        // Score 3 → High
        assert_eq!(Severity::from_score(3), Severity::High);
        // Score 4 → Critical (anything >= 4)
        assert_eq!(Severity::from_score(4), Severity::Critical);
        // Score 255 → Critical
        assert_eq!(Severity::from_score(255), Severity::Critical);
    }

    #[test]
    fn test_severity_score_and_from_score_roundtrip() {
        for sev in [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ] {
            assert_eq!(Severity::from_score(sev.score()), sev);
        }
    }

    // ── Likelihood comprehensive tests ────────────────────────────────────────

    #[test]
    fn test_likelihood_display_all_variants() {
        assert_eq!(format!("{}", Likelihood::Unlikely), "Unlikely");
        assert_eq!(format!("{}", Likelihood::Possible), "Possible");
        assert_eq!(format!("{}", Likelihood::Likely), "Likely");
        assert_eq!(format!("{}", Likelihood::AlmostCertain), "Almost Certain");
    }

    #[test]
    fn test_likelihood_ordering() {
        assert!(Likelihood::Unlikely < Likelihood::Possible);
        assert!(Likelihood::Possible < Likelihood::Likely);
        assert!(Likelihood::Likely < Likelihood::AlmostCertain);
    }

    #[test]
    fn test_likelihood_from_score_boundary_values() {
        assert_eq!(Likelihood::from_score(0), Likelihood::Unlikely);
        assert_eq!(Likelihood::from_score(1), Likelihood::Unlikely);
        assert_eq!(Likelihood::from_score(2), Likelihood::Possible);
        assert_eq!(Likelihood::from_score(3), Likelihood::Likely);
        assert_eq!(Likelihood::from_score(4), Likelihood::AlmostCertain);
        assert_eq!(Likelihood::from_score(200), Likelihood::AlmostCertain);
    }

    #[test]
    fn test_likelihood_score_and_from_score_roundtrip() {
        for lik in [
            Likelihood::Unlikely,
            Likelihood::Possible,
            Likelihood::Likely,
            Likelihood::AlmostCertain,
        ] {
            assert_eq!(Likelihood::from_score(lik.score()), lik);
        }
    }

    // ── AssetType comprehensive tests ─────────────────────────────────────────

    #[test]
    fn test_asset_type_display_all_variants() {
        assert_eq!(format!("{}", AssetType::UserData), "User Data");
        assert_eq!(format!("{}", AssetType::Credentials), "Credentials");
        assert_eq!(format!("{}", AssetType::ApiKeys), "API Keys");
        assert_eq!(format!("{}", AssetType::Configuration), "Configuration");
        assert_eq!(format!("{}", AssetType::SourceCode), "Source Code");
        assert_eq!(format!("{}", AssetType::Infrastructure), "Infrastructure");
        assert_eq!(format!("{}", AssetType::FinancialData), "Financial Data");
        assert_eq!(
            format!("{}", AssetType::IntellectualProperty),
            "Intellectual Property"
        );
        assert_eq!(format!("{}", AssetType::Availability), "Availability");
        assert_eq!(format!("{}", AssetType::Other), "Other");
    }

    // ── Asset builder comprehensive tests ─────────────────────────────────────

    #[test]
    fn test_asset_builder_with_location_and_owner() {
        let asset = Asset::new("API Secret", AssetType::ApiKeys)
            .with_location("secrets-manager")
            .with_owner("platform-team");

        assert_eq!(asset.location.as_deref(), Some("secrets-manager"));
        assert_eq!(asset.owner.as_deref(), Some("platform-team"));
    }

    #[test]
    fn test_asset_defaults() {
        let asset = Asset::new("Unnamed", AssetType::Other);
        assert_eq!(asset.value, 3);
        assert_eq!(asset.sensitivity, 3);
        assert!(asset.location.is_none());
        assert!(asset.owner.is_none());
        assert!(asset.classification.is_none());
        assert!(asset.description.is_empty());
    }

    #[test]
    fn test_asset_risk_score_boundary_values() {
        // Minimum value and sensitivity → floor(1+1)/2 = 1
        let low_asset = Asset::new("Low", AssetType::Other)
            .with_value(1)
            .with_sensitivity(1);
        assert_eq!(low_asset.risk_score(), 1);

        // Maximum value and sensitivity → floor(5+5)/2 = 5
        let high_asset = Asset::new("High", AssetType::Other)
            .with_value(5)
            .with_sensitivity(5);
        assert_eq!(high_asset.risk_score(), 5);

        // Mixed: (4+2)/2 = 3
        let mixed_asset = Asset::new("Mixed", AssetType::Other)
            .with_value(4)
            .with_sensitivity(2);
        assert_eq!(mixed_asset.risk_score(), 3);
    }

    #[test]
    fn test_asset_value_clamped_to_min_1() {
        let asset = Asset::new("Test", AssetType::Other).with_value(0);
        assert_eq!(asset.value, 1);
    }

    #[test]
    fn test_asset_sensitivity_clamped_to_max_5() {
        let asset = Asset::new("Test", AssetType::Other).with_sensitivity(99);
        assert_eq!(asset.sensitivity, 5);
    }

    #[test]
    fn test_asset_all_types_can_be_created() {
        let types = [
            AssetType::UserData,
            AssetType::Credentials,
            AssetType::ApiKeys,
            AssetType::Configuration,
            AssetType::SourceCode,
            AssetType::Infrastructure,
            AssetType::FinancialData,
            AssetType::IntellectualProperty,
            AssetType::Availability,
            AssetType::Other,
        ];
        for t in types {
            let asset = Asset::new("test", t);
            assert_eq!(asset.asset_type, t);
        }
    }

    // ── Threat builder comprehensive tests ────────────────────────────────────

    #[test]
    fn test_threat_defaults() {
        let threat = Threat::new("Default Threat", StrideCategory::Spoofing);
        assert_eq!(threat.severity, Severity::Medium);
        assert_eq!(threat.likelihood, Likelihood::Possible);
        assert_eq!(threat.status, ThreatStatus::Open);
        assert!(threat.description.is_empty());
        assert!(threat.impact.is_empty());
        assert!(threat.affected_assets.is_empty());
        assert!(threat.prerequisites.is_empty());
        assert!(threat.mitigations.is_empty());
        assert!(threat.recommendations.is_empty());
        assert!(threat.attack_vector.is_none());
        assert!(threat.source_file.is_none());
        assert!(threat.source_line.is_none());
    }

    #[test]
    fn test_threat_with_multiple_assets_and_mitigations() {
        let threat = Threat::new("Multi-asset", StrideCategory::InformationDisclosure)
            .with_affected_asset("asset-1")
            .with_affected_asset("asset-2")
            .with_affected_asset("asset-3")
            .with_mitigation("Encrypt at rest")
            .with_mitigation("Encrypt in transit")
            .with_recommendation("Use TLS 1.3")
            .with_recommendation("Rotate keys quarterly");

        assert_eq!(threat.affected_assets.len(), 3);
        assert_eq!(threat.mitigations.len(), 2);
        assert_eq!(threat.recommendations.len(), 2);
    }

    #[test]
    fn test_threat_risk_score_max() {
        // Critical × AlmostCertain = 4×4 = 16 (max possible)
        let threat = Threat::new("Worst Case", StrideCategory::ElevationOfPrivilege)
            .with_severity(Severity::Critical)
            .with_likelihood(Likelihood::AlmostCertain);
        assert_eq!(threat.risk_score(), 16);
        assert_eq!(threat.risk_level(), RiskLevel::Critical);
    }

    #[test]
    fn test_threat_risk_score_min() {
        // Low × Unlikely = 1×1 = 1
        let threat = Threat::new("Best Case", StrideCategory::Repudiation)
            .with_severity(Severity::Low)
            .with_likelihood(Likelihood::Unlikely);
        assert_eq!(threat.risk_score(), 1);
        assert_eq!(threat.risk_level(), RiskLevel::Low);
    }

    #[test]
    fn test_threat_risk_score_is_capped_at_16() {
        // The min() call ensures the product never exceeds 16
        let threat = Threat::new("Cap Test", StrideCategory::Tampering)
            .with_severity(Severity::Critical) // 4
            .with_likelihood(Likelihood::AlmostCertain); // 4 → 4*4 = 16
        assert!(threat.risk_score() <= 16);
    }

    #[test]
    fn test_threat_risk_level_moderate_boundary() {
        // score 4 → Moderate
        let threat = Threat::new("Moderate", StrideCategory::Spoofing)
            .with_severity(Severity::Medium) // 2
            .with_likelihood(Likelihood::Possible); // 2 → 2*2=4
        assert_eq!(threat.risk_score(), 4);
        assert_eq!(threat.risk_level(), RiskLevel::Moderate);
    }

    #[test]
    fn test_threat_risk_level_high_boundary() {
        // score 7 → High (Medium=2 * Likely=3 = 6 → Moderate; High=3 * Possible=2 = 6 → Moderate)
        // High=3 * Likely=3 = 9 → High
        let threat = Threat::new("High Risk", StrideCategory::Tampering)
            .with_severity(Severity::High) // 3
            .with_likelihood(Likelihood::Likely); // 3 → 9
        assert_eq!(threat.risk_score(), 9);
        assert_eq!(threat.risk_level(), RiskLevel::High);
    }

    #[test]
    fn test_threat_with_source_file() {
        let path = PathBuf::from("/src/api/handlers.rs");
        let threat =
            Threat::new("File Threat", StrideCategory::Tampering).with_source(path.clone(), 100);

        assert_eq!(threat.source_file.as_deref(), Some(path.as_path()));
        assert_eq!(threat.source_line, Some(100));
    }

    #[test]
    fn test_threat_all_categories_can_be_created() {
        let categories = StrideCategory::all();
        for cat in categories {
            let threat = Threat::new("Test", cat);
            assert_eq!(threat.category, cat);
        }
    }

    // ── ThreatStatus comprehensive tests ─────────────────────────────────────

    #[test]
    fn test_threat_status_display_all_variants() {
        assert_eq!(format!("{}", ThreatStatus::Open), "Open");
        assert_eq!(format!("{}", ThreatStatus::Mitigated), "Mitigated");
        assert_eq!(format!("{}", ThreatStatus::Accepted), "Accepted");
        assert_eq!(format!("{}", ThreatStatus::Transferred), "Transferred");
        assert_eq!(format!("{}", ThreatStatus::Closed), "Closed");
    }

    #[test]
    fn test_threat_status_all_variants_can_be_set() {
        let statuses = [
            ThreatStatus::Open,
            ThreatStatus::Mitigated,
            ThreatStatus::Accepted,
            ThreatStatus::Transferred,
            ThreatStatus::Closed,
        ];
        for s in statuses {
            let mut t = Threat::new("test", StrideCategory::Spoofing);
            t.status = s;
            assert_eq!(t.status, s);
        }
    }

    // ── RiskLevel comprehensive tests ─────────────────────────────────────────

    #[test]
    fn test_risk_level_display_all_variants() {
        assert_eq!(format!("{}", RiskLevel::Low), "Low");
        assert_eq!(format!("{}", RiskLevel::Moderate), "Moderate");
        assert_eq!(format!("{}", RiskLevel::High), "High");
        assert_eq!(format!("{}", RiskLevel::Critical), "Critical");
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Low < RiskLevel::Moderate);
        assert!(RiskLevel::Moderate < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_from_score_all_boundaries() {
        // 0 and 1-3 → Low
        assert_eq!(RiskLevel::from_score(0), RiskLevel::Low);
        assert_eq!(RiskLevel::from_score(3), RiskLevel::Low);
        // 4-6 → Moderate
        assert_eq!(RiskLevel::from_score(4), RiskLevel::Moderate);
        assert_eq!(RiskLevel::from_score(6), RiskLevel::Moderate);
        // 7-11 → High
        assert_eq!(RiskLevel::from_score(7), RiskLevel::High);
        assert_eq!(RiskLevel::from_score(11), RiskLevel::High);
        // 12+ → Critical
        assert_eq!(RiskLevel::from_score(12), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(16), RiskLevel::Critical);
        assert_eq!(RiskLevel::from_score(255), RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_score_range_all_variants() {
        assert_eq!(RiskLevel::Low.score_range(), (1, 3));
        assert_eq!(RiskLevel::Moderate.score_range(), (4, 6));
        assert_eq!(RiskLevel::High.score_range(), (7, 11));
        assert_eq!(RiskLevel::Critical.score_range(), (12, 16));
    }

    // ── SecurityControl builder comprehensive tests ───────────────────────────

    #[test]
    fn test_security_control_defaults() {
        let ctrl = SecurityControl::new("MFA", ControlType::Preventive);
        assert_eq!(ctrl.name, "MFA");
        assert_eq!(ctrl.status, ControlStatus::Planned);
        assert_eq!(ctrl.effectiveness, 3);
        assert!(ctrl.description.is_empty());
        assert!(ctrl.mitigates.is_empty());
        assert!(ctrl.owner.is_none());
        assert!(ctrl.notes.is_none());
    }

    #[test]
    fn test_security_control_builder_all_methods() {
        let ctrl = SecurityControl::new("Rate Limiter", ControlType::Preventive)
            .with_description("Limits requests per IP")
            .with_status(ControlStatus::Implemented)
            .with_effectiveness(5)
            .mitigates_threat("threat-42")
            .mitigates_threat("threat-99")
            .with_owner("platform");

        assert_eq!(ctrl.description, "Limits requests per IP");
        assert_eq!(ctrl.status, ControlStatus::Implemented);
        assert_eq!(ctrl.effectiveness, 5);
        assert_eq!(ctrl.mitigates, vec!["threat-42", "threat-99"]);
        assert_eq!(ctrl.owner.as_deref(), Some("platform"));
    }

    #[test]
    fn test_security_control_effectiveness_clamped() {
        let lo = SecurityControl::new("Lo", ControlType::Detective).with_effectiveness(0);
        assert_eq!(lo.effectiveness, 1);

        let hi = SecurityControl::new("Hi", ControlType::Detective).with_effectiveness(10);
        assert_eq!(hi.effectiveness, 5);
    }

    #[test]
    fn test_control_type_display_all_variants() {
        assert_eq!(format!("{}", ControlType::Preventive), "Preventive");
        assert_eq!(format!("{}", ControlType::Detective), "Detective");
        assert_eq!(format!("{}", ControlType::Corrective), "Corrective");
        assert_eq!(format!("{}", ControlType::Deterrent), "Deterrent");
        assert_eq!(format!("{}", ControlType::Compensating), "Compensating");
    }

    #[test]
    fn test_control_status_display_all_variants() {
        assert_eq!(format!("{}", ControlStatus::Planned), "Planned");
        assert_eq!(format!("{}", ControlStatus::Partial), "Partial");
        assert_eq!(format!("{}", ControlStatus::Implemented), "Implemented");
        assert_eq!(format!("{}", ControlStatus::NotApplicable), "N/A");
    }

    #[test]
    fn test_all_control_types_can_be_created() {
        let types = [
            ControlType::Preventive,
            ControlType::Detective,
            ControlType::Corrective,
            ControlType::Deterrent,
            ControlType::Compensating,
        ];
        for ct in types {
            let ctrl = SecurityControl::new("test", ct);
            assert_eq!(ctrl.control_type, ct);
        }
    }

    // ── EntryPoint builder comprehensive tests ────────────────────────────────

    #[test]
    fn test_entry_point_defaults() {
        let ep = EntryPoint::new("CLI", EntryPointType::Cli);
        assert_eq!(ep.trust_level, TrustLevel::Anonymous);
        assert!(!ep.requires_auth);
        assert!(ep.threats.is_empty());
        assert!(ep.data_flows.is_empty());
        assert!(ep.description.is_empty());
    }

    #[test]
    fn test_entry_point_builder_with_threat() {
        let ep = EntryPoint::new("/upload", EntryPointType::FileUpload)
            .with_description("File upload endpoint")
            .with_trust_level(TrustLevel::Authenticated)
            .with_threat("threat-1")
            .with_threat("threat-2")
            .requires_authentication();

        assert_eq!(ep.description, "File upload endpoint");
        assert_eq!(ep.trust_level, TrustLevel::Authenticated);
        assert_eq!(ep.threats, vec!["threat-1", "threat-2"]);
        assert!(ep.requires_auth);
    }

    #[test]
    fn test_entry_point_type_display_all_variants() {
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
        assert_eq!(
            format!("{}", EntryPointType::UserInterface),
            "User Interface"
        );
        assert_eq!(format!("{}", EntryPointType::Other), "Other");
    }

    // ── TrustLevel comprehensive tests ────────────────────────────────────────

    #[test]
    fn test_trust_level_display_all_variants() {
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

    // ── TrustBoundary builder comprehensive tests ─────────────────────────────

    #[test]
    fn test_trust_boundary_defaults() {
        let tb = TrustBoundary::new("Corp Network");
        assert_eq!(tb.name, "Corp Network");
        assert!(tb.description.is_empty());
        assert!(tb.components.is_empty());
        assert_eq!(tb.internal_trust, TrustLevel::System);
        assert_eq!(tb.external_trust, TrustLevel::Anonymous);
    }

    #[test]
    fn test_trust_boundary_with_multiple_components() {
        let tb = TrustBoundary::new("DMZ")
            .with_description("Demilitarized zone")
            .with_component("Web Server")
            .with_component("Load Balancer")
            .with_component("WAF")
            .with_trust_levels(TrustLevel::Privileged, TrustLevel::Anonymous);

        assert_eq!(tb.description, "Demilitarized zone");
        assert_eq!(tb.components.len(), 3);
        assert!(tb.components.contains(&"Web Server".to_string()));
        assert_eq!(tb.internal_trust, TrustLevel::Privileged);
        assert_eq!(tb.external_trust, TrustLevel::Anonymous);
    }

    // ── RiskMatrix comprehensive tests ────────────────────────────────────────

    #[test]
    fn test_risk_matrix_default() {
        let matrix = RiskMatrix::default();
        // All cells empty
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
                assert!(matrix.threats_at(sev, lik).is_empty());
            }
        }
    }

    #[test]
    fn test_risk_matrix_all_cells() {
        let mut matrix = RiskMatrix::new();

        // Populate all 16 cells
        let severities = [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ];
        let likelihoods = [
            Likelihood::Unlikely,
            Likelihood::Possible,
            Likelihood::Likely,
            Likelihood::AlmostCertain,
        ];

        let mut idx = 0u32;
        for sev in severities {
            for lik in likelihoods {
                matrix.add_threat(&format!("t{}", idx), sev, lik);
                idx += 1;
            }
        }

        // Each cell should have exactly one threat
        for sev in severities {
            for lik in likelihoods {
                assert_eq!(
                    matrix.threats_at(sev, lik).len(),
                    1,
                    "Expected 1 threat at {:?} x {:?}",
                    sev,
                    lik
                );
            }
        }
    }

    #[test]
    fn test_risk_matrix_empty_cell_returns_empty_slice() {
        let matrix = RiskMatrix::new();
        let result = matrix.threats_at(Severity::Critical, Likelihood::AlmostCertain);
        assert!(result.is_empty());
    }

    #[test]
    fn test_risk_matrix_to_text_contains_headers() {
        let matrix = RiskMatrix::new();
        let text = matrix.to_text();
        assert!(text.contains("LIKELIHOOD"));
        assert!(text.contains("Unlikely"));
        assert!(text.contains("Possible"));
        assert!(text.contains("Likely"));
        assert!(text.contains("Certain"));
    }

    #[test]
    fn test_risk_matrix_to_text_shows_threat_counts() {
        let mut matrix = RiskMatrix::new();
        matrix.add_threat("a", Severity::Critical, Likelihood::AlmostCertain);
        matrix.add_threat("b", Severity::Critical, Likelihood::AlmostCertain);
        matrix.add_threat("c", Severity::Critical, Likelihood::AlmostCertain);

        let text = matrix.to_text();
        // The count "3" should appear in the rendered table
        assert!(text.contains("3"));
    }

    // ── ThreatModel comprehensive tests ──────────────────────────────────────

    #[test]
    fn test_threat_model_empty_overall_risk_score() {
        let model = ThreatModel::new("Empty");
        assert_eq!(model.overall_risk_score(), 0.0);
    }

    #[test]
    fn test_threat_model_overall_risk_score_excludes_mitigated() {
        let mut model = ThreatModel::new("Test");

        // Add a high-risk open threat
        model.add_threat(
            Threat::new("Open High", StrideCategory::Tampering)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );

        // Add a mitigated threat (should not affect score)
        let mid = model.add_threat(
            Threat::new("Mitigated", StrideCategory::Spoofing)
                .with_severity(Severity::Low)
                .with_likelihood(Likelihood::Unlikely),
        );
        model.get_threat_mut(&mid).unwrap().status = ThreatStatus::Mitigated;

        let score = model.overall_risk_score();
        // Only the open Critical×AlmostCertain threat contributes: 4*4 = 16
        assert!((score - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_threat_model_overall_risk_score_average() {
        let mut model = ThreatModel::new("Avg Test");

        // Low × Unlikely = 1
        model.add_threat(
            Threat::new("T1", StrideCategory::Spoofing)
                .with_severity(Severity::Low)
                .with_likelihood(Likelihood::Unlikely),
        );
        // High × Likely = 3*3 = 9
        model.add_threat(
            Threat::new("T2", StrideCategory::Tampering)
                .with_severity(Severity::High)
                .with_likelihood(Likelihood::Likely),
        );

        // Average = (1 + 9) / 2 = 5.0
        let score = model.overall_risk_score();
        assert!((score - 5.0).abs() < 0.01, "Expected 5.0, got {}", score);
    }

    #[test]
    fn test_threat_model_risk_distribution_only_counts_open() {
        let mut model = ThreatModel::new("Test");

        // Open critical
        model.add_threat(
            Threat::new("C", StrideCategory::Tampering)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );

        // Accepted – should not appear in distribution
        let tid = model.add_threat(
            Threat::new("A", StrideCategory::Spoofing)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );
        model.get_threat_mut(&tid).unwrap().status = ThreatStatus::Accepted;

        let dist = model.risk_distribution();
        let total: usize = dist.values().sum();
        assert_eq!(total, 1, "Only open threats should be in distribution");
    }

    #[test]
    fn test_threat_model_threats_by_status_all_variants() {
        let mut model = ThreatModel::new("Status Test");

        let statuses = [
            ThreatStatus::Open,
            ThreatStatus::Mitigated,
            ThreatStatus::Accepted,
            ThreatStatus::Transferred,
            ThreatStatus::Closed,
        ];

        for (i, s) in statuses.iter().enumerate() {
            let id = model.add_threat(Threat::new(format!("T{}", i), StrideCategory::Spoofing));
            model.get_threat_mut(&id).unwrap().status = *s;
        }

        for s in statuses {
            assert_eq!(
                model.threats_by_status(s).len(),
                1,
                "Status {:?} should have 1 threat",
                s
            );
        }
    }

    #[test]
    fn test_threat_model_critical_threats_excludes_non_open() {
        let mut model = ThreatModel::new("Critical Test");

        // Open Critical-risk → included
        model.add_threat(
            Threat::new("OpenCrit", StrideCategory::ElevationOfPrivilege)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );

        // Mitigated Critical-risk → excluded
        let id = model.add_threat(
            Threat::new("MitigatedCrit", StrideCategory::ElevationOfPrivilege)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );
        model.get_threat_mut(&id).unwrap().status = ThreatStatus::Mitigated;

        assert_eq!(model.critical_threats().len(), 1);
    }

    #[test]
    fn test_threat_model_add_and_get_control() {
        let mut model = ThreatModel::new("Controls Test");

        let ctrl = SecurityControl::new("Firewall", ControlType::Preventive)
            .with_status(ControlStatus::Implemented);
        let id = model.add_control(ctrl);

        let retrieved = model.get_control(&id).expect("Control should be found");
        assert_eq!(retrieved.name, "Firewall");
        assert_eq!(retrieved.status, ControlStatus::Implemented);
    }

    #[test]
    fn test_threat_model_get_control_nonexistent() {
        let model = ThreatModel::new("Empty");
        assert!(model.get_control("nonexistent-id").is_none());
    }

    #[test]
    fn test_threat_model_get_asset_nonexistent() {
        let model = ThreatModel::new("Empty");
        assert!(model.get_asset("nonexistent-id").is_none());
    }

    #[test]
    fn test_threat_model_get_threat_nonexistent() {
        let model = ThreatModel::new("Empty");
        assert!(model.get_threat("nonexistent-id").is_none());
    }

    #[test]
    fn test_threat_model_iterators() {
        let mut model = ThreatModel::new("Iter Test");

        model.add_asset(Asset::new("A1", AssetType::UserData));
        model.add_asset(Asset::new("A2", AssetType::Credentials));

        model.add_threat(Threat::new("T1", StrideCategory::Spoofing));
        model.add_threat(Threat::new("T2", StrideCategory::Tampering));
        model.add_threat(Threat::new("T3", StrideCategory::Repudiation));

        model.add_control(SecurityControl::new("C1", ControlType::Preventive));

        let asset_count: usize = model.assets().count();
        let threat_count: usize = model.threats().count();
        let control_count: usize = model.controls().count();

        assert_eq!(asset_count, 2);
        assert_eq!(threat_count, 3);
        assert_eq!(control_count, 1);
    }

    #[test]
    fn test_threat_model_generate_risk_matrix() {
        let mut model = ThreatModel::new("Matrix Test");

        model.add_threat(
            Threat::new("H-L", StrideCategory::Tampering)
                .with_severity(Severity::High)
                .with_likelihood(Likelihood::Likely),
        );
        // Mitigated → should not appear in matrix
        let mid = model.add_threat(
            Threat::new("C-AC", StrideCategory::Spoofing)
                .with_severity(Severity::Critical)
                .with_likelihood(Likelihood::AlmostCertain),
        );
        model.get_threat_mut(&mid).unwrap().status = ThreatStatus::Mitigated;

        let matrix = model.generate_risk_matrix();

        // Open threat is at High × Likely
        assert_eq!(
            matrix.threats_at(Severity::High, Likelihood::Likely).len(),
            1
        );
        // Mitigated threat not in matrix
        assert!(matrix
            .threats_at(Severity::Critical, Likelihood::AlmostCertain)
            .is_empty());
    }

    #[test]
    fn test_threat_model_report_contains_all_sections() {
        let mut model =
            ThreatModel::new("Full Report").with_description("Comprehensive system threat model");

        let asset_id = model.add_asset(
            Asset::new("User DB", AssetType::UserData)
                .with_description("Stores all user PII")
                .with_value(5)
                .with_sensitivity(5),
        );

        model.add_threat(
            Threat::new("SQL Injection", StrideCategory::Tampering)
                .with_severity(Severity::High)
                .with_likelihood(Likelihood::Likely)
                .with_affected_asset(&asset_id),
        );

        model.add_control(
            SecurityControl::new("Parameterized Queries", ControlType::Preventive)
                .with_status(ControlStatus::Implemented)
                .with_description("All DB calls use prepared statements"),
        );

        model.add_entry_point(
            EntryPoint::new("/api/data", EntryPointType::RestApi)
                .with_trust_level(TrustLevel::Authenticated)
                .requires_authentication(),
        );

        let report = model.generate_report();

        assert!(report.contains("# Threat Model: Full Report"));
        assert!(report.contains("Comprehensive system threat model"));
        assert!(report.contains("## Executive Summary"));
        assert!(report.contains("Total Threats"));
        assert!(report.contains("Open Threats"));
        assert!(report.contains("Critical Threats"));
        assert!(report.contains("Overall Risk Score"));
        assert!(report.contains("## Risk Distribution"));
        assert!(report.contains("## Assets"));
        assert!(report.contains("User DB"));
        assert!(report.contains("## Threats by STRIDE Category"));
        assert!(report.contains("SQL Injection"));
        assert!(report.contains("## Security Controls"));
        assert!(report.contains("Parameterized Queries"));
        assert!(report.contains("## Attack Surface"));
        assert!(report.contains("/api/data"));
    }

    #[test]
    fn test_threat_model_report_empty_model() {
        let model = ThreatModel::new("Empty App");
        let report = model.generate_report();

        assert!(report.contains("# Threat Model: Empty App"));
        assert!(report.contains("Total Threats**: 0"));
        assert!(report.contains("Open Threats**: 0"));
        assert!(report.contains("Critical Threats**: 0"));
        assert!(report.contains("Overall Risk Score**: 0.0"));
    }

    #[test]
    fn test_threat_model_stride_coverage_all_zeros_when_empty() {
        let model = ThreatModel::new("Empty");
        let coverage = model.stride_coverage();
        assert_eq!(coverage.len(), 6);
        for (_, count) in &coverage {
            assert_eq!(*count, 0);
        }
    }

    #[test]
    fn test_threat_model_stride_coverage_all_categories() {
        let mut model = ThreatModel::new("STRIDE All");

        for cat in StrideCategory::all() {
            model.add_threat(Threat::new("T", cat));
        }

        let coverage = model.stride_coverage();
        for cat in StrideCategory::all() {
            assert_eq!(
                *coverage.get(&cat).unwrap(),
                1,
                "Category {:?} should have 1 threat",
                cat
            );
        }
    }

    #[test]
    fn test_threat_model_multiple_entry_points_and_boundaries() {
        let mut model = ThreatModel::new("Multi-surface");

        model.add_entry_point(EntryPoint::new("REST", EntryPointType::RestApi));
        model.add_entry_point(EntryPoint::new("GQL", EntryPointType::GraphQL));
        model.add_entry_point(EntryPoint::new("WS", EntryPointType::WebSocket));

        model.add_trust_boundary(TrustBoundary::new("LAN"));
        model.add_trust_boundary(TrustBoundary::new("DMZ"));

        assert_eq!(model.entry_points().len(), 3);
        assert_eq!(model.trust_boundaries().len(), 2);
    }

    // ── StrideAnalyzer comprehensive tests ───────────────────────────────────

    #[test]
    fn test_stride_analyzer_default_equals_new() {
        let a1 = StrideAnalyzer::new();
        let a2 = StrideAnalyzer::default();

        // Both should have patterns for all 6 categories
        for cat in StrideCategory::all() {
            let p1 = a1.get_patterns(cat);
            let p2 = a2.get_patterns(cat);
            assert_eq!(p1.len(), p2.len());
        }
    }

    #[test]
    fn test_stride_analyzer_has_patterns_for_all_categories() {
        let analyzer = StrideAnalyzer::new();

        for cat in StrideCategory::all() {
            let patterns = analyzer.get_patterns(cat);
            assert!(!patterns.is_empty(), "Expected patterns for {:?}", cat);
        }
    }

    #[test]
    fn test_stride_analyzer_empty_content_produces_no_threats() {
        let analyzer = StrideAnalyzer::new();
        let threats = analyzer.analyze("", &PathBuf::from("empty.rs"));
        assert!(threats.is_empty());
    }

    #[test]
    fn test_stride_analyzer_no_matching_content_produces_no_threats() {
        let analyzer = StrideAnalyzer::new();
        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let threats = analyzer.analyze(code, &PathBuf::from("math.rs"));
        assert!(threats.is_empty());
    }

    #[test]
    fn test_stride_analyzer_detects_spoofing() {
        let analyzer = StrideAnalyzer::new();
        // Contains "no auth" pattern keyword
        let code = "// This endpoint is unauthenticated and accepts anonymous connections";
        let threats = analyzer.analyze(code, &PathBuf::from("auth.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::Spoofing),
            "Expected Spoofing threat"
        );
    }

    #[test]
    fn test_stride_analyzer_detects_tampering() {
        let analyzer = StrideAnalyzer::new();
        let code = r#"
fn get_user(db: &Db, id: &str) {
    let query = format!("SELECT * FROM users WHERE id = '{}'", id);
    db.execute(&query)
}
"#;
        let threats = analyzer.analyze(code, &PathBuf::from("db.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::Tampering),
            "Expected Tampering threat (SQL pattern)"
        );
    }

    #[test]
    fn test_stride_analyzer_detects_repudiation() {
        let analyzer = StrideAnalyzer::new();
        let code = "// Missing audit log for transaction records. No log or track of payments.";
        let threats = analyzer.analyze(code, &PathBuf::from("payments.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::Repudiation),
            "Expected Repudiation threat"
        );
    }

    #[test]
    fn test_stride_analyzer_detects_information_disclosure() {
        let analyzer = StrideAnalyzer::new();
        let code = r#"
fn debug_handler(err: &Error) {
    println!("Error: {:?} stack trace: {}", err, err.backtrace());
}
"#;
        let threats = analyzer.analyze(code, &PathBuf::from("handler.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::InformationDisclosure),
            "Expected InformationDisclosure threat"
        );
    }

    #[test]
    fn test_stride_analyzer_detects_denial_of_service() {
        let analyzer = StrideAnalyzer::new();
        let code = r#"
// No rate limiting on this api endpoint request handler
fn handle_request(req: Request) {
    loop {
        process(req);
    }
}
"#;
        let threats = analyzer.analyze(code, &PathBuf::from("server.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::DenialOfService),
            "Expected DenialOfService threat"
        );
    }

    #[test]
    fn test_stride_analyzer_detects_elevation_of_privilege() {
        let analyzer = StrideAnalyzer::new();
        let code = r#"
// No permission check for admin role
fn delete_user(caller_id: u64, target_id: u64) {
    db.delete(target_id);
}
"#;
        let threats = analyzer.analyze(code, &PathBuf::from("admin.rs"));
        assert!(
            threats
                .iter()
                .any(|t| t.category == StrideCategory::ElevationOfPrivilege),
            "Expected ElevationOfPrivilege threat"
        );
    }

    #[test]
    fn test_stride_analyzer_threat_has_source_file() {
        let analyzer = StrideAnalyzer::new();
        let code = "let query = format!(\"SELECT * FROM users WHERE id = {}\", id);";
        let path = PathBuf::from("src/db.rs");
        let threats = analyzer.analyze(code, &path);

        assert!(!threats.is_empty());
        for threat in &threats {
            assert!(
                threat.source_file.is_some(),
                "Threat should have source file"
            );
        }
    }

    #[test]
    fn test_stride_analyzer_one_threat_per_pattern_per_file() {
        let analyzer = StrideAnalyzer::new();
        // Multiple lines contain "sql"/"query" keywords, but the analyzer breaks after the
        // first matching line — so exactly one Tampering/SQL threat is produced per file.
        let code = r#"
let q1 = format!("sql query: SELECT * FROM users WHERE id = {}", id);
let q2 = format!("sql query: SELECT * FROM orders WHERE user_id = {}", uid);
let q3 = "another sql query here";
"#;
        let threats = analyzer.analyze(code, &PathBuf::from("db.rs"));

        // Count threats for Tampering/SQL Injection category
        let tampering: Vec<_> = threats
            .iter()
            .filter(|t| t.category == StrideCategory::Tampering && t.title.contains("SQL"))
            .collect();

        // Should be exactly 1 threat for "SQL Injection" pattern (break after first matching line)
        assert_eq!(tampering.len(), 1, "Only one SQL Injection threat per file");
    }

    // ── ThreatPattern comprehensive tests ────────────────────────────────────

    #[test]
    fn test_threat_pattern_empty_keywords_never_matches() {
        let pattern = ThreatPattern::new("Empty", vec![]);
        assert!(!pattern.matches("sql query password token"));
        assert!(!pattern.matches(""));
    }

    #[test]
    fn test_threat_pattern_case_sensitivity() {
        let pattern = ThreatPattern::new("SQL", vec!["sql"]);
        // matches() checks the content as-is (not lowercased)
        assert!(pattern.matches("sql query"));
        // Uppercase SQL will not match lowercase "sql" keyword
        assert!(!pattern.matches("SQL QUERY"));
    }

    #[test]
    fn test_threat_pattern_multiple_keywords_any_matches() {
        let pattern = ThreatPattern::new("Auth", vec!["token", "session", "cookie"]);
        assert!(pattern.matches("use session here"));
        assert!(pattern.matches("save the cookie"));
        assert!(pattern.matches("bearer token"));
        assert!(!pattern.matches("no sensitive words here whatsoever"));
    }

    #[test]
    fn test_threat_pattern_name_and_keywords_stored_correctly() {
        let pattern = ThreatPattern::new("My Pattern", vec!["alpha", "beta", "gamma"]);
        assert_eq!(pattern.name, "My Pattern");
        assert_eq!(pattern.keywords, vec!["alpha", "beta", "gamma"]);
    }

    // ── AttackSurfaceMapper comprehensive tests ───────────────────────────────

    #[test]
    fn test_attack_surface_mapper_default_equals_new() {
        let m1 = AttackSurfaceMapper::new();
        let m2 = AttackSurfaceMapper::default();
        let ep1 = m1.map("#[get(\"/\")]");
        let ep2 = m2.map("#[get(\"/\")]");
        assert_eq!(ep1.len(), ep2.len());
    }

    #[test]
    fn test_attack_surface_mapper_empty_content() {
        let mapper = AttackSurfaceMapper::new();
        let entry_points = mapper.map("");
        assert!(entry_points.is_empty());
    }

    #[test]
    fn test_attack_surface_mapper_detects_rest_api_get() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"#[get("/users")] async fn get_users() {}"#;
        let eps = mapper.map(code);
        assert!(eps.iter().any(|e| e.entry_type == EntryPointType::RestApi));
    }

    #[test]
    fn test_attack_surface_mapper_detects_rest_api_post() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"#[post("/users")] async fn create_user() {}"#;
        let eps = mapper.map(code);
        assert!(eps.iter().any(|e| e.entry_type == EntryPointType::RestApi));
    }

    #[test]
    fn test_attack_surface_mapper_detects_graphql() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"
struct Query;
impl Query {
    fn get_user(&self, id: i32) -> User { todo!() }
}
"#;
        let eps = mapper.map(code);
        assert!(eps.iter().any(|e| e.entry_type == EntryPointType::GraphQL));
    }

    #[test]
    fn test_attack_surface_mapper_detects_database() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"
let result = conn.execute("SELECT * FROM users WHERE id = $1", &[&id])?;
"#;
        let eps = mapper.map(code);
        assert!(eps.iter().any(|e| e.entry_type == EntryPointType::Database));
    }

    #[test]
    fn test_attack_surface_mapper_detects_file_upload() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"
fn handle_upload(multipart: Multipart) -> Result<()> {
    save_file(multipart.file_field())?;
    Ok(())
}
"#;
        let eps = mapper.map(code);
        assert!(eps
            .iter()
            .any(|e| e.entry_type == EntryPointType::FileUpload));
    }

    #[test]
    fn test_attack_surface_mapper_detects_cli() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"
use clap::Parser;

#[derive(Parser)]
struct Args {
    #[arg(short)]
    verbose: bool,
}
"#;
        let eps = mapper.map(code);
        assert!(eps.iter().any(|e| e.entry_type == EntryPointType::Cli));
    }

    #[test]
    fn test_attack_surface_mapper_no_false_positive_for_unrelated_code() {
        let mapper = AttackSurfaceMapper::new();
        let code = r#"
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}
"#;
        let eps = mapper.map(code);
        assert!(eps.is_empty(), "Pure math code should have no entry points");
    }

    // ── EntryPointDetector comprehensive tests ────────────────────────────────

    #[test]
    fn test_entry_point_detector_returns_none_when_no_match() {
        let detector = EntryPointDetector::new(EntryPointType::RestApi, vec!["#[get", "#[post"]);
        assert!(detector.detect("fn plain_function() {}").is_none());
    }

    #[test]
    fn test_entry_point_detector_returns_some_when_matched() {
        let detector = EntryPointDetector::new(EntryPointType::RestApi, vec!["#[get", "#[post"]);
        let result = detector.detect("#[get(\"/\")] fn root() {}");
        assert!(result.is_some());
        let ep = result.unwrap();
        assert_eq!(ep.entry_type, EntryPointType::RestApi);
    }

    #[test]
    fn test_entry_point_detector_first_matching_pattern_wins() {
        // If both patterns match, we should still get one entry point
        let detector = EntryPointDetector::new(EntryPointType::Database, vec!["SELECT", "INSERT"]);
        let code = "let _ = conn.query(\"SELECT * FROM t\"); let _ = conn.execute(\"INSERT INTO t VALUES(?)\");";
        let result = detector.detect(code);
        assert!(result.is_some());
    }

    // ── SecurityScanner comprehensive tests ───────────────────────────────────

    #[test]
    fn test_security_scanner_default_equals_new() {
        let s1 = SecurityScanner::new();
        let s2 = SecurityScanner::default();
        let r1 = s1.scan_file("", &PathBuf::from("a.rs"));
        let r2 = s2.scan_file("", &PathBuf::from("a.rs"));
        assert_eq!(r1.threats.len(), r2.threats.len());
        assert_eq!(r1.entry_points.len(), r2.entry_points.len());
    }

    #[test]
    fn test_security_scanner_empty_content_produces_empty_result() {
        let scanner = SecurityScanner::new();
        let result = scanner.scan_file("", &PathBuf::from("empty.rs"));

        assert!(result.threats.is_empty());
        assert!(result.entry_points.is_empty());
    }

    #[test]
    fn test_security_scanner_result_file_path_matches_input() {
        let scanner = SecurityScanner::new();
        let path = PathBuf::from("src/api/mod.rs");
        let result = scanner.scan_file("", &path);

        assert_eq!(result.file, path);
    }

    #[test]
    fn test_security_scanner_detects_both_threats_and_entry_points() {
        let scanner = SecurityScanner::new();
        let code = r#"
#[get("/users")]
fn get_users(db: &Db) {
    // No auth check - unauthenticated access
    let query = format!("SELECT * FROM users WHERE id = {}", unsafe_input);
    db.execute(&query)
}
"#;
        let result = scanner.scan_file(code, &PathBuf::from("users.rs"));

        assert!(!result.threats.is_empty(), "Should detect security threats");
        assert!(
            !result.entry_points.is_empty(),
            "Should detect REST API entry point"
        );
    }

    #[test]
    fn test_security_scanner_clean_code_has_no_threats() {
        let scanner = SecurityScanner::new();
        let code = r#"
/// Returns the sum of two integers.
pub fn add(x: i32, y: i32) -> i32 {
    x + y
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        assert_eq!(add(2, 2), 4);
    }
}
"#;
        let result = scanner.scan_file(code, &PathBuf::from("math.rs"));
        assert!(
            result.threats.is_empty(),
            "Clean code should produce no threats"
        );
    }

    // ── ID uniqueness cross-type tests ────────────────────────────────────────

    #[test]
    fn test_ids_have_correct_prefix() {
        let t = Threat::new("T", StrideCategory::Spoofing);
        let a = Asset::new("A", AssetType::Other);
        let c = SecurityControl::new("C", ControlType::Preventive);

        assert!(t.id.starts_with("threat-"), "Threat ID prefix");
        assert!(a.id.starts_with("asset-"), "Asset ID prefix");
        assert!(c.id.starts_with("control-"), "Control ID prefix");
    }

    #[test]
    fn test_many_threats_all_have_unique_ids() {
        use std::collections::HashSet;
        let ids: HashSet<String> = (0..50)
            .map(|i| Threat::new(format!("T{}", i), StrideCategory::Spoofing).id)
            .collect();
        assert_eq!(ids.len(), 50, "All 50 threat IDs should be unique");
    }

    #[test]
    fn test_many_assets_all_have_unique_ids() {
        use std::collections::HashSet;
        let ids: HashSet<String> = (0..50)
            .map(|i| Asset::new(format!("A{}", i), AssetType::Other).id)
            .collect();
        assert_eq!(ids.len(), 50, "All 50 asset IDs should be unique");
    }

    #[test]
    fn test_many_controls_all_have_unique_ids() {
        use std::collections::HashSet;
        let ids: HashSet<String> = (0..50)
            .map(|i| SecurityControl::new(format!("C{}", i), ControlType::Preventive).id)
            .collect();
        assert_eq!(ids.len(), 50, "All 50 control IDs should be unique");
    }

    // ── Clone / Copy tests ────────────────────────────────────────────────────

    #[test]
    fn test_threat_clone_is_independent() {
        let original =
            Threat::new("Original", StrideCategory::Tampering).with_severity(Severity::High);

        let mut cloned = original.clone();
        cloned.severity = Severity::Low;

        assert_eq!(original.severity, Severity::High);
        assert_eq!(cloned.severity, Severity::Low);
    }

    #[test]
    fn test_asset_clone_is_independent() {
        let original = Asset::new("Original", AssetType::UserData).with_value(5);
        let mut cloned = original.clone();
        cloned.value = 1;

        assert_eq!(original.value, 5);
        assert_eq!(cloned.value, 1);
    }

    #[test]
    fn test_security_control_clone_is_independent() {
        let original =
            SecurityControl::new("Original", ControlType::Preventive).with_effectiveness(5);
        let mut cloned = original.clone();
        cloned.effectiveness = 1;

        assert_eq!(original.effectiveness, 5);
        assert_eq!(cloned.effectiveness, 1);
    }

    #[test]
    fn test_stride_category_copy() {
        let cat = StrideCategory::Spoofing;
        let copy = cat; // Copy semantics
        assert_eq!(cat, copy);
    }

    // ── Edge case and integration tests ──────────────────────────────────────

    #[test]
    fn test_threat_model_get_threat_mut_changes_persist() {
        let mut model = ThreatModel::new("Mut Test");

        let id = model.add_threat(Threat::new("T", StrideCategory::Spoofing));
        {
            let t = model.get_threat_mut(&id).unwrap();
            t.status = ThreatStatus::Closed;
            t.severity = Severity::Critical;
        }

        let retrieved = model.get_threat(&id).unwrap();
        assert_eq!(retrieved.status, ThreatStatus::Closed);
        assert_eq!(retrieved.severity, Severity::Critical);
    }

    #[test]
    fn test_stride_coverage_counts_all_occurrences() {
        let mut model = ThreatModel::new("Coverage Count");

        for _ in 0..5 {
            model.add_threat(Threat::new("Spoof", StrideCategory::Spoofing));
        }
        for _ in 0..3 {
            model.add_threat(Threat::new("Tamp", StrideCategory::Tampering));
        }

        let cov = model.stride_coverage();
        assert_eq!(*cov.get(&StrideCategory::Spoofing).unwrap(), 5);
        assert_eq!(*cov.get(&StrideCategory::Tampering).unwrap(), 3);
        assert_eq!(*cov.get(&StrideCategory::Repudiation).unwrap(), 0);
    }

    #[test]
    fn test_full_threat_model_workflow() {
        let mut model = ThreatModel::new("E-commerce Platform")
            .with_description("Security model for e-commerce system");

        // Define assets
        let db_id = model.add_asset(
            Asset::new("Customer Database", AssetType::UserData)
                .with_value(5)
                .with_sensitivity(5)
                .with_owner("data-team")
                .with_location("AWS RDS"),
        );

        let payment_id = model.add_asset(
            Asset::new("Payment Data", AssetType::FinancialData)
                .with_value(5)
                .with_sensitivity(5),
        );

        // Define threats
        let sqli_id = model.add_threat(
            Threat::new("SQL Injection via search", StrideCategory::Tampering)
                .with_severity(Severity::High)
                .with_likelihood(Likelihood::Likely)
                .with_affected_asset(&db_id)
                .with_impact("Full database compromise")
                .with_mitigation("Parameterized queries already in 80% of code")
                .with_recommendation("Complete parameterization of all queries"),
        );

        let payment_threat_id = model.add_threat(
            Threat::new(
                "Payment data interception",
                StrideCategory::InformationDisclosure,
            )
            .with_severity(Severity::Critical)
            .with_likelihood(Likelihood::Possible)
            .with_affected_asset(&payment_id)
            .with_attack_vector("Man-in-the-middle on payment API")
            .with_impact("Financial loss and regulatory penalties"),
        );

        // Define controls
        let ctrl_id = model.add_control(
            SecurityControl::new("TLS 1.3 Enforcement", ControlType::Preventive)
                .with_status(ControlStatus::Implemented)
                .with_effectiveness(5)
                .mitigates_threat(&payment_threat_id)
                .with_owner("infra-team"),
        );

        // Mark SQL injection as partially mitigated
        model.get_threat_mut(&sqli_id).unwrap().status = ThreatStatus::Mitigated;

        // Add entry points
        model.add_entry_point(
            EntryPoint::new("/api/search", EntryPointType::RestApi)
                .requires_authentication()
                .with_trust_level(TrustLevel::Authenticated)
                .with_threat(&sqli_id),
        );

        // Add trust boundary
        model.add_trust_boundary(
            TrustBoundary::new("Payment Gateway Boundary")
                .with_component("Stripe API")
                .with_trust_levels(TrustLevel::System, TrustLevel::Authenticated),
        );

        // Assertions
        assert_eq!(model.assets().count(), 2);
        assert_eq!(model.threats().count(), 2);
        assert_eq!(model.controls().count(), 1);
        assert_eq!(model.entry_points().len(), 1);
        assert_eq!(model.trust_boundaries().len(), 1);

        // Only payment_threat is open
        let open = model.open_threats();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].title, "Payment data interception");

        // Critical threats: payment_threat (Critical × Possible = 4*2 = 8 → High, not Critical)
        // Let's verify
        let payment_threat = model.get_threat(&payment_threat_id).unwrap();
        assert_eq!(payment_threat.risk_score(), 8); // 4 * 2 = 8 → High
        assert_eq!(payment_threat.risk_level(), RiskLevel::High);

        // Control is linked to payment threat
        let ctrl = model.get_control(&ctrl_id).unwrap();
        assert_eq!(ctrl.mitigates, vec![payment_threat_id.clone()]);

        // Overall risk score: only open threats
        let score = model.overall_risk_score();
        assert!((score - 8.0).abs() < f32::EPSILON);

        // Report includes all components
        let report = model.generate_report();
        assert!(report.contains("E-commerce Platform"));
        assert!(report.contains("Customer Database"));
        assert!(report.contains("SQL Injection via search"));
        assert!(report.contains("TLS 1.3 Enforcement"));
    }
}
