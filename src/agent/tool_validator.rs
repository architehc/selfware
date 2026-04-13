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
        .ok_or_else(|| {
            ToolError::InvalidToolCall {
                name: tool_name.to_string(),
                message: format!(
                    "Unknown tool '{}'. Available tools: {:?}",
                    tool_name,
                    definitions.iter().map(|d| &d.function.name).collect::<Vec<_>>()
                ),
            }
        })?;

    // Parse arguments
    let args: Value = serde_json::from_str(&call.function.arguments).map_err(|e| {
        ToolError::InvalidToolCall {
            name: tool_name.to_string(),
            message: format!("Arguments are not valid JSON: {}", e),
        }
    })?;

    // Arguments must be an object
    let args_obj = args.as_object().ok_or_else(|| {
        ToolError::InvalidArguments {
            name: tool_name.to_string(),
            message: "Tool arguments must be a JSON object".to_string(),
        }
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

    // Basic type validation for known scalar types
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, prop_schema) in properties {
            if let Some(arg_val) = args_obj.get(prop_name) {
                if let Some(prop_type) = prop_schema.get("type").and_then(|t| t.as_str()) {
                    if !value_matches_type(arg_val, prop_type) {
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
            }
        }
    }

    Ok(())
}

/// Best-effort type check for JSON schema types.
fn value_matches_type(value: &Value, schema_type: &str) -> bool {
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
mod tests {
    use super::*;
    use crate::api::types::{FunctionDefinition, ToolDefinition, ToolFunction};

    fn make_definition(name: &str, required: Vec<&str>) -> ToolDefinition {
        let required: Vec<Value> = required.into_iter().map(|s| Value::String(s.to_string())).collect();
        ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: name.to_string(),
                description: "test tool".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "count": { "type": "integer" }
                    },
                    "required": required
                }),
            },
        }
    }

    #[test]
    fn test_validate_tool_call_ok() {
        let defs = vec![make_definition("file_read", vec!["path"])];
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{"path": "/tmp/test.txt"}"#.to_string(),
            },
        };
        assert!(validate_tool_call(&call, &defs).is_ok());
    }

    #[test]
    fn test_validate_unknown_tool() {
        let defs = vec![make_definition("file_read", vec!["path"])];
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "unknown_tool".to_string(),
                arguments: r#"{}"#.to_string(),
            },
        };
        assert!(validate_tool_call(&call, &defs).is_err());
    }

    #[test]
    fn test_validate_missing_required_arg() {
        let defs = vec![make_definition("file_read", vec!["path"])];
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{"count": 5}"#.to_string(),
            },
        };
        let err = validate_tool_call(&call, &defs).unwrap_err().to_string();
        assert!(err.contains("Missing required argument 'path'"));
    }

    #[test]
    fn test_validate_wrong_type() {
        let defs = vec![make_definition("file_read", vec![])];
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{"path": 123}"#.to_string(),
            },
        };
        let err = validate_tool_call(&call, &defs).unwrap_err().to_string();
        assert!(err.contains("expected type 'string'"));
    }

    #[test]
    fn test_validate_structure_empty_id() {
        let call = ToolCall {
            id: "".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{}"#.to_string(),
            },
        };
        let err = call.validate_structure().unwrap_err().to_string();
        assert!(err.contains("missing a valid 'id'"));
    }

    #[test]
    fn test_validate_structure_invalid_json() {
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{not json}"#.to_string(),
            },
        };
        let err = call.validate_structure().unwrap_err().to_string();
        assert!(err.contains("not valid JSON"));
    }
}
