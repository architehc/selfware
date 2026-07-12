use anyhow::Result;
use colored::*;
use tracing::debug;
use uuid::Uuid;

use super::tui_events::AgentEvent;
use super::*;
use crate::analysis::vector_store::EmbeddingProvider;
use crate::session::cache::LlmCacheEntry;

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

        let (stream, request_meta) = self
            .client
            .chat_stream_with_meta(messages, tools, thinking)
            .await?;
        // request_meta is plumbed through to meta_out at the bottom of this
        // function, after we've also harvested finish_reason / token usage
        // from the SSE stream.
        let mut captured_finish_reason: Option<String> = None;
        let mut captured_prompt_tokens: Option<u32> = None;
        let mut captured_completion_tokens: Option<u32> = None;
        let mut captured_total_tokens: Option<u32> = None;
        let mut captured_cost: Option<f64> = None;

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_buf = String::new();
        // Which suppressed tag we're currently inside, if any
        let mut suppressed_tag_idx: Option<usize> = None;

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
                result = rx.recv() => {
                    match result {
                        Some(r) => r,
                        None => break,
                    }
                }
            };

            let chunk = chunk_result?;

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
                    // Stop spinner on first reasoning
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        tui_spinner_active = false;
                    } else if let Some(s) = spinner.take() {
                        drop(s);
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
                StreamChunk::Usage(u) => {
                    debug!(
                        "Token usage: {} prompt, {} completion",
                        u.prompt_tokens, u.completion_tokens
                    );
                    sticky_state.add_tokens(u.completion_tokens as u64);
                    output::record_tokens(u.prompt_tokens as u64, u.completion_tokens as u64);
                    output::print_token_usage(u.prompt_tokens as u64, u.completion_tokens as u64);

                    captured_prompt_tokens = Some(u.prompt_tokens as u32);
                    captured_completion_tokens = Some(u.completion_tokens as u32);
                    captured_total_tokens = Some(u.total_tokens as u32);
                    captured_cost = u.cost;

                    self.emit_event(AgentEvent::TokenUsage {
                        prompt_tokens: u.prompt_tokens as u64,
                        completion_tokens: u.completion_tokens as u64,
                    });
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
                StreamChunk::Done => break,
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
        self.emit_progress(super::progress::ProgressEvent::LlmResponseReceived {
            finish_reason: captured_finish_reason
                .clone()
                .unwrap_or_else(|| "stream_end".into()),
            completion_tokens: captured_completion_tokens.unwrap_or(0),
        });

        if let Some(slot) = meta_out {
            *slot = crate::api::types::ChatMetadata {
                request_body: request_meta.request_body,
                elapsed_ms: request_meta.elapsed_ms,
                finish_reason: captured_finish_reason,
                prompt_tokens: captured_prompt_tokens,
                completion_tokens: captured_completion_tokens,
                total_tokens: captured_total_tokens,
                cost: captured_cost,
            };
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
mod tests {
    use super::*;

    // =========================================================================
    // extract_tool_name tests: <function=name> pattern
    // =========================================================================

    #[test]
    fn test_extract_tool_name_function_equals_pattern() {
        let xml = r#"<function=file_read>{"path": "foo.rs"}</function>"#;
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("file_read".to_string()));
    }

    #[test]
    fn test_extract_tool_name_function_equals_with_angle_bracket() {
        let xml = "<function=shell_exec>";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("shell_exec".to_string()));
    }

    #[test]
    fn test_extract_tool_name_function_equals_with_newline() {
        let xml = "<function=git_status\nsome other content";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("git_status".to_string()));
    }

    #[test]
    fn test_extract_tool_name_function_equals_with_surrounding_text() {
        let xml = "some text before <function=cargo_check> and after";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("cargo_check".to_string()));
    }

    #[test]
    fn test_extract_tool_name_function_equals_with_less_than_terminator() {
        let xml = "<function=my_tool<extra>";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("my_tool".to_string()));
    }

    // =========================================================================
    // extract_tool_name tests: <function>name</function> pattern
    // =========================================================================

    #[test]
    fn test_extract_tool_name_function_tag_pattern() {
        let xml = "<function>file_write</function>";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("file_write".to_string()));
    }

    #[test]
    fn test_extract_tool_name_function_tag_with_whitespace() {
        let xml = "<function>  grep_search  </function>";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(
            result,
            Some("grep_search".to_string()),
            "Whitespace around the name should be trimmed"
        );
    }

    #[test]
    fn test_extract_tool_name_function_tag_with_surrounding_content() {
        let xml = "prefix text <function>directory_tree</function> suffix text";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("directory_tree".to_string()));
    }

    // =========================================================================
    // extract_tool_name tests: empty / None cases
    // =========================================================================

    #[test]
    fn test_extract_tool_name_empty_string() {
        let result = Agent::extract_tool_name("");
        assert_eq!(result, None, "Empty string should return None");
    }

    #[test]
    fn test_extract_tool_name_no_function_tag() {
        let result = Agent::extract_tool_name("just some regular text with no tags");
        assert_eq!(
            result, None,
            "Text without function tags should return None"
        );
    }

    #[test]
    fn test_extract_tool_name_function_equals_empty_name() {
        // <function=> followed immediately by a terminator yields an empty name
        let result = Agent::extract_tool_name("<function=>");
        assert_eq!(
            result, None,
            "Empty name after function= should return None"
        );
    }

    #[test]
    fn test_extract_tool_name_function_tag_empty_body() {
        let result = Agent::extract_tool_name("<function></function>");
        assert_eq!(
            result, None,
            "Empty body inside <function></function> should return None"
        );
    }

    #[test]
    fn test_extract_tool_name_function_tag_whitespace_only_body() {
        let result = Agent::extract_tool_name("<function>   </function>");
        assert_eq!(
            result, None,
            "Whitespace-only body inside <function> tags should return None"
        );
    }

    // =========================================================================
    // extract_tool_name tests: malformed input
    // =========================================================================

    #[test]
    fn test_extract_tool_name_unclosed_function_tag() {
        // <function>name but no closing </function>
        let result = Agent::extract_tool_name("<function>file_read");
        assert_eq!(result, None, "Unclosed <function> tag should return None");
    }

    #[test]
    fn test_extract_tool_name_partial_function_equals() {
        // <function without = sign
        let result = Agent::extract_tool_name("<function something>");
        assert_eq!(
            result, None,
            "Partial <function without = should return None"
        );
    }

    #[test]
    fn test_extract_tool_name_other_xml_tags() {
        let result = Agent::extract_tool_name("<tool>file_read</tool>");
        assert_eq!(result, None, "Non-function XML tags should return None");
    }

    #[test]
    fn test_extract_tool_name_function_equals_takes_priority() {
        // When both patterns are present, <function=name> is checked first
        let xml = "<function=first_tool> <function>second_tool</function>";
        let result = Agent::extract_tool_name(xml);
        assert_eq!(
            result,
            Some("first_tool".to_string()),
            "<function=name> pattern should take priority"
        );
    }

    #[test]
    fn test_extract_tool_name_complex_xml_content() {
        let xml = r#"<tool_call>
<function=file_edit>
{"path": "src/main.rs", "old_str": "hello", "new_str": "world"}
</function>
</tool_call>"#;
        let result = Agent::extract_tool_name(xml);
        assert_eq!(result, Some("file_edit".to_string()));
    }

    // =========================================================================
    // Streaming display filter tests
    // =========================================================================

    #[test]
    fn find_earliest_open_tag_tool_call() {
        let buf = "hello <tool_call>stuff";
        let result = find_earliest_open_tag(buf);
        assert_eq!(result, Some((6, 0)));
    }

    #[test]
    fn find_earliest_open_tag_tool() {
        let buf = "hello <tool>stuff";
        let result = find_earliest_open_tag(buf);
        assert_eq!(result, Some((6, 1)));
    }

    #[test]
    fn find_earliest_open_tag_think() {
        let buf = "text <think>reasoning here";
        let result = find_earliest_open_tag(buf);
        assert_eq!(result, Some((5, 2)));
    }

    #[test]
    fn find_earliest_open_tag_thinking() {
        let buf = "text <thinking>reasoning here";
        let result = find_earliest_open_tag(buf);
        assert_eq!(result, Some((5, 3)));
    }

    #[test]
    fn find_earliest_open_tag_none() {
        let buf = "just regular text no tags";
        assert_eq!(find_earliest_open_tag(buf), None);
    }

    #[test]
    fn find_earliest_open_tag_picks_first_when_multiple() {
        let buf = "<think>reasoning</think> <tool>call</tool>";
        let result = find_earliest_open_tag(buf);
        assert_eq!(result.unwrap().0, 0); // <think> is first
    }

    #[test]
    fn has_partial_tag_at_end_detects_partial_tool() {
        assert!(has_partial_tag_at_end("hello <too"));
        assert!(has_partial_tag_at_end("hello <tool"));
        assert!(has_partial_tag_at_end("hello <t"));
    }

    #[test]
    fn has_partial_tag_at_end_detects_partial_think() {
        assert!(has_partial_tag_at_end("hello <thin"));
        assert!(has_partial_tag_at_end("hello <think"));
    }

    #[test]
    fn has_partial_tag_at_end_no_partial() {
        assert!(!has_partial_tag_at_end("hello world"));
        assert!(!has_partial_tag_at_end("hello <tool>complete"));
        assert!(!has_partial_tag_at_end(""));
    }

    #[test]
    fn extract_display_name_from_tool_block() {
        let xml = "<tool>\n<name>file_write</name>\n<arguments>{}</arguments>\n</tool>";
        assert_eq!(extract_display_name(xml), Some("file_write".to_string()));
    }

    #[test]
    fn extract_display_name_from_function_block() {
        let xml = r#"<tool_call><function=shell_exec>{"cmd":"ls"}</function></tool_call>"#;
        assert_eq!(extract_display_name(xml), Some("shell_exec".to_string()));
    }

    #[test]
    fn extract_display_name_from_think_block() {
        let xml = "<think>I should read the file first</think>";
        assert_eq!(extract_display_name(xml), None);
    }

    #[test]
    fn suppressed_tags_covers_all_local_model_formats() {
        // Ensure all common local model XML formats are covered
        let formats = ["<tool_call>", "<tool>", "<think>", "<thinking>"];
        for fmt in &formats {
            assert!(
                SUPPRESSED_TAGS.iter().any(|(open, _)| open == fmt),
                "Missing suppressed tag: {}",
                fmt
            );
        }
    }
}
