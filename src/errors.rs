use std::path::PathBuf;
use thiserror::Error;

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

    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),

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
    // Check if wrapped as SelfwareError::Agent(AgentError::ConfirmationRequired)
    if let Some(SelfwareError::Agent(AgentError::ConfirmationRequired { .. })) =
        e.downcast_ref::<SelfwareError>()
    {
        return true;
    }

    // Also check if AgentError was returned directly into anyhow (e.g. from execution.rs)
    if let Some(AgentError::ConfirmationRequired { .. }) = e.downcast_ref::<AgentError>() {
        return true;
    }

    false
}

#[derive(Error, Debug)]
pub enum ResourceError {
    #[error("Memory exhausted: {0}")]
    MemoryExhausted(String),

    #[error("GPU error: {0}")]
    Gpu(String),

    #[error("Disk exhausted: {0}")]
    DiskExhausted(String),

    #[error("Resource quota exceeded for {resource}: used {used}, limit {limit}")]
    QuotaExceeded {
        resource: String,
        used: u64,
        limit: u64,
    },

    #[error("Resource unavailable: {0}")]
    Unavailable(String),
}

pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_ERROR: u8 = 1;
pub const EXIT_CONFIG_ERROR: u8 = 2;
pub const EXIT_API_ERROR: u8 = 4;
pub const EXIT_SAFETY_ERROR: u8 = 5;
pub const EXIT_CONFIRMATION_REQUIRED: u8 = 6;

/// Determine the appropriate process exit code for an error.
pub fn get_exit_code(e: &anyhow::Error) -> u8 {
    if is_confirmation_error(e) {
        return EXIT_CONFIRMATION_REQUIRED;
    }

    if let Some(selfware_err) = e.downcast_ref::<SelfwareError>() {
        return match selfware_err {
            SelfwareError::Config(_) => EXIT_CONFIG_ERROR,
            SelfwareError::Api(_) => EXIT_API_ERROR,
            SelfwareError::Safety(_) => EXIT_SAFETY_ERROR,
            _ => EXIT_ERROR,
        };
    }

    // Direct enum unwraps fallback
    if e.downcast_ref::<ApiError>().is_some() {
        return EXIT_API_ERROR;
    }
    if e.downcast_ref::<SafetyError>().is_some() {
        return EXIT_SAFETY_ERROR;
    }
    if e.downcast_ref::<AgentError>().is_some() {
        return EXIT_ERROR;
    }

    // Fallback string matching only for cases where specific types aren't available
    let msg = e.to_string().to_lowercase();
    if msg.contains("config") {
        return EXIT_CONFIG_ERROR;
    } else if msg.contains("api error") || msg.contains("network") {
        return EXIT_API_ERROR;
    } else if msg.contains("safety") || msg.contains("blocked") {
        return EXIT_SAFETY_ERROR;
    }

    EXIT_ERROR
}
