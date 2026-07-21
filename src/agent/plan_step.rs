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

        // Per-turn debug logging — gated on the unified `--debug=turns` channel
        // (or the legacy SELFWARE_DEBUG / SELFWARE_DEBUG_TURNS env vars).
        // Verbose mode also forces it on for interactive use.
        output::debug_output(&self.config.debug, "Planning Response", content.text());

        if content.is_empty() {
            warn!("Model returned empty planning content!");
        }

        // When plan mode is active, try to parse a structured plan from the
        // model's response and store it for UI review / execution tracking.
        if self.plan_mode {
            let plan_text = content.text();
            if let Some(plan) = super::plan_mode::parse_plan_from_llm(plan_text) {
                info!("Parsed structured plan with {} step(s)", plan.steps.len());
                self.store_plan(plan);
                self.plan_mode_manager.store_plan_text(plan_text);
            } else {
                // Even if parsing fails, keep the raw text so the user can
                // review the model's prose plan.
                self.plan_mode_manager.store_plan_text(plan_text);
            }
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
        // Uses the unified extractor so native and text-fallback paths agree.
        let has_tool_calls = !crate::api::tool_calling::extract_tool_calls(
            &assistant_msg,
            self.config.agent.native_function_calling,
        )
        .is_empty();
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

        // Sanitize before these enter history AND before they're dispatched
        // below: a truncated stream can leave a tool_call with invalid-JSON
        // args, and storing it as assistant.tool_calls with no matching
        // role=tool reply produces an unpaired tool_call that strict backends
        // 400 on the next request — the same hazard fixed in
        // get_assistant_step_response, on the planning path.
        let native_tool_calls = native_tool_calls.and_then(|calls| {
            let (kept, dropped) = super::assistant_response::sanitize_tool_calls(calls);
            if dropped > 0 {
                debug!("Sanitized {} malformed planning tool call(s)", dropped);
            }
            (!kept.is_empty()).then_some(kept)
        });

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
        let parsed_calls_for_artifact: Vec<crate::api::types::ToolCall> =
            native_tool_calls.clone().unwrap_or_default();
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
        )
        .await;

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

        // Accumulate token usage from this planning call. Delta-add (never
        // total = input + output): after a resume, `total` carries the
        // restored prior-run budget whose input/output split was not
        // persisted, so a from-parts recompute would silently erase it.
        let step_input = plan_meta.prompt_tokens.map(|p| p as usize).unwrap_or(0);
        let step_output = plan_meta.completion_tokens.map(|c| c as usize).unwrap_or(0);
        self.cumulative_token_usage.input += step_input;
        self.cumulative_token_usage.output += step_output;
        self.cumulative_token_usage.total += plan_meta
            .total_tokens
            .map(|t| t as usize)
            .unwrap_or(step_input + step_output);

        // Accumulate cost from this planning call so USD caps account for
        // planning spend, not just execution spend.
        if let Some(cost) = plan_meta.cost {
            self.cumulative_cost_usd += cost;
        }

        // Return whether there are tool calls to execute
        Ok(has_tool_calls)
    }

    /// A planning response with no tool calls may already BE the final answer
    /// for a plain chat / analysis task. Apply the same sanity gates the
    /// execution path applies before accepting a tool-less answer
    /// (execution.rs:650-687): substantial cleaned length, not confused, not
    /// narrating pending work, and the completion gate accepts it.
    ///
    /// Mutation tasks never finalize here — a task whose deliverable is an
    /// edit must still go plan → execute (the EmptyDiff / NoSourceEdit /
    /// TestOnlyPatch gates would reject it anyway; classifying first keeps
    /// the edit path bit-identical to before). Plan mode is likewise left
    /// untouched: the user asked to review before anything is accepted.
    ///
    /// Returns the cleaned answer when it can be accepted as the final one.
    pub(super) async fn planning_answer_ready_to_finalize(&mut self) -> Option<String> {
        if self.plan_mode || self.current_task_requires_mutation() {
            return None;
        }
        // The planning response is the assistant message plan() just pushed.
        let content = self
            .messages
            .last()
            .filter(|m| m.role == "assistant")
            .map(|m| m.content.text().to_string())?;
        let clean = super::recovery::strip_think_blocks(&content)
            .trim()
            .to_string();
        if clean.len() < 40
            || super::verification::is_confused_response(&content)
            || super::verification::is_incomplete_action_response(&content)
        {
            return None;
        }
        // The completion gate inspects `last_assistant_response`
        // (incomplete-action / exact-target / capability-disclaimer checks),
        // so point it at the candidate answer before asking.
        self.last_assistant_response = clean.clone();
        if self.check_completion_gate().await.is_some() {
            return None;
        }
        Some(clean)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/plan_step/plan_step_test.rs"]
mod tests;
