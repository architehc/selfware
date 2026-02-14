//! Cloud Infrastructure Management
//!
//! Multi-cloud infrastructure management supporting AWS, GCP, and Azure.
//! Provides unified interface for resource provisioning, cost tracking,
//! and compliance checking across cloud providers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static RESOURCE_COUNTER: AtomicU64 = AtomicU64::new(1);
static OPERATION_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_resource_id() -> String {
    format!("res-{}", RESOURCE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_operation_id() -> String {
    format!("op-{}", OPERATION_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// Cloud Providers
// ============================================================================

/// Supported cloud providers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloudProvider {
    AWS,
    GCP,
    Azure,
    DigitalOcean,
    Linode,
    Custom,
}

impl CloudProvider {
    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::AWS => "Amazon Web Services",
            Self::GCP => "Google Cloud Platform",
            Self::Azure => "Microsoft Azure",
            Self::DigitalOcean => "DigitalOcean",
            Self::Linode => "Linode",
            Self::Custom => "Custom Provider",
        }
    }

    /// Get short code
    pub fn code(&self) -> &'static str {
        match self {
            Self::AWS => "aws",
            Self::GCP => "gcp",
            Self::Azure => "azure",
            Self::DigitalOcean => "do",
            Self::Linode => "linode",
            Self::Custom => "custom",
        }
    }

    /// Get available regions
    pub fn regions(&self) -> Vec<&'static str> {
        match self {
            Self::AWS => vec![
                "us-east-1",
                "us-east-2",
                "us-west-1",
                "us-west-2",
                "eu-west-1",
                "eu-west-2",
                "eu-central-1",
                "ap-southeast-1",
                "ap-southeast-2",
                "ap-northeast-1",
            ],
            Self::GCP => vec![
                "us-central1",
                "us-east1",
                "us-west1",
                "europe-west1",
                "europe-west2",
                "asia-east1",
                "asia-southeast1",
            ],
            Self::Azure => vec![
                "eastus",
                "eastus2",
                "westus",
                "westus2",
                "northeurope",
                "westeurope",
                "southeastasia",
                "eastasia",
            ],
            Self::DigitalOcean => vec!["nyc1", "nyc3", "sfo2", "sfo3", "ams3", "sgp1"],
            Self::Linode => vec!["us-east", "us-west", "eu-west", "ap-south"],
            Self::Custom => vec!["default"],
        }
    }
}

// ============================================================================
// Resource Types
// ============================================================================

/// Cloud resource types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    // Compute
    VirtualMachine,
    Container,
    Function,
    Kubernetes,

    // Storage
    ObjectStorage,
    BlockStorage,
    FileStorage,
    Archive,

    // Network
    VPC,
    Subnet,
    LoadBalancer,
    CDN,
    DNS,
    Firewall,

    // Database
    RelationalDB,
    NoSQLDB,
    Cache,
    DataWarehouse,

    // Security
    IAMRole,
    IAMPolicy,
    Secret,
    Certificate,

    // Messaging
    Queue,
    Topic,
    EventBus,
}

impl ResourceType {
    /// Get category
    pub fn category(&self) -> ResourceCategory {
        match self {
            Self::VirtualMachine | Self::Container | Self::Function | Self::Kubernetes => {
                ResourceCategory::Compute
            }
            Self::ObjectStorage | Self::BlockStorage | Self::FileStorage | Self::Archive => {
                ResourceCategory::Storage
            }
            Self::VPC
            | Self::Subnet
            | Self::LoadBalancer
            | Self::CDN
            | Self::DNS
            | Self::Firewall => ResourceCategory::Network,
            Self::RelationalDB | Self::NoSQLDB | Self::Cache | Self::DataWarehouse => {
                ResourceCategory::Database
            }
            Self::IAMRole | Self::IAMPolicy | Self::Secret | Self::Certificate => {
                ResourceCategory::Security
            }
            Self::Queue | Self::Topic | Self::EventBus => ResourceCategory::Messaging,
        }
    }

    /// Get AWS service name
    pub fn aws_service(&self) -> &'static str {
        match self {
            Self::VirtualMachine => "ec2",
            Self::Container => "ecs",
            Self::Function => "lambda",
            Self::Kubernetes => "eks",
            Self::ObjectStorage => "s3",
            Self::BlockStorage => "ebs",
            Self::FileStorage => "efs",
            Self::Archive => "glacier",
            Self::VPC => "vpc",
            Self::Subnet => "subnet",
            Self::LoadBalancer => "elb",
            Self::CDN => "cloudfront",
            Self::DNS => "route53",
            Self::Firewall => "security-group",
            Self::RelationalDB => "rds",
            Self::NoSQLDB => "dynamodb",
            Self::Cache => "elasticache",
            Self::DataWarehouse => "redshift",
            Self::IAMRole => "iam-role",
            Self::IAMPolicy => "iam-policy",
            Self::Secret => "secrets-manager",
            Self::Certificate => "acm",
            Self::Queue => "sqs",
            Self::Topic => "sns",
            Self::EventBus => "eventbridge",
        }
    }

    /// Get GCP service name
    pub fn gcp_service(&self) -> &'static str {
        match self {
            Self::VirtualMachine => "compute",
            Self::Container => "cloud-run",
            Self::Function => "cloud-functions",
            Self::Kubernetes => "gke",
            Self::ObjectStorage => "cloud-storage",
            Self::BlockStorage => "persistent-disk",
            Self::FileStorage => "filestore",
            Self::Archive => "archive-storage",
            Self::VPC => "vpc",
            Self::Subnet => "subnet",
            Self::LoadBalancer => "load-balancing",
            Self::CDN => "cloud-cdn",
            Self::DNS => "cloud-dns",
            Self::Firewall => "firewall-rules",
            Self::RelationalDB => "cloud-sql",
            Self::NoSQLDB => "firestore",
            Self::Cache => "memorystore",
            Self::DataWarehouse => "bigquery",
            Self::IAMRole => "iam-role",
            Self::IAMPolicy => "iam-policy",
            Self::Secret => "secret-manager",
            Self::Certificate => "certificate-manager",
            Self::Queue => "cloud-tasks",
            Self::Topic => "pubsub",
            Self::EventBus => "eventarc",
        }
    }
}

/// Resource categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceCategory {
    Compute,
    Storage,
    Network,
    Database,
    Security,
    Messaging,
}

// ============================================================================
// Resource Status
// ============================================================================

/// Resource lifecycle status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    Creating,
    Running,
    Stopped,
    Updating,
    Deleting,
    Deleted,
    Failed,
    Unknown,
}

impl ResourceStatus {
    /// Check if resource is active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Creating | Self::Updating)
    }

    /// Check if resource is terminal
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Deleted | Self::Failed)
    }
}

// ============================================================================
// Cloud Resource
// ============================================================================

/// Cloud resource representation
#[derive(Debug, Clone)]
pub struct CloudResource {
    pub id: String,
    pub name: String,
    pub provider: CloudProvider,
    pub resource_type: ResourceType,
    pub region: String,
    pub status: ResourceStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
    pub tags: HashMap<String, String>,
    pub properties: HashMap<String, String>,
    pub cost_per_hour: f64,
    pub dependencies: Vec<String>,
}

impl CloudResource {
    /// Create new resource
    pub fn new(
        name: impl Into<String>,
        provider: CloudProvider,
        resource_type: ResourceType,
        region: impl Into<String>,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            id: generate_resource_id(),
            name: name.into(),
            provider,
            resource_type,
            region: region.into(),
            status: ResourceStatus::Creating,
            created_at: now,
            updated_at: now,
            tags: HashMap::new(),
            properties: HashMap::new(),
            cost_per_hour: 0.0,
            dependencies: Vec::new(),
        }
    }

    /// Add tag
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Add property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Set cost per hour
    pub fn with_cost(mut self, cost_per_hour: f64) -> Self {
        self.cost_per_hour = cost_per_hour;
        self
    }

    /// Add dependency
    pub fn with_dependency(mut self, resource_id: impl Into<String>) -> Self {
        self.dependencies.push(resource_id.into());
        self
    }

    /// Get resource ARN (AWS-style)
    pub fn arn(&self) -> String {
        format!(
            "arn:{}:{}:{}:{}",
            self.provider.code(),
            self.resource_type.aws_service(),
            self.region,
            self.id
        )
    }

    /// Get monthly cost estimate
    pub fn monthly_cost(&self) -> f64 {
        self.cost_per_hour * 24.0 * 30.0
    }

    /// Update status
    pub fn set_status(&mut self, status: ResourceStatus) {
        self.status = status;
        self.updated_at = SystemTime::now();
    }
}

// ============================================================================
// Resource Spec (for creation)
// ============================================================================

/// Specification for creating a resource
#[derive(Debug, Clone)]
pub struct ResourceSpec {
    pub name: String,
    pub resource_type: ResourceType,
    pub region: String,
    pub size: String,
    pub tags: HashMap<String, String>,
    pub properties: HashMap<String, String>,
}

impl ResourceSpec {
    /// Create new spec
    pub fn new(
        name: impl Into<String>,
        resource_type: ResourceType,
        region: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            resource_type,
            region: region.into(),
            size: "small".to_string(),
            tags: HashMap::new(),
            properties: HashMap::new(),
        }
    }

    /// Set size
    pub fn with_size(mut self, size: impl Into<String>) -> Self {
        self.size = size.into();
        self
    }

    /// Add tag
    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    /// Add property
    pub fn with_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }
}

// ============================================================================
// Cloud Credentials
// ============================================================================

/// Cloud provider credentials
#[derive(Debug, Clone)]
pub struct CloudCredentials {
    pub provider: CloudProvider,
    pub access_key: String,
    pub secret_key: String,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub extra: HashMap<String, String>,
}

impl CloudCredentials {
    /// Create AWS credentials
    pub fn aws(access_key: impl Into<String>, secret_key: impl Into<String>) -> Self {
        Self {
            provider: CloudProvider::AWS,
            access_key: access_key.into(),
            secret_key: secret_key.into(),
            region: None,
            project_id: None,
            extra: HashMap::new(),
        }
    }

    /// Create GCP credentials
    pub fn gcp(service_account_key: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            provider: CloudProvider::GCP,
            access_key: service_account_key.into(),
            secret_key: String::new(),
            region: None,
            project_id: Some(project_id.into()),
            extra: HashMap::new(),
        }
    }

    /// Create Azure credentials
    pub fn azure(
        tenant_id: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        let mut extra = HashMap::new();
        extra.insert("tenant_id".to_string(), tenant_id.into());
        Self {
            provider: CloudProvider::Azure,
            access_key: client_id.into(),
            secret_key: client_secret.into(),
            region: None,
            project_id: None,
            extra,
        }
    }

    /// Set region
    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }

    /// Validate credentials format
    pub fn validate(&self) -> Result<(), String> {
        if self.access_key.is_empty() {
            return Err("Access key is required".to_string());
        }

        match self.provider {
            CloudProvider::GCP => {
                if self.project_id.is_none() {
                    return Err("Project ID is required for GCP".to_string());
                }
            }
            CloudProvider::Azure => {
                if !self.extra.contains_key("tenant_id") {
                    return Err("Tenant ID is required for Azure".to_string());
                }
            }
            _ => {}
        }

        Ok(())
    }
}

// ============================================================================
// Cloud Operations
// ============================================================================

/// Cloud operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Create,
    Read,
    Update,
    Delete,
    Start,
    Stop,
    Restart,
    Scale,
    Backup,
    Restore,
}

/// Operation status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Cloud operation result
#[derive(Debug, Clone)]
pub struct CloudOperation {
    pub id: String,
    pub operation_type: OperationType,
    pub resource_id: String,
    pub status: OperationStatus,
    pub started_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub error: Option<String>,
    pub output: HashMap<String, String>,
}

impl CloudOperation {
    /// Create new operation
    pub fn new(operation_type: OperationType, resource_id: impl Into<String>) -> Self {
        Self {
            id: generate_operation_id(),
            operation_type,
            resource_id: resource_id.into(),
            status: OperationStatus::Pending,
            started_at: SystemTime::now(),
            completed_at: None,
            error: None,
            output: HashMap::new(),
        }
    }

    /// Mark as running
    pub fn start(&mut self) {
        self.status = OperationStatus::Running;
    }

    /// Mark as completed
    pub fn complete(&mut self) {
        self.status = OperationStatus::Completed;
        self.completed_at = Some(SystemTime::now());
    }

    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.status = OperationStatus::Failed;
        self.completed_at = Some(SystemTime::now());
        self.error = Some(error.into());
    }

    /// Get duration
    pub fn duration(&self) -> Option<Duration> {
        self.completed_at
            .and_then(|end| end.duration_since(self.started_at).ok())
    }
}

// ============================================================================
// Cost Tracking
// ============================================================================

/// Cost tracking for resources
#[derive(Debug, Clone)]
pub struct CostTracker {
    resources: HashMap<String, ResourceCost>,
    budget_alerts: Vec<BudgetAlert>,
}

/// Resource cost data
#[derive(Debug, Clone)]
pub struct ResourceCost {
    pub resource_id: String,
    pub resource_name: String,
    pub provider: CloudProvider,
    pub resource_type: ResourceType,
    pub hourly_cost: f64,
    pub total_hours: f64,
    pub total_cost: f64,
}

/// Budget alert configuration
#[derive(Debug, Clone)]
pub struct BudgetAlert {
    pub name: String,
    pub threshold: f64,
    pub current_spend: f64,
    pub period: BudgetPeriod,
    pub triggered: bool,
}

/// Budget period
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetPeriod {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

impl CostTracker {
    /// Create new cost tracker
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            budget_alerts: Vec::new(),
        }
    }

    /// Track resource cost
    pub fn track_resource(&mut self, resource: &CloudResource, hours: f64) {
        let cost = ResourceCost {
            resource_id: resource.id.clone(),
            resource_name: resource.name.clone(),
            provider: resource.provider,
            resource_type: resource.resource_type,
            hourly_cost: resource.cost_per_hour,
            total_hours: hours,
            total_cost: resource.cost_per_hour * hours,
        };
        self.resources.insert(resource.id.clone(), cost);
    }

    /// Add budget alert
    pub fn add_alert(&mut self, name: impl Into<String>, threshold: f64, period: BudgetPeriod) {
        self.budget_alerts.push(BudgetAlert {
            name: name.into(),
            threshold,
            current_spend: 0.0,
            period,
            triggered: false,
        });
    }

    /// Get total spend
    pub fn total_spend(&self) -> f64 {
        self.resources.values().map(|r| r.total_cost).sum()
    }

    /// Get spend by provider
    pub fn spend_by_provider(&self) -> HashMap<CloudProvider, f64> {
        let mut by_provider: HashMap<CloudProvider, f64> = HashMap::new();
        for cost in self.resources.values() {
            *by_provider.entry(cost.provider).or_insert(0.0) += cost.total_cost;
        }
        by_provider
    }

    /// Get spend by resource type
    pub fn spend_by_type(&self) -> HashMap<ResourceType, f64> {
        let mut by_type: HashMap<ResourceType, f64> = HashMap::new();
        for cost in self.resources.values() {
            *by_type.entry(cost.resource_type).or_insert(0.0) += cost.total_cost;
        }
        by_type
    }

    /// Check budget alerts
    pub fn check_alerts(&mut self) -> Vec<&BudgetAlert> {
        let total = self.total_spend();
        for alert in &mut self.budget_alerts {
            alert.current_spend = total;
            if total >= alert.threshold && !alert.triggered {
                alert.triggered = true;
            }
        }
        self.budget_alerts.iter().filter(|a| a.triggered).collect()
    }

    /// Get top expensive resources
    pub fn top_expensive(&self, limit: usize) -> Vec<&ResourceCost> {
        let mut costs: Vec<_> = self.resources.values().collect();
        costs.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap());
        costs.into_iter().take(limit).collect()
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Infrastructure as Code
// ============================================================================

/// Infrastructure definition
#[derive(Debug, Clone)]
pub struct InfrastructureDefinition {
    pub name: String,
    pub version: String,
    pub provider: CloudProvider,
    pub resources: Vec<ResourceDefinition>,
    pub variables: HashMap<String, String>,
    pub outputs: Vec<OutputDefinition>,
}

/// Resource definition for IaC
#[derive(Debug, Clone)]
pub struct ResourceDefinition {
    pub name: String,
    pub resource_type: ResourceType,
    pub region: String,
    pub properties: HashMap<String, String>,
    pub depends_on: Vec<String>,
}

/// Output definition
#[derive(Debug, Clone)]
pub struct OutputDefinition {
    pub name: String,
    pub value: String,
    pub description: String,
}

impl InfrastructureDefinition {
    /// Create new infrastructure definition
    pub fn new(name: impl Into<String>, provider: CloudProvider) -> Self {
        Self {
            name: name.into(),
            version: "1.0.0".to_string(),
            provider,
            resources: Vec::new(),
            variables: HashMap::new(),
            outputs: Vec::new(),
        }
    }

    /// Add resource
    pub fn add_resource(&mut self, resource: ResourceDefinition) {
        self.resources.push(resource);
    }

    /// Add variable
    pub fn add_variable(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(name.into(), value.into());
    }

    /// Add output
    pub fn add_output(&mut self, output: OutputDefinition) {
        self.outputs.push(output);
    }

    /// Validate definition
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.name.is_empty() {
            errors.push("Infrastructure name is required".to_string());
        }

        if self.resources.is_empty() {
            errors.push("At least one resource is required".to_string());
        }

        // Check for circular dependencies
        let resource_names: std::collections::HashSet<_> =
            self.resources.iter().map(|r| &r.name).collect();

        for resource in &self.resources {
            for dep in &resource.depends_on {
                if !resource_names.contains(dep) {
                    errors.push(format!(
                        "Resource '{}' depends on unknown resource '{}'",
                        resource.name, dep
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get deployment order (topological sort)
    pub fn deployment_order(&self) -> Vec<&ResourceDefinition> {
        let mut order = Vec::new();
        let mut remaining: Vec<_> = self.resources.iter().collect();
        let mut deployed: std::collections::HashSet<&str> = std::collections::HashSet::new();

        while !remaining.is_empty() {
            let mut progress = false;
            remaining.retain(|resource| {
                let deps_met = resource
                    .depends_on
                    .iter()
                    .all(|d| deployed.contains(d.as_str()));
                if deps_met {
                    order.push(*resource);
                    deployed.insert(&resource.name);
                    progress = true;
                    false
                } else {
                    true
                }
            });

            if !progress && !remaining.is_empty() {
                // Circular dependency, add remaining in any order
                order.append(&mut remaining);
            }
        }

        order
    }

    /// Generate Terraform HCL
    pub fn to_terraform(&self) -> String {
        let mut hcl = String::new();

        // Provider block
        hcl.push_str(&format!("provider \"{}\" {{\n", self.provider.code()));
        if let Some(region) = self.variables.get("region") {
            hcl.push_str(&format!("  region = \"{}\"\n", region));
        }
        hcl.push_str("}\n\n");

        // Variables
        for (name, value) in &self.variables {
            hcl.push_str(&format!(
                "variable \"{}\" {{\n  default = \"{}\"\n}}\n\n",
                name, value
            ));
        }

        // Resources
        for resource in &self.resources {
            hcl.push_str(&format!(
                "resource \"{}\" \"{}\" {{\n",
                resource.resource_type.aws_service(),
                resource.name
            ));
            for (key, value) in &resource.properties {
                hcl.push_str(&format!("  {} = \"{}\"\n", key, value));
            }
            if !resource.depends_on.is_empty() {
                let deps: Vec<_> = resource
                    .depends_on
                    .iter()
                    .map(|d| format!("{}.{}", d, d))
                    .collect();
                hcl.push_str(&format!("  depends_on = [{}]\n", deps.join(", ")));
            }
            hcl.push_str("}\n\n");
        }

        // Outputs
        for output in &self.outputs {
            hcl.push_str(&format!(
                "output \"{}\" {{\n  value = {}\n  description = \"{}\"\n}}\n\n",
                output.name, output.value, output.description
            ));
        }

        hcl
    }
}

// ============================================================================
// Compliance Checker
// ============================================================================

/// Compliance rule
#[derive(Debug, Clone)]
pub struct ComplianceRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub severity: ComplianceSeverity,
    pub check: ComplianceCheck,
}

/// Compliance severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ComplianceSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Compliance check types
#[derive(Debug, Clone)]
pub enum ComplianceCheck {
    RequireTag(String),
    RequireEncryption,
    RequirePrivateAccess,
    MaxCost(f64),
    AllowedRegions(Vec<String>),
    RequireBackup,
    Custom(String),
}

/// Compliance violation
#[derive(Debug, Clone)]
pub struct ComplianceViolation {
    pub rule: ComplianceRule,
    pub resource_id: String,
    pub resource_name: String,
    pub message: String,
}

/// Compliance checker
#[derive(Debug, Clone)]
pub struct ComplianceChecker {
    rules: Vec<ComplianceRule>,
}

impl ComplianceChecker {
    /// Create new checker
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Create with default rules
    pub fn with_defaults() -> Self {
        let mut checker = Self::new();

        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "Require environment tag".to_string(),
            description: "All resources must have an environment tag".to_string(),
            severity: ComplianceSeverity::Medium,
            check: ComplianceCheck::RequireTag("environment".to_string()),
        });

        checker.add_rule(ComplianceRule {
            id: "R002".to_string(),
            name: "Require owner tag".to_string(),
            description: "All resources must have an owner tag".to_string(),
            severity: ComplianceSeverity::Low,
            check: ComplianceCheck::RequireTag("owner".to_string()),
        });

        checker.add_rule(ComplianceRule {
            id: "R003".to_string(),
            name: "Storage encryption".to_string(),
            description: "All storage resources must be encrypted".to_string(),
            severity: ComplianceSeverity::High,
            check: ComplianceCheck::RequireEncryption,
        });

        checker
    }

    /// Add rule
    pub fn add_rule(&mut self, rule: ComplianceRule) {
        self.rules.push(rule);
    }

    /// Check resource compliance
    pub fn check_resource(&self, resource: &CloudResource) -> Vec<ComplianceViolation> {
        let mut violations = Vec::new();

        for rule in &self.rules {
            if let Some(message) = self.check_rule(rule, resource) {
                violations.push(ComplianceViolation {
                    rule: rule.clone(),
                    resource_id: resource.id.clone(),
                    resource_name: resource.name.clone(),
                    message,
                });
            }
        }

        violations
    }

    /// Check single rule
    fn check_rule(&self, rule: &ComplianceRule, resource: &CloudResource) -> Option<String> {
        match &rule.check {
            ComplianceCheck::RequireTag(tag) => {
                if !resource.tags.contains_key(tag) {
                    Some(format!("Missing required tag: {}", tag))
                } else {
                    None
                }
            }
            ComplianceCheck::RequireEncryption => {
                if resource.resource_type.category() == ResourceCategory::Storage {
                    if resource.properties.get("encryption") != Some(&"enabled".to_string()) {
                        Some("Storage resource is not encrypted".to_string())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            ComplianceCheck::RequirePrivateAccess => {
                if resource.properties.get("public_access") == Some(&"true".to_string()) {
                    Some("Resource has public access enabled".to_string())
                } else {
                    None
                }
            }
            ComplianceCheck::MaxCost(max) => {
                if resource.monthly_cost() > *max {
                    Some(format!(
                        "Monthly cost ${:.2} exceeds limit ${:.2}",
                        resource.monthly_cost(),
                        max
                    ))
                } else {
                    None
                }
            }
            ComplianceCheck::AllowedRegions(regions) => {
                if !regions.contains(&resource.region) {
                    Some(format!(
                        "Region '{}' is not in allowed list: {:?}",
                        resource.region, regions
                    ))
                } else {
                    None
                }
            }
            ComplianceCheck::RequireBackup => {
                if resource.properties.get("backup_enabled") != Some(&"true".to_string()) {
                    Some("Backup is not enabled for this resource".to_string())
                } else {
                    None
                }
            }
            ComplianceCheck::Custom(rule_expr) => {
                // Custom rules would need evaluation logic
                Some(format!("Custom rule '{}' not evaluated", rule_expr))
            }
        }
    }

    /// Check all resources
    pub fn check_all(&self, resources: &[CloudResource]) -> Vec<ComplianceViolation> {
        resources
            .iter()
            .flat_map(|r| self.check_resource(r))
            .collect()
    }

    /// Get compliance score (0-100)
    pub fn compliance_score(&self, resources: &[CloudResource]) -> f64 {
        if resources.is_empty() || self.rules.is_empty() {
            return 100.0;
        }

        let total_checks = resources.len() * self.rules.len();
        let violations = self.check_all(resources).len();
        let passed = total_checks - violations;

        (passed as f64 / total_checks as f64) * 100.0
    }
}

impl Default for ComplianceChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Multi-Cloud Manager
// ============================================================================

/// Multi-cloud resource manager
#[derive(Debug)]
pub struct CloudManager {
    resources: HashMap<String, CloudResource>,
    credentials: HashMap<CloudProvider, CloudCredentials>,
    operations: Vec<CloudOperation>,
    cost_tracker: CostTracker,
    compliance: ComplianceChecker,
}

impl CloudManager {
    /// Create new cloud manager
    pub fn new() -> Self {
        Self {
            resources: HashMap::new(),
            credentials: HashMap::new(),
            operations: Vec::new(),
            cost_tracker: CostTracker::new(),
            compliance: ComplianceChecker::with_defaults(),
        }
    }

    /// Add credentials for provider
    pub fn add_credentials(&mut self, credentials: CloudCredentials) {
        self.credentials.insert(credentials.provider, credentials);
    }

    /// Create resource
    pub fn create_resource(
        &mut self,
        provider: CloudProvider,
        spec: ResourceSpec,
    ) -> Result<CloudResource, String> {
        // Validate credentials
        if !self.credentials.contains_key(&provider) {
            return Err(format!("No credentials for provider {:?}", provider));
        }

        // Validate region
        if !provider.regions().contains(&spec.region.as_str()) {
            return Err(format!(
                "Invalid region '{}' for provider {:?}",
                spec.region, provider
            ));
        }

        // Create resource
        let mut resource = CloudResource::new(spec.name, provider, spec.resource_type, spec.region);

        // Apply tags
        for (key, value) in spec.tags {
            resource.tags.insert(key, value);
        }

        // Apply properties
        for (key, value) in spec.properties {
            resource.properties.insert(key, value);
        }

        // Set cost based on size
        resource.cost_per_hour = self.estimate_cost(&spec.size, spec.resource_type);

        // Create operation
        let mut op = CloudOperation::new(OperationType::Create, &resource.id);
        op.start();
        op.complete();
        self.operations.push(op);

        // Set status
        resource.set_status(ResourceStatus::Running);

        // Store resource
        self.resources.insert(resource.id.clone(), resource.clone());

        // Track cost
        self.cost_tracker.track_resource(&resource, 0.0);

        Ok(resource)
    }

    /// Estimate hourly cost based on size
    fn estimate_cost(&self, size: &str, resource_type: ResourceType) -> f64 {
        let base_cost = match resource_type.category() {
            ResourceCategory::Compute => 0.05,
            ResourceCategory::Storage => 0.01,
            ResourceCategory::Network => 0.02,
            ResourceCategory::Database => 0.08,
            ResourceCategory::Security => 0.0,
            ResourceCategory::Messaging => 0.001,
        };

        let multiplier = match size {
            "micro" => 0.5,
            "small" => 1.0,
            "medium" => 2.0,
            "large" => 4.0,
            "xlarge" => 8.0,
            "2xlarge" => 16.0,
            _ => 1.0,
        };

        base_cost * multiplier
    }

    /// Get resource by ID
    pub fn get_resource(&self, id: &str) -> Option<&CloudResource> {
        self.resources.get(id)
    }

    /// Get resource by ID (mutable)
    pub fn get_resource_mut(&mut self, id: &str) -> Option<&mut CloudResource> {
        self.resources.get_mut(id)
    }

    /// Delete resource
    pub fn delete_resource(&mut self, id: &str) -> Result<(), String> {
        // First check if resource exists
        if !self.resources.contains_key(id) {
            return Err(format!("Resource not found: {}", id));
        }

        // Check dependencies (immutable borrow)
        let dependents: Vec<_> = self
            .resources
            .values()
            .filter(|r| r.dependencies.contains(&id.to_string()))
            .map(|r| r.id.clone())
            .collect();

        if !dependents.is_empty() {
            return Err(format!(
                "Cannot delete: resource has dependents: {:?}",
                dependents
            ));
        }

        // Create operation
        let mut op = CloudOperation::new(OperationType::Delete, id);
        op.start();
        op.complete();
        self.operations.push(op);

        // Now get mutable reference and update status
        if let Some(resource) = self.resources.get_mut(id) {
            resource.set_status(ResourceStatus::Deleted);
        }
        Ok(())
    }

    /// List resources by provider
    pub fn list_by_provider(&self, provider: CloudProvider) -> Vec<&CloudResource> {
        self.resources
            .values()
            .filter(|r| r.provider == provider && !r.status.is_terminal())
            .collect()
    }

    /// List resources by type
    pub fn list_by_type(&self, resource_type: ResourceType) -> Vec<&CloudResource> {
        self.resources
            .values()
            .filter(|r| r.resource_type == resource_type && !r.status.is_terminal())
            .collect()
    }

    /// List all active resources
    pub fn list_active(&self) -> Vec<&CloudResource> {
        self.resources
            .values()
            .filter(|r| r.status.is_active())
            .collect()
    }

    /// Get cost tracker
    pub fn cost_tracker(&self) -> &CostTracker {
        &self.cost_tracker
    }

    /// Get mutable cost tracker
    pub fn cost_tracker_mut(&mut self) -> &mut CostTracker {
        &mut self.cost_tracker
    }

    /// Check compliance for all resources
    pub fn check_compliance(&self) -> Vec<ComplianceViolation> {
        let active: Vec<_> = self.list_active().into_iter().cloned().collect();
        self.compliance.check_all(&active)
    }

    /// Get compliance score
    pub fn compliance_score(&self) -> f64 {
        let active: Vec<_> = self.list_active().into_iter().cloned().collect();
        self.compliance.compliance_score(&active)
    }

    /// Deploy infrastructure
    pub fn deploy(&mut self, definition: &InfrastructureDefinition) -> Result<Vec<String>, String> {
        definition.validate().map_err(|e| e.join(", "))?;

        let mut resource_ids = Vec::new();
        let order = definition.deployment_order();

        for resource_def in order {
            let spec = ResourceSpec::new(
                &resource_def.name,
                resource_def.resource_type,
                &resource_def.region,
            );

            let resource = self.create_resource(definition.provider, spec)?;
            resource_ids.push(resource.id);
        }

        Ok(resource_ids)
    }

    /// Get operations history
    pub fn operations(&self) -> &[CloudOperation] {
        &self.operations
    }

    /// Get recent operations
    pub fn recent_operations(&self, limit: usize) -> Vec<&CloudOperation> {
        self.operations.iter().rev().take(limit).collect()
    }
}

impl Default for CloudManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Resource Discovery
// ============================================================================

/// Discovered resource from cloud
#[derive(Debug, Clone)]
pub struct DiscoveredResource {
    pub provider: CloudProvider,
    pub resource_type: ResourceType,
    pub id: String,
    pub name: String,
    pub region: String,
    pub tags: HashMap<String, String>,
    pub created_at: Option<SystemTime>,
    pub managed: bool,
}

/// Resource discovery service
#[derive(Debug)]
pub struct ResourceDiscovery {
    discovered: Vec<DiscoveredResource>,
}

impl ResourceDiscovery {
    /// Create new discovery service
    pub fn new() -> Self {
        Self {
            discovered: Vec::new(),
        }
    }

    /// Simulate discovering resources
    pub fn discover(
        &mut self,
        _credentials: &CloudCredentials,
        _resource_types: Option<&[ResourceType]>,
    ) -> &[DiscoveredResource] {
        // In real implementation, would call cloud APIs
        // For now, return empty list
        &self.discovered
    }

    /// Import discovered resource
    pub fn import(&self, discovered: &DiscoveredResource) -> CloudResource {
        let mut resource = CloudResource::new(
            &discovered.name,
            discovered.provider,
            discovered.resource_type,
            &discovered.region,
        );
        resource.id = discovered.id.clone();
        resource.tags = discovered.tags.clone();
        resource.set_status(ResourceStatus::Running);
        resource
    }

    /// Find unmanaged resources
    pub fn find_unmanaged(&self) -> Vec<&DiscoveredResource> {
        self.discovered.iter().filter(|r| !r.managed).collect()
    }
}

impl Default for ResourceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_provider_name() {
        assert_eq!(CloudProvider::AWS.name(), "Amazon Web Services");
        assert_eq!(CloudProvider::GCP.name(), "Google Cloud Platform");
        assert_eq!(CloudProvider::Azure.name(), "Microsoft Azure");
    }

    #[test]
    fn test_cloud_provider_code() {
        assert_eq!(CloudProvider::AWS.code(), "aws");
        assert_eq!(CloudProvider::GCP.code(), "gcp");
        assert_eq!(CloudProvider::Azure.code(), "azure");
    }

    #[test]
    fn test_cloud_provider_regions() {
        let aws_regions = CloudProvider::AWS.regions();
        assert!(aws_regions.contains(&"us-east-1"));
        assert!(aws_regions.contains(&"eu-west-1"));

        let gcp_regions = CloudProvider::GCP.regions();
        assert!(gcp_regions.contains(&"us-central1"));
    }

    #[test]
    fn test_resource_type_category() {
        assert_eq!(
            ResourceType::VirtualMachine.category(),
            ResourceCategory::Compute
        );
        assert_eq!(
            ResourceType::ObjectStorage.category(),
            ResourceCategory::Storage
        );
        assert_eq!(
            ResourceType::LoadBalancer.category(),
            ResourceCategory::Network
        );
        assert_eq!(
            ResourceType::RelationalDB.category(),
            ResourceCategory::Database
        );
    }

    #[test]
    fn test_resource_type_service_names() {
        assert_eq!(ResourceType::VirtualMachine.aws_service(), "ec2");
        assert_eq!(ResourceType::VirtualMachine.gcp_service(), "compute");
        assert_eq!(ResourceType::ObjectStorage.aws_service(), "s3");
        assert_eq!(ResourceType::ObjectStorage.gcp_service(), "cloud-storage");
    }

    #[test]
    fn test_resource_status() {
        assert!(ResourceStatus::Running.is_active());
        assert!(ResourceStatus::Creating.is_active());
        assert!(!ResourceStatus::Stopped.is_active());
        assert!(ResourceStatus::Deleted.is_terminal());
        assert!(ResourceStatus::Failed.is_terminal());
    }

    #[test]
    fn test_cloud_resource_creation() {
        let resource = CloudResource::new(
            "my-vm",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        );

        assert_eq!(resource.name, "my-vm");
        assert_eq!(resource.provider, CloudProvider::AWS);
        assert_eq!(resource.resource_type, ResourceType::VirtualMachine);
        assert_eq!(resource.region, "us-east-1");
        assert_eq!(resource.status, ResourceStatus::Creating);
    }

    #[test]
    fn test_cloud_resource_builder() {
        let resource = CloudResource::new(
            "web-server",
            CloudProvider::GCP,
            ResourceType::VirtualMachine,
            "us-central1",
        )
        .with_tag("env", "production")
        .with_tag("team", "platform")
        .with_property("machine_type", "n1-standard-4")
        .with_cost(0.19);

        assert_eq!(resource.tags.get("env"), Some(&"production".to_string()));
        assert_eq!(resource.tags.get("team"), Some(&"platform".to_string()));
        assert_eq!(
            resource.properties.get("machine_type"),
            Some(&"n1-standard-4".to_string())
        );
        assert!((resource.cost_per_hour - 0.19).abs() < 0.001);
    }

    #[test]
    fn test_cloud_resource_arn() {
        let resource = CloudResource::new(
            "test-bucket",
            CloudProvider::AWS,
            ResourceType::ObjectStorage,
            "us-east-1",
        );

        let arn = resource.arn();
        assert!(arn.starts_with("arn:aws:s3:us-east-1:"));
    }

    #[test]
    fn test_cloud_resource_monthly_cost() {
        let resource = CloudResource::new(
            "db",
            CloudProvider::AWS,
            ResourceType::RelationalDB,
            "us-east-1",
        )
        .with_cost(0.10);

        let monthly = resource.monthly_cost();
        assert!((monthly - 72.0).abs() < 0.01); // 0.10 * 24 * 30
    }

    #[test]
    fn test_resource_spec() {
        let spec = ResourceSpec::new("my-bucket", ResourceType::ObjectStorage, "us-east-1")
            .with_size("large")
            .with_tag("env", "test")
            .with_property("versioning", "enabled");

        assert_eq!(spec.name, "my-bucket");
        assert_eq!(spec.size, "large");
        assert_eq!(spec.tags.get("env"), Some(&"test".to_string()));
    }

    #[test]
    fn test_aws_credentials() {
        let creds = CloudCredentials::aws("AKID123", "secret123").with_region("us-east-1");

        assert_eq!(creds.provider, CloudProvider::AWS);
        assert_eq!(creds.access_key, "AKID123");
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_gcp_credentials() {
        let creds = CloudCredentials::gcp("service-key", "my-project");

        assert_eq!(creds.provider, CloudProvider::GCP);
        assert_eq!(creds.project_id, Some("my-project".to_string()));
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_azure_credentials() {
        let creds = CloudCredentials::azure("tenant-id", "client-id", "secret");

        assert_eq!(creds.provider, CloudProvider::Azure);
        assert!(creds.extra.contains_key("tenant_id"));
        assert!(creds.validate().is_ok());
    }

    #[test]
    fn test_credentials_validation_failure() {
        let creds = CloudCredentials {
            provider: CloudProvider::GCP,
            access_key: "key".to_string(),
            secret_key: String::new(),
            region: None,
            project_id: None,
            extra: HashMap::new(),
        };

        assert!(creds.validate().is_err());
    }

    #[test]
    fn test_cloud_operation() {
        let mut op = CloudOperation::new(OperationType::Create, "res-1");

        assert_eq!(op.status, OperationStatus::Pending);
        assert!(op.duration().is_none());

        op.start();
        assert_eq!(op.status, OperationStatus::Running);

        op.complete();
        assert_eq!(op.status, OperationStatus::Completed);
        assert!(op.duration().is_some());
    }

    #[test]
    fn test_cloud_operation_failure() {
        let mut op = CloudOperation::new(OperationType::Delete, "res-1");
        op.start();
        op.fail("Permission denied");

        assert_eq!(op.status, OperationStatus::Failed);
        assert_eq!(op.error, Some("Permission denied".to_string()));
    }

    #[test]
    fn test_cost_tracker() {
        let mut tracker = CostTracker::new();

        let resource = CloudResource::new(
            "vm1",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_cost(0.10);

        tracker.track_resource(&resource, 100.0);

        assert!((tracker.total_spend() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_cost_tracker_by_provider() {
        let mut tracker = CostTracker::new();

        let aws_resource = CloudResource::new(
            "vm1",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_cost(0.10);

        let gcp_resource = CloudResource::new(
            "vm2",
            CloudProvider::GCP,
            ResourceType::VirtualMachine,
            "us-central1",
        )
        .with_cost(0.15);

        tracker.track_resource(&aws_resource, 100.0);
        tracker.track_resource(&gcp_resource, 100.0);

        let by_provider = tracker.spend_by_provider();
        assert!((by_provider.get(&CloudProvider::AWS).unwrap() - 10.0).abs() < 0.01);
        assert!((by_provider.get(&CloudProvider::GCP).unwrap() - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_budget_alerts() {
        let mut tracker = CostTracker::new();
        tracker.add_alert("daily-limit", 50.0, BudgetPeriod::Daily);

        let resource = CloudResource::new(
            "vm1",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_cost(1.0);

        tracker.track_resource(&resource, 100.0); // $100 spend

        let alerts = tracker.check_alerts();
        assert_eq!(alerts.len(), 1);
        assert!(alerts[0].triggered);
    }

    #[test]
    fn test_top_expensive_resources() {
        let mut tracker = CostTracker::new();

        for i in 0..5 {
            let resource = CloudResource::new(
                format!("vm{}", i),
                CloudProvider::AWS,
                ResourceType::VirtualMachine,
                "us-east-1",
            )
            .with_cost((i as f64 + 1.0) * 0.10);
            tracker.track_resource(&resource, 100.0);
        }

        let top = tracker.top_expensive(3);
        assert_eq!(top.len(), 3);
        assert!(top[0].total_cost > top[1].total_cost);
        assert!(top[1].total_cost > top[2].total_cost);
    }

    #[test]
    fn test_infrastructure_definition() {
        let mut infra = InfrastructureDefinition::new("my-app", CloudProvider::AWS);

        infra.add_variable("region", "us-east-1");
        infra.add_resource(ResourceDefinition {
            name: "vpc".to_string(),
            resource_type: ResourceType::VPC,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec![],
        });
        infra.add_resource(ResourceDefinition {
            name: "subnet".to_string(),
            resource_type: ResourceType::Subnet,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec!["vpc".to_string()],
        });

        assert!(infra.validate().is_ok());
    }

    #[test]
    fn test_infrastructure_validation_failure() {
        let mut infra = InfrastructureDefinition::new("my-app", CloudProvider::AWS);

        infra.add_resource(ResourceDefinition {
            name: "subnet".to_string(),
            resource_type: ResourceType::Subnet,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec!["nonexistent".to_string()],
        });

        assert!(infra.validate().is_err());
    }

    #[test]
    fn test_deployment_order() {
        let mut infra = InfrastructureDefinition::new("my-app", CloudProvider::AWS);

        infra.add_resource(ResourceDefinition {
            name: "app".to_string(),
            resource_type: ResourceType::VirtualMachine,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec!["subnet".to_string()],
        });
        infra.add_resource(ResourceDefinition {
            name: "vpc".to_string(),
            resource_type: ResourceType::VPC,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec![],
        });
        infra.add_resource(ResourceDefinition {
            name: "subnet".to_string(),
            resource_type: ResourceType::Subnet,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec!["vpc".to_string()],
        });

        let order = infra.deployment_order();
        let names: Vec<_> = order.iter().map(|r| &r.name).collect();

        // VPC should come before subnet, subnet before app
        let vpc_pos = names.iter().position(|n| *n == "vpc").unwrap();
        let subnet_pos = names.iter().position(|n| *n == "subnet").unwrap();
        let app_pos = names.iter().position(|n| *n == "app").unwrap();

        assert!(vpc_pos < subnet_pos);
        assert!(subnet_pos < app_pos);
    }

    #[test]
    fn test_terraform_generation() {
        let mut infra = InfrastructureDefinition::new("my-app", CloudProvider::AWS);
        infra.add_variable("region", "us-east-1");
        infra.add_resource(ResourceDefinition {
            name: "vpc".to_string(),
            resource_type: ResourceType::VPC,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec![],
        });

        let hcl = infra.to_terraform();

        assert!(hcl.contains("provider \"aws\""));
        assert!(hcl.contains("variable \"region\""));
        assert!(hcl.contains("resource \"vpc\" \"vpc\""));
    }

    #[test]
    fn test_compliance_checker_default_rules() {
        let checker = ComplianceChecker::with_defaults();
        assert!(checker.rules.len() >= 3);
    }

    #[test]
    fn test_compliance_check_missing_tag() {
        let checker = ComplianceChecker::with_defaults();

        let resource = CloudResource::new(
            "test",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        );

        let violations = checker.check_resource(&resource);

        // Should have violations for missing environment and owner tags
        assert!(violations.iter().any(|v| v.message.contains("environment")));
        assert!(violations.iter().any(|v| v.message.contains("owner")));
    }

    #[test]
    fn test_compliance_check_encryption() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "Encryption".to_string(),
            description: "Storage must be encrypted".to_string(),
            severity: ComplianceSeverity::High,
            check: ComplianceCheck::RequireEncryption,
        });

        let resource = CloudResource::new(
            "my-bucket",
            CloudProvider::AWS,
            ResourceType::ObjectStorage,
            "us-east-1",
        );

        let violations = checker.check_resource(&resource);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("not encrypted"));
    }

    #[test]
    fn test_compliance_score() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "Test".to_string(),
            description: "Test".to_string(),
            severity: ComplianceSeverity::Medium,
            check: ComplianceCheck::RequireTag("env".to_string()),
        });

        let resource1 = CloudResource::new(
            "r1",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_tag("env", "prod");
        let resource2 = CloudResource::new(
            "r2",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        );

        let resources = vec![resource1, resource2];
        let score = checker.compliance_score(&resources);

        // 1 of 2 resources compliant = 50%
        assert!((score - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_cloud_manager_creation() {
        let manager = CloudManager::new();
        assert!(manager.list_active().is_empty());
    }

    #[test]
    fn test_cloud_manager_add_credentials() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        assert!(manager.credentials.contains_key(&CloudProvider::AWS));
    }

    #[test]
    fn test_cloud_manager_create_resource() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let spec = ResourceSpec::new("my-vm", ResourceType::VirtualMachine, "us-east-1");
        let result = manager.create_resource(CloudProvider::AWS, spec);

        assert!(result.is_ok());
        let resource = result.unwrap();
        assert_eq!(resource.name, "my-vm");
        assert_eq!(resource.status, ResourceStatus::Running);
    }

    #[test]
    fn test_cloud_manager_no_credentials() {
        let mut manager = CloudManager::new();

        let spec = ResourceSpec::new("my-vm", ResourceType::VirtualMachine, "us-east-1");
        let result = manager.create_resource(CloudProvider::AWS, spec);

        assert!(result.is_err());
    }

    #[test]
    fn test_cloud_manager_invalid_region() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let spec = ResourceSpec::new("my-vm", ResourceType::VirtualMachine, "invalid-region");
        let result = manager.create_resource(CloudProvider::AWS, spec);

        assert!(result.is_err());
    }

    #[test]
    fn test_cloud_manager_delete_resource() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let spec = ResourceSpec::new("my-vm", ResourceType::VirtualMachine, "us-east-1");
        let resource = manager.create_resource(CloudProvider::AWS, spec).unwrap();

        let result = manager.delete_resource(&resource.id);
        assert!(result.is_ok());

        let deleted = manager.get_resource(&resource.id).unwrap();
        assert_eq!(deleted.status, ResourceStatus::Deleted);
    }

    #[test]
    fn test_cloud_manager_delete_with_dependents() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let vpc_spec = ResourceSpec::new("vpc", ResourceType::VPC, "us-east-1");
        let vpc = manager
            .create_resource(CloudProvider::AWS, vpc_spec)
            .unwrap();

        // Create subnet that depends on VPC
        let mut subnet = CloudResource::new(
            "subnet",
            CloudProvider::AWS,
            ResourceType::Subnet,
            "us-east-1",
        )
        .with_dependency(&vpc.id);
        subnet.set_status(ResourceStatus::Running);
        manager.resources.insert(subnet.id.clone(), subnet);

        // Should fail because subnet depends on VPC
        let result = manager.delete_resource(&vpc.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_cloud_manager_list_by_provider() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));
        manager.add_credentials(CloudCredentials::gcp("key", "project"));

        let aws_spec = ResourceSpec::new("aws-vm", ResourceType::VirtualMachine, "us-east-1");
        manager
            .create_resource(CloudProvider::AWS, aws_spec)
            .unwrap();

        let gcp_spec = ResourceSpec::new("gcp-vm", ResourceType::VirtualMachine, "us-central1");
        manager
            .create_resource(CloudProvider::GCP, gcp_spec)
            .unwrap();

        let aws_resources = manager.list_by_provider(CloudProvider::AWS);
        assert_eq!(aws_resources.len(), 1);
        assert_eq!(aws_resources[0].name, "aws-vm");
    }

    #[test]
    fn test_cloud_manager_list_by_type() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let vm_spec = ResourceSpec::new("vm", ResourceType::VirtualMachine, "us-east-1");
        manager
            .create_resource(CloudProvider::AWS, vm_spec)
            .unwrap();

        let bucket_spec = ResourceSpec::new("bucket", ResourceType::ObjectStorage, "us-east-1");
        manager
            .create_resource(CloudProvider::AWS, bucket_spec)
            .unwrap();

        let vms = manager.list_by_type(ResourceType::VirtualMachine);
        assert_eq!(vms.len(), 1);
        assert_eq!(vms[0].name, "vm");
    }

    #[test]
    fn test_cloud_manager_deploy_infrastructure() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let mut infra = InfrastructureDefinition::new("app", CloudProvider::AWS);
        infra.add_resource(ResourceDefinition {
            name: "vpc".to_string(),
            resource_type: ResourceType::VPC,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec![],
        });
        infra.add_resource(ResourceDefinition {
            name: "subnet".to_string(),
            resource_type: ResourceType::Subnet,
            region: "us-east-1".to_string(),
            properties: HashMap::new(),
            depends_on: vec!["vpc".to_string()],
        });

        let result = manager.deploy(&infra);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_cloud_manager_operations_history() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let spec = ResourceSpec::new("vm", ResourceType::VirtualMachine, "us-east-1");
        let resource = manager.create_resource(CloudProvider::AWS, spec).unwrap();

        manager.delete_resource(&resource.id).unwrap();

        let ops = manager.operations();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].operation_type, OperationType::Create);
        assert_eq!(ops[1].operation_type, OperationType::Delete);
    }

    #[test]
    fn test_cloud_manager_compliance_check() {
        let mut manager = CloudManager::new();
        manager.add_credentials(CloudCredentials::aws("key", "secret"));

        let spec = ResourceSpec::new("vm", ResourceType::VirtualMachine, "us-east-1");
        manager.create_resource(CloudProvider::AWS, spec).unwrap();

        let violations = manager.check_compliance();

        // Should have violations for missing tags (default rules)
        assert!(!violations.is_empty());
    }

    #[test]
    fn test_resource_discovery() {
        let discovery = ResourceDiscovery::new();
        assert!(discovery.find_unmanaged().is_empty());
    }

    #[test]
    fn test_discovered_resource_import() {
        let discovery = ResourceDiscovery::new();

        let discovered = DiscoveredResource {
            provider: CloudProvider::AWS,
            resource_type: ResourceType::VirtualMachine,
            id: "i-12345".to_string(),
            name: "discovered-vm".to_string(),
            region: "us-east-1".to_string(),
            tags: HashMap::new(),
            created_at: Some(SystemTime::now()),
            managed: false,
        };

        let resource = discovery.import(&discovered);
        assert_eq!(resource.id, "i-12345");
        assert_eq!(resource.name, "discovered-vm");
        assert_eq!(resource.status, ResourceStatus::Running);
    }

    #[test]
    fn test_compliance_severity_ordering() {
        assert!(ComplianceSeverity::Critical > ComplianceSeverity::High);
        assert!(ComplianceSeverity::High > ComplianceSeverity::Medium);
        assert!(ComplianceSeverity::Medium > ComplianceSeverity::Low);
        assert!(ComplianceSeverity::Low > ComplianceSeverity::Info);
    }

    #[test]
    fn test_max_cost_compliance_rule() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "Max cost".to_string(),
            description: "Max $100/month".to_string(),
            severity: ComplianceSeverity::High,
            check: ComplianceCheck::MaxCost(100.0),
        });

        let expensive = CloudResource::new(
            "expensive-vm",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_cost(0.50); // $360/month

        let violations = checker.check_resource(&expensive);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("exceeds limit"));
    }

    #[test]
    fn test_allowed_regions_compliance_rule() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "EU only".to_string(),
            description: "Resources must be in EU".to_string(),
            severity: ComplianceSeverity::Medium,
            check: ComplianceCheck::AllowedRegions(vec![
                "eu-west-1".to_string(),
                "eu-central-1".to_string(),
            ]),
        });

        let us_resource = CloudResource::new(
            "vm",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        );

        let violations = checker.check_resource(&us_resource);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("not in allowed list"));
    }

    #[test]
    fn test_public_access_compliance_rule() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "No public".to_string(),
            description: "No public access".to_string(),
            severity: ComplianceSeverity::Critical,
            check: ComplianceCheck::RequirePrivateAccess,
        });

        let public_bucket = CloudResource::new(
            "bucket",
            CloudProvider::AWS,
            ResourceType::ObjectStorage,
            "us-east-1",
        )
        .with_property("public_access", "true");

        let violations = checker.check_resource(&public_bucket);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("public access"));
    }

    #[test]
    fn test_backup_compliance_rule() {
        let mut checker = ComplianceChecker::new();
        checker.add_rule(ComplianceRule {
            id: "R001".to_string(),
            name: "Backup required".to_string(),
            description: "Backup must be enabled".to_string(),
            severity: ComplianceSeverity::High,
            check: ComplianceCheck::RequireBackup,
        });

        let db = CloudResource::new(
            "db",
            CloudProvider::AWS,
            ResourceType::RelationalDB,
            "us-east-1",
        );

        let violations = checker.check_resource(&db);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("Backup is not enabled"));
    }

    #[test]
    fn test_compliant_resource() {
        let checker = ComplianceChecker::with_defaults();

        let resource = CloudResource::new(
            "compliant-vm",
            CloudProvider::AWS,
            ResourceType::VirtualMachine,
            "us-east-1",
        )
        .with_tag("environment", "production")
        .with_tag("owner", "team-platform");

        let violations = checker.check_resource(&resource);

        // Storage encryption rule doesn't apply to VMs
        // So only tag rules matter, which are satisfied
        let tag_violations: Vec<_> = violations
            .iter()
            .filter(|v| v.message.contains("tag"))
            .collect();
        assert!(tag_violations.is_empty());
    }
}
