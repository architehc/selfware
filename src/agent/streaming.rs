use anyhow::Result;
use colored::*;
use tracing::debug;

use super::*;

use super::tui_events::AgentEvent;

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

        // Start loading spinner with a random phrase while waiting for first token
        let mut spinner = Some(crate::ui::spinner::TerminalSpinner::start(
            crate::ui::loading_phrases::random_phrase(),
        ));
        let mut phrase_rotation = tokio::time::Instant::now();

        let stream = self.client.chat_stream(messages, tools, thinking).await?;

        let mut rx = stream.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut in_reasoning = false;
        let mut display_buf = String::new();
        let mut in_tool_tag = false;

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            // Rotate loading phrase every 3 seconds while spinner is active
            if let Some(ref s) = spinner {
                if phrase_rotation.elapsed() > tokio::time::Duration::from_secs(3) {
                    s.set_message(crate::ui::loading_phrases::random_phrase());
                    phrase_rotation = tokio::time::Instant::now();
                }
            }

            match chunk {
                StreamChunk::Content(text) => {
                    // Stop spinner on first content
                    if let Some(s) = spinner.take() {
                        drop(s);
                    }
                    if in_reasoning {
                        // Finished reasoning, now showing content
                        in_reasoning = false;
                        if !output::is_compact() {
                            println!(); // End reasoning line
                        }
                    }
                    // Always accumulate full content for parsing
                    content.push_str(&text);

                    // Filter out <tool_call> XML blocks from display
                    // Buffer content and only print text outside tool_call tags
                    display_buf.push_str(&text);

                    // Process display buffer: suppress tool_call blocks
                    loop {
                        if in_tool_tag {
                            // We're inside a <tool_call> - look for closing tag
                            if let Some(end_pos) = display_buf.find("</tool_call>") {
                                let end = end_pos + "</tool_call>".len();
                                // Extract the tool call text to show a clean summary
                                let tool_xml = &display_buf[..end];
                                if let Some(fname) = Self::extract_tool_name(tool_xml) {
                                    print!("  {} {}...", "🔧".dimmed(), fname.bright_cyan());
                                    io::stdout().flush().ok();
                                }
                                display_buf = display_buf[end..].to_string();
                                in_tool_tag = false;
                            } else {
                                break; // Wait for more data
                            }
                        } else {
                            // Look for start of <tool_call>
                            if let Some(start_pos) = display_buf.find("<tool_call>") {
                                // Print everything before the tag
                                let before = &display_buf[..start_pos];
                                if !before.is_empty() {
                                    print!("{}", before);
                                    io::stdout().flush().ok();
                                }
                                display_buf = display_buf[start_pos..].to_string();
                                in_tool_tag = true;
                            } else if display_buf.contains('<') && !display_buf.contains('>') {
                                // Partial tag at end - buffer it
                                break;
                            } else {
                                // No tags - print everything
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

        // Flush any remaining display buffer (non-tool-call text)
        if !display_buf.is_empty() && !in_tool_tag {
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
