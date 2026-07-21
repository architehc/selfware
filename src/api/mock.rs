use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct MockLlmClient {
    responses: Mutex<VecDeque<ChatResponse>>,
}

impl MockLlmClient {
    /// Create a new mock client with an empty response queue.
    pub fn new() -> Self {
        Self {
            responses: Mutex::new(VecDeque::new()),
        }
    }

    /// Create a mock client pre-loaded with the given responses.
    ///
    /// Responses are returned in FIFO order by `chat()`.
    pub fn with_responses(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
        }
    }
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn chat(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<ToolDefinition>>,
        _thinking: ThinkingMode,
    ) -> anyhow::Result<ChatResponse> {
        let mut queue = self
            .responses
            .lock()
            .map_err(|_| anyhow::anyhow!("Mock responses mutex poisoned"))?;
        queue
            .pop_front()
            .ok_or_else(|| anyhow::anyhow!("No more mock responses"))
    }

    async fn chat_stream(
        &self,
        _messages: Vec<Message>,
        _tools: Option<Vec<ToolDefinition>>,
        _thinking: ThinkingMode,
    ) -> anyhow::Result<StreamingResponse> {
        Err(anyhow::anyhow!("Streaming not supported in mock client"))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/api/mock/mod_test.rs"]
mod tests;
