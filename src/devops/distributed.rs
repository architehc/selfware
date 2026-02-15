//! Distributed Systems Tooling
//!
//! Tools for working with distributed systems including tracing correlation,
//! consensus protocol debugging, service mesh configuration, and circuit
//! breaker tuning.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static TRACE_COUNTER: AtomicU64 = AtomicU64::new(1);
static SPAN_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_trace_id() -> String {
    format!("{:032x}", TRACE_COUNTER.fetch_add(1, Ordering::SeqCst))
}

fn generate_span_id() -> String {
    format!("{:016x}", SPAN_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// ============================================================================
// Distributed Tracing
// ============================================================================

/// Trace context for distributed tracing
#[derive(Debug, Clone)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub sampled: bool,
    pub baggage: HashMap<String, String>,
}

impl TraceContext {
    /// Create new root context
    pub fn new() -> Self {
        Self {
            trace_id: generate_trace_id(),
            span_id: generate_span_id(),
            parent_span_id: None,
            sampled: true,
            baggage: HashMap::new(),
        }
    }

    /// Create child context
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: generate_span_id(),
            parent_span_id: Some(self.span_id.clone()),
            sampled: self.sampled,
            baggage: self.baggage.clone(),
        }
    }

    /// Add baggage item
    pub fn with_baggage(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.baggage.insert(key.into(), value.into());
        self
    }

    /// Format as W3C traceparent header
    pub fn to_traceparent(&self) -> String {
        let flags = if self.sampled { "01" } else { "00" };
        format!("00-{}-{}-{}", self.trace_id, self.span_id, flags)
    }

    /// Parse from W3C traceparent header
    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() != 4 {
            return None;
        }

        Some(Self {
            trace_id: parts[1].to_string(),
            span_id: parts[2].to_string(),
            parent_span_id: None,
            sampled: parts[3] == "01",
            baggage: HashMap::new(),
        })
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A span in a distributed trace
#[derive(Debug, Clone)]
pub struct Span {
    pub context: TraceContext,
    pub operation_name: String,
    pub service_name: String,
    pub kind: SpanKind,
    pub start_time: SystemTime,
    pub end_time: Option<SystemTime>,
    pub status: SpanStatus,
    pub attributes: HashMap<String, SpanValue>,
    pub events: Vec<SpanEvent>,
    pub links: Vec<SpanLink>,
}

/// Span kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Internal,
    Server,
    Client,
    Producer,
    Consumer,
}

/// Span status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanStatus {
    Unset,
    Ok,
    Error,
}

/// Span attribute value
#[derive(Debug, Clone)]
pub enum SpanValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Array(Vec<SpanValue>),
}

/// Span event
#[derive(Debug, Clone)]
pub struct SpanEvent {
    pub name: String,
    pub timestamp: SystemTime,
    pub attributes: HashMap<String, SpanValue>,
}

/// Link to another span
#[derive(Debug, Clone)]
pub struct SpanLink {
    pub trace_id: String,
    pub span_id: String,
    pub attributes: HashMap<String, SpanValue>,
}

impl Span {
    /// Create new span
    pub fn new(
        context: TraceContext,
        operation_name: impl Into<String>,
        service_name: impl Into<String>,
    ) -> Self {
        Self {
            context,
            operation_name: operation_name.into(),
            service_name: service_name.into(),
            kind: SpanKind::Internal,
            start_time: SystemTime::now(),
            end_time: None,
            status: SpanStatus::Unset,
            attributes: HashMap::new(),
            events: Vec::new(),
            links: Vec::new(),
        }
    }

    /// Set span kind
    pub fn with_kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Add attribute
    pub fn with_attribute(mut self, key: impl Into<String>, value: SpanValue) -> Self {
        self.attributes.insert(key.into(), value);
        self
    }

    /// Add event
    pub fn add_event(&mut self, name: impl Into<String>) {
        self.events.push(SpanEvent {
            name: name.into(),
            timestamp: SystemTime::now(),
            attributes: HashMap::new(),
        });
    }

    /// Add link
    pub fn add_link(&mut self, trace_id: impl Into<String>, span_id: impl Into<String>) {
        self.links.push(SpanLink {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            attributes: HashMap::new(),
        });
    }

    /// End span
    pub fn end(&mut self) {
        self.end_time = Some(SystemTime::now());
    }

    /// End span with status
    pub fn end_with_status(&mut self, status: SpanStatus) {
        self.status = status;
        self.end_time = Some(SystemTime::now());
    }

    /// Get duration
    pub fn duration(&self) -> Option<Duration> {
        self.end_time
            .and_then(|end| end.duration_since(self.start_time).ok())
    }
}

/// Trace analyzer for correlating spans
#[derive(Debug)]
pub struct TraceAnalyzer {
    spans: HashMap<String, Vec<Span>>,
    service_map: HashMap<String, Vec<String>>,
}

impl TraceAnalyzer {
    /// Create new analyzer
    pub fn new() -> Self {
        Self {
            spans: HashMap::new(),
            service_map: HashMap::new(),
        }
    }

    /// Add span
    pub fn add_span(&mut self, span: Span) {
        let trace_id = span.context.trace_id.clone();
        let service = span.service_name.clone();

        self.spans.entry(trace_id.clone()).or_default().push(span);
        self.service_map.entry(service).or_default().push(trace_id);
    }

    /// Get trace by ID
    pub fn get_trace(&self, trace_id: &str) -> Option<&Vec<Span>> {
        self.spans.get(trace_id)
    }

    /// Find slow spans
    pub fn find_slow_spans(&self, threshold: Duration) -> Vec<&Span> {
        self.spans
            .values()
            .flatten()
            .filter(|s| s.duration().map(|d| d > threshold).unwrap_or(false))
            .collect()
    }

    /// Find error spans
    pub fn find_error_spans(&self) -> Vec<&Span> {
        self.spans
            .values()
            .flatten()
            .filter(|s| s.status == SpanStatus::Error)
            .collect()
    }

    /// Get service dependency graph
    pub fn service_dependencies(&self) -> HashMap<String, Vec<String>> {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();

        for spans in self.spans.values() {
            let mut span_map: HashMap<&str, &Span> = HashMap::new();
            for span in spans {
                span_map.insert(&span.context.span_id, span);
            }

            for span in spans {
                if let Some(parent_id) = &span.context.parent_span_id {
                    if let Some(parent) = span_map.get(parent_id.as_str()) {
                        if parent.service_name != span.service_name {
                            deps.entry(parent.service_name.clone())
                                .or_default()
                                .push(span.service_name.clone());
                        }
                    }
                }
            }
        }

        // Deduplicate
        for calls in deps.values_mut() {
            calls.sort();
            calls.dedup();
        }

        deps
    }

    /// Calculate trace latency breakdown
    pub fn latency_breakdown(&self, trace_id: &str) -> HashMap<String, Duration> {
        let mut breakdown: HashMap<String, Duration> = HashMap::new();

        if let Some(spans) = self.spans.get(trace_id) {
            for span in spans {
                if let Some(duration) = span.duration() {
                    *breakdown.entry(span.service_name.clone()).or_default() += duration;
                }
            }
        }

        breakdown
    }

    /// Correlate spans to code locations
    pub fn correlate_to_code(&self, span: &Span) -> Option<CodeLocation> {
        // Look for code.filepath and code.lineno attributes
        let filepath = match span.attributes.get("code.filepath") {
            Some(SpanValue::String(s)) => Some(s.clone()),
            _ => None,
        };

        let lineno = match span.attributes.get("code.lineno") {
            Some(SpanValue::Int(n)) if *n >= 0 => u32::try_from(*n).ok(),
            _ => None,
        };

        let function = match span.attributes.get("code.function") {
            Some(SpanValue::String(s)) => Some(s.clone()),
            _ => None,
        };

        if filepath.is_some() || function.is_some() {
            Some(CodeLocation {
                filepath,
                lineno,
                function,
            })
        } else {
            None
        }
    }
}

impl Default for TraceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Code location from span
#[derive(Debug, Clone)]
pub struct CodeLocation {
    pub filepath: Option<String>,
    pub lineno: Option<u32>,
    pub function: Option<String>,
}

// ============================================================================
// Consensus Protocol Debugging
// ============================================================================

/// Raft node state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaftState {
    Follower,
    Candidate,
    Leader,
}

/// Raft log entry
#[derive(Debug, Clone)]
pub struct RaftLogEntry {
    pub term: u64,
    pub index: u64,
    pub command: String,
    pub committed: bool,
}

/// Raft node for debugging
#[derive(Debug, Clone)]
pub struct RaftNode {
    pub id: String,
    pub state: RaftState,
    pub current_term: u64,
    pub voted_for: Option<String>,
    pub log: Vec<RaftLogEntry>,
    pub commit_index: u64,
    pub last_applied: u64,
    pub leader_id: Option<String>,
    pub peers: Vec<String>,
}

impl RaftNode {
    /// Create new node
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: RaftState::Follower,
            current_term: 0,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
            leader_id: None,
            peers: Vec::new(),
        }
    }

    /// Add peer
    pub fn with_peer(mut self, peer: impl Into<String>) -> Self {
        self.peers.push(peer.into());
        self
    }

    /// Become candidate
    pub fn become_candidate(&mut self) {
        self.state = RaftState::Candidate;
        self.current_term += 1;
        self.voted_for = Some(self.id.clone());
    }

    /// Become leader
    pub fn become_leader(&mut self) {
        self.state = RaftState::Leader;
        self.leader_id = Some(self.id.clone());
    }

    /// Become follower
    pub fn become_follower(&mut self, term: u64) {
        self.state = RaftState::Follower;
        self.current_term = term;
        self.voted_for = None;
    }

    /// Append log entry
    pub fn append_entry(&mut self, command: impl Into<String>) {
        let index = self.log.len() as u64 + 1;
        self.log.push(RaftLogEntry {
            term: self.current_term,
            index,
            command: command.into(),
            committed: false,
        });
    }

    /// Commit entries up to index
    pub fn commit_to(&mut self, index: u64) {
        for entry in &mut self.log {
            if entry.index <= index && !entry.committed {
                entry.committed = true;
            }
        }
        self.commit_index = index;
    }

    /// Get last log index
    pub fn last_log_index(&self) -> u64 {
        self.log.last().map(|e| e.index).unwrap_or(0)
    }

    /// Get last log term
    pub fn last_log_term(&self) -> u64 {
        self.log.last().map(|e| e.term).unwrap_or(0)
    }
}

/// Raft cluster debugger
#[derive(Debug)]
pub struct RaftDebugger {
    nodes: HashMap<String, RaftNode>,
    messages: Vec<RaftMessage>,
    timeline: Vec<RaftEvent>,
}

/// Raft message types
#[derive(Debug, Clone)]
pub enum RaftMessage {
    RequestVote {
        from: String,
        to: String,
        term: u64,
        last_log_index: u64,
        last_log_term: u64,
    },
    RequestVoteResponse {
        from: String,
        to: String,
        term: u64,
        granted: bool,
    },
    AppendEntries {
        from: String,
        to: String,
        term: u64,
        entries: Vec<RaftLogEntry>,
        leader_commit: u64,
    },
    AppendEntriesResponse {
        from: String,
        to: String,
        term: u64,
        success: bool,
        match_index: u64,
    },
}

/// Raft event for timeline
#[derive(Debug, Clone)]
pub struct RaftEvent {
    pub timestamp: SystemTime,
    pub node_id: String,
    pub event_type: RaftEventType,
    pub details: String,
}

/// Raft event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaftEventType {
    StateChange,
    TermChange,
    VoteGranted,
    VoteDenied,
    LogAppend,
    LogCommit,
    LeaderElected,
    Timeout,
}

impl RaftDebugger {
    /// Create new debugger
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            messages: Vec::new(),
            timeline: Vec::new(),
        }
    }

    /// Add node
    pub fn add_node(&mut self, node: RaftNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Get node
    pub fn get_node(&self, id: &str) -> Option<&RaftNode> {
        self.nodes.get(id)
    }

    /// Get node mut
    pub fn get_node_mut(&mut self, id: &str) -> Option<&mut RaftNode> {
        self.nodes.get_mut(id)
    }

    /// Record event
    pub fn record_event(
        &mut self,
        node_id: &str,
        event_type: RaftEventType,
        details: impl Into<String>,
    ) {
        self.timeline.push(RaftEvent {
            timestamp: SystemTime::now(),
            node_id: node_id.to_string(),
            event_type,
            details: details.into(),
        });
    }

    /// Send message
    pub fn send_message(&mut self, message: RaftMessage) {
        self.messages.push(message);
    }

    /// Find leader
    pub fn find_leader(&self) -> Option<&RaftNode> {
        self.nodes.values().find(|n| n.state == RaftState::Leader)
    }

    /// Check cluster health
    pub fn cluster_health(&self) -> ClusterHealth {
        let total = self.nodes.len();
        let leaders: Vec<_> = self
            .nodes
            .values()
            .filter(|n| n.state == RaftState::Leader)
            .collect();

        let followers = self
            .nodes
            .values()
            .filter(|n| n.state == RaftState::Follower)
            .count();

        let candidates = self
            .nodes
            .values()
            .filter(|n| n.state == RaftState::Candidate)
            .count();

        let issues = if leaders.len() > 1 {
            vec!["Split brain: multiple leaders detected".to_string()]
        } else if leaders.is_empty() && total > 0 {
            vec!["No leader elected".to_string()]
        } else {
            vec![]
        };

        ClusterHealth {
            total_nodes: total,
            leaders: leaders.len(),
            followers,
            candidates,
            healthy: issues.is_empty(),
            issues,
        }
    }

    /// Detect split brain
    pub fn detect_split_brain(&self) -> bool {
        self.nodes
            .values()
            .filter(|n| n.state == RaftState::Leader)
            .count()
            > 1
    }

    /// Get log divergence
    pub fn log_divergence(&self) -> HashMap<String, u64> {
        let max_index = self
            .nodes
            .values()
            .map(|n| n.last_log_index())
            .max()
            .unwrap_or(0);

        self.nodes
            .iter()
            .map(|(id, node)| (id.clone(), max_index - node.last_log_index()))
            .collect()
    }

    /// Get timeline
    pub fn timeline(&self) -> &[RaftEvent] {
        &self.timeline
    }
}

impl Default for RaftDebugger {
    fn default() -> Self {
        Self::new()
    }
}

/// Cluster health status
#[derive(Debug, Clone)]
pub struct ClusterHealth {
    pub total_nodes: usize,
    pub leaders: usize,
    pub followers: usize,
    pub candidates: usize,
    pub healthy: bool,
    pub issues: Vec<String>,
}

// ============================================================================
// Service Mesh Configuration
// ============================================================================

/// Service mesh type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceMeshType {
    Istio,
    Linkerd,
    Consul,
    Envoy,
}

impl ServiceMeshType {
    /// Get mesh name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Istio => "Istio",
            Self::Linkerd => "Linkerd",
            Self::Consul => "Consul Connect",
            Self::Envoy => "Envoy",
        }
    }
}

/// Traffic policy
#[derive(Debug, Clone)]
pub struct TrafficPolicy {
    pub name: String,
    pub destination: String,
    pub load_balancer: LoadBalancer,
    pub connection_pool: Option<ConnectionPool>,
    pub outlier_detection: Option<OutlierDetection>,
    pub tls: Option<TLSSettings>,
}

/// Load balancer settings
#[derive(Debug, Clone)]
pub struct LoadBalancer {
    pub algorithm: LoadBalancerAlgorithm,
    pub simple: Option<SimpleLoadBalancer>,
    pub consistent_hash: Option<ConsistentHash>,
}

/// Load balancer algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadBalancerAlgorithm {
    RoundRobin,
    LeastConnections,
    Random,
    Passthrough,
    ConsistentHash,
}

/// Simple load balancer type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleLoadBalancer {
    RoundRobin,
    LeastConn,
    Random,
    Passthrough,
}

/// Consistent hash settings
#[derive(Debug, Clone)]
pub struct ConsistentHash {
    pub hash_key: HashKey,
    pub minimum_ring_size: u32,
}

/// Hash key for consistent hashing
#[derive(Debug, Clone)]
pub enum HashKey {
    Header(String),
    Cookie(String),
    SourceIP,
    QueryParam(String),
}

/// Connection pool settings
#[derive(Debug, Clone)]
pub struct ConnectionPool {
    pub tcp: TcpSettings,
    pub http: Option<HttpSettings>,
}

/// TCP connection settings
#[derive(Debug, Clone)]
pub struct TcpSettings {
    pub max_connections: u32,
    pub connect_timeout: Duration,
    pub tcp_keepalive: Option<TcpKeepalive>,
}

/// TCP keepalive settings
#[derive(Debug, Clone)]
pub struct TcpKeepalive {
    pub time: Duration,
    pub interval: Duration,
    pub probes: u32,
}

/// HTTP connection settings
#[derive(Debug, Clone)]
pub struct HttpSettings {
    pub h2_upgrade_policy: H2UpgradePolicy,
    pub max_requests_per_connection: u32,
    pub max_retries: u32,
    pub idle_timeout: Duration,
}

/// HTTP/2 upgrade policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H2UpgradePolicy {
    Default,
    DoNotUpgrade,
    UpgradeIfWhen,
}

/// Outlier detection settings
#[derive(Debug, Clone)]
pub struct OutlierDetection {
    pub consecutive_errors: u32,
    pub interval: Duration,
    pub base_ejection_time: Duration,
    pub max_ejection_percent: u32,
    pub min_health_percent: u32,
}

/// TLS settings
#[derive(Debug, Clone)]
pub struct TLSSettings {
    pub mode: TLSMode,
    pub client_certificate: Option<String>,
    pub private_key: Option<String>,
    pub ca_certificates: Option<String>,
    pub sni: Option<String>,
}

/// TLS mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TLSMode {
    Disable,
    Simple,
    Mutual,
    Istio,
}

impl TrafficPolicy {
    /// Create new policy
    pub fn new(name: impl Into<String>, destination: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            destination: destination.into(),
            load_balancer: LoadBalancer {
                algorithm: LoadBalancerAlgorithm::RoundRobin,
                simple: Some(SimpleLoadBalancer::RoundRobin),
                consistent_hash: None,
            },
            connection_pool: None,
            outlier_detection: None,
            tls: None,
        }
    }

    /// Set load balancer
    pub fn with_load_balancer(mut self, algorithm: LoadBalancerAlgorithm) -> Self {
        self.load_balancer.algorithm = algorithm;
        self
    }

    /// Set connection pool
    pub fn with_connection_pool(mut self, pool: ConnectionPool) -> Self {
        self.connection_pool = Some(pool);
        self
    }

    /// Set outlier detection
    pub fn with_outlier_detection(mut self, detection: OutlierDetection) -> Self {
        self.outlier_detection = Some(detection);
        self
    }

    /// Generate Istio YAML
    pub fn to_istio_yaml(&self) -> String {
        let mut yaml = String::new();

        yaml.push_str("apiVersion: networking.istio.io/v1beta1\n");
        yaml.push_str("kind: DestinationRule\n");
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", self.name));
        yaml.push_str("spec:\n");
        yaml.push_str(&format!("  host: {}\n", self.destination));
        yaml.push_str("  trafficPolicy:\n");
        yaml.push_str("    loadBalancer:\n");
        yaml.push_str(&format!(
            "      simple: {}\n",
            match self.load_balancer.algorithm {
                LoadBalancerAlgorithm::RoundRobin => "ROUND_ROBIN",
                LoadBalancerAlgorithm::LeastConnections => "LEAST_CONN",
                LoadBalancerAlgorithm::Random => "RANDOM",
                LoadBalancerAlgorithm::Passthrough => "PASSTHROUGH",
                LoadBalancerAlgorithm::ConsistentHash => "ROUND_ROBIN",
            }
        ));

        if let Some(ref pool) = self.connection_pool {
            yaml.push_str("    connectionPool:\n");
            yaml.push_str("      tcp:\n");
            yaml.push_str(&format!(
                "        maxConnections: {}\n",
                pool.tcp.max_connections
            ));
            yaml.push_str(&format!(
                "        connectTimeout: {}s\n",
                pool.tcp.connect_timeout.as_secs()
            ));
        }

        if let Some(ref detection) = self.outlier_detection {
            yaml.push_str("    outlierDetection:\n");
            yaml.push_str(&format!(
                "      consecutiveErrors: {}\n",
                detection.consecutive_errors
            ));
            yaml.push_str(&format!(
                "      interval: {}s\n",
                detection.interval.as_secs()
            ));
            yaml.push_str(&format!(
                "      baseEjectionTime: {}s\n",
                detection.base_ejection_time.as_secs()
            ));
            yaml.push_str(&format!(
                "      maxEjectionPercent: {}\n",
                detection.max_ejection_percent
            ));
        }

        yaml
    }
}

/// Virtual service for routing
#[derive(Debug, Clone)]
pub struct VirtualService {
    pub name: String,
    pub hosts: Vec<String>,
    pub http_routes: Vec<HttpRoute>,
}

/// HTTP route
#[derive(Debug, Clone)]
pub struct HttpRoute {
    pub name: String,
    pub match_rules: Vec<HttpMatchRequest>,
    pub route: Vec<RouteDestination>,
    pub timeout: Option<Duration>,
    pub retries: Option<RetryPolicy>,
}

/// HTTP match request
#[derive(Debug, Clone)]
pub struct HttpMatchRequest {
    pub uri: Option<StringMatch>,
    pub headers: HashMap<String, StringMatch>,
    pub method: Option<String>,
}

/// String matching
#[derive(Debug, Clone)]
pub enum StringMatch {
    Exact(String),
    Prefix(String),
    Regex(String),
}

/// Route destination
#[derive(Debug, Clone)]
pub struct RouteDestination {
    pub host: String,
    pub port: u16,
    pub weight: u32,
}

/// Retry policy
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub attempts: u32,
    pub per_try_timeout: Duration,
    pub retry_on: Vec<String>,
}

impl VirtualService {
    /// Create new virtual service
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hosts: Vec::new(),
            http_routes: Vec::new(),
        }
    }

    /// Add host
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.hosts.push(host.into());
        self
    }

    /// Add route
    pub fn add_route(&mut self, route: HttpRoute) {
        self.http_routes.push(route);
    }

    /// Generate Istio YAML
    pub fn to_istio_yaml(&self) -> String {
        let mut yaml = String::new();

        yaml.push_str("apiVersion: networking.istio.io/v1beta1\n");
        yaml.push_str("kind: VirtualService\n");
        yaml.push_str("metadata:\n");
        yaml.push_str(&format!("  name: {}\n", self.name));
        yaml.push_str("spec:\n");
        yaml.push_str("  hosts:\n");
        for host in &self.hosts {
            yaml.push_str(&format!("    - {}\n", host));
        }

        if !self.http_routes.is_empty() {
            yaml.push_str("  http:\n");
            for route in &self.http_routes {
                yaml.push_str(&format!("    - name: {}\n", route.name));
                if let Some(timeout) = route.timeout {
                    yaml.push_str(&format!("      timeout: {}s\n", timeout.as_secs()));
                }
                yaml.push_str("      route:\n");
                for dest in &route.route {
                    yaml.push_str("        - destination:\n");
                    yaml.push_str(&format!("            host: {}\n", dest.host));
                    yaml.push_str("            port:\n");
                    yaml.push_str(&format!("              number: {}\n", dest.port));
                    yaml.push_str(&format!("          weight: {}\n", dest.weight));
                }
            }
        }

        yaml
    }
}

// ============================================================================
// Circuit Breaker
// ============================================================================

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker configuration
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub timeout: Duration,
    pub half_open_max_calls: u32,
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
            window_size: Duration::from_secs(60),
        }
    }
}

/// Circuit breaker
#[derive(Debug)]
pub struct CircuitBreaker {
    pub name: String,
    pub config: CircuitBreakerConfig,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<SystemTime>,
    pub last_state_change: SystemTime,
    pub metrics: CircuitBreakerMetrics,
}

/// Circuit breaker metrics
#[derive(Debug, Clone, Default)]
pub struct CircuitBreakerMetrics {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub rejected_calls: u64,
    pub state_changes: u64,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            name: name.into(),
            config,
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            last_state_change: SystemTime::now(),
            metrics: CircuitBreakerMetrics::default(),
        }
    }

    /// Check if call is allowed
    pub fn allow_call(&mut self) -> bool {
        self.metrics.total_calls += 1;

        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has passed
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed().unwrap_or_default() >= self.config.timeout {
                        self.transition_to(CircuitState::HalfOpen);
                        true
                    } else {
                        self.metrics.rejected_calls += 1;
                        false
                    }
                } else {
                    self.metrics.rejected_calls += 1;
                    false
                }
            }
            CircuitState::HalfOpen => {
                // Allow limited calls
                self.success_count + self.failure_count < self.config.half_open_max_calls
            }
        }
    }

    /// Record success
    pub fn record_success(&mut self) {
        self.metrics.successful_calls += 1;

        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.success_threshold {
                    self.transition_to(CircuitState::Closed);
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Record failure
    pub fn record_failure(&mut self) {
        self.metrics.failed_calls += 1;
        self.last_failure_time = Some(SystemTime::now());

        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                if self.failure_count >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                self.failure_count += 1;
                self.transition_to(CircuitState::Open);
            }
            CircuitState::Open => {}
        }
    }

    /// Transition to new state
    fn transition_to(&mut self, new_state: CircuitState) {
        self.state = new_state;
        self.last_state_change = SystemTime::now();
        self.failure_count = 0;
        self.success_count = 0;
        self.metrics.state_changes += 1;
    }

    /// Reset circuit breaker
    pub fn reset(&mut self) {
        self.transition_to(CircuitState::Closed);
    }

    /// Get health percentage
    pub fn health_percentage(&self) -> f64 {
        if self.metrics.total_calls == 0 {
            return 100.0;
        }
        (self.metrics.successful_calls as f64 / self.metrics.total_calls as f64) * 100.0
    }

    /// Get failure rate
    pub fn failure_rate(&self) -> f64 {
        if self.metrics.total_calls == 0 {
            return 0.0;
        }
        (self.metrics.failed_calls as f64 / self.metrics.total_calls as f64) * 100.0
    }
}

/// Circuit breaker tuner
#[derive(Debug)]
pub struct CircuitBreakerTuner {
    samples: Vec<TuningSample>,
    target_failure_rate: f64,
    _target_recovery_time: Duration,
}

/// Sample for tuning
#[derive(Debug, Clone)]
pub struct TuningSample {
    pub timestamp: SystemTime,
    pub failure_rate: f64,
    pub latency_p99: Duration,
    pub state: CircuitState,
}

/// Tuning recommendation
#[derive(Debug, Clone)]
pub struct TuningRecommendation {
    pub parameter: String,
    pub current_value: String,
    pub recommended_value: String,
    pub reason: String,
}

impl CircuitBreakerTuner {
    /// Create new tuner
    pub fn new(target_failure_rate: f64, target_recovery_time: Duration) -> Self {
        Self {
            samples: Vec::new(),
            target_failure_rate,
            _target_recovery_time: target_recovery_time,
        }
    }

    /// Add sample
    pub fn add_sample(&mut self, sample: TuningSample) {
        self.samples.push(sample);
    }

    /// Analyze and recommend
    pub fn recommend(&self, current_config: &CircuitBreakerConfig) -> Vec<TuningRecommendation> {
        let mut recommendations = Vec::new();

        if self.samples.is_empty() {
            return recommendations;
        }

        let avg_failure_rate: f64 =
            self.samples.iter().map(|s| s.failure_rate).sum::<f64>() / self.samples.len() as f64;

        // Recommend failure threshold adjustment
        if avg_failure_rate > self.target_failure_rate * 1.5 {
            let new_threshold = (current_config.failure_threshold as f64 * 0.8).max(2.0) as u32;
            recommendations.push(TuningRecommendation {
                parameter: "failure_threshold".to_string(),
                current_value: current_config.failure_threshold.to_string(),
                recommended_value: new_threshold.to_string(),
                reason: format!(
                    "High failure rate ({:.1}%) detected, lower threshold for faster circuit opening",
                    avg_failure_rate
                ),
            });
        } else if avg_failure_rate < self.target_failure_rate * 0.5 {
            let new_threshold = (current_config.failure_threshold as f64 * 1.2).min(20.0) as u32;
            recommendations.push(TuningRecommendation {
                parameter: "failure_threshold".to_string(),
                current_value: current_config.failure_threshold.to_string(),
                recommended_value: new_threshold.to_string(),
                reason: format!(
                    "Low failure rate ({:.1}%), can increase threshold to reduce false positives",
                    avg_failure_rate
                ),
            });
        }

        // Recommend timeout adjustment
        let open_samples: Vec<_> = self
            .samples
            .iter()
            .filter(|s| s.state == CircuitState::Open)
            .collect();

        if !open_samples.is_empty() {
            let avg_latency: Duration =
                open_samples.iter().map(|s| s.latency_p99).sum::<Duration>()
                    / open_samples.len() as u32;

            if avg_latency > current_config.timeout {
                let new_timeout = avg_latency + Duration::from_secs(10);
                recommendations.push(TuningRecommendation {
                    parameter: "timeout".to_string(),
                    current_value: format!("{}s", current_config.timeout.as_secs()),
                    recommended_value: format!("{}s", new_timeout.as_secs()),
                    reason: "P99 latency exceeds current timeout, increase to allow recovery"
                        .to_string(),
                });
            }
        }

        recommendations
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_new() {
        let ctx = TraceContext::new();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.parent_span_id.is_none());
        assert!(ctx.sampled);
    }

    #[test]
    fn test_trace_context_child() {
        let parent = TraceContext::new();
        let child = parent.child();

        assert_eq!(parent.trace_id, child.trace_id);
        assert_ne!(parent.span_id, child.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id.clone()));
    }

    #[test]
    fn test_trace_context_traceparent() {
        let ctx = TraceContext::new();
        let header = ctx.to_traceparent();

        assert!(header.starts_with("00-"));
        assert!(header.ends_with("-01"));

        let parsed = TraceContext::from_traceparent(&header).unwrap();
        assert_eq!(ctx.trace_id, parsed.trace_id);
        assert_eq!(ctx.span_id, parsed.span_id);
    }

    #[test]
    fn test_span_creation() {
        let ctx = TraceContext::new();
        let span = Span::new(ctx, "test-operation", "test-service").with_kind(SpanKind::Server);

        assert_eq!(span.operation_name, "test-operation");
        assert_eq!(span.service_name, "test-service");
        assert_eq!(span.kind, SpanKind::Server);
    }

    #[test]
    fn test_span_end() {
        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "test", "service");

        assert!(span.end_time.is_none());
        span.end();
        assert!(span.end_time.is_some());
        assert!(span.duration().is_some());
    }

    #[test]
    fn test_span_events() {
        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "test", "service");

        span.add_event("event1");
        span.add_event("event2");

        assert_eq!(span.events.len(), 2);
    }

    #[test]
    fn test_trace_analyzer() {
        let mut analyzer = TraceAnalyzer::new();

        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "test", "service-a");
        span.end_with_status(SpanStatus::Error);

        analyzer.add_span(span);

        let errors = analyzer.find_error_spans();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_trace_analyzer_slow_spans() {
        let mut analyzer = TraceAnalyzer::new();

        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "slow", "service");
        std::thread::sleep(Duration::from_millis(10));
        span.end();

        analyzer.add_span(span);

        let slow = analyzer.find_slow_spans(Duration::from_millis(5));
        assert_eq!(slow.len(), 1);
    }

    #[test]
    fn test_raft_node_creation() {
        let node = RaftNode::new("node-1")
            .with_peer("node-2")
            .with_peer("node-3");

        assert_eq!(node.id, "node-1");
        assert_eq!(node.state, RaftState::Follower);
        assert_eq!(node.peers.len(), 2);
    }

    #[test]
    fn test_raft_node_become_candidate() {
        let mut node = RaftNode::new("node-1");
        node.become_candidate();

        assert_eq!(node.state, RaftState::Candidate);
        assert_eq!(node.current_term, 1);
        assert_eq!(node.voted_for, Some("node-1".to_string()));
    }

    #[test]
    fn test_raft_node_become_leader() {
        let mut node = RaftNode::new("node-1");
        node.become_candidate();
        node.become_leader();

        assert_eq!(node.state, RaftState::Leader);
        assert_eq!(node.leader_id, Some("node-1".to_string()));
    }

    #[test]
    fn test_raft_node_log() {
        let mut node = RaftNode::new("node-1");
        node.become_leader();

        node.append_entry("SET x 1");
        node.append_entry("SET y 2");

        assert_eq!(node.last_log_index(), 2);
        assert_eq!(node.log.len(), 2);
    }

    #[test]
    fn test_raft_node_commit() {
        let mut node = RaftNode::new("node-1");
        node.become_leader();

        node.append_entry("SET x 1");
        node.append_entry("SET y 2");
        node.commit_to(1);

        assert!(node.log[0].committed);
        assert!(!node.log[1].committed);
        assert_eq!(node.commit_index, 1);
    }

    #[test]
    fn test_raft_debugger() {
        let mut debugger = RaftDebugger::new();

        let mut node1 = RaftNode::new("node-1");
        node1.become_leader();
        debugger.add_node(node1);

        debugger.add_node(RaftNode::new("node-2"));
        debugger.add_node(RaftNode::new("node-3"));

        let leader = debugger.find_leader();
        assert!(leader.is_some());
        assert_eq!(leader.unwrap().id, "node-1");
    }

    #[test]
    fn test_raft_cluster_health() {
        let mut debugger = RaftDebugger::new();

        let mut leader = RaftNode::new("node-1");
        leader.become_leader();
        debugger.add_node(leader);

        debugger.add_node(RaftNode::new("node-2"));
        debugger.add_node(RaftNode::new("node-3"));

        let health = debugger.cluster_health();
        assert!(health.healthy);
        assert_eq!(health.leaders, 1);
        assert_eq!(health.followers, 2);
    }

    #[test]
    fn test_raft_split_brain() {
        let mut debugger = RaftDebugger::new();

        let mut leader1 = RaftNode::new("node-1");
        leader1.become_leader();
        debugger.add_node(leader1);

        let mut leader2 = RaftNode::new("node-2");
        leader2.become_leader();
        debugger.add_node(leader2);

        assert!(debugger.detect_split_brain());
        let health = debugger.cluster_health();
        assert!(!health.healthy);
    }

    #[test]
    fn test_traffic_policy() {
        let policy = TrafficPolicy::new("my-policy", "my-service")
            .with_load_balancer(LoadBalancerAlgorithm::LeastConnections);

        assert_eq!(policy.destination, "my-service");
        assert_eq!(
            policy.load_balancer.algorithm,
            LoadBalancerAlgorithm::LeastConnections
        );
    }

    #[test]
    fn test_traffic_policy_to_yaml() {
        let policy = TrafficPolicy::new("test-policy", "test-service");
        let yaml = policy.to_istio_yaml();

        assert!(yaml.contains("DestinationRule"));
        assert!(yaml.contains("test-policy"));
        assert!(yaml.contains("test-service"));
    }

    #[test]
    fn test_traffic_policy_with_outlier_detection() {
        let policy =
            TrafficPolicy::new("test", "service").with_outlier_detection(OutlierDetection {
                consecutive_errors: 5,
                interval: Duration::from_secs(10),
                base_ejection_time: Duration::from_secs(30),
                max_ejection_percent: 50,
                min_health_percent: 30,
            });

        let yaml = policy.to_istio_yaml();
        assert!(yaml.contains("outlierDetection"));
        assert!(yaml.contains("consecutiveErrors: 5"));
    }

    #[test]
    fn test_virtual_service() {
        let vs = VirtualService::new("my-vs").with_host("my-service.default.svc.cluster.local");

        assert_eq!(vs.name, "my-vs");
        assert_eq!(vs.hosts.len(), 1);
    }

    #[test]
    fn test_virtual_service_to_yaml() {
        let vs = VirtualService::new("test-vs").with_host("test-service");
        let yaml = vs.to_istio_yaml();

        assert!(yaml.contains("VirtualService"));
        assert!(yaml.contains("test-vs"));
        assert!(yaml.contains("test-service"));
    }

    #[test]
    fn test_circuit_breaker_closed() {
        let mut cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());

        assert_eq!(cb.state, CircuitState::Closed);
        assert!(cb.allow_call());
    }

    #[test]
    fn test_circuit_breaker_opens_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("test", config);

        cb.allow_call();
        cb.record_failure();
        cb.allow_call();
        cb.record_failure();
        cb.allow_call();
        cb.record_failure();

        assert_eq!(cb.state, CircuitState::Open);
    }

    #[test]
    fn test_circuit_breaker_rejects_when_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            timeout: Duration::from_secs(60),
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("test", config);

        cb.allow_call();
        cb.record_failure();

        assert_eq!(cb.state, CircuitState::Open);
        assert!(!cb.allow_call());
        assert_eq!(cb.metrics.rejected_calls, 1);
    }

    #[test]
    fn test_circuit_breaker_closes_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_millis(1),
            half_open_max_calls: 5,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("test", config);

        // Open the circuit
        cb.allow_call();
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(5));

        // Should transition to half-open
        assert!(cb.allow_call());
        assert_eq!(cb.state, CircuitState::HalfOpen);

        // Record successes
        cb.record_success();
        cb.allow_call();
        cb.record_success();

        assert_eq!(cb.state, CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_metrics() {
        let mut cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());

        cb.allow_call();
        cb.record_success();
        cb.allow_call();
        cb.record_failure();

        assert_eq!(cb.metrics.total_calls, 2);
        assert_eq!(cb.metrics.successful_calls, 1);
        assert_eq!(cb.metrics.failed_calls, 1);
    }

    #[test]
    fn test_circuit_breaker_health() {
        let mut cb = CircuitBreaker::new("test", CircuitBreakerConfig::default());

        for _ in 0..8 {
            cb.allow_call();
            cb.record_success();
        }
        for _ in 0..2 {
            cb.allow_call();
            cb.record_failure();
        }

        assert!((cb.health_percentage() - 80.0).abs() < 0.1);
        assert!((cb.failure_rate() - 20.0).abs() < 0.1);
    }

    #[test]
    fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new("test", config);

        cb.allow_call();
        cb.record_failure();
        assert_eq!(cb.state, CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state, CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_tuner() {
        let tuner = CircuitBreakerTuner::new(5.0, Duration::from_secs(30));
        let config = CircuitBreakerConfig::default();

        let recommendations = tuner.recommend(&config);
        assert!(recommendations.is_empty()); // No samples yet
    }

    #[test]
    fn test_circuit_breaker_tuner_high_failure() {
        let mut tuner = CircuitBreakerTuner::new(5.0, Duration::from_secs(30));

        tuner.add_sample(TuningSample {
            timestamp: SystemTime::now(),
            failure_rate: 15.0,
            latency_p99: Duration::from_millis(100),
            state: CircuitState::Closed,
        });

        let config = CircuitBreakerConfig::default();
        let recommendations = tuner.recommend(&config);

        assert!(!recommendations.is_empty());
        assert!(recommendations
            .iter()
            .any(|r| r.parameter == "failure_threshold"));
    }

    #[test]
    fn test_service_mesh_type() {
        assert_eq!(ServiceMeshType::Istio.name(), "Istio");
        assert_eq!(ServiceMeshType::Linkerd.name(), "Linkerd");
    }

    #[test]
    fn test_code_location_from_span() {
        let analyzer = TraceAnalyzer::new();
        let ctx = TraceContext::new();
        let span = Span::new(ctx, "test", "service")
            .with_attribute(
                "code.filepath",
                SpanValue::String("/src/main.rs".to_string()),
            )
            .with_attribute("code.lineno", SpanValue::Int(42))
            .with_attribute(
                "code.function",
                SpanValue::String("handle_request".to_string()),
            );

        let location = analyzer.correlate_to_code(&span);
        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.filepath, Some("/src/main.rs".to_string()));
        assert_eq!(loc.lineno, Some(42));
    }

    #[test]
    fn test_latency_breakdown() {
        let mut analyzer = TraceAnalyzer::new();

        let ctx = TraceContext::new();
        let trace_id = ctx.trace_id.clone();

        let mut span1 = Span::new(ctx.clone(), "op1", "service-a");
        span1.end();
        analyzer.add_span(span1);

        let mut span2 = Span::new(ctx.child(), "op2", "service-b");
        span2.end();
        analyzer.add_span(span2);

        let breakdown = analyzer.latency_breakdown(&trace_id);
        assert!(breakdown.contains_key("service-a"));
        assert!(breakdown.contains_key("service-b"));
    }

    #[test]
    fn test_log_divergence() {
        let mut debugger = RaftDebugger::new();

        let mut node1 = RaftNode::new("node-1");
        node1.become_leader();
        node1.append_entry("cmd1");
        node1.append_entry("cmd2");
        debugger.add_node(node1);

        let mut node2 = RaftNode::new("node-2");
        node2.append_entry("cmd1");
        debugger.add_node(node2);

        let divergence = debugger.log_divergence();
        assert_eq!(divergence.get("node-1"), Some(&0));
        assert_eq!(divergence.get("node-2"), Some(&1));
    }

    // ================== Additional Coverage Tests ==================

    #[test]
    fn test_trace_context_default() {
        let ctx = TraceContext::default();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.sampled);
    }

    #[test]
    fn test_trace_context_with_baggage() {
        let ctx = TraceContext::new()
            .with_baggage("user-id", "12345")
            .with_baggage("request-id", "abc");
        assert_eq!(ctx.baggage.get("user-id"), Some(&"12345".to_string()));
        assert_eq!(ctx.baggage.len(), 2);
    }

    #[test]
    fn test_trace_context_from_invalid_traceparent() {
        assert!(TraceContext::from_traceparent("invalid").is_none());
        assert!(TraceContext::from_traceparent("00-abc").is_none());
    }

    #[test]
    fn test_trace_context_traceparent_unsampled() {
        let mut ctx = TraceContext::new();
        ctx.sampled = false;
        let header = ctx.to_traceparent();
        assert!(header.ends_with("-00"));

        let parsed = TraceContext::from_traceparent(&header);
        assert!(parsed.is_some());
        assert!(!parsed.unwrap().sampled);
    }

    #[test]
    fn test_span_kind_all_variants() {
        let kinds = vec![
            SpanKind::Internal,
            SpanKind::Server,
            SpanKind::Client,
            SpanKind::Producer,
            SpanKind::Consumer,
        ];
        for kind in kinds {
            let ctx = TraceContext::new();
            let span = Span::new(ctx, "op", "svc").with_kind(kind);
            assert_eq!(span.kind, kind);
        }
    }

    #[test]
    fn test_span_status_all_variants() {
        let statuses = vec![SpanStatus::Unset, SpanStatus::Ok, SpanStatus::Error];
        for status in statuses {
            let ctx = TraceContext::new();
            let mut span = Span::new(ctx, "op", "svc");
            span.end_with_status(status);
            assert_eq!(span.status, status);
        }
    }

    #[test]
    fn test_span_value_all_variants() {
        let values = vec![
            SpanValue::String("test".to_string()),
            SpanValue::Int(42),
            SpanValue::Float(3.15),
            SpanValue::Bool(true),
            SpanValue::Array(vec![SpanValue::Int(1), SpanValue::Int(2)]),
        ];
        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "op", "svc");

        for (i, value) in values.into_iter().enumerate() {
            span.attributes.insert(format!("attr_{}", i), value);
        }
        assert_eq!(span.attributes.len(), 5);
    }

    #[test]
    fn test_span_add_link() {
        let ctx = TraceContext::new();
        let mut span = Span::new(ctx, "op", "svc");

        span.add_link("trace-123", "span-456");
        span.add_link("trace-789", "span-012");

        assert_eq!(span.links.len(), 2);
        assert_eq!(span.links[0].trace_id, "trace-123");
    }

    #[test]
    fn test_trace_analyzer_get_trace() {
        let mut analyzer = TraceAnalyzer::new();
        let ctx = TraceContext::new();
        let trace_id = ctx.trace_id.clone();

        let mut span = Span::new(ctx, "op", "svc");
        span.end();
        analyzer.add_span(span);

        assert!(analyzer.get_trace(&trace_id).is_some());
        assert!(analyzer.get_trace("nonexistent").is_none());
    }

    #[test]
    fn test_trace_analyzer_service_dependencies() {
        let mut analyzer = TraceAnalyzer::new();

        let ctx = TraceContext::new();
        let mut parent = Span::new(ctx.clone(), "parent-op", "service-a");
        parent.end();
        analyzer.add_span(parent);

        let child_ctx = ctx.child();
        let mut child = Span::new(child_ctx, "child-op", "service-b");
        child.end();
        analyzer.add_span(child);

        let deps = analyzer.service_dependencies();
        assert!(deps
            .get("service-a")
            .map(|d| d.contains(&"service-b".to_string()))
            .unwrap_or(false));
    }

    #[test]
    fn test_load_balancer_algorithm_all_variants() {
        let algos = vec![
            LoadBalancerAlgorithm::RoundRobin,
            LoadBalancerAlgorithm::LeastConnections,
            LoadBalancerAlgorithm::Random,
            LoadBalancerAlgorithm::Passthrough,
            LoadBalancerAlgorithm::ConsistentHash,
        ];
        for algo in algos {
            let policy = TrafficPolicy::new("policy", "svc").with_load_balancer(algo);
            assert_eq!(policy.load_balancer.algorithm, algo);
        }
    }

    #[test]
    fn test_circuit_state_all_variants() {
        let states = vec![
            CircuitState::Closed,
            CircuitState::Open,
            CircuitState::HalfOpen,
        ];
        for state in &states {
            assert!(!format!("{:?}", state).is_empty());
        }
    }

    #[test]
    fn test_circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert!(config.failure_threshold > 0);
        assert!(config.success_threshold > 0);
    }

    #[test]
    fn test_service_mesh_type_all() {
        assert_eq!(ServiceMeshType::Istio.name(), "Istio");
        assert_eq!(ServiceMeshType::Linkerd.name(), "Linkerd");
        assert_eq!(ServiceMeshType::Consul.name(), "Consul Connect");
        assert_eq!(ServiceMeshType::Envoy.name(), "Envoy");
    }

    #[test]
    fn test_virtual_service_add_route() {
        let mut vs = VirtualService::new("test-vs").with_host("my-service");

        vs.add_route(HttpRoute {
            name: "test-route".to_string(),
            match_rules: vec![],
            route: vec![],
            timeout: None,
            retries: None,
        });

        assert_eq!(vs.http_routes.len(), 1);
    }

    #[test]
    fn test_trace_analyzer_default() {
        let analyzer = TraceAnalyzer::default();
        assert!(analyzer.spans.is_empty());
    }

    #[test]
    fn test_raft_state_all() {
        let states = vec![RaftState::Follower, RaftState::Candidate, RaftState::Leader];
        for state in &states {
            assert!(!format!("{:?}", state).is_empty());
        }
    }

    #[test]
    fn test_raft_node_become_follower() {
        let mut node = RaftNode::new("node-1");
        node.become_candidate();
        node.become_leader();
        node.become_follower(5);

        assert_eq!(node.state, RaftState::Follower);
        assert_eq!(node.current_term, 5);
        assert!(node.voted_for.is_none());
    }

    #[test]
    fn test_raft_debugger_default() {
        let debugger = RaftDebugger::default();
        assert!(debugger.nodes.is_empty());
    }

    #[test]
    fn test_tuning_recommendation() {
        let rec = TuningRecommendation {
            parameter: "timeout".to_string(),
            current_value: "30s".to_string(),
            recommended_value: "60s".to_string(),
            reason: "High latency".to_string(),
        };
        assert!(!rec.parameter.is_empty());
        assert!(rec.reason.contains("latency"));
    }
}
