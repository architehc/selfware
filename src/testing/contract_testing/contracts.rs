//! Consumer-Driven Contracts (Pact-style)

use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
