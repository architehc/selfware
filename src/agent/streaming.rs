use anyhow::Result;
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

/// All tool-call / reasoning markup regions that local models may emit
/// inside `content` and that must be hidden from display. Each entry is
/// `(open_tag, close_tag)`; the streaming renderer suppresses everything
/// between (and including) them. The recorded `content` is never touched —
/// the parser still sees every byte.
///
/// Indices 2..=4 are reasoning regions (see [`is_think_tag`]); the rest are
/// tool-call regions. Keep the first five in place: tests and the wait-phase
/// classifier address them by index.
const SUPPRESSED_TAGS: &[(&str, &str)] = &[
    ("<tool_call>", "</tool_call>"),
    ("<tool>", "</tool>"),
    ("<think>", "</think>"),
    ("<thinking>", "</thinking>"),
    ("<|channel>", "<channel|>"),
    // Bare Qwen call without the `<tool_call>` wrapper (the parser accepts it).
    ("<function=", "</function>"),
    // Kimi/Moonshot tool section and bare call.
    ("<|open|>tools", "<|close|>tools"),
    ("<|open|>call ", "<|close|>call"),
];

/// Whether a [`SUPPRESSED_TAGS`] index is a reasoning region (its inner text
/// is reasoning, not a tool call).
fn is_think_tag(idx: usize) -> bool {
    matches!(idx, 2..=4)
}

/// Wrapper / closing tokens that carry nothing displayable on their own and
/// must be dropped when they appear OUTSIDE a suppressed region: a stray
/// `</tool_call>` after a bare `<function=…></function>` call or a `<tool>`
/// block (seen live on qwen38-flash-next, 0.9.1), a doubled closer, Kimi
/// section separators. Only exact tokens are listed — none can occur in
/// ordinary prose.
const STRAY_MARKUP: &[&str] = &[
    "</tool_call>",
    "</tool>",
    "</function>",
    "</think>",
    "</thinking>",
    "<channel|>",
    "<|close|>tools",
    "<|close|>call",
    "<|close|>message",
    "<|close|>argument",
    "<|sep|>",
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

/// Find the earliest stray markup token in `buf`: `(byte_offset, len)`.
/// At equal offsets the longest token wins.
fn find_earliest_stray(buf: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for tok in STRAY_MARKUP {
        if let Some(pos) = buf.find(tok) {
            let better = match best {
                None => true,
                Some((b, len)) => pos < b || (pos == b && tok.len() > len),
            };
            if better {
                best = Some((pos, tok.len()));
            }
        }
    }
    best
}

/// Byte offset where a trailing, possibly-incomplete markup token starts:
/// `buf` ends with a proper prefix of an opening tag or a stray token, so
/// the rest of it may arrive in the next chunk. `None` when the tail is
/// plain text.
fn partial_markup_start(buf: &str) -> Option<usize> {
    let tokens = SUPPRESSED_TAGS
        .iter()
        .map(|(open, _)| *open)
        .chain(STRAY_MARKUP.iter().copied());
    let mut longest = 0;
    for tok in tokens {
        for prefix_len in (1..tok.len()).rev() {
            if prefix_len <= longest {
                break;
            }
            if buf.ends_with(&tok[..prefix_len]) {
                longest = prefix_len;
                break;
            }
        }
    }
    (longest > 0).then(|| buf.len() - longest)
}

/// Check if `buf` ends with a prefix of any opening suppressed tag or stray
/// markup token, indicating we should buffer instead of printing (the rest
/// of the tag may arrive in the next chunk).
#[cfg(test)]
fn has_partial_tag_at_end(buf: &str) -> bool {
    partial_markup_start(buf).is_some()
}

/// Remove closing suppressed tags that have no opener in `text` (e.g. a
/// native-FC model echoing `</tool_call>` after its call). The opener-driven
/// filter never sees them, so they leaked into the chat as a bare
/// `</tool_call>` message (0.9.1 TUI field finding).
pub(crate) fn strip_orphan_close_tags(text: &str) -> std::borrow::Cow<'_, str> {
    if !SUPPRESSED_TAGS
        .iter()
        .any(|(_, close)| text.contains(close))
    {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = text.to_string();
    for (_, close) in SUPPRESSED_TAGS {
        out = out.replace(close, "");
    }
    std::borrow::Cow::Owned(out)
}

/// The user-visible part of a complete model response: every suppressed
/// block (tool-call markup, `<think>` reasoning) and any stray closing tag
/// removed — the same text the streaming filter would have shown.
pub(crate) fn visible_response_text(content: &str) -> String {
    let mut rest = content;
    let mut out = String::new();
    while let Some((start, idx)) = find_earliest_open_tag(rest) {
        out.push_str(&rest[..start]);
        let (open, close) = SUPPRESSED_TAGS[idx];
        let after_open = &rest[start + open.len()..];
        match after_open.find(close) {
            Some(end) => rest = &after_open[end + close.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    strip_orphan_close_tags(&out).trim().to_string()
}

/// Extract a tool name from a suppressed XML block for a clean one-line
/// summary.  Tries `<name>x</name>` (used by `<tool>` blocks), the
/// existing `<function=x>` / `<function>x</function>` patterns, and Kimi's
/// `call tool="x"`.
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
    if let Some(start) = xml.find("call tool=\"") {
        let rest = &xml[start + "call tool=\"".len()..];
        if let Some(end) = rest.find('"') {
            let name = rest[..end].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    Agent::extract_tool_name(xml)
}

/// One displayable piece of a streamed response after markup filtering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DisplayPiece {
    /// Prose to show.
    Text(String),
    /// A suppressed tool-call region closed; carries the tool name when one
    /// could be read from the markup.
    ToolCall(Option<String>),
    /// The trimmed inner text of an inline reasoning block
    /// (`<think>…</think>` and friends).
    Think(String),
}

/// Incremental display filter for streamed `content`: turns raw chunks into
/// [`DisplayPiece`]s so no tool-call markup (opening tag, body, closing tag,
/// a stray closer after the region, Kimi section tokens) ever reaches the
/// terminal or the TUI, even when a tag is split across chunks. It only
/// decides what is SHOWN; the caller keeps accumulating the raw content for
/// the parser.
#[derive(Debug, Default)]
pub(crate) struct StreamDisplayFilter {
    buf: String,
    inside: Option<usize>,
}

impl StreamDisplayFilter {
    /// Whether the stream is currently inside a suppressed region.
    #[cfg(test)]
    pub(crate) fn inside_region(&self) -> bool {
        self.inside.is_some()
    }

    /// Whether the stream is currently inside an inline reasoning region.
    pub(crate) fn inside_think(&self) -> bool {
        self.inside.is_some_and(is_think_tag)
    }

    /// Whether the stream is currently inside a tool-call region.
    pub(crate) fn inside_tool_call(&self) -> bool {
        self.inside.is_some_and(|i| !is_think_tag(i))
    }

    /// Feed one chunk; returns the pieces that are ready to display.
    pub(crate) fn push(&mut self, text: &str) -> Vec<DisplayPiece> {
        self.buf.push_str(text);
        let mut out = Vec::new();
        loop {
            if let Some(idx) = self.inside {
                let (open, close) = SUPPRESSED_TAGS[idx];
                let Some(end_pos) = self.buf.find(close) else {
                    break; // wait for the closing tag
                };
                let end = end_pos + close.len();
                let block: String = self.buf.drain(..end).collect();
                self.inside = None;
                if is_think_tag(idx) {
                    let inner = &block[open.len()..block.len() - close.len()];
                    let trimmed = inner.trim();
                    if !trimmed.is_empty() {
                        out.push(DisplayPiece::Think(trimmed.to_string()));
                    }
                } else {
                    out.push(DisplayPiece::ToolCall(extract_display_name(&block)));
                }
                continue;
            }
            let open = find_earliest_open_tag(&self.buf);
            let stray = find_earliest_stray(&self.buf);
            let open_first = match (open, stray) {
                (Some((pos, _)), Some((stray_pos, _))) => pos <= stray_pos,
                (Some(_), None) => true,
                (None, _) => false,
            };
            if let (true, Some((pos, idx))) = (open_first, open) {
                Self::push_text(&mut out, &self.buf[..pos]);
                self.buf.drain(..pos);
                self.inside = Some(idx);
            } else if let Some((pos, len)) = stray {
                Self::push_text(&mut out, &self.buf[..pos]);
                self.buf.drain(..pos + len);
            } else {
                let keep_from = partial_markup_start(&self.buf).unwrap_or(self.buf.len());
                Self::push_text(&mut out, &self.buf[..keep_from]);
                self.buf.drain(..keep_from);
                break;
            }
        }
        out
    }

    /// End of stream: a held partial token was plain text after all and is
    /// shown; an unclosed region stays hidden (its text is not displayed —
    /// callers that must show the answer compare against what WAS shown).
    pub(crate) fn finish(&mut self) -> Vec<DisplayPiece> {
        let mut out = Vec::new();
        if self.inside.is_none() {
            Self::push_text(&mut out, &self.buf);
        }
        self.buf.clear();
        out
    }

    fn push_text(out: &mut Vec<DisplayPiece>, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(DisplayPiece::Text(prev)) = out.last_mut() {
            prev.push_str(text);
        } else {
            out.push(DisplayPiece::Text(text.to_string()));
        }
    }
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

/// Text-mode prose pipeline for one streamed response: echo gate (a
/// word-for-word repeat of the last tool-free answer is not printed again),
/// then the line renderer (Markdown styling, blank-line collapse). `shown`
/// is every prose byte of this response that is on screen — printed now or
/// already there — and goes to the answer ledger at the end.
struct TextProse {
    renderer: output::live::ProseRenderer,
    echo: output::live::EchoGate,
    shown: String,
}

impl TextProse {
    fn new() -> Self {
        Self {
            renderer: output::live::ProseRenderer::new(output::markdown_styled())
                .with_linker(output::hyperlink::Linker::for_terminal()),
            echo: output::live::EchoGate::new(output::live::echo_target()),
            shown: String::new(),
        }
    }

    fn push(&mut self, text: &str) -> String {
        self.shown.push_str(text);
        let pass = self.echo.push(text);
        self.renderer.push(&pass)
    }

    fn finish(&mut self) -> String {
        let pass = self.echo.finish();
        let mut out = self.renderer.push(&pass);
        out.push_str(&self.renderer.finish());
        out
    }
}

/// Print rendered prose lines to a (possibly raw-mode) terminal.
fn print_prose(rendered: &str) {
    use std::io::Write;
    if rendered.is_empty() {
        return;
    }
    // Replace \n with \r\n so every newline resets to col 0
    let safe = rendered.replace('\n', "\r\n");
    let _lock = output::OUTPUT_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    print!("{}", safe);
    std::io::stdout().flush().ok();
    output::note_streamed_text(&safe);
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

    /// Route filtered stream pieces: prose to the TUI, or (text mode) into
    /// `prose`, returning the rendered lines ready to print; a closed
    /// tool-call region becomes a TUI progress event only (text mode
    /// announces each call once, with the dispatch line "Step N → tool
    /// arg"); inline reasoning goes into the accumulated reasoning.
    fn show_display_pieces(
        &self,
        pieces: Vec<DisplayPiece>,
        reasoning: &mut String,
        prose: Option<&mut TextProse>,
    ) -> String {
        let tui_active = crate::output::is_tui_active();
        let mut rendered = String::new();
        let mut prose = prose;
        for piece in pieces {
            match piece {
                DisplayPiece::Text(text) => {
                    // The TUI renders it; structured output turns it into
                    // stream-json `text_delta` lines (stdout is JSON-only).
                    if tui_active || crate::output::is_json_mode() {
                        self.emit_event(AgentEvent::AssistantDelta { text });
                    } else if let Some(p) = prose.as_deref_mut() {
                        rendered.push_str(&p.push(&text));
                    }
                }
                DisplayPiece::ToolCall(Some(name)) if tui_active => {
                    self.emit_event(AgentEvent::ToolProgress {
                        name,
                        status: "parsing".into(),
                    });
                }
                DisplayPiece::ToolCall(_) => {}
                DisplayPiece::Think(inner) => {
                    if !output::is_compact() {
                        reasoning.push_str(&inner);
                    }
                }
            }
        }
        rendered
    }

    /// A reasoning block ended in default text mode. On a terminal the
    /// indicator was transient (the spinner line); a non-tty log gets one
    /// summary line instead, so it still records that reasoning happened.
    fn end_reasoning_indicator(&self, block: &str) {
        if output::is_plain_mode()
            && !output::is_verbose()
            && !output::is_compact()
            && !block.trim().is_empty()
        {
            output::thinking(block, false);
        }
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

    /// Surface a response that did NOT stream (non-streaming call, fallback
    /// or cache hit) on the event channel in structured-output mode, so a
    /// stream-json consumer still receives the answer as a `text_delta`
    /// rather than only in the final result object.
    pub(super) fn emit_unstreamed_text(&self, content: &str) {
        if !crate::output::is_json_mode() || crate::output::is_tui_active() {
            return;
        }
        let visible = visible_response_text(content);
        if !visible.is_empty() {
            self.emit_event(AgentEvent::AssistantDelta {
                text: format!("{visible}\n"),
            });
        }
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
            self.emit_unstreamed_text(&cached.response);
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

        // Start the spinner with an honest waiting label (no invented
        // activity) until the first token arrives.
        let initial_phrase = crate::ui::loading_phrases::waiting_label();
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
        let cancel = self.cancel_token();
        let mut send_fut = Box::pin(self.client.chat_stream_with_meta(messages, tools, thinking));
        let (stream, request_meta) = loop {
            tokio::select! {
                biased;
                _ = async {
                    loop {
                        if cancel.load(std::sync::atomic::Ordering::Relaxed)
                            || crate::is_shutdown_requested()
                        {
                            return;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                } => {
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                    } else {
                        drop(spinner.take());
                    }
                    return Err(crate::errors::AgentError::for_current_shutdown().into());
                }
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
        // Clock for the part after the headers: request_meta carries only the
        // time to headers. The whole call is the sum; the decode-speed sample
        // below deliberately uses this part alone.
        let stream_started = std::time::Instant::now();

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_filter = StreamDisplayFilter::default();
        // Reasoning display state: running char count for the one-line
        // indicator, where the current reasoning block starts (plain-mode
        // summary line), the verbose stream's blank-line collapse, and
        // whether the spinner currently shows the indicator (the waiting
        // heartbeat must not overwrite it).
        let mut reasoning_chars = 0usize;
        let mut reasoning_block_start = 0usize;
        let mut reasoning_collapser = output::live::BlankCollapser::default();
        let mut reasoning_indicator_live = false;
        // Text-mode prose pipeline (None in TUI / JSON / quiet mode).
        let mut text_prose = (!tui_active && !suppress_stream_stdout).then(TextProse::new);
        let mut captured_logprobs: Option<serde_json::Value> = None;
        // How the loop below ended. `stream_ended_with_done` tracks the
        // [DONE] sentinel; `runaway_cut` tracks the deliberate monologue
        // cutoff. Both matter for the terminal-indication guard after the
        // loop: a stream that produced content but never reached an accepted
        // terminal is truncated, not complete.
        let mut stream_ended_with_done = false;
        let mut runaway_cut = false;

        loop {
            // Use select to check cancellation even when recv is waiting
            let chunk_result = tokio::select! {
                biased;
                _ = async {
                    loop {
                        if cancel.load(std::sync::atomic::Ordering::Relaxed)
                            || crate::is_shutdown_requested()
                        {
                            return;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                } => {
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        // tui_spinner_active stays true here — the break exits the loop
                    } else {
                        drop(spinner.take());
                    }
                    return Err(crate::errors::AgentError::for_current_shutdown().into());
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
                                in_reasoning || display_filter.inside_think(),
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
                                spinner.as_ref().filter(|_| !reasoning_indicator_live),
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
                // Any supported call syntax counts as in flight: the old
                // `contains("<tool")` missed Kimi `<|open|>call` and bare
                // Qwen `<function=`, so a long file_write in those formats
                // was cut mid-call and lost (review, 0.9.1).
                !tool_calls.is_empty() || crate::tool_parser::text_opens_tool_call(&content),
                self.task_requires_mutation_now(),
            ) {
                let streamed = content.len() + reasoning.len();
                warn!("runaway monologue truncated at {streamed} chars (no tool call in flight)");
                content.push_str(&monologue_cut_notice(streamed));
                runaway_cut = true;
                break;
            }

            // Refresh the spinner every 2 seconds while it is active with what
            // was actually observed (phase + tokens so far) — e.g. a long
            // reasoning block shows "Model reasoning · ~1.2K tokens".
            let spinner_live = if tui_active {
                tui_spinner_active
            } else {
                spinner.is_some()
            };
            if spinner_live && phrase_rotation.elapsed() > tokio::time::Duration::from_secs(2) {
                let phase = super::llm_wait::LlmWaitPhase::classify(
                    &content,
                    &reasoning,
                    tool_calls.len(),
                    in_reasoning || display_filter.inside_think(),
                );
                let (tokens, source) = super::llm_wait::tokens_so_far(
                    captured_completion_tokens,
                    &content,
                    &reasoning,
                );
                let text = super::llm_wait::live_spinner_status(phase, tokens, source);
                if tui_active {
                    self.emit_event(AgentEvent::SpinnerUpdate { message: text });
                } else if let Some(s) = spinner.as_ref().filter(|_| !reasoning_indicator_live) {
                    // The "Thinking… (N chars)" indicator owns the line while live.
                    s.set_message(&text);
                }
                phrase_rotation = tokio::time::Instant::now();
            }

            // NOTE: Do not call bar.update() during streaming — cursor
            // save/restore doesn't work reliably while stdout is actively
            // printing content and causes the bar to spam every line.
            // The bar is shown once at the end via bar.finish().

            match chunk {
                StreamChunk::Content(text) => {
                    // The TUI spinner stops on first content. The terminal
                    // spinner stays until there is a rendered line to print
                    // (below): a response that opens with a tool call keeps
                    // its sign of life instead of going blank.
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        tui_spinner_active = false;
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
                        } else if !suppress_stream_stdout {
                            // End the inline reasoning line, if one is open
                            // (no extra blank line).
                            output::close_stream_line();
                            self.end_reasoning_indicator(&reasoning[reasoning_block_start..]);
                        }
                        reasoning_indicator_live = false;
                        if let Some(s) = spinner.as_ref() {
                            s.set_message(initial_phrase);
                        }
                    }
                    sticky_state.set_activity("Generating...");
                    // Always accumulate full content for parsing
                    content.push_str(&text);

                    // Filter tool-call / reasoning markup from the display;
                    // `content` above keeps every byte for the parser.
                    let pieces = display_filter.push(&text);
                    let rendered =
                        self.show_display_pieces(pieces, &mut reasoning, text_prose.as_mut());
                    if !rendered.is_empty() {
                        if let Some(s) = spinner.take() {
                            // Drop stops the spinner task and clears its line;
                            // let the task fully exit before printing.
                            drop(s);
                            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                        }
                        print_prose(&rendered);
                    } else if spinner.is_none()
                        && text_prose.is_some()
                        && display_filter.inside_tool_call()
                    {
                        // A long tool call (a file_write body) streams with
                        // nothing to print: show a sign of life, not silence.
                        spinner = Some(crate::ui::spinner::TerminalSpinner::start(
                            "Composing tool call…",
                        ));
                    }
                }
                StreamChunk::Reasoning(text) => {
                    // TUI: the reasoning pane gets every delta. Text mode:
                    // under --verbose the full reasoning streams (blank runs
                    // collapsed); by default only a one-line indicator —
                    // the spinner reads "Thinking… (1.2k chars)", updated in
                    // place and cleared when the answer starts (a model's
                    // whole self-debate filled screens, 0.9.1). Compact:
                    // nothing (unchanged).
                    if tui_active && tui_spinner_active {
                        self.emit_event(AgentEvent::SpinnerStop);
                        tui_spinner_active = false;
                    }
                    sticky_state
                        .is_thinking
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                    sticky_state.set_activity("Thinking...");
                    if !in_reasoning {
                        reasoning_block_start = reasoning.len();
                    }
                    reasoning_chars += text.chars().count();
                    if tui_active {
                        in_reasoning = true;
                        self.emit_event(AgentEvent::ThinkingDelta { text: text.clone() });
                    } else if output::is_compact() {
                        // Not shown; the spinner keeps the waiting heartbeat.
                    } else if suppress_stream_stdout {
                        // JSON / quiet: nothing is shown (as before).
                        drop(spinner.take());
                    } else if output::is_verbose() {
                        if let Some(s) = spinner.take() {
                            drop(s);
                            tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                        }
                        if !in_reasoning {
                            reasoning_collapser = output::live::BlankCollapser::default();
                            output::thinking_prefix();
                        }
                        in_reasoning = true;
                        let shown = reasoning_collapser.push(&text);
                        if !shown.is_empty() {
                            output::thinking(&shown, true);
                            io::stdout().flush().ok();
                        }
                    } else {
                        in_reasoning = true;
                        reasoning_indicator_live = true;
                        let label = output::live::reasoning_indicator(reasoning_chars);
                        match spinner.as_ref() {
                            Some(s) => s.set_message(&label),
                            None => {
                                spinner = Some(crate::ui::spinner::TerminalSpinner::start(&label));
                            }
                        }
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
                    self.session_main_loop_tokens.fetch_add(
                        prompt_delta.saturating_add(completion_delta),
                        std::sync::atomic::Ordering::Relaxed,
                    );
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

        // Flush the display filter: a held partial token that never became
        // markup is shown; an unclosed region stays hidden.
        let pieces = display_filter.finish();
        let mut rendered = self.show_display_pieces(pieces, &mut reasoning, text_prose.as_mut());
        if let Some(prose) = text_prose.as_mut() {
            rendered.push_str(&prose.finish());
        }
        if !rendered.is_empty() {
            if let Some(s) = spinner.take() {
                drop(s);
                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
            }
            print_prose(&rendered);
        }
        // Prose always ends its last line; only an inline reasoning line can
        // still be open. (The old unconditional println! left a blank line
        // after every response.)
        if !tui_active && !suppress_stream_stdout {
            output::close_stream_line();
            if in_reasoning {
                self.end_reasoning_indicator(&reasoning[reasoning_block_start..]);
            }
        }
        // What reached the screen, for the final answer's print-once check
        // and the next response's echo gate. A response with a tool call is
        // narration, never an echo target.
        if let Some(prose) = text_prose.take() {
            let tool_free =
                tool_calls.is_empty() && !crate::tool_parser::text_opens_tool_call(&content);
            output::record_shown_prose(&prose.shown, tool_free);
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
        // Whole call = time to headers + the stream after them. One value for
        // every consumer: the wrap-up forecast's call shape
        // (agent::call_forecast), the `llm_response_received` event (whose
        // contract is the full call — it used to get the stream part only)
        // and the turn artifact's `elapsed_ms` (which used to get the header
        // part only: 879 ms for a 701 s call).
        let time_to_headers_ms = request_meta
            .time_to_headers_ms
            .unwrap_or(request_meta.elapsed_ms);
        let whole_call_ms =
            time_to_headers_ms.saturating_add(stream_started.elapsed().as_millis() as u64);
        self.client.record_call_shape(
            captured_prompt_tokens.unwrap_or(0) as u64,
            captured_completion_tokens.unwrap_or(0) as u64,
            whole_call_ms,
        );
        self.emit_progress(super::progress::ProgressEvent::LlmResponseReceived {
            finish_reason: captured_finish_reason
                .clone()
                .unwrap_or_else(|| "stream_end".into()),
            completion_tokens: captured_completion_tokens.unwrap_or(0),
            elapsed_ms: whole_call_ms,
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
                elapsed_ms: whole_call_ms,
                time_to_headers_ms: Some(time_to_headers_ms),
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
            cancel.load(std::sync::atomic::Ordering::Relaxed) || crate::is_shutdown_requested(),
        ) {
            return Err(crate::errors::ApiError::EmptyStream.into());
        }

        if cancel.load(std::sync::atomic::Ordering::Relaxed) || crate::is_shutdown_requested() {
            return Err(crate::errors::AgentError::for_current_shutdown().into());
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
