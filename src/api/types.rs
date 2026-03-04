use serde::{Deserialize, Serialize};

/// Message content that can be either plain text or a sequence of multimodal
/// blocks (text + images).  Serializes as a plain JSON string for `Text` and
/// as a JSON array for `Blocks`, matching the OpenAI vision API format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Plain text content (backward-compatible default).
    Text(String),
    /// Array of content blocks (text + image_url) for multimodal messages.
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Create a plain-text content value.
    pub fn from_text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// Extract the text portion of the content.  For `Text`, returns the
    /// string directly.  For `Blocks`, returns the text of the first `Text`
    /// block, or `""` if none exists.
    pub fn text(&self) -> &str {
        match self {
            Self::Text(s) => s,
            Self::Blocks(blocks) => blocks
                .iter()
                .find_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .unwrap_or(""),
        }
    }

    /// Returns `true` if any block contains an image.
    pub fn has_images(&self) -> bool {
        match self {
            Self::Text(_) => false,
            Self::Blocks(blocks) => blocks
                .iter()
                .any(|b| matches!(b, ContentBlock::ImageUrl { .. })),
        }
    }

    /// Length of the text portion (for token estimation, truncation, etc.).
    pub fn len(&self) -> usize {
        self.text().len()
    }

    /// Returns `true` if the text portion is empty.
    pub fn is_empty(&self) -> bool {
        self.text().is_empty()
    }

    /// Check if the text portion contains a substring.
    pub fn contains(&self, pat: &str) -> bool {
        self.text().contains(pat)
    }

    /// Iterator over the characters of the text portion.
    pub fn chars(&self) -> std::str::Chars<'_> {
        self.text().chars()
    }

    /// Convert to `Blocks` (if not already) and append an image.
    pub fn with_image(self, base64_png: &str) -> Self {
        let mut blocks = match self {
            Self::Text(s) => vec![ContentBlock::Text { text: s }],
            Self::Blocks(b) => b,
        };
        blocks.push(ContentBlock::ImageUrl {
            image_url: ImageUrl {
                url: format!("data:image/png;base64,{}", base64_png),
                detail: None,
            },
        });
        Self::Blocks(blocks)
    }
}

impl Default for MessageContent {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl PartialEq for MessageContent {
    fn eq(&self, other: &Self) -> bool {
        self.text() == other.text()
    }
}

impl Eq for MessageContent {}

impl std::fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text())
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

impl PartialEq<str> for MessageContent {
    fn eq(&self, other: &str) -> bool {
        self.text() == other
    }
}

impl PartialEq<&str> for MessageContent {
    fn eq(&self, other: &&str) -> bool {
        self.text() == *other
    }
}

impl PartialEq<String> for MessageContent {
    fn eq(&self, other: &String) -> bool {
        self.text() == other
    }
}

/// A single content block within a multimodal message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    /// Plain text block.
    #[serde(rename = "text")]
    Text { text: String },
    /// Image reference (base64 data URI or URL).
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

/// Image URL payload for the `image_url` content block type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// `"data:image/png;base64,..."` or a remote URL.
    pub url: String,
    /// Resolution hint: `"low"`, `"high"`, or `"auto"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: MessageContent::Text(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: MessageContent::Text(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Text(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn assistant_with_reasoning(
        content: impl Into<String>,
        reasoning: impl Into<String>,
    ) -> Self {
        Self {
            role: "assistant".to_string(),
            content: MessageContent::Text(content.into()),
            reasoning_content: Some(reasoning.into()),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }

    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: MessageContent::Text(content.into()),
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: Some(tool_call_id.into()),
            name: None,
        }
    }

    /// Create a user message with multimodal content (text + images).
    pub fn user_multimodal(content: MessageContent) -> Self {
        Self {
            role: "user".to_string(),
            content,
            reasoning_content: None,
            tool_calls: None,
            tool_call_id: None,
            name: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub def_type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

// OpenAI API compatible types (used in tests and for API completeness)
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChoiceDelta>,
}

// OpenAI API compatible types (used in tests and for API completeness)
#[derive(Debug, Serialize, Deserialize)]
pub struct ChoiceDelta {
    pub index: usize,
    pub delta: MessageDelta,
    pub finish_reason: Option<String>,
}

// OpenAI API compatible types (used in tests and for API completeness)
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

// OpenAI API compatible types (used in tests and for API completeness)
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: Option<FunctionDelta>,
}

// OpenAI API compatible types (used in tests and for API completeness)
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDelta {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_system() {
        let msg = Message::system("You are a helpful assistant");
        assert_eq!(msg.role, "system");
        assert_eq!(msg.content, "You are a helpful assistant");
        assert!(msg.reasoning_content.is_none());
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello!");
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello!");
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there!");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn test_message_assistant_with_reasoning() {
        let msg = Message::assistant_with_reasoning("Answer", "I thought about it");
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Answer");
        assert_eq!(
            msg.reasoning_content,
            Some("I thought about it".to_string())
        );
    }

    #[test]
    fn test_message_tool() {
        let msg = Message::tool(r#"{"result": "success"}"#, "call_123");
        assert_eq!(msg.role, "tool");
        assert_eq!(msg.tool_call_id, Some("call_123".to_string()));
    }

    #[test]
    fn test_message_serialization() {
        let msg = Message::user("Test message");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"Test message\""));
        // Optional fields should not appear when None
        assert!(!json.contains("reasoning_content"));
    }

    #[test]
    fn test_message_deserialization() {
        let json = r#"{"role": "assistant", "content": "Hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            id: "call_1".to_string(),
            call_type: "function".to_string(),
            function: ToolFunction {
                name: "file_read".to_string(),
                arguments: r#"{"path": "test.txt"}"#.to_string(),
            },
        };
        let json = serde_json::to_string(&call).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"name\":\"file_read\""));
    }

    #[test]
    fn test_chat_response_deserialization() {
        let json = r#"{
            "id": "resp_123",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;
        let response: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "resp_123");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_usage_struct() {
        let usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        assert_eq!(
            usage.prompt_tokens + usage.completion_tokens,
            usage.total_tokens
        );
    }

    #[test]
    fn test_tool_definition_serialization() {
        let def = ToolDefinition {
            def_type: "function".to_string(),
            function: FunctionDefinition {
                name: "test_tool".to_string(),
                description: "A test tool".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            },
        };
        let json = serde_json::to_string(&def).unwrap();
        assert!(json.contains("\"type\":\"function\""));
        assert!(json.contains("\"name\":\"test_tool\""));
    }

    #[test]
    fn test_message_delta_default() {
        let delta = MessageDelta::default();
        assert!(delta.role.is_none());
        assert!(delta.content.is_none());
        assert!(delta.reasoning_content.is_none());
        assert!(delta.tool_calls.is_none());
    }

    #[test]
    fn test_choice_struct() {
        let choice = Choice {
            index: 0,
            message: Message::assistant("Hello"),
            reasoning_content: Some("I thought about it".to_string()),
            finish_reason: Some("stop".to_string()),
        };
        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.content, "Hello");
        assert_eq!(
            choice.reasoning_content,
            Some("I thought about it".to_string())
        );
        assert_eq!(choice.finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_tool_function_struct() {
        let func = ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path": "/test"}"#.to_string(),
        };
        assert_eq!(func.name, "file_read");
        assert!(func.arguments.contains("path"));
    }

    #[test]
    fn test_function_definition_struct() {
        let def = FunctionDefinition {
            name: "my_tool".to_string(),
            description: "Does something".to_string(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        };
        assert_eq!(def.name, "my_tool");
        assert_eq!(def.description, "Does something");
    }

    #[test]
    fn test_chat_response_chunk_deserialization() {
        let json = r#"{
            "id": "chunk_123",
            "object": "chat.completion.chunk",
            "created": 1234567890,
            "model": "test-model",
            "choices": [{
                "index": 0,
                "delta": {},
                "finish_reason": null
            }]
        }"#;
        let chunk: ChatResponseChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "chunk_123");
        assert_eq!(chunk.choices.len(), 1);
    }

    #[test]
    fn test_tool_call_delta_deserialization() {
        let json = r#"{
            "index": 0,
            "id": "call_123",
            "type": "function",
            "function": {"name": "test", "arguments": "{}"}
        }"#;
        let delta: ToolCallDelta = serde_json::from_str(json).unwrap();
        assert_eq!(delta.index, 0);
        assert_eq!(delta.id, Some("call_123".to_string()));
    }

    #[test]
    fn test_function_delta_struct() {
        let delta = FunctionDelta {
            name: Some("test_func".to_string()),
            arguments: Some("{\"a\": 1}".to_string()),
        };
        assert_eq!(delta.name, Some("test_func".to_string()));
        assert!(delta.arguments.is_some());
    }

    #[test]
    fn test_message_with_tool_calls() {
        let json = r#"{
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "file_read",
                    "arguments": "{\"path\": \"test.txt\"}"
                }
            }]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.tool_calls.is_some());
        let calls = msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.name, "file_read");
    }

    #[test]
    fn test_message_clone() {
        let msg1 = Message::user("Test");
        let msg2 = msg1.clone();
        assert_eq!(msg1.content, msg2.content);
        assert_eq!(msg1.role, msg2.role);
    }

    #[test]
    fn test_message_debug() {
        let msg = Message::user("Debug test");
        let debug_str = format!("{:?}", msg);
        assert!(debug_str.contains("user"));
        assert!(debug_str.contains("Debug test"));
    }

    #[test]
    fn test_message_content_text_serde_roundtrip() {
        // Text content serializes as a plain JSON string
        let msg = Message::user("Hello world");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"content\":\"Hello world\""));
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content.text(), "Hello world");
        assert!(!parsed.content.has_images());
    }

    #[test]
    fn test_message_content_blocks_serde_roundtrip() {
        // Blocks content serializes as a JSON array
        let content = MessageContent::from_text("Describe this image").with_image("iVBORw0KGgo=");
        let msg = Message::user_multimodal(content);
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"type\":\"image_url\""));
        let parsed: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content.text(), "Describe this image");
        assert!(parsed.content.has_images());
    }

    #[test]
    fn test_message_content_backward_compat_deserialization() {
        // Plain string JSON deserializes as Text variant
        let json = r#"{"role": "user", "content": "Hello"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.text(), "Hello");
        assert!(!msg.content.has_images());
    }

    #[test]
    fn test_message_content_blocks_deserialization() {
        // Array JSON deserializes as Blocks variant
        let json = r#"{"role": "user", "content": [
            {"type": "text", "text": "What is this?"},
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,abc="}}
        ]}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.text(), "What is this?");
        assert!(msg.content.has_images());
    }

    #[test]
    fn test_message_content_helpers() {
        let mc = MessageContent::from_text("hello");
        assert_eq!(mc.len(), 5);
        assert!(!mc.is_empty());
        assert!(mc.contains("ell"));
        assert!(!mc.contains("xyz"));
        assert_eq!(mc.chars().count(), 5);
        assert_eq!(format!("{}", mc), "hello");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoice {
    pub text: String,
    pub index: usize,
    pub finish_reason: Option<String>,
}
