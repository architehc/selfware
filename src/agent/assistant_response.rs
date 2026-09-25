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
    /// Accumulate a NON-streaming response's token usage into the session-wide
    /// counters and display, mirroring what the streaming path does on its
    /// `StreamChunk::Usage` arm (see `agent/streaming.rs`). Without this, runs
    /// with `agent.streaming = false` — and streaming-failure fallback calls —
    /// never fed `output::get_total_tokens()`, so `/cost` and the session
    /// stats showed 0 tokens for the entire run.
    fn record_nonstreaming_usage(&self, usage: &crate::api::types::Usage) {
        let prompt = usage.prompt_tokens as u64;
        let completion = usage.completion_tokens as u64;
        output::record_tokens(prompt, completion);
        output::print_token_usage(prompt, completion);
        self.emit_event(AgentEvent::TokenUsage {
            prompt_tokens: prompt,
            completion_tokens: completion,
        });
    }

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
            if self.effective_native_fc() {
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

        // Work ledger: one turn per model request; record the successful
        // tool results the last step appended before trim/compaction can
        // drop them (trim_message_history and the compressors observe too).
        self.compressor.begin_ledger_turn(self.current_task_text());
        self.compressor.observe_work(&self.messages);

        // Hard-truncate message history to stay within context window before
        // any API call.  This prevents exceeding the model's context limit when
        // compression is skipped or fails.
        self.trim_message_history();

        let compression_threshold = self.compressor.compression_threshold();
        let before_compression_messages = self.messages.len();
        let before_compression_tokens = self.compressor.estimate_tokens(&self.messages);
        if self.compressor.should_compress(&self.messages) {
            info!("Context compression triggered");
            match self
                .compressor
                .compress_with_task(&self.client, &self.messages, self.current_task_text())
                .await
            {
                Ok((compressed, _usage)) => {
                    let after_tokens = self.compressor.estimate_tokens(&compressed);
                    let did_compress = after_tokens < before_compression_tokens
                        || compressed.len() < before_compression_messages;
                    if did_compress {
                        self.messages = compressed;
                        self.sync_api_usage();
                        self.log_context_compression_event(
                            super::session_log::ContextCompressionLogDetails {
                                strategy: "summary",
                                success: true,
                                before_messages: before_compression_messages,
                                after_messages: self.messages.len(),
                                before_tokens: before_compression_tokens,
                                after_tokens,
                                threshold: compression_threshold,
                                error: None,
                            },
                        );
                    } else {
                        warn!("Context compression summary yielded no size reduction, using hard fallback");
                        self.messages = self
                            .compressor
                            .hard_compress_with_task(&self.messages, self.current_task_text());
                        let final_tokens = self.compressor.estimate_tokens(&self.messages);
                        self.sync_api_usage();
                        self.log_context_compression_event(
                            super::session_log::ContextCompressionLogDetails {
                                strategy: "hard_fallback",
                                success: final_tokens < before_compression_tokens,
                                before_messages: before_compression_messages,
                                after_messages: self.messages.len(),
                                before_tokens: before_compression_tokens,
                                after_tokens: final_tokens,
                                threshold: compression_threshold,
                                error: Some("summary yielded no reduction"),
                            },
                        );
                    }
                }
                Err(e) => {
                    warn!("Compression failed, using hard limit: {}", e);
                    self.messages = self
                        .compressor
                        .hard_compress_with_task(&self.messages, self.current_task_text());
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

        // Whatever the compressors did, the task must still be in the history
        // (fit_request_to_context_budget pins it in the request as well).
        self.ensure_task_anchor_present();

        let mut request_messages = self.messages.clone();
        let mut turn_hints = Vec::new();
        if let Some(learning_hint) = self.build_learning_hint(self.learning_context()) {
            turn_hints.push(learning_hint);
        }
        if let Some(failure_hint) = self.pending_failure_hint.take() {
            turn_hints.push(failure_hint);
        }

        // Inject context map awareness: L1 tree in the request tail, boundary before recent.
        if self.context_map.file_count() > 0 {
            turn_hints.push(self.context_map.render_tree());
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
                        // Cap RAG context chunks dynamically based on max_context_tokens
                        // so small context windows (24k/40k) are not overwhelmed by retrieved code.
                        let max_rag_tokens = (self.max_context_tokens / 16).clamp(500, 4000);
                        let context_str = if ctx.token_count > max_rag_tokens {
                            let chars: Vec<char> = ctx.context.chars().collect();
                            let target_chars = (chars.len() as f64
                                * (max_rag_tokens as f64 / ctx.token_count as f64))
                                as usize;
                            chars.into_iter().take(target_chars).collect::<String>()
                                + "\n...[RAG context truncated to fit budget]"
                        } else {
                            ctx.context
                        };
                        let rag_hint = format!(
                            "## Relevant Code Context (RAG)\n\
                             The following code chunks were retrieved from the indexed codebase \
                             based on semantic similarity to the current query. Use them as \
                             reference when answering.\n\n{}",
                            context_str
                        );
                        debug!(
                            "RAG injected {} tokens from {} sources ({}ms)",
                            ctx.token_count.min(max_rag_tokens),
                            ctx.sources.len(),
                            ctx.retrieval_time_ms
                        );
                        turn_hints.push(rag_hint);
                    }
                    Ok(_) => {} // No relevant results
                    Err(e) => {
                        debug!("RAG retrieval error (non-fatal): {}", e);
                    }
                }
            }
        }

        // The per-turn hints above are NOT merged into the system message any
        // more: they change every turn (token counts, failure hints, RAG), so
        // merging them made the system prompt differ on every request and
        // defeated any provider prefix cache. They travel in the request tail
        // (see `finish_request_with_tail`) together with the work ledger.

        // RoPE-aware: inject context boundary marker before recent messages.
        // This exploits the recency effect — model sees boundary and knows
        // everything above is reference, everything below is active task.
        if self.context_map.file_count() > 0 && request_messages.len() > 8 {
            let boundary = self.context_map.render_boundary();
            // Insert 6 messages from the end (before the recent window) — but
            // NEVER split an assistant(tool_calls) / role=tool result pair:
            // OpenAI-compatible endpoints (OpenAI/vLLM/SGLang) reject with
            // HTTP 400 any payload whose tool results do not IMMEDIATELY
            // follow their assistant message. When the history ends on an
            // OPEN pair (assistant tool_calls whose results have not been
            // appended yet), skip the insertion entirely so the results can
            // land right behind the call on the next dispatch step.
            if let Some(insert_pos) = context_boundary_insert_pos(&request_messages) {
                request_messages.insert(insert_pos, Message::user(boundary));
            }
        }

        // Whatever the source history contained, the assembled request must
        // keep every assistant(tool_calls) directly followed by its tool
        // results — re-run the pairing invariants after boundary injection so
        // an orphaned pair can never reach the provider (HTTP 400).
        request_messages = Agent::apply_tool_call_pair_invariants(request_messages);

        // Ensure the fully-assembled request (including injected hints, project tree, and RAG)
        // stays strictly within the context budget so small context windows (24k/40k) never
        // overflow: trim, then hard-clamp (text AND historical tool-call arguments). If the
        // measured payload is STILL over budget, do not dispatch — return the typed
        // ContextOverflow so the loop's bounded compress-and-retry recovery handles it.
        //
        // The history is fitted into the budget LEFT AFTER the per-turn tail
        // (hints + work ledger, measured), and the tail is attached at the
        // very end of the request — never in the system message.
        let ledger = self
            .compressor
            .render_work_ledger(super::context::work_ledger_token_cap(
                self.max_context_tokens,
            ));
        request_messages = Self::finish_request_with_tail(
            request_messages,
            turn_hints,
            ledger,
            self.max_context_tokens,
            self.current_checkpoint.as_ref(),
        )?;

        // Captured per-call metadata (request body, finish_reason, tokens,
        // elapsed_ms) — populated by whichever branch makes the actual call.
        // Stays `None` only if all branches return early via `?`.
        #[allow(unused_assignments)]
        let mut chat_metadata: Option<crate::api::types::ChatMetadata> = None;
        // Whole-call wall time of a SUCCESSFUL streamed call (send → stream
        // end). `ChatMetadata::elapsed_ms` on the streamed path is only
        // time-to-headers, so the zero-content long-call check needs its own
        // clock. `None` for the non-streaming path, whose client already
        // types a long empty call as `ZeroContentLongCall`.
        let mut streamed_call_elapsed_ms: Option<u64> = None;
        // `force_non_streaming` latches after a streamed turn came back empty: the
        // streaming path is the one that produced nothing, so the retry uses the
        // path that did not rather than repeating the failing request.
        let (content, reasoning) = if self.config.agent.streaming && !self.force_non_streaming {
            let mut local_meta = crate::api::types::ChatMetadata::default();
            let stream_call_started = std::time::Instant::now();
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
                    streamed_call_elapsed_ms =
                        Some(stream_call_started.elapsed().as_millis() as u64);
                    if self.effective_native_fc() {
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
                        return Err(crate::errors::AgentError::for_current_shutdown().into());
                    }

                    // Detect "Assistant response prefill incompatible" 400s for
                    // FailureMode classification.
                    if stream_err
                        .to_string()
                        .to_lowercase()
                        .contains("prefill incompatible")
                    {
                        self.note_prefill_400();
                    }

                    // A terminal 4xx (e.g. 401 from a missing/invalid API key)
                    // fails identically over the non-streaming endpoint —
                    // re-issuing it would just double-hit the provider and bury
                    // the remediation hint under a fallback error. Only fall
                    // back for transport/streaming-level failures where a
                    // non-streaming retry could plausibly succeed.
                    // A context-window overflow is equally identical over the
                    // non-streaming endpoint (same payload); surface it to the
                    // loop's compression recovery instead of re-sending it.
                    let context_overflow = crate::errors::is_context_overflow_error(&stream_err);
                    if is_terminal_api_client_error(&stream_err) || context_overflow {
                        warn!(
                            "Streaming request failed with {} ({}); not falling back to non-streaming",
                            if context_overflow {
                                "context-window overflow"
                            } else {
                                "terminal client error"
                            },
                            stream_err
                        );
                        self.log_turn_end_event(
                            "assistant_step",
                            false,
                            false,
                            turn_start.elapsed().as_millis() as u64,
                            Some(stream_err.to_string()),
                            serde_json::json!({
                                "message_count": self.messages.len(),
                                "estimated_message_tokens": self.estimate_messages_tokens(),
                            }),
                        );
                        return Err(stream_err);
                    }

                    warn!(
                        "Streaming request failed ({}); retrying this step with non-streaming API",
                        stream_err
                    );

                    let response = self
                        .await_nonstreaming_llm(self.client.chat_with_meta(
                            request_messages,
                            self.api_tools(),
                            ThinkingMode::Enabled,
                        ))
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
                                return Err(
                                    crate::errors::AgentError::for_current_shutdown().into()
                                );
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

                    // The fallback is a NON-streaming response: its usage never
                    // passes through the SSE usage arm, so record it here.
                    self.record_nonstreaming_usage(
                        fallback_meta
                            .accounted_usage
                            .as_ref()
                            .unwrap_or(&response.usage),
                    );

                    let choice = response
                        .choices
                        .into_iter()
                        .next()
                        .context("No response from model")?;

                    let message = choice.message;
                    let content = message.content.text().to_string();
                    let reasoning = message.reasoning_content.clone();

                    if self.effective_native_fc() && message.tool_calls.is_some() {
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
                .await_nonstreaming_llm(self.client.chat_with_meta(
                    request_messages,
                    self.api_tools(),
                    ThinkingMode::Enabled,
                ))
                .await;
            let (response, sync_meta) = match response {
                Ok((response, meta)) => (response, meta),
                Err(e) => {
                    if self.is_cancelled() {
                        return Err(crate::errors::AgentError::for_current_shutdown().into());
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

            // A non-streaming response never produces a `StreamChunk::Usage`
            // event, so accumulate its usage into the session totals here.
            self.record_nonstreaming_usage(
                sync_meta
                    .accounted_usage
                    .as_ref()
                    .unwrap_or(&response.usage),
            );

            let choice = response
                .choices
                .into_iter()
                .next()
                .context("No response from model")?;

            let message = choice.message;
            let content = message.content.text().to_string();
            let reasoning = message.reasoning_content.clone();

            if self.effective_native_fc() && message.tool_calls.is_some() {
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

        // A response cut off by the completion budget (finish_reason ==
        // "length") whose only output is a reasoning trace — no answer text —
        // is a TRUNCATED turn, not a deliverable. The reasoning is a partial
        // trace, and feeding it through the promotion below would store the
        // truncated reasoning as the final answer (2026-09-21 review, P2:
        // on the streamed path the length-truncated reasoning was promoted
        // to content and accepted by the earlier completion gates, bypassing
        // execution's length rejection). Fail typed here — same contract as
        // the non-streaming client's `reasoning_budget_exhausted` — so the
        // two paths share one semantic: reasoning-only + length is
        // `ReasoningBudgetExhausted`, never a completed turn.
        if chat_metadata
            .as_ref()
            .and_then(|m| m.finish_reason.as_deref())
            == Some("length")
            && content.trim().is_empty()
            && reasoning.as_ref().is_some_and(|r| !r.trim().is_empty())
        {
            let reasoning_chars = reasoning.as_ref().map(|r| r.trim().len()).unwrap_or(0);
            return Err(
                crate::errors::ApiError::ReasoningBudgetExhausted { reasoning_chars }.into(),
            );
        }

        // Tag-free abliterated models: the qwen3 reasoning parser can
        // classify the ENTIRE response as reasoning_content, leaving content
        // empty — a loop reading only content sees a zero-turn forever
        // (server-side analysis 2026-09-04; matches the ablit-wave "zeros").
        // The tool-call extractor already falls back to reasoning
        // (tool_collect.rs); promote the turn text ONLY when tool markup
        // or native tool calls are present. Never promote pure reasoning
        // monologue as the final answer text (Ranked #1: reasoning-only long
        // stream becoming final answer when content is empty).
        let mut content = content;
        if content.trim().is_empty() {
            if let Some(r) = reasoning.as_ref().filter(|r| !r.trim().is_empty()) {
                let has_tool_markup = r.contains("<tool")
                    || r.contains("<function=")
                    || r.contains("<|open|>call")
                    || r.contains("<tool_call>");
                if has_tool_markup || native_tool_calls.is_some() {
                    info!(
                        "Content empty but reasoning_content has tool calls ({} chars) — promoting to content",
                        r.len()
                    );
                    content = r.clone();
                } else {
                    debug!(
                        "Content empty and reasoning_content contains no tool calls ({} chars) — not promoting to content",
                        r.len()
                    );
                }
            }
        }

        // Streamed mirror of the non-streaming client's zero-content
        // long-call outcome: a streamed call that COMPLETED after a long wait
        // but delivered no answer and no tool call must fail typed with the
        // elapsed time named, not pass through as an empty turn. The
        // reasoning-bearing shape gets its own variant so the report never
        // says "no reasoning" when the model did think.
        if let Some(elapsed_ms) = streamed_call_elapsed_ms {
            let has_tool_calls = native_tool_calls.as_ref().is_some_and(|c| !c.is_empty())
                || text_fallback_tool_calls
                    .as_ref()
                    .is_some_and(|c| !c.is_empty());
            if let Some(err) = streamed_long_empty_call_outcome(
                elapsed_ms,
                self.client.zero_content_long_call_threshold_ms(),
                &content,
                reasoning.as_deref(),
                has_tool_calls,
            ) {
                return Err(err.into());
            }
        }

        // Qwen3.5 best practice / thinking retention: historical model output
        // strips inline <think> blocks before storage in message history.
        // Retention of reasoning_content in history is gated by `preserve_thinking`
        // (via build_assistant_history_message). Keep raw content in the response
        // for tool parsing.

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

        let history_msg = build_assistant_history_message(
            &content,
            reasoning.clone(),
            native_tool_calls.clone(),
            self.config.preserve_thinking(),
        );
        self.messages.push(history_msg);

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

        self.sync_api_usage();

        // Validate raw provider measurements before fallback estimation or aggregation (Rule 3)
        if let Some(meta) = chat_metadata.as_ref() {
            if let (Some(p), Some(c), Some(t)) = (
                meta.prompt_tokens,
                meta.completion_tokens,
                meta.total_tokens,
            ) {
                if t != 0 && (p as usize).saturating_add(c as usize) != t as usize {
                    warn!(
                        "Provider reported unreconciled token usage: prompt={} + completion={} != total={}",
                        p, c, t
                    );
                }
            }
        }
        // Missing provider fields still use the measured content-token fallback.
        // Reported components have already been charged by the attempt ledger.
        let already_accounted = chat_metadata
            .as_ref()
            .is_some_and(|meta| meta.accounted_usage.is_some());
        let reported_total = chat_metadata
            .as_ref()
            .and_then(|m| m.total_tokens)
            .map(|t| t as usize);
        let estimated_input = if !already_accounted && reported_prompt.is_none() {
            input_tokens
        } else {
            0
        };
        let estimated_output = if !already_accounted && reported_completion.is_none() {
            output_tokens
        } else {
            0
        };
        // Reconcile missing estimated components against what the ledger has already
        // accounted for this step (Rule 4). If the provider reported a total_tokens
        // that already covers the step's expected total, adding estimated output/input
        // directly to cumulative total would double-count those tokens.
        let missing_total = if !already_accounted {
            let step_expected_total = input_tokens.saturating_add(output_tokens);
            let step_accounted_total = reported_total.unwrap_or(0).max(
                reported_prompt
                    .unwrap_or(0)
                    .saturating_add(reported_completion.unwrap_or(0)),
            );
            step_expected_total.saturating_sub(step_accounted_total)
        } else {
            0
        };
        self.cumulative_token_usage.input += estimated_input;
        self.cumulative_token_usage.output += estimated_output;
        self.cumulative_token_usage.total += missing_total;
        self.cumulative_token_usage.total = self.cumulative_token_usage.total.max(
            self.cumulative_token_usage
                .input
                .saturating_add(self.cumulative_token_usage.output),
        );
        self.client
            .ensure_budget_floor(self.cumulative_token_usage.total, self.cumulative_cost_usd);

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

/// True when the error chain contains an API HTTP-status error that the HTTP
/// client already classified as terminal: a 4xx other than 429 (e.g. a 401
/// from a missing/invalid API key). Retrying such a request — at the agent
/// planning level or via the streaming→non-streaming fallback — only
/// duplicates a call that can never succeed and delays the remediation hint
/// the client attached. 5xx / 429 / network errors remain retryable.
///
/// NOT terminal: a provider context-window rejection (a 400/413/422 whose
/// body names the context length — see
/// [`crate::errors::is_provider_context_overflow`]). The client types those
/// as `ApiError::ContextOverflow`; this exclusion covers any raw
/// `HttpStatus` that slipped past, so the loop can compress and retry.
///
/// Also terminal: [`WallClockBudgetExceeded`](crate::api::client::WallClockBudgetExceeded).
/// The run-level wall budget is already exhausted, so a planning-level retry
/// would only burn backoff sleeps (the client blocks the actual billable
/// request) and — worse — risk the stop being misfiled as a transient
/// network failure instead of a budget stop. The per-call cap
/// (`CallTimeBudgetExceeded`) is terminal for the same reason: the client
/// deliberately does not retry a cap breach (a retry would burn another full
/// cap window), so the planner must file it as a budget stop, not a
/// transient failure.
pub(super) fn is_terminal_api_client_error(e: &anyhow::Error) -> bool {
    e.chain().any(|cause| {
        if cause
            .downcast_ref::<crate::api::client::WallClockBudgetExceeded>()
            .is_some()
            || cause
                .downcast_ref::<crate::api::client::UsageBudgetExceeded>()
                .is_some()
            || cause
                .downcast_ref::<crate::api::client::CallTimeBudgetExceeded>()
                .is_some()
        {
            return true;
        }
        matches!(
            cause.downcast_ref::<crate::errors::ApiError>(),
            Some(crate::errors::ApiError::HttpStatus { status, message })
                if (400..500).contains(status)
                    && *status != 429
                    && !crate::errors::is_provider_context_overflow(*status, message)
        )
    })
}

/// Typed outcome for a streamed call that completed after at least
/// `threshold_ms` without an answer or a tool call. Pure so both shapes are
/// unit-testable without a live stream.
///
/// - no content, no reasoning → [`ApiError::ZeroContentLongCall`]
/// - reasoning only (field or inline `<think>` block) →
///   [`ApiError::ReasoningOnlyLongCall`]
///
/// `None` below the threshold, when any tool call arrived, or when the
/// content carries deliverable text.
///
/// [`ApiError::ZeroContentLongCall`]: crate::errors::ApiError::ZeroContentLongCall
/// [`ApiError::ReasoningOnlyLongCall`]: crate::errors::ApiError::ReasoningOnlyLongCall
pub(super) fn streamed_long_empty_call_outcome(
    elapsed_ms: u64,
    threshold_ms: u64,
    content: &str,
    reasoning: Option<&str>,
    has_tool_calls: bool,
) -> Option<crate::errors::ApiError> {
    if elapsed_ms < threshold_ms || has_tool_calls {
        return None;
    }
    if !super::recovery::strip_think_blocks(content)
        .trim()
        .is_empty()
    {
        return None;
    }
    // Inline <think> text in an otherwise-empty content counts as reasoning.
    let reasoning_chars = reasoning.map(|r| r.trim().len()).unwrap_or(0) + content.trim().len();
    Some(if reasoning_chars == 0 {
        crate::errors::ApiError::ZeroContentLongCall { elapsed_ms }
    } else {
        crate::errors::ApiError::ReasoningOnlyLongCall {
            elapsed_ms,
            reasoning_chars,
        }
    })
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

/// True when the history ends on an OPEN tool-call pair: the trailing message
/// is an assistant that still carries `tool_calls`, so its role=tool results
/// have not been appended yet and will land there on the next dispatch step.
/// Any message inserted now would wedge itself between the call and its
/// results.
fn history_ends_on_open_pair(messages: &[crate::api::types::Message]) -> bool {
    messages
        .last()
        .map(|m| m.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()))
        .unwrap_or(false)
}

/// True when inserting a message at `pos` (the index the new message would
/// occupy in `messages`) would land INSIDE an assistant(tool_calls) →
/// contiguous role=tool run. Splitting such a pair makes OpenAI-compatible
/// endpoints reject the whole payload with HTTP 400 ("messages with role
/// 'tool' must immediately follow an assistant message with 'tool_calls'").
fn insertion_splits_tool_pair(messages: &[crate::api::types::Message], pos: usize) -> bool {
    if pos == 0 || pos > messages.len() {
        return false;
    }
    // The inserted message would land right before `messages[pos]`: nothing
    // is split unless that message is a tool result of a pair starting
    // further back.
    if messages.get(pos).map(|m| m.role.as_str()) != Some("tool") {
        return false;
    }
    // Walk back over the contiguous run of tool results to its head; a run
    // whose head is an assistant carrying tool_calls is a live pair region.
    let mut head = pos;
    while head > 0 && messages[head - 1].role == "tool" {
        head -= 1;
    }
    head > 0
        && messages[head - 1]
            .tool_calls
            .as_ref()
            .is_some_and(|calls| !calls.is_empty())
}

/// Where may the user boundary message be inserted without splitting any
/// assistant(tool_calls)/tool-result pair? Nominally 6 messages from the end
/// (before the recent window); when that lands inside a pair it is pushed
/// back to just before the pair's opening assistant message. Returns `None`
/// when the history ends on an open pair — the results arrive on the next
/// step, so NO insertion may happen (skip entirely).
fn context_boundary_insert_pos(messages: &[crate::api::types::Message]) -> Option<usize> {
    if history_ends_on_open_pair(messages) {
        return None;
    }
    let desired = messages.len().saturating_sub(6);
    let mut pos = desired;
    while pos > 0 && insertion_splits_tool_pair(messages, pos) {
        pos -= 1;
    }
    Some(pos)
}

/// Build the assistant `Message` for storage in agent history.
///
/// Strips inline `<think>` blocks from `content`.
/// Only attaches `reasoning_content` if `preserve_thinking` is enabled.
pub(crate) fn build_assistant_history_message(
    content: &str,
    reasoning: Option<String>,
    native_tool_calls: Option<Vec<crate::api::types::ToolCall>>,
    preserve_thinking: bool,
) -> crate::api::types::Message {
    let history_content = super::recovery::strip_think_blocks(content);
    let history_reasoning = if preserve_thinking { reasoning } else { None };

    crate::api::types::Message {
        role: "assistant".to_string(),
        content: history_content.into(),
        reasoning_content: history_reasoning,
        tool_calls: native_tool_calls,
        tool_call_id: None,
        name: None,
    }
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
#[path = "../../tests/unit/agent/assistant_response/assistant_response_test.rs"]
mod usage_fallback_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/assistant_response/assistant_response_sanitize_tool_calls_test.rs"]
mod sanitize_tool_calls_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/assistant_response/assistant_response_terminal_error_test.rs"]
mod terminal_error_tests;

#[cfg(test)]
#[path = "../../tests/unit/agent/assistant_response/assistant_response_request_tail_test.rs"]
mod request_tail_tests;

#[cfg(test)]
mod empty_stream_contract_tests {
    use super::is_terminal_api_client_error;
    use crate::errors::ApiError;

    /// An unexplained empty stream must reach the non-streaming fallback rather
    /// than terminating the step. The fallback either succeeds or surfaces the
    /// provider's real diagnostic — measured on SGLang, the same request that
    /// streams empty returns a clean HTTP 400 non-streamed.
    #[test]
    fn empty_stream_falls_back_instead_of_terminating() {
        let err: anyhow::Error = ApiError::EmptyStream.into();
        assert!(
            !is_terminal_api_client_error(&err),
            "EmptyStream must fall back to non-streaming"
        );
    }

    #[test]
    fn a_4xx_is_still_terminal() {
        let err: anyhow::Error = ApiError::HttpStatus {
            status: 401,
            message: "unauthorized".into(),
        }
        .into();
        assert!(is_terminal_api_client_error(&err));
    }
}
