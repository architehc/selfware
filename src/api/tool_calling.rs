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
use crate::tool_parser::{parse_tool_calls, ParseRejection, ParsedToolCall};

/// Attach tool definitions to a chat-completion request body.
///
/// When `native_function_calling` is true and tools are present, sets:
/// - `body.tools` = the JSON-serialized tool definitions
/// - `body.tool_choice` = `"auto"`
///
/// When `native_function_calling` is false, nothing is attached. The XML
/// prompt path must NOT carry the OpenAI `tools` schema: reasoning / non-FC
/// models and minimalist servers reject the field outright with HTTP 400
/// ("tools parameter is not supported by this model"). Tool definitions
/// reach those models through the XML-protocol system message built by
/// `convert_body_to_xml`, not through the wire body.
///
/// When `tools` is `None`, this is a no-op.
pub fn attach_tools(
    body: &mut serde_json::Value,
    tools: &Option<Vec<ToolDefinition>>,
    native_function_calling: bool,
) {
    let Some(tools) = tools else { return };
    if !native_function_calling {
        return;
    }
    body["tools"] = serde_json::json!(tools);
    body["tool_choice"] = serde_json::json!("auto");
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
pub fn extract_tool_calls(message: &Message, native_function_calling: bool) -> Vec<ToolCall> {
    extract_tool_calls_detailed(message, native_function_calling).calls
}

/// Tool calls extracted from a message, plus the tool-call text the parser
/// could not accept. Rejections are never executed; callers that dispatch
/// tools must report them to the model (see `Agent::collect_tool_calls`).
#[derive(Debug, Default)]
pub struct ExtractedToolCalls {
    pub calls: Vec<ToolCall>,
    pub rejections: Vec<ParseRejection>,
}

/// [`extract_tool_calls`] that also returns the parse rejections of the
/// text-fallback path (native calls carry none).
pub fn extract_tool_calls_detailed(
    message: &Message,
    _native_function_calling: bool,
) -> ExtractedToolCalls {
    if let Some(native) = &message.tool_calls {
        if !native.is_empty() {
            return ExtractedToolCalls {
                calls: native.clone(),
                rejections: Vec::new(),
            };
        }
    }
    extract_tool_calls_from_text_detailed(&message.content.text_all())
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
    extract_tool_calls_from_text_detailed(content).calls
}

/// [`extract_tool_calls_from_text`] that also returns the parse rejections.
pub fn extract_tool_calls_from_text_detailed(content: &str) -> ExtractedToolCalls {
    let parsed = parse_tool_calls(content);
    ExtractedToolCalls {
        calls: parsed
            .tool_calls
            .into_iter()
            .map(parsed_to_tool_call)
            .collect(),
        rejections: parsed.rejections,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/api/tool_calling/tool_calling_test.rs"]
mod tests;
