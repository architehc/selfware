//! API Compatibility Checking

use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;

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
