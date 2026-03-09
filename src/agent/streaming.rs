use anyhow::Result;
use colored::*;
use tracing::debug;

use super::*;

use super::tui_events::AgentEvent;

/// All XML tag pairs that local models may emit and should be hidden from
/// display.  Each entry is `(open_tag, close_tag)`.  The streaming renderer
/// suppresses everything between (and including) these tags.
const SUPPRESSED_TAGS: &[(&str, &str)] = &[
    ("<tool_call>", "</tool_call>"),
    ("<tool>", "</tool>"),
    ("<think>", "</think>"),
    ("<thinking>", "</thinking>"),
];

/// Find the earliest opening tag from `SUPPRESSED_TAGS` in `buf`.
/// Returns `(byte_offset, tag_index)` or `None`.
fn find_earliest_open_tag(buf: &str) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for (i, &(open, _)) in SUPPRESSED_TAGS.iter().enumerate() {
        if let Some(pos) = buf.find(open) {
            if best.map_or(true, |(b, _)| pos < b) {
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

    /// Chat with streaming, displaying output as it arrives
    /// Returns (content, reasoning, tool_calls) tuple
    pub(super) async fn chat_streaming(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<crate::api::types::ToolDefinition>>,
        thinking: ThinkingMode,
    ) -> Result<(String, Option<String>, Option<Vec<ToolCall>>)> {
        use std::io::{self, Write};

        // Activate the sticky status bar if running interactively
        let mode_label = match self.execution_mode() {
            crate::config::ExecutionMode::Normal => "normal",
            crate::config::ExecutionMode::AutoEdit => "auto-edit",
            crate::config::ExecutionMode::Yolo => "YOLO",
            crate::config::ExecutionMode::Daemon => "daemon",
        };
        let sticky_state = crate::ui::sticky_bar::StickyState::new(
            mode_label,
            &self.config.model,
        );
        // Sticky bar is available but disabled by default until terminal
        // compatibility is fully validated.  Set SELFWARE_STICKY_BAR=1 to enable.
        let sticky = if self.is_interactive()
            && std::env::var("SELFWARE_STICKY_BAR").map_or(false, |v| v == "1")
        {
            crate::ui::sticky_bar::StickyBar::activate(sticky_state.clone())
        } else {
            None
        };

        // Start loading spinner with a random phrase while waiting for first token
        let mut spinner = Some(crate::ui::spinner::TerminalSpinner::start(
            crate::ui::loading_phrases::random_phrase(),
        ));
        let mut phrase_rotation = tokio::time::Instant::now();
        let mut last_bar_update = tokio::time::Instant::now();

        let stream = self.client.chat_stream(messages, tools, thinking).await?;

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_buf = String::new();
        // Which suppressed tag we're currently inside, if any
        let mut suppressed_tag_idx: Option<usize> = None;

        let cancel = self.cancel_token();

        while let Some(chunk_result) = rx.recv().await {
            // Check for ESC / Ctrl+C cancellation
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                // Drop spinner if still active
                drop(spinner.take());
                break;
            }

            let chunk = chunk_result?;

            // Rotate loading phrase every 3 seconds while spinner is active
            if let Some(ref s) = spinner {
                if phrase_rotation.elapsed() > tokio::time::Duration::from_secs(3) {
                    s.set_message(crate::ui::loading_phrases::random_phrase());
                    phrase_rotation = tokio::time::Instant::now();
                }
            }

            // Refresh sticky bar every 500ms
            if let Some(ref bar) = sticky {
                if last_bar_update.elapsed() > tokio::time::Duration::from_millis(500) {
                    bar.update();
                    last_bar_update = tokio::time::Instant::now();
                }
            }

            match chunk {
                StreamChunk::Content(text) => {
                    // Stop spinner on first content
                    if let Some(s) = spinner.take() {
                        drop(s);
                    }
                    if in_reasoning {
                        in_reasoning = false;
                        sticky_state.is_thinking.store(false, std::sync::atomic::Ordering::Relaxed);
                        sticky_state.thinking_secs.store(
                            sticky_state.started.elapsed().as_secs(),
                            std::sync::atomic::Ordering::Relaxed,
                        );
                        if !output::is_compact() {
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
                                        print!(
                                            "  {} {}...",
                                            "🔧".dimmed(),
                                            fname.bright_cyan()
                                        );
                                        io::stdout().flush().ok();
                                    }
                                }
                                // For <think> blocks, optionally show as dimmed reasoning
                                if is_think && !output::is_compact() {
                                    // Extract inner text, strip the open/close tags
                                    let (open, _) = SUPPRESSED_TAGS[tag_idx];
                                    let inner = &block
                                        [open.len()..block.len().saturating_sub(close.len())];
                                    let trimmed = inner.trim();
                                    if !trimmed.is_empty() {
                                        reasoning.push_str(trimmed);
                                    }
                                }
                                display_buf = display_buf[end..].to_string();
                                suppressed_tag_idx = None;
                            } else {
                                break; // Wait for more data
                            }
                        } else {
                            // Look for the earliest opening suppressed tag
                            if let Some((start_pos, tag_idx)) =
                                find_earliest_open_tag(&display_buf)
                            {
                                // Print everything before the tag
                                let before = &display_buf[..start_pos];
                                if !before.is_empty() {
                                    print!("{}", before);
                                    io::stdout().flush().ok();
                                }
                                display_buf = display_buf[start_pos..].to_string();
                                suppressed_tag_idx = Some(tag_idx);
                            } else if has_partial_tag_at_end(&display_buf) {
                                // Partial opening tag at end — buffer it
                                break;
                            } else {
                                // No tags — print everything
                                if !display_buf.is_empty() {
                                    print!("{}", display_buf);
                                    io::stdout().flush().ok();
                                }
                                display_buf.clear();
                                break;
                            }
                        }
                    }
                }
                StreamChunk::Reasoning(text) => {
                    // Stop spinner on first reasoning
                    if let Some(s) = spinner.take() {
                        drop(s);
                    }
                    sticky_state.is_thinking.store(true, std::sync::atomic::Ordering::Relaxed);
                    sticky_state.set_activity("Thinking...");
                    if !output::is_compact() {
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

                    self.emit_event(AgentEvent::TokenUsage {
                        prompt_tokens: u.prompt_tokens as u64,
                        completion_tokens: u.completion_tokens as u64,
                    });
                }
                StreamChunk::Done => break,
            }
        }

        // Flush any remaining display buffer (non-suppressed text)
        if !display_buf.is_empty() && suppressed_tag_idx.is_none() {
            print!("{}", display_buf);
            io::stdout().flush().ok();
        }

        // Ensure we end with a newline if we printed content
        if !content.is_empty() || !reasoning.is_empty() {
            println!();
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
        assert_eq!(
            extract_display_name(xml),
            Some("file_write".to_string())
        );
    }

    #[test]
    fn extract_display_name_from_function_block() {
        let xml = r#"<tool_call><function=shell_exec>{"cmd":"ls"}</function></tool_call>"#;
        assert_eq!(
            extract_display_name(xml),
            Some("shell_exec".to_string())
        );
    }

    #[test]
    fn extract_display_name_from_think_block() {
        let xml = "<think>I should read the file first</think>";
        assert_eq!(extract_display_name(xml), None);
    }

    #[test]
    fn suppressed_tags_covers_all_local_model_formats() {
        // Ensure all common local model XML formats are covered
        let formats = [
            "<tool_call>",
            "<tool>",
            "<think>",
            "<thinking>",
        ];
        for fmt in &formats {
            assert!(
                SUPPRESSED_TAGS.iter().any(|(open, _)| open == fmt),
                "Missing suppressed tag: {}",
                fmt
            );
        }
    }
}
