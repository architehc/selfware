//! State Validation
//!
//! Provides validation for workflow state against defined schemas.
//! Ensures type safety and required field constraints.

use crate::errors::SelfwareError;
use crate::swl::types::schema::{FieldType, StateField, StateSchema};
use serde_json::Value;
use std::collections::HashMap;
use std::result::Result as StdResult;

/// Validation error for state values
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Field '{}': {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validate state against a schema
pub fn validate_state_against_schema(
    state: &HashMap<String, Value>,
    schema: &StateSchema,
) -> StdResult<(), SelfwareError> {
    let mut errors = Vec::new();

    // Check required fields
    for field in &schema.fields {
        if field.default.is_none() && !state.contains_key(&field.name) {
            errors.push(ValidationError {
                field: field.name.clone(),
                message: "Required field is missing".to_string(),
            });
            continue;
        }

        // Validate type if field is present
        if let Some(value) = state.get(&field.name) {
            if let Err(e) = validate_value_type(value, &field.field_type) {
                errors.push(ValidationError {
                    field: field.name.clone(),
                    message: e,
                });
            }
        }
    }

    // Check for unknown fields
    let known_fields: std::collections::HashSet<_> =
        schema.fields.iter().map(|f| &f.name).collect();

    for key in state.keys() {
        if !known_fields.contains(key) {
            errors.push(ValidationError {
                field: key.clone(),
                message: "Unknown field (not defined in schema)".to_string(),
            });
        }
    }

    if !errors.is_empty() {
        let error_messages: Vec<_> = errors.iter().map(|e| e.to_string()).collect();
        return Err(SelfwareError::Internal(format!(
            "State validation failed:\n{}",
            error_messages.join("\n")
        )));
    }

    Ok(())
}

/// Validate a single value against a field type
pub fn validate_value_type(
    value: &Value,
    field_type: &FieldType,
) -> std::result::Result<(), String> {
    match field_type {
        FieldType::String => {
            if !value.is_string() {
                return Err(format!("Expected string, got {}", json_type_name(value)));
            }
        }
        FieldType::Integer => {
            if !value.is_i64() && !value.is_u64() {
                return Err(format!("Expected integer, got {}", json_type_name(value)));
            }
        }
        FieldType::Float => {
            if !value.is_f64() && !value.is_i64() && !value.is_u64() {
                return Err(format!("Expected float, got {}", json_type_name(value)));
            }
        }
        FieldType::Boolean => {
            if !value.is_boolean() {
                return Err(format!("Expected boolean, got {}", json_type_name(value)));
            }
        }
        FieldType::Array(element_type) => {
            if let Some(arr) = value.as_array() {
                for (i, item) in arr.iter().enumerate() {
                    if let Err(e) = validate_value_type(item, element_type) {
                        return Err(format!("Array element {}: {}", i, e));
                    }
                }
            } else {
                return Err(format!("Expected array, got {}", json_type_name(value)));
            }
        }
        FieldType::Object(_) => {
            if !value.is_object() {
                return Err(format!("Expected object, got {}", json_type_name(value)));
            }
            // Note: For nested objects, we could validate field types recursively
            // but for now we just check it's an object
        }
        FieldType::AgentRef => {
            // AgentRef should be a string (agent name)
            if !value.is_string() {
                return Err(format!(
                    "Expected agent reference (string), got {}",
                    json_type_name(value)
                ));
            }
        }
        FieldType::ToolRef => {
            // ToolRef should be a string (tool name)
            if !value.is_string() {
                return Err(format!(
                    "Expected tool reference (string), got {}",
                    json_type_name(value)
                ));
            }
        }
    }

    Ok(())
}

/// Get the JSON type name for a value
fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "float"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Validate a specific field in state
pub fn validate_field(
    state: &HashMap<String, Value>,
    field: &StateField,
) -> std::result::Result<(), ValidationError> {
    // Check if required field is present
    if field.default.is_none() && !state.contains_key(&field.name) {
        return Err(ValidationError {
            field: field.name.clone(),
            message: "Required field is missing".to_string(),
        });
    }

    // Validate type if present
    if let Some(value) = state.get(&field.name) {
        if let Err(msg) = validate_value_type(value, &field.field_type) {
            return Err(ValidationError {
                field: field.name.clone(),
                message: msg,
            });
        }
    }

    Ok(())
}

/// Apply schema defaults to a state value (mutable)
pub fn apply_defaults(state: &mut HashMap<String, Value>, schema: &StateSchema) {
    for field in &schema.fields {
        if !state.contains_key(&field.name) {
            if let Some(ref default) = field.default {
                if let Ok(json_value) = yaml_to_json(default) {
                    state.insert(field.name.clone(), json_value);
                }
            }
        }
    }
}

/// Convert YAML value to JSON value
fn yaml_to_json(value: &serde_yaml::Value) -> std::result::Result<Value, serde_json::Error> {
    serde_json::to_value(value)
}

/// Get a summary of validation issues
pub fn get_validation_issues(
    state: &HashMap<String, Value>,
    schema: &StateSchema,
) -> Vec<ValidationError> {
    let mut issues = Vec::new();

    // Check required fields
    for field in &schema.fields {
        if let Err(e) = validate_field(state, field) {
            issues.push(e);
        }
    }

    // Check for unknown fields
    let known_fields: std::collections::HashSet<_> =
        schema.fields.iter().map(|f| &f.name).collect();

    for key in state.keys() {
        if !known_fields.contains(key) {
            issues.push(ValidationError {
                field: key.clone(),
                message: "Unknown field (not defined in schema)".to_string(),
            });
        }
    }

    issues
}

/// Validate state transitions
///
/// Checks that a state transition is valid (e.g., required fields aren't removed)
pub fn validate_state_transition(
    old_state: &HashMap<String, Value>,
    new_state: &HashMap<String, Value>,
    schema: &StateSchema,
) -> StdResult<(), SelfwareError> {
    // Check that required fields weren't removed
    for field in &schema.fields {
        if field.default.is_none()
            && old_state.contains_key(&field.name)
            && !new_state.contains_key(&field.name)
        {
            return Err(SelfwareError::Internal(format!(
                "State transition invalid: required field '{}' was removed",
                field.name
            )));
        }
    }

    // Validate the new state
    validate_state_against_schema(new_state, schema)
}

#[cfg(test)]
#[path = "../../../tests/unit/swl/state/validation/validation_test.rs"]
mod tests;
