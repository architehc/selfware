use anyhow::{Context, Result};
use colored::*;
use tracing::{debug, info, warn};

use super::*;
use crate::api::ThinkingMode;

pub(super) struct AssistantStepResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    /// Tool calls returned in the model's `message.tool_calls` field.
    /// These are emitted back as `role=tool` messages and DO get stored on
    /// the assistant history message's `tool_calls` field.
    pub native_tool_calls: Option<Vec<crate::api::types::ToolCall>>,
    /// Tool calls parsed from `<tool>...</tool>` blocks in the assistant's
    /// text content (sglang fallback / mixed-mode thinking models).
    ///
    /// CRITICAL: these MUST NOT be stored as `assistant.tool_calls` in the
    /// conversation history — otherwise the resulting message is shaped
    /// like a native FC call, but the dispatcher emits the result as
    /// `<tool_result>` (role=user). Some endpoints reject the next turn
    /// with "tool_calls without matching tool messages". They are carried
    /// here as a side channel for the dispatch loop to pick up.
    #[allow(dead_code)]
    pub text_fallback_tool_calls: Option<Vec<crate::api::types::ToolCall>>,
    /// Characters of actual content (excludes think blocks).
    #[allow(dead_code)]
    pub content_chars: usize,
    /// Characters inside think/reasoning blocks.
    pub reasoning_chars: usize,
    /// Per-call metadata populated from the live HTTP / SSE layer.
    /// Used by the per-turn debug capture; `None` when this struct was built
    /// from a previously-stored assistant message (no fresh call was made).
    pub metadata: Option<crate::api::types::ChatMetadata>,
}

impl Agent {
    pub(super) async fn get_assistant_step_response(
        &mut self,
        use_last_message: bool,
    ) -> Result<AssistantStepResponse> {
        use crate::api::types::Message;

        let turn_start = std::time::Instant::now();
        let mut native_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;
        let mut text_fallback_tool_calls: Option<Vec<crate::api::types::ToolCall>> = None;
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
            let content_text = last_msg.content.text().to_string();
            let reasoning_clone = last_msg.reasoning_content.clone();
            let response = AssistantStepResponse {
                content_chars: content_text.len(),
                reasoning_chars: reasoning_clone.as_ref().map(|r| r.len()).unwrap_or(0),
                content: content_text,
                reasoning_content: reasoning_clone,
                native_tool_calls,
                // When replaying a stored assistant message, no synthetic
                // tool_calls were ever stored on it (per the new invariant)
                // so any text-format tool calls are still in `content` and
                // will be re-parsed by `collect_tool_calls`.
                text_fallback_tool_calls: None,
                metadata: None,
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

        // Auto-optimize context: downgrade stale files (>120s since last access).
        let optimized = self.context_map.auto_optimize(120);
        if optimized > 0 {
            debug!("Context auto-optimized: freed {} tokens", optimized);
        }

        // Hard-truncate message history to stay within context window before
        // any API call.  This prevents exceeding the model's context limit when
        // compression is skipped or fails.
        self.trim_message_history();

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
                    warn!("Compression failed, using hard limit: {}", e);
                    self.messages = self.compressor.hard_compress(&self.messages);
                    let error_text = e.to_string();
                    self.log_context_compression_event(
                        super::session_log::ContextCompressionLogDetails {
                            strategy: "hard_fallback",
                            success: false,
                            before_messages: before_compression_messages,
                            after_messages: self.messages.len(),
                            before_tokens: before_compression_tokens,
                            after_tokens: self.compressor.estimate_tokens(&self.messages),
                            threshold: compression_threshold,
                            error: Some(&error_text),
                        },
                    );
                }
            }
        }

        let mut request_messages = self.messages.clone();
        let mut system_hints = Vec::new();
        if let Some(learning_hint) = self.build_learning_hint(self.learning_context()) {
            system_hints.push(learning_hint);
        }
        if let Some(failure_hint) = self.pending_failure_hint.take() {
            system_hints.push(failure_hint);
        }

        // Inject context map awareness: L1 tree in system prompt, boundary before recent.
        if self.context_map.file_count() > 0 {
            system_hints.push(self.context_map.render_tree());
        }

        // RAG: inject relevant code chunks from scanned index
        if let Some(ref rag_engine) = self.rag_engine {
            // Query from the TASK objective, not the last user message. The most
            // recent user message is often an injected system directive ("continue
            // working", a tool result, a stuck-loop nudge), which retrieves chunks
            // irrelevant to the actual task. Prefer the task context, then the
            // original (first) user message, then the last user message.
            let task_ctx = self.task_context_for_classification();
            let query = if !task_ctx.is_empty() && task_ctx != "general" {
                task_ctx.to_string()
            } else {
                self.messages
                    .iter()
                    .find(|m| m.role == "user")
                    .or_else(|| self.messages.iter().rev().find(|m| m.role == "user"))
                    .map(|m| m.content.text().to_string())
                    .unwrap_or_default()
            };

            if !query.is_empty() {
                let engine = rag_engine.read().await;
                match engine.retrieve(&query).await {
                    Ok(ctx) if !ctx.context.is_empty() && ctx.token_count > 0 => {
                        let rag_hint = format!(
                            "## Relevant Code Context (RAG)\n\
                             The following code chunks were retrieved from the indexed codebase \
                             based on semantic similarity to the current query. Use them as \
                             reference when answering.\n\n{}",
                            ctx.context
                        );
                        debug!(
                            "RAG injected {} tokens from {} sources ({}ms)",
                            ctx.token_count,
                            ctx.sources.len(),
                            ctx.retrieval_time_ms
                        );
                        system_hints.push(rag_hint);
                    }
                    Ok(_) => {} // No relevant results
                    Err(e) => {
                        debug!("RAG retrieval error (non-fatal): {}", e);
                    }
                }
            }
        }

        if !system_hints.is_empty() {
            let merged_hints = system_hints.join("\n\n");
            // Merge into existing system message to maintain OpenAI message ordering
            // (system messages must precede all user/assistant/tool messages)
            if let Some(first) = request_messages.first_mut() {
                if first.role == "system" {
                    first.content = format!("{}\n\n{}", first.content, merged_hints).into();
                } else {
                    request_messages.insert(0, Message::system(merged_hints));
                }
            } else {
                request_messages.insert(0, Message::system(merged_hints));
            }
        }

        // RoPE-aware: inject context boundary marker before recent messages.
        // This exploits the recency effect — model sees boundary and knows
        // everything above is reference, everything below is active task.
        if self.context_map.file_count() > 0 && request_messages.len() > 8 {
            let boundary = self.context_map.render_boundary();
            // Insert 6 messages from the end (before the recent window).
            let insert_pos = request_messages.len().saturating_sub(6);
            request_messages.insert(insert_pos, Message::user(boundary));
        }

        // Captured per-call metadata (request body, finish_reason, tokens,
        // elapsed_ms) — populated by whichever branch makes the actual call.
        // Stays `None` only if all branches return early via `?`.
        #[allow(unused_assignments)]
        let mut chat_metadata: Option<crate::api::types::ChatMetadata> = None;
        let (content, reasoning) = if self.config.agent.streaming {
            let mut local_meta = crate::api::types::ChatMetadata::default();
            match self
                .chat_streaming(
                    request_messages.clone(),
                    self.api_tools(),
                    ThinkingMode::Enabled,
                    Some(&mut local_meta),
                )
                .await
            {
                Ok((content, reasoning, stream_tool_calls)) => {
                    chat_metadata = Some(local_meta);
                    if self.config.agent.native_function_calling {
                        let has_native = stream_tool_calls
                            .as_ref()
                            .map(|t| !t.is_empty())
                            .unwrap_or(false);

                        if has_native {
                            native_tool_calls = stream_tool_calls.clone();
                            info!(
                                "Received {} native tool calls from stream",
                                native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                            );
                        } else if !content.is_empty() {
                            // Fallback: sglang returns tool_calls:[] but puts
                            // Qwen3-format calls in content. Route through the
                            // unified extractor so this path matches the agent
                            // and SWL runtime parsers exactly.
                            let parsed_calls =
                                crate::api::tool_calling::extract_tool_calls_from_text(&content);
                            if !parsed_calls.is_empty() {
                                info!(
                                    "Native FC returned empty tool_calls; parsed {} from content (sglang fallback)",
                                    parsed_calls.len()
                                );
                                // Preserve native/message-history invariants:
                                // text-fallback calls are dispatched from text,
                                // but must NOT be stored as assistant.tool_calls
                                // because their results are emitted as XML/user
                                // messages rather than role=tool messages.
                                text_fallback_tool_calls = Some(parsed_calls);
                            }
                        }
                    }
                    (content, reasoning)
                }
                Err(stream_err) => {
                    // A shutdown request aborts the in-flight provider call and
                    // surfaces here as an error. Treat it as cancellation — don't
                    // fall back or retry — so the loop saves one checkpoint and
                    // exits cleanly without claiming completion.
                    if self.is_cancelled() {
                        return Err(crate::errors::AgentError::Cancelled.into());
                    }
                    warn!(
                        "Streaming request failed ({}); retrying this step with non-streaming API",
                        stream_err
                    );

                    // Detect "Assistant response prefill incompatible" 400s for
                    // FailureMode classification.
                    if stream_err
                        .to_string()
                        .to_lowercase()
                        .contains("prefill incompatible")
                    {
                        self.note_prefill_400();
                    }

                    let response = self
                        .client
                        .chat_with_meta(request_messages, self.api_tools(), ThinkingMode::Enabled)
                        .await
                        .with_context(|| {
                            format!(
                                "Streaming failed: {}. Non-streaming fallback request also failed",
                                stream_err
                            )
                        });
                    let (response, fallback_meta) = match response {
                        Ok((response, meta)) => (response, meta),
                        Err(e) => {
                            if self.is_cancelled() {
                                return Err(crate::errors::AgentError::Cancelled.into());
                            }
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

                    let message = choice.message;
                    let content = message.content.text().to_string();
                    let reasoning = message.reasoning_content.clone();

                    if self.config.agent.native_function_calling && message.tool_calls.is_some() {
                        native_tool_calls = message.tool_calls.clone();
                        info!(
                            "Received {} native tool calls from fallback API",
                            native_tool_calls.as_ref().map(|t| t.len()).unwrap_or(0)
                        );
                    }

                    debug!(
                        "Fallback model response content ({} chars): {}",
                        content.len(),
                        content
                    );
                    if content.is_empty() {
                        warn!("Fallback model returned empty content!");
                    }
                    if let Some(ref r) = reasoning {
                        cli_println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                        debug!("Fallback reasoning ({} chars): {}", r.len(), r);
                    }

                    chat_metadata = Some(fallback_meta);
                    (content, reasoning)
                }
            }
        } else {
            let response = self
                .client
                .chat_with_meta(request_messages, self.api_tools(), ThinkingMode::Enabled)
                .await;
            let (response, sync_meta) = match response {
                Ok((response, meta)) => (response, meta),
                Err(e) => {
                    if self.is_cancelled() {
                        return Err(crate::errors::AgentError::Cancelled.into());
                    }
                    if e.to_string()
                        .to_lowercase()
                        .contains("prefill incompatible")
                    {
                        self.note_prefill_400();
                    }
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

            let message = choice.message;
            let content = message.content.text().to_string();
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

            if self.config.debug.should_log_responses() {
                cli_println!("{}", "=== DEBUG: Raw Model Response ===".bright_magenta());
                cli_println!("{}", content);
                cli_println!("{}", "=== END DEBUG ===".bright_magenta());
            }

            if content.is_empty() {
                warn!("Model returned empty content!");
            }

            if let Some(ref r) = reasoning {
                cli_println!("{} {}", "Thinking:".dimmed(), r.dimmed());
                debug!("Reasoning content ({} chars): {}", r.len(), r);
            }

            chat_metadata = Some(sync_meta);
            (content, reasoning)
        };

        // Qwen3.5 best practice: "No Thinking Content in History — historical
        // model output should only include the final output part and does not
        // need to include the thinking content."
        // Strip inline <think> blocks from content before storing in message
        // history. Keep the raw content in the response for tool parsing.
        // Qwen3.5 best practice: "No Thinking Content in History — historical
        // model output should only include the final output part and does not
        // need to include the thinking content."
        // Strip inline <think> blocks from content and omit reasoning_content.
        let history_content = super::recovery::strip_think_blocks(&content);

        // Sanitize native tool calls before they enter history. A truncated
        // stream can leave `ToolCallAccumulator::flush` emitting a tool_call
        // whose arguments are not valid JSON (flush only checks id/name).
        // Stored as `assistant.tool_calls` with no matching `role=tool` reply,
        // that call is an unpaired tool_call — which strict backends
        // (vLLM/SGLang) reject with a 400 on the *next* request, sending the
        // run into a recovery death spiral. Dropping the malformed call makes
        // the turn a no-op the loop nudges/retries instead. Sanitizing here
        // (before both the history push and the dispatched response) keeps
        // history and dispatch consistent, and covers native FC from the
        // non-streaming path too.
        if let Some(calls) = native_tool_calls.take() {
            let (kept, dropped) = sanitize_tool_calls(calls);
            if dropped > 0 {
                debug!(
                    "Sanitized {} malformed native tool call(s) from assistant step",
                    dropped
                );
            }
            native_tool_calls = if kept.is_empty() { None } else { Some(kept) };
        }

        // Estimate the prompt size BEFORE appending the assistant reply, so the
        // usage fallback below (used when the backend omits `usage`) doesn't
        // double-count this response in the input total.
        let prompt_token_estimate = self.estimate_messages_tokens();

        self.messages.push(crate::api::types::Message {
            role: "assistant".to_string(),
            content: history_content.into(),
            reasoning_content: None, // Excluded from history per Qwen3.5 best practice
            tool_calls: native_tool_calls.clone(),
            tool_call_id: None,
            name: None,
        });

        // Accumulate token usage from this assistant step. Prefer the provider-
        // reported numbers, but fall back to a tokenizer estimate when the
        // backend omits `usage` (common on local vLLM/SGLang). Without this,
        // cumulative usage never grows on those backends and `max_budget_tokens`
        // silently never trips. The estimate is intentionally conservative — a
        // hard cap should err toward stopping slightly early, not overshooting.
        let reported_prompt = chat_metadata
            .as_ref()
            .and_then(|m| m.prompt_tokens)
            .map(|p| p as usize);
        let reported_completion = chat_metadata
            .as_ref()
            .and_then(|m| m.completion_tokens)
            .map(|c| c as usize);
        let reported_total = chat_metadata.as_ref().and_then(|m| m.total_tokens);

        let output_estimate = crate::token_count::estimate_content_tokens(&content)
            + reasoning
                .as_ref()
                .map(|r| crate::token_count::estimate_content_tokens(r))
                .unwrap_or(0);
        let (input_tokens, output_tokens) = resolve_step_token_counts(
            reported_prompt,
            reported_completion,
            prompt_token_estimate,
            output_estimate,
        );

        self.cumulative_token_usage.input += input_tokens;
        self.cumulative_token_usage.output += output_tokens;
        // Trust a provider-reported total only when it also reported both
        // components; otherwise derive it from the (possibly estimated)
        // running input/output so the total can't disagree with the parts.
        if reported_prompt.is_some() && reported_completion.is_some() {
            if let Some(total) = reported_total {
                self.cumulative_token_usage.total += total as usize;
            } else {
                self.cumulative_token_usage.total =
                    self.cumulative_token_usage.input + self.cumulative_token_usage.output;
            }
        } else {
            self.cumulative_token_usage.total =
                self.cumulative_token_usage.input + self.cumulative_token_usage.output;
        }
        if let Some(cost) = chat_metadata.as_ref().and_then(|m| m.cost) {
            self.cumulative_cost_usd += cost;
        }

        let response = AssistantStepResponse {
            content_chars: content.len(),
            reasoning_chars: reasoning.as_ref().map(|r| r.len()).unwrap_or(0),
            content,
            reasoning_content: reasoning,
            native_tool_calls,
            text_fallback_tool_calls,
            metadata: chat_metadata,
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
}

/// Drop structurally-invalid tool calls (missing id/name, wrong type, or
/// non-JSON arguments) before they enter conversation history.
///
/// See the call site in `get_assistant_step_response` for why an unpaired,
/// malformed tool_call is dangerous (400 death spiral on strict backends).
/// Returns the retained calls and the count dropped.
pub(super) fn sanitize_tool_calls(
    calls: Vec<crate::api::types::ToolCall>,
) -> (Vec<crate::api::types::ToolCall>, usize) {
    let before = calls.len();
    let kept: Vec<_> = calls
        .into_iter()
        .filter(|tc| match tc.validate_structure() {
            Ok(()) => true,
            Err(e) => {
                warn!(
                    "Dropping malformed tool call '{}' before history push: {}",
                    tc.function.name, e
                );
                false
            }
        })
        .collect();
    let dropped = before - kept.len();
    (kept, dropped)
}

/// Resolve a step's (input, output) token counts, preferring provider-reported
/// values and falling back to tokenizer estimates when the backend omits
/// `usage`. Keeping this pure makes the fallback behavior directly testable.
pub(super) fn resolve_step_token_counts(
    reported_prompt: Option<usize>,
    reported_completion: Option<usize>,
    prompt_estimate: usize,
    output_estimate: usize,
) -> (usize, usize) {
    (
        reported_prompt.unwrap_or(prompt_estimate),
        reported_completion.unwrap_or(output_estimate),
    )
}

#[cfg(test)]
mod usage_fallback_tests {
    use super::resolve_step_token_counts;

    #[test]
    fn uses_reported_values_when_present() {
        let (input, output) = resolve_step_token_counts(Some(1200), Some(300), 999, 999);
        assert_eq!(input, 1200);
        assert_eq!(output, 300);
    }

    #[test]
    fn falls_back_to_estimates_when_usage_absent() {
        // Backend omitted usage entirely (common on local vLLM/SGLang).
        let (input, output) = resolve_step_token_counts(None, None, 1500, 220);
        assert_eq!(input, 1500);
        assert_eq!(output, 220);
    }

    #[test]
    fn mixes_reported_and_estimated_per_field() {
        // Provider gave prompt tokens but not completion tokens.
        let (input, output) = resolve_step_token_counts(Some(1200), None, 999, 220);
        assert_eq!(input, 1200);
        assert_eq!(output, 220);
    }
}

#[cfg(test)]
mod sanitize_tool_calls_tests {
    use super::sanitize_tool_calls;
    use crate::api::types::{ToolCall, ToolFunction};

    fn call(id: &str, name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    #[test]
    fn keeps_well_formed_calls() {
        let (kept, dropped) = sanitize_tool_calls(vec![
            call("id1", "file_read", r#"{"path":"a.rs"}"#),
            call("id2", "shell_exec", r#"{"command":"ls"}"#),
        ]);
        assert_eq!(kept.len(), 2);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn drops_call_with_truncated_json_args() {
        // Simulates a stream cut mid-arguments: valid id+name, invalid JSON.
        let (kept, dropped) = sanitize_tool_calls(vec![
            call("id1", "file_read", r#"{"path":"a.rs"}"#),
            call("id2", "file_write", r#"{"path":"b.rs","content":"partial"#),
        ]);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "id1");
    }

    #[test]
    fn drops_call_missing_name_or_id() {
        let (kept, dropped) = sanitize_tool_calls(vec![
            call("", "file_read", "{}"),
            call("id2", "", "{}"),
            call("id3", "file_read", "{}"),
        ]);
        assert_eq!(dropped, 2);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "id3");
    }

    #[test]
    fn empty_input_yields_empty_output() {
        let (kept, dropped) = sanitize_tool_calls(vec![]);
        assert!(kept.is_empty());
        assert_eq!(dropped, 0);
    }
}
