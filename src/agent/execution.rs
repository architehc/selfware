use anyhow::{Context, Result};
use chrono::Utc;
use colored::*;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::*;
use crate::api::ThinkingMode;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::CyclePhase;
use crate::tool_parser::parse_tool_calls;

struct AssistantStepResponse {
    content: String,
    reasoning_content: Option<String>,
    native_tool_calls: Option<Vec<crate::api::types::ToolCall>>,
}

impl Agent {
    /// Execute a step with tool call logging for checkpoints
    /// If `use_last_message` is true, process tool calls from the last assistant message
    /// instead of making a new API call (used after planning phase)
    pub(super) async fn execute_step_with_logging(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(false).await
    }

    /// Execute tool calls from the last assistant message (after planning)
    pub(super) async fn execute_pending_tool_calls(
        &mut self,
        _task_description: &str,
    ) -> Result<bool> {
        self.execute_step_internal(true).await
    }

    /// Internal execution logic
    /// If `use_last_message` is true, process tool calls from the last assistant message
    async fn execute_step_internal(&mut self, use_last_message: bool) -> Result<bool> {
        let response = self.get_assistant_step_response(use_last_message).await?;
        let content = response.content.clone();
        let tool_calls = self.collect_tool_calls(
            &content,
            response.reasoning_content.as_deref(),
            response.native_tool_calls.as_ref(),
        );

        debug!("Total tool calls to execute: {}", tool_calls.len());

        // If no tool calls found, check if content looks like it should have them
        if tool_calls.is_empty()
            && (content.contains("<tool")
                || content.contains("tool_name")
                || content.contains("function"))
        {
            warn!("Content appears to contain tool-related keywords but no valid tool calls were parsed:");
            warn!(
                "Content preview: {}",
                &content.chars().take(500).collect::<String>()
            );
        }

        // If no tool calls but content suggests intent to act, prompt for action
        if self.should_prompt_for_action(&content, tool_calls.is_empty(), use_last_message) {
            info!("Detected intent without action, prompting model to use tools");
            output::intent_without_action();
            self.messages.push(Message::user(
                "Please use the appropriate tools to take action now. Don't just describe what you'll do - actually execute the tools."
            ));
            return Ok(false); // Continue loop to get actual tool calls
        }

        if !tool_calls.is_empty() {
            // Execute tools with logging
            for (name, args_str, tool_call_id) in tool_calls {
                output::tool_call(&name);

                let start_time = std::time::Instant::now();
                let use_native_fc =
                    self.config.agent.native_function_calling && tool_call_id.is_some();

                // Safety check
                let call_id = tool_call_id
                    .clone()
                    .unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));
                let fake_call = crate::api::types::ToolCall {
                    id: call_id.clone(),
                    call_type: "function".to_string(),
                    function: crate::api::types::ToolFunction {
                        name: name.clone(),
                        arguments: args_str.clone(),
                    },
                };

                if let Err(e) = self.safety.check_tool_call(&fake_call) {
                    let error_msg = format!("Safety check failed: {}", e);
                    output::safety_blocked(&error_msg);

                    // Push result with appropriate message type
                    if use_native_fc {
                        self.messages.push(Message::tool(
                            serde_json::json!({"error": error_msg}).to_string(),
                            &call_id,
                        ));
                    } else {
                        self.messages.push(Message::user(format!(
                            "<tool_result><error>{}</error></tool_result>",
                            error_msg
                        )));
                    }

                    // Log failed tool call
                    if let Some(ref mut checkpoint) = self.current_checkpoint {
                        checkpoint.log_tool_call(ToolCallLog {
                            timestamp: Utc::now(),
                            tool_name: name.clone(),
                            arguments: args_str.clone(),
                            result: Some(error_msg),
                            success: false,
                            duration_ms: Some(start_time.elapsed().as_millis() as u64),
                        });
                    }
                    continue;
                }

                // Check execution mode for confirmation
                if self.needs_confirmation(&name) {
                    use std::io::{self, Write};

                    // Show tool call preview
                    let args_preview: String = args_str.chars().take(100).collect();
                    let args_display = if args_str.len() > 100 {
                        format!("{}...", args_preview)
                    } else {
                        args_preview
                    };

                    // In non-interactive mode, fail fast - don't silently skip tools
                    if !self.is_interactive() {
                        return Err(AgentError::ConfirmationRequired {
                            tool_name: name.clone(),
                        }
                        .into());
                    }

                    println!(
                        "{} Tool: {} Args: {}",
                        "⚠️".bright_yellow(),
                        name.bright_cyan(),
                        args_display.bright_white()
                    );
                    print!("{}", "Execute? [y/N/s(kip all)]: ".bright_yellow());
                    io::stdout().flush().ok();

                    let mut response = String::new();
                    if io::stdin().read_line(&mut response).is_ok() {
                        let response = response.trim().to_lowercase();
                        match response.as_str() {
                            "y" | "yes" => {
                                // Proceed with execution
                            }
                            "s" | "skip" => {
                                // Switch to yolo mode for this session
                                self.set_execution_mode(crate::config::ExecutionMode::Yolo);
                                println!(
                                    "{} Switched to YOLO mode for this session",
                                    "⚡".bright_yellow()
                                );
                            }
                            _ => {
                                // User rejected - skip this tool
                                let skip_msg = "Tool execution skipped by user";
                                println!("{} {}", "⏭️".bright_yellow(), skip_msg);

                                if use_native_fc {
                                    self.messages.push(Message::tool(
                                        serde_json::json!({"skipped": skip_msg}).to_string(),
                                        &call_id,
                                    ));
                                } else {
                                    self.messages.push(Message::user(format!(
                                        "<tool_result><skipped>{}</skipped></tool_result>",
                                        skip_msg
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                }

                // Parse and execute
                let args: Value = match serde_json::from_str(&args_str) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = format!("Invalid JSON arguments: {}", e);
                        println!("{} {}", "✗".bright_red(), err);

                        // Push result with appropriate message type
                        if use_native_fc {
                            self.messages.push(Message::tool(
                                serde_json::json!({"error": err}).to_string(),
                                &call_id,
                            ));
                        } else {
                            self.messages.push(Message::user(format!(
                                "<tool_result><error>{}</error></tool_result>",
                                err
                            )));
                        }

                        // Log failed tool call
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_tool_call(ToolCallLog {
                                timestamp: Utc::now(),
                                tool_name: name.clone(),
                                arguments: args_str.clone(),
                                result: Some(err),
                                success: false,
                                duration_ms: Some(start_time.elapsed().as_millis() as u64),
                            });
                        }
                        continue;
                    }
                };

                debug!("Tool arguments: {}", args);

                let (success, result) = match self.tools.get(&name) {
                    Some(tool) => {
                        match tool.execute(args.clone()).await {
                            Ok(result) => {
                                output::tool_success(&name);
                                let result_str = serde_json::to_string(&result)?;

                                // Log successful tool call
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_tool_call(ToolCallLog {
                                        timestamp: Utc::now(),
                                        tool_name: name.clone(),
                                        arguments: args_str.clone(),
                                        result: Some(result_str.chars().take(1000).collect()),
                                        success: true,
                                        duration_ms: Some(start_time.elapsed().as_millis() as u64),
                                    });
                                }

                                // Run verification after file_edit tool
                                let verification_result = if name == "file_edit" {
                                    if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                                        info!("Running verification after file_edit on {}", path);
                                        self.cognitive_state.set_phase(CyclePhase::Verify);
                                        match self
                                            .verification_gate
                                            .verify_change(
                                                &[path.to_string()],
                                                &format!("file_edit:{}", path),
                                            )
                                            .await
                                        {
                                            Ok(report) => {
                                                if report.overall_passed {
                                                    self.cognitive_state
                                                        .episodic_memory
                                                        .what_worked(
                                                            "file_edit",
                                                            &format!(
                                                                "Edit to {} passed verification",
                                                                path
                                                            ),
                                                        );
                                                    output::verification_report(
                                                        &format!("{}", report),
                                                        true,
                                                    );
                                                    None
                                                } else {
                                                    self.cognitive_state
                                                        .episodic_memory
                                                        .what_failed(
                                                            "file_edit",
                                                            &format!(
                                                                "Edit to {} failed verification",
                                                                path
                                                            ),
                                                        );
                                                    output::verification_report(
                                                        &format!("{}", report),
                                                        false,
                                                    );
                                                    Some(format!("\n\n<verification_failed>\n{}\n</verification_failed>", report))
                                                }
                                            }
                                            Err(e) => {
                                                warn!("Verification failed to run: {}", e);
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                                // Enhance cargo_check output with error analysis
                                let enhanced_result = if name == "cargo_check"
                                    && result_str.contains("\"success\":false")
                                {
                                    self.enhance_cargo_errors(&result_str)
                                } else {
                                    result_str.clone()
                                };

                                // Combine result with verification if applicable
                                let final_result = match verification_result {
                                    Some(ver_msg) => format!("{}{}", enhanced_result, ver_msg),
                                    None => enhanced_result,
                                };

                                (true, final_result)
                            }
                            Err(e) => {
                                output::tool_failure(&name, &e.to_string());

                                // Log failed tool call
                                if let Some(ref mut checkpoint) = self.current_checkpoint {
                                    checkpoint.log_tool_call(ToolCallLog {
                                        timestamp: Utc::now(),
                                        tool_name: name.clone(),
                                        arguments: args_str.clone(),
                                        result: Some(e.to_string()),
                                        success: false,
                                        duration_ms: Some(start_time.elapsed().as_millis() as u64),
                                    });
                                }

                                // Record failure in cognitive state
                                self.cognitive_state
                                    .episodic_memory
                                    .what_failed(&name, &e.to_string());

                                (false, e.to_string())
                            }
                        }
                    }
                    None => {
                        let err = format!("Unknown tool: {}", name);
                        println!("{} {}", "✗".bright_red(), err);

                        // Log unknown tool call
                        if let Some(ref mut checkpoint) = self.current_checkpoint {
                            checkpoint.log_tool_call(ToolCallLog {
                                timestamp: Utc::now(),
                                tool_name: name.clone(),
                                arguments: args_str.clone(),
                                result: Some(err.clone()),
                                success: false,
                                duration_ms: Some(start_time.elapsed().as_millis() as u64),
                            });
                        }

                        (false, err)
                    }
                };

                // Push result with appropriate message type
                if use_native_fc {
                    // For native function calling, send JSON result with tool role
                    let result_json = if success {
                        result
                    } else {
                        serde_json::json!({"error": result}).to_string()
                    };
                    self.messages.push(Message::tool(result_json, &call_id));
                } else {
                    // For XML-based calling, wrap in tool_result tags
                    let formatted = if success {
                        format!("<tool_result>{}</tool_result>", result)
                    } else {
                        format!("<tool_result><error>{}</error></tool_result>", result)
                    };
                    self.messages.push(Message::user(formatted));
                }
            }

            Ok(false) // Continue loop
        } else {
            // No tool calls, task complete
            output::final_answer(&content);
            // Note: assistant message was already added above when not using last message
            Ok(true)
        }
    }

    async fn get_assistant_step_response(
        &mut self,
        use_last_message: bool,
    ) -> Result<AssistantStepResponse> {
        let mut native_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;

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
            return Ok(AssistantStepResponse {
                content: last_msg.content.clone(),
                reasoning_content: last_msg.reasoning_content.clone(),
                native_tool_calls,
            });
        }

        if self.compressor.should_compress(&self.messages) {
            info!("Context compression triggered");
            match self.compressor.compress(&self.client, &self.messages).await {
                Ok(compressed) => {
                    self.messages = compressed;
                }
                Err(e) => {
                    warn!("Compression failed, using hard limit: {}", e);
                    self.messages = self.compressor.hard_compress(&self.messages);
                }
            }
        }

        let (content, reasoning) = if self.config.agent.streaming {
            let (content, reasoning, stream_tool_calls) = self
                .chat_streaming(
                    self.messages.clone(),
                    self.api_tools(),
                    ThinkingMode::Enabled,
                )
                .await?;

            if self.config.agent.native_function_calling && stream_tool_calls.is_some() {
                native_tool_calls = stream_tool_calls.clone();
                info!(
                    "Received {} native tool calls from stream",
                    native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                );
            }

            (content, reasoning)
        } else {
            let response = self
                .client
                .chat(
                    self.messages.clone(),
                    self.api_tools(),
                    ThinkingMode::Enabled,
                )
                .await?;

            let choice = response
                .choices
                .into_iter()
                .next()
                .context("No response from model")?;

            let message = choice.message;
            let content = message.content.clone();
            let reasoning = message.reasoning_content.clone();

            if self.config.agent.native_function_calling && message.tool_calls.is_some() {
                native_tool_calls = message.tool_calls.clone();
                info!(
                    "Received {} native tool calls from API",
                    native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                );
            }

            debug!(
                "Raw model response content ({} chars): {}",
                content.len(),
                content
            );

            if std::env::var("SELFWARE_DEBUG").is_ok() {
                println!("{}", "=== DEBUG: Raw Model Response ===".bright_magenta());
                println!("{}", content);
                println!("{}", "=== END DEBUG ===".bright_magenta());
            }

            if content.is_empty() {
                warn!("Model returned empty content!");
            }

            if let Some(ref r) = reasoning {
                println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                debug!("Reasoning content ({} chars): {}", r.len(), r);
            }

            (content, reasoning)
        };

        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: reasoning.clone(),
            tool_calls: native_tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        Ok(AssistantStepResponse {
            content,
            reasoning_content: reasoning,
            native_tool_calls,
        })
    }

    fn collect_tool_calls(
        &self,
        content: &str,
        reasoning_content: Option<&str>,
        native_tool_calls: Option<&Vec<crate::api::types::ToolCall>>,
    ) -> Vec<(String, String, Option<String>)> {
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

    fn should_prompt_for_action(
        &self,
        content: &str,
        has_no_tool_calls: bool,
        use_last_message: bool,
    ) -> bool {
        if !has_no_tool_calls || use_last_message || content.len() >= 1000 {
            return false;
        }

        let intent_phrases = [
            "let me", "i'll ", "i will", "let's", "first,", "starting", "begin by", "going to",
            "need to", "start by", "help you",
        ];
        let content_lower = content.to_lowercase();
        intent_phrases.iter().any(|p| content_lower.contains(p))
    }

    /// Plan phase - returns true if model wants to execute tools (should continue to execution)
    /// This now combines planning with initial tool extraction to avoid double API calls
    pub(super) async fn plan(&mut self) -> Result<bool> {
        // Tools are embedded in system prompt - see WORKAROUND comment in Agent::new()
        debug!("Sending planning request to model...");
        let response = self
            .client
            .chat(
                self.messages.clone(),
                self.api_tools(),
                ThinkingMode::Enabled,
            )
            .await?;

        let choice = response
            .choices
            .into_iter()
            .next()
            .context("No response from model")?;

        let assistant_msg = choice.message;
        let content = &assistant_msg.content;

        // Debug logging for planning response
        debug!(
            "Planning response content ({} chars): {}",
            content.len(),
            content
        );

        // Verbose logging when SELFWARE_DEBUG is set or verbose mode
        output::debug_output("Planning Response", content);

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
        let (has_tool_calls, native_tool_calls) =
            if self.config.agent.native_function_calling && assistant_msg.tool_calls.is_some() {
                let tool_calls = assistant_msg.tool_calls.as_ref().unwrap();
                info!(
                    "Planning response has {} native tool calls",
                    tool_calls.len()
                );
                (!tool_calls.is_empty(), assistant_msg.tool_calls.clone())
            } else {
                let parsed = !parse_tool_calls(content).tool_calls.is_empty();
                debug!("Planning response has tool calls (parsed): {}", parsed);
                (parsed, None)
            };

        self.messages.push(Message {
            role: "assistant".to_string(),
            content: content.clone(),
            reasoning_content: assistant_msg.reasoning_content,
            tool_calls: native_tool_calls,
            tool_call_id: None,
            name: None,
        });

        // Return whether there are tool calls to execute
        Ok(has_tool_calls)
    }
}
