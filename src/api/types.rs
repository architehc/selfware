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
    #[serde(default)]
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
///     cost: None,
/// };
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Number of tokens in the prompt (including system message and context).
    #[serde(default)]
    pub prompt_tokens: usize,
    /// Number of tokens in the generated completion.
    #[serde(default)]
    pub completion_tokens: usize,
    /// Total tokens used (prompt + completion).
    #[serde(default)]
    pub total_tokens: usize,
    /// Provider-reported cost in USD for this call, when available (e.g.
    /// OpenRouter's `usage.cost`). `None` for providers that don't report it.
    #[serde(default)]
    pub cost: Option<f64>,
}

/// Side-channel metadata produced alongside a chat call.
///
/// Carries the exact request body that was sent (so debug capture can record
/// it without re-serializing) and the wall time spent in the HTTP / stream
/// layer.  For streaming requests, `finish_reason` and the usage fields are
/// populated from the SSE stream rather than the (synthetic) `ChatResponse`.
///
/// This is *only* exposed via the `*_with_meta` API on `ApiClient` — the
/// public `chat`/`chat_stream` signatures are unchanged.
#[derive(Debug, Clone, Default)]
pub struct ChatMetadata {
    /// The full request body that was POSTed. Includes `messages`, `tools`,
    /// `temperature`, `max_tokens`, etc. Does **not** contain credentials —
    /// those live on HTTP headers, never the body.
    pub request_body: serde_json::Value,
    /// Wall-clock duration of the HTTP request (or stream consumption).
    pub elapsed_ms: u64,
    /// Optional `finish_reason` from the response (`"stop"`, `"length"`,
    /// `"tool_calls"`, …). `None` when the backend doesn't report it.
    pub finish_reason: Option<String>,
    /// Prompt tokens reported by the backend, if any.
    pub prompt_tokens: Option<u32>,
    /// Completion tokens reported by the backend, if any.
    pub completion_tokens: Option<u32>,
    /// Total tokens reported by the backend (prompt + completion + reasoning,
    /// per backend convention). `None` when the backend doesn't report it.
    pub total_tokens: Option<u32>,
    /// Provider-reported cost in USD for this call, when available.
    pub cost: Option<f64>,
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
#[path = "../../tests/unit/api/types/types_test.rs"]
mod tests;
