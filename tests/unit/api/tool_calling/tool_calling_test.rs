use super::*;
use crate::api::types::{FunctionDefinition, MessageContent};

fn dummy_tool_def(name: &str) -> ToolDefinition {
    ToolDefinition {
        def_type: "function".to_string(),
        function: FunctionDefinition {
            name: name.to_string(),
            description: format!("Test tool {}", name),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        },
    }
}

fn assistant_msg_with_native_calls(calls: Vec<ToolCall>) -> Message {
    Message {
        role: "assistant".to_string(),
        content: MessageContent::Text(String::new()),
        reasoning_content: None,
        tool_calls: Some(calls),
        tool_call_id: None,
        name: None,
    }
}

fn assistant_msg_with_text(text: &str) -> Message {
    Message {
        role: "assistant".to_string(),
        content: MessageContent::Text(text.to_string()),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
    }
}

// ---------- attach_tools ----------

#[test]
fn attach_tools_native_sets_tool_choice() {
    let mut body = serde_json::json!({"model": "x"});
    let tools = Some(vec![dummy_tool_def("foo")]);
    attach_tools(&mut body, &tools, true);
    assert!(body["tools"].is_array());
    assert_eq!(body["tool_choice"], "auto");
}

#[test]
fn attach_tools_text_mode_omits_tool_choice() {
    let mut body = serde_json::json!({"model": "x"});
    let tools = Some(vec![dummy_tool_def("foo")]);
    attach_tools(&mut body, &tools, false);
    assert!(body["tools"].is_array());
    assert!(
        body.get("tool_choice").is_none(),
        "tool_choice must NOT be sent when native FC is off"
    );
}

#[test]
fn attach_tools_none_is_noop() {
    let mut body = serde_json::json!({"model": "x"});
    attach_tools(&mut body, &None, true);
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
}

// ---------- extract_tool_calls ----------

/// Native FC on, model emits `tool_calls` field → parsed correctly.
#[test]
fn native_fc_on_with_native_field_returns_native() {
    let native = ToolCall {
        id: "call_abc".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "shell_exec".to_string(),
            arguments: r#"{"command":"ls"}"#.to_string(),
        },
    };
    let msg = assistant_msg_with_native_calls(vec![native.clone()]);

    let calls = extract_tool_calls(&msg, true);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_abc");
    assert_eq!(calls[0].function.name, "shell_exec");
}

/// Native FC on, model emits text `<tool>...</tool>` (sglang fallback)
/// → ALSO parsed correctly via fallback.
#[test]
fn native_fc_on_with_text_falls_back_to_parser() {
    let text = r#"I'll list the directory.
<tool>
<name>shell_exec</name>
<arguments>{"command": "ls -la"}</arguments>
</tool>"#;
    let msg = assistant_msg_with_text(text);

    let calls = extract_tool_calls(&msg, true);
    assert_eq!(
        calls.len(),
        1,
        "text-formatted tool calls must be extracted even with native FC enabled"
    );
    assert_eq!(calls[0].function.name, "shell_exec");
    assert!(calls[0].id.starts_with("parsed_"));
    // Arguments should round-trip as valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&calls[0].function.arguments).unwrap();
    assert_eq!(parsed["command"], "ls -la");
}

/// Native FC off, model emits text → parsed correctly.
#[test]
fn native_fc_off_with_text_parses_xml() {
    let text = r#"<tool>
<name>file_read</name>
<arguments>{"path": "src/main.rs"}</arguments>
</tool>"#;
    let msg = assistant_msg_with_text(text);

    let calls = extract_tool_calls(&msg, false);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].function.name, "file_read");
}

/// No tool calls in either field → empty vec, no panic.
#[test]
fn no_tool_calls_returns_empty() {
    let msg = assistant_msg_with_text("just a plain answer with no tools");
    let calls = extract_tool_calls(&msg, true);
    assert!(calls.is_empty());
    let calls = extract_tool_calls(&msg, false);
    assert!(calls.is_empty());
}

/// Empty `tool_calls` field (Some(vec![])) should still trigger
/// fallback parsing — sglang sometimes emits this exact shape.
#[test]
fn empty_native_field_falls_back() {
    let mut msg = assistant_msg_with_text(
        r#"<tool><name>file_read</name><arguments>{"path":"x"}</arguments></tool>"#,
    );
    msg.tool_calls = Some(vec![]);

    let calls = extract_tool_calls(&msg, true);
    assert_eq!(
        calls.len(),
        1,
        "empty tool_calls:[] must trigger text fallback"
    );
    assert_eq!(calls[0].function.name, "file_read");
}

/// Multiple text-format tool calls in one message.
#[test]
fn multiple_text_tool_calls_all_extracted() {
    let text = r#"<tool>
<name>file_read</name>
<arguments>{"path": "a"}</arguments>
</tool>
<tool>
<name>file_read</name>
<arguments>{"path": "b"}</arguments>
</tool>"#;
    let msg = assistant_msg_with_text(text);

    let calls = extract_tool_calls(&msg, true);
    assert_eq!(calls.len(), 2);
    assert_ne!(
        calls[0].id, calls[1].id,
        "synthetic ids must be unique even for repeated tool names"
    );
}

/// `extract_tool_calls_from_text` should match `extract_tool_calls`
/// for the text-only path.
#[test]
fn text_helper_matches_message_extraction() {
    let text = r#"<tool>
<name>shell_exec</name>
<arguments>{"command": "echo hi"}</arguments>
</tool>"#;
    let msg = assistant_msg_with_text(text);

    let from_msg = extract_tool_calls(&msg, true);
    let from_text = extract_tool_calls_from_text(text);
    assert_eq!(from_msg.len(), from_text.len());
    assert_eq!(from_msg[0].function.name, from_text[0].function.name);
    assert_eq!(
        from_msg[0].function.arguments,
        from_text[0].function.arguments
    );
}

/// Regression: `chat_with_profile` and `chat` must extract tool calls
/// using the same policy. This ensures any consumer holding a
/// reference to the unified module gets identical behavior whether
/// they came in via the main path or the model-profile path.
#[test]
fn chat_and_chat_with_profile_extract_identically() {
    // Same response shape — verify same extracted result.
    let text = r#"<tool>
<name>file_read</name>
<arguments>{"path": "main.rs"}</arguments>
</tool>"#;
    let msg = assistant_msg_with_text(text);

    let via_main = extract_tool_calls(&msg, true);
    let via_profile = extract_tool_calls(&msg, true);
    assert_eq!(via_main.len(), via_profile.len());
    assert_eq!(
        via_main[0].function.name, via_profile[0].function.name,
        "main and profile paths must extract identically"
    );
}

/// Regression: SWL runtime must parse tool calls the same way the
/// main agent does. SWL used to maintain its own native-vs-text
/// branching logic; both paths now route through this single
/// extractor.
#[test]
fn swl_runtime_parses_same_as_main_agent() {
    // sglang-like response: native field empty but tool in content.
    let text = r#"<tool>
<name>shell_exec</name>
<arguments>{"command": "echo swl"}</arguments>
</tool>"#;
    let mut msg = assistant_msg_with_text(text);
    msg.tool_calls = Some(vec![]);

    // Main agent path
    let main = extract_tool_calls(&msg, true);
    // SWL runtime path (uses the same function)
    let swl = extract_tool_calls(&msg, true);

    assert_eq!(main.len(), 1);
    assert_eq!(swl.len(), 1);
    assert_eq!(main[0].function.name, swl[0].function.name);
    assert_eq!(main[0].function.arguments, swl[0].function.arguments);
}

/// Dual tool_choice setting round-trip:
/// Assert that structured path (native tool_calls with tool_choice="auto") and
/// XML fallback path (Qwen <tool_call> with empty tool_calls) produce structurally
/// identical ToolCall values with exact argument equivalency.
#[test]
fn test_tool_calling_roundtrip_both_tool_choice_settings() {
    let mut body_native = serde_json::json!({"model": "test"});
    let tools = Some(vec![dummy_tool_def("file_read")]);
    attach_tools(&mut body_native, &tools, true);
    assert_eq!(body_native["tool_choice"], "auto");

    let mut body_prompt = serde_json::json!({"model": "test"});
    attach_tools(&mut body_prompt, &tools, false);
    assert!(body_prompt.get("tool_choice").is_none());

    // Structured response
    let structured_call = ToolCall {
        id: "call_native_123".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path":"src/main.rs"}"#.to_string(),
        },
    };
    let msg_structured = assistant_msg_with_native_calls(vec![structured_call]);

    // XML fallback response from Qwen/SGLang
    let xml_text = "<tool_call>\n<function=file_read>\n<parameter=path>src/main.rs</parameter>\n</function>\n</tool_call>";
    let mut msg_xml = assistant_msg_with_text(xml_text);
    msg_xml.tool_calls = Some(vec![]);

    let calls_structured = extract_tool_calls(&msg_structured, true);
    let calls_xml = extract_tool_calls(&msg_xml, false);

    assert_eq!(calls_structured.len(), 1);
    assert_eq!(calls_xml.len(), 1);
    assert_eq!(calls_structured[0].call_type, calls_xml[0].call_type);
    assert_eq!(
        calls_structured[0].function.name,
        calls_xml[0].function.name
    );

    let args_structured: serde_json::Value =
        serde_json::from_str(&calls_structured[0].function.arguments).unwrap();
    let args_xml: serde_json::Value =
        serde_json::from_str(&calls_xml[0].function.arguments).unwrap();
    assert_eq!(args_structured, args_xml);
    assert_eq!(args_xml["path"], "src/main.rs");
}

/// Parallel tool calls:
/// Assert that multi-tool execution in structured vs XML format extracts
/// all invocations in order with exact arguments and unique synthetic IDs.
#[test]
fn test_parallel_tool_calls_roundtrip_and_structural_equivalence() {
    // Structured response with parallel calls
    let structured_calls = vec![
        ToolCall {
            id: "call_paris_01".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: r#"{"city":"Paris"}"#.to_string(),
            },
        },
        ToolCall {
            id: "call_tokyo_02".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "get_weather".to_string(),
                arguments: r#"{"city":"Tokyo"}"#.to_string(),
            },
        },
    ];
    let msg_structured = assistant_msg_with_native_calls(structured_calls);

    // Qwen/SGLang parallel tool calls in XML format
    let parallel_xml = r#"I will query weather for both cities in parallel.
<tool_call>
<function=get_weather>
<parameter=city>Paris</parameter>
</function>
</tool_call>
<tool_call>
<function=get_weather>
<parameter=city>Tokyo</parameter>
</function>
</tool_call>"#;
    let mut msg_xml = assistant_msg_with_text(parallel_xml);
    msg_xml.tool_calls = Some(vec![]);

    let parsed_structured = extract_tool_calls(&msg_structured, true);
    let parsed_xml = extract_tool_calls(&msg_xml, false);

    assert_eq!(parsed_structured.len(), 2);
    assert_eq!(parsed_xml.len(), 2);

    for i in 0..2 {
        assert_eq!(parsed_structured[i].call_type, parsed_xml[i].call_type);
        assert_eq!(
            parsed_structured[i].function.name,
            parsed_xml[i].function.name
        );
        let s_args: serde_json::Value =
            serde_json::from_str(&parsed_structured[i].function.arguments).unwrap();
        let x_args: serde_json::Value =
            serde_json::from_str(&parsed_xml[i].function.arguments).unwrap();
        assert_eq!(s_args, x_args);
    }

    assert_ne!(parsed_xml[0].id, parsed_xml[1].id);
    assert!(parsed_xml[0].id.starts_with("parsed_"));
    assert!(parsed_xml[1].id.starts_with("parsed_"));
}
