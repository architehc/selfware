//! API Testing Framework
//!
//! HTTP testing capabilities:
//! - Request building and execution
//! - Request history and replay
//! - Environment variables
//! - Response validation
//! - Request chaining

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// HTTP method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            "HEAD" => Some(Self::Head),
            "OPTIONS" => Some(Self::Options),
            _ => None,
        }
    }

    pub fn has_body(&self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch)
    }
}

/// Content type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    Json,
    FormUrlEncoded,
    Multipart,
    Text,
    Xml,
    Binary,
    Custom(String),
}

impl ContentType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Json => "application/json",
            Self::FormUrlEncoded => "application/x-www-form-urlencoded",
            Self::Multipart => "multipart/form-data",
            Self::Text => "text/plain",
            Self::Xml => "application/xml",
            Self::Binary => "application/octet-stream",
            Self::Custom(s) => s,
        }
    }

    pub fn from_header(header: &str) -> Self {
        let lower = header.to_lowercase();
        if lower.contains("application/json") {
            Self::Json
        } else if lower.contains("x-www-form-urlencoded") {
            Self::FormUrlEncoded
        } else if lower.contains("multipart/form-data") {
            Self::Multipart
        } else if lower.contains("text/plain") {
            Self::Text
        } else if lower.contains("application/xml") || lower.contains("text/xml") {
            Self::Xml
        } else if lower.contains("octet-stream") {
            Self::Binary
        } else {
            Self::Custom(header.to_string())
        }
    }
}

/// HTTP request
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// Request ID
    pub id: String,
    /// Method
    pub method: HttpMethod,
    /// URL
    pub url: String,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Query parameters
    pub query_params: HashMap<String, String>,
    /// Body
    pub body: Option<String>,
    /// Content type
    pub content_type: Option<ContentType>,
    /// Timeout in milliseconds
    pub timeout_ms: u64,
    /// Name for organization
    pub name: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: &str) -> Self {
        let id = format!(
            "req_{:x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
        );
        Self {
            id,
            method,
            url: url.to_string(),
            headers: HashMap::new(),
            query_params: HashMap::new(),
            body: None,
            content_type: None,
            timeout_ms: 30000,
            name: None,
            tags: Vec::new(),
        }
    }

    /// GET request
    pub fn get(url: &str) -> Self {
        Self::new(HttpMethod::Get, url)
    }

    /// POST request
    pub fn post(url: &str) -> Self {
        Self::new(HttpMethod::Post, url)
    }

    /// PUT request
    pub fn put(url: &str) -> Self {
        Self::new(HttpMethod::Put, url)
    }

    /// DELETE request
    pub fn delete(url: &str) -> Self {
        Self::new(HttpMethod::Delete, url)
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_query(mut self, key: &str, value: &str) -> Self {
        self.query_params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_json_body(mut self, body: &str) -> Self {
        self.body = Some(body.to_string());
        self.content_type = Some(ContentType::Json);
        self.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        self
    }

    pub fn with_body(mut self, body: &str, content_type: ContentType) -> Self {
        self.body = Some(body.to_string());
        self.content_type = Some(content_type.clone());
        self.headers.insert(
            "Content-Type".to_string(),
            content_type.as_str().to_string(),
        );
        self
    }

    pub fn with_auth_bearer(mut self, token: &str) -> Self {
        self.headers
            .insert("Authorization".to_string(), format!("Bearer {}", token));
        self
    }

    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_string());
        self
    }

    /// Build full URL with query params
    pub fn full_url(&self) -> String {
        if self.query_params.is_empty() {
            self.url.clone()
        } else {
            let params: Vec<String> = self
                .query_params
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!("{}?{}", self.url, params.join("&"))
        }
    }
}

/// HTTP response
#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// Status code
    pub status: u16,
    /// Status text
    pub status_text: String,
    /// Response headers
    pub headers: HashMap<String, String>,
    /// Response body
    pub body: String,
    /// Response time in milliseconds
    pub time_ms: u64,
    /// Body size in bytes
    pub size_bytes: usize,
    /// Content type
    pub content_type: Option<ContentType>,
}

impl HttpResponse {
    pub fn new(status: u16, body: String) -> Self {
        let size_bytes = body.len();
        Self {
            status,
            status_text: Self::status_text(status),
            headers: HashMap::new(),
            body,
            time_ms: 0,
            size_bytes,
            content_type: None,
        }
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        // Detect content type from headers
        if let Some(ct) = headers.get("Content-Type").or(headers.get("content-type")) {
            self.content_type = Some(ContentType::from_header(ct));
        }
        self.headers = headers;
        self
    }

    pub fn with_time(mut self, time_ms: u64) -> Self {
        self.time_ms = time_ms;
        self
    }

    fn status_text(status: u16) -> String {
        match status {
            200 => "OK".to_string(),
            201 => "Created".to_string(),
            204 => "No Content".to_string(),
            301 => "Moved Permanently".to_string(),
            302 => "Found".to_string(),
            304 => "Not Modified".to_string(),
            400 => "Bad Request".to_string(),
            401 => "Unauthorized".to_string(),
            403 => "Forbidden".to_string(),
            404 => "Not Found".to_string(),
            405 => "Method Not Allowed".to_string(),
            422 => "Unprocessable Entity".to_string(),
            429 => "Too Many Requests".to_string(),
            500 => "Internal Server Error".to_string(),
            502 => "Bad Gateway".to_string(),
            503 => "Service Unavailable".to_string(),
            _ => "Unknown".to_string(),
        }
    }

    /// Is successful (2xx)?
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Is client error (4xx)?
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Is server error (5xx)?
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Parse body as JSON
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.body)
    }

    /// Extract value from JSON body using path (e.g., "data.user.id")
    pub fn json_path(&self, path: &str) -> Option<serde_json::Value> {
        let json: serde_json::Value = serde_json::from_str(&self.body).ok()?;
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &json;

        for part in parts {
            // Check for array index
            if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx)?;
            } else {
                current = current.get(part)?;
            }
        }

        Some(current.clone())
    }
}

/// A recorded request/response pair
#[derive(Debug, Clone)]
pub struct RequestRecord {
    /// Request
    pub request: HttpRequest,
    /// Response (if received)
    pub response: Option<HttpResponse>,
    /// Timestamp
    pub timestamp: u64,
    /// Was successful?
    pub success: bool,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl RequestRecord {
    pub fn new(request: HttpRequest) -> Self {
        Self {
            request,
            response: None,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            success: false,
            error: None,
        }
    }

    pub fn with_response(mut self, response: HttpResponse) -> Self {
        self.success = response.is_success();
        self.response = Some(response);
        self
    }

    pub fn with_error(mut self, error: &str) -> Self {
        self.success = false;
        self.error = Some(error.to_string());
        self
    }
}

/// Environment for variable substitution
#[derive(Debug, Clone)]
pub struct Environment {
    /// Environment name
    pub name: String,
    /// Variables
    pub variables: HashMap<String, String>,
    /// Secret variables (masked in logs)
    pub secrets: HashMap<String, String>,
    /// Is active?
    pub active: bool,
}

impl Environment {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            variables: HashMap::new(),
            secrets: HashMap::new(),
            active: false,
        }
    }

    pub fn with_variable(mut self, key: &str, value: &str) -> Self {
        self.variables.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_secret(mut self, key: &str, value: &str) -> Self {
        self.secrets.insert(key.to_string(), value.to_string());
        self
    }

    /// Get a variable value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.variables.get(key).or_else(|| self.secrets.get(key))
    }

    /// Set a variable
    pub fn set(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    /// Substitute variables in a string ({{variable}})
    pub fn substitute(&self, input: &str) -> String {
        let mut result = input.to_string();

        for (key, value) in &self.variables {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }

        for (key, value) in &self.secrets {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }

        result
    }
}

/// Response assertion
#[derive(Debug, Clone)]
pub struct Assertion {
    /// Assertion type
    pub assertion_type: AssertionType,
    /// Expected value
    pub expected: String,
    /// Actual value
    pub actual: Option<String>,
    /// Passed?
    pub passed: bool,
    /// Error message
    pub error: Option<String>,
}

/// Type of assertion
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertionType {
    /// Status code equals
    StatusEquals,
    /// Status code in range
    StatusInRange,
    /// Header exists
    HeaderExists,
    /// Header equals
    HeaderEquals,
    /// Body contains
    BodyContains,
    /// Body equals
    BodyEquals,
    /// JSON path equals
    JsonPathEquals,
    /// JSON path exists
    JsonPathExists,
    /// Response time less than
    ResponseTimeLessThan,
}

impl Assertion {
    pub fn status_equals(expected: u16) -> Self {
        Self {
            assertion_type: AssertionType::StatusEquals,
            expected: expected.to_string(),
            actual: None,
            passed: false,
            error: None,
        }
    }

    pub fn header_exists(header: &str) -> Self {
        Self {
            assertion_type: AssertionType::HeaderExists,
            expected: header.to_string(),
            actual: None,
            passed: false,
            error: None,
        }
    }

    pub fn body_contains(substring: &str) -> Self {
        Self {
            assertion_type: AssertionType::BodyContains,
            expected: substring.to_string(),
            actual: None,
            passed: false,
            error: None,
        }
    }

    pub fn json_path_equals(path: &str, expected: &str) -> Self {
        Self {
            assertion_type: AssertionType::JsonPathEquals,
            expected: format!("{}={}", path, expected),
            actual: None,
            passed: false,
            error: None,
        }
    }

    pub fn response_time_less_than(ms: u64) -> Self {
        Self {
            assertion_type: AssertionType::ResponseTimeLessThan,
            expected: ms.to_string(),
            actual: None,
            passed: false,
            error: None,
        }
    }

    /// Evaluate assertion against response
    pub fn evaluate(&mut self, response: &HttpResponse) -> bool {
        match &self.assertion_type {
            AssertionType::StatusEquals => {
                let expected: u16 = self.expected.parse().unwrap_or(0);
                self.actual = Some(response.status.to_string());
                self.passed = response.status == expected;
                if !self.passed {
                    self.error = Some(format!(
                        "Expected status {}, got {}",
                        expected, response.status
                    ));
                }
            }
            AssertionType::StatusInRange => {
                // Expected format: "200-299"
                let parts: Vec<&str> = self.expected.split('-').collect();
                if parts.len() == 2 {
                    let min: u16 = parts[0].parse().unwrap_or(0);
                    let max: u16 = parts[1].parse().unwrap_or(0);
                    self.actual = Some(response.status.to_string());
                    self.passed = response.status >= min && response.status <= max;
                    if !self.passed {
                        self.error = Some(format!(
                            "Expected status in range {}-{}, got {}",
                            min, max, response.status
                        ));
                    }
                }
            }
            AssertionType::HeaderExists => {
                self.passed = response.headers.contains_key(&self.expected)
                    || response.headers.contains_key(&self.expected.to_lowercase());
                if !self.passed {
                    self.error = Some(format!("Header '{}' not found", self.expected));
                }
            }
            AssertionType::HeaderEquals => {
                // Expected format: "Header-Name=value"
                if let Some((header, value)) = self.expected.split_once('=') {
                    let actual = response
                        .headers
                        .get(header)
                        .or_else(|| response.headers.get(&header.to_lowercase()));
                    self.actual = actual.cloned();
                    self.passed = actual.is_some_and(|v| v == value);
                    if !self.passed {
                        self.error = Some(format!(
                            "Header '{}' expected '{}', got {:?}",
                            header, value, actual
                        ));
                    }
                }
            }
            AssertionType::BodyContains => {
                self.passed = response.body.contains(&self.expected);
                if !self.passed {
                    self.error = Some(format!("Body does not contain '{}'", self.expected));
                }
            }
            AssertionType::BodyEquals => {
                self.actual = Some(response.body.clone());
                self.passed = response.body == self.expected;
                if !self.passed {
                    self.error = Some("Body does not match expected value".to_string());
                }
            }
            AssertionType::JsonPathEquals => {
                // Expected format: "path=value"
                if let Some((path, value)) = self.expected.split_once('=') {
                    let actual = response.json_path(path);
                    self.actual = actual.as_ref().map(|v| v.to_string());
                    self.passed = actual.is_some_and(|v| v.to_string().trim_matches('"') == value);
                    if !self.passed {
                        self.error = Some(format!(
                            "JSON path '{}' expected '{}', got {:?}",
                            path, value, self.actual
                        ));
                    }
                }
            }
            AssertionType::JsonPathExists => {
                let actual = response.json_path(&self.expected);
                self.passed = actual.is_some();
                if !self.passed {
                    self.error = Some(format!("JSON path '{}' does not exist", self.expected));
                }
            }
            AssertionType::ResponseTimeLessThan => {
                let max_ms: u64 = self.expected.parse().unwrap_or(0);
                self.actual = Some(response.time_ms.to_string());
                self.passed = response.time_ms < max_ms;
                if !self.passed {
                    self.error = Some(format!(
                        "Response time {}ms exceeds {}ms",
                        response.time_ms, max_ms
                    ));
                }
            }
        }
        self.passed
    }
}

/// Test case for API testing
#[derive(Debug, Clone)]
pub struct TestCase {
    /// Test name
    pub name: String,
    /// Request to execute
    pub request: HttpRequest,
    /// Assertions to validate
    pub assertions: Vec<Assertion>,
    /// Extract variables from response
    pub extractions: Vec<VariableExtraction>,
    /// Pre-request script (placeholder)
    pub pre_request_script: Option<String>,
    /// Post-response script (placeholder)
    pub post_response_script: Option<String>,
}

impl TestCase {
    pub fn new(name: &str, request: HttpRequest) -> Self {
        Self {
            name: name.to_string(),
            request,
            assertions: Vec::new(),
            extractions: Vec::new(),
            pre_request_script: None,
            post_response_script: None,
        }
    }

    pub fn with_assertion(mut self, assertion: Assertion) -> Self {
        self.assertions.push(assertion);
        self
    }

    pub fn with_extraction(mut self, extraction: VariableExtraction) -> Self {
        self.extractions.push(extraction);
        self
    }

    /// Run assertions against response
    pub fn run_assertions(&mut self, response: &HttpResponse) -> TestResult {
        let mut passed = 0;
        let mut failed = 0;

        for assertion in &mut self.assertions {
            if assertion.evaluate(response) {
                passed += 1;
            } else {
                failed += 1;
            }
        }

        TestResult {
            name: self.name.clone(),
            passed: failed == 0,
            assertions_passed: passed,
            assertions_failed: failed,
            assertions: self.assertions.clone(),
            response_time_ms: response.time_ms,
        }
    }

    /// Extract variables from response
    pub fn extract_variables(&self, response: &HttpResponse, env: &mut Environment) {
        for extraction in &self.extractions {
            if let Some(value) = extraction.extract(response) {
                env.set(&extraction.variable_name, &value);
            }
        }
    }
}

/// Variable extraction from response
#[derive(Debug, Clone)]
pub struct VariableExtraction {
    /// Variable name to set
    pub variable_name: String,
    /// Extraction source
    pub source: ExtractionSource,
    /// Path or pattern
    pub path: String,
}

/// Source for variable extraction
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionSource {
    /// Extract from JSON body
    JsonPath,
    /// Extract from header
    Header,
    /// Extract from body using regex
    BodyRegex,
    /// Extract response status
    Status,
}

impl VariableExtraction {
    pub fn json_path(variable: &str, path: &str) -> Self {
        Self {
            variable_name: variable.to_string(),
            source: ExtractionSource::JsonPath,
            path: path.to_string(),
        }
    }

    pub fn header(variable: &str, header_name: &str) -> Self {
        Self {
            variable_name: variable.to_string(),
            source: ExtractionSource::Header,
            path: header_name.to_string(),
        }
    }

    /// Extract value from response
    pub fn extract(&self, response: &HttpResponse) -> Option<String> {
        match &self.source {
            ExtractionSource::JsonPath => response.json_path(&self.path).map(|v| {
                if let Some(s) = v.as_str() {
                    s.to_string()
                } else {
                    v.to_string()
                }
            }),
            ExtractionSource::Header => response
                .headers
                .get(&self.path)
                .or_else(|| response.headers.get(&self.path.to_lowercase()))
                .cloned(),
            ExtractionSource::BodyRegex => {
                if let Ok(re) = regex::Regex::new(&self.path) {
                    re.captures(&response.body)
                        .and_then(|c| c.get(1).or(c.get(0)))
                        .map(|m| m.as_str().to_string())
                } else {
                    None
                }
            }
            ExtractionSource::Status => Some(response.status.to_string()),
        }
    }
}

/// Result of running a test case
#[derive(Debug, Clone)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// Overall passed?
    pub passed: bool,
    /// Number of assertions passed
    pub assertions_passed: usize,
    /// Number of assertions failed
    pub assertions_failed: usize,
    /// Individual assertion results
    pub assertions: Vec<Assertion>,
    /// Response time
    pub response_time_ms: u64,
}

/// Main API testing client
pub struct ApiTestClient {
    /// Request history
    history: RwLock<Vec<RequestRecord>>,
    /// Environments
    environments: RwLock<HashMap<String, Environment>>,
    /// Active environment name
    active_environment: RwLock<Option<String>>,
    /// Maximum history size
    max_history: usize,
}

impl ApiTestClient {
    pub fn new() -> Self {
        Self {
            history: RwLock::new(Vec::new()),
            environments: RwLock::new(HashMap::new()),
            active_environment: RwLock::new(None),
            max_history: 1000,
        }
    }

    /// Add an environment
    pub fn add_environment(&self, env: Environment) {
        if let Ok(mut envs) = self.environments.write() {
            let name = env.name.clone();
            envs.insert(name.clone(), env);
            // Set as active if first
            if let Ok(mut active) = self.active_environment.write() {
                if active.is_none() {
                    *active = Some(name);
                }
            }
        }
    }

    /// Set active environment
    pub fn set_active_environment(&self, name: &str) -> bool {
        if let Ok(envs) = self.environments.read() {
            if envs.contains_key(name) {
                if let Ok(mut active) = self.active_environment.write() {
                    *active = Some(name.to_string());
                    return true;
                }
            }
        }
        false
    }

    /// Get active environment
    pub fn get_active_environment(&self) -> Option<Environment> {
        if let Ok(active) = self.active_environment.read() {
            if let Some(name) = active.as_ref() {
                if let Ok(envs) = self.environments.read() {
                    return envs.get(name).cloned();
                }
            }
        }
        None
    }

    /// Substitute variables in request
    pub fn substitute_request(&self, mut request: HttpRequest) -> HttpRequest {
        if let Some(env) = self.get_active_environment() {
            request.url = env.substitute(&request.url);
            if let Some(body) = &request.body {
                request.body = Some(env.substitute(body));
            }
            for (_, value) in request.headers.iter_mut() {
                *value = env.substitute(value);
            }
            for (_, value) in request.query_params.iter_mut() {
                *value = env.substitute(value);
            }
        }
        request
    }

    /// Record a request (mock execution for testing)
    pub fn record_request(&self, request: HttpRequest, response: Option<HttpResponse>) {
        let mut record = RequestRecord::new(request);
        if let Some(resp) = response {
            record = record.with_response(resp);
        }

        if let Ok(mut history) = self.history.write() {
            history.push(record);
            if history.len() > self.max_history {
                history.drain(0..self.max_history / 2);
            }
        }
    }

    /// Get request history
    pub fn get_history(&self) -> Vec<RequestRecord> {
        self.history.read().map(|h| h.clone()).unwrap_or_default()
    }

    /// Get history by tag
    pub fn get_history_by_tag(&self, tag: &str) -> Vec<RequestRecord> {
        self.history
            .read()
            .map(|h| {
                h.iter()
                    .filter(|r| r.request.tags.contains(&tag.to_string()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Clear history
    pub fn clear_history(&self) {
        if let Ok(mut history) = self.history.write() {
            history.clear();
        }
    }

    /// Run a test case
    pub fn run_test(&self, test: &mut TestCase, response: HttpResponse) -> TestResult {
        // Substitute variables in request
        test.request = self.substitute_request(test.request.clone());

        // Record
        self.record_request(test.request.clone(), Some(response.clone()));

        // Extract variables
        if let Some(mut env) = self.get_active_environment() {
            test.extract_variables(&response, &mut env);
            // Update environment
            if let Ok(mut envs) = self.environments.write() {
                envs.insert(env.name.clone(), env);
            }
        }

        // Run assertions
        test.run_assertions(&response)
    }

    /// Get statistics
    pub fn get_stats(&self) -> ClientStats {
        let history = self.history.read().map(|h| h.clone()).unwrap_or_default();
        let total = history.len();
        let successful = history.iter().filter(|r| r.success).count();

        let avg_time: f64 = if total > 0 {
            history
                .iter()
                .filter_map(|r| r.response.as_ref().map(|resp| resp.time_ms as f64))
                .sum::<f64>()
                / total as f64
        } else {
            0.0
        };

        ClientStats {
            total_requests: total,
            successful_requests: successful,
            failed_requests: total - successful,
            avg_response_time_ms: avg_time,
            environments_count: self.environments.read().map(|e| e.len()).unwrap_or(0),
        }
    }
}

impl Default for ApiTestClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Client statistics
#[derive(Debug, Clone)]
pub struct ClientStats {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub avg_response_time_ms: f64,
    pub environments_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_as_str() {
        assert_eq!(HttpMethod::Get.as_str(), "GET");
        assert_eq!(HttpMethod::Post.as_str(), "POST");
        assert_eq!(HttpMethod::Delete.as_str(), "DELETE");
    }

    #[test]
    fn test_http_method_from_str() {
        assert_eq!(HttpMethod::from_str("GET"), Some(HttpMethod::Get));
        assert_eq!(HttpMethod::from_str("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::from_str("invalid"), None);
    }

    #[test]
    fn test_http_method_has_body() {
        assert!(!HttpMethod::Get.has_body());
        assert!(HttpMethod::Post.has_body());
        assert!(HttpMethod::Put.has_body());
    }

    #[test]
    fn test_content_type_as_str() {
        assert_eq!(ContentType::Json.as_str(), "application/json");
        assert_eq!(
            ContentType::FormUrlEncoded.as_str(),
            "application/x-www-form-urlencoded"
        );
    }

    #[test]
    fn test_content_type_from_header() {
        assert_eq!(
            ContentType::from_header("application/json"),
            ContentType::Json
        );
        assert_eq!(ContentType::from_header("text/plain"), ContentType::Text);
    }

    #[test]
    fn test_http_request_new() {
        let req = HttpRequest::new(HttpMethod::Get, "https://api.example.com");
        assert!(!req.id.is_empty());
        assert_eq!(req.method, HttpMethod::Get);
    }

    #[test]
    fn test_http_request_builders() {
        let req = HttpRequest::get("https://api.example.com")
            .with_header("Accept", "application/json")
            .with_query("page", "1")
            .with_timeout(5000);

        assert_eq!(
            req.headers.get("Accept"),
            Some(&"application/json".to_string())
        );
        assert_eq!(req.query_params.get("page"), Some(&"1".to_string()));
        assert_eq!(req.timeout_ms, 5000);
    }

    #[test]
    fn test_http_request_full_url() {
        let req = HttpRequest::get("https://api.example.com/users")
            .with_query("page", "1")
            .with_query("limit", "10");

        let url = req.full_url();
        assert!(url.contains("page=1"));
        assert!(url.contains("limit=10"));
    }

    #[test]
    fn test_http_request_with_json_body() {
        let req =
            HttpRequest::post("https://api.example.com").with_json_body(r#"{"name": "test"}"#);

        assert!(req.body.is_some());
        assert_eq!(req.content_type, Some(ContentType::Json));
    }

    #[test]
    fn test_http_response_new() {
        let resp = HttpResponse::new(200, "OK".to_string());
        assert_eq!(resp.status, 200);
        assert_eq!(resp.status_text, "OK");
    }

    #[test]
    fn test_http_response_status_checks() {
        let success = HttpResponse::new(200, "".to_string());
        assert!(success.is_success());
        assert!(!success.is_client_error());

        let client_error = HttpResponse::new(404, "".to_string());
        assert!(!client_error.is_success());
        assert!(client_error.is_client_error());

        let server_error = HttpResponse::new(500, "".to_string());
        assert!(server_error.is_server_error());
    }

    #[test]
    fn test_http_response_json_path() {
        let resp = HttpResponse::new(200, r#"{"data": {"user": {"id": 123}}}"#.to_string());
        let value = resp.json_path("data.user.id");
        assert!(value.is_some());
        assert_eq!(value.unwrap().as_i64(), Some(123));
    }

    #[test]
    fn test_environment_new() {
        let env = Environment::new("test");
        assert_eq!(env.name, "test");
    }

    #[test]
    fn test_environment_substitute() {
        let env = Environment::new("test")
            .with_variable("base_url", "https://api.example.com")
            .with_variable("token", "abc123");

        let result = env.substitute("{{base_url}}/users?token={{token}}");
        assert_eq!(result, "https://api.example.com/users?token=abc123");
    }

    #[test]
    fn test_assertion_status_equals() {
        let mut assertion = Assertion::status_equals(200);
        let response = HttpResponse::new(200, "".to_string());
        assert!(assertion.evaluate(&response));
        assert!(assertion.passed);
    }

    #[test]
    fn test_assertion_status_equals_fail() {
        let mut assertion = Assertion::status_equals(200);
        let response = HttpResponse::new(404, "".to_string());
        assert!(!assertion.evaluate(&response));
        assert!(assertion.error.is_some());
    }

    #[test]
    fn test_assertion_body_contains() {
        let mut assertion = Assertion::body_contains("success");
        let response = HttpResponse::new(200, "Operation was a success!".to_string());
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_json_path_equals() {
        let mut assertion = Assertion::json_path_equals("data.id", "123");
        let response = HttpResponse::new(200, r#"{"data": {"id": "123"}}"#.to_string());
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_response_time() {
        let mut assertion = Assertion::response_time_less_than(1000);
        let response = HttpResponse::new(200, "".to_string()).with_time(500);
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_test_case_new() {
        let request = HttpRequest::get("https://api.example.com");
        let test = TestCase::new("Get Users", request);
        assert_eq!(test.name, "Get Users");
    }

    #[test]
    fn test_test_case_run_assertions() {
        let request = HttpRequest::get("https://api.example.com");
        let mut test = TestCase::new("Test", request)
            .with_assertion(Assertion::status_equals(200))
            .with_assertion(Assertion::body_contains("hello"));

        let response = HttpResponse::new(200, "hello world".to_string());
        let result = test.run_assertions(&response);

        assert!(result.passed);
        assert_eq!(result.assertions_passed, 2);
    }

    #[test]
    fn test_variable_extraction_json_path() {
        let extraction = VariableExtraction::json_path("user_id", "data.user.id");
        let response =
            HttpResponse::new(200, r#"{"data": {"user": {"id": "abc123"}}}"#.to_string());
        let value = extraction.extract(&response);
        assert_eq!(value, Some("abc123".to_string()));
    }

    #[test]
    fn test_variable_extraction_header() {
        let extraction = VariableExtraction::header("token", "X-Auth-Token");
        let mut headers = HashMap::new();
        headers.insert("X-Auth-Token".to_string(), "secret123".to_string());
        let response = HttpResponse::new(200, "".to_string()).with_headers(headers);
        let value = extraction.extract(&response);
        assert_eq!(value, Some("secret123".to_string()));
    }

    #[test]
    fn test_api_test_client_new() {
        let client = ApiTestClient::new();
        let stats = client.get_stats();
        assert_eq!(stats.total_requests, 0);
    }

    #[test]
    fn test_api_test_client_environment() {
        let client = ApiTestClient::new();
        let env = Environment::new("dev").with_variable("base_url", "https://dev.example.com");
        client.add_environment(env);

        let active = client.get_active_environment();
        assert!(active.is_some());
        assert_eq!(active.unwrap().name, "dev");
    }

    #[test]
    fn test_api_test_client_record() {
        let client = ApiTestClient::new();
        let request = HttpRequest::get("https://api.example.com");
        let response = HttpResponse::new(200, "OK".to_string());

        client.record_request(request, Some(response));

        let history = client.get_history();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_api_test_client_substitute() {
        let client = ApiTestClient::new();
        let env = Environment::new("test").with_variable("host", "api.example.com");
        client.add_environment(env);

        let request = HttpRequest::get("https://{{host}}/users");
        let substituted = client.substitute_request(request);

        assert_eq!(substituted.url, "https://api.example.com/users");
    }

    #[test]
    fn test_request_record_new() {
        let request = HttpRequest::get("https://api.example.com");
        let record = RequestRecord::new(request);
        assert!(!record.success);
    }

    #[test]
    fn test_request_record_with_response() {
        let request = HttpRequest::get("https://api.example.com");
        let response = HttpResponse::new(200, "".to_string());
        let record = RequestRecord::new(request).with_response(response);
        assert!(record.success);
    }

    #[test]
    fn test_test_result() {
        let result = TestResult {
            name: "Test".to_string(),
            passed: true,
            assertions_passed: 2,
            assertions_failed: 0,
            assertions: Vec::new(),
            response_time_ms: 100,
        };
        assert!(result.passed);
    }

    #[test]
    fn test_client_stats() {
        let stats = ClientStats {
            total_requests: 10,
            successful_requests: 8,
            failed_requests: 2,
            avg_response_time_ms: 150.0,
            environments_count: 2,
        };
        assert_eq!(
            stats.successful_requests + stats.failed_requests,
            stats.total_requests
        );
    }

    #[test]
    fn test_http_method_all_variants() {
        assert_eq!(HttpMethod::Put.as_str(), "PUT");
        assert_eq!(HttpMethod::Patch.as_str(), "PATCH");
        assert_eq!(HttpMethod::Head.as_str(), "HEAD");
        assert_eq!(HttpMethod::Options.as_str(), "OPTIONS");
    }

    #[test]
    fn test_http_method_from_str_all() {
        assert_eq!(HttpMethod::from_str("PUT"), Some(HttpMethod::Put));
        assert_eq!(HttpMethod::from_str("patch"), Some(HttpMethod::Patch));
        assert_eq!(HttpMethod::from_str("HEAD"), Some(HttpMethod::Head));
        assert_eq!(HttpMethod::from_str("OPTIONS"), Some(HttpMethod::Options));
        assert_eq!(HttpMethod::from_str("DELETE"), Some(HttpMethod::Delete));
    }

    #[test]
    fn test_http_method_has_body_all() {
        assert!(HttpMethod::Patch.has_body());
        assert!(!HttpMethod::Delete.has_body());
        assert!(!HttpMethod::Head.has_body());
        assert!(!HttpMethod::Options.has_body());
    }

    #[test]
    fn test_http_method_debug() {
        let method = HttpMethod::Get;
        let debug = format!("{:?}", method);
        assert!(debug.contains("Get"));
    }

    #[test]
    fn test_http_method_clone() {
        let method = HttpMethod::Post;
        let cloned = method.clone();
        assert_eq!(method, cloned);
    }

    #[test]
    fn test_http_method_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(HttpMethod::Get);
        set.insert(HttpMethod::Post);
        set.insert(HttpMethod::Get); // Duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_content_type_all_variants() {
        assert_eq!(ContentType::Multipart.as_str(), "multipart/form-data");
        assert_eq!(ContentType::Xml.as_str(), "application/xml");
        assert_eq!(ContentType::Binary.as_str(), "application/octet-stream");

        let custom = ContentType::Custom("custom/type".to_string());
        assert_eq!(custom.as_str(), "custom/type");
    }

    #[test]
    fn test_content_type_from_header_all() {
        assert_eq!(
            ContentType::from_header("multipart/form-data"),
            ContentType::Multipart
        );
        assert_eq!(
            ContentType::from_header("application/xml"),
            ContentType::Xml
        );
        assert_eq!(ContentType::from_header("text/xml"), ContentType::Xml);
        assert_eq!(
            ContentType::from_header("application/octet-stream"),
            ContentType::Binary
        );

        let unknown = ContentType::from_header("application/unknown");
        assert!(matches!(unknown, ContentType::Custom(_)));
    }

    #[test]
    fn test_content_type_clone() {
        let ct = ContentType::Json;
        let cloned = ct.clone();
        assert_eq!(ct, cloned);
    }

    #[test]
    fn test_content_type_debug() {
        let ct = ContentType::Text;
        let debug = format!("{:?}", ct);
        assert!(debug.contains("Text"));
    }

    #[test]
    fn test_http_request_put() {
        let req = HttpRequest::put("https://api.example.com/users/1");
        assert_eq!(req.method, HttpMethod::Put);
    }

    #[test]
    fn test_http_request_delete() {
        let req = HttpRequest::delete("https://api.example.com/users/1");
        assert_eq!(req.method, HttpMethod::Delete);
    }

    #[test]
    fn test_http_request_with_name() {
        let req = HttpRequest::get("https://api.example.com").with_name("Get all users");
        assert_eq!(req.name, Some("Get all users".to_string()));
    }

    #[test]
    fn test_http_request_with_tag() {
        let req = HttpRequest::get("https://api.example.com")
            .with_tag("user")
            .with_tag("api");
        assert_eq!(req.tags.len(), 2);
        assert!(req.tags.contains(&"user".to_string()));
    }

    #[test]
    fn test_http_request_with_auth_bearer() {
        let req = HttpRequest::get("https://api.example.com").with_auth_bearer("my_token_123");
        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer my_token_123".to_string())
        );
    }

    #[test]
    fn test_http_request_with_body() {
        let req = HttpRequest::post("https://api.example.com")
            .with_body("key=value", ContentType::FormUrlEncoded);
        assert_eq!(req.body, Some("key=value".to_string()));
        assert_eq!(req.content_type, Some(ContentType::FormUrlEncoded));
    }

    #[test]
    fn test_http_request_full_url_no_params() {
        let req = HttpRequest::get("https://api.example.com/users");
        assert_eq!(req.full_url(), "https://api.example.com/users");
    }

    #[test]
    fn test_http_request_clone() {
        let req =
            HttpRequest::get("https://api.example.com").with_header("Accept", "application/json");
        let cloned = req.clone();
        assert_eq!(req.url, cloned.url);
        assert_eq!(req.method, cloned.method);
    }

    #[test]
    fn test_http_response_all_status_texts() {
        assert_eq!(
            HttpResponse::new(201, "".to_string()).status_text,
            "Created"
        );
        assert_eq!(
            HttpResponse::new(204, "".to_string()).status_text,
            "No Content"
        );
        assert_eq!(
            HttpResponse::new(301, "".to_string()).status_text,
            "Moved Permanently"
        );
        assert_eq!(HttpResponse::new(302, "".to_string()).status_text, "Found");
        assert_eq!(
            HttpResponse::new(304, "".to_string()).status_text,
            "Not Modified"
        );
        assert_eq!(
            HttpResponse::new(400, "".to_string()).status_text,
            "Bad Request"
        );
        assert_eq!(
            HttpResponse::new(401, "".to_string()).status_text,
            "Unauthorized"
        );
        assert_eq!(
            HttpResponse::new(403, "".to_string()).status_text,
            "Forbidden"
        );
        assert_eq!(
            HttpResponse::new(404, "".to_string()).status_text,
            "Not Found"
        );
        assert_eq!(
            HttpResponse::new(405, "".to_string()).status_text,
            "Method Not Allowed"
        );
        assert_eq!(
            HttpResponse::new(422, "".to_string()).status_text,
            "Unprocessable Entity"
        );
        assert_eq!(
            HttpResponse::new(429, "".to_string()).status_text,
            "Too Many Requests"
        );
        assert_eq!(
            HttpResponse::new(500, "".to_string()).status_text,
            "Internal Server Error"
        );
        assert_eq!(
            HttpResponse::new(502, "".to_string()).status_text,
            "Bad Gateway"
        );
        assert_eq!(
            HttpResponse::new(503, "".to_string()).status_text,
            "Service Unavailable"
        );
        assert_eq!(
            HttpResponse::new(999, "".to_string()).status_text,
            "Unknown"
        );
    }

    #[test]
    fn test_http_response_with_headers() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        let resp = HttpResponse::new(200, "{}".to_string()).with_headers(headers);
        assert_eq!(resp.content_type, Some(ContentType::Json));
    }

    #[test]
    fn test_http_response_json_path_array() {
        let resp = HttpResponse::new(200, r#"{"items": ["a", "b", "c"]}"#.to_string());
        let value = resp.json_path("items.0");
        assert!(value.is_some());
        assert_eq!(value.unwrap().as_str(), Some("a"));
    }

    #[test]
    fn test_http_response_json_parse() {
        let resp = HttpResponse::new(200, r#"{"id": 1, "name": "test"}"#.to_string());
        let json: Result<serde_json::Value, _> = resp.json();
        assert!(json.is_ok());
    }

    #[test]
    fn test_http_response_clone() {
        let resp = HttpResponse::new(200, "body".to_string()).with_time(100);
        let cloned = resp.clone();
        assert_eq!(resp.status, cloned.status);
        assert_eq!(resp.time_ms, cloned.time_ms);
    }

    #[test]
    fn test_environment_set_variable() {
        let mut env = Environment::new("test");
        env.set("key", "value");
        assert_eq!(env.variables.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_environment_get_variable() {
        let env = Environment::new("test").with_variable("key", "value");
        assert_eq!(env.get("key"), Some(&"value".to_string()));
    }

    #[test]
    fn test_environment_get_secret() {
        let env = Environment::new("test").with_secret("api_key", "secret123");
        assert_eq!(env.get("api_key"), Some(&"secret123".to_string()));
    }

    #[test]
    fn test_environment_clone() {
        let env = Environment::new("test").with_variable("key", "value");
        let cloned = env.clone();
        assert_eq!(env.name, cloned.name);
    }

    #[test]
    fn test_environment_substitute_missing() {
        let env = Environment::new("test");
        let result = env.substitute("{{missing}}");
        assert_eq!(result, "{{missing}}"); // Unchanged
    }

    #[test]
    fn test_assertion_header_equals() {
        let mut assertion = Assertion {
            assertion_type: AssertionType::HeaderEquals,
            expected: "X-Custom=value".to_string(),
            actual: None,
            passed: false,
            error: None,
        };
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        let response = HttpResponse::new(200, "".to_string()).with_headers(headers);
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_header_exists() {
        let mut assertion = Assertion::header_exists("X-Custom");
        let mut headers = HashMap::new();
        headers.insert("X-Custom".to_string(), "value".to_string());
        let response = HttpResponse::new(200, "".to_string()).with_headers(headers);
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_body_equals() {
        let mut assertion = Assertion {
            assertion_type: AssertionType::BodyEquals,
            expected: "expected body".to_string(),
            actual: None,
            passed: false,
            error: None,
        };
        let response = HttpResponse::new(200, "expected body".to_string());
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_json_path_exists() {
        let mut assertion = Assertion {
            assertion_type: AssertionType::JsonPathExists,
            expected: "data.id".to_string(),
            actual: None,
            passed: false,
            error: None,
        };
        let response = HttpResponse::new(200, r#"{"data": {"id": 123}}"#.to_string());
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_assertion_clone() {
        let assertion = Assertion::status_equals(200);
        let cloned = assertion.clone();
        assert_eq!(assertion.assertion_type, cloned.assertion_type);
    }

    #[test]
    fn test_test_case_with_extraction() {
        let request = HttpRequest::get("https://api.example.com");
        let test = TestCase::new("Test", request)
            .with_extraction(VariableExtraction::json_path("user_id", "data.id"));
        assert_eq!(test.extractions.len(), 1);
    }

    #[test]
    fn test_test_case_extract_variables() {
        let request = HttpRequest::get("https://api.example.com");
        let test = TestCase::new("Test", request)
            .with_extraction(VariableExtraction::json_path("user_id", "data.id"));

        let mut env = Environment::new("test");
        let response = HttpResponse::new(200, r#"{"data": {"id": "abc123"}}"#.to_string());

        test.extract_variables(&response, &mut env);
        assert_eq!(env.variables.get("user_id"), Some(&"abc123".to_string()));
    }

    #[test]
    fn test_variable_extraction_body_regex() {
        let extraction = VariableExtraction {
            variable_name: "token".to_string(),
            source: ExtractionSource::BodyRegex,
            path: r#"token=([a-z0-9]+)"#.to_string(),
        };
        let response = HttpResponse::new(200, "token=abc123&other=value".to_string());
        let value = extraction.extract(&response);
        assert_eq!(value, Some("abc123".to_string()));
    }

    #[test]
    fn test_variable_extraction_status() {
        let extraction = VariableExtraction {
            variable_name: "status".to_string(),
            source: ExtractionSource::Status,
            path: "".to_string(),
        };
        let response = HttpResponse::new(201, "".to_string());
        let value = extraction.extract(&response);
        assert_eq!(value, Some("201".to_string()));
    }

    #[test]
    fn test_variable_extraction_header_lowercase() {
        let extraction = VariableExtraction::header("token", "x-auth-token");
        let mut headers = HashMap::new();
        headers.insert("x-auth-token".to_string(), "secret".to_string());
        let response = HttpResponse::new(200, "".to_string()).with_headers(headers);
        let value = extraction.extract(&response);
        assert_eq!(value, Some("secret".to_string()));
    }

    #[test]
    fn test_api_test_client_set_active_environment() {
        let client = ApiTestClient::new();

        let env1 = Environment::new("dev");
        let env2 = Environment::new("prod");

        client.add_environment(env1);
        client.add_environment(env2);

        assert!(client.set_active_environment("prod"));
        assert_eq!(client.get_active_environment().unwrap().name, "prod");
    }

    #[test]
    fn test_api_test_client_set_active_nonexistent() {
        let client = ApiTestClient::new();
        assert!(!client.set_active_environment("nonexistent"));
    }

    #[test]
    fn test_api_test_client_get_history_by_tag() {
        let client = ApiTestClient::new();

        let req1 = HttpRequest::get("https://api.example.com").with_tag("user");
        let req2 = HttpRequest::get("https://api.example.com").with_tag("admin");

        client.record_request(req1, Some(HttpResponse::new(200, "".to_string())));
        client.record_request(req2, Some(HttpResponse::new(200, "".to_string())));

        let user_history = client.get_history_by_tag("user");
        assert_eq!(user_history.len(), 1);
    }

    #[test]
    fn test_api_test_client_clear_history() {
        let client = ApiTestClient::new();

        let req = HttpRequest::get("https://api.example.com");
        client.record_request(req, Some(HttpResponse::new(200, "".to_string())));

        assert_eq!(client.get_history().len(), 1);
        client.clear_history();
        assert_eq!(client.get_history().len(), 0);
    }

    #[test]
    fn test_api_test_client_stats_with_data() {
        let client = ApiTestClient::new();

        let req = HttpRequest::get("https://api.example.com");
        let resp = HttpResponse::new(200, "".to_string()).with_time(100);
        client.record_request(req, Some(resp));

        let stats = client.get_stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.successful_requests, 1);
    }

    #[test]
    fn test_api_test_client_default() {
        let client = ApiTestClient::default();
        assert_eq!(client.get_stats().total_requests, 0);
    }

    #[test]
    fn test_request_record_with_error() {
        let request = HttpRequest::get("https://api.example.com");
        let record = RequestRecord::new(request).with_error("Connection refused");
        assert!(!record.success);
        assert_eq!(record.error, Some("Connection refused".to_string()));
    }

    #[test]
    fn test_test_result_clone() {
        let result = TestResult {
            name: "Test".to_string(),
            passed: true,
            assertions_passed: 2,
            assertions_failed: 0,
            assertions: Vec::new(),
            response_time_ms: 100,
        };
        let cloned = result.clone();
        assert_eq!(result.name, cloned.name);
    }

    #[test]
    fn test_client_stats_clone() {
        let stats = ClientStats {
            total_requests: 10,
            successful_requests: 8,
            failed_requests: 2,
            avg_response_time_ms: 150.0,
            environments_count: 2,
        };
        let cloned = stats.clone();
        assert_eq!(stats.total_requests, cloned.total_requests);
    }

    #[test]
    fn test_http_response_is_redirect() {
        // Test 3xx range
        let resp301 = HttpResponse::new(301, "".to_string());
        let resp302 = HttpResponse::new(302, "".to_string());
        assert!(!resp301.is_success());
        assert!(!resp302.is_client_error());
        assert!(!resp302.is_server_error());
    }

    #[test]
    fn test_assertion_status_in_range() {
        let mut assertion = Assertion {
            assertion_type: AssertionType::StatusInRange,
            expected: "200-299".to_string(),
            actual: None,
            passed: false,
            error: None,
        };
        let response = HttpResponse::new(201, "".to_string());
        assert!(assertion.evaluate(&response));
    }

    #[test]
    fn test_extraction_source_debug() {
        let source = ExtractionSource::JsonPath;
        let debug = format!("{:?}", source);
        assert!(debug.contains("JsonPath"));
    }

    #[test]
    fn test_api_test_client_substitute_body() {
        let client = ApiTestClient::new();
        let env = Environment::new("test").with_variable("user_id", "123");
        client.add_environment(env);

        let request =
            HttpRequest::post("https://api.example.com").with_json_body(r#"{"id": "{{user_id}}"}"#);
        let substituted = client.substitute_request(request);

        assert!(substituted.body.unwrap().contains("123"));
    }

    #[test]
    fn test_api_test_client_run_test() {
        let client = ApiTestClient::new();
        let env = Environment::new("test");
        client.add_environment(env);

        let request = HttpRequest::get("https://api.example.com");
        let mut test = TestCase::new("Test", request).with_assertion(Assertion::status_equals(200));

        let response = HttpResponse::new(200, "".to_string());
        let result = client.run_test(&mut test, response);

        assert!(result.passed);
    }
}
