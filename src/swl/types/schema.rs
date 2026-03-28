//! State Schema Definitions
//!
//! Defines the structure of workflow state with typed fields.

use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// State schema definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSchema {
    #[serde(default)]
    pub fields: Vec<StateField>,
}

impl Default for StateSchema {
    fn default() -> Self {
        Self { fields: vec![] }
    }
}

impl StateSchema {
    /// Create a new empty schema
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a field to the schema
    pub fn add_field(&mut self, field: StateField) {
        self.fields.push(field);
    }

    /// Get a field by name
    pub fn get_field(&self, name: &str) -> Option<&StateField> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// Check if a field exists
    pub fn has_field(&self, name: &str) -> bool {
        self.get_field(name).is_some()
    }

    /// Get all field names
    pub fn field_names(&self) -> Vec<&str> {
        self.fields.iter().map(|f| f.name.as_str()).collect()
    }

    /// Validate that all required fields are present in a value
    pub fn validate_required(&self, value: &serde_yaml::Value) -> crate::errors::Result<()> {
        if let serde_yaml::Value::Mapping(map) = value {
            for field in &self.fields {
                if field.default.is_none() {
                    let key = serde_yaml::Value::String(field.name.clone());
                    if !map.contains_key(&key) {
                        return Err(crate::errors::SelfwareError::Internal(format!(
                            "Required field '{}' is missing",
                            field.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Apply default values to a state object
    pub fn apply_defaults(&self, value: &mut serde_yaml::Value) {
        if let serde_yaml::Value::Mapping(map) = value {
            for field in &self.fields {
                let key = serde_yaml::Value::String(field.name.clone());
                if !map.contains_key(&key) {
                    if let Some(ref default) = field.default {
                        map.insert(key, default.clone());
                    }
                }
            }
        }
    }
}

/// Individual state field definition
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StateField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub default: Option<serde_yaml::Value>,
    #[serde(default)]
    pub description: Option<String>,
}

impl<'de> Deserialize<'de> for StateField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawStateField {
            name: String,
            #[serde(rename = "type")]
            field_type: FieldType,
            #[serde(default)]
            element_type: Option<FieldType>,
            #[serde(default)]
            default: Option<serde_yaml::Value>,
            #[serde(default)]
            description: Option<String>,
        }

        let raw = RawStateField::deserialize(deserializer)?;
        let field_type = match (raw.field_type, raw.element_type) {
            (FieldType::Array(_), Some(element_type)) => FieldType::Array(Box::new(element_type)),
            (FieldType::Array(_), None) => FieldType::Array(Box::new(FieldType::String)),
            (FieldType::Object(_), _) => FieldType::Object(HashMap::new()),
            (field_type, _) => field_type,
        };

        Ok(Self {
            name: raw.name,
            field_type,
            default: raw.default,
            description: raw.description,
        })
    }
}

/// Field type enumeration
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// String value
    String,
    /// Integer value
    Integer,
    /// Floating point value
    Float,
    /// Boolean value
    Boolean,
    /// Array of values with element type
    Array(Box<FieldType>),
    /// Object with named fields
    Object(HashMap<String, FieldType>),
    /// Reference to an agent
    AgentRef,
    /// Reference to a tool
    ToolRef,
}

impl<'de> Deserialize<'de> for FieldType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;

        match raw.as_str() {
            "string" => Ok(Self::String),
            "integer" => Ok(Self::Integer),
            "float" => Ok(Self::Float),
            "boolean" => Ok(Self::Boolean),
            "array" => Ok(Self::Array(Box::new(Self::String))),
            "object" => Ok(Self::Object(HashMap::new())),
            "agent_ref" => Ok(Self::AgentRef),
            "tool_ref" => Ok(Self::ToolRef),
            other => Err(D::Error::unknown_variant(
                other,
                &[
                    "string",
                    "integer",
                    "float",
                    "boolean",
                    "array",
                    "object",
                    "agent_ref",
                    "tool_ref",
                ],
            )),
        }
    }
}

impl Default for FieldType {
    fn default() -> Self {
        Self::String
    }
}

impl FieldType {
    /// Get the type name as a string
    pub fn type_name(&self) -> &'static str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Float => "float",
            FieldType::Boolean => "boolean",
            FieldType::Array(_) => "array",
            FieldType::Object(_) => "object",
            FieldType::AgentRef => "agent_ref",
            FieldType::ToolRef => "tool_ref",
        }
    }

    /// Check if this type is a scalar (not a collection)
    pub fn is_scalar(&self) -> bool {
        matches!(
            self,
            FieldType::String | FieldType::Integer | FieldType::Float | FieldType::Boolean
        )
    }

    /// Check if this type is a collection (array or object)
    pub fn is_collection(&self) -> bool {
        matches!(self, FieldType::Array(_) | FieldType::Object(_))
    }

    /// Check if this is a reference type
    pub fn is_reference(&self) -> bool {
        matches!(self, FieldType::AgentRef | FieldType::ToolRef)
    }
}

/// Schema builder for ergonomic schema construction
#[derive(Debug, Default)]
pub struct SchemaBuilder {
    fields: Vec<StateField>,
}

impl SchemaBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a string field
    pub fn string(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });
        self
    }

    /// Add a string field with default
    pub fn string_with_default(mut self, name: &str, default: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::String,
            default: Some(serde_yaml::Value::String(default.to_string())),
            description: None,
        });
        self
    }

    /// Add an integer field
    pub fn integer(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Integer,
            default: None,
            description: None,
        });
        self
    }

    /// Add an integer field with default
    pub fn integer_with_default(mut self, name: &str, default: i64) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Integer,
            default: Some(serde_yaml::Value::Number(default.into())),
            description: None,
        });
        self
    }

    /// Add a float field
    pub fn float(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Float,
            default: None,
            description: None,
        });
        self
    }

    /// Add a boolean field
    pub fn boolean(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Boolean,
            default: None,
            description: None,
        });
        self
    }

    /// Add a boolean field with default
    pub fn boolean_with_default(mut self, name: &str, default: bool) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Boolean,
            default: Some(serde_yaml::Value::Bool(default)),
            description: None,
        });
        self
    }

    /// Add an array field
    pub fn array(mut self, name: &str, element_type: FieldType) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::Array(Box::new(element_type)),
            default: Some(serde_yaml::Value::Sequence(vec![])),
            description: None,
        });
        self
    }

    /// Add an agent reference field
    pub fn agent_ref(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::AgentRef,
            default: None,
            description: None,
        });
        self
    }

    /// Add a tool reference field
    pub fn tool_ref(mut self, name: &str) -> Self {
        self.fields.push(StateField {
            name: name.to_string(),
            field_type: FieldType::ToolRef,
            default: None,
            description: None,
        });
        self
    }

    /// Add a field with description
    pub fn with_description(mut self, description: &str) -> Self {
        if let Some(field) = self.fields.last_mut() {
            field.description = Some(description.to_string());
        }
        self
    }

    /// Build the schema
    pub fn build(self) -> StateSchema {
        StateSchema {
            fields: self.fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_schema_new() {
        let schema = StateSchema::new();
        assert!(schema.fields.is_empty());
    }

    #[test]
    fn test_state_schema_add_field() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "test".to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });
        assert_eq!(schema.fields.len(), 1);
    }

    #[test]
    fn test_state_schema_get_field() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "test".to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });

        assert!(schema.get_field("test").is_some());
        assert!(schema.get_field("missing").is_none());
    }

    #[test]
    fn test_state_schema_has_field() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "test".to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });

        assert!(schema.has_field("test"));
        assert!(!schema.has_field("missing"));
    }

    #[test]
    fn test_state_schema_field_names() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "a".to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });
        schema.add_field(StateField {
            name: "b".to_string(),
            field_type: FieldType::Integer,
            default: None,
            description: None,
        });

        let names = schema.field_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn test_state_schema_validate_required() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "required".to_string(),
            field_type: FieldType::String,
            default: None,
            description: None,
        });
        schema.add_field(StateField {
            name: "optional".to_string(),
            field_type: FieldType::String,
            default: Some(serde_yaml::Value::String("default".to_string())),
            description: None,
        });

        // Missing required field
        let empty = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        assert!(schema.validate_required(&empty).is_err());

        // Has required field
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            serde_yaml::Value::String("required".to_string()),
            serde_yaml::Value::String("value".to_string()),
        );
        let valid = serde_yaml::Value::Mapping(map);
        assert!(schema.validate_required(&valid).is_ok());
    }

    #[test]
    fn test_state_schema_apply_defaults() {
        let mut schema = StateSchema::new();
        schema.add_field(StateField {
            name: "with_default".to_string(),
            field_type: FieldType::String,
            default: Some(serde_yaml::Value::String("default".to_string())),
            description: None,
        });

        let mut value = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
        schema.apply_defaults(&mut value);

        if let serde_yaml::Value::Mapping(map) = value {
            let key = serde_yaml::Value::String("with_default".to_string());
            assert_eq!(
                map.get(&key),
                Some(&serde_yaml::Value::String("default".to_string()))
            );
        }
    }

    #[test]
    fn test_field_type_type_name() {
        assert_eq!(FieldType::String.type_name(), "string");
        assert_eq!(FieldType::Integer.type_name(), "integer");
        assert_eq!(FieldType::Float.type_name(), "float");
        assert_eq!(FieldType::Boolean.type_name(), "boolean");
        assert_eq!(
            FieldType::Array(Box::new(FieldType::String)).type_name(),
            "array"
        );
        assert_eq!(FieldType::AgentRef.type_name(), "agent_ref");
        assert_eq!(FieldType::ToolRef.type_name(), "tool_ref");
    }

    #[test]
    fn test_field_type_is_scalar() {
        assert!(FieldType::String.is_scalar());
        assert!(FieldType::Integer.is_scalar());
        assert!(FieldType::Float.is_scalar());
        assert!(FieldType::Boolean.is_scalar());
        assert!(!FieldType::Array(Box::new(FieldType::String)).is_scalar());
        assert!(!FieldType::Object(HashMap::new()).is_scalar());
        assert!(!FieldType::AgentRef.is_scalar());
    }

    #[test]
    fn test_field_type_is_collection() {
        assert!(!FieldType::String.is_collection());
        assert!(FieldType::Array(Box::new(FieldType::String)).is_collection());
        assert!(FieldType::Object(HashMap::new()).is_collection());
    }

    #[test]
    fn test_field_type_is_reference() {
        assert!(!FieldType::String.is_reference());
        assert!(FieldType::AgentRef.is_reference());
        assert!(FieldType::ToolRef.is_reference());
    }

    #[test]
    fn test_schema_builder_string() {
        let schema = SchemaBuilder::new()
            .string("name")
            .string_with_default("status", "pending")
            .build();

        assert_eq!(schema.fields.len(), 2);
        assert!(matches!(schema.fields[0].field_type, FieldType::String));
        assert!(schema.fields[1].default.is_some());
    }

    #[test]
    fn test_schema_builder_integer() {
        let schema = SchemaBuilder::new()
            .integer("count")
            .integer_with_default("limit", 100)
            .build();

        assert_eq!(schema.fields.len(), 2);
        assert!(matches!(schema.fields[0].field_type, FieldType::Integer));
        assert_eq!(
            schema.fields[1].default.as_ref().unwrap().as_i64(),
            Some(100)
        );
    }

    #[test]
    fn test_schema_builder_float() {
        let schema = SchemaBuilder::new().float("ratio").build();

        assert!(matches!(schema.fields[0].field_type, FieldType::Float));
    }

    #[test]
    fn test_schema_builder_boolean() {
        let schema = SchemaBuilder::new()
            .boolean("active")
            .boolean_with_default("enabled", true)
            .build();

        assert!(matches!(schema.fields[0].field_type, FieldType::Boolean));
        assert_eq!(
            schema.fields[1].default.as_ref().unwrap().as_bool(),
            Some(true)
        );
    }

    #[test]
    fn test_schema_builder_array() {
        let schema = SchemaBuilder::new()
            .array("items", FieldType::String)
            .build();

        assert!(matches!(schema.fields[0].field_type, FieldType::Array(_)));
    }

    #[test]
    fn test_schema_builder_refs() {
        let schema = SchemaBuilder::new()
            .agent_ref("reviewer")
            .tool_ref("file_tool")
            .build();

        assert!(matches!(schema.fields[0].field_type, FieldType::AgentRef));
        assert!(matches!(schema.fields[1].field_type, FieldType::ToolRef));
    }

    #[test]
    fn test_schema_builder_with_description() {
        let schema = SchemaBuilder::new()
            .string("name")
            .with_description("The name field")
            .build();

        assert_eq!(
            schema.fields[0].description,
            Some("The name field".to_string())
        );
    }

    #[test]
    fn test_schema_builder_chaining() {
        let schema = SchemaBuilder::new()
            .string("name")
            .integer("age")
            .boolean("active")
            .float("score")
            .build();

        assert_eq!(schema.fields.len(), 4);
    }

    #[test]
    fn test_field_type_serialization_roundtrip() {
        let types = vec![
            FieldType::String,
            FieldType::Integer,
            FieldType::Float,
            FieldType::Boolean,
            FieldType::AgentRef,
            FieldType::ToolRef,
        ];

        for original in types {
            let yaml = serde_yaml::to_string(&original).unwrap();
            let deserialized: FieldType = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(original, deserialized);
        }
    }

    #[test]
    fn test_array_type_serialization() {
        let yaml = r#"
name: items
type: array
element_type: integer
"#;

        let field: StateField = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            field.field_type,
            FieldType::Array(Box::new(FieldType::Integer))
        );
    }

    #[test]
    fn test_object_type_serialization() {
        let yaml = r#"
name: payload
type: object
"#;

        let field: StateField = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(field.field_type, FieldType::Object(_)));
    }
}
