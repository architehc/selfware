use anyhow::{Context, Result};
use tracing::{debug, info, warn};

use super::*;
use crate::api::ThinkingMode;

impl Agent {
    /// Plan phase - returns true if model wants to execute tools (should continue to execution)
    /// This now combines planning with initial tool extraction to avoid double API calls
    pub(super) async fn plan(&mut self) -> Result<bool> {
        use crate::api::types::Message;

        // Tools are embedded in system prompt - see WORKAROUND comment in Agent::new()
        debug!("Sending planning request to model...");
        let turn_start = std::time::Instant::now();
        self.log_turn_start_event("planning", false, self.messages.len());
        self.trim_message_history();
        let mut request_messages = self.messages.clone();
        if let Some(learning_hint) = self.build_learning_hint(self.learning_context()) {
            // Merge into existing system message to maintain OpenAI message ordering
            if let Some(first) = request_messages.first_mut() {
                if first.role == "system" {
                    first.content = format!("{}\n\n{}", first.content, learning_hint).into();
                } else {
                    request_messages.insert(0, Message::system(learning_hint));
                }
            } else {
                request_messages.insert(0, Message::system(learning_hint));
            }
        }
        // Capture per-call metadata so the planning step also gets a
        // turn_NNNN.json artifact under <workdir>/.selfware/turns/.
        let mut plan_meta = crate::api::types::ChatMetadata::default();
        // Use streaming for planning so the user sees progress and can cancel.
        // Non-streaming blocks silently for 60+ seconds while the model thinks.
        let assistant_msg = if self.config.agent.streaming {
            match self
                .chat_streaming(
                    request_messages.clone(),
                    self.api_tools(),
                    ThinkingMode::Enabled,
                    Some(&mut plan_meta),
                )
                .await
            {
                Ok((content, reasoning, tool_calls)) => crate::api::types::Message {
                    role: "assistant".to_string(),
                    content: content.into(),
                    reasoning_content: reasoning,
                    tool_calls,
                    tool_call_id: None,
                    name: None,
                },
                Err(e) => {
                    self.log_turn_end_event(
                        "planning",
                        false,
                        false,
                        turn_start.elapsed().as_millis() as u64,
                        Some(e.to_string()),
                        serde_json::json!({
                            "message_count": self.messages.len(),
                            "estimated_message_tokens": self.estimate_messages_tokens(),
                        }),
                    );
                    return Err(e);
                }
            }
        } else {
            let response = self
                .client
                .chat_with_meta(request_messages, self.api_tools(), ThinkingMode::Enabled)
                .await;
            let response = match response {
                Ok((response, meta)) => {
                    plan_meta = meta;
                    response
                }
                Err(e) => {
                    self.log_turn_end_event(
                        "planning",
                        false,
                        false,
                        turn_start.elapsed().as_millis() as u64,
                        Some(e.to_string()),
                        serde_json::json!({
                            "message_count": self.messages.len(),
                            "estimated_message_tokens": self.estimate_messages_tokens(),
                        }),
                    );
                    return Err(e);
                }
            };

            response
                .choices
                .into_iter()
                .next()
                .context("No response from model")?
                .message
        };
        let content = &assistant_msg.content;

        // Debug logging for planning response
        debug!(
            "Planning response content ({} chars): {}",
            content.len(),
            content
        );

        // Verbose logging when SELFWARE_DEBUG is set or verbose mode
        output::debug_output("Planning Response", content.text());

        if content.is_empty() {
            warn!("Model returned empty planning content!");
        }

        if let Some(ref reasoning) = assistant_msg.reasoning_content {
            debug!(
                "Planning reasoning ({} chars): {}",
                reasoning.len(),
                reasoning
            );
            if let Some(r) = &assistant_msg.reasoning_content {
                output::thinking(r, false);
            }
        }

        // Check if the planning response contains tool calls
        // For native function calling, check tool_calls field; otherwise parse from content
        let has_tool_calls = self.message_has_tool_calls(&assistant_msg);
        let native_tool_calls = if let (true, Some(tool_calls)) = (
            self.config.agent.native_function_calling,
            assistant_msg.tool_calls.as_ref(),
        ) {
            info!(
                "Planning response has {} native tool calls",
                tool_calls.len()
            );
            assistant_msg.tool_calls.clone()
        } else {
            debug!(
                "Planning response has tool calls (parsed): {}",
                has_tool_calls
            );
            None
        };

        // Snapshot reasoning_content before the message-push moves it, so the
        // turn artifact can capture the model's <think> output too.
        let reasoning_for_artifact = assistant_msg.reasoning_content.clone();
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: assistant_msg.reasoning_content,
            tool_calls: native_tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        // Per-turn debug capture for the planning step. Increment the
        // counter so this becomes turn_0001.json (planning is always the
        // first LLM call of a task).
        self.turn_artifact_seq += 1;
        let plan_step_idx = self.turn_artifact_seq;
        let parsed_calls_for_artifact: Vec<crate::api::types::ToolCall> = native_tool_calls
            .clone()
            .unwrap_or_default();
        let plan_decision = if parsed_calls_for_artifact.is_empty() {
            super::turn_artifacts::AgentDecision::NoToolCall
        } else {
            super::turn_artifacts::AgentDecision::ExecutedTools {
                tools: parsed_calls_for_artifact
                    .iter()
                    .map(|c| c.function.name.clone())
                    .collect(),
            }
        };
        // plan_meta.request_body is empty when the streaming branch took an
        // error path before assigning it; write_turn_artifact handles that
        // by checking for an empty body.
        let plan_meta_opt = if plan_meta.request_body.is_null()
            || plan_meta
                .request_body
                .as_object()
                .map(|o| o.is_empty())
                .unwrap_or(true)
        {
            None
        } else {
            Some(plan_meta.clone())
        };
        self.write_turn_artifact(
            plan_step_idx,
            plan_meta_opt.as_ref(),
            &parsed_calls_for_artifact,
            plan_decision,
            content.text(),
            reasoning_for_artifact.as_deref(),
        );

        self.log_turn_end_event(
            "planning",
            false,
            true,
            turn_start.elapsed().as_millis() as u64,
            None,
            serde_json::json!({
                "content_chars": content.len(),
                "has_tool_calls": has_tool_calls,
                "native_tool_calls": self.messages.last().and_then(|m| m.tool_calls.as_ref()).map(|calls| calls.len()).unwrap_or(0),
                "message_count": self.messages.len(),
                "estimated_message_tokens": self.estimate_messages_tokens(),
            }),
        );

        // Return whether there are tool calls to execute
        Ok(has_tool_calls)
    }
}
