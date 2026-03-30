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

/// Check if a value satisfies a field's constraints
pub fn satisfies_constraints(value: &Value, field: &StateField) -> std::result::Result<(), String> {
    // Type check
    validate_value_type(value, &field.field_type)?;

    // Additional constraints could be added here:
    // - String length limits
    // - Numeric range constraints
    // - Regex patterns
    // - Enum values
    // etc.

    Ok(())
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
mod tests {
    use super::*;
    use crate::swl::types::schema::{FieldType, SchemaBuilder};

    fn create_test_schema() -> StateSchema {
        SchemaBuilder::new()
            .string("name")
            .integer("count")
            .boolean_with_default("active", true)
            .float("score")
            .array("items", FieldType::String)
            .build()
    }

    #[test]
    fn test_validate_string() {
        assert!(validate_value_type(&serde_json::json!("hello"), &FieldType::String).is_ok());
        assert!(validate_value_type(&serde_json::json!(42), &FieldType::String).is_err());
    }

    #[test]
    fn test_validate_integer() {
        assert!(validate_value_type(&serde_json::json!(42), &FieldType::Integer).is_ok());
        assert!(validate_value_type(&serde_json::json!(-10), &FieldType::Integer).is_ok());
        assert!(validate_value_type(&serde_json::json!(3.15), &FieldType::Integer).is_err());
        assert!(validate_value_type(&serde_json::json!("42"), &FieldType::Integer).is_err());
    }

    #[test]
    fn test_validate_float() {
        assert!(validate_value_type(&serde_json::json!(3.15), &FieldType::Float).is_ok());
        assert!(validate_value_type(&serde_json::json!(42), &FieldType::Float).is_ok()); // Integers are valid floats
        assert!(validate_value_type(&serde_json::json!("3.14"), &FieldType::Float).is_err());
    }

    #[test]
    fn test_validate_boolean() {
        assert!(validate_value_type(&serde_json::json!(true), &FieldType::Boolean).is_ok());
        assert!(validate_value_type(&serde_json::json!(false), &FieldType::Boolean).is_ok());
        assert!(validate_value_type(&serde_json::json!(1), &FieldType::Boolean).is_err());
    }

    #[test]
    fn test_validate_array() {
        let string_array = FieldType::Array(Box::new(FieldType::String));
        assert!(validate_value_type(&serde_json::json!(["a", "b", "c"]), &string_array).is_ok());
        assert!(validate_value_type(&serde_json::json!([]), &string_array).is_ok());
        assert!(validate_value_type(&serde_json::json!(["a", 1, "c"]), &string_array).is_err());

        let int_array = FieldType::Array(Box::new(FieldType::Integer));
        assert!(validate_value_type(&serde_json::json!([1, 2, 3]), &int_array).is_ok());
    }

    #[test]
    fn test_validate_object() {
        assert!(validate_value_type(
            &serde_json::json!({"key": "value"}),
            &FieldType::Object(std::collections::HashMap::new())
        )
        .is_ok());
        assert!(validate_value_type(
            &serde_json::json!({}),
            &FieldType::Object(std::collections::HashMap::new())
        )
        .is_ok());
        assert!(validate_value_type(
            &serde_json::json!("not an object"),
            &FieldType::Object(std::collections::HashMap::new())
        )
        .is_err());
    }

    #[test]
    fn test_validate_state_against_schema() {
        let schema = create_test_schema();

        // Valid state
        let mut valid_state = HashMap::new();
        valid_state.insert("name".to_string(), serde_json::json!("test"));
        valid_state.insert("count".to_string(), serde_json::json!(42));
        valid_state.insert("score".to_string(), serde_json::json!(95.5));
        valid_state.insert("items".to_string(), serde_json::json!(["a", "b"]));
        // "active" has default, so not required

        assert!(validate_state_against_schema(&valid_state, &schema).is_ok());

        // Missing required field
        let mut invalid_state = HashMap::new();
        invalid_state.insert("name".to_string(), serde_json::json!("test"));
        // missing "count" which is required (no default)

        assert!(validate_state_against_schema(&invalid_state, &schema).is_err());

        // Wrong type
        let mut wrong_type_state = HashMap::new();
        wrong_type_state.insert("name".to_string(), serde_json::json!("test"));
        wrong_type_state.insert("count".to_string(), serde_json::json!("not a number"));

        assert!(validate_state_against_schema(&wrong_type_state, &schema).is_err());

        // Unknown field
        let mut unknown_field_state = HashMap::new();
        unknown_field_state.insert("name".to_string(), serde_json::json!("test"));
        unknown_field_state.insert("count".to_string(), serde_json::json!(42));
        unknown_field_state.insert("unknown".to_string(), serde_json::json!("value"));

        assert!(validate_state_against_schema(&unknown_field_state, &schema).is_err());
    }

    #[test]
    fn test_apply_defaults() {
        let schema = create_test_schema();
        let mut state = HashMap::new();

        apply_defaults(&mut state, &schema);

        // "active" has default true
        assert_eq!(state.get("active"), Some(&serde_json::json!(true)));
        // "name" has no default
        assert!(!state.contains_key("name"));
    }

    #[test]
    fn test_validate_state_transition() {
        let schema = create_test_schema();

        let mut old_state = HashMap::new();
        old_state.insert("name".to_string(), serde_json::json!("test"));
        old_state.insert("count".to_string(), serde_json::json!(42));
        old_state.insert("score".to_string(), serde_json::json!(95.5));

        // Valid transition: adding a field
        let mut new_state = old_state.clone();
        new_state.insert("active".to_string(), serde_json::json!(true));
        assert!(validate_state_transition(&old_state, &new_state, &schema).is_ok());

        // Invalid transition: removing a required field
        let mut bad_state = old_state.clone();
        bad_state.remove("count");
        assert!(validate_state_transition(&old_state, &bad_state, &schema).is_err());

        // Invalid transition: still missing a required field from the schema
        let mut partial_state = HashMap::new();
        partial_state.insert("name".to_string(), serde_json::json!("test"));
        partial_state.insert("count".to_string(), serde_json::json!(42));
        assert!(validate_state_transition(&old_state, &partial_state, &schema).is_err());

        // Valid transition: removing a field that has a default
        let mut ok_state = old_state.clone();
        ok_state.insert("active".to_string(), serde_json::json!(true));
        ok_state.remove("active");
        assert!(validate_state_transition(&old_state, &ok_state, &schema).is_ok());
    }

    #[test]
    fn test_get_validation_issues() {
        let schema = create_test_schema();
        let mut state = HashMap::new();
        state.insert("name".to_string(), serde_json::json!("test"));
        // missing "count" which is required
        state.insert("unknown".to_string(), serde_json::json!("value"));

        let issues = get_validation_issues(&state, &schema);
        assert_eq!(issues.len(), 3); // Missing required fields + unknown field
    }
}
