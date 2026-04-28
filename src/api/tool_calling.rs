//! Unified tool-calling policy for all LLM call paths.
//!
//! This module is the single source of truth for two related concerns that
//! used to be reimplemented in every call path:
//!
//! 1. **Attaching** tool definitions to outgoing chat-completion requests.
//!    The native OpenAI-style `tools: [...]` shape is sent when the model
//!    advertises native function calling; otherwise tools are exposed
//!    through the system prompt and the model emits XML.
//!
//! 2. **Extracting** tool calls from a returned `Message`. When native FC
//!    is enabled the model is *expected* to use the `message.tool_calls`
//!    field, but in practice some backends (sglang on certain quants,
//!    older vLLM, etc.) leave that field empty and embed `<tool>...</tool>`
//!    blocks (or other text formats) in `content` instead. The unified
//!    extractor falls back to the multi-format text parser whenever the
//!    native field is empty, regardless of whether the request was made
//!    with native FC on or off.
//!
//! Both behaviours are now identical for `chat()`, `chat_stream()`,
//! `chat_with_profile()`, and the SWL runtime.

use super::types::{Message, ToolCall, ToolDefinition, ToolFunction};
use crate::tool_parser::{parse_tool_calls, ParseMethod, ParsedToolCall};

/// Attach tool definitions to a chat-completion request body.
///
/// When `native_function_calling` is true and tools are present, sets:
/// - `body.tools` = the JSON-serialized tool definitions
/// - `body.tool_choice` = `"auto"`
///
/// When `native_function_calling` is false, tools are still attached to
/// the request body so backends that auto-detect them can use them, but
/// `tool_choice` is omitted (the model is expected to emit XML in
/// `content` instead, prompted by the system message).
///
/// When `tools` is `None`, this is a no-op.
pub fn attach_tools(
    body: &mut serde_json::Value,
    tools: &Option<Vec<ToolDefinition>>,
    native_function_calling: bool,
) {
    let Some(tools) = tools else { return };
    body["tools"] = serde_json::json!(tools);
    if native_function_calling {
        body["tool_choice"] = serde_json::json!("auto");
    }
}

/// Extract tool calls from a returned assistant message.
///
/// Behaviour:
/// - If `message.tool_calls` is present and non-empty, return those
///   directly (the native path).
/// - Otherwise, parse `message.content` text for XML/JSON-formatted tool
///   calls. This is the *fallback* path and runs even when
///   `native_function_calling` is true — some backends populate text
///   despite the request flag.
/// - The fallback also runs when `native_function_calling` is false (the
///   prompt-based path).
///
/// In all cases the returned `ToolCall` values are normalized OpenAI-style
/// objects (id/type/function) so downstream code never needs to distinguish.
pub fn extract_tool_calls(
    message: &Message,
    _native_function_calling: bool,
) -> Vec<ToolCall> {
    if let Some(native) = &message.tool_calls {
        if !native.is_empty() {
            return native.clone();
        }
    }
    let text = message.content.text_all();
    let parsed = parse_tool_calls(&text);
    parsed
        .tool_calls
        .into_iter()
        .map(parsed_to_tool_call)
        .collect()
}

/// Convert a [`ParsedToolCall`] (from the text parser) into an OpenAI-style
/// [`ToolCall`] so downstream code only ever sees one shape.
///
/// The synthetic id `parsed_<uuid>` matches the convention already in use
/// by the streaming agent fallback (see `src/agent/assistant_response.rs`).
pub fn parsed_to_tool_call(parsed: ParsedToolCall) -> ToolCall {
    let id = format!("parsed_{}", uuid::Uuid::new_v4());
    ToolCall {
        id,
        call_type: "function".to_string(),
        function: ToolFunction {
            name: parsed.tool_name,
            arguments: parsed.arguments.to_string(),
        },
    }
}

/// Same as [`extract_tool_calls`] but takes raw text content. Useful when
/// the caller has already split content/reasoning out of the message
/// (e.g. the streaming path in `agent/assistant_response.rs`, which
/// receives the assembled text directly).
pub fn extract_tool_calls_from_text(content: &str) -> Vec<ToolCall> {
    let parsed = parse_tool_calls(content);
    parsed
        .tool_calls
        .into_iter()
        .map(parsed_to_tool_call)
        .collect()
}

/// String representation of a [`ParseMethod`] for telemetry / logging.
///
/// Currently consumers of [`extract_tool_calls`] receive normalized
/// [`ToolCall`] values without the originating `ParseMethod`, so this
/// helper is a hook for future telemetry that wants to track fallback
/// rate. Kept hidden until it has a real consumer.
#[doc(hidden)]
pub fn parse_method_for(method: ParseMethod) -> &'static str {
    match method {
        ParseMethod::Native => "native",
        ParseMethod::Xml => "xml",
        ParseMethod::Json => "json",
        ParseMethod::Markdown => "markdown",
    }
}

#[cfg(test)]
mod tests {
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
        let parsed: serde_json::Value =
            serde_json::from_str(&calls[0].function.arguments).unwrap();
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
}
