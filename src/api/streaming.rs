//! Streaming response infrastructure: SSE parsing, chunk types, and tool call accumulation.

use anyhow::Result;
use futures::StreamExt;
use once_cell::sync::Lazy;
use tokio::sync::{mpsc, Semaphore};
use tracing::warn;
use std::time::Duration;

use crate::errors::ApiError;
use super::types::{self, ChatResponse, Choice, Message, ToolCall, Usage};

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

        // Spawn the stream processor with a permit to limit concurrent tasks
        tokio::spawn(async move {
            // Acquire permit at start of task (will wait if limit reached)
            let _permit = match STREAM_SEMAPHORE.acquire().await {
                Ok(p) => Some(p),
                Err(e) => {
                    let _ = tx
                        .send(Err(ApiError::Network(format!("Stream semaphore error: {}", e)).into()))
                        .await;
                    return;
                }
            };
            let mut stream = self.response.bytes_stream();
            let mut buffer = String::new();
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
                        if tx
                            .send(Err(ApiError::Timeout.into()))
                            .await
                            .is_err()
                        {
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
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

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
        };

        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result?;

            match chunk {
                StreamChunk::Content(text) => content.push_str(&text),
                StreamChunk::Reasoning(text) => reasoning.push_str(&text),
                StreamChunk::ToolCall(call) => tool_calls.push(call),
                StreamChunk::Usage(u) => usage = u,
                StreamChunk::Done => break,
            }
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
                finish_reason: Some("stop".to_string()),
            }],
            usage,
        })
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

        if let Some(entry) = self.pending.get_mut(&index) {
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
        } else {
            self.pending
                .insert(index, (id, call_type, name, args_chunk));
        }
        None
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
            .collect()
    }
}

/// Parse a Server-Sent Events (SSE) event, returning zero or more StreamChunks.
pub(crate) fn parse_sse_event(event: &str, accumulator: &mut ToolCallAccumulator) -> Vec<StreamChunk> {
    let mut chunks = Vec::new();

    for line in event.lines() {
        if let Some(data) = line.strip_prefix("data: ") {
            if data == "[DONE]" {
                for call in accumulator.flush() {
                    chunks.push(StreamChunk::ToolCall(call));
                }
                chunks.push(StreamChunk::Done);
                return chunks;
            }

            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
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

                if choice
                    .and_then(|c| c.get("finish_reason"))
                    .and_then(|f| f.as_str())
                    .is_some()
                {
                    for call in accumulator.flush() {
                        chunks.push(StreamChunk::ToolCall(call));
                    }
                }

                if let Some(usage) = json.get("usage") {
                    if let Ok(u) = serde_json::from_value::<Usage>(usage.clone()) {
                        chunks.push(StreamChunk::Usage(u));
                    }
                }
            }
        }
    }
    chunks
}
