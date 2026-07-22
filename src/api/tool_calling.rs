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
pub fn extract_tool_calls(message: &Message, _native_function_calling: bool) -> Vec<ToolCall> {
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


#[cfg(test)]
#[path = "../../tests/unit/api/tool_calling/tool_calling_test.rs"]
mod tests;
