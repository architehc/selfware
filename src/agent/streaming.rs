use anyhow::Result;
use colored::*;
use tracing::debug;
use uuid::Uuid;

use super::tui_events::AgentEvent;
use super::*;
use crate::analysis::vector_store::EmbeddingProvider;
use crate::session::cache::LlmCacheEntry;

/// Providers may repeat cumulative snapshots or briefly send smaller ones.
/// Session counters and events must add only new tokens, like the run ledger.
fn streaming_usage_delta(
    previous: &mut crate::api::Usage,
    current: &crate::api::Usage,
) -> (u64, u64) {
    let prompt = current.prompt_tokens.saturating_sub(previous.prompt_tokens) as u64;
    let completion = current
        .completion_tokens
        .saturating_sub(previous.completion_tokens) as u64;
    previous.prompt_tokens = previous.prompt_tokens.max(current.prompt_tokens);
    previous.completion_tokens = previous.completion_tokens.max(current.completion_tokens);
    previous.total_tokens = previous.total_tokens.max(current.total_tokens).max(
        previous
            .prompt_tokens
            .saturating_add(previous.completion_tokens),
    );
    previous.cost = current.cost;
    previous.reasoning_tokens = match (previous.reasoning_tokens, current.reasoning_tokens) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    previous.completion_tokens_details = crate::api::types::merge_completion_details_max(
        previous.completion_tokens_details.as_ref(),
        current.completion_tokens_details.as_ref(),
    );
    previous.prompt_tokens_details = crate::api::types::merge_prompt_details_max(
        previous.prompt_tokens_details.as_ref(),
        current.prompt_tokens_details.as_ref(),
    );
    (prompt, completion)
}

/// All XML tag pairs that local models may emit and should be hidden from
/// display.  Each entry is `(open_tag, close_tag)`.  The streaming renderer
/// suppresses everything between (and including) these tags.
const SUPPRESSED_TAGS: &[(&str, &str)] = &[
    ("<tool_call>", "</tool_call>"),
    ("<tool>", "</tool>"),
    ("<think>", "</think>"),
    ("<thinking>", "</thinking>"),
    ("<|channel>", "<channel|>"),
];

/// Char budget for one streaming response on a mutation task (~8k tokens).
/// Past it with no tool call in flight, the response is a runaway monologue:
/// measured 2026-08-24 on TB 3.0 `cli-2ph-simplex`, where two such responses
/// (~1,059 log lines in the first) consumed a 2500s task timeout without a
/// single edit. Cutting the stream lets the no-action escalation fire on
/// schedule instead of after a 20-minute stall.
pub(crate) const MONOLOGUE_CHAR_BUDGET: usize = 32_000;

/// The cutoff condition: over budget, no tool call in flight (native calls
/// parsed, or XML tool markup in the content), and a mutation task. Read-only
/// tasks and plain chat are exempt — long prose is the deliverable there.
pub(crate) fn is_runaway_monologue(
    streamed_chars: usize,
    tool_call_in_flight: bool,
    requires_mutation: bool,
) -> bool {
    requires_mutation && !tool_call_in_flight && streamed_chars > MONOLOGUE_CHAR_BUDGET
}

/// Marker appended to truncated content so the model sees in its own history
/// that the monologue was cut, and what to do instead.
fn monologue_cut_notice(chars: usize) -> String {
    format!(
        "\n\n[selfware: response truncated at {chars} chars — long analysis produced \
         no tool call; respond with the next tool call only]"
    )
}

/// Find the earliest opening tag from `SUPPRESSED_TAGS` in `buf`.
/// Returns `(byte_offset, tag_index)` or `None`.
fn find_earliest_open_tag(buf: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (i, &(open, _)) in SUPPRESSED_TAGS.iter().enumerate() {
        if let Some(pos) = buf.find(open) {
            if best.is_none() || best.is_some_and(|(b, _)| pos < b) {
                best = Some((pos, i));
            }
        }
    }
    best
}

/// Check if `buf` ends with a prefix of any opening suppressed tag,
/// indicating we should buffer instead of printing (the rest of the tag
/// may arrive in the next chunk).
fn has_partial_tag_at_end(buf: &str) -> bool {
    for &(open, _) in SUPPRESSED_TAGS {
        for prefix_len in 1..open.len() {
            if buf.ends_with(&open[..prefix_len]) {
                return true;
            }
        }
    }
    false
}

/// Extract a tool name from a suppressed XML block for a clean one-line
/// summary.  Tries `<name>x</name>` (used by `<tool>` blocks) and the
/// existing `<function=x>` / `<function>x</function>` patterns.
fn extract_display_name(xml: &str) -> Option<String> {
    // <name>tool_name</name> — used in <tool> blocks from Qwen
    if let Some(start) = xml.find("<name>") {
        let rest = &xml[start + "<name>".len()..];
        if let Some(end) = rest.find("</name>") {
            let name = rest[..end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    Agent::extract_tool_name(xml)
}

/// Whether a finished stream produced nothing AND never explained why.
///
/// Extracted as a pure function so the discrimination can be tested
/// exhaustively. It is the part that silently regresses: every argument here
/// has a benign case that must stay `Ok`, and widening the condition by one
/// term turns an honest empty response into a spurious failure.
///
/// Note on coverage: this covers the DECISION only. That the decision reaches
/// the non-streaming fallback at both call sites is behavioural, and is covered
/// by the container fixture in target/install-validation (a server that answers
/// `[DONE]` when streamed and a real completion when not), not by this crate's
/// unit tests.
pub(crate) fn is_unexplained_empty_stream(
    no_content: bool,
    no_reasoning: bool,
    no_tool_calls: bool,
    provider_explained_itself: bool,
    cancelled: bool,
) -> bool {
    no_content && no_reasoning && no_tool_calls && !provider_explained_itself && !cancelled
}

/// Whether the consumer's loop ended on a stream that PRODUCED output but
/// never reached an accepted terminal indication — the [DONE] sentinel or a
/// provider `finish_reason` — and did not exit for a deliberate reason
/// (caller cancel, runaway-monologue cutoff).
///
/// This is the consumer-side mirror of the producer's `collect()` terminal
/// contract (see `crate::api::streaming::StreamingResponse::collect`), so the
/// two sides agree on what "complete" means: the same shapes the producer
/// fails with a typed protocol error must also fail here instead of being
/// promoted to a stored success with a synthesized `finish_reason: stream_end`.
/// Deliberate truncations — cancel and the runaway-monologue cutoff — are not
/// incomplete-provider streams and stay `Ok` with their own recovery
/// semantics. All-empty streams are excluded: they fall through to
/// [`is_unexplained_empty_stream`] so the `EmptyStream` discrimination is
/// preserved exactly.
pub(crate) fn stream_ended_truncated_without_terminal(
    ended_with_done: bool,
    has_finish_reason: bool,
    produced_output: bool,
    cancelled: bool,
    runaway_cut: bool,
) -> bool {
    !ended_with_done && !has_finish_reason && produced_output && !cancelled && !runaway_cut
}

impl Agent {
    /// Extract function name from a tool_call XML block for clean display
    pub(super) fn extract_tool_name(xml: &str) -> Option<String> {
        // Match <function=name> or <function>name pattern
        if let Some(start) = xml.find("<function=") {
            let rest = &xml[start + "<function=".len()..];
            let end = rest.find(['>', '<', '\n']).unwrap_or(rest.len());
            let name = rest[..end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        // Also try <function>name</function> pattern
        if let Some(start) = xml.find("<function>") {
            let rest = &xml[start + "<function>".len()..];
            if let Some(end) = rest.find("</function>") {
                let name = rest[..end].trim();
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
        None
    }

    /// Check LLM cache for a matching previous request
    /// Returns cached response if found, None otherwise
    async fn check_llm_cache(
        &self,
        messages: &[Message],
        tools: &Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<Option<LlmCacheEntry>> {
        // Generate cache key from model, messages, tools, and thinking mode.
        // Including the model name prevents cross-model semantic matches.
        let prompt = Self::messages_to_prompt(messages);
        let key = format!(
            "{}:{}:{:?}:{:?}",
            self.config.model, prompt, tools, thinking
        );

        // Compute a real context hash from the full key so that entries with
        // different model / prompt / tools / thinking never collide.
        let context_hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut h);
            h.finish()
        };

        // Generate embedding for the prompt
        let embedding = self.cache_manager.llm_embedding.embed(&prompt).await?;

        // Look up in cache using the real context hash
        let cached = self
            .cache_manager
            .llm_cache
            .lookup(&prompt, &embedding, context_hash, &self.config.model)
            .await;

        Ok(cached)
    }

    /// Convert messages to a single prompt string for caching
    fn messages_to_prompt(messages: &[Message]) -> String {
        messages
            .iter()
            .map(|m| format!("[{}]: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Cache a response after streaming completes
    ///
    /// This function stores the LLM response in the cache for future reuse,
    /// using embeddings for semantic matching of similar requests.
    pub async fn cache_response(
        &self,
        messages: &[Message],
        tools: &Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
        content: &str,
        reasoning: &Option<String>,
        tool_calls: &Option<Vec<ToolCall>>,
    ) {
        // Never cache a response that carried tool calls: the cache stores only
        // text (content + reasoning) and a later cache hit returns None for tool
        // calls, so it would silently replace a needed tool invocation with stale
        // prose. Only pure-text responses are safe to serve from cache.
        if tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
            return;
        }

        // Also never cache a response whose *content* contains a text/XML tool
        // call (e.g. GLM/Qwen style). Without native tool_calls this would be
        // stored as plain prose and replayed as a non-tool completion on a hit.
        let parsed = crate::tool_parser::parse_tool_calls(content);
        if !parsed.tool_calls.is_empty() {
            return;
        }

        let prompt = Self::messages_to_prompt(messages);
        let key = format!(
            "{}:{}:{:?}:{:?}",
            self.config.model, prompt, tools, thinking
        );

        // Compute a real context hash from the full key (includes model).
        let context_hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut h);
            h.finish()
        };

        let embedding = match self.cache_manager.llm_embedding.embed(&prompt).await {
            Ok(e) => e,
            Err(e) => {
                debug!("Failed to generate embedding for cache: {}", e);
                return;
            }
        };

        // Build response text from content and reasoning
        let mut response = content.to_string();
        if let Some(reason) = reasoning {
            if !reason.is_empty() {
                response.push_str("\n\nReasoning: ");
                response.push_str(reason);
            }
        }

        let entry = LlmCacheEntry {
            id: Uuid::new_v4().to_string(),
            prompt: prompt.clone(),
            embedding,
            response,
            model: self.config.model.clone(),
            input_tokens: 0,                     // Would need to track this
            output_tokens: content.len() as u32, // Approximation
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            hit_count: 0,
            context_hash,
            file_paths: vec![],
        };

        self.cache_manager.llm_cache.store(entry).await;
    }

    /// Chat with streaming, displaying output as it arrives
    /// Returns (content, reasoning, tool_calls) tuple.
    ///
    /// `meta_out` is populated with the request body and per-turn timing /
    /// finish_reason / token usage when set to `Some(_)` by the caller. This
    /// is how the per-turn debug capture in `execute_step_internal` learns
    /// what was actually sent over the wire and how the model ended its turn.
    /// True when the current task requires code changes — the monologue
    /// cutoff applies only then. Empty task context (plain interactive chat)
    /// is never cut: long prose answers are legitimate deliverables there.
    fn task_requires_mutation_now(&self) -> bool {
        !self.current_task_context.is_empty()
            && !self.current_task_is_read_only()
            && super::tool_dispatch::task_requires_mutation(self.task_context_for_classification())
    }

    /// Surface one waiting heartbeat: always as a [`ProgressEvent`]
    /// (stream-json / stderr trace), and on whichever spinner is still live
    /// (TUI or terminal) as `"<phrase> — <phase> <N>s"`. Once the spinner has
    /// stopped (content or reasoning is visibly streaming) only the progress
    /// event is emitted.
    ///
    /// [`ProgressEvent`]: super::progress::ProgressEvent
    pub(super) fn report_llm_wait(
        &self,
        event: super::progress::ProgressEvent,
        base_phrase: &str,
        tui_spinner_live: bool,
        terminal_spinner: Option<&crate::ui::spinner::TerminalSpinner>,
    ) {
        if tui_spinner_live {
            if let Some(text) = super::llm_wait::spinner_status(base_phrase, &event, true) {
                self.emit_event(AgentEvent::SpinnerUpdate { message: text });
            }
        } else if let Some(s) = terminal_spinner {
            // The terminal spinner already appends its own elapsed time.
            if let Some(text) = super::llm_wait::spinner_status(base_phrase, &event, false) {
                s.set_message(&text);
            }
        }
        self.emit_progress(event);
    }

    /// Run a NON-streaming model call with the waiting heartbeat: every
    /// `LLM_WAIT_TICK` an `llm_waiting phase=awaiting_response` progress event
    /// is emitted until the response arrives (the phase of a non-streaming
    /// call is not observable, so none is guessed).
    pub(super) async fn await_nonstreaming_llm<F, T>(&self, fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        // Boxed so the caller's async frame does not grow by the request future.
        super::llm_wait::await_with_ticks(
            Box::pin(fut),
            super::llm_wait::LlmWaitTicker::start(),
            |ev| self.emit_progress(ev),
        )
        .await
    }

    pub(super) async fn chat_streaming(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
        meta_out: Option<&mut crate::api::types::ChatMetadata>,
    ) -> Result<(String, Option<String>, Option<Vec<ToolCall>>)> {
        use std::io::{self, Write};

        // --- Cache Integration: Check for cached response before API call ---
        if let Some(cached) = self.check_llm_cache(&messages, &tools, thinking).await? {
            debug!("LLM cache hit: returning cached response");
            // For cached responses, return just the content
            return Ok((cached.response, None, None));
        }

        // Clone messages and tools for caching after streaming (they will be moved below)
        let messages_for_cache = messages.clone();
        let tools_for_cache = tools.clone();

        // Activate the sticky status bar if running interactively
        let mode_label = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "normal",
            crate::config::ExecutionMode::AutoEdit => "auto-edit",
            crate::config::ExecutionMode::Yolo => "YOLO",
            crate::config::ExecutionMode::Daemon => "daemon",
        };
        let sticky_state = crate::ui::sticky_bar::StickyState::new(mode_label, &self.config.model);
        // Sticky bar is tracked for state (tokens, activity, bash count) but
        // NOT rendered during streaming — cursor positioning breaks with raw
        // stdout output. The state is used for the post-task summary line.
        let _sticky: Option<crate::ui::sticky_bar::StickyBar> = None;

        // Start loading spinner with a random phrase while waiting for first token
        let initial_phrase = crate::ui::loading_phrases::random_phrase();
        let tui_active = crate::output::is_tui_active();
        // In JSON or quiet mode, streamed prose must NOT be printed to stdout —
        // it would pollute the machine-readable output stream. The text is still
        // accumulated into `content` and returned via the normal result path.
        let suppress_stream_stdout = crate::output::is_json_mode() || crate::output::is_quiet();
        // Track whether the TUI spinner is logically active (to avoid
        // sending SpinnerUpdate/SpinnerStop after it has already stopped).
        let mut tui_spinner_active = false;
        let mut spinner = if tui_active {
            self.emit_event(AgentEvent::SpinnerStart {
                message: initial_phrase.to_string(),
            });
            tui_spinner_active = true;
            None
        } else {
            Some(crate::ui::spinner::TerminalSpinner::start(initial_phrase))
        };
        let mut phrase_rotation = tokio::time::Instant::now();
        let _last_bar_update = tokio::time::Instant::now();

        // Acquire concurrency governor permit before sending the streaming request.
        // The permit is held for the duration of the streaming response and released on drop.
        let _stream_permit = self
            .governor
            .acquire_stream()
            .await
            .map_err(|e| anyhow::anyhow!("concurrency governor error: {}", e))?;

        // Waiting heartbeat: a queued/prefilling or long-reasoning call can be
        // silent for minutes, so every LLM_WAIT_TICK surface elapsed time,
        // phase, and tokens so far (progress event + spinner text).
        let mut wait_ticker = super::llm_wait::LlmWaitTicker::start();
        // Boxed: keeps the large request future out of this (already deep)
        // async frame — inlining it overflowed the test-thread stack.
        let mut send_fut = Box::pin(self.client.chat_stream_with_meta(messages, tools, thinking));
        let (stream, request_meta) = loop {
            tokio::select! {
                biased;
                sent = &mut send_fut => break sent?,
                _ = tokio::time::sleep_until(wait_ticker.next_due()) => {
                    // Headers not back yet: still queued / prefilling.
                    let event = wait_ticker.fire(
                        super::llm_wait::LlmWaitPhase::Prefill,
                        0,
                        super::llm_wait::LlmWaitTokenSource::Estimate,
                    );
                    self.report_llm_wait(
                        event,
                        initial_phrase,
                        tui_active && tui_spinner_active,
                        spinner.as_ref(),
                    );
                }
            }
        };
        // request_meta is plumbed through to meta_out at the bottom of this
        // function, after we've also harvested finish_reason / token usage
        // from the SSE stream.
        let mut captured_finish_reason: Option<String> = None;
        let mut captured_prompt_tokens: Option<u32> = None;
        let mut captured_completion_tokens: Option<u32> = None;
        let mut captured_total_tokens: Option<u32> = None;
        let mut captured_cost: Option<f64> = None;
        let mut reported_usage = crate::api::Usage::default();
        // Whole-call timer: request_meta.elapsed_ms measures time-to-headers
        // for streaming, so the speed sample below needs its own clock.
        let stream_started = std::time::Instant::now();

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_buf = String::new();
        // Which suppressed tag we're currently inside, if any
        let mut suppressed_tag_idx: Option<usize> = None;
        let mut captured_logprobs: Option<serde_json::Value> = None;
        // How the loop below ended. `stream_ended_with_done` tracks the
        // [DONE] sentinel; `runaway_cut` tracks the deliberate monologue
        // cutoff. Both matter for the terminal-indication guard after the
        // loop: a stream that produced content but never reached an accepted
        // terminal is truncated, not complete.
        let mut stream_ended_with_done = false;
        let mut runaway_cut = false;

        let cancel = self.cancel_token();

        loop {
            // Use select to check cancellation even when recv is waiting
            let chunk_result = tokio::select! {
                biased;
                _ = async {
                    loop {
                        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                            return;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    }
                } => {
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        // tui_spinner_active stays true here — the break exits the loop
                    } else {
                        drop(spinner.take());
                    }
                    break;
                }
                step = super::llm_wait::recv_or_tick(&mut rx, &wait_ticker) => {
                    match step {
                        super::llm_wait::RecvOrTick::Item(Some(r)) => r,
                        super::llm_wait::RecvOrTick::Item(None) => break,
                        super::llm_wait::RecvOrTick::Tick => {
                            let phase = super::llm_wait::LlmWaitPhase::classify(
                                &content,
                                &reasoning,
                                tool_calls.len(),
                                in_reasoning
                                    || suppressed_tag_idx.is_some_and(|i| i >= 2),
                            );
                            let (tokens, source) = super::llm_wait::tokens_so_far(
                                captured_completion_tokens,
                                &content,
                                &reasoning,
                            );
                            let event = wait_ticker.fire(phase, tokens, source);
                            self.report_llm_wait(
                                event,
                                initial_phrase,
                                tui_active && tui_spinner_active,
                                spinner.as_ref(),
                            );
                            continue;
                        }
                    }
                }
            };

            let chunk = chunk_result?;

            // Runaway-monologue cutoff: checked per chunk, before processing.
            if is_runaway_monologue(
                content.len() + reasoning.len(),
                !tool_calls.is_empty() || content.contains("<tool"),
                self.task_requires_mutation_now(),
            ) {
                let streamed = content.len() + reasoning.len();
                warn!("runaway monologue truncated at {streamed} chars (no tool call in flight)");
                content.push_str(&monologue_cut_notice(streamed));
                runaway_cut = true;
                break;
            }

            // Rotate loading phrase every 3 seconds while spinner is active
            if tui_active {
                if tui_spinner_active
                    && phrase_rotation.elapsed() > tokio::time::Duration::from_secs(3)
                {
                    let new_phrase = crate::ui::loading_phrases::random_phrase();
                    self.emit_event(AgentEvent::SpinnerUpdate {
                        message: new_phrase.to_string(),
                    });
                    phrase_rotation = tokio::time::Instant::now();
                }
            } else if let Some(ref s) = spinner {
                if phrase_rotation.elapsed() > tokio::time::Duration::from_secs(3) {
                    s.set_message(crate::ui::loading_phrases::random_phrase());
                    phrase_rotation = tokio::time::Instant::now();
                }
            }

            // NOTE: Do not call bar.update() during streaming — cursor
            // save/restore doesn't work reliably while stdout is actively
            // printing content and causes the bar to spam every line.
            // The bar is shown once at the end via bar.finish().

            match chunk {
                StreamChunk::Content(text) => {
                    // Stop spinner on first content — must complete
                    // before we print anything to avoid interleaving
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        tui_spinner_active = false;
                    } else if let Some(s) = spinner.take() {
                        // Drop stops the spinner task and prints final line
                        drop(s);
                        // Small delay to let the spinner task fully exit
                        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                    }
                    if in_reasoning {
                        in_reasoning = false;
                        sticky_state
                            .is_thinking
                            .store(false, std::sync::atomic::Ordering::Relaxed);
                        sticky_state.thinking_secs.store(
                            sticky_state.started.elapsed().as_secs(),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        if tui_active {
                            self.emit_event(AgentEvent::ThinkingEnd);
                        } else if !output::is_compact() && !suppress_stream_stdout {
                            println!();
                        }
                    }
                    sticky_state.set_activity("Generating...");
                    // Always accumulate full content for parsing
                    content.push_str(&text);

                    // Buffer content and filter suppressed XML tags from display
                    display_buf.push_str(&text);

                    loop {
                        if let Some(tag_idx) = suppressed_tag_idx {
                            // We're inside a suppressed tag — look for its closing tag
                            let (_, close) = SUPPRESSED_TAGS[tag_idx];
                            if let Some(end_pos) = display_buf.find(close) {
                                let end = end_pos + close.len();
                                let block = &display_buf[..end];
                                // For tool tags, show a clean one-line summary
                                let is_think = tag_idx >= 2; // <think> and <thinking>
                                if !is_think {
                                    if let Some(fname) = extract_display_name(block) {
                                        if tui_active {
                                            self.emit_event(AgentEvent::ToolProgress {
                                                name: fname,
                                                status: "parsing".into(),
                                            });
                                        } else if !suppress_stream_stdout {
                                            print!(
                                                "\r\n  {} {}...",
                                                "🔧".dimmed(),
                                                fname.bright_cyan()
                                            );
                                            io::stdout().flush().ok();
                                        }
                                    }
                                }
                                // For <think> blocks, optionally show as dimmed reasoning
                                if is_think && !output::is_compact() {
                                    // Extract inner text, strip the open/close tags
                                    let (open, _) = SUPPRESSED_TAGS[tag_idx];
                                    let inner =
                                        &block[open.len()..block.len().saturating_sub(close.len())];
                                    let trimmed = inner.trim();
                                    if !trimmed.is_empty() {
                                        reasoning.push_str(trimmed);
                                    }
                                }
                                display_buf.drain(..end);
                                suppressed_tag_idx = None;
                            } else {
                                break; // Wait for more data
                            }
                        } else {
                            // Look for the earliest opening suppressed tag
                            if let Some((start_pos, tag_idx)) = find_earliest_open_tag(&display_buf)
                            {
                                // Emit/print everything before the tag
                                let before = &display_buf[..start_pos];
                                if !before.is_empty() {
                                    if tui_active {
                                        self.emit_event(AgentEvent::AssistantDelta {
                                            text: before.to_string(),
                                        });
                                    } else if !suppress_stream_stdout {
                                        // Replace \n with \r\n so every newline resets to col 0
                                        let safe = before.replace('\n', "\r\n");
                                        print!("{}", safe);
                                        io::stdout().flush().ok();
                                    }
                                }
                                display_buf.drain(..start_pos);
                                suppressed_tag_idx = Some(tag_idx);
                            } else if has_partial_tag_at_end(&display_buf) {
                                // Partial opening tag at end — buffer it
                                break;
                            } else {
                                // No tags — emit/print everything
                                if !display_buf.is_empty() {
                                    if tui_active {
                                        self.emit_event(AgentEvent::AssistantDelta {
                                            text: display_buf.clone(),
                                        });
                                    } else if !suppress_stream_stdout {
                                        let safe = display_buf.replace('\n', "\r\n");
                                        print!("{}", safe);
                                        io::stdout().flush().ok();
                                    }
                                }
                                display_buf.clear();
                                break;
                            }
                        }
                    }
                }
                StreamChunk::Reasoning(text) => {
                    // Stop spinner on first reasoning — unless (compact,
                    // non-TUI) the reasoning is not printed: then the spinner
                    // is the only sign of life and keeps showing the
                    // waiting heartbeat until content arrives.
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        tui_spinner_active = false;
                    } else if !tui_active && !output::is_compact() {
                        if let Some(s) = spinner.take() {
                            drop(s);
                        }
                    }
                    sticky_state
                        .is_thinking
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    sticky_state.set_activity("Thinking...");
                    if tui_active {
                        if !in_reasoning {
                            in_reasoning = true;
                        }
                        self.emit_event(AgentEvent::ThinkingDelta { text: text.clone() });
                    } else if !output::is_compact() {
                        if !in_reasoning {
                            in_reasoning = true;
                            output::thinking_prefix();
                        }
                        output::thinking(&text, true);
                        io::stdout().flush().ok();
                    }
                    reasoning.push_str(&text);
                }
                StreamChunk::ToolCall(call) => {
                    tool_calls.push(call);
                }
                StreamChunk::Usage(u, coverage) => {
                    debug!(
                        "Token usage: {} prompt, {} completion",
                        u.prompt_tokens, u.completion_tokens
                    );
                    let (prompt_delta, completion_delta) =
                        streaming_usage_delta(&mut reported_usage, &u);
                    sticky_state.add_tokens(completion_delta);
                    output::record_tokens(prompt_delta, completion_delta);
                    output::print_token_usage(
                        reported_usage.prompt_tokens as u64,
                        reported_usage.completion_tokens as u64,
                    );

                    if coverage.prompt {
                        captured_prompt_tokens = Some(reported_usage.prompt_tokens as u32);
                    }
                    if coverage.completion {
                        captured_completion_tokens = Some(reported_usage.completion_tokens as u32);
                    }
                    if coverage.total {
                        captured_total_tokens = Some(reported_usage.total_tokens as u32);
                    }
                    if u.cost.is_some() {
                        captured_cost = reported_usage.cost;
                    }

                    self.emit_event(AgentEvent::TokenUsage {
                        prompt_tokens: prompt_delta,
                        completion_tokens: completion_delta,
                    });
                }
                StreamChunk::Logprobs(lp) => {
                    if let Some(existing) = &mut captured_logprobs {
                        crate::api::streaming::merge_logprobs(existing, lp);
                    } else {
                        captured_logprobs = Some(lp);
                    }
                }
                StreamChunk::FinishReason(reason) => {
                    captured_finish_reason = Some(reason);
                }
                StreamChunk::Error(msg) => {
                    return Err(anyhow::anyhow!(
                        "Provider streamed an error mid-response: {}",
                        msg
                    ));
                }
                StreamChunk::Done => {
                    stream_ended_with_done = true;
                    break;
                }
            }
        }

        // Flush any remaining display buffer (non-suppressed text)
        if !display_buf.is_empty() && suppressed_tag_idx.is_none() {
            if tui_active {
                self.emit_event(AgentEvent::AssistantDelta {
                    text: display_buf.clone(),
                });
            } else if !suppress_stream_stdout {
                let safe = display_buf.replace('\n', "\r\n");
                print!("{}", safe);
                io::stdout().flush().ok();
            }
        }

        // Trailing newline is DISPLAY output — suppress it in json/quiet mode.
        if !tui_active && !suppress_stream_stdout && (!content.is_empty() || !reasoning.is_empty())
        {
            println!();
        }

        // Terminal-indication guard, mirror of the producer (W2b). The
        // producer's `collect()` (`crate::api::streaming`) fails a stream that
        // produced events but never reached an accepted terminal — the [DONE]
        // sentinel or a provider `finish_reason`. This consumer reads the raw
        // channel, so it must reach the same verdict itself instead of
        // promoting truncation to success by synthesizing "stream_end"
        // (2026-09-21 review, P2; consumer side) — half-written prose or a
        // partial tool call must not be handed to execution as a clean turn.
        // Deliberate truncations stay benign: caller cancel and the
        // runaway-monologue cutoff carry their own recovery semantics.
        // All-empty streams fall through to the narrow EmptyStream guard
        // below, preserving the exact error discrimination of that shape.
        let produced_output =
            !content.is_empty() || !reasoning.is_empty() || !tool_calls.is_empty();
        if stream_ended_truncated_without_terminal(
            stream_ended_with_done,
            captured_finish_reason.is_some(),
            produced_output,
            cancel.load(std::sync::atomic::Ordering::Relaxed),
            runaway_cut,
        ) {
            return Err(crate::errors::ApiError::Parse(format!(
                "stream ended before an accepted terminal indication (no [DONE], no finish_reason): \
                 truncated after {} content chars / {} reasoning chars / {} tool call(s)",
                content.len(),
                reasoning.len(),
                tool_calls.len(),
            ))
            .into());
        }

        // Response caching is NOT display — it must run regardless of output
        // mode. (It was previously nested under the display guard above, so
        // json/quiet mode accidentally skipped caching.)
        if !tui_active && !content.is_empty() {
            let reasoning_opt: Option<String> = if reasoning.is_empty() {
                None
            } else {
                Some(reasoning.clone())
            };
            let tool_calls_opt: Option<Vec<ToolCall>> = if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls.clone())
            };

            self.cache_response(
                &messages_for_cache,
                &tools_for_cache,
                thinking,
                &content,
                &reasoning_opt,
                &tool_calls_opt,
            )
            .await;
        }

        // Mirror the non-streaming path: emit `LlmResponseReceived` once the
        // SSE stream has produced its final usage / finish-reason chunks.  The
        // request-side event was already emitted inside
        // `chat_stream_with_meta`.
        //
        // By this point the stream reached an accepted terminal ([DONE] or a
        // provider finish_reason) or ended deliberately (cancel /
        // runaway-monologue cutoff) — the truncated-without-terminal case was
        // rejected above. The synthesize "stream_end" fallback therefore only
        // fires for a stream that ended on [DONE] without a finish_reason
        // chunk, which is a complete stream, not a truncation.
        self.emit_progress(super::progress::ProgressEvent::LlmResponseReceived {
            finish_reason: captured_finish_reason
                .clone()
                .unwrap_or_else(|| "stream_end".into()),
            completion_tokens: captured_completion_tokens.unwrap_or(0),
            elapsed_ms: stream_started.elapsed().as_millis() as u64,
        });

        // Feed the endpoint's measured effective speed into the client's
        // adaptive timeout. Non-streaming calls already do this (client.rs);
        // streaming calls were invisible to the tracker, so streaming-only
        // endpoints kept the conservative 3 t/s presumption and a 2h per-call
        // ceiling instead of a measured one.
        let stream_elapsed_secs = stream_started.elapsed().as_secs_f64();
        if stream_elapsed_secs > 0.0 && captured_completion_tokens.unwrap_or(0) > 0 {
            self.client
                .speed_tracker
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .record(captured_completion_tokens.unwrap_or(0) as f64 / stream_elapsed_secs);
        }

        // Recorded before the meta block consumes it below.
        let provider_explained_itself = captured_finish_reason.is_some();

        if let Some(slot) = meta_out {
            *slot = crate::api::types::ChatMetadata {
                request_body: request_meta.request_body,
                elapsed_ms: request_meta.elapsed_ms,
                finish_reason: captured_finish_reason,
                prompt_tokens: captured_prompt_tokens,
                completion_tokens: captured_completion_tokens,
                total_tokens: captured_total_tokens,
                cost: captured_cost,
                accounted_usage: None,
                logprobs: captured_logprobs,
            };
        }

        // A stream that produced nothing AND never declared a finish_reason did
        // not complete — the provider closed it. Returning Ok here hands the
        // agent an empty turn, which execution.rs answers by nudging the model
        // to respond, pushing more tokens at a backend that is already failing.
        //
        // Deliberately narrow, so the honest empty cases stay Ok:
        //   - cancelled by the caller      -> the loop broke on the cancel token
        //   - finish_reason present        -> the provider explained itself
        //                                     (`length` is ReasoningBudgetExhausted)
        //   - reasoning-only or tool-only  -> tokens were produced
        if is_unexplained_empty_stream(
            content.is_empty(),
            reasoning.is_empty(),
            tool_calls.is_empty(),
            provider_explained_itself,
            cancel.load(std::sync::atomic::Ordering::Relaxed),
        ) {
            return Err(crate::errors::ApiError::EmptyStream.into());
        }

        Ok((
            content,
            if reasoning.is_empty() {
                None
            } else {
                Some(reasoning)
            },
            if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/streaming/streaming_test.rs"]
mod tests;

#[cfg(test)]
mod empty_stream_guard_tests {
    use super::is_unexplained_empty_stream;

    /// The only shape that is an error: nothing produced, nothing explained,
    /// nobody cancelled.
    #[test]
    fn only_an_unexplained_silent_stream_is_an_error() {
        assert!(is_unexplained_empty_stream(true, true, true, false, false));
    }

    /// Every benign empty case must stay Ok. Each of these was a real shape a
    /// provider can legitimately return.
    #[test]
    fn honest_empty_cases_are_not_errors() {
        // Cancelled by the caller — the loop broke on the cancel token.
        assert!(!is_unexplained_empty_stream(true, true, true, false, true));
        // The provider explained itself; `length` is ReasoningBudgetExhausted.
        assert!(!is_unexplained_empty_stream(true, true, true, true, false));
        // Reasoning arrived, answer did not: tokens were produced.
        assert!(!is_unexplained_empty_stream(
            true, false, true, false, false
        ));
        // Tool-only completion: a valid turn with no prose.
        assert!(!is_unexplained_empty_stream(
            true, true, false, false, false
        ));
        // Content arrived.
        assert!(!is_unexplained_empty_stream(
            false, true, true, false, false
        ));
    }

    /// Exhaustive: exactly one of the 32 combinations may be an error.
    #[test]
    fn exactly_one_of_thirty_two_combinations_errors() {
        let mut errors = 0;
        for bits in 0..32u8 {
            if is_unexplained_empty_stream(
                bits & 1 != 0,
                bits & 2 != 0,
                bits & 4 != 0,
                bits & 8 != 0,
                bits & 16 != 0,
            ) {
                errors += 1;
            }
        }
        assert_eq!(errors, 1, "the guard must stay narrow");
    }
}

#[cfg(test)]
mod truncated_stream_guard_tests {
    use super::stream_ended_truncated_without_terminal as trunc;

    /// The consumer-side equivalent of the producer's P2 hole: a stream that
    /// sent CONTENT and then closed without [DONE] and without a provider
    /// finish_reason is TRUNCATED, not a stored success.
    #[test]
    fn content_then_close_without_terminal_is_truncated() {
        // ended_with_done=false, has_finish_reason=false,
        // produced_output=true, cancelled=false, runaway_cut=false
        assert!(trunc(false, false, true, false, false));
    }

    /// Every accepted terminal / deliberate exit must stay Ok.
    #[test]
    fn accepted_terminals_and_deliberate_exits_are_not_truncated() {
        // [DONE] sentinel arrived: complete, whatever else happened.
        assert!(!trunc(true, false, true, false, false));
        // Provider finish_reason arrived (clean-EOF providers with no [DONE]).
        assert!(!trunc(false, true, true, false, false));
        assert!(!trunc(true, true, true, false, false));
        // Caller cancelled: deliberate, the content shown is the partial view.
        assert!(!trunc(false, false, true, true, false));
        // Runaway-monologue cutoff: deliberate truncation with its own notice.
        assert!(!trunc(false, false, true, false, true));
        // Nothing produced: that shape belongs to the EmptyStream guard.
        assert!(!trunc(false, false, false, false, false));
    }

    /// Exhaustive: exactly ONE of the 32 combinations may be an error — the
    /// stream produced output but reached no accepted terminal and ended
    /// without a deliberate exit. Every other combination is a complete
    /// stream ([DONE] or finish_reason), a deliberate truncation (cancel /
    /// runaway cut), or an all-empty shape that belongs to the EmptyStream
    /// guard.
    #[test]
    fn exactly_one_of_thirty_two_combinations_is_truncated() {
        let mut errors = 0;
        for bits in 0..32u8 {
            if trunc(
                bits & 1 != 0,  // ended_with_done
                bits & 2 != 0,  // has_finish_reason
                bits & 4 != 0,  // produced_output
                bits & 8 != 0,  // cancelled
                bits & 16 != 0, // runaway_cut
            ) {
                errors += 1;
            }
        }
        assert_eq!(
            errors, 1,
            "the consumer guard must match the producer contract exactly"
        );
    }
}
