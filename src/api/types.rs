use serde::{Deserialize, Deserializer, Serialize};

/// Custom deserializer that treats JSON `null` as an empty `MessageContent::Text("")`.
/// vLLM returns `"content": null` when reasoning mode consumes all tokens.
fn deserialize_nullable_content<'de, D>(deserializer: D) -> Result<MessageContent, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<MessageContent>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_else(|| MessageContent::Text(String::new())))
}

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

    /// Concatenate all text blocks separated by `\n`. For `Text`, returns the
    /// string directly. For `Blocks`, joins all `Text` block contents.
    pub fn text_all(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Blocks(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                texts.join("\n")
            }
        }
    }

    /// Count the number of image blocks in the content.
    pub fn image_count(&self) -> usize {
        match self {
            Self::Text(_) => 0,
            Self::Blocks(blocks) => blocks
                .iter()
                .filter(|b| matches!(b, ContentBlock::ImageUrl { .. }))
                .count(),
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

    /// Return a copy with all image blocks removed. If only one text block
    /// remains, collapses back to `Text` variant.
    pub fn strip_images(&self) -> Self {
        match self {
            Self::Text(_) => self.clone(),
            Self::Blocks(blocks) => {
                let text_blocks: Vec<ContentBlock> = blocks
                    .iter()
                    .filter(|b| matches!(b, ContentBlock::Text { .. }))
                    .cloned()
                    .collect();
                if text_blocks.len() == 1 {
                    if let ContentBlock::Text { text } = &text_blocks[0] {
                        return Self::Text(text.clone());
                    }
                }
                Self::Blocks(text_blocks)
            }
        }
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
///
/// Content blocks allow mixing different types of content (text and images)
/// within a single message. This is essential for vision-language models
/// that can process both text and image inputs.
///
/// # Example
///
/// ```
/// use selfware::api::types::{ContentBlock, ImageUrl};
///
/// let text_block = ContentBlock::Text {
///     text: "What is in this image?".to_string(),
/// };
///
/// let image_block = ContentBlock::ImageUrl {
///     image_url: ImageUrl {
///         url: "data:image/png;base64,iVBORw0KGgo...".to_string(),
///         detail: Some("auto".to_string()),
///     },
/// };
/// ```
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
///
/// Supports both base64-encoded data URIs and remote HTTP URLs.
/// The `detail` field allows controlling image processing resolution.
///
/// # Example
///
/// ```
/// use selfware::api::types::ImageUrl;
///
/// // Using a base64-encoded image
/// let image = ImageUrl {
///     url: "data:image/png;base64,iVBORw0KGgo...".to_string(),
///     detail: Some("auto".to_string()),
/// };
///
/// // Using a remote URL
/// let remote_image = ImageUrl {
///     url: "https://example.com/image.png".to_string(),
///     detail: Some("high".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    /// `"data:image/png;base64,..."` or a remote URL.
    pub url: String,
    /// Resolution hint: `"low"`, `"high"`, or `"auto"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A chat message representing a single turn in a conversation.
///
/// Messages form the core of the chat completion API. Each message has a role
/// indicating who sent it (system, user, assistant, or tool) and content
/// containing the actual message text or multimodal content.
///
/// # Roles
///
/// - `"system"`: Sets the behavior and context for the assistant
/// - `"user"`: Represents input from the end user
/// - `"assistant"`: Contains the model's response
/// - `"tool"`: Contains the result of a tool/function call
///
/// # Example
///
/// ```
/// use selfware::api::types::Message;
///
/// // Create different types of messages
/// let system = Message::system("You are a helpful assistant");
/// let user = Message::user("What is the weather?");
/// let assistant = Message::assistant("I can help you with that!");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The role of the message author: `"system"`, `"user"`, `"assistant"`, or `"tool"`.
    pub role: String,
    /// The content of the message. Can be plain text or multimodal blocks.
    /// Uses a custom deserializer to handle `null` content from vLLM.
    #[serde(default, deserialize_with = "deserialize_nullable_content")]
    pub content: MessageContent,
    /// Reasoning content — accepts both `reasoning_content` (OpenAI/SGLang)
    /// and `reasoning` (vLLM) field names.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "reasoning")]
    pub reasoning_content: Option<String>,
    /// Tool calls made by the assistant. Present when the model decides
    /// to invoke one or more tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// The ID of the tool call this message is responding to.
    /// Required for tool role messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional name identifying the sender in group conversations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Message {
    /// Create a system message to set assistant behavior.
    ///
    /// System messages provide context, instructions, or personality
    /// settings that guide the assistant's responses.
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

    /// Create a user message with the given content.
    ///
    /// User messages represent input from the end user.
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

    /// Create an assistant message with the given content.
    ///
    /// Assistant messages represent the model's responses.
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

    /// Create an assistant message with reasoning content.
    ///
    /// Some models provide separate reasoning content showing the
    /// model's thought process before the final answer.
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

    /// Create a tool message with the given result.
    ///
    /// Tool messages contain the output from tool/function execution
    /// and must reference the original tool call ID.
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

    /// Return a clone with all image blocks stripped from the content.
    pub fn strip_images(&self) -> Self {
        Self {
            role: self.role.clone(),
            content: self.content.strip_images(),
            reasoning_content: self.reasoning_content.clone(),
            tool_calls: self.tool_calls.clone(),
            tool_call_id: self.tool_call_id.clone(),
            name: self.name.clone(),
        }
    }

    /// Create a user message with multimodal content (text + images).
    ///
    /// Use this for vision tasks where the user provides both text
    /// and images in a single message.
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

/// A tool call requested by the assistant.
///
/// When the model decides to use a tool, it returns a tool call containing
/// the function name and serialized arguments. The application should
/// execute the function and return the result in a tool message.
///
/// # Example
///
/// ```
/// use selfware::api::types::{ToolCall, ToolFunction};
///
/// let tool_call = ToolCall {
///     id: "call_123".to_string(),
///     call_type: "function".to_string(),
///     function: ToolFunction {
///         name: "get_weather".to_string(),
///         arguments: r#"{"location": "San Francisco"}"#.to_string(),
///     },
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique identifier for this tool call, used to match with tool responses.
    pub id: String,
    /// The type of call, typically `"function"`.
    #[serde(rename = "type")]
    pub call_type: String,
    /// The function being called with its arguments.
    pub function: ToolFunction,
}

/// Function call details within a tool call.
///
/// Contains the name of the function to invoke and the JSON-serialized
/// arguments to pass to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    /// Name of the function to call.
    pub name: String,
    /// JSON-encoded arguments object for the function.
    pub arguments: String,
}

impl ToolCall {
    /// Validate the structural integrity of a tool call.
    ///
    /// Checks that:
    /// - `id` is non-empty
    /// - `type` is `"function"`
    /// - `function.name` is non-empty
    /// - `function.arguments` is valid JSON
    pub fn validate_structure(&self) -> anyhow::Result<()> {
        if self.id.trim().is_empty() {
            anyhow::bail!("Tool call is missing a valid 'id'");
        }
        if self.call_type != "function" {
            anyhow::bail!(
                "Tool call has unsupported type '{}', expected 'function'",
                self.call_type
            );
        }
        if self.function.name.trim().is_empty() {
            anyhow::bail!("Tool call is missing a valid function name");
        }
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&self.function.arguments) {
            anyhow::bail!(
                "Tool call arguments for '{}' are not valid JSON: {}",
                self.function.name,
                e
            );
        }
        Ok(())
    }
}

impl Usage {
    /// Validate that token usage numbers are internally consistent.
    ///
    /// Checks that:
    /// - `total_tokens` equals `prompt_tokens + completion_tokens`
    /// - All values are within reasonable bounds
    pub fn validate(&self) -> anyhow::Result<()> {
        let expected_total = self.prompt_tokens.saturating_add(self.completion_tokens);
        if self.total_tokens != expected_total {
            anyhow::bail!(
                "Token usage mismatch: prompt_tokens ({}) + completion_tokens ({}) = {}, but total_tokens is {}",
                self.prompt_tokens,
                self.completion_tokens,
                expected_total,
                self.total_tokens
            );
        }
        Ok(())
    }

    /// Validate usage against model configuration limits.
    pub fn validate_against_limits(
        &self,
        context_length: usize,
        max_tokens: usize,
    ) -> anyhow::Result<()> {
        self.validate()?;
        if self.prompt_tokens > context_length {
            anyhow::bail!(
                "Prompt tokens ({}) exceed context_length ({})",
                self.prompt_tokens,
                context_length
            );
        }
        if self.completion_tokens > max_tokens {
            anyhow::bail!(
                "Completion tokens ({}) exceed max_tokens ({})",
                self.completion_tokens,
                max_tokens
            );
        }
        if self.total_tokens > context_length {
            anyhow::bail!(
                "Total tokens ({}) exceed context_length ({})",
                self.total_tokens,
                context_length
            );
        }
        Ok(())
    }
}

/// Definition of a tool available to the model.
///
/// Tool definitions describe functions the model can call, including
/// their names, descriptions, and JSON schemas for parameters.
///
/// # Example
///
/// ```
/// use selfware::api::types::{ToolDefinition, FunctionDefinition};
/// use serde_json::json;
///
/// let tool = ToolDefinition {
///     def_type: "function".to_string(),
///     function: FunctionDefinition {
///         name: "get_weather".to_string(),
///         description: "Get the current weather for a location".to_string(),
///         parameters: json!({
///             "type": "object",
///             "properties": {
///                 "location": {
///                     "type": "string",
///                     "description": "City name"
///                 }
///             },
///             "required": ["location"]
///         }),
///     },
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// The type of tool, typically `"function"`.
    #[serde(rename = "type")]
    pub def_type: String,
    /// The function definition including name, description, and parameters.
    pub function: FunctionDefinition,
}

/// Function definition for a tool.
///
/// Describes a callable function including its name, human-readable
/// description, and JSON schema for expected parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Name of the function (must match the actual function name).
    pub name: String,
    /// Description of what the function does, used by the model
    /// to decide when to call it.
    pub description: String,
    /// JSON Schema object describing the function parameters.
    pub parameters: serde_json::Value,
}

/// Response from a chat completion request.
///
/// Contains the model's output, including the generated message(s),
/// token usage statistics, and response metadata.
///
/// # Example
///
/// ```
/// use selfware::api::types::{ChatResponse, Choice, Message, Usage};
///
/// // A typical response structure
/// // {
/// //   "id": "chatcmpl-123",
/// //   "object": "chat.completion",
/// //   "created": 1677652288,
/// //   "model": "gpt-3.5-turbo",
/// //   "choices": [...],
/// //   "usage": {...}
/// // }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Unique identifier for this chat completion.
    pub id: String,
    /// Object type, typically `"chat.completion"`.
    pub object: String,
    /// Unix timestamp (in seconds) when the response was created.
    pub created: u64,
    /// The model used to generate the completion.
    pub model: String,
    /// List of completion choices (usually one, but can be multiple with `n > 1`).
    pub choices: Vec<Choice>,
    /// Token usage statistics for this request.
    pub usage: Usage,
}

/// A single completion choice within a chat response.
///
/// Each choice represents one possible completion. When `n > 1` is requested,
/// multiple choices may be returned. The `index` field indicates the order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    /// Index of this choice in the list (0-based).
    pub index: usize,
    /// The generated message from the assistant.
    pub message: Message,
    /// Reasoning content from the model (if supported and available).
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "reasoning")]
    pub reasoning_content: Option<String>,
    /// Reason why the generation stopped: `"stop"`, `"length"`, `"tool_calls"`, etc.
    pub finish_reason: Option<String>,
}

/// Token usage statistics for a request/response.
///
/// Tracks the number of tokens used in the prompt, the completion,
/// and the total for billing and rate limit purposes.
///
/// # Example
///
/// ```
/// use selfware::api::types::Usage;
///
/// let usage = Usage {
///     prompt_tokens: 50,
///     completion_tokens: 30,
///     total_tokens: 80,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt (including system message and context).
    pub prompt_tokens: usize,
    /// Number of tokens in the generated completion.
    pub completion_tokens: usize,
    /// Total tokens used (prompt + completion).
    pub total_tokens: usize,
}

/// A streaming chunk of a chat completion response.
///
/// When streaming is enabled, the API sends multiple `ChatResponseChunk`
/// objects rather than a single complete response. Each chunk contains
/// a delta (incremental update) to the message.
///
/// # Streaming vs Non-Streaming
///
/// - Non-streaming: Single `ChatResponse` with complete message
/// - Streaming: Multiple `ChatResponseChunk` objects with incremental deltas
///
/// # Example
///
/// A streaming response sends chunks like:
/// ```json
/// {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1234567890,
///  "model":"gpt-4","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    /// Unique identifier for this chat completion (shared across all chunks).
    pub id: String,
    /// Object type, typically `"chat.completion.chunk"`.
    pub object: String,
    /// Unix timestamp when the chunk was created.
    pub created: u64,
    /// The model generating the completion.
    pub model: String,
    /// List of choice deltas (usually one).
    pub choices: Vec<ChoiceDelta>,
}

/// A delta (incremental update) within a streaming chunk.
///
/// Choice deltas contain partial updates to the message being generated.
/// Multiple deltas together form the complete message.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChoiceDelta {
    /// Index of this choice (0-based).
    pub index: usize,
    /// The delta containing incremental message content.
    pub delta: MessageDelta,
    /// Reason why generation stopped, if complete.
    pub finish_reason: Option<String>,
}

/// Incremental message content in a streaming response.
///
/// Each `MessageDelta` contains partial content that should be appended
/// to build the complete message. Fields are `Option` because different
/// chunks may contain different pieces (role, content, tool_calls).
///
/// # Example
///
/// A complete streaming response might send these deltas:
/// 1. `{"role": "assistant"}` - Start of assistant message
/// 2. `{"content": "Hello"}` - First token(s)
/// 3. `{"content": " world"}` - More tokens
/// 4. `{"content": "!"}` - Final token
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MessageDelta {
    /// The role of the message (usually only present in first chunk).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Incremental text content to append.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Incremental reasoning content (if supported).
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "reasoning")]
    pub reasoning_content: Option<String>,
    /// Incremental tool call updates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Incremental update to a tool call in a streaming response.
///
/// Tool calls in streaming mode arrive in pieces across multiple chunks.
/// The `index` identifies which tool call this delta belongs to, and
/// the other fields contain partial data.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of the tool call being updated.
    pub index: usize,
    /// Partial or complete tool call ID.
    pub id: Option<String>,
    /// Type of call (usually `"function"`).
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    /// Partial function call details.
    pub function: Option<FunctionDelta>,
}

/// Incremental update to function call arguments in a streaming response.
///
/// Function arguments in streaming mode are sent as partial JSON strings
/// that need to be concatenated to form the complete arguments object.
#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDelta {
    /// Partial or complete function name.
    pub name: Option<String>,
    /// Partial JSON arguments string (concatenate across chunks).
    pub arguments: Option<String>,
}

/// Request for a legacy completion (non-chat) endpoint.
///
/// Legacy completions generate text based on a prompt without the
/// structured message format of chat completions.
///
/// # Note
///
/// Chat completions are preferred for most use cases. Legacy completions
/// are supported for backward compatibility.
///
/// # Example
///
/// ```
/// use selfware::api::types::CompletionRequest;
///
/// let request = CompletionRequest {
///     model: "text-davinci-003".to_string(),
///     prompt: "Once upon a time".to_string(),
///     max_tokens: Some(100),
///     temperature: Some(0.7),
///     top_p: Some(1.0),
///     stop: Some(vec!["\n\n".to_string()]),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model ID to use for completion.
    pub model: String,
    /// The prompt text to complete.
    pub prompt: String,
    /// Maximum number of tokens to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Sampling temperature (0.0 to 2.0, higher = more random).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter (0.0 to 1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Stop sequences that will end generation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

/// Response from a legacy completion request.
///
/// Contains the generated text completions and usage statistics.
/// Unlike chat completions, these return plain text rather than messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// Unique identifier for this completion.
    pub id: String,
    /// Object type, typically `"text_completion"`.
    pub object: String,
    /// Unix timestamp when the response was created.
    pub created: u64,
    /// The model used for generation.
    pub model: String,
    /// List of completion choices.
    pub choices: Vec<CompletionChoice>,
    /// Token usage statistics (optional in legacy API).
    pub usage: Option<Usage>,
}

/// A single completion choice in a legacy completion response.
///
/// Contains the generated text and metadata about why generation stopped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChoice {
    /// The generated text completion.
    pub text: String,
    /// Index of this choice (0-based).
    pub index: usize,
    /// Reason why generation stopped.
    pub finish_reason: Option<String>,
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
    fn test_text_all_plain_text() {
        let mc = MessageContent::from_text("hello world");
        assert_eq!(mc.text_all(), "hello world");
    }

    #[test]
    fn test_text_all_blocks_with_images() {
        let mc = MessageContent::from_text("first")
            .with_image("img1")
            .with_image("img2");
        // Add a second text block manually
        let mut blocks = match mc {
            MessageContent::Blocks(b) => b,
            _ => panic!("expected Blocks"),
        };
        blocks.push(ContentBlock::Text {
            text: "second".to_string(),
        });
        let mc = MessageContent::Blocks(blocks);
        assert_eq!(mc.text_all(), "first\nsecond");
    }

    #[test]
    fn test_image_count() {
        let mc = MessageContent::from_text("hello");
        assert_eq!(mc.image_count(), 0);

        let mc = mc.with_image("img1").with_image("img2");
        assert_eq!(mc.image_count(), 2);
    }

    #[test]
    fn test_strip_images_plain_text() {
        let mc = MessageContent::from_text("hello");
        let stripped = mc.strip_images();
        assert_eq!(stripped.text(), "hello");
        assert!(!stripped.has_images());
    }

    #[test]
    fn test_strip_images_blocks() {
        let mc = MessageContent::from_text("describe this").with_image("abc123");
        assert!(mc.has_images());
        let stripped = mc.strip_images();
        assert!(!stripped.has_images());
        assert_eq!(stripped.text(), "describe this");
        // Should collapse to Text variant when only one text block remains
        assert!(matches!(stripped, MessageContent::Text(_)));
    }

    #[test]
    fn test_message_strip_images() {
        let content = MessageContent::from_text("look at this").with_image("img_data");
        let msg = Message::user_multimodal(content);
        assert!(msg.content.has_images());
        let stripped = msg.strip_images();
        assert!(!stripped.content.has_images());
        assert_eq!(stripped.role, "user");
        assert_eq!(stripped.content.text(), "look at this");
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
