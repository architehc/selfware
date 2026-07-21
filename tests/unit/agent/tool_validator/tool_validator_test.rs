use super::*;
use crate::api::types::{FunctionDefinition, ToolDefinition, ToolFunction};

fn make_definition(name: &str, required: Vec<&str>) -> ToolDefinition {
    let required: Vec<Value> = required
        .into_iter()
        .map(|s| Value::String(s.to_string()))
        .collect();
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
    assert!(err.contains("expected type"));
    assert!(err.contains("string"));
}

#[test]
fn test_validate_union_type_accepts_any_member() {
    let defs = vec![ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "example".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "value": { "type": ["string", "null"] }
                }
            }),
        },
    }];

    let string_call = ToolCall {
        id: "1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "example".to_string(),
            arguments: r#"{"value": "hello"}"#.to_string(),
        },
    };
    assert!(validate_tool_call(&string_call, &defs).is_ok());

    let null_call = ToolCall {
        id: "2".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "example".to_string(),
            arguments: r#"{"value": null}"#.to_string(),
        },
    };
    assert!(validate_tool_call(&null_call, &defs).is_ok());

    let bad_call = ToolCall {
        id: "3".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "example".to_string(),
            arguments: r#"{"value": 42}"#.to_string(),
        },
    };
    assert!(validate_tool_call(&bad_call, &defs).is_err());
}

#[test]
fn test_validate_enum_rejects_invalid_value() {
    let defs = vec![ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: "example".to_string(),
            description: "test".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["fast", "slow"] }
                }
            }),
        },
    }];

    let ok_call = ToolCall {
        id: "1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "example".to_string(),
            arguments: r#"{"mode": "fast"}"#.to_string(),
        },
    };
    assert!(validate_tool_call(&ok_call, &defs).is_ok());

    let bad_call = ToolCall {
        id: "2".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "example".to_string(),
            arguments: r#"{"mode": "turbo"}"#.to_string(),
        },
    };
    let err = validate_tool_call(&bad_call, &defs)
        .unwrap_err()
        .to_string();
    assert!(err.contains("must be one of"));
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
