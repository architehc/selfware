//! Kubernetes Operator
//!
//! Kubernetes resource management features:
//! - Deployment management
//! - Scaling operations
//! - Log analysis
//! - Troubleshooting
//! - Manifest generation

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Global counters for unique IDs
static RESOURCE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static MANIFEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_resource_id() -> String {
    format!(
        "res_{}_{:x}",
        RESOURCE_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn generate_manifest_id() -> String {
    format!(
        "manifest_{}_{:x}",
        MANIFEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
        current_timestamp()
    )
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// Kubernetes Resource Types
// ============================================================================

/// Kubernetes API version
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiVersion {
    V1,
    AppsV1,
    BatchV1,
    NetworkingV1,
    AutoscalingV1,
    AutoscalingV2,
    RbacV1,
    Custom(String),
}

impl ApiVersion {
    pub fn as_str(&self) -> &str {
        match self {
            ApiVersion::V1 => "v1",
            ApiVersion::AppsV1 => "apps/v1",
            ApiVersion::BatchV1 => "batch/v1",
            ApiVersion::NetworkingV1 => "networking.k8s.io/v1",
            ApiVersion::AutoscalingV1 => "autoscaling/v1",
            ApiVersion::AutoscalingV2 => "autoscaling/v2",
            ApiVersion::RbacV1 => "rbac.authorization.k8s.io/v1",
            ApiVersion::Custom(s) => s.as_str(),
        }
    }
}

/// Kubernetes resource kind
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Pod,
    Deployment,
    Service,
    ConfigMap,
    Secret,
    Namespace,
    ServiceAccount,
    Ingress,
    PersistentVolumeClaim,
    PersistentVolume,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
    HorizontalPodAutoscaler,
    NetworkPolicy,
    Role,
    RoleBinding,
    ClusterRole,
    ClusterRoleBinding,
    Custom(String),
}

impl ResourceKind {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceKind::Pod => "Pod",
            ResourceKind::Deployment => "Deployment",
            ResourceKind::Service => "Service",
            ResourceKind::ConfigMap => "ConfigMap",
            ResourceKind::Secret => "Secret",
            ResourceKind::Namespace => "Namespace",
            ResourceKind::ServiceAccount => "ServiceAccount",
            ResourceKind::Ingress => "Ingress",
            ResourceKind::PersistentVolumeClaim => "PersistentVolumeClaim",
            ResourceKind::PersistentVolume => "PersistentVolume",
            ResourceKind::StatefulSet => "StatefulSet",
            ResourceKind::DaemonSet => "DaemonSet",
            ResourceKind::Job => "Job",
            ResourceKind::CronJob => "CronJob",
            ResourceKind::HorizontalPodAutoscaler => "HorizontalPodAutoscaler",
            ResourceKind::NetworkPolicy => "NetworkPolicy",
            ResourceKind::Role => "Role",
            ResourceKind::RoleBinding => "RoleBinding",
            ResourceKind::ClusterRole => "ClusterRole",
            ResourceKind::ClusterRoleBinding => "ClusterRoleBinding",
            ResourceKind::Custom(s) => s.as_str(),
        }
    }

    pub fn api_version(&self) -> ApiVersion {
        match self {
            ResourceKind::Pod
            | ResourceKind::Service
            | ResourceKind::ConfigMap
            | ResourceKind::Secret
            | ResourceKind::Namespace
            | ResourceKind::ServiceAccount
            | ResourceKind::PersistentVolumeClaim
            | ResourceKind::PersistentVolume => ApiVersion::V1,
            ResourceKind::Deployment | ResourceKind::StatefulSet | ResourceKind::DaemonSet => {
                ApiVersion::AppsV1
            }
            ResourceKind::Job | ResourceKind::CronJob => ApiVersion::BatchV1,
            ResourceKind::Ingress | ResourceKind::NetworkPolicy => ApiVersion::NetworkingV1,
            ResourceKind::HorizontalPodAutoscaler => ApiVersion::AutoscalingV2,
            ResourceKind::Role
            | ResourceKind::RoleBinding
            | ResourceKind::ClusterRole
            | ResourceKind::ClusterRoleBinding => ApiVersion::RbacV1,
            ResourceKind::Custom(_) => ApiVersion::V1,
        }
    }
}

/// Resource status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
    Terminating,
    Creating,
    Scaling,
}

impl ResourceStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceStatus::Pending => "Pending",
            ResourceStatus::Running => "Running",
            ResourceStatus::Succeeded => "Succeeded",
            ResourceStatus::Failed => "Failed",
            ResourceStatus::Unknown => "Unknown",
            ResourceStatus::Terminating => "Terminating",
            ResourceStatus::Creating => "Creating",
            ResourceStatus::Scaling => "Scaling",
        }
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self, ResourceStatus::Running | ResourceStatus::Succeeded)
    }
}

/// Container port
#[derive(Debug, Clone)]
pub struct ContainerPort {
    pub name: Option<String>,
    pub container_port: u16,
    pub protocol: String,
}

impl ContainerPort {
    pub fn new(port: u16) -> Self {
        Self {
            name: None,
            container_port: port,
            protocol: "TCP".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn udp(mut self) -> Self {
        self.protocol = "UDP".to_string();
        self
    }
}

/// Container resource requirements
#[derive(Debug, Clone, Default)]
pub struct ResourceRequirements {
    pub cpu_request: Option<String>,
    pub cpu_limit: Option<String>,
    pub memory_request: Option<String>,
    pub memory_limit: Option<String>,
}

impl ResourceRequirements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cpu(mut self, request: impl Into<String>, limit: impl Into<String>) -> Self {
        self.cpu_request = Some(request.into());
        self.cpu_limit = Some(limit.into());
        self
    }

    pub fn with_memory(mut self, request: impl Into<String>, limit: impl Into<String>) -> Self {
        self.memory_request = Some(request.into());
        self.memory_limit = Some(limit.into());
        self
    }
}

/// Container specification
#[derive(Debug, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub ports: Vec<ContainerPort>,
    pub env: HashMap<String, String>,
    pub resources: ResourceRequirements,
    pub command: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub image_pull_policy: String,
}

impl ContainerSpec {
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            ports: Vec::new(),
            env: HashMap::new(),
            resources: ResourceRequirements::default(),
            command: None,
            args: None,
            image_pull_policy: "IfNotPresent".to_string(),
        }
    }

    pub fn with_port(mut self, port: ContainerPort) -> Self {
        self.ports.push(port);
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn with_resources(mut self, resources: ResourceRequirements) -> Self {
        self.resources = resources;
        self
    }

    pub fn with_command(mut self, command: Vec<String>) -> Self {
        self.command = Some(command);
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = Some(args);
        self
    }

    pub fn always_pull(mut self) -> Self {
        self.image_pull_policy = "Always".to_string();
        self
    }
}

// ============================================================================
// Kubernetes Resources
// ============================================================================

/// Kubernetes resource metadata
#[derive(Debug, Clone)]
pub struct ResourceMetadata {
    pub name: String,
    pub namespace: Option<String>,
    pub labels: HashMap<String, String>,
    pub annotations: HashMap<String, String>,
}

impl ResourceMetadata {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            namespace: None,
            labels: HashMap::new(),
            annotations: HashMap::new(),
        }
    }

    pub fn in_namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = Some(ns.into());
        self
    }

    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    pub fn with_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }
}

/// A Kubernetes resource
#[derive(Debug, Clone)]
pub struct K8sResource {
    /// Internal ID
    pub id: String,
    /// API version
    pub api_version: ApiVersion,
    /// Resource kind
    pub kind: ResourceKind,
    /// Metadata
    pub metadata: ResourceMetadata,
    /// Resource-specific spec (as YAML/JSON string)
    pub spec: Option<String>,
    /// Status
    pub status: ResourceStatus,
    /// Created timestamp
    pub created_at: u64,
    /// Last updated
    pub updated_at: u64,
}

impl K8sResource {
    pub fn new(kind: ResourceKind, metadata: ResourceMetadata) -> Self {
        let now = current_timestamp();
        Self {
            id: generate_resource_id(),
            api_version: kind.api_version(),
            kind,
            metadata,
            spec: None,
            status: ResourceStatus::Pending,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_spec(mut self, spec: impl Into<String>) -> Self {
        self.spec = Some(spec.into());
        self
    }

    pub fn with_status(mut self, status: ResourceStatus) -> Self {
        self.status = status;
        self
    }

    /// Get full resource name (namespace/name or just name)
    pub fn full_name(&self) -> String {
        match &self.metadata.namespace {
            Some(ns) => format!("{}/{}", ns, self.metadata.name),
            None => self.metadata.name.clone(),
        }
    }

    /// Check if resource is healthy
    pub fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }
}

// ============================================================================
// Deployment Management
// ============================================================================

/// Deployment strategy type
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DeploymentStrategy {
    #[default]
    RollingUpdate,
    Recreate,
}

impl DeploymentStrategy {
    pub fn as_str(&self) -> &str {
        match self {
            DeploymentStrategy::RollingUpdate => "RollingUpdate",
            DeploymentStrategy::Recreate => "Recreate",
        }
    }
}

/// Deployment specification
#[derive(Debug, Clone)]
pub struct DeploymentSpec {
    pub replicas: u32,
    pub selector: HashMap<String, String>,
    pub containers: Vec<ContainerSpec>,
    pub strategy: DeploymentStrategy,
    pub max_unavailable: Option<String>,
    pub max_surge: Option<String>,
}

impl DeploymentSpec {
    pub fn new(replicas: u32) -> Self {
        Self {
            replicas,
            selector: HashMap::new(),
            containers: Vec::new(),
            strategy: DeploymentStrategy::default(),
            max_unavailable: Some("25%".to_string()),
            max_surge: Some("25%".to_string()),
        }
    }

    pub fn with_selector(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.selector.insert(key.into(), value.into());
        self
    }

    pub fn with_container(mut self, container: ContainerSpec) -> Self {
        self.containers.push(container);
        self
    }

    pub fn with_strategy(mut self, strategy: DeploymentStrategy) -> Self {
        self.strategy = strategy;
        self
    }
}

/// Deployment manager
#[derive(Debug, Default)]
pub struct DeploymentManager {
    deployments: HashMap<String, K8sResource>,
    specs: HashMap<String, DeploymentSpec>,
}

impl DeploymentManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a deployment
    pub fn create(
        &mut self,
        name: impl Into<String>,
        namespace: impl Into<String>,
        spec: DeploymentSpec,
    ) -> String {
        let name_str = name.into();
        let metadata = ResourceMetadata::new(&name_str).in_namespace(namespace);

        // Add app label from selector
        let metadata = if let Some((k, v)) = spec.selector.iter().next() {
            metadata.with_label(k, v)
        } else {
            metadata.with_label("app", &name_str)
        };

        let resource = K8sResource::new(ResourceKind::Deployment, metadata)
            .with_status(ResourceStatus::Creating);

        let id = resource.id.clone();
        self.deployments.insert(id.clone(), resource);
        self.specs.insert(id.clone(), spec);
        id
    }

    /// Get a deployment
    pub fn get(&self, id: &str) -> Option<&K8sResource> {
        self.deployments.get(id)
    }

    /// Get deployment spec
    pub fn get_spec(&self, id: &str) -> Option<&DeploymentSpec> {
        self.specs.get(id)
    }

    /// Scale a deployment
    pub fn scale(&mut self, id: &str, replicas: u32) -> bool {
        if let Some(spec) = self.specs.get_mut(id) {
            spec.replicas = replicas;
            if let Some(resource) = self.deployments.get_mut(id) {
                resource.status = ResourceStatus::Scaling;
                resource.updated_at = current_timestamp();
            }
            true
        } else {
            false
        }
    }

    /// Update deployment image
    pub fn update_image(&mut self, id: &str, container_name: &str, new_image: &str) -> bool {
        if let Some(spec) = self.specs.get_mut(id) {
            for container in &mut spec.containers {
                if container.name == container_name {
                    container.image = new_image.to_string();
                    if let Some(resource) = self.deployments.get_mut(id) {
                        resource.updated_at = current_timestamp();
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Delete a deployment
    pub fn delete(&mut self, id: &str) -> Option<K8sResource> {
        self.specs.remove(id);
        self.deployments.remove(id)
    }

    /// List all deployments
    pub fn list(&self) -> Vec<&K8sResource> {
        self.deployments.values().collect()
    }

    /// List deployments in namespace
    pub fn list_in_namespace(&self, namespace: &str) -> Vec<&K8sResource> {
        self.deployments
            .values()
            .filter(|d| d.metadata.namespace.as_deref() == Some(namespace))
            .collect()
    }

    /// Get deployment count
    pub fn count(&self) -> usize {
        self.deployments.len()
    }
}

// ============================================================================
// Service Management
// ============================================================================

/// Service type
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServiceType {
    #[default]
    ClusterIP,
    NodePort,
    LoadBalancer,
    ExternalName,
}

impl ServiceType {
    pub fn as_str(&self) -> &str {
        match self {
            ServiceType::ClusterIP => "ClusterIP",
            ServiceType::NodePort => "NodePort",
            ServiceType::LoadBalancer => "LoadBalancer",
            ServiceType::ExternalName => "ExternalName",
        }
    }
}

/// Service port
#[derive(Debug, Clone)]
pub struct ServicePort {
    pub name: Option<String>,
    pub port: u16,
    pub target_port: u16,
    pub node_port: Option<u16>,
    pub protocol: String,
}

impl ServicePort {
    pub fn new(port: u16, target_port: u16) -> Self {
        Self {
            name: None,
            port,
            target_port,
            node_port: None,
            protocol: "TCP".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_node_port(mut self, port: u16) -> Self {
        self.node_port = Some(port);
        self
    }
}

/// Service specification
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub service_type: ServiceType,
    pub selector: HashMap<String, String>,
    pub ports: Vec<ServicePort>,
    pub external_name: Option<String>,
}

impl ServiceSpec {
    pub fn new(service_type: ServiceType) -> Self {
        Self {
            service_type,
            selector: HashMap::new(),
            ports: Vec::new(),
            external_name: None,
        }
    }

    pub fn with_selector(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.selector.insert(key.into(), value.into());
        self
    }

    pub fn with_port(mut self, port: ServicePort) -> Self {
        self.ports.push(port);
        self
    }

    pub fn external_name(mut self, name: impl Into<String>) -> Self {
        self.external_name = Some(name.into());
        self
    }
}

// ============================================================================
// Manifest Generation
// ============================================================================

/// Manifest format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManifestFormat {
    #[default]
    Yaml,
    Json,
}

impl ManifestFormat {
    pub fn as_str(&self) -> &str {
        match self {
            ManifestFormat::Yaml => "yaml",
            ManifestFormat::Json => "json",
        }
    }

    pub fn file_extension(&self) -> &str {
        match self {
            ManifestFormat::Yaml => ".yaml",
            ManifestFormat::Json => ".json",
        }
    }
}

/// Generated manifest
#[derive(Debug, Clone)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub format: ManifestFormat,
    pub content: String,
    pub resources: Vec<ResourceKind>,
    pub created_at: u64,
}

impl Manifest {
    pub fn new(
        name: impl Into<String>,
        format: ManifestFormat,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: generate_manifest_id(),
            name: name.into(),
            format,
            content: content.into(),
            resources: Vec::new(),
            created_at: current_timestamp(),
        }
    }

    pub fn with_resource(mut self, kind: ResourceKind) -> Self {
        self.resources.push(kind);
        self
    }
}

/// Manifest generator
#[derive(Debug, Default)]
pub struct ManifestGenerator {
    format: ManifestFormat,
}

impl ManifestGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_format(mut self, format: ManifestFormat) -> Self {
        self.format = format;
        self
    }

    /// Generate deployment manifest
    pub fn deployment(&self, name: &str, namespace: &str, spec: &DeploymentSpec) -> Manifest {
        let mut yaml = String::new();

        yaml.push_str(&format!("apiVersion: {}\n", ApiVersion::AppsV1.as_str()));
        yaml.push_str(&format!("kind: {}\n", ResourceKind::Deployment.as_str()));
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", name));
        yaml.push_str(&format!("  namespace: {}\n", namespace));

        if !spec.selector.is_empty() {
            yaml.push_str("  labels:\n");
            for (k, v) in &spec.selector {
                yaml.push_str(&format!("    {}: {}\n", k, v));
            }
        }

        yaml.push_str("spec:\n");
        yaml.push_str(&format!("  replicas: {}\n", spec.replicas));

        if !spec.selector.is_empty() {
            yaml.push_str("  selector:\n");
            yaml.push_str("    matchLabels:\n");
            for (k, v) in &spec.selector {
                yaml.push_str(&format!("      {}: {}\n", k, v));
            }
        }

        yaml.push_str("  strategy:\n");
        yaml.push_str(&format!("    type: {}\n", spec.strategy.as_str()));

        yaml.push_str("  template:\n");
        yaml.push_str("    metadata:\n");
        if !spec.selector.is_empty() {
            yaml.push_str("      labels:\n");
            for (k, v) in &spec.selector {
                yaml.push_str(&format!("        {}: {}\n", k, v));
            }
        }
        yaml.push_str("    spec:\n");
        yaml.push_str("      containers:\n");

        for container in &spec.containers {
            yaml.push_str(&format!("        - name: {}\n", container.name));
            yaml.push_str(&format!("          image: {}\n", container.image));
            yaml.push_str(&format!(
                "          imagePullPolicy: {}\n",
                container.image_pull_policy
            ));

            if !container.ports.is_empty() {
                yaml.push_str("          ports:\n");
                for port in &container.ports {
                    yaml.push_str(&format!(
                        "            - containerPort: {}\n",
                        port.container_port
                    ));
                    yaml.push_str(&format!("              protocol: {}\n", port.protocol));
                }
            }

            if !container.env.is_empty() {
                yaml.push_str("          env:\n");
                for (k, v) in &container.env {
                    yaml.push_str(&format!("            - name: {}\n", k));
                    yaml.push_str(&format!("              value: \"{}\"\n", v));
                }
            }
        }

        Manifest::new(format!("{}-deployment", name), self.format, yaml)
            .with_resource(ResourceKind::Deployment)
    }

    /// Generate service manifest
    pub fn service(&self, name: &str, namespace: &str, spec: &ServiceSpec) -> Manifest {
        let mut yaml = String::new();

        yaml.push_str(&format!("apiVersion: {}\n", ApiVersion::V1.as_str()));
        yaml.push_str(&format!("kind: {}\n", ResourceKind::Service.as_str()));
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", name));
        yaml.push_str(&format!("  namespace: {}\n", namespace));

        yaml.push_str("spec:\n");
        yaml.push_str(&format!("  type: {}\n", spec.service_type.as_str()));

        if !spec.selector.is_empty() {
            yaml.push_str("  selector:\n");
            for (k, v) in &spec.selector {
                yaml.push_str(&format!("    {}: {}\n", k, v));
            }
        }

        if !spec.ports.is_empty() {
            yaml.push_str("  ports:\n");
            for port in &spec.ports {
                yaml.push_str(&format!("    - port: {}\n", port.port));
                yaml.push_str(&format!("      targetPort: {}\n", port.target_port));
                yaml.push_str(&format!("      protocol: {}\n", port.protocol));
                if let Some(ref name) = port.name {
                    yaml.push_str(&format!("      name: {}\n", name));
                }
            }
        }

        Manifest::new(format!("{}-service", name), self.format, yaml)
            .with_resource(ResourceKind::Service)
    }

    /// Generate ConfigMap manifest
    pub fn config_map(
        &self,
        name: &str,
        namespace: &str,
        data: &HashMap<String, String>,
    ) -> Manifest {
        let mut yaml = String::new();

        yaml.push_str(&format!("apiVersion: {}\n", ApiVersion::V1.as_str()));
        yaml.push_str(&format!("kind: {}\n", ResourceKind::ConfigMap.as_str()));
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", name));
        yaml.push_str(&format!("  namespace: {}\n", namespace));

        if !data.is_empty() {
            yaml.push_str("data:\n");
            for (k, v) in data {
                yaml.push_str(&format!("  {}: |\n", k));
                for line in v.lines() {
                    yaml.push_str(&format!("    {}\n", line));
                }
            }
        }

        Manifest::new(format!("{}-configmap", name), self.format, yaml)
            .with_resource(ResourceKind::ConfigMap)
    }

    /// Generate HPA manifest
    pub fn hpa(
        &self,
        name: &str,
        namespace: &str,
        target_deployment: &str,
        min_replicas: u32,
        max_replicas: u32,
        target_cpu_percent: u32,
    ) -> Manifest {
        let mut yaml = String::new();

        yaml.push_str(&format!(
            "apiVersion: {}\n",
            ApiVersion::AutoscalingV2.as_str()
        ));
        yaml.push_str(&format!(
            "kind: {}\n",
            ResourceKind::HorizontalPodAutoscaler.as_str()
        ));
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", name));
        yaml.push_str(&format!("  namespace: {}\n", namespace));

        yaml.push_str("spec:\n");
        yaml.push_str("  scaleTargetRef:\n");
        yaml.push_str(&format!(
            "    apiVersion: {}\n",
            ApiVersion::AppsV1.as_str()
        ));
        yaml.push_str("    kind: Deployment\n");
        yaml.push_str(&format!("    name: {}\n", target_deployment));

        yaml.push_str(&format!("  minReplicas: {}\n", min_replicas));
        yaml.push_str(&format!("  maxReplicas: {}\n", max_replicas));

        yaml.push_str("  metrics:\n");
        yaml.push_str("    - type: Resource\n");
        yaml.push_str("      resource:\n");
        yaml.push_str("        name: cpu\n");
        yaml.push_str("        target:\n");
        yaml.push_str("          type: Utilization\n");
        yaml.push_str(&format!(
            "          averageUtilization: {}\n",
            target_cpu_percent
        ));

        Manifest::new(format!("{}-hpa", name), self.format, yaml)
            .with_resource(ResourceKind::HorizontalPodAutoscaler)
    }
}

// ============================================================================
// Troubleshooting
// ============================================================================

/// Issue severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

impl IssueSeverity {
    pub fn as_str(&self) -> &str {
        match self {
            IssueSeverity::Info => "info",
            IssueSeverity::Warning => "warning",
            IssueSeverity::Error => "error",
            IssueSeverity::Critical => "critical",
        }
    }
}

/// Detected issue
#[derive(Debug, Clone)]
pub struct K8sIssue {
    pub resource_id: String,
    pub resource_name: String,
    pub kind: ResourceKind,
    pub severity: IssueSeverity,
    pub title: String,
    pub description: String,
    pub suggested_fix: Option<String>,
    pub detected_at: u64,
}

impl K8sIssue {
    pub fn new(
        resource_id: impl Into<String>,
        resource_name: impl Into<String>,
        kind: ResourceKind,
        severity: IssueSeverity,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            resource_id: resource_id.into(),
            resource_name: resource_name.into(),
            kind,
            severity,
            title: title.into(),
            description: description.into(),
            suggested_fix: None,
            detected_at: current_timestamp(),
        }
    }

    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }
}

/// Troubleshooting analyzer
#[derive(Debug, Default)]
pub struct TroubleshootingAnalyzer {
    issues: Vec<K8sIssue>,
}

impl TroubleshootingAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyze a deployment for issues
    pub fn analyze_deployment(&mut self, resource: &K8sResource, spec: &DeploymentSpec) {
        // Check for zero replicas
        if spec.replicas == 0 {
            self.issues.push(
                K8sIssue::new(
                    &resource.id,
                    &resource.metadata.name,
                    ResourceKind::Deployment,
                    IssueSeverity::Warning,
                    "Zero replicas",
                    "Deployment has 0 replicas - no pods will be running",
                )
                .with_fix("Scale the deployment to at least 1 replica"),
            );
        }

        // Check for missing resource limits
        for container in &spec.containers {
            if container.resources.cpu_limit.is_none() || container.resources.memory_limit.is_none()
            {
                self.issues.push(K8sIssue::new(
                    &resource.id,
                    &resource.metadata.name,
                    ResourceKind::Deployment,
                    IssueSeverity::Warning,
                    "Missing resource limits",
                    format!("Container '{}' has no resource limits set", container.name),
                ));
            }

            // Check for latest tag
            if container.image.ends_with(":latest") || !container.image.contains(':') {
                self.issues.push(
                    K8sIssue::new(
                        &resource.id,
                        &resource.metadata.name,
                        ResourceKind::Deployment,
                        IssueSeverity::Warning,
                        "Using 'latest' tag",
                        format!(
                            "Container '{}' uses 'latest' or untagged image",
                            container.name
                        ),
                    )
                    .with_fix("Use a specific version tag for reproducible deployments"),
                );
            }
        }

        // Check for single replica without HPA
        if spec.replicas == 1 && spec.strategy == DeploymentStrategy::RollingUpdate {
            self.issues.push(
                K8sIssue::new(
                    &resource.id,
                    &resource.metadata.name,
                    ResourceKind::Deployment,
                    IssueSeverity::Info,
                    "Single replica deployment",
                    "Deployment has only 1 replica - may cause downtime during updates",
                )
                .with_fix("Consider using multiple replicas or a Recreate strategy"),
            );
        }
    }

    /// Get all issues
    pub fn issues(&self) -> &[K8sIssue] {
        &self.issues
    }

    /// Get issues by severity
    pub fn by_severity(&self, severity: IssueSeverity) -> Vec<&K8sIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity == severity)
            .collect()
    }

    /// Get critical issues
    pub fn critical(&self) -> Vec<&K8sIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity >= IssueSeverity::Error)
            .collect()
    }

    /// Clear issues
    pub fn clear(&mut self) {
        self.issues.clear();
    }

    /// Get issue count
    pub fn count(&self) -> usize {
        self.issues.len()
    }
}

// ============================================================================
// Log Analysis
// ============================================================================

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
    Fatal,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warning => "WARNING",
            LogLevel::Error => "ERROR",
            LogLevel::Fatal => "FATAL",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "DEBUG" | "TRACE" => LogLevel::Debug,
            "INFO" => LogLevel::Info,
            "WARN" | "WARNING" => LogLevel::Warning,
            "ERROR" | "ERR" => LogLevel::Error,
            "FATAL" | "CRITICAL" | "PANIC" => LogLevel::Fatal,
            _ => LogLevel::Info,
        }
    }
}

/// Log entry
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: Option<u64>,
    pub level: LogLevel,
    pub message: String,
    pub container: Option<String>,
    pub pod: Option<String>,
}

impl LogEntry {
    pub fn new(level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            timestamp: Some(current_timestamp()),
            level,
            message: message.into(),
            container: None,
            pod: None,
        }
    }

    pub fn from_pod(mut self, pod: impl Into<String>) -> Self {
        self.pod = Some(pod.into());
        self
    }

    pub fn from_container(mut self, container: impl Into<String>) -> Self {
        self.container = Some(container.into());
        self
    }
}

/// Log analyzer
#[derive(Debug, Default)]
pub struct LogAnalyzer {
    entries: Vec<LogEntry>,
    _error_patterns: Vec<String>,
}

impl LogAnalyzer {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            _error_patterns: vec![
                "error".to_string(),
                "exception".to_string(),
                "panic".to_string(),
                "fatal".to_string(),
                "failed".to_string(),
                "timeout".to_string(),
                "connection refused".to_string(),
                "out of memory".to_string(),
                "killed".to_string(),
            ],
        }
    }

    /// Parse log line
    pub fn parse_line(&self, line: &str) -> LogEntry {
        let level = if line.contains("ERROR") || line.contains("error") {
            LogLevel::Error
        } else if line.contains("WARN") || line.contains("warn") {
            LogLevel::Warning
        } else if line.contains("DEBUG") || line.contains("debug") {
            LogLevel::Debug
        } else if line.contains("FATAL") || line.contains("panic") {
            LogLevel::Fatal
        } else {
            LogLevel::Info
        };

        LogEntry::new(level, line)
    }

    /// Add log entry
    pub fn add(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    /// Parse multiple lines
    pub fn parse_logs(&mut self, logs: &str, pod: Option<&str>, container: Option<&str>) {
        for line in logs.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let mut entry = self.parse_line(line);
            if let Some(p) = pod {
                entry = entry.from_pod(p);
            }
            if let Some(c) = container {
                entry = entry.from_container(c);
            }
            self.add(entry);
        }
    }

    /// Get errors
    pub fn errors(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| matches!(e.level, LogLevel::Error | LogLevel::Fatal))
            .collect()
    }

    /// Get warnings
    pub fn warnings(&self) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.level == LogLevel::Warning)
            .collect()
    }

    /// Search for pattern
    pub fn search(&self, pattern: &str) -> Vec<&LogEntry> {
        let pattern_lower = pattern.to_lowercase();
        self.entries
            .iter()
            .filter(|e| e.message.to_lowercase().contains(&pattern_lower))
            .collect()
    }

    /// Get entry count
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.level, LogLevel::Error | LogLevel::Fatal))
            .count()
    }

    /// Clear entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // API Version Tests

    #[test]
    fn test_api_version_as_str() {
        assert_eq!(ApiVersion::V1.as_str(), "v1");
        assert_eq!(ApiVersion::AppsV1.as_str(), "apps/v1");
        assert_eq!(ApiVersion::BatchV1.as_str(), "batch/v1");
    }

    // Resource Kind Tests

    #[test]
    fn test_resource_kind_as_str() {
        assert_eq!(ResourceKind::Pod.as_str(), "Pod");
        assert_eq!(ResourceKind::Deployment.as_str(), "Deployment");
        assert_eq!(ResourceKind::Service.as_str(), "Service");
    }

    #[test]
    fn test_resource_kind_api_version() {
        assert_eq!(ResourceKind::Pod.api_version(), ApiVersion::V1);
        assert_eq!(ResourceKind::Deployment.api_version(), ApiVersion::AppsV1);
        assert_eq!(ResourceKind::CronJob.api_version(), ApiVersion::BatchV1);
    }

    // Resource Status Tests

    #[test]
    fn test_resource_status_is_healthy() {
        assert!(ResourceStatus::Running.is_healthy());
        assert!(ResourceStatus::Succeeded.is_healthy());
        assert!(!ResourceStatus::Failed.is_healthy());
        assert!(!ResourceStatus::Pending.is_healthy());
    }

    // Container Spec Tests

    #[test]
    fn test_container_spec_creation() {
        let container = ContainerSpec::new("app", "nginx:1.21")
            .with_port(ContainerPort::new(80))
            .with_env("ENV", "production")
            .always_pull();

        assert_eq!(container.name, "app");
        assert_eq!(container.image, "nginx:1.21");
        assert_eq!(container.ports.len(), 1);
        assert_eq!(container.env.get("ENV"), Some(&"production".to_string()));
        assert_eq!(container.image_pull_policy, "Always");
    }

    #[test]
    fn test_container_port() {
        let port = ContainerPort::new(8080).with_name("http").udp();
        assert_eq!(port.container_port, 8080);
        assert_eq!(port.name, Some("http".to_string()));
        assert_eq!(port.protocol, "UDP");
    }

    // Resource Metadata Tests

    #[test]
    fn test_resource_metadata() {
        let metadata = ResourceMetadata::new("my-app")
            .in_namespace("default")
            .with_label("app", "my-app")
            .with_annotation("description", "My application");

        assert_eq!(metadata.name, "my-app");
        assert_eq!(metadata.namespace, Some("default".to_string()));
        assert_eq!(metadata.labels.get("app"), Some(&"my-app".to_string()));
    }

    // K8s Resource Tests

    #[test]
    fn test_k8s_resource_creation() {
        let metadata = ResourceMetadata::new("test-pod").in_namespace("default");
        let resource = K8sResource::new(ResourceKind::Pod, metadata);

        assert_eq!(resource.kind, ResourceKind::Pod);
        assert_eq!(resource.api_version, ApiVersion::V1);
        assert_eq!(resource.status, ResourceStatus::Pending);
    }

    #[test]
    fn test_k8s_resource_full_name() {
        let metadata = ResourceMetadata::new("my-pod").in_namespace("kube-system");
        let resource = K8sResource::new(ResourceKind::Pod, metadata);

        assert_eq!(resource.full_name(), "kube-system/my-pod");
    }

    // Deployment Manager Tests

    #[test]
    fn test_deployment_manager_create() {
        let mut manager = DeploymentManager::new();

        let spec = DeploymentSpec::new(3)
            .with_selector("app", "web")
            .with_container(ContainerSpec::new("web", "nginx:1.21"));

        let id = manager.create("web-deploy", "default", spec);

        assert_eq!(manager.count(), 1);
        assert!(manager.get(&id).is_some());
    }

    #[test]
    fn test_deployment_manager_scale() {
        let mut manager = DeploymentManager::new();

        let spec = DeploymentSpec::new(1);
        let id = manager.create("test", "default", spec);

        assert!(manager.scale(&id, 5));
        assert_eq!(manager.get_spec(&id).unwrap().replicas, 5);
    }

    #[test]
    fn test_deployment_manager_update_image() {
        let mut manager = DeploymentManager::new();

        let spec = DeploymentSpec::new(1).with_container(ContainerSpec::new("app", "nginx:1.20"));
        let id = manager.create("test", "default", spec);

        assert!(manager.update_image(&id, "app", "nginx:1.21"));
        assert_eq!(
            manager.get_spec(&id).unwrap().containers[0].image,
            "nginx:1.21"
        );
    }

    // Service Spec Tests

    #[test]
    fn test_service_spec_creation() {
        let spec = ServiceSpec::new(ServiceType::LoadBalancer)
            .with_selector("app", "web")
            .with_port(ServicePort::new(80, 8080));

        assert_eq!(spec.service_type, ServiceType::LoadBalancer);
        assert_eq!(spec.selector.get("app"), Some(&"web".to_string()));
        assert_eq!(spec.ports.len(), 1);
    }

    // Manifest Generator Tests

    #[test]
    fn test_manifest_generator_deployment() {
        let generator = ManifestGenerator::new();

        let spec = DeploymentSpec::new(2)
            .with_selector("app", "test")
            .with_container(ContainerSpec::new("app", "nginx:1.21"));

        let manifest = generator.deployment("test", "default", &spec);

        assert!(manifest.content.contains("kind: Deployment"));
        assert!(manifest.content.contains("replicas: 2"));
        assert!(manifest.content.contains("nginx:1.21"));
    }

    #[test]
    fn test_manifest_generator_service() {
        let generator = ManifestGenerator::new();

        let spec = ServiceSpec::new(ServiceType::ClusterIP)
            .with_selector("app", "test")
            .with_port(ServicePort::new(80, 8080));

        let manifest = generator.service("test-svc", "default", &spec);

        assert!(manifest.content.contains("kind: Service"));
        assert!(manifest.content.contains("type: ClusterIP"));
    }

    #[test]
    fn test_manifest_generator_config_map() {
        let generator = ManifestGenerator::new();

        let mut data = HashMap::new();
        data.insert("config.yaml".to_string(), "key: value".to_string());

        let manifest = generator.config_map("test-config", "default", &data);

        assert!(manifest.content.contains("kind: ConfigMap"));
        assert!(manifest.content.contains("config.yaml"));
    }

    #[test]
    fn test_manifest_generator_hpa() {
        let generator = ManifestGenerator::new();

        let manifest = generator.hpa("test-hpa", "default", "test-deploy", 1, 10, 50);

        assert!(manifest.content.contains("HorizontalPodAutoscaler"));
        assert!(manifest.content.contains("minReplicas: 1"));
        assert!(manifest.content.contains("maxReplicas: 10"));
    }

    // Troubleshooting Tests

    #[test]
    fn test_k8s_issue_creation() {
        let issue = K8sIssue::new(
            "res_1",
            "my-deploy",
            ResourceKind::Deployment,
            IssueSeverity::Warning,
            "Test Issue",
            "This is a test issue",
        )
        .with_fix("Apply the fix");

        assert_eq!(issue.severity, IssueSeverity::Warning);
        assert!(issue.suggested_fix.is_some());
    }

    #[test]
    fn test_troubleshooting_analyzer() {
        let mut analyzer = TroubleshootingAnalyzer::new();

        let metadata = ResourceMetadata::new("test").in_namespace("default");
        let resource = K8sResource::new(ResourceKind::Deployment, metadata);

        let spec = DeploymentSpec::new(0).with_container(ContainerSpec::new("app", "nginx:latest"));

        analyzer.analyze_deployment(&resource, &spec);

        assert!(!analyzer.issues().is_empty());
    }

    #[test]
    fn test_issue_severity_ordering() {
        assert!(IssueSeverity::Critical > IssueSeverity::Error);
        assert!(IssueSeverity::Error > IssueSeverity::Warning);
        assert!(IssueSeverity::Warning > IssueSeverity::Info);
    }

    // Log Analysis Tests

    #[test]
    fn test_log_level_from_str() {
        assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("warn"), LogLevel::Warning);
        assert_eq!(LogLevel::from_str("DEBUG"), LogLevel::Debug);
    }

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(LogLevel::Error, "Connection failed")
            .from_pod("my-pod")
            .from_container("app");

        assert_eq!(entry.level, LogLevel::Error);
        assert_eq!(entry.pod, Some("my-pod".to_string()));
        assert_eq!(entry.container, Some("app".to_string()));
    }

    #[test]
    fn test_log_analyzer_parse() {
        let mut analyzer = LogAnalyzer::new();

        analyzer.parse_logs(
            "INFO: Starting server\nERROR: Connection failed\nWARN: High memory usage",
            Some("test-pod"),
            None,
        );

        assert_eq!(analyzer.count(), 3);
        assert_eq!(analyzer.error_count(), 1);
    }

    #[test]
    fn test_log_analyzer_search() {
        let mut analyzer = LogAnalyzer::new();

        analyzer.add(LogEntry::new(
            LogLevel::Error,
            "Connection failed to database",
        ));
        analyzer.add(LogEntry::new(LogLevel::Info, "Server started"));

        let results = analyzer.search("connection");
        assert_eq!(results.len(), 1);
    }

    // Unique ID Tests

    #[test]
    fn test_unique_resource_ids() {
        let id1 = generate_resource_id();
        let id2 = generate_resource_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_unique_manifest_ids() {
        let id1 = generate_manifest_id();
        let id2 = generate_manifest_id();
        assert_ne!(id1, id2);
    }

    // Type String Tests

    #[test]
    fn test_deployment_strategy_as_str() {
        assert_eq!(DeploymentStrategy::RollingUpdate.as_str(), "RollingUpdate");
        assert_eq!(DeploymentStrategy::Recreate.as_str(), "Recreate");
    }

    #[test]
    fn test_service_type_as_str() {
        assert_eq!(ServiceType::ClusterIP.as_str(), "ClusterIP");
        assert_eq!(ServiceType::LoadBalancer.as_str(), "LoadBalancer");
    }

    #[test]
    fn test_manifest_format_extension() {
        assert_eq!(ManifestFormat::Yaml.file_extension(), ".yaml");
        assert_eq!(ManifestFormat::Json.file_extension(), ".json");
    }

    #[test]
    fn test_issue_severity_as_str() {
        assert_eq!(IssueSeverity::Critical.as_str(), "critical");
        assert_eq!(IssueSeverity::Warning.as_str(), "warning");
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
        assert_eq!(LogLevel::Warning.as_str(), "WARNING");
    }

    #[test]
    fn test_resource_requirements() {
        let req = ResourceRequirements::new()
            .with_cpu("100m", "500m")
            .with_memory("128Mi", "512Mi");

        assert_eq!(req.cpu_request, Some("100m".to_string()));
        assert_eq!(req.memory_limit, Some("512Mi".to_string()));
    }

    #[test]
    fn test_deployment_manager_list() {
        let mut manager = DeploymentManager::new();

        manager.create("deploy1", "ns1", DeploymentSpec::new(1));
        manager.create("deploy2", "ns1", DeploymentSpec::new(2));
        manager.create("deploy3", "ns2", DeploymentSpec::new(3));

        assert_eq!(manager.list().len(), 3);
        assert_eq!(manager.list_in_namespace("ns1").len(), 2);
    }
}
