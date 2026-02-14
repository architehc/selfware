//! Contract & Integration Testing Tools
//!
//! Provides consumer-driven contracts, service virtualization,
//! test container orchestration, and API compatibility checking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static CONTRACT_COUNTER: AtomicU64 = AtomicU64::new(1);
static STUB_COUNTER: AtomicU64 = AtomicU64::new(1);
static CONTAINER_COUNTER: AtomicU64 = AtomicU64::new(1);

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================================
// Consumer-Driven Contracts (Pact-style)
// ============================================================================

/// HTTP method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }
}

/// Matcher type for contract matching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Matcher {
    Exact(String),
    Regex(String),
    Type,
    Include(String),
    Integer,
    Decimal,
    Boolean,
    Null,
    ArrayContaining(Box<Matcher>),
    EachLike(Box<Matcher>),
}

impl Matcher {
    pub fn matches(&self, value: &str) -> bool {
        match self {
            Matcher::Exact(expected) => value == expected,
            Matcher::Regex(pattern) => regex::Regex::new(pattern)
                .map(|re| re.is_match(value))
                .unwrap_or(false),
            Matcher::Type => true,
            Matcher::Include(substring) => value.contains(substring),
            Matcher::Integer => value.parse::<i64>().is_ok(),
            Matcher::Decimal => value.parse::<f64>().is_ok(),
            Matcher::Boolean => value == "true" || value == "false",
            Matcher::Null => value == "null" || value.is_empty(),
            _ => true,
        }
    }
}

/// Request definition in a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractRequest {
    /// HTTP method
    pub method: HttpMethod,
    /// Path
    pub path: String,
    /// Query parameters
    pub query: HashMap<String, String>,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body (JSON string)
    pub body: Option<String>,
    /// Body matchers
    pub body_matchers: HashMap<String, Matcher>,
}

impl ContractRequest {
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            body_matchers: HashMap::new(),
        }
    }

    pub fn with_query(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.insert(key.into(), value.into());
        self
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_body_matcher(mut self, path: impl Into<String>, matcher: Matcher) -> Self {
        self.body_matchers.insert(path.into(), matcher);
        self
    }
}

/// Response definition in a contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResponse {
    /// Status code
    pub status: u16,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body (JSON string)
    pub body: Option<String>,
    /// Body matchers
    pub body_matchers: HashMap<String, Matcher>,
}

impl ContractResponse {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: None,
            body_matchers: HashMap::new(),
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_body_matcher(mut self, path: impl Into<String>, matcher: Matcher) -> Self {
        self.body_matchers.insert(path.into(), matcher);
        self
    }
}

/// Contract interaction (request-response pair)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interaction {
    /// Description
    pub description: String,
    /// Provider state
    pub provider_state: Option<String>,
    /// Request
    pub request: ContractRequest,
    /// Response
    pub response: ContractResponse,
}

impl Interaction {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            provider_state: None,
            request: ContractRequest::new(HttpMethod::Get, "/"),
            response: ContractResponse::new(200),
        }
    }

    pub fn given(mut self, state: impl Into<String>) -> Self {
        self.provider_state = Some(state.into());
        self
    }

    pub fn upon_receiving(mut self, request: ContractRequest) -> Self {
        self.request = request;
        self
    }

    pub fn will_respond_with(mut self, response: ContractResponse) -> Self {
        self.response = response;
        self
    }
}

/// Pact contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    /// Contract ID
    pub contract_id: String,
    /// Consumer name
    pub consumer: String,
    /// Provider name
    pub provider: String,
    /// Interactions
    pub interactions: Vec<Interaction>,
    /// Metadata
    pub metadata: HashMap<String, String>,
    /// Created timestamp
    pub created_at: u64,
}

impl Contract {
    pub fn new(consumer: impl Into<String>, provider: impl Into<String>) -> Self {
        let contract_id = format!(
            "contract_{}",
            CONTRACT_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        Self {
            contract_id,
            consumer: consumer.into(),
            provider: provider.into(),
            interactions: Vec::new(),
            metadata: HashMap::new(),
            created_at: current_timestamp(),
        }
    }

    pub fn add_interaction(&mut self, interaction: Interaction) {
        self.interactions.push(interaction);
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Contract verification result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Contract ID
    pub contract_id: String,
    /// Overall success
    pub success: bool,
    /// Individual interaction results
    pub interaction_results: Vec<InteractionResult>,
    /// Verification timestamp
    pub verified_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionResult {
    /// Interaction description
    pub description: String,
    /// Success
    pub success: bool,
    /// Mismatches
    pub mismatches: Vec<String>,
}

/// Contract verifier
#[derive(Debug, Clone)]
pub struct ContractVerifier {
    /// Provider base URL
    pub provider_url: String,
    /// Provider states setup
    pub state_handlers: HashMap<String, String>,
}

impl ContractVerifier {
    pub fn new(provider_url: impl Into<String>) -> Self {
        Self {
            provider_url: provider_url.into(),
            state_handlers: HashMap::new(),
        }
    }

    pub fn register_state_handler(
        &mut self,
        state: impl Into<String>,
        setup_command: impl Into<String>,
    ) {
        self.state_handlers
            .insert(state.into(), setup_command.into());
    }

    /// Verify a contract against the provider (async version with real HTTP)
    pub async fn verify_async(&self, contract: &Contract) -> VerificationResult {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let mut results = Vec::new();

        for interaction in &contract.interactions {
            let result = self.verify_interaction_async(&client, interaction).await;
            results.push(result);
        }

        let success = results.iter().all(|r| r.success);

        VerificationResult {
            contract_id: contract.contract_id.clone(),
            success,
            interaction_results: results,
            verified_at: current_timestamp(),
        }
    }

    /// Validate contract structure without making HTTP requests
    ///
    /// This performs offline validation only - it checks that:
    /// - Request paths are not empty
    /// - Response status codes are set
    ///
    /// For actual provider verification with HTTP requests, use `verify_async()`.
    pub fn validate_structure(&self, contract: &Contract) -> VerificationResult {
        let mut results = Vec::new();

        for interaction in &contract.interactions {
            let result = self.verify_interaction_sync(interaction);
            results.push(result);
        }

        let success = results.iter().all(|r| r.success);

        VerificationResult {
            contract_id: contract.contract_id.clone(),
            success,
            interaction_results: results,
            verified_at: current_timestamp(),
        }
    }

    /// Deprecated: Use `validate_structure()` for offline validation
    /// or `verify_async()` for real provider verification.
    #[deprecated(since = "0.2.0", note = "Use validate_structure() or verify_async()")]
    pub fn verify(&self, contract: &Contract) -> VerificationResult {
        self.validate_structure(contract)
    }

    /// Synchronous verification - validates contract structure only
    fn verify_interaction_sync(&self, interaction: &Interaction) -> InteractionResult {
        let mut mismatches = Vec::new();

        // Validate request structure
        if interaction.request.path.is_empty() {
            mismatches.push("Request path is empty".to_string());
        }

        // Validate response structure
        if interaction.response.status == 0 {
            mismatches.push("Response status is not set".to_string());
        }

        InteractionResult {
            description: interaction.description.clone(),
            success: mismatches.is_empty(),
            mismatches,
        }
    }

    /// Async verification - makes real HTTP requests to the provider
    async fn verify_interaction_async(
        &self,
        client: &reqwest::Client,
        interaction: &Interaction,
    ) -> InteractionResult {
        let mut mismatches = Vec::new();

        // Build URL
        let url = format!(
            "{}{}{}",
            self.provider_url.trim_end_matches('/'),
            if interaction.request.path.starts_with('/') {
                ""
            } else {
                "/"
            },
            interaction.request.path
        );

        // Build request
        let method = match interaction.request.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        // Build URL with query parameters
        let full_url = if interaction.request.query.is_empty() {
            url
        } else {
            let query_string: String = interaction
                .request
                .query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("&");
            format!("{}?{}", url, query_string)
        };

        let mut request_builder = client.request(method, &full_url);

        // Add headers
        for (key, value) in &interaction.request.headers {
            request_builder = request_builder.header(key, value);
        }

        // Add body
        if let Some(ref body) = interaction.request.body {
            request_builder = request_builder.body(body.clone());
        }

        // Execute request
        match request_builder.send().await {
            Ok(response) => {
                // Check status code
                let actual_status = response.status().as_u16();
                if actual_status != interaction.response.status {
                    mismatches.push(format!(
                        "Status mismatch: expected {}, got {}",
                        interaction.response.status, actual_status
                    ));
                }

                // Check headers
                for (key, expected_value) in &interaction.response.headers {
                    match response.headers().get(key) {
                        Some(actual) => {
                            if let Ok(actual_str) = actual.to_str() {
                                if actual_str != expected_value {
                                    mismatches.push(format!(
                                        "Header '{}' mismatch: expected '{}', got '{}'",
                                        key, expected_value, actual_str
                                    ));
                                }
                            }
                        }
                        None => {
                            mismatches.push(format!("Missing expected header: {}", key));
                        }
                    }
                }

                // Check body if expected
                if let Some(ref expected_body) = interaction.response.body {
                    match response.text().await {
                        Ok(actual_body) => {
                            // Apply body matchers
                            if interaction.response.body_matchers.is_empty() {
                                // Exact match if no matchers
                                if actual_body.trim() != expected_body.trim() {
                                    mismatches.push(format!(
                                        "Body mismatch: expected '{}...', got '{}...'",
                                        &expected_body[..expected_body.len().min(100)],
                                        &actual_body[..actual_body.len().min(100)]
                                    ));
                                }
                            } else {
                                // Use matchers
                                for (path, matcher) in &interaction.response.body_matchers {
                                    if !matcher.matches(&actual_body) {
                                        mismatches.push(format!(
                                            "Body matcher failed for path '{}': {:?}",
                                            path, matcher
                                        ));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            mismatches.push(format!("Failed to read response body: {}", e));
                        }
                    }
                }
            }
            Err(e) => {
                mismatches.push(format!("Request failed: {}", e));
            }
        }

        InteractionResult {
            description: interaction.description.clone(),
            success: mismatches.is_empty(),
            mismatches,
        }
    }
}

// ============================================================================
// Service Virtualization
// ============================================================================

/// Stub request matcher
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubRequest {
    /// Method
    pub method: Option<HttpMethod>,
    /// URL pattern
    pub url_pattern: String,
    /// Header matchers
    pub headers: HashMap<String, Matcher>,
    /// Body matchers
    pub body_matchers: HashMap<String, Matcher>,
    /// Priority (higher = matches first)
    pub priority: u32,
}

impl StubRequest {
    pub fn new(url_pattern: impl Into<String>) -> Self {
        Self {
            method: None,
            url_pattern: url_pattern.into(),
            headers: HashMap::new(),
            body_matchers: HashMap::new(),
            priority: 0,
        }
    }

    pub fn with_method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    pub fn with_header_matcher(mut self, header: impl Into<String>, matcher: Matcher) -> Self {
        self.headers.insert(header.into(), matcher);
        self
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn matches(
        &self,
        method: HttpMethod,
        url: &str,
        _headers: &HashMap<String, String>,
    ) -> bool {
        if let Some(expected_method) = &self.method {
            if *expected_method != method {
                return false;
            }
        }

        // Simple pattern matching
        if self.url_pattern.contains('*') {
            let parts: Vec<&str> = self.url_pattern.split('*').collect();
            if parts.len() == 2 {
                return url.starts_with(parts[0]) && url.ends_with(parts[1]);
            }
        }

        url == self.url_pattern || url.contains(&self.url_pattern)
    }
}

/// Stub response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubResponse {
    /// Status code
    pub status: u16,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body
    pub body: Option<String>,
    /// Delay before response (milliseconds)
    pub delay_ms: u64,
    /// Fault simulation
    pub fault: Option<FaultType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FaultType {
    ConnectionReset,
    EmptyResponse,
    MalformedResponse,
    RandomDataThenClose,
    Timeout,
}

impl StubResponse {
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: None,
            delay_ms: 0,
            fault: None,
        }
    }

    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn with_fault(mut self, fault: FaultType) -> Self {
        self.fault = Some(fault);
        self
    }
}

/// Stub mapping (request -> response)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StubMapping {
    /// Mapping ID
    pub id: String,
    /// Name
    pub name: String,
    /// Request matcher
    pub request: StubRequest,
    /// Response
    pub response: StubResponse,
    /// Enabled
    pub enabled: bool,
    /// Hit count
    pub hit_count: u64,
}

impl StubMapping {
    pub fn new(name: impl Into<String>, request: StubRequest, response: StubResponse) -> Self {
        let id = format!("stub_{}", STUB_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            id,
            name: name.into(),
            request,
            response,
            enabled: true,
            hit_count: 0,
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn record_hit(&mut self) {
        self.hit_count += 1;
    }
}

/// Mock server (WireMock-style)
#[derive(Debug, Clone)]
pub struct MockServer {
    /// Server name
    pub name: String,
    /// Port
    pub port: u16,
    /// Stub mappings
    pub mappings: Vec<StubMapping>,
    /// Request log
    pub request_log: Vec<RequestLogEntry>,
    /// Running state
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLogEntry {
    /// Timestamp
    pub timestamp: u64,
    /// Method
    pub method: HttpMethod,
    /// URL
    pub url: String,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body
    pub body: Option<String>,
    /// Matched stub
    pub matched_stub: Option<String>,
}

impl MockServer {
    pub fn new(name: impl Into<String>, port: u16) -> Self {
        Self {
            name: name.into(),
            port,
            mappings: Vec::new(),
            request_log: Vec::new(),
            running: false,
        }
    }

    pub fn stub(&mut self, mapping: StubMapping) {
        self.mappings.push(mapping);
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn reset(&mut self) {
        self.mappings.clear();
        self.request_log.clear();
    }

    pub fn find_mapping(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &HashMap<String, String>,
    ) -> Option<&StubMapping> {
        self.mappings
            .iter()
            .filter(|m| m.enabled && m.request.matches(method, url, headers))
            .max_by_key(|m| m.request.priority)
    }

    pub fn handle_request(
        &mut self,
        method: HttpMethod,
        url: &str,
        headers: HashMap<String, String>,
        body: Option<String>,
    ) -> Option<StubResponse> {
        let matched_stub = self
            .find_mapping(method, url, &headers)
            .map(|m| m.id.clone());

        self.request_log.push(RequestLogEntry {
            timestamp: current_timestamp(),
            method,
            url: url.to_string(),
            headers: headers.clone(),
            body,
            matched_stub: matched_stub.clone(),
        });

        if let Some(stub_id) = matched_stub {
            if let Some(mapping) = self.mappings.iter_mut().find(|m| m.id == stub_id) {
                mapping.record_hit();
                return Some(mapping.response.clone());
            }
        }

        None
    }

    pub fn verify_request_count(&self, url: &str, expected: usize) -> bool {
        let count = self
            .request_log
            .iter()
            .filter(|r| r.url.contains(url))
            .count();
        count == expected
    }

    pub fn unmatched_requests(&self) -> Vec<&RequestLogEntry> {
        self.request_log
            .iter()
            .filter(|r| r.matched_stub.is_none())
            .collect()
    }
}

// ============================================================================
// Test Container Orchestration
// ============================================================================

/// Container type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerType {
    Database,
    MessageQueue,
    Cache,
    MockService,
    Custom,
}

/// Container state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerState {
    Created,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// Test container configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestContainer {
    /// Container ID
    pub container_id: String,
    /// Name
    pub name: String,
    /// Image
    pub image: String,
    /// Container type
    pub container_type: ContainerType,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Port mappings (host:container)
    pub ports: HashMap<u16, u16>,
    /// Volume mounts
    pub volumes: Vec<(String, String)>,
    /// State
    pub state: ContainerState,
    /// Health check command
    pub health_check: Option<String>,
    /// Startup timeout
    pub startup_timeout: Duration,
}

impl TestContainer {
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        let container_id = format!("tc_{}", CONTAINER_COUNTER.fetch_add(1, Ordering::SeqCst));
        Self {
            container_id,
            name: name.into(),
            image: image.into(),
            container_type: ContainerType::Custom,
            env: HashMap::new(),
            ports: HashMap::new(),
            volumes: Vec::new(),
            state: ContainerState::Created,
            health_check: None,
            startup_timeout: Duration::from_secs(60),
        }
    }

    pub fn postgres() -> Self {
        Self::new("postgres", "postgres:15")
            .with_type(ContainerType::Database)
            .with_env("POSTGRES_PASSWORD", "test")
            .with_port(5432, 5432)
            .with_health_check("pg_isready -U postgres")
    }

    pub fn redis() -> Self {
        Self::new("redis", "redis:7")
            .with_type(ContainerType::Cache)
            .with_port(6379, 6379)
            .with_health_check("redis-cli ping")
    }

    pub fn rabbitmq() -> Self {
        Self::new("rabbitmq", "rabbitmq:3-management")
            .with_type(ContainerType::MessageQueue)
            .with_port(5672, 5672)
            .with_port(15672, 15672)
    }

    pub fn with_type(mut self, container_type: ContainerType) -> Self {
        self.container_type = container_type;
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn with_port(mut self, host_port: u16, container_port: u16) -> Self {
        self.ports.insert(host_port, container_port);
        self
    }

    pub fn with_volume(
        mut self,
        host_path: impl Into<String>,
        container_path: impl Into<String>,
    ) -> Self {
        self.volumes.push((host_path.into(), container_path.into()));
        self
    }

    pub fn with_health_check(mut self, command: impl Into<String>) -> Self {
        self.health_check = Some(command.into());
        self
    }

    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    pub fn to_docker_run(&self) -> String {
        let mut cmd = format!("docker run -d --name {}", self.name);

        for (key, value) in &self.env {
            cmd.push_str(&format!(" -e {}={}", key, value));
        }

        for (host, container) in &self.ports {
            cmd.push_str(&format!(" -p {}:{}", host, container));
        }

        for (host, container) in &self.volumes {
            cmd.push_str(&format!(" -v {}:{}", host, container));
        }

        cmd.push_str(&format!(" {}", self.image));
        cmd
    }
}

/// Container orchestrator
#[derive(Debug, Clone)]
pub struct ContainerOrchestrator {
    /// Project name (prefix for container names)
    pub project: String,
    /// Containers
    pub containers: HashMap<String, TestContainer>,
    /// Dependency order
    dependencies: HashMap<String, Vec<String>>,
}

impl ContainerOrchestrator {
    pub fn new(project: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            containers: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    pub fn add_container(&mut self, container: TestContainer) {
        self.containers.insert(container.name.clone(), container);
    }

    pub fn set_dependency(&mut self, container: impl Into<String>, depends_on: impl Into<String>) {
        self.dependencies
            .entry(container.into())
            .or_default()
            .push(depends_on.into());
    }

    pub fn start_order(&self) -> Vec<&str> {
        // Simple topological sort for startup order
        let mut order = Vec::new();
        let mut started: std::collections::HashSet<&str> = std::collections::HashSet::new();

        while started.len() < self.containers.len() {
            for name in self.containers.keys() {
                if started.contains(name.as_str()) {
                    continue;
                }

                let deps = self.dependencies.get(name).cloned().unwrap_or_default();
                if deps.iter().all(|d| started.contains(d.as_str())) {
                    order.push(name.as_str());
                    started.insert(name);
                }
            }

            // Break if no progress (cycle detected)
            if order.len() == started.len() && started.len() < self.containers.len() {
                break;
            }
        }

        order
    }

    pub fn generate_compose(&self) -> String {
        let mut yaml = String::new();
        yaml.push_str("version: '3.8'\n\n");
        yaml.push_str("services:\n");

        for (name, container) in &self.containers {
            yaml.push_str(&format!("  {}:\n", name));
            yaml.push_str(&format!("    image: {}\n", container.image));

            if !container.env.is_empty() {
                yaml.push_str("    environment:\n");
                for (key, value) in &container.env {
                    yaml.push_str(&format!("      - {}={}\n", key, value));
                }
            }

            if !container.ports.is_empty() {
                yaml.push_str("    ports:\n");
                for (host, container_port) in &container.ports {
                    yaml.push_str(&format!("      - \"{}:{}\"\n", host, container_port));
                }
            }

            if let Some(deps) = self.dependencies.get(name) {
                if !deps.is_empty() {
                    yaml.push_str("    depends_on:\n");
                    for dep in deps {
                        yaml.push_str(&format!("      - {}\n", dep));
                    }
                }
            }

            yaml.push('\n');
        }

        yaml
    }

    pub fn start_all(&mut self) {
        let order: Vec<String> = self
            .start_order()
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        for name in order {
            if let Some(container) = self.containers.get_mut(&name) {
                container.state = ContainerState::Running;
            }
        }
    }

    pub fn stop_all(&mut self) {
        for container in self.containers.values_mut() {
            container.state = ContainerState::Stopped;
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.containers
            .values()
            .all(|c| c.state == ContainerState::Running)
    }
}

// ============================================================================
// API Compatibility Checking
// ============================================================================

/// API version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiVersion {
    /// Version string
    pub version: String,
    /// Endpoints
    pub endpoints: Vec<ApiEndpoint>,
    /// Created timestamp
    pub created_at: u64,
}

impl ApiVersion {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            endpoints: Vec::new(),
            created_at: current_timestamp(),
        }
    }

    pub fn add_endpoint(&mut self, endpoint: ApiEndpoint) {
        self.endpoints.push(endpoint);
    }
}

/// API endpoint definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEndpoint {
    /// Method
    pub method: HttpMethod,
    /// Path
    pub path: String,
    /// Query parameters
    pub query_params: Vec<ApiParameter>,
    /// Request body schema
    pub request_body: Option<ApiSchema>,
    /// Response schema
    pub response: ApiSchema,
    /// Deprecated
    pub deprecated: bool,
}

impl ApiEndpoint {
    pub fn new(method: HttpMethod, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query_params: Vec::new(),
            request_body: None,
            response: ApiSchema::empty(),
            deprecated: false,
        }
    }

    pub fn with_query_param(mut self, param: ApiParameter) -> Self {
        self.query_params.push(param);
        self
    }

    pub fn with_request_body(mut self, schema: ApiSchema) -> Self {
        self.request_body = Some(schema);
        self
    }

    pub fn with_response(mut self, schema: ApiSchema) -> Self {
        self.response = schema;
        self
    }

    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }
}

/// API parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiParameter {
    /// Name
    pub name: String,
    /// Type
    pub param_type: String,
    /// Required
    pub required: bool,
    /// Description
    pub description: Option<String>,
}

impl ApiParameter {
    pub fn new(name: impl Into<String>, param_type: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: param_type.into(),
            required: false,
            description: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
}

/// API schema (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSchema {
    /// Schema type
    pub schema_type: String,
    /// Properties (for object types)
    pub properties: HashMap<String, ApiSchemaProperty>,
    /// Required properties
    pub required: Vec<String>,
}

impl ApiSchema {
    pub fn empty() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: HashMap::new(),
            required: Vec::new(),
        }
    }

    pub fn object() -> Self {
        Self::empty()
    }

    pub fn with_property(mut self, name: impl Into<String>, property: ApiSchemaProperty) -> Self {
        let name = name.into();
        if property.required {
            self.required.push(name.clone());
        }
        self.properties.insert(name, property);
        self
    }
}

/// API schema property
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSchemaProperty {
    /// Type
    pub prop_type: String,
    /// Format (e.g., date-time, email)
    pub format: Option<String>,
    /// Required
    pub required: bool,
    /// Nullable
    pub nullable: bool,
}

impl ApiSchemaProperty {
    pub fn string() -> Self {
        Self {
            prop_type: "string".to_string(),
            format: None,
            required: false,
            nullable: false,
        }
    }

    pub fn integer() -> Self {
        Self {
            prop_type: "integer".to_string(),
            format: None,
            required: false,
            nullable: false,
        }
    }

    pub fn boolean() -> Self {
        Self {
            prop_type: "boolean".to_string(),
            format: None,
            required: false,
            nullable: false,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }
}

/// Compatibility change type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityChangeType {
    EndpointAdded,
    EndpointRemoved,
    ParameterAdded,
    RequiredParameterAdded,
    ParameterRemoved,
    TypeChanged,
    ResponseChanged,
    Deprecated,
}

impl CompatibilityChangeType {
    pub fn is_breaking(&self) -> bool {
        matches!(
            self,
            CompatibilityChangeType::EndpointRemoved
                | CompatibilityChangeType::RequiredParameterAdded
                | CompatibilityChangeType::ParameterRemoved
                | CompatibilityChangeType::TypeChanged
        )
    }
}

/// Compatibility change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityChange {
    /// Change type
    pub change_type: CompatibilityChangeType,
    /// Affected path
    pub path: String,
    /// Description
    pub description: String,
}

impl CompatibilityChange {
    pub fn new(
        change_type: CompatibilityChangeType,
        path: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            change_type,
            path: path.into(),
            description: description.into(),
        }
    }
}

/// API compatibility checker
#[derive(Debug, Clone)]
pub struct CompatibilityChecker;

impl CompatibilityChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check(
        &self,
        old_version: &ApiVersion,
        new_version: &ApiVersion,
    ) -> Vec<CompatibilityChange> {
        let mut changes = Vec::new();

        // Check for removed endpoints
        for old_endpoint in &old_version.endpoints {
            let exists = new_version
                .endpoints
                .iter()
                .any(|e| e.method == old_endpoint.method && e.path == old_endpoint.path);

            if !exists {
                changes.push(CompatibilityChange::new(
                    CompatibilityChangeType::EndpointRemoved,
                    &old_endpoint.path,
                    format!(
                        "{} {} was removed",
                        old_endpoint.method.as_str(),
                        old_endpoint.path
                    ),
                ));
            }
        }

        // Check for added endpoints
        for new_endpoint in &new_version.endpoints {
            let existed = old_version
                .endpoints
                .iter()
                .any(|e| e.method == new_endpoint.method && e.path == new_endpoint.path);

            if !existed {
                changes.push(CompatibilityChange::new(
                    CompatibilityChangeType::EndpointAdded,
                    &new_endpoint.path,
                    format!(
                        "{} {} was added",
                        new_endpoint.method.as_str(),
                        new_endpoint.path
                    ),
                ));
            }
        }

        // Check for deprecated endpoints
        for new_endpoint in &new_version.endpoints {
            if new_endpoint.deprecated {
                let was_deprecated = old_version.endpoints.iter().any(|e| {
                    e.method == new_endpoint.method && e.path == new_endpoint.path && e.deprecated
                });

                if !was_deprecated {
                    changes.push(CompatibilityChange::new(
                        CompatibilityChangeType::Deprecated,
                        &new_endpoint.path,
                        format!(
                            "{} {} was deprecated",
                            new_endpoint.method.as_str(),
                            new_endpoint.path
                        ),
                    ));
                }
            }
        }

        changes
    }

    pub fn breaking_changes(
        &self,
        old_version: &ApiVersion,
        new_version: &ApiVersion,
    ) -> Vec<CompatibilityChange> {
        self.check(old_version, new_version)
            .into_iter()
            .filter(|c| c.change_type.is_breaking())
            .collect()
    }

    pub fn is_compatible(&self, old_version: &ApiVersion, new_version: &ApiVersion) -> bool {
        self.breaking_changes(old_version, new_version).is_empty()
    }
}

impl Default for CompatibilityChecker {
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

    // Contract tests
    #[test]
    fn test_http_method() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
    }

    #[test]
    fn test_matcher_exact() {
        let matcher = Matcher::Exact("hello".to_string());
        assert!(matcher.matches("hello"));
        assert!(!matcher.matches("world"));
    }

    #[test]
    fn test_matcher_include() {
        let matcher = Matcher::Include("foo".to_string());
        assert!(matcher.matches("foobar"));
        assert!(matcher.matches("barfoo"));
        assert!(!matcher.matches("bar"));
    }

    #[test]
    fn test_matcher_integer() {
        let matcher = Matcher::Integer;
        assert!(matcher.matches("123"));
        assert!(matcher.matches("-456"));
        assert!(!matcher.matches("12.34"));
        assert!(!matcher.matches("abc"));
    }

    #[test]
    fn test_contract_request() {
        let request = ContractRequest::new(HttpMethod::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(r#"{"name": "test"}"#);

        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.path, "/api/users");
        assert!(request.headers.contains_key("Content-Type"));
    }

    #[test]
    fn test_contract_response() {
        let response = ContractResponse::new(201)
            .with_header("Location", "/api/users/1")
            .with_body(r#"{"id": 1}"#);

        assert_eq!(response.status, 201);
        assert!(response.body.is_some());
    }

    #[test]
    fn test_interaction() {
        let interaction = Interaction::new("Create a user")
            .given("no users exist")
            .upon_receiving(ContractRequest::new(HttpMethod::Post, "/users"))
            .will_respond_with(ContractResponse::new(201));

        assert_eq!(interaction.description, "Create a user");
        assert!(interaction.provider_state.is_some());
    }

    #[test]
    fn test_contract() {
        let mut contract = Contract::new("consumer", "provider");
        contract.add_interaction(Interaction::new("Test"));

        assert_eq!(contract.interactions.len(), 1);
        assert!(contract.contract_id.starts_with("contract_"));
    }

    #[test]
    fn test_contract_verifier_valid_structure() {
        // Interaction::new() creates valid structure (path="/", status=200)
        let mut contract = Contract::new("consumer", "provider");
        contract.add_interaction(Interaction::new("Test"));

        let verifier = ContractVerifier::new("http://localhost:8080");
        let result = verifier.validate_structure(&contract);

        assert!(result.success);
    }

    #[test]
    fn test_contract_verifier_invalid_structure() {
        let mut contract = Contract::new("consumer", "provider");
        let mut interaction = Interaction::new("Test");
        interaction.request.path = String::new(); // Empty path = invalid
        interaction.response.status = 0; // Zero status = invalid
        contract.add_interaction(interaction);

        let verifier = ContractVerifier::new("http://localhost:8080");
        let result = verifier.validate_structure(&contract);

        assert!(!result.success);
        assert_eq!(result.interaction_results.len(), 1);
        assert!(!result.interaction_results[0].mismatches.is_empty());
    }

    // Service virtualization tests
    #[test]
    fn test_stub_request() {
        let request = StubRequest::new("/api/users/*")
            .with_method(HttpMethod::Get)
            .with_priority(10);

        assert!(request.matches(HttpMethod::Get, "/api/users/1", &HashMap::new()));
        assert!(!request.matches(HttpMethod::Post, "/api/users/1", &HashMap::new()));
    }

    #[test]
    fn test_stub_response() {
        let response = StubResponse::new(200)
            .with_header("Content-Type", "application/json")
            .with_body(r#"{"status": "ok"}"#)
            .with_delay(100);

        assert_eq!(response.status, 200);
        assert_eq!(response.delay_ms, 100);
    }

    #[test]
    fn test_stub_response_fault() {
        let response = StubResponse::new(500).with_fault(FaultType::Timeout);

        assert_eq!(response.fault, Some(FaultType::Timeout));
    }

    #[test]
    fn test_mock_server() {
        let mut server = MockServer::new("test-server", 8080);

        let mapping = StubMapping::new(
            "Get User",
            StubRequest::new("/users/1").with_method(HttpMethod::Get),
            StubResponse::new(200).with_body(r#"{"id": 1}"#),
        );

        server.stub(mapping);
        server.start();

        assert!(server.running);
        assert_eq!(server.mappings.len(), 1);
    }

    #[test]
    fn test_mock_server_handle_request() {
        let mut server = MockServer::new("test", 8080);

        server.stub(StubMapping::new(
            "Test",
            StubRequest::new("/test"),
            StubResponse::new(200).with_body("ok"),
        ));

        let response = server.handle_request(HttpMethod::Get, "/test", HashMap::new(), None);

        assert!(response.is_some());
        assert_eq!(response.unwrap().status, 200);
        assert_eq!(server.request_log.len(), 1);
    }

    #[test]
    fn test_mock_server_verify() {
        let mut server = MockServer::new("test", 8080);
        server.stub(StubMapping::new(
            "Test",
            StubRequest::new("/api"),
            StubResponse::new(200),
        ));

        server.handle_request(HttpMethod::Get, "/api", HashMap::new(), None);
        server.handle_request(HttpMethod::Get, "/api", HashMap::new(), None);

        assert!(server.verify_request_count("/api", 2));
    }

    // Container tests
    #[test]
    fn test_container_postgres() {
        let container = TestContainer::postgres();

        assert_eq!(container.name, "postgres");
        assert!(container.image.contains("postgres"));
        assert!(container.ports.contains_key(&5432));
    }

    #[test]
    fn test_container_redis() {
        let container = TestContainer::redis();

        assert_eq!(container.name, "redis");
        assert!(container.ports.contains_key(&6379));
    }

    #[test]
    fn test_container_docker_run() {
        let container = TestContainer::new("myapp", "myapp:latest")
            .with_env("DEBUG", "true")
            .with_port(8080, 80);

        let cmd = container.to_docker_run();
        assert!(cmd.contains("docker run"));
        assert!(cmd.contains("-e DEBUG=true"));
        assert!(cmd.contains("-p 8080:80"));
    }

    #[test]
    fn test_container_orchestrator() {
        let mut orchestrator = ContainerOrchestrator::new("test-project");

        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::redis());

        orchestrator.set_dependency("redis", "postgres");

        let order = orchestrator.start_order();
        assert!(!order.is_empty());
    }

    #[test]
    fn test_container_compose() {
        let mut orchestrator = ContainerOrchestrator::new("test");

        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::new("app", "app:latest").with_port(8080, 8080));
        orchestrator.set_dependency("app", "postgres");

        let yaml = orchestrator.generate_compose();
        assert!(yaml.contains("version:"));
        assert!(yaml.contains("services:"));
        assert!(yaml.contains("postgres:"));
    }

    // API compatibility tests
    #[test]
    fn test_api_endpoint() {
        let endpoint = ApiEndpoint::new(HttpMethod::Get, "/users")
            .with_query_param(ApiParameter::new("page", "integer"))
            .with_response(ApiSchema::object());

        assert_eq!(endpoint.path, "/users");
        assert_eq!(endpoint.query_params.len(), 1);
    }

    #[test]
    fn test_api_schema() {
        let schema = ApiSchema::object()
            .with_property("id", ApiSchemaProperty::integer().required())
            .with_property("name", ApiSchemaProperty::string().required())
            .with_property("email", ApiSchemaProperty::string().nullable());

        assert_eq!(schema.properties.len(), 3);
        assert_eq!(schema.required.len(), 2);
    }

    #[test]
    fn test_compatibility_change() {
        let change = CompatibilityChange::new(
            CompatibilityChangeType::EndpointRemoved,
            "/api/v1/users",
            "Endpoint was removed",
        );

        assert!(change.change_type.is_breaking());
    }

    #[test]
    fn test_compatibility_checker_added() {
        let old = ApiVersion::new("1.0.0");
        let mut new = ApiVersion::new("1.1.0");
        new.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users"));

        let checker = CompatibilityChecker::new();
        let changes = checker.check(&old, &new);

        assert_eq!(changes.len(), 1);
        assert_eq!(
            changes[0].change_type,
            CompatibilityChangeType::EndpointAdded
        );
    }

    #[test]
    fn test_compatibility_checker_removed() {
        let mut old = ApiVersion::new("1.0.0");
        old.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users"));

        let new = ApiVersion::new("1.1.0");

        let checker = CompatibilityChecker::new();
        let changes = checker.check(&old, &new);

        assert_eq!(changes.len(), 1);
        assert!(changes[0].change_type.is_breaking());
    }

    #[test]
    fn test_compatibility_checker_is_compatible() {
        let old = ApiVersion::new("1.0.0");
        let mut new = ApiVersion::new("1.1.0");
        new.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users"));

        let checker = CompatibilityChecker::new();
        assert!(checker.is_compatible(&old, &new)); // Adding is not breaking
    }

    #[test]
    fn test_compatibility_checker_not_compatible() {
        let mut old = ApiVersion::new("1.0.0");
        old.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users"));

        let new = ApiVersion::new("2.0.0");

        let checker = CompatibilityChecker::new();
        assert!(!checker.is_compatible(&old, &new)); // Removing is breaking
    }

    #[test]
    fn test_deprecated_endpoint() {
        let mut old = ApiVersion::new("1.0.0");
        old.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users"));

        let mut new = ApiVersion::new("1.1.0");
        new.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/users").deprecated());

        let checker = CompatibilityChecker::new();
        let changes = checker.check(&old, &new);

        assert!(changes
            .iter()
            .any(|c| c.change_type == CompatibilityChangeType::Deprecated));
    }

    #[test]
    fn test_http_method_all_variants() {
        let methods = [
            HttpMethod::Get,
            HttpMethod::Post,
            HttpMethod::Put,
            HttpMethod::Patch,
            HttpMethod::Delete,
            HttpMethod::Head,
            HttpMethod::Options,
        ];

        for method in methods {
            let _ = method.as_str();
            let _ = serde_json::to_string(&method).unwrap();
        }
    }

    #[test]
    fn test_http_method_serde_roundtrip() {
        let method = HttpMethod::Patch;
        let json = serde_json::to_string(&method).unwrap();
        let parsed: HttpMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(method, parsed);
    }

    #[test]
    fn test_matcher_regex() {
        let matcher = Matcher::Regex(r"^\d{3}$".to_string());
        assert!(matcher.matches("123"));
        assert!(!matcher.matches("12"));
        assert!(!matcher.matches("1234"));
    }

    #[test]
    fn test_matcher_type() {
        let matcher = Matcher::Type;
        assert!(matcher.matches("anything"));
        assert!(matcher.matches(""));
    }

    #[test]
    fn test_matcher_decimal() {
        let matcher = Matcher::Decimal;
        assert!(matcher.matches("12.34"));
        assert!(matcher.matches("123"));
        assert!(matcher.matches("-45.67"));
        assert!(!matcher.matches("abc"));
    }

    #[test]
    fn test_matcher_boolean() {
        let matcher = Matcher::Boolean;
        assert!(matcher.matches("true"));
        assert!(matcher.matches("false"));
        assert!(!matcher.matches("yes"));
    }

    #[test]
    fn test_matcher_null() {
        let matcher = Matcher::Null;
        assert!(matcher.matches("null"));
        assert!(matcher.matches(""));
        assert!(!matcher.matches("something"));
    }

    #[test]
    fn test_matcher_array_containing() {
        let matcher = Matcher::ArrayContaining(Box::new(Matcher::Integer));
        // This just tests it's created, actual array matching would need more
        assert!(matcher.matches("anything")); // Falls through to true
    }

    #[test]
    fn test_matcher_each_like() {
        let matcher = Matcher::EachLike(Box::new(Matcher::Exact("item".to_string())));
        assert!(matcher.matches("anything")); // Falls through to true
    }

    #[test]
    fn test_matcher_clone() {
        let matcher = Matcher::Exact("test".to_string());
        let cloned = matcher.clone();
        assert!(cloned.matches("test"));
    }

    #[test]
    fn test_matcher_serde_roundtrip() {
        let matcher = Matcher::Include("search".to_string());
        let json = serde_json::to_string(&matcher).unwrap();
        let parsed: Matcher = serde_json::from_str(&json).unwrap();
        assert!(parsed.matches("searchable"));
    }

    #[test]
    fn test_contract_request_with_query() {
        let request = ContractRequest::new(HttpMethod::Get, "/search")
            .with_query("q", "test")
            .with_query("page", "1");

        assert_eq!(request.query.len(), 2);
        assert_eq!(request.query.get("q"), Some(&"test".to_string()));
    }

    #[test]
    fn test_contract_request_with_body_matcher() {
        let request = ContractRequest::new(HttpMethod::Post, "/api")
            .with_body_matcher("$.id", Matcher::Integer)
            .with_body_matcher("$.name", Matcher::Type);

        assert_eq!(request.body_matchers.len(), 2);
    }

    #[test]
    fn test_contract_request_clone() {
        let request = ContractRequest::new(HttpMethod::Get, "/test").with_header("Auth", "token");

        let cloned = request.clone();
        assert_eq!(request.path, cloned.path);
        assert_eq!(request.headers, cloned.headers);
    }

    #[test]
    fn test_contract_response_with_body_matcher() {
        let response = ContractResponse::new(200)
            .with_body_matcher("$.status", Matcher::Exact("ok".to_string()));

        assert_eq!(response.body_matchers.len(), 1);
    }

    #[test]
    fn test_contract_response_clone() {
        let response = ContractResponse::new(404);
        let cloned = response.clone();
        assert_eq!(response.status, cloned.status);
    }

    #[test]
    fn test_interaction_clone() {
        let interaction = Interaction::new("Test").given("some state");

        let cloned = interaction.clone();
        assert_eq!(interaction.description, cloned.description);
    }

    #[test]
    fn test_contract_set_metadata() {
        let mut contract = Contract::new("consumer", "provider");
        contract.set_metadata("version", "1.0");
        contract.set_metadata("pactSpecVersion", "3.0");

        assert_eq!(contract.metadata.len(), 2);
    }

    #[test]
    fn test_contract_to_json() {
        let contract = Contract::new("consumer", "provider");
        let json = contract.to_json();

        assert!(json.contains("consumer"));
        assert!(json.contains("provider"));
    }

    #[test]
    fn test_contract_clone() {
        let mut contract = Contract::new("c", "p");
        contract.add_interaction(Interaction::new("test"));

        let cloned = contract.clone();
        assert_eq!(contract.interactions.len(), cloned.interactions.len());
    }

    #[test]
    fn test_verification_result_serde() {
        let result = VerificationResult {
            contract_id: "contract_1".to_string(),
            success: true,
            interaction_results: vec![],
            verified_at: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: VerificationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(result.success, parsed.success);
    }

    #[test]
    fn test_interaction_result_clone() {
        let result = InteractionResult {
            description: "Test".to_string(),
            success: false,
            mismatches: vec!["Mismatch".to_string()],
        };

        let cloned = result.clone();
        assert_eq!(result.mismatches, cloned.mismatches);
    }

    #[test]
    fn test_contract_verifier_register_state_handler() {
        let mut verifier = ContractVerifier::new("http://localhost");
        verifier.register_state_handler("users exist", "setup_users.sh");

        assert_eq!(verifier.state_handlers.len(), 1);
    }

    #[test]
    fn test_stub_request_matches_exact() {
        let request = StubRequest::new("/exact/path");
        assert!(request.matches(HttpMethod::Get, "/exact/path", &HashMap::new()));
    }

    #[test]
    fn test_stub_request_matches_wildcard() {
        let request = StubRequest::new("/api/*");
        assert!(request.matches(HttpMethod::Get, "/api/users", &HashMap::new()));
        assert!(request.matches(HttpMethod::Get, "/api/items", &HashMap::new()));
    }

    #[test]
    fn test_stub_request_with_header_matcher() {
        let request = StubRequest::new("/api")
            .with_header_matcher("Content-Type", Matcher::Include("json".to_string()));

        assert_eq!(request.headers.len(), 1);
    }

    #[test]
    fn test_stub_mapping_disable_enable() {
        let mut mapping =
            StubMapping::new("Test", StubRequest::new("/test"), StubResponse::new(200));

        assert!(mapping.enabled);
        mapping.disable();
        assert!(!mapping.enabled);
        mapping.enable();
        assert!(mapping.enabled);
    }

    #[test]
    fn test_stub_mapping_record_hit() {
        let mut mapping =
            StubMapping::new("Test", StubRequest::new("/test"), StubResponse::new(200));

        assert_eq!(mapping.hit_count, 0);
        mapping.record_hit();
        mapping.record_hit();
        assert_eq!(mapping.hit_count, 2);
    }

    #[test]
    fn test_fault_type_all_variants() {
        let faults = [
            FaultType::ConnectionReset,
            FaultType::EmptyResponse,
            FaultType::MalformedResponse,
            FaultType::RandomDataThenClose,
            FaultType::Timeout,
        ];

        for fault in faults {
            let _ = serde_json::to_string(&fault).unwrap();
        }
    }

    #[test]
    fn test_mock_server_reset() {
        let mut server = MockServer::new("test", 8080);
        server.stub(StubMapping::new(
            "Test",
            StubRequest::new("/test"),
            StubResponse::new(200),
        ));
        server.handle_request(HttpMethod::Get, "/test", HashMap::new(), None);

        assert_eq!(server.mappings.len(), 1);
        assert_eq!(server.request_log.len(), 1);

        server.reset();

        assert!(server.mappings.is_empty());
        assert!(server.request_log.is_empty());
    }

    #[test]
    fn test_mock_server_stop() {
        let mut server = MockServer::new("test", 8080);
        server.start();
        assert!(server.running);

        server.stop();
        assert!(!server.running);
    }

    #[test]
    fn test_mock_server_unmatched_requests() {
        let mut server = MockServer::new("test", 8080);
        server.handle_request(HttpMethod::Get, "/unknown", HashMap::new(), None);

        let unmatched = server.unmatched_requests();
        assert_eq!(unmatched.len(), 1);
    }

    #[test]
    fn test_request_log_entry_serde() {
        let entry = RequestLogEntry {
            timestamp: 1234567890,
            method: HttpMethod::Post,
            url: "/api/test".to_string(),
            headers: HashMap::new(),
            body: Some("test".to_string()),
            matched_stub: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: RequestLogEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.url, parsed.url);
    }

    #[test]
    fn test_container_type_all_variants() {
        let types = [
            ContainerType::Database,
            ContainerType::MessageQueue,
            ContainerType::Cache,
            ContainerType::MockService,
            ContainerType::Custom,
        ];

        for t in types {
            let _ = serde_json::to_string(&t).unwrap();
        }
    }

    #[test]
    fn test_container_state_all_variants() {
        let states = [
            ContainerState::Created,
            ContainerState::Starting,
            ContainerState::Running,
            ContainerState::Stopping,
            ContainerState::Stopped,
            ContainerState::Failed,
        ];

        for state in states {
            let _ = serde_json::to_string(&state).unwrap();
        }
    }

    #[test]
    fn test_test_container_rabbitmq() {
        let container = TestContainer::rabbitmq();
        assert_eq!(container.name, "rabbitmq");
        assert!(container.ports.contains_key(&5672));
        assert!(container.ports.contains_key(&15672));
    }

    #[test]
    fn test_test_container_with_volume() {
        let container =
            TestContainer::new("app", "app:latest").with_volume("/data", "/container/data");

        assert_eq!(container.volumes.len(), 1);
        assert_eq!(container.volumes[0].0, "/data");
    }

    #[test]
    fn test_test_container_with_startup_timeout() {
        let container = TestContainer::new("slow", "slow:latest")
            .with_startup_timeout(Duration::from_secs(120));

        assert_eq!(container.startup_timeout, Duration::from_secs(120));
    }

    #[test]
    fn test_container_orchestrator_start_stop_all() {
        let mut orchestrator = ContainerOrchestrator::new("test");
        orchestrator.add_container(TestContainer::postgres());
        orchestrator.add_container(TestContainer::redis());

        orchestrator.start_all();
        assert!(orchestrator.is_healthy());

        orchestrator.stop_all();
        assert!(!orchestrator.is_healthy());
    }

    #[test]
    fn test_container_orchestrator_empty() {
        let orchestrator = ContainerOrchestrator::new("empty");
        let order = orchestrator.start_order();
        assert!(order.is_empty());
        assert!(orchestrator.is_healthy()); // Empty is considered healthy
    }

    #[test]
    fn test_api_version_serde() {
        let mut version = ApiVersion::new("1.0.0");
        version.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/test"));

        let json = serde_json::to_string(&version).unwrap();
        let parsed: ApiVersion = serde_json::from_str(&json).unwrap();

        assert_eq!(version.version, parsed.version);
    }

    #[test]
    fn test_api_endpoint_with_request_body() {
        let endpoint = ApiEndpoint::new(HttpMethod::Post, "/users")
            .with_request_body(ApiSchema::object())
            .with_response(ApiSchema::object());

        assert!(endpoint.request_body.is_some());
    }

    #[test]
    fn test_api_parameter_required() {
        let param = ApiParameter::new("id", "integer").required();

        assert!(param.required);
    }

    #[test]
    fn test_api_schema_property_with_format() {
        let prop = ApiSchemaProperty::string()
            .with_format("date-time")
            .required();

        assert_eq!(prop.format, Some("date-time".to_string()));
        assert!(prop.required);
    }

    #[test]
    fn test_compatibility_change_type_is_breaking() {
        assert!(CompatibilityChangeType::EndpointRemoved.is_breaking());
        assert!(CompatibilityChangeType::RequiredParameterAdded.is_breaking());
        assert!(CompatibilityChangeType::ParameterRemoved.is_breaking());
        assert!(CompatibilityChangeType::TypeChanged.is_breaking());

        assert!(!CompatibilityChangeType::EndpointAdded.is_breaking());
        assert!(!CompatibilityChangeType::ParameterAdded.is_breaking());
        assert!(!CompatibilityChangeType::Deprecated.is_breaking());
        assert!(!CompatibilityChangeType::ResponseChanged.is_breaking());
    }

    #[test]
    fn test_compatibility_checker_default() {
        let checker = CompatibilityChecker;
        let old = ApiVersion::new("1.0");
        let new = ApiVersion::new("1.1");

        assert!(checker.is_compatible(&old, &new));
    }

    #[test]
    fn test_compatibility_checker_breaking_changes() {
        let mut old = ApiVersion::new("1.0");
        old.add_endpoint(ApiEndpoint::new(HttpMethod::Get, "/removed"));

        let new = ApiVersion::new("2.0");

        let checker = CompatibilityChecker::new();
        let breaking = checker.breaking_changes(&old, &new);

        assert_eq!(breaking.len(), 1);
    }

    #[test]
    fn test_mock_server_find_mapping_priority() {
        let mut server = MockServer::new("test", 8080);

        server.stub(StubMapping::new(
            "Low Priority",
            StubRequest::new("/api").with_priority(1),
            StubResponse::new(200),
        ));

        server.stub(StubMapping::new(
            "High Priority",
            StubRequest::new("/api").with_priority(10),
            StubResponse::new(201),
        ));

        let mapping = server.find_mapping(HttpMethod::Get, "/api", &HashMap::new());
        assert!(mapping.is_some());
        assert_eq!(mapping.unwrap().response.status, 201);
    }

    #[test]
    fn test_stub_request_clone() {
        let request = StubRequest::new("/test")
            .with_method(HttpMethod::Post)
            .with_priority(5);

        let cloned = request.clone();
        assert_eq!(request.url_pattern, cloned.url_pattern);
        assert_eq!(request.priority, cloned.priority);
    }

    #[test]
    fn test_stub_response_clone() {
        let response = StubResponse::new(200).with_body("test").with_delay(100);

        let cloned = response.clone();
        assert_eq!(response.status, cloned.status);
        assert_eq!(response.delay_ms, cloned.delay_ms);
    }

    #[test]
    fn test_test_container_clone() {
        let container = TestContainer::postgres();
        let cloned = container.clone();

        assert_eq!(container.name, cloned.name);
        assert_eq!(container.image, cloned.image);
    }

    #[test]
    fn test_container_orchestrator_clone() {
        let mut orchestrator = ContainerOrchestrator::new("test");
        orchestrator.add_container(TestContainer::postgres());

        let cloned = orchestrator.clone();
        assert_eq!(orchestrator.project, cloned.project);
        assert_eq!(orchestrator.containers.len(), cloned.containers.len());
    }

    #[test]
    fn test_matcher_regex_invalid() {
        let matcher = Matcher::Regex("[invalid".to_string());
        // Invalid regex should return false
        assert!(!matcher.matches("test"));
    }

    #[test]
    fn test_contract_verifier_clone() {
        let mut verifier = ContractVerifier::new("http://localhost:8080");
        verifier.register_state_handler("state", "handler");

        let cloned = verifier.clone();
        assert_eq!(verifier.provider_url, cloned.provider_url);
    }

    #[test]
    fn test_api_schema_empty() {
        let schema = ApiSchema::empty();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_empty());
    }

    #[test]
    fn test_api_schema_property_boolean() {
        let prop = ApiSchemaProperty::boolean().nullable();
        assert_eq!(prop.prop_type, "boolean");
        assert!(prop.nullable);
    }

    #[test]
    fn test_compatibility_change_clone() {
        let change = CompatibilityChange::new(
            CompatibilityChangeType::EndpointAdded,
            "/new",
            "New endpoint",
        );

        let cloned = change.clone();
        assert_eq!(change.path, cloned.path);
    }

    #[test]
    fn test_compatibility_checker_clone() {
        let checker = CompatibilityChecker::new();
        let cloned = checker.clone();
        // Both should work the same
        let v1 = ApiVersion::new("1.0");
        let v2 = ApiVersion::new("1.1");
        assert_eq!(
            checker.is_compatible(&v1, &v2),
            cloned.is_compatible(&v1, &v2)
        );
    }
}
