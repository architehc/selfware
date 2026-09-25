use tracing::{debug, info, warn};

use super::*;
use crate::api::tool_calling::{
    extract_tool_calls_detailed, extract_tool_calls_from_text_detailed,
};
use crate::tool_parser::ParseRejection;

pub(super) type CollectedToolCall = (String, String, Option<String>);

impl Agent {
    /// Collect tool calls from a model response into the agent's internal
    /// `(name, args, id)` triples.
    ///
    /// Routes through the unified [`extract_tool_calls`] policy:
    /// - If the message has native `tool_calls`, those are returned (validated).
    /// - Otherwise, the multi-format parser scans `content`, then falls back to
    ///   `reasoning_content` if the content branch yields nothing.
    ///
    /// This replaces the previously-duplicated native-vs-text branching logic
    /// so the agent and the SWL runtime parse responses identically.
    ///
    /// The second element lists tool-call text the parser could not accept
    /// (malformed or mixed syntax). Those calls did NOT run; the caller must
    /// report them to the model as refused calls (see `dispatch_model_batch`).
    /// The reasoning fallback goes through the same parser and is adopted
    /// whole (calls and rejections) when it yields calls.
    pub(super) fn collect_tool_calls(
        &self,
        content: &str,
        reasoning_content: Option<&str>,
        native_tool_calls: Option<&Vec<crate::api::types::ToolCall>>,
    ) -> (Vec<CollectedToolCall>, Vec<ParseRejection>) {
        // Build a synthetic Message we can hand off to the unified extractor.
        let msg = crate::api::types::Message {
            role: "assistant".to_string(),
            content: crate::api::types::MessageContent::from_text(content),
            reasoning_content: reasoning_content.map(|s| s.to_string()),
            tool_calls: native_tool_calls.cloned(),
            tool_call_id: None,
            name: None,
        };

        let detailed = extract_tool_calls_detailed(&msg, self.effective_native_fc());
        let mut extracted = detailed.calls;
        let mut rejections = detailed.rejections;

        // Canonicalize alias argument spellings (old_string → old_str,
        // file_path → path, cmd → command, ...) immediately after extraction:
        // native calls are schema-validated before the tool deserializer, so
        // the serde aliases on the Args structs alone cannot rescue an alias
        // spelling. Doing it here keeps the early validation diagnostic below
        // quiet and hands canonical arguments to dispatch, bookkeeping, and
        // artifacts.
        for tc in extracted.iter_mut() {
            tc.function.arguments = crate::agent::tool_validator::normalize_tool_arg_aliases(
                &tc.function.name,
                &tc.function.arguments,
            );
        }

        // Validate native tool calls against the loaded schema for early
        // diagnostic.  Validation failures are logged but do not abort —
        // individual bad calls will still be rejected at dispatch time.
        if !extracted.is_empty() && msg.tool_calls.as_ref().is_some_and(|c| !c.is_empty()) {
            let defs = self.tools.definitions();
            if let Err(e) = crate::agent::tool_validator::validate_tool_calls(&extracted, &defs) {
                warn!("Native tool call batch validation failed: {}", e);
            }
            extracted.retain(|tc| match tc.validate_structure() {
                Ok(()) => true,
                Err(e) => {
                    warn!(
                        "Skipping malformed native tool call '{}': {}",
                        tc.function.name, e
                    );
                    false
                }
            });
        }

        // Fallback: when the content branch produced nothing (no call and no
        // rejected call markup), scan reasoning_content with the same parser.
        if extracted.is_empty() && rejections.is_empty() {
            if let Some(reasoning_text) = reasoning_content {
                let from_reasoning = extract_tool_calls_from_text_detailed(reasoning_text);
                if !from_reasoning.calls.is_empty() {
                    info!(
                        "Found {} tool calls in reasoning content",
                        from_reasoning.calls.len()
                    );
                    extracted = from_reasoning.calls;
                    rejections = from_reasoning.rejections;
                }
            }
        }
        for rejection in &rejections {
            warn!(
                "Unparsed tool call ({}) will be reported to the model: {}",
                rejection.tool_name.as_deref().unwrap_or("unknown tool"),
                rejection.reason
            );
        }

        if !extracted.is_empty() {
            info!(
                "Extracted {} tool call(s) via unified extractor",
                extracted.len()
            );
        }

        let calls = extracted
            .into_iter()
            .map(|tc| {
                debug!(
                    "Tool call: {} (id: {}) with args: {}",
                    tc.function.name, tc.id, tc.function.arguments
                );
                // Synthetic ids generated by the text-fallback extractor start
                // with `parsed_`. Downstream dispatch code uses
                // `tool_call_id.is_some()` to decide whether to use the native
                // function-calling result-message shape, so we collapse those
                // synthetic ids back to `None` to preserve the prior behaviour.
                let id = if tc.id.is_empty() || tc.id.starts_with("parsed_") {
                    None
                } else {
                    Some(tc.id)
                };
                (tc.function.name, tc.function.arguments, id)
            })
            .collect();
        (calls, rejections)
    }
}
