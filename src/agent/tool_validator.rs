//! Tool call validation against JSON schemas.
//!
//! Validates that LLM-generated tool calls conform to the registered
//! tool definitions, checking required fields and basic types.

use anyhow::{bail, Result};
use serde_json::Value;

use crate::api::types::{ToolCall, ToolDefinition};
use crate::errors::ToolError;

/// Validate a tool call against its registered definition schema.
///
/// Returns `Ok(())` if the call passes structural and schema validation,
/// otherwise returns a descriptive error.
pub fn validate_tool_call(call: &ToolCall, definitions: &[ToolDefinition]) -> Result<()> {
    // First: structural sanity checks
    call.validate_structure()?;

    let tool_name = call.function.name.trim();

    // Find the matching definition
    let definition = definitions
        .iter()
        .find(|d| d.function.name == tool_name)
        .ok_or_else(|| ToolError::InvalidToolCall {
            name: tool_name.to_string(),
            message: format!(
                "Unknown tool '{}'. Available tools: {:?}",
                tool_name,
                definitions
                    .iter()
                    .map(|d| &d.function.name)
                    .collect::<Vec<_>>()
            ),
        })?;

    // Parse arguments
    let args: Value =
        serde_json::from_str(&call.function.arguments).map_err(|e| ToolError::InvalidToolCall {
            name: tool_name.to_string(),
            message: format!("Arguments are not valid JSON: {}", e),
        })?;

    // Arguments must be an object
    let args_obj = args
        .as_object()
        .ok_or_else(|| ToolError::InvalidArguments {
            name: tool_name.to_string(),
            message: "Tool arguments must be a JSON object".to_string(),
        })?;

    // Validate required fields from schema
    let schema = &definition.function.parameters;
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(req_str) = req.as_str() {
                if !args_obj.contains_key(req_str) {
                    return Err(ToolError::InvalidArguments {
                        name: tool_name.to_string(),
                        message: format!("Missing required argument '{}'", req_str),
                    }
                    .into());
                }
            }
        }
    }

    // Basic type validation for known scalar types (including JSON Schema unions).
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, prop_schema) in properties {
            if let Some(arg_val) = args_obj.get(prop_name) {
                if let Some(prop_type) = prop_schema.get("type") {
                    if !value_matches_schema_type(arg_val, prop_type) {
                        return Err(ToolError::InvalidArguments {
                            name: tool_name.to_string(),
                            message: format!(
                                "Argument '{}' expected type '{}' but got incompatible value",
                                prop_name, prop_type
                            ),
                        }
                        .into());
                    }
                }

                // Enum validation
                if let Some(enum_values) = prop_schema.get("enum").and_then(|e| e.as_array()) {
                    if !enum_values.iter().any(|allowed| allowed == arg_val) {
                        return Err(ToolError::InvalidArguments {
                            name: tool_name.to_string(),
                            message: format!(
                                "Argument '{}' must be one of {:?}",
                                prop_name, enum_values
                            ),
                        }
                        .into());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Best-effort type check for JSON schema types. `schema_type` may be a single
/// type string or an array of type strings (a union such as `["string", "null"]`).
fn value_matches_schema_type(value: &Value, schema_type: &Value) -> bool {
    if let Some(type_str) = schema_type.as_str() {
        return value_matches_single_type(value, type_str);
    }

    if let Some(types) = schema_type.as_array() {
        return types.iter().any(|t| value_matches_schema_type(value, t));
    }

    // Unknown `type` shapes pass through.
    true
}

fn value_matches_single_type(value: &Value, schema_type: &str) -> bool {
    match schema_type {
        "string" => value.is_string(),
        "integer" => value.is_i64() || value.is_u64(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "null" => value.is_null(),
        _ => true, // Unknown types pass through
    }
}

/// Batch-validate a list of tool calls.
///
/// Collects all validation errors and returns them as a single error message
/// so the caller can report every bad tool call at once.
pub fn validate_tool_calls(calls: &[ToolCall], definitions: &[ToolDefinition]) -> Result<()> {
    let mut errors: Vec<String> = Vec::new();

    for (i, call) in calls.iter().enumerate() {
        if let Err(e) = validate_tool_call(call, definitions) {
            errors.push(format!("[call {}] {}", i + 1, e));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!("Tool call validation failed:\n{}", errors.join("\n"));
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/tool_validator/tool_validator_test.rs"]
mod tests;
