//! Tool execution module - handles parsing, validation, and execution of tool calls

use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use serde_json::Value;
use tracing::{debug, warn};

use super::*;
use crate::api::types::Message;
use crate::checkpoint::ToolCallLog;
use crate::cognitive::self_improvement::Outcome;
use crate::errors::AgentError;
use crate::hooks::{HookAction, HookContext};
use chrono::Utc;

type CollectedToolCall = (String, String, Option<String>);
const TOOL_CONFIRM_ARGS_PREVIEW_CHARS: usize = 240;
const TOOL_FAILURE_HINT_PREVIEW_CHARS: usize = 400;
const FAILED_TOOL_ATTEMPT_WINDOW_SIZE: usize = 16;

/// Truncate a string to max_chars, adding ellipsis if truncated
pub(super) fn truncate_chars(s: &str, max_chars: usize) -> String {
    let collected: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        format!("{}...", collected)
    } else {
        collected
    }
}

/// Canonicalize tool arguments by normalizing JSON formatting
pub(super) fn canonicalize_tool_args(args_str: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args_str)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| args_str.to_string())
}

/// Hash tool arguments for deduplication and detection
pub(super) fn hash_tool_args(args_str: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonicalize_tool_args(args_str).hash(&mut hasher);
    hasher.finish()
}

/// Record a failed tool attempt for repetition detection
impl Agent {
    pub(super) fn record_failed_tool_attempt(
        &mut self,
        tool_name: &str,
        args_str: &str,
        failure_kind: &'static str,
        error: &str,
    ) {
        let args_hash = hash_tool_args(args_str);
        let error_preview = truncate_chars(error, TOOL_FAILURE_HINT_PREVIEW_CHARS);

        self.recent_failed_tool_attempts.push_back(FailedToolAttempt {
            tool_name: tool_name.to_string(),
            args_hash,
            failure_kind,
            error_preview,
        });

        // Maintain fixed-size window
        while self.recent_failed_tool_attempts.len() > FAILED_TOOL_ATTEMPT_WINDOW_SIZE {
            self.recent_failed_tool_attempts.pop_front();
        }
    }

    /// Clear the failed tool attempts window
    pub(super) fn clear_failed_tool_attempts(&mut self) {
        self.recent_failed_tool_attempts.clear();
    }

    /// Build context for a tool call (call_id, use_native_fc flag, fake_call string)
    fn build_tool_call_context(
        &self,
        name: &str,
        args_str: &str,
        tool_call_id: Option<String>,
    ) -> (String, bool, String) {
        let use_native_fc = self.config.agent.native_function_calling;
        let call_id = tool_call_id.unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4()));

        let fake_call = if use_native_fc {
            format!(r#"{{"id":"{}","function{{"name":"{}","arguments":{}}}}"#, call_id, name, args_str)
        } else {
            format!(r#"{{"name":"{}","arguments":{}}}"#, name, args_str)
        };

        (call_id, use_native_fc, fake_call)
    }

    /// Parse tool arguments, handling errors and pushing error messages
    fn parse_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> Option<Value> {
        let args = match serde_json::from_str::<Value>(args_str) {
            Ok(v) => v,
            Err(e) => {
                warn!("Invalid JSON for tool {} arguments: {}", name, e);
                let error_msg = format!("Invalid JSON arguments: {}", e);
                self.push_tool_result_message(use_native_fc, call_id, name, false, &error_msg);
                self.log_tool_call(name, args_str, &error_msg, false, start_time, false);
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return None;
            }
        };
        Some(args)
    }

    /// Validate tool arguments, handling errors and pushing error messages
    fn validate_tool_args(
        &mut self,
        name: &str,
        args_str: &str,
        args: &Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> bool {
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => {
                warn!("Unknown tool: {}", name);
                let error_msg = format!("Unknown tool: {}", name);
                self.push_tool_result_message(use_native_fc, call_id, name, false, &error_msg);
                self.log_tool_call(name, args_str, &error_msg, false, start_time, false);
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return false;
            }
        };

        // Validate required arguments
        if let Some(req) = tool.schema().get("required").and_then(|v| v.as_array()) {
            let missing: Vec<&str> = req
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|&field| args.get(field).is_none())
                .collect();

            if !missing.is_empty() {
                let error_msg = format!("Missing required arguments: {:?}", missing);
                warn!("Tool {} missing arguments: {:?}", name, missing);
                self.push_tool_result_message(use_native_fc, call_id, name, false, &error_msg);
                self.log_tool_call(name, args_str, &error_msg, false, start_time, false);
                self.emit_event(AgentEvent::ToolCompleted {
                    name: name.clone(),
                    success: false,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                });
                return false;
            }
        }

        true
    }

    /// Execute a single tool and handle the result
    async fn execute_single_tool(
        &mut self,
        name: &str,
        args_str: &str,
        args: Value,
        call_id: &str,
        use_native_fc: bool,
        start_time: std::time::Instant,
    ) -> Result<()> {
        let tool = self.tools.get(name).ok_or_else(|| {
            anyhow::anyhow!("Tool not found: {}. Available tools: {}", name, self.tools.list().iter().map(|t| t.name()).collect::<Vec<_>>().join(", "))
        })?;

        // Hook: before_tool
        let hook_context = HookContext {
            task: &self.current_task_context,
            tool_name: name,
            tool_args: &args_str,
        };
        let action = self.hook_registry.before_tool(&hook_context);
        if matches!(action, HookAction::Skip) {
            info!("Hook skipped tool: {}", name);
            return Ok(());
        }

        // Check for tool result cache hit
        let cached_result = self.cache_manager.tool_cache.get(name, &args);
        let result = if let Some(cached) = cached_result {
            debug!("Tool result cache hit for {}", name);
            cached
        } else {
            // Execute the tool
            let result = tool.execute(&args).await;

            // Cache read-only tool results
            if tool.is_readonly() {
                self.cache_manager.tool_cache.insert(name, &args, &result);
            }

            result
        };

        let (success, result_str) = match result {
            Ok(v) => (true, v),
            Err(e) => {
                let err_str = e.to_string();
                warn!("Tool {} failed: {}", name, err_str);
                (false, err_str)
            }
        };

        // Enhance tool result if needed (e.g., cargo_check errors)
        let enhanced_result = self.maybe_enhance_tool_result(name, &result_str);

        // Push tool result message
        self.push_tool_result_message(use_native_fc, call_id, name, success, &enhanced_result);

        // Log the tool call
        self.log_tool_call(name, args_str, &result_str, success, start_time, true);

        // Track task state after tool execution
        self.track_task_state_after_tool(name, &args_str, success);

        // Emit event
        let duration_ms = start_time.elapsed().as_millis() as u64;
        self.emit_event(AgentEvent::ToolCompleted {
            name: name.to_string(),
            success,
            duration_ms,
        });

        // Record in self-improvement
        self.self_improvement.record_tool(
            name,
            self.learning_context(),
            if success { Outcome::Success } else { Outcome::Failure },
            duration_ms,
            if !success { Some(result_str) } else { None },
        );

        // Record error if failed
        if !success {
            self.remember_failed_tool(name, &result_str);
            self.record_failed_tool_attempt(name, args_str, "execution", &result_str);
            self.self_improvement.record_error(
                &result_str,
                "tool",
                self.learning_context(),
                name,
                None,
            );
        }

        // Hook: after_tool
        let hook_context = HookContext {
            task: &self.current_task_context,
            tool_name: name,
            tool_args: &args_str,
        };
        let _ = self.hook_registry.after_tool(&hook_context, success);

        Ok(())
    }
}
