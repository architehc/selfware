use tracing::{debug, info, warn};

use super::*;
use crate::tool_parser::parse_tool_calls;

pub(super) type CollectedToolCall = (String, String, Option<String>);

impl Agent {
    pub(super) fn message_has_tool_calls(
        &self,
        assistant_msg: &crate::api::types::Message,
    ) -> bool {
        if self.config.agent.native_function_calling
            && assistant_msg
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
        {
            return true;
        }

        !parse_tool_calls(assistant_msg.content.text())
            .tool_calls
            .is_empty()
    }

    pub(super) fn collect_tool_calls(
        &self,
        content: &str,
        reasoning_content: Option<&str>,
        native_tool_calls: Option<&Vec<crate::api::types::ToolCall>>,
    ) -> Vec<(String, String, Option<String>)> {
        if self.config.agent.native_function_calling {
            if let Some(native_calls) = native_tool_calls {
                if !native_calls.is_empty() {
                    info!("Using {} native tool calls from API", native_calls.len());

                    // Validate all native calls structurally and against schemas
                    let defs = self.tools.definitions();
                    if let Err(e) = crate::agent::tool_validator::validate_tool_calls(native_calls, &defs) {
                        warn!("Native tool call batch validation failed: {}", e);
                        // Continue anyway — individual bad calls will be rejected at dispatch time
                    }

                    let mut collected = Vec::with_capacity(native_calls.len());
                    for tc in native_calls {
                        if let Err(e) = tc.validate_structure() {
                            warn!(
                                "Skipping malformed native tool call '{}': {}",
                                tc.function.name, e
                            );
                            continue;
                        }
                        debug!(
                            "Native tool call: {} (id: {}) with args: {}",
                            tc.function.name, tc.id, tc.function.arguments
                        );
                        collected.push((
                            tc.function.name.clone(),
                            tc.function.arguments.clone(),
                            Some(tc.id.clone()),
                        ));
                    }
                    if !collected.is_empty() {
                        return collected;
                    }
                }
            }
        }

        info!(
            "Falling back to XML parsing (native FC returned {} tool calls)",
            native_tool_calls.map(|t| t.len()).unwrap_or(0)
        );
        debug!("Looking for tool calls with multi-format parser...");

        let parse_result = parse_tool_calls(content);
        let mut tool_calls: Vec<(String, String, Option<String>)> = parse_result
            .tool_calls
            .iter()
            .map(|tc| {
                debug!(
                    "Found tool call in content: {} with args: {}",
                    tc.tool_name, tc.arguments
                );
                (tc.tool_name.clone(), tc.arguments.to_string(), None)
            })
            .collect();

        for error in &parse_result.parse_errors {
            warn!("Tool parse error: {}", error);
        }

        if tool_calls.is_empty() {
            if let Some(reasoning_text) = reasoning_content {
                let reasoning_result = parse_tool_calls(reasoning_text);
                let reasoning_tools: Vec<(String, String, Option<String>)> = reasoning_result
                    .tool_calls
                    .iter()
                    .map(|tc| {
                        debug!(
                            "Found tool call in reasoning: {} with args: {}",
                            tc.tool_name, tc.arguments
                        );
                        (tc.tool_name.clone(), tc.arguments.to_string(), None)
                    })
                    .collect();
                if !reasoning_tools.is_empty() {
                    info!(
                        "Found {} tool calls in reasoning content",
                        reasoning_tools.len()
                    );
                    tool_calls = reasoning_tools;
                }
            }
        }

        tool_calls
    }
}
