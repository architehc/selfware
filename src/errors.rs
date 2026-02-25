use thiserror::Error;
use std::path::PathBuf;

/// The central error type for the Selfware system.
///
/// This hierarchy enables programmatic recovery and unified error handling
/// across agent, API, tools, and safety layers.
#[derive(Error, Debug)]
pub enum SelfwareError {
    #[error("Agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("API error: {0}")]
    Api(#[from] ApiError),

    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    #[error("Safety error: {0}")]
    Safety(#[from] SafetyError),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Tool '{tool_name}' requires confirmation but running in non-interactive mode. Use --yolo to auto-approve tools, or run interactively.")]
    ConfirmationRequired { tool_name: String },

    #[error("Iteration limit reached ({limit})")]
    IterationLimit { limit: usize },

    #[error("Step timeout after {seconds} seconds")]
    StepTimeout { seconds: u64 },

    #[error("Invalid state transition from {from} to {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Task cancelled by user")]
    Cancelled,

    #[error("Missing system prompt")]
    MissingSystemPrompt,

    #[error("Agent loop panicked: {0}")]
    Panic(String),
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("API Request timed out")]
    Timeout,

    #[error("Rate limit exceeded. Retry after {retry_after_secs:?} seconds")]
    RateLimit { retry_after_secs: Option<u64> },

    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("API returned status {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("Failed to parse API response: {0}")]
    Parse(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool '{name}' failed: {message}")]
    Execution { name: String, message: String },

    #[error("Tool '{name}' not found")]
    NotFound { name: String },

    #[error("Invalid arguments for tool '{name}': {message}")]
    InvalidArguments { name: String, message: String },

    #[error("Tool execution timed out")]
    Timeout,
}

#[derive(Error, Debug)]
pub enum SafetyError {
    #[error("Path blocked by safety policy: {path}")]
    BlockedPath { path: String },

    #[error("Dangerous command blocked: {command} ({reason})")]
    BlockedCommand { command: String, reason: String },

    #[error("Potential secret detected in content: {finding}")]
    SecretDetected { finding: String },

    #[error("Action requires manual confirmation: {action}")]
    ConfirmationRequired { action: String },

    #[error("Path traversal attempt detected: {path}")]
    PathTraversal { path: String },
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Failed to save checkpoint: {0}")]
    CheckpointSave(String),

    #[error("Failed to load checkpoint: {0}")]
    CheckpointLoad(String),

    #[error("Storage error at {path}: {message}")]
    Storage { path: PathBuf, message: String },

    #[error("Session history corrupted: {0}")]
    HistoryCorrupted(String),
}

pub type Result<T> = std::result::Result<T, SelfwareError>;

/// Check if an anyhow error is a confirmation-required error (fatal in non-interactive mode)
pub fn is_confirmation_error(e: &anyhow::Error) -> bool {
    e.downcast_ref::<SelfwareError>()
        .and_then(|se| match se {
            SelfwareError::Agent(ae) => Some(ae),
            _ => None,
        })
        .map(|ae| matches!(ae, AgentError::ConfirmationRequired { .. }))
        .unwrap_or(false)
}
