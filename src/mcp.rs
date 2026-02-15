//! Model Context Protocol (MCP) Implementation
//!
//! Universal LLM tool interface for standardized communication between
//! agents and models. Supports tool discovery, invocation, and context
//! management across different model backends.
//!
//! Features:
//! - Tool schema registration and discovery
//! - Standardized request/response format
//! - Context window management
//! - Multi-model routing support
//! - Streaming capabilities
//! - Error handling and retry logic

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// MCP Protocol version
pub const MCP_VERSION: &str = "0.1.0";

/// Maximum context window size in tokens (default)
pub const DEFAULT_MAX_CONTEXT: usize = 128_000;

/// Tool parameter type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    #[default]
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
}

/// Tool parameter schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    /// Parameter name
    pub name: String,
    /// Parameter type
    #[serde(rename = "type")]
    pub param_type: ParamType,
    /// Description
    pub description: String,
    /// Whether required
    #[serde(default)]
    pub required: bool,
    /// Default value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<JsonValue>,
    /// Enum values (for constrained strings)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
}

impl ToolParameter {
    /// Create a required string parameter
    pub fn required_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: ParamType::String,
            description: description.into(),
            required: true,
            default: None,
            enum_values: None,
        }
    }

    /// Create an optional string parameter
    pub fn optional_string(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            param_type: ParamType::String,
            description: description.into(),
            required: false,
            default: None,
            enum_values: None,
        }
    }

    /// Create a boolean parameter
    pub fn boolean(name: impl Into<String>, description: impl Into<String>, default: bool) -> Self {
        Self {
            name: name.into(),
            param_type: ParamType::Boolean,
            description: description.into(),
            required: false,
            default: Some(JsonValue::Bool(default)),
            enum_values: None,
        }
    }

    /// Create an integer parameter
    pub fn integer(
        name: impl Into<String>,
        description: impl Into<String>,
        required: bool,
    ) -> Self {
        Self {
            name: name.into(),
            param_type: ParamType::Integer,
            description: description.into(),
            required,
            default: None,
            enum_values: None,
        }
    }

    /// Add default value
    pub fn with_default(mut self, value: JsonValue) -> Self {
        self.default = Some(value);
        self
    }

    /// Add enum constraint
    pub fn with_enum(mut self, values: Vec<String>) -> Self {
        self.enum_values = Some(values);
        self
    }
}

/// Tool schema for MCP
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    /// Tool name (unique identifier)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Parameters
    pub parameters: Vec<ToolParameter>,
    /// Category for grouping
    #[serde(default)]
    pub category: String,
    /// Whether tool is dangerous (needs confirmation)
    #[serde(default)]
    pub dangerous: bool,
    /// Example usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<String>,
    /// Tags for discovery
    #[serde(default)]
    pub tags: Vec<String>,
}

impl ToolSchema {
    /// Create new tool schema
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: Vec::new(),
            category: String::new(),
            dangerous: false,
            example: None,
            tags: Vec::new(),
        }
    }

    /// Add parameter
    pub fn with_param(mut self, param: ToolParameter) -> Self {
        self.parameters.push(param);
        self
    }

    /// Set category
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = category.into();
        self
    }

    /// Mark as dangerous
    pub fn dangerous(mut self) -> Self {
        self.dangerous = true;
        self
    }

    /// Add example
    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.example = Some(example.into());
        self
    }

    /// Add tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Get required parameters
    pub fn required_params(&self) -> Vec<&ToolParameter> {
        self.parameters.iter().filter(|p| p.required).collect()
    }

    /// Validate arguments against schema
    pub fn validate(&self, args: &HashMap<String, JsonValue>) -> Result<()> {
        // Check required parameters
        for param in self.required_params() {
            if !args.contains_key(&param.name) {
                return Err(anyhow!("Missing required parameter: {}", param.name));
            }
        }

        // Validate types and enum constraints
        for (name, value) in args {
            if let Some(param) = self.parameters.iter().find(|p| &p.name == name) {
                // Type validation
                let type_ok = match param.param_type {
                    ParamType::String => value.is_string(),
                    ParamType::Integer => value.is_i64() || value.is_u64(),
                    ParamType::Number => value.is_number(),
                    ParamType::Boolean => value.is_boolean(),
                    ParamType::Array => value.is_array(),
                    ParamType::Object => value.is_object(),
                };

                if !type_ok {
                    return Err(anyhow!(
                        "Parameter '{}' has wrong type: expected {:?}",
                        name,
                        param.param_type
                    ));
                }

                // Enum validation
                if let Some(ref enum_values) = param.enum_values {
                    if let Some(s) = value.as_str() {
                        if !enum_values.contains(&s.to_string()) {
                            return Err(anyhow!(
                                "Parameter '{}' must be one of: {:?}",
                                name,
                                enum_values
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Tool invocation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequest {
    /// Request ID
    pub id: String,
    /// Tool name
    pub tool: String,
    /// Arguments
    pub arguments: HashMap<String, JsonValue>,
    /// Context (optional conversation/session info)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<RequestContext>,
}

impl ToolRequest {
    /// Create new tool request
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            tool: tool.into(),
            arguments: HashMap::new(),
            context: None,
        }
    }

    /// Add argument
    pub fn with_arg(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.arguments.insert(name.into(), value);
        self
    }

    /// Add string argument
    pub fn with_string(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments
            .insert(name.into(), JsonValue::String(value.into()));
        self
    }

    /// Add context
    pub fn with_context(mut self, context: RequestContext) -> Self {
        self.context = Some(context);
        self
    }
}

/// Request context
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RequestContext {
    /// Session ID
    pub session_id: Option<String>,
    /// Conversation ID
    pub conversation_id: Option<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// User ID
    pub user_id: Option<String>,
    /// Custom metadata
    #[serde(default)]
    pub metadata: HashMap<String, JsonValue>,
}

impl RequestContext {
    /// Create new context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set working directory
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}

/// Tool response status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    #[default]
    Success,
    Error,
    Pending,
    Cancelled,
}

/// Tool invocation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    /// Request ID (matches request)
    pub id: String,
    /// Status
    pub status: ResponseStatus,
    /// Result data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<JsonValue>,
    /// Error message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Execution time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Token usage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenUsage>,
}

impl ToolResponse {
    /// Create success response
    pub fn success(id: impl Into<String>, result: JsonValue) -> Self {
        Self {
            id: id.into(),
            status: ResponseStatus::Success,
            result: Some(result),
            error: None,
            duration_ms: None,
            tokens: None,
        }
    }

    /// Create error response
    pub fn error(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: ResponseStatus::Error,
            result: None,
            error: Some(message.into()),
            duration_ms: None,
            tokens: None,
        }
    }

    /// Add duration
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    /// Add token usage
    pub fn with_tokens(mut self, tokens: TokenUsage) -> Self {
        self.tokens = Some(tokens);
        self
    }
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens
    pub input: usize,
    /// Output tokens
    pub output: usize,
    /// Total tokens
    pub total: usize,
    /// Cached tokens (if any)
    #[serde(default)]
    pub cached: usize,
}

impl TokenUsage {
    /// Create new token usage
    pub fn new(input: usize, output: usize) -> Self {
        Self {
            input,
            output,
            total: input + output,
            cached: 0,
        }
    }
}

/// Message role in conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    #[default]
    User,
    Assistant,
    Tool,
}

/// Conversation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role
    pub role: MessageRole,
    /// Content
    pub content: String,
    /// Tool call ID (for tool responses)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made by assistant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
}

impl Message {
    /// Create system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            timestamp: None,
        }
    }

    /// Create user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            timestamp: None,
        }
    }

    /// Create assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
            timestamp: None,
        }
    }

    /// Create tool response message
    pub fn tool(content: impl Into<String>, tool_call_id: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
            timestamp: None,
        }
    }

    /// Add tool calls
    pub fn with_tool_calls(mut self, calls: Vec<ToolCall>) -> Self {
        self.tool_calls = Some(calls);
        self
    }
}

/// Tool call made by assistant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID
    pub id: String,
    /// Tool name
    pub name: String,
    /// Arguments as JSON
    pub arguments: HashMap<String, JsonValue>,
}

impl ToolCall {
    /// Create new tool call
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            arguments: HashMap::new(),
        }
    }

    /// Add argument
    pub fn with_arg(mut self, name: impl Into<String>, value: JsonValue) -> Self {
        self.arguments.insert(name.into(), value);
        self
    }
}

/// Context window for managing conversation history
#[derive(Debug)]
pub struct ContextWindow {
    /// Messages in context
    messages: Vec<Message>,
    /// Maximum tokens
    max_tokens: usize,
    /// Current estimated tokens
    current_tokens: usize,
    /// System message (always kept)
    system_message: Option<Message>,
}

impl ContextWindow {
    /// Create new context window
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            current_tokens: 0,
            system_message: None,
        }
    }

    /// Set system message
    pub fn set_system(&mut self, message: Message) {
        if message.role == MessageRole::System {
            self.system_message = Some(message);
        }
    }

    /// Add message to context
    pub fn add_message(&mut self, message: Message) {
        let tokens = Self::estimate_tokens(&message.content);
        self.current_tokens += tokens;
        self.messages.push(message);

        // Trim if over limit
        self.trim_to_fit();
    }

    /// Get all messages (including system)
    pub fn messages(&self) -> Vec<&Message> {
        let mut result = Vec::new();
        if let Some(ref sys) = self.system_message {
            result.push(sys);
        }
        result.extend(self.messages.iter());
        result
    }

    /// Get current token count
    pub fn token_count(&self) -> usize {
        self.current_tokens
    }

    /// Get remaining capacity
    pub fn remaining(&self) -> usize {
        self.max_tokens.saturating_sub(self.current_tokens)
    }

    /// Clear all messages (keep system)
    pub fn clear(&mut self) {
        self.messages.clear();
        self.current_tokens = 0;
    }

    /// Trim oldest messages to fit within limit
    fn trim_to_fit(&mut self) {
        while self.current_tokens > self.max_tokens && !self.messages.is_empty() {
            let removed = self.messages.remove(0);
            self.current_tokens = self
                .current_tokens
                .saturating_sub(Self::estimate_tokens(&removed.content));
        }
    }

    /// Estimate token count for text using shared tokenizer utilities.
    fn estimate_tokens(text: &str) -> usize {
        crate::token_count::estimate_content_tokens(text).max(1)
    }
}

/// Model capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Maximum context window
    pub max_context: usize,
    /// Supports tool use
    pub supports_tools: bool,
    /// Supports vision/images
    pub supports_vision: bool,
    /// Supports streaming
    pub supports_streaming: bool,
    /// Maximum output tokens
    pub max_output: usize,
}

impl ModelCapabilities {
    /// Create for Claude
    pub fn claude() -> Self {
        Self {
            max_context: 200_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            max_output: 8192,
        }
    }

    /// Create for GPT-4
    pub fn gpt4() -> Self {
        Self {
            max_context: 128_000,
            supports_tools: true,
            supports_vision: true,
            supports_streaming: true,
            max_output: 4096,
        }
    }

    /// Create for local model
    pub fn local(max_context: usize) -> Self {
        Self {
            max_context,
            supports_tools: true,
            supports_vision: false,
            supports_streaming: true,
            max_output: 2048,
        }
    }
}

/// Model provider trait
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Get model name
    fn name(&self) -> &str;

    /// Get capabilities
    fn capabilities(&self) -> &ModelCapabilities;

    /// Complete a conversation
    async fn complete(&self, messages: &[Message], tools: &[ToolSchema]) -> Result<Message>;

    /// Stream a completion
    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        callback: Box<dyn Fn(String) + Send>,
    ) -> Result<Message>;
}

/// Mock model provider for testing
pub struct MockModelProvider {
    name: String,
    capabilities: ModelCapabilities,
    responses: Vec<String>,
    response_idx: std::sync::atomic::AtomicUsize,
}

impl MockModelProvider {
    /// Create new mock provider
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            capabilities: ModelCapabilities::local(8192),
            responses: vec!["Mock response".to_string()],
            response_idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Add responses
    pub fn with_responses(mut self, responses: Vec<String>) -> Self {
        self.responses = responses;
        self
    }
}

#[async_trait]
impl ModelProvider for MockModelProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn capabilities(&self) -> &ModelCapabilities {
        &self.capabilities
    }

    async fn complete(&self, _messages: &[Message], _tools: &[ToolSchema]) -> Result<Message> {
        let idx = self
            .response_idx
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let response = self
            .responses
            .get(idx % self.responses.len())
            .cloned()
            .unwrap_or_default();
        Ok(Message::assistant(response))
    }

    async fn stream(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        callback: Box<dyn Fn(String) + Send>,
    ) -> Result<Message> {
        let response = self.complete(messages, tools).await?;
        callback(response.content.clone());
        Ok(response)
    }
}

/// Tool executor trait
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute a tool
    async fn execute(&self, request: &ToolRequest) -> ToolResponse;

    /// Get available tools
    fn tools(&self) -> &[ToolSchema];

    /// Get tool by name
    fn get_tool(&self, name: &str) -> Option<&ToolSchema>;
}

/// MCP Server - coordinates tools and models
pub struct McpServer {
    /// Registered tools
    tools: HashMap<String, ToolSchema>,
    /// Tool executors
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
    /// Model providers
    providers: HashMap<String, Arc<dyn ModelProvider>>,
    /// Default provider
    default_provider: Option<String>,
    /// Execution statistics
    stats: McpStats,
}

/// MCP statistics
#[derive(Debug, Clone, Default)]
pub struct McpStats {
    /// Total requests
    pub total_requests: usize,
    /// Successful requests
    pub successful: usize,
    /// Failed requests
    pub failed: usize,
    /// Total execution time (ms)
    pub total_time_ms: u64,
    /// Tool usage counts
    pub tool_usage: HashMap<String, usize>,
}

impl McpServer {
    /// Create new MCP server
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            executors: HashMap::new(),
            providers: HashMap::new(),
            default_provider: None,
            stats: McpStats::default(),
        }
    }

    /// Register a tool schema
    pub fn register_tool(&mut self, schema: ToolSchema) {
        self.tools.insert(schema.name.clone(), schema);
    }

    /// Register a tool executor
    pub fn register_executor(&mut self, name: impl Into<String>, executor: Arc<dyn ToolExecutor>) {
        let name = name.into();
        for tool in executor.tools() {
            self.tools.insert(tool.name.clone(), tool.clone());
        }
        self.executors.insert(name, executor);
    }

    /// Register a model provider
    pub fn register_provider(&mut self, name: impl Into<String>, provider: Arc<dyn ModelProvider>) {
        let name = name.into();
        if self.default_provider.is_none() {
            self.default_provider = Some(name.clone());
        }
        self.providers.insert(name, provider);
    }

    /// Set default provider
    pub fn set_default_provider(&mut self, name: impl Into<String>) {
        self.default_provider = Some(name.into());
    }

    /// Get all registered tools
    pub fn list_tools(&self) -> Vec<&ToolSchema> {
        self.tools.values().collect()
    }

    /// Get tool by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolSchema> {
        self.tools.get(name)
    }

    /// Get tools by category
    pub fn tools_by_category(&self, category: &str) -> Vec<&ToolSchema> {
        self.tools
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Search tools by tag
    pub fn tools_by_tag(&self, tag: &str) -> Vec<&ToolSchema> {
        self.tools
            .values()
            .filter(|t| t.tags.contains(&tag.to_string()))
            .collect()
    }

    /// Execute a tool request
    pub async fn execute(&mut self, request: ToolRequest) -> ToolResponse {
        let start = Instant::now();

        self.stats.total_requests += 1;
        *self
            .stats
            .tool_usage
            .entry(request.tool.clone())
            .or_insert(0) += 1;

        // Validate tool exists
        let schema = match self.tools.get(&request.tool) {
            Some(s) => s,
            None => {
                self.stats.failed += 1;
                return ToolResponse::error(&request.id, format!("Unknown tool: {}", request.tool));
            }
        };

        // Validate arguments
        if let Err(e) = schema.validate(&request.arguments) {
            self.stats.failed += 1;
            return ToolResponse::error(&request.id, e.to_string());
        }

        // Find executor
        for executor in self.executors.values() {
            if executor.get_tool(&request.tool).is_some() {
                let response = executor.execute(&request).await;
                let duration = start.elapsed().as_millis() as u64;
                self.stats.total_time_ms += duration;

                if response.status == ResponseStatus::Success {
                    self.stats.successful += 1;
                } else {
                    self.stats.failed += 1;
                }

                return response.with_duration(duration);
            }
        }

        self.stats.failed += 1;
        ToolResponse::error(
            &request.id,
            format!("No executor for tool: {}", request.tool),
        )
    }

    /// Complete using model
    pub async fn complete(
        &self,
        messages: Vec<Message>,
        provider_name: Option<&str>,
    ) -> Result<Message> {
        let provider_name = provider_name
            .map(|s| s.to_string())
            .or_else(|| self.default_provider.clone())
            .ok_or_else(|| anyhow!("No model provider available"))?;

        let provider = self
            .providers
            .get(&provider_name)
            .ok_or_else(|| anyhow!("Provider not found: {}", provider_name))?;

        let tools: Vec<ToolSchema> = self.tools.values().cloned().collect();
        provider.complete(&messages, &tools).await
    }

    /// Get statistics
    pub fn stats(&self) -> &McpStats {
        &self.stats
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = McpStats::default();
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP Client for interacting with remote MCP servers
pub struct McpClient {
    /// Server URL
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
    /// Request timeout
    timeout: Duration,
}

impl McpClient {
    /// Create new client
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// List available tools
    pub async fn list_tools(&self) -> Result<Vec<ToolSchema>> {
        let url = format!("{}/tools", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await?
            .json::<Vec<ToolSchema>>()
            .await?;
        Ok(response)
    }

    /// Execute a tool
    pub async fn execute(&self, request: ToolRequest) -> Result<ToolResponse> {
        let url = format!("{}/execute", self.base_url);
        let response = self
            .client
            .post(&url)
            .timeout(self.timeout)
            .json(&request)
            .send()
            .await?
            .json::<ToolResponse>()
            .await?;
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_parameter_creation() {
        let param = ToolParameter::required_string("path", "File path");
        assert_eq!(param.name, "path");
        assert!(param.required);
        assert_eq!(param.param_type, ParamType::String);
    }

    #[test]
    fn test_tool_parameter_optional() {
        let param = ToolParameter::optional_string("encoding", "File encoding");
        assert!(!param.required);
    }

    #[test]
    fn test_tool_parameter_boolean() {
        let param = ToolParameter::boolean("recursive", "Search recursively", true);
        assert_eq!(param.param_type, ParamType::Boolean);
        assert_eq!(param.default, Some(JsonValue::Bool(true)));
    }

    #[test]
    fn test_tool_parameter_with_enum() {
        let param = ToolParameter::required_string("format", "Output format")
            .with_enum(vec!["json".into(), "yaml".into()]);
        assert_eq!(
            param.enum_values,
            Some(vec!["json".to_string(), "yaml".to_string()])
        );
    }

    #[test]
    fn test_tool_schema_creation() {
        let schema = ToolSchema::new("file_read", "Read a file")
            .with_param(ToolParameter::required_string("path", "File path"))
            .with_category("file")
            .with_tag("io");

        assert_eq!(schema.name, "file_read");
        assert_eq!(schema.category, "file");
        assert!(schema.tags.contains(&"io".to_string()));
    }

    #[test]
    fn test_tool_schema_dangerous() {
        let schema = ToolSchema::new("file_delete", "Delete a file").dangerous();
        assert!(schema.dangerous);
    }

    #[test]
    fn test_tool_schema_validate_missing_required() {
        let schema = ToolSchema::new("test", "Test tool")
            .with_param(ToolParameter::required_string("path", "Path"));

        let args = HashMap::new();
        let result = schema.validate(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_schema_validate_wrong_type() {
        let schema = ToolSchema::new("test", "Test tool")
            .with_param(ToolParameter::integer("count", "Count", true));

        let mut args = HashMap::new();
        args.insert(
            "count".to_string(),
            JsonValue::String("not a number".into()),
        );

        let result = schema.validate(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_schema_validate_success() {
        let schema = ToolSchema::new("test", "Test tool")
            .with_param(ToolParameter::required_string("path", "Path"));

        let mut args = HashMap::new();
        args.insert("path".to_string(), JsonValue::String("/tmp".into()));

        let result = schema.validate(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_tool_schema_validate_enum() {
        let schema = ToolSchema::new("test", "Test").with_param(
            ToolParameter::required_string("format", "Format")
                .with_enum(vec!["json".into(), "yaml".into()]),
        );

        let mut valid_args = HashMap::new();
        valid_args.insert("format".to_string(), JsonValue::String("json".into()));
        assert!(schema.validate(&valid_args).is_ok());

        let mut invalid_args = HashMap::new();
        invalid_args.insert("format".to_string(), JsonValue::String("xml".into()));
        assert!(schema.validate(&invalid_args).is_err());
    }

    #[test]
    fn test_tool_request_creation() {
        let request = ToolRequest::new("file_read")
            .with_string("path", "/tmp/test.txt")
            .with_arg("lines", JsonValue::Number(100.into()));

        assert_eq!(request.tool, "file_read");
        assert_eq!(
            request.arguments.get("path"),
            Some(&JsonValue::String("/tmp/test.txt".into()))
        );
    }

    #[test]
    fn test_tool_request_with_context() {
        let context = RequestContext::new()
            .with_session("session-123")
            .with_working_dir("/project");

        let request = ToolRequest::new("test").with_context(context);

        assert!(request.context.is_some());
        assert_eq!(
            request.context.as_ref().unwrap().session_id,
            Some("session-123".to_string())
        );
    }

    #[test]
    fn test_tool_response_success() {
        let response =
            ToolResponse::success("req-1", JsonValue::String("result".into())).with_duration(100);

        assert_eq!(response.status, ResponseStatus::Success);
        assert_eq!(response.duration_ms, Some(100));
    }

    #[test]
    fn test_tool_response_error() {
        let response = ToolResponse::error("req-1", "Something went wrong");

        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_token_usage() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.input, 100);
        assert_eq!(usage.output, 50);
        assert_eq!(usage.total, 150);
    }

    #[test]
    fn test_message_creation() {
        let system = Message::system("You are a helpful assistant");
        assert_eq!(system.role, MessageRole::System);

        let user = Message::user("Hello");
        assert_eq!(user.role, MessageRole::User);

        let assistant = Message::assistant("Hi there!");
        assert_eq!(assistant.role, MessageRole::Assistant);

        let tool = Message::tool("File content here", "call-123");
        assert_eq!(tool.role, MessageRole::Tool);
        assert_eq!(tool.tool_call_id, Some("call-123".to_string()));
    }

    #[test]
    fn test_message_with_tool_calls() {
        let call = ToolCall::new("file_read").with_arg("path", JsonValue::String("/tmp".into()));

        let message = Message::assistant("I'll read that file").with_tool_calls(vec![call]);

        assert!(message.tool_calls.is_some());
        assert_eq!(message.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_context_window() {
        let mut window = ContextWindow::new(100);

        window.set_system(Message::system("System message"));
        window.add_message(Message::user("Hello"));
        window.add_message(Message::assistant("Hi"));

        assert_eq!(window.messages().len(), 3); // system + 2 messages
        assert!(window.token_count() > 0);
    }

    #[test]
    fn test_context_window_trim() {
        let mut window = ContextWindow::new(10); // Very small

        window.add_message(Message::user(
            "This is a very long message that should cause trimming",
        ));
        window.add_message(Message::user("Another message"));

        // Should have trimmed to fit
        assert!(window.token_count() <= 10);
    }

    #[test]
    fn test_context_window_clear() {
        let mut window = ContextWindow::new(1000);
        window.set_system(Message::system("System"));
        window.add_message(Message::user("Hello"));

        window.clear();

        // System should still be there, but messages cleared
        assert_eq!(window.messages().len(), 1);
        assert_eq!(window.token_count(), 0);
    }

    #[test]
    fn test_model_capabilities_claude() {
        let caps = ModelCapabilities::claude();
        assert_eq!(caps.max_context, 200_000);
        assert!(caps.supports_tools);
        assert!(caps.supports_vision);
    }

    #[test]
    fn test_model_capabilities_local() {
        let caps = ModelCapabilities::local(8192);
        assert_eq!(caps.max_context, 8192);
        assert!(!caps.supports_vision);
    }

    #[tokio::test]
    async fn test_mock_model_provider() {
        let provider = MockModelProvider::new("test-model")
            .with_responses(vec!["Response 1".into(), "Response 2".into()]);

        let messages = vec![Message::user("Hello")];
        let response1 = provider.complete(&messages, &[]).await.unwrap();
        let response2 = provider.complete(&messages, &[]).await.unwrap();

        assert_eq!(response1.content, "Response 1");
        assert_eq!(response2.content, "Response 2");
    }

    #[test]
    fn test_mcp_server_register_tool() {
        let mut server = McpServer::new();

        server.register_tool(
            ToolSchema::new("file_read", "Read file")
                .with_category("file")
                .with_tag("io"),
        );

        assert!(server.get_tool("file_read").is_some());
        assert_eq!(server.list_tools().len(), 1);
    }

    #[test]
    fn test_mcp_server_tools_by_category() {
        let mut server = McpServer::new();

        server.register_tool(ToolSchema::new("file_read", "Read").with_category("file"));
        server.register_tool(ToolSchema::new("file_write", "Write").with_category("file"));
        server.register_tool(ToolSchema::new("git_status", "Status").with_category("git"));

        let file_tools = server.tools_by_category("file");
        assert_eq!(file_tools.len(), 2);

        let git_tools = server.tools_by_category("git");
        assert_eq!(git_tools.len(), 1);
    }

    #[test]
    fn test_mcp_server_tools_by_tag() {
        let mut server = McpServer::new();

        server.register_tool(
            ToolSchema::new("file_read", "Read")
                .with_tag("io")
                .with_tag("read"),
        );
        server.register_tool(ToolSchema::new("file_write", "Write").with_tag("io"));

        let io_tools = server.tools_by_tag("io");
        assert_eq!(io_tools.len(), 2);

        let read_tools = server.tools_by_tag("read");
        assert_eq!(read_tools.len(), 1);
    }

    #[tokio::test]
    async fn test_mcp_server_execute_unknown_tool() {
        let mut server = McpServer::new();
        let request = ToolRequest::new("unknown_tool");

        let response = server.execute(request).await;

        assert_eq!(response.status, ResponseStatus::Error);
        assert!(response.error.unwrap().contains("Unknown tool"));
    }

    #[tokio::test]
    async fn test_mcp_server_execute_validation_error() {
        let mut server = McpServer::new();
        server.register_tool(
            ToolSchema::new("test", "Test")
                .with_param(ToolParameter::required_string("path", "Path")),
        );

        let request = ToolRequest::new("test"); // Missing required param

        let response = server.execute(request).await;

        assert_eq!(response.status, ResponseStatus::Error);
        assert!(response.error.unwrap().contains("Missing required"));
    }

    #[test]
    fn test_mcp_stats() {
        let mut server = McpServer::new();
        server.register_tool(ToolSchema::new("test", "Test"));

        let stats = server.stats();
        assert_eq!(stats.total_requests, 0);

        server.reset_stats();
        assert_eq!(server.stats().total_requests, 0);
    }

    #[tokio::test]
    async fn test_mcp_server_register_provider() {
        let mut server = McpServer::new();
        let provider = Arc::new(MockModelProvider::new("test"));

        server.register_provider("test", provider);

        assert!(server.default_provider.is_some());
    }

    #[test]
    fn test_tool_call_creation() {
        let call = ToolCall::new("file_read")
            .with_arg("path", JsonValue::String("/tmp/test".into()))
            .with_arg("encoding", JsonValue::String("utf-8".into()));

        assert_eq!(call.name, "file_read");
        assert_eq!(call.arguments.len(), 2);
    }

    #[test]
    fn test_param_type_default() {
        assert_eq!(ParamType::default(), ParamType::String);
    }

    #[test]
    fn test_response_status_default() {
        assert_eq!(ResponseStatus::default(), ResponseStatus::Success);
    }

    #[test]
    fn test_message_role_default() {
        assert_eq!(MessageRole::default(), MessageRole::User);
    }

    #[test]
    fn test_tool_schema_with_example() {
        let schema = ToolSchema::new("test", "Test").with_example(r#"{"path": "/tmp/test.txt"}"#);

        assert_eq!(
            schema.example,
            Some(r#"{"path": "/tmp/test.txt"}"#.to_string())
        );
    }

    #[test]
    fn test_request_context_default() {
        let ctx = RequestContext::default();
        assert!(ctx.session_id.is_none());
        assert!(ctx.working_dir.is_none());
    }

    #[test]
    fn test_context_window_remaining() {
        let mut window = ContextWindow::new(1000);
        assert_eq!(window.remaining(), 1000);

        window.add_message(Message::user("Hello")); // ~2 tokens
        assert!(window.remaining() < 1000);
    }

    #[test]
    fn test_tool_response_with_tokens() {
        let usage = TokenUsage::new(100, 50);
        let response = ToolResponse::success("1", JsonValue::Null).with_tokens(usage);

        assert!(response.tokens.is_some());
        assert_eq!(response.tokens.unwrap().total, 150);
    }

    #[test]
    fn test_mcp_client_creation() {
        let client = McpClient::new("http://localhost:8080").with_timeout(Duration::from_secs(60));

        assert_eq!(client.base_url, "http://localhost:8080");
        assert_eq!(client.timeout, Duration::from_secs(60));
    }
}
