//! Message handling module - handles message building, tool results, and context management

use anyhow::{Context, Result};
use tracing::{debug, warn};

use super::*;
use crate::api::types::Message;
use crate::checkpoint::ToolCallLog;
use crate::api::ApiClient;
use chrono::Utc;

/// Try to extract a `base64_png` field from a JSON tool result string.
pub(super) fn try_extract_base64_png(result: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(result)
        .ok()?
        .get("base64_png")?
        .as_str()
        .map(String::from)
}

/// Build a text summary of a JSON tool result by removing the large `base64_png`
/// blob and adding an `"image_attached": true` marker.
pub(super) fn build_image_result_summary(result: &str) -> String {
    if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(result) {
        if let Some(obj) = v.as_object_mut() {
            obj.remove("base64_png");
            obj.insert("image_attached".to_string(), serde_json::Value::Bool(true));
        }
        serde_json::to_string(&v).unwrap_or_else(|_| result.to_string())
    } else {
        result.to_string()
    }
}

impl Agent {
    /// Push a tool result message to the conversation
    pub(super) fn push_tool_result_message(
        &mut self,
        use_native_fc: bool,
        call_id: &str,
        _tool_name: &str,
        success: bool,
        result: &str,
    ) {
        // Detect base64_png in successful tool results and promote to multimodal
        if success {
            if let Some(base64_png) = try_extract_base64_png(result) {
                let summary = build_image_result_summary(result);
                let content =
                    crate::api::types::MessageContent::from_text(&summary).with_image(&base64_png);
                if use_native_fc {
                    // For native FC, create a tool message with multimodal content
                    self.messages.push(crate::api::types::Message {
                        role: "tool".to_string(),
                        content,
                        reasoning_content: None,
                        tool_calls: None,
                        tool_call_id: Some(call_id.to_string()),
                        name: None,
                    });
                } else {
                    self.messages.push(Message::user_multimodal(content));
                }
                return;
            }
        }

        if use_native_fc {
            let result_json = if success {
                result.to_string()
            } else {
                serde_json::json!({"error": result}).to_string()
            };
            self.messages.push(Message::tool(result_json, call_id));
        } else {
            let formatted = if success {
                format!("<tool_result>{}</tool_result>", result)
            } else {
                format!("<tool_result><error>{}</error></tool_result>", result)
            };
            self.messages.push(Message::user(formatted));
        }
    }

    /// Log a tool call to the checkpoint and session logger
    pub(super) fn log_tool_call(
        &mut self,
        tool_name: &str,
        arguments: &str,
        result: &str,
        success: bool,
        start_time: std::time::Instant,
        truncate_result: bool,
    ) {
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.log_session_tool_call_event(
            tool_name,
            arguments,
            result,
            success,
            duration_ms,
            truncate_result,
        );

        if let Some(ref mut checkpoint) = self.current_checkpoint {
            let logged_result = if truncate_result {
                result.chars().take(1000).collect()
            } else {
                result.to_string()
            };
            checkpoint.log_tool_call(ToolCallLog {
                timestamp: Utc::now(),
                tool_name: tool_name.to_string(),
                arguments: arguments.to_string(),
                result: Some(logged_result),
                success,
                duration_ms: Some(duration_ms),
            });
        }
    }

    /// Get the assistant step response, either from last message or by making an API call
    async fn get_assistant_step_response(
        &mut self,
        use_last_message: bool,
    ) -> Result<AssistantStepResponse> {
        let turn_start = std::time::Instant::now();
        let mut native_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;
        self.log_turn_start_event("assistant_step", use_last_message, self.messages.len());

        if use_last_message {
            let last_msg = self
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "assistant")
                .context("No previous assistant message found")?;
            debug!(
                "Using content from last assistant message ({} chars)",
                last_msg.content.len()
            );
            if self.config.agent.native_function_calling {
                native_tool_calls = last_msg.tool_calls.clone();
            }
            let response = AssistantStepResponse {
                content: last_msg.content.text().to_string(),
                reasoning_content: last_msg.reasoning_content.clone(),
                native_tool_calls,
            };
            self.log_turn_end_event(
                "assistant_step",
                true,
                true,
                turn_start.elapsed().as_millis() as u64,
                None,
                serde_json::json!({
                    "content_chars": response.content.len(),
                    "reasoning_chars": response.reasoning_content.as_ref().map(|r| r.len()).unwrap_or(0),
                    "native_tool_calls": response.native_tool_calls.as_ref().map(|calls| calls.len()).unwrap_or(0),
                    "message_count": self.messages.len(),
                    "estimated_message_tokens": self.estimate_messages_tokens(),
                }),
            );
            return Ok(response);
        }

        // Hard-truncate message history to stay within context window before
        // any API call.  This prevents exceeding the model's context limit when
        // compression is skipped or fails.
        self.trim_message_history();

        // Three-Layer Context Compression: Check and auto-trigger if needed
        if let Some(metrics) = self.compact_auto_trigger().await {
            info!("Auto-triggered compression: {}", metrics.summary());
        }

        let compression_threshold = self.compressor.compression_threshold();
        let before_compression_messages = self.messages.len();
        let before_compression_tokens = self.compressor.estimate_tokens(&self.messages);
        if self.compressor.should_compress(&self.messages) {
            info!("Context compression triggered");
            match self.compressor.compress(&self.client, &self.messages).await {
                Ok(compressed) => {
                    self.messages = compressed;
                    self.log_context_compression_event(
                        super::session_log::ContextCompressionLogDetails {
                            strategy: "summary",
                            success: true,
                            before_messages: before_compression_messages,
                            after_messages: self.messages.len(),
                            before_tokens: before_compression_tokens,
                            after_tokens: self.compressor.estimate_tokens(&self.messages),
                            threshold: compression_threshold,
                            error: None,
                        },
                    );
                }
                Err(e) => {
                    warn!("Context compression failed: {}", e);
                    self.log_context_compression_event(
                        super::session_log::ContextCompressionLogDetails {
                            strategy: "summary",
                            success: false,
                            before_messages: before_compression_messages,
                            after_messages: self.messages.len(),
                            before_tokens: before_compression_tokens,
                            after_tokens: self.compressor.estimate_tokens(&self.messages),
                            threshold: compression_threshold,
                            error: Some(e.to_string()),
                        },
                    );
                }
            }
        }

        // Build request messages with learning hint
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

        // Make the API call
        let response = self
            .client
            .chat(request_messages, self.api_tools(), ThinkingMode::Enabled)
            .await;

        let response = match response {
            Ok(response) => response,
            Err(e) => {
                self.log_turn_end_event(
                    "assistant_step",
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

        let choice = response
            .choices
            .into_iter()
            .next()
            .context("No response from model")?;

        let assistant_msg = choice.message;
        let content = &assistant_msg.content;
        let reasoning = assistant_msg.reasoning_content;
        let tool_calls = if self.config.agent.native_function_calling {
            assistant_msg.tool_calls.clone()
        } else {
            None
        };

        debug!(
            "Assistant step response: {} chars, {} tool calls",
            content.len(),
            tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
        );

        // Push the assistant message to the conversation
        self.messages.push(Message {
            role: "assistant".to_string(),
            content: assistant_msg.content.clone(),
            reasoning_content: assistant_msg.reasoning_content.clone(),
            tool_calls: assistant_msg.tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        let response = AssistantStepResponse {
            content: content.text().to_string(),
            reasoning_content: reasoning.clone(),
            native_tool_calls: tool_calls.clone(),
        };

        self.log_turn_end_event(
            "assistant_step",
            false,
            true,
            turn_start.elapsed().as_millis() as u64,
            None,
            serde_json::json!({
                "content_chars": response.content.len(),
                "reasoning_chars": response.reasoning_content.as_ref().map(|r| r.len()).unwrap_or(0),
                "native_tool_calls": response.native_tool_calls.as_ref().map(|calls| calls.len()).unwrap_or(0),
                "message_count": self.messages.len(),
                "estimated_message_tokens": self.estimate_messages_tokens(),
            }),
        );

        Ok(response)
    }

    /// Check if an assistant message has tool calls
    pub(super) fn message_has_tool_calls(&self, assistant_msg: &Message) -> bool {
        if self.config.agent.native_function_calling
            && assistant_msg
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
        {
            return true;
        }

        !crate::tool_parser::parse_tool_calls(assistant_msg.content.text())
            .tool_calls
            .is_empty()
    }

    /// Collect tool calls from content, reasoning, and/or native tool calls
    pub(super) fn collect_tool_calls(
        &self,
        content: &str,
        reasoning_content: Option<&str>,
        native_tool_calls: Option<&Vec<crate::api::types::ToolCall>>,
    ) -> Vec<CollectedToolCall> {
        if self.config.agent.native_function_calling
            && native_tool_calls.is_some_and(|calls| !calls.is_empty())
        {
            let native_calls = native_tool_calls.unwrap();
            info!("Using {} native tool calls from API", native_calls.len());
            return native_calls
                .iter()
                .map(|tc| {
                    debug!(
                        "Native tool call: {} (id: {}) with args: {}",
                        tc.function.name, tc.id, tc.function.arguments
                    );
                    (
                        tc.function.name.clone(),
                        tc.function.arguments.clone(),
                        Some(tc.id.clone()),
                    )
                })
                .collect();
        }

        info!(
            "Falling back to XML parsing (native FC returned {} tool calls)",
            native_tool_calls.map(|t| t.len()).unwrap_or(0)
        );
        debug!("Looking for tool calls with multi-format parser...");

        let parse_result = crate::tool_parser::parse_tool_calls(content);
        let mut tool_calls: Vec<CollectedToolCall> = parse_result
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
                let reasoning_result = crate::tool_parser::parse_tool_calls(reasoning_text);
                let reasoning_tools: Vec<CollectedToolCall> = reasoning_result
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
