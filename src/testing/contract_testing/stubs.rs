//! Service Virtualization (WireMock-style)

use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
