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

// --- normalize_tool_arg_aliases ------------------------------------------

/// Parse [`normalize_tool_arg_aliases`] output back into a JSON value.
fn normalized(tool: &str, args: &str) -> serde_json::Value {
    serde_json::from_str(&normalize_tool_arg_aliases(tool, args)).unwrap()
}

#[test]
fn test_normalize_file_edit_old_string_new_string() {
    // The exact spelling the progress guard guidance injected (with the
    // stray tool_type field) — validation rejects this shape without
    // normalization because the schema requires path/old_str/new_str.
    let out = normalized(
        "file_edit",
        r#"{"tool_type":"file_edit","path":"src/a.rs","old_string":"x","new_string":"y"}"#,
    );
    assert_eq!(out["path"], "src/a.rs");
    assert_eq!(out["old_str"], "x");
    assert_eq!(out["new_str"], "y");
    assert!(out.get("old_string").is_none());
    assert!(out.get("new_string").is_none());
    // The unknown tool_type field is preserved (later stages ignore it),
    // keeping the normalization purely additive over mutation.
    assert_eq!(out["tool_type"], "file_edit");
}

#[test]
fn test_normalize_path_and_content_aliases() {
    assert_eq!(
        normalized("file_write", r#"{"file_path":"a.txt","text":"hi"}"#),
        serde_json::json!({"path": "a.txt", "content": "hi"})
    );
    assert_eq!(
        normalized("file_write", r#"{"file":"a.txt","body":"hi"}"#),
        serde_json::json!({"path": "a.txt", "content": "hi"})
    );
    assert_eq!(
        normalized("file_read", r#"{"filepath":"a.txt"}"#),
        serde_json::json!({"path": "a.txt"})
    );
    assert_eq!(
        normalized("directory_tree", r#"{"file":"a"}"#),
        serde_json::json!({"path": "a"})
    );
}

#[test]
fn test_normalize_shell_exec_cmd() {
    assert_eq!(
        normalized("shell_exec", r#"{"cmd":"echo hi"}"#),
        serde_json::json!({"command": "echo hi"})
    );
}

#[test]
fn test_normalize_multi_edit_edits_items() {
    let out = normalized(
        "file_multi_edit",
        r#"{"path":"t","edits":[{"path":"a","old_string":"x","new_string":"y"},{"filepath":"b","old_string":"p","new_string":"q"}]}"#,
    );
    assert_eq!(out["edits"][0]["path"], "a");
    assert_eq!(out["edits"][0]["old_str"], "x");
    assert_eq!(out["edits"][0]["new_str"], "y");
    assert_eq!(out["edits"][1]["path"], "b");
    assert_eq!(out["edits"][1]["old_str"], "p");
}

#[test]
fn test_normalize_is_idempotent_and_preserves_canonical() {
    let input = r#"{"path":"a.txt","old_str":"x","new_str":"y","cmd":"keep"}"#;
    let once = normalize_tool_arg_aliases("file_edit", input);
    let twice = normalize_tool_arg_aliases("file_edit", &once);
    assert_eq!(once, twice);
    // Non-file_edit tools and canonical spellings pass through untouched.
    assert_eq!(
        normalize_tool_arg_aliases("git_status", input),
        input.to_string()
    );
}

#[test]
fn test_normalize_passthrough_on_non_object_or_unparsable() {
    assert_eq!(
        normalize_tool_arg_aliases("file_edit", r#"{not json}"#),
        r#"{not json}"#.to_string()
    );
    assert_eq!(
        normalize_tool_arg_aliases("file_edit", "[1,2,3]"),
        "[1,2,3]".to_string()
    );
    assert_eq!(normalize_tool_arg_aliases("file_edit", ""), "".to_string());
}

#[test]
fn test_normalize_git_diff_and_screen_aliases() {
    assert_eq!(
        normalized("git_diff", r#"{"file":"src/main.rs"}"#),
        serde_json::json!({"path": "src/main.rs"})
    );
    assert_eq!(
        normalized("git_diff", r#"{"file_path":"src/main.rs"}"#),
        serde_json::json!({"path": "src/main.rs"})
    );
    assert_eq!(
        normalized("computer_screen", r#"{"target":"region"}"#),
        serde_json::json!({"action": "region"})
    );
    assert_eq!(
        normalized("screen_capture", r#"{"action":"screen"}"#),
        serde_json::json!({"target": "screen"})
    );
}
