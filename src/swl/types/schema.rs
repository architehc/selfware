//! State Schema Definitions
//!
//! Defines the structure of workflow state with typed fields.

use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// State schema definition
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StateSchema {
    #[serde(default)]
    pub fields: Vec<StateField>,
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
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    /// String value
    #[default]
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
#[path = "../../../tests/unit/swl/types/schema/schema_test.rs"]
mod tests;
