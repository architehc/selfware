use anyhow::{bail, Context, Result};
use tracing::{debug, info, warn};

use super::*;
use crate::api::ThinkingMode;

impl Agent {
    /// Plan phase - returns true if model wants to execute tools (should continue to execution)
    /// This now combines planning with initial tool extraction to avoid double API calls
    #[cfg(test)] // test entry point; the task loop calls `plan_with_thinking`
    pub(super) async fn plan(&mut self) -> Result<bool> {
        self.plan_with_thinking(ThinkingMode::Enabled).await
    }

    /// [`Agent::plan`] with an explicit thinking mode: the planning loop in
    /// `task_runner` passes `ThinkingMode::StepDown` for its single bounded
    /// retry after a `ReasoningBudgetExhausted` planning turn.
    pub(super) async fn plan_with_thinking(&mut self, thinking: ThinkingMode) -> Result<bool> {
        use crate::api::types::Message;

        // Tools are embedded in system prompt - see WORKAROUND comment in Agent::new()
        debug!("Sending planning request to model...");
        let turn_start = std::time::Instant::now();
        self.log_turn_start_event("planning", false, self.messages.len());
        self.trim_message_history();
        // Same request assembly as the execution path (assistant_response.rs):
        // per-turn content (the learning hint, the work ledger) travels in the
        // `<selfware_context_note kind=turn_context>` tail at the END of the
        // request, and mid-conversation system banners are demoted to user
        // notes, so the system message is byte-identical to the execution
        // requests. Merging the hint into the system message here made the
        // planning request's prefix differ from every execution request's.
        //
        // `finish_request_with_tail` fits the history into the budget left
        // after the measured tail and returns the typed ContextOverflow when
        // it still does not fit (the planning retry loop routes it to bounded
        // compress-and-retry recovery).
        let turn_hints: Vec<String> = self
            .build_learning_hint(self.learning_context())
            .into_iter()
            .collect();
        self.sync_path_key_root();
        let compressor = &self.compressor;
        let request_messages = Self::finish_request_with_tail_and_ledger(
            self.messages.clone(),
            turn_hints,
            &|history, cap| compressor.render_work_ledger_for(cap, history),
            self.max_context_tokens,
            self.current_checkpoint.as_ref(),
            &compressor.path_keys(),
        )?;
        // Capture per-call metadata so the planning step also gets a
        // turn_NNNN.json artifact under <workdir>/.selfware/turns/.
        let mut plan_meta = crate::api::types::ChatMetadata::default();
        // Use streaming for planning so the user sees progress and can cancel.
        // Non-streaming blocks silently for 60+ seconds while the model thinks.
        //
        // Streaming renders reasoning deltas as they arrive (streaming.rs); the
        // non-streaming arms render nothing. Track which one ran so the
        // assembled block below is only printed when nothing streamed it —
        // printing it after a streamed turn put the whole chain of thought in
        // the transcript twice.
        //
        // `force_non_streaming` latches after a streamed turn came back empty
        // (the empty-response recovery in execution.rs): the streaming path is
        // the one that produced nothing, so the planning retry must use the
        // path that did rather than repeat the failing request. Ignoring the
        // latch here re-burned the empty stream on the very next call.
        let mut reasoning_streamed = false;
        let assistant_msg = if self.config.agent.streaming && !self.force_non_streaming {
            match self
                .chat_streaming(
                    request_messages.clone(),
                    self.api_tools(),
                    thinking,
                    Some(&mut plan_meta),
                )
                .await
            {
                Ok((content, reasoning, tool_calls)) => {
                    reasoning_streamed = true;
                    crate::api::types::Message {
                        role: "assistant".to_string(),
                        content: content.into(),
                        reasoning_content: reasoning,
                        tool_calls,
                        tool_call_id: None,
                        name: None,
                    }
                }
                Err(e) => {
                    // A streaming failure that a non-streaming retry could
                    // plausibly survive must fall back here too. Planning was
                    // the path a cross-distribution install test found: a
                    // provider returning an empty stream killed the run at
                    // planning, three streamed requests and zero non-streamed,
                    // because this arm returned the error directly while the
                    // identical guard in assistant_response.rs fell back.
                    if super::assistant_response::is_terminal_api_client_error(&e)
                        || self.is_cancelled()
                    {
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

                    warn!(
                        "Streaming planning request failed ({}); retrying this step with non-streaming API",
                        e
                    );
                    let fallback = self
                        .await_nonstreaming_llm(self.client.chat_with_meta(
                            request_messages,
                            self.api_tools(),
                            thinking,
                        ))
                        .await
                        .with_context(|| {
                            format!(
                                "Streaming planning failed: {e}. Non-streaming fallback request also failed"
                            )
                        });
                    match fallback {
                        Ok((response, meta)) => {
                            plan_meta = meta;
                            response
                                .choices
                                .into_iter()
                                .next()
                                .context("No response from model")?
                                .message
                        }
                        Err(fallback_err) => {
                            self.log_turn_end_event(
                                "planning",
                                false,
                                false,
                                turn_start.elapsed().as_millis() as u64,
                                Some(fallback_err.to_string()),
                                serde_json::json!({
                                    "message_count": self.messages.len(),
                                    "estimated_message_tokens": self.estimate_messages_tokens(),
                                }),
                            );
                            return Err(fallback_err);
                        }
                    }
                }
            }
        } else {
            let response = self
                .await_nonstreaming_llm(self.client.chat_with_meta(
                    request_messages,
                    self.api_tools(),
                    thinking,
                ))
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
        // Same contract as the execution turn (assistant_response.rs): a
        // planning response cut off by the completion budget whose only output
        // is a reasoning trace is TRUNCATED, not a plan — fail typed so the
        // planning loop's bounded step-down retry can recover it.
        if plan_meta.finish_reason.as_deref() == Some("length")
            && assistant_msg.content.text().trim().is_empty()
            && assistant_msg
                .reasoning_content
                .as_ref()
                .is_some_and(|r| !r.trim().is_empty())
        {
            let reasoning_chars = assistant_msg
                .reasoning_content
                .as_ref()
                .map(|r| r.trim().len())
                .unwrap_or(0);
            let err: anyhow::Error = crate::errors::ApiError::ReasoningBudgetExhausted {
                reasoning_chars,
                retry: None,
            }
            .into();
            self.log_turn_end_event(
                "planning",
                false,
                false,
                turn_start.elapsed().as_millis() as u64,
                Some(err.to_string()),
                serde_json::json!({
                    "message_count": self.messages.len(),
                    "estimated_message_tokens": self.estimate_messages_tokens(),
                }),
            );
            return Err(err);
        }

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
            // A streamed turn already showed these deltas live, so printing the
            // assembled block again duplicated the entire chain of thought.
            // Only the non-streaming paths need it.
            if !reasoning_streamed {
                output::thinking(reasoning, false);
            }
        }

        // Check if the planning response contains tool calls
        // Uses the unified extractor so native and text-fallback paths agree.
        let has_tool_calls = !crate::api::tool_calling::extract_tool_calls(
            &assistant_msg,
            self.effective_native_fc(),
        )
        .is_empty();
        let native_tool_calls = if let (true, Some(tool_calls)) = (
            self.effective_native_fc(),
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
            let (kept, dropped) = super::assistant_response::sanitize_tool_calls_reporting(calls);
            if !dropped.is_empty() {
                debug!(
                    "Sanitized {} malformed planning tool call(s)",
                    dropped.len()
                );
            }
            // Reported by the dispatch that follows, like the execution step.
            self.pending_native_rejections = dropped;
            (!kept.is_empty()).then_some(kept)
        });

        // An empty planning response is a provider hiccup, not a plan. Apply
        // the same bounded recovery the execution step uses (execution.rs):
        // count it toward the consecutive-empty streak, latch
        // `force_non_streaming` so the retry takes the path that delivered
        // tokens, and after two consecutive empties stop with a typed reason
        // instead of silently planning from nothing. Classified here — before
        // the history push — so an empty planning turn leaves no garbage
        // assistant message behind, and any non-empty planning response (with
        // or without tool calls) ends the streak.
        if !has_tool_calls {
            let clean_final = super::recovery::strip_think_blocks(content.text())
                .trim()
                .to_string();
            if clean_final.is_empty() {
                // Record the empty provider attempt before any exit so the
                // turn artifact and end event keep the request/finish-reason
                // evidence for this provider failure (review: empty planning
                // responses bypassed diagnostic recording).
                self.turn_artifact_seq += 1;
                let plan_step_idx = self.turn_artifact_seq;
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
                let empty_calls: Vec<crate::api::types::ToolCall> = Vec::new();
                self.write_turn_artifact(
                    plan_step_idx,
                    plan_meta_opt.as_ref(),
                    &empty_calls,
                    super::turn_artifacts::AgentDecision::NoToolCall,
                    "",
                    None,
                )
                .await;
                self.log_turn_end_event(
                    "planning",
                    false,
                    true,
                    turn_start.elapsed().as_millis() as u64,
                    None,
                    serde_json::json!({
                        "content_chars": 0,
                        "has_tool_calls": false,
                        "rejected": "empty_planning_response",
                    }),
                );

                self.consecutive_empty_responses += 1;
                if self.consecutive_empty_responses
                    >= super::recovery::MAX_CONSECUTIVE_EMPTY_RESPONSES
                {
                    let reasoning_chars = assistant_msg
                        .reasoning_content
                        .as_deref()
                        .map(|r| r.trim().len())
                        .unwrap_or(0)
                        + content.text().trim().len();
                    bail!(
                        "{}",
                        super::recovery::empty_response_loop_message(
                            self.consecutive_empty_responses,
                            reasoning_chars,
                        )
                    );
                }
                if self.config.agent.streaming && !self.force_non_streaming {
                    self.force_non_streaming = true;
                    info!(
                        "Empty planning response — retrying the next turn with streaming \
                         disabled (the streamed request produced nothing)"
                    );
                }
                info!("Rejected empty planning response — nudging for an actual plan");
                self.messages.push(Message::user(
                    "<selfware_system_directive>\n\
                     Your last planning response was empty. Provide your actual plan now: \
                     name the files you will change and the tools you will call.\n\
                     </selfware_system_directive>"
                        .to_string(),
                ));
                return Ok(false);
            }
            // Any non-empty planning response ends the empty streak.
            self.consecutive_empty_responses = 0;
        }
        // Snapshot reasoning_content before the message-push moves it, so the
        // turn artifact can capture the model's <think> output too.
        let reasoning_for_artifact = assistant_msg.reasoning_content.clone();
        let history_msg = super::assistant_response::build_assistant_history_message(
            content.text(),
            assistant_msg.reasoning_content,
            native_tool_calls.clone(),
            self.config.preserve_thinking(),
        );
        self.messages.push(history_msg);

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
        self.sync_api_usage();

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
        // Read-only alone is not enough to demand grounding: a general-knowledge
        // answer ("explain how a hash map works") is still accepted from the
        // planning turn in one request. The gate applies when the task asks
        // about THIS workspace's code, where an answer produced without opening
        // anything came from the prompt rather than from the code.
        if self.current_task_is_read_only() && self.total_tool_call_count() == 0 {
            let project_name = super::current_project_root()
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            if super::task_policy::task_references_project_code(
                self.task_context_for_classification(),
                &project_name,
            ) {
                const UNGROUNDED_REASON: &str = "read-only report without any read";
                let already_asked = self
                    .messages
                    .iter()
                    .any(|m| m.content.text().contains(UNGROUNDED_REASON));
                if !already_asked {
                    self.messages.push(crate::api::types::Message::system(
                        super::task_policy::policy_envelope(
                            super::task_policy::PolicyKind::Gate,
                            true,
                            UNGROUNDED_REASON,
                            "This is a read-only report about this workspace, but nothing has \
                             been read yet. Read the relevant files with your tools first and \
                             cite file:line from what you actually opened, then deliver the \
                             report.",
                        ),
                    ));
                }
                return None;
            }
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
