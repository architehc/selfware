use super::sanitize_tool_calls;
use crate::api::types::{ToolCall, ToolFunction};

fn call(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall {
        id: id.to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: name.to_string(),
            arguments: args.to_string(),
        },
    }
}

#[test]
fn keeps_well_formed_calls() {
    let (kept, dropped) = sanitize_tool_calls(vec![
        call("id1", "file_read", r#"{"path":"a.rs"}"#),
        call("id2", "shell_exec", r#"{"command":"ls"}"#),
    ]);
    assert_eq!(kept.len(), 2);
    assert_eq!(dropped, 0);
}

#[test]
fn drops_call_with_truncated_json_args() {
    // Simulates a stream cut mid-arguments: valid id+name, invalid JSON.
    let (kept, dropped) = sanitize_tool_calls(vec![
        call("id1", "file_read", r#"{"path":"a.rs"}"#),
        call("id2", "file_write", r#"{"path":"b.rs","content":"partial"#),
    ]);
    assert_eq!(dropped, 1);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id, "id1");
}

#[test]
fn drops_call_missing_name_or_id() {
    let (kept, dropped) = sanitize_tool_calls(vec![
        call("", "file_read", "{}"),
        call("id2", "", "{}"),
        call("id3", "file_read", "{}"),
    ]);
    assert_eq!(dropped, 2);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].id, "id3");
}

#[test]
fn empty_input_yields_empty_output() {
    let (kept, dropped) = sanitize_tool_calls(vec![]);
    assert!(kept.is_empty());
    assert_eq!(dropped, 0);
}
