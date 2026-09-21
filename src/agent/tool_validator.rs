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

/// Argument-name aliases models commonly emit for tool calls, keyed by tool.
/// Each entry maps an alias spelling to the canonical schema field name.
///
/// These must match the `#[serde(alias = ...)]` attributes on the tools' Args
/// structs (src/tools/file.rs, src/tools/shell_exec/mod.rs).
const TOOL_ARG_ALIASES: &[(&str, &[(&str, &str)])] = &[
    (
        "file_read",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
        ],
    ),
    (
        "file_write",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
            ("text", "content"),
            ("body", "content"),
        ],
    ),
    (
        "file_edit",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
            ("old_string", "old_str"),
            ("new_string", "new_str"),
        ],
    ),
    (
        "file_delete",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
        ],
    ),
    (
        "file_multi_edit",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
            ("old_string", "old_str"),
            ("new_string", "new_str"),
        ],
    ),
    (
        "directory_tree",
        &[
            ("file_path", "path"),
            ("file", "path"),
            ("filepath", "path"),
        ],
    ),
    ("shell_exec", &[("cmd", "command")]),
];

/// Rewrite alias argument spellings to the canonical schema field names for a
/// tool call's arguments JSON.
///
/// Native function calls are schema-validated BEFORE the tool's deserializer
/// runs (`validate_tool_call` / `validate_tool_arguments_schema` check the
/// schema's `required` list against the raw argument object), so the serde
/// aliases on the Args structs alone cannot rescue an alias spelling — e.g.
/// the progress guard once injected `old_string`/`new_string` guidance and a
/// faithfully-mirroring model failed with "missing field 'old_str'". This must
/// run at the earliest dispatch point, before validation, safety checks, and
/// bookkeeping.
///
/// The alias map is applied at the top level and, for `file_multi_edit`, also
/// inside the `edits` array items (which carry the same path/old_str/new_str
/// keys). Returns the input unchanged when it fails to parse or is not a JSON
/// object, and is idempotent (canonical spellings pass through untouched).
pub(crate) fn normalize_tool_arg_aliases(tool_name: &str, args_str: &str) -> String {
    let Some(aliases) = TOOL_ARG_ALIASES
        .iter()
        .find(|(name, _)| *name == tool_name)
        .map(|(_, aliases)| aliases)
    else {
        return args_str.to_string();
    };

    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(args_str) else {
        return args_str.to_string();
    };
    let Some(obj) = value.as_object_mut() else {
        return args_str.to_string();
    };

    apply_alias_map(obj, aliases);
    if tool_name == "file_multi_edit" {
        if let Some(edits) = obj.get_mut("edits").and_then(|e| e.as_array_mut()) {
            for item in edits {
                if let Some(item_obj) = item.as_object_mut() {
                    apply_alias_map(item_obj, aliases);
                }
            }
        }
    }

    serde_json::to_string(&value).unwrap_or_else(|_| args_str.to_string())
}

/// Move alias-keyed values onto their canonical keys. When both spellings are
/// present the canonical value wins (the alias value is dropped).
fn apply_alias_map(obj: &mut serde_json::Map<String, serde_json::Value>, aliases: &[(&str, &str)]) {
    for (alias, canonical) in aliases {
        if let Some(value) = obj.remove(*alias) {
            obj.entry((*canonical).to_string()).or_insert(value);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/tool_validator/tool_validator_test.rs"]
mod tests;
