//! Streaming response infrastructure: SSE parsing, chunk types, and tool call accumulation.

use anyhow::Result;
use futures::StreamExt;
use once_cell::sync::Lazy;
use std::time::Duration;
use tokio::sync::{mpsc, Semaphore};
use tracing::warn;

use super::types::{self, ChatResponse, Choice, Message, ToolCall, Usage};
use crate::errors::ApiError;

/// Semaphore to limit concurrent streaming API tasks to prevent resource exhaustion.
static STREAM_SEMAPHORE: Lazy<Semaphore> = Lazy::new(|| Semaphore::new(100));

/// A streaming response that yields chunks as they arrive
pub struct StreamingResponse {
    response: reqwest::Response,
    chunk_timeout: Duration,
}

impl std::fmt::Debug for StreamingResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingResponse")
            .field("status", &self.response.status())
            .field("chunk_timeout_secs", &self.chunk_timeout.as_secs())
            .finish()
    }
}

impl StreamingResponse {
    pub(crate) fn new(response: reqwest::Response, chunk_timeout: Duration) -> Self {
        Self {
            response,
            chunk_timeout,
        }
    }

    /// Process the stream and send chunks through a channel
    ///
    /// Uses a semaphore to limit concurrent streaming tasks and prevent resource exhaustion.
    pub async fn into_channel(self) -> mpsc::Receiver<Result<StreamChunk>> {
        let (tx, rx) = mpsc::channel(32);

        // Acquire the permit before spawning so the number of outstanding
        // streaming tasks is actually bounded by the semaphore rather than
        // unbounded during the spawn itself.
        let permit = match STREAM_SEMAPHORE.acquire().await {
            Ok(p) => Some(p),
            Err(e) => {
                let _ = tx
                    .send(Err(ApiError::Network(format!(
                        "Stream semaphore error: {}",
                        e
                    ))
                    .into()))
                    .await;
                return rx;
            }
        };

        // Spawn the stream processor with a permit to limit concurrent tasks
        tokio::spawn(async move {
            let _permit = permit;
            let mut stream = self.response.bytes_stream();
            let mut buffer = String::new();
            let mut pending_utf8 = Vec::new();
            let mut accumulator = ToolCallAccumulator::new();
            let chunk_timeout = self.chunk_timeout;

            loop {
                let chunk_opt = match tokio::time::timeout(chunk_timeout, stream.next()).await {
                    Ok(Some(result)) => Some(result),
                    Ok(None) => None, // Stream ended
                    Err(_elapsed) => {
                        for call in accumulator.flush() {
                            if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                                warn!(
                                    "Streaming receiver dropped while sending buffered tool call after timeout"
                                );
                                return;
                            }
                        }
                        if tx.send(Err(ApiError::Timeout.into())).await.is_err() {
                            warn!("Streaming receiver dropped while sending timeout error");
                        }
                        return;
                    }
                };
                let Some(chunk_result) = chunk_opt else {
                    break;
                };
                match chunk_result {
                    Ok(bytes) => {
                        append_utf8_chunk(&mut buffer, &mut pending_utf8, &bytes);

                        // Normalize CRLF line endings so the delimiter scan only
                        // needs to look for \n\n.
                        buffer = buffer.replace("\r\n", "\n");

                        // Process complete SSE events
                        while let Some(pos) = buffer.find("\n\n") {
                            let event = buffer[..pos].to_string();
                            buffer = buffer[pos + 2..].to_string();

                            for chunk in parse_sse_event(&event, &mut accumulator) {
                                if tx.send(Ok(chunk)).await.is_err() {
                                    warn!(
                                        "Streaming receiver dropped while forwarding parsed stream chunk"
                                    );
                                    return; // Receiver dropped
                                }
                            }
                        }
                    }
                    Err(e) => {
                        for call in accumulator.flush() {
                            if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                                warn!(
                                    "Streaming receiver dropped while sending buffered tool call after stream error"
                                );
                                return;
                            }
                        }
                        if tx
                            .send(Err(ApiError::Network(format!("Stream error: {}", e)).into()))
                            .await
                            .is_err()
                        {
                            warn!("Streaming receiver dropped while sending stream error");
                        }
                        return;
                    }
                }
            }

            if !pending_utf8.is_empty() {
                buffer.push_str(&String::from_utf8_lossy(&pending_utf8));
            }

            // Flush trailing buffer (data without final \n\n)
            let remaining = buffer.trim().to_string();
            if !remaining.is_empty() {
                for chunk in parse_sse_event(&remaining, &mut accumulator) {
                    if tx.send(Ok(chunk)).await.is_err() {
                        warn!("Streaming receiver dropped while sending trailing buffered chunk");
                        return;
                    }
                }
            }

            // Flush any remaining accumulated tool calls
            for call in accumulator.flush() {
                if tx.send(Ok(StreamChunk::ToolCall(call))).await.is_err() {
                    warn!("Streaming receiver dropped while flushing final tool calls");
                    return;
                }
            }
        });

        rx
    }

    /// Collect all chunks into a complete response
    pub async fn collect(self) -> Result<ChatResponse> {
        let mut rx = self.into_channel().await;
        let mut content = String::new();
        let mut reasoning = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost: None,
        };
        let mut finish_reason: Option<String> = None;

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            match chunk {
                StreamChunk::Content(text) => content.push_str(&text),
                StreamChunk::Reasoning(text) => reasoning.push_str(&text),
                StreamChunk::ToolCall(call) => tool_calls.push(call),
                StreamChunk::Usage(u) => {
                    if let Err(e) = u.validate() {
                        tracing::warn!(
                            "Streaming usage chunk has inconsistent token counts: {}. Ignoring chunk.",
                            e
                        );
                    } else {
                        usage = u;
                    }
                }
                StreamChunk::FinishReason(reason) => {
                    finish_reason = Some(reason);
                }
                StreamChunk::Done => break,
            }
        }

        if let Err(e) = usage.validate() {
            tracing::warn!(
                "Final streamed usage has inconsistent token counts: {}. Using zeroed usage.",
                e
            );
            usage = Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
                cost: None,
            };
        }

        Ok(ChatResponse {
            id: "streamed".to_string(),
            object: "chat.completion".to_string(),
            created: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model: "unknown".to_string(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: "assistant".to_string(),
                    content: content.into(),
                    reasoning_content: if reasoning.is_empty() {
                        None
                    } else {
                        Some(reasoning)
                    },
                    tool_calls: if tool_calls.is_empty() {
                        None
                    } else {
                        Some(tool_calls)
                    },
                    tool_call_id: None,
                    name: None,
                },
                reasoning_content: None,
                finish_reason: finish_reason.or_else(|| Some("stop".to_string())),
            }],
            usage,
        })
    }
}

fn append_utf8_chunk(buffer: &mut String, pending: &mut Vec<u8>, bytes: &[u8]) {
    pending.extend_from_slice(bytes);

    loop {
        match std::str::from_utf8(pending) {
            Ok(valid) => {
                buffer.push_str(valid);
                pending.clear();
                return;
            }
            Err(err) => {
                let valid_up_to = err.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending[..valid_up_to])
                        .expect("valid_up_to prefix must be valid utf-8");
                    buffer.push_str(valid);
                    pending.drain(..valid_up_to);
                    continue;
                }

                if let Some(error_len) = err.error_len() {
                    buffer.push('\u{FFFD}');
                    pending.drain(..error_len);
                    continue;
                }

                return;
            }
        }
    }
}

/// A chunk received from an SSE streaming response.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text content
    Content(String),
    /// Reasoning/thinking content
    Reasoning(String),
    /// A tool call
    ToolCall(ToolCall),
    /// Token usage information
    Usage(Usage),
    /// The model's reported finish reason for this turn (e.g. `"stop"`,
    /// `"length"`, `"tool_calls"`).  Emitted at most once per stream when
    /// the backend includes it on the SSE choice; consumers that don't care
    /// can safely ignore it.
    FinishReason(String),
    /// Stream is complete
    Done,
}

/// Accumulates incremental tool call deltas from SSE streaming into complete ToolCall objects.
#[derive(Default)]
pub(crate) struct ToolCallAccumulator {
    /// In-progress tool calls keyed by index
    pending: std::collections::HashMap<usize, (String, String, String, String)>, // (id, type, name, args)
}

impl ToolCallAccumulator {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn process_delta(&mut self, delta: &serde_json::Value) -> Option<types::ToolCall> {
        let index = delta.get("index").and_then(|v| v.as_u64())? as usize;
        let id = delta
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let call_type = delta
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let func = delta.get("function");
        let name = func
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let args_chunk = func
            .and_then(|f| f.get("arguments"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let entry = if let Some(entry) = self.pending.get_mut(&index) {
            entry.3.push_str(&args_chunk);
            if !id.is_empty() {
                entry.0 = id;
            }
            if !call_type.is_empty() {
                entry.1 = call_type;
            }
            if !name.is_empty() {
                entry.2 = name;
            }
            (entry.0.clone(), entry.1.clone(), entry.2.clone(), entry.3.clone())
        } else {
            self.pending
                .insert(index, (id.clone(), call_type.clone(), name.clone(), args_chunk.clone()));
            (id, call_type, name, args_chunk)
        };

        // Emit the tool call as soon as all required fields are present and the
        // accumulated arguments form a complete JSON object. This allows the
        // agent to act on tool_calls mid-stream instead of only at stream end.
        let complete = !entry.0.is_empty()
            && !entry.2.is_empty()
            && serde_json::from_str::<serde_json::Value>(&entry.3).is_ok();
        if complete {
            self.pending.remove(&index);
            Some(types::ToolCall {
                id: entry.0,
                call_type: entry.1,
                function: types::ToolFunction {
                    name: entry.2,
                    arguments: entry.3,
                },
            })
        } else {
            None
        }
    }

    pub(crate) fn flush(&mut self) -> Vec<types::ToolCall> {
        let mut calls: Vec<_> = self.pending.drain().collect();
        calls.sort_by_key(|(idx, _)| *idx);
        calls
            .into_iter()
            .map(|(_, (id, call_type, name, args))| types::ToolCall {
                id,
                call_type,
                function: types::ToolFunction {
                    name,
                    arguments: args,
                },
            })
            .filter(|tc| !tc.id.is_empty() && !tc.function.name.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{append_utf8_chunk, parse_sse_event, StreamChunk, ToolCallAccumulator};

    #[test]
    fn append_utf8_chunk_preserves_split_multibyte_codepoint() {
        let mut buffer = String::new();
        let mut pending = Vec::new();
        let text = "data: hello 🦀\n\n";
        let bytes = text.as_bytes();
        let split = text.find('🦀').unwrap() + 1;

        append_utf8_chunk(&mut buffer, &mut pending, &bytes[..split]);
        assert!(!pending.is_empty());

        append_utf8_chunk(&mut buffer, &mut pending, &bytes[split..]);
        assert_eq!(buffer, text);
        assert!(pending.is_empty());
    }

    #[test]
    fn parse_sse_event_handles_crlf_delimiters() {
        let event = "data: {\"choices\":[{\"delta\":{\"content\":\"hello\"}}]}\r\n\r\n";
        let mut acc = ToolCallAccumulator::new();
        let chunks = parse_sse_event(event, &mut acc);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], StreamChunk::Content(text) if text == "hello"));
    }
}

/// Parse a Server-Sent Events (SSE) event, returning zero or more StreamChunks.
pub(crate) fn parse_sse_event(
    event: &str,
    accumulator: &mut ToolCallAccumulator,
) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();

    for line in event.lines() {
        let Some(data) = line.strip_prefix("data: ") else {
            continue;
        };

        if data == "[DONE]" {
            for call in accumulator.flush() {
                chunks.push(StreamChunk::ToolCall(call));
            }
            chunks.push(StreamChunk::Done);
            return chunks;
        }

        let json = match serde_json::from_str::<serde_json::Value>(data) {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to parse SSE data line as JSON: {} (data: {})", e, data);
                continue;
            }
        };
        let choice = json.get("choices").and_then(|c| c.get(0));
        let delta = choice.and_then(|c| c.get("delta"));

        if let Some(content) = delta
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
        {
            if !content.is_empty() {
                chunks.push(StreamChunk::Content(content.to_string()));
            }
        }

        if let Some(reasoning) = delta
            .and_then(|d| d.get("reasoning_content").or_else(|| d.get("reasoning")))
            .and_then(|c| c.as_str())
        {
            if !reasoning.is_empty() {
                chunks.push(StreamChunk::Reasoning(reasoning.to_string()));
            }
        }

        if let Some(tool_calls) = delta
            .and_then(|d| d.get("tool_calls"))
            .and_then(|tc| tc.as_array())
        {
            for tc_delta in tool_calls {
                if let Some(completed) = accumulator.process_delta(tc_delta) {
                    chunks.push(StreamChunk::ToolCall(completed));
                }
            }
        }

        if let Some(finish) = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|f| f.as_str())
        {
            for call in accumulator.flush() {
                chunks.push(StreamChunk::ToolCall(call));
            }
            if !finish.is_empty() {
                chunks.push(StreamChunk::FinishReason(finish.to_string()));
            }
        }

        if let Some(usage) = json.get("usage") {
            if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                chunks.push(StreamChunk::Usage(u));
            }
        }
    }
    chunks
}
