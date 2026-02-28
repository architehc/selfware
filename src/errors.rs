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

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // is_confirmation_error tests
    // =========================================================================

    #[test]
    fn test_is_confirmation_error_with_selfware_agent_wrapper() {
        // SelfwareError::Agent(AgentError::ConfirmationRequired) wrapped in anyhow
        let err = SelfwareError::Agent(AgentError::ConfirmationRequired {
            tool_name: "shell_exec".to_string(),
        });
        let anyhow_err: anyhow::Error = err.into();
        assert!(
            is_confirmation_error(&anyhow_err),
            "SelfwareError::Agent(ConfirmationRequired) should be detected"
        );
    }

    #[test]
    fn test_is_confirmation_error_with_direct_agent_error() {
        // AgentError::ConfirmationRequired put directly into anyhow (no SelfwareError wrapper)
        let err: anyhow::Error = AgentError::ConfirmationRequired {
            tool_name: "file_write".to_string(),
        }
        .into();
        assert!(
            is_confirmation_error(&err),
            "Direct AgentError::ConfirmationRequired should be detected"
        );
    }

    #[test]
    fn test_is_confirmation_error_plain_anyhow() {
        let err = anyhow::anyhow!("something went wrong");
        assert!(
            !is_confirmation_error(&err),
            "Plain anyhow error should not be a confirmation error"
        );
    }

    #[test]
    fn test_is_confirmation_error_api_error() {
        let err: anyhow::Error = SelfwareError::Api(ApiError::Timeout).into();
        assert!(
            !is_confirmation_error(&err),
            "ApiError::Timeout should not be a confirmation error"
        );
    }

    #[test]
    fn test_is_confirmation_error_tool_error() {
        let err: anyhow::Error = SelfwareError::Tool(ToolError::NotFound {
            name: "missing_tool".to_string(),
        })
        .into();
        assert!(
            !is_confirmation_error(&err),
            "ToolError should not be a confirmation error"
        );
    }

    #[test]
    fn test_is_confirmation_error_safety_error() {
        let err: anyhow::Error = SelfwareError::Safety(SafetyError::BlockedPath {
            path: "/etc/passwd".to_string(),
        })
        .into();
        assert!(
            !is_confirmation_error(&err),
            "SafetyError should not be a confirmation error"
        );
    }

    #[test]
    fn test_is_confirmation_error_safety_confirmation_required() {
        // SafetyError also has a ConfirmationRequired variant, but it is NOT the agent one
        let err: anyhow::Error = SelfwareError::Safety(SafetyError::ConfirmationRequired {
            action: "delete all files".to_string(),
        })
        .into();
        assert!(
            !is_confirmation_error(&err),
            "SafetyError::ConfirmationRequired is not the agent-level confirmation error"
        );
    }

    #[test]
    fn test_is_confirmation_error_other_agent_errors() {
        let cases: Vec<AgentError> = vec![
            AgentError::IterationLimit { limit: 10 },
            AgentError::StepTimeout { seconds: 30 },
            AgentError::Cancelled,
            AgentError::MissingSystemPrompt,
            AgentError::Panic("oops".to_string()),
            AgentError::InvalidStateTransition {
                from: "A".to_string(),
                to: "B".to_string(),
            },
        ];
        for agent_err in cases {
            let display = format!("{}", agent_err);
            let err: anyhow::Error = agent_err.into();
            assert!(
                !is_confirmation_error(&err),
                "AgentError '{}' should not be a confirmation error",
                display
            );
        }
    }

    // =========================================================================
    // get_exit_code tests
    // =========================================================================

    #[test]
    fn test_exit_code_confirmation_required_via_selfware_wrapper() {
        let err: anyhow::Error = SelfwareError::Agent(AgentError::ConfirmationRequired {
            tool_name: "shell_exec".to_string(),
        })
        .into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_CONFIRMATION_REQUIRED,
            "ConfirmationRequired should yield exit code 6"
        );
    }

    #[test]
    fn test_exit_code_confirmation_required_direct() {
        let err: anyhow::Error = AgentError::ConfirmationRequired {
            tool_name: "git_push".to_string(),
        }
        .into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_CONFIRMATION_REQUIRED,
            "Direct AgentError::ConfirmationRequired should yield exit code 6"
        );
    }

    #[test]
    fn test_exit_code_config_error() {
        let err: anyhow::Error = SelfwareError::Config("missing API key".to_string()).into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_CONFIG_ERROR,
            "Config error should yield exit code 2"
        );
    }

    #[test]
    fn test_exit_code_api_error_wrapped() {
        let err: anyhow::Error =
            SelfwareError::Api(ApiError::Authentication("bad key".to_string())).into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_API_ERROR,
            "Api error should yield exit code 4"
        );
    }

    #[test]
    fn test_exit_code_api_error_direct() {
        // ApiError placed directly into anyhow (not wrapped in SelfwareError)
        let err: anyhow::Error = ApiError::Timeout.into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_API_ERROR,
            "Direct ApiError should yield exit code 4"
        );
    }

    #[test]
    fn test_exit_code_safety_error_wrapped() {
        let err: anyhow::Error = SelfwareError::Safety(SafetyError::BlockedCommand {
            command: "rm -rf /".to_string(),
            reason: "dangerous".to_string(),
        })
        .into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_SAFETY_ERROR,
            "Safety error should yield exit code 5"
        );
    }

    #[test]
    fn test_exit_code_safety_error_direct() {
        let err: anyhow::Error = SafetyError::SecretDetected {
            finding: "AWS key".to_string(),
        }
        .into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_SAFETY_ERROR,
            "Direct SafetyError should yield exit code 5"
        );
    }

    #[test]
    fn test_exit_code_agent_error_non_confirmation() {
        // Non-confirmation AgentError should yield generic EXIT_ERROR
        let err: anyhow::Error =
            SelfwareError::Agent(AgentError::IterationLimit { limit: 50 }).into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_ERROR,
            "Non-confirmation agent error should yield exit code 1"
        );
    }

    #[test]
    fn test_exit_code_tool_error() {
        let err: anyhow::Error = SelfwareError::Tool(ToolError::Execution {
            name: "shell_exec".to_string(),
            message: "command not found".to_string(),
        })
        .into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_ERROR,
            "Tool error should yield exit code 1"
        );
    }

    #[test]
    fn test_exit_code_session_error() {
        let err: anyhow::Error =
            SelfwareError::Session(SessionError::CheckpointSave("disk full".to_string())).into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_ERROR,
            "Session error should yield exit code 1"
        );
    }

    #[test]
    fn test_exit_code_internal_error() {
        let err: anyhow::Error = SelfwareError::Internal("unexpected state".to_string()).into();
        assert_eq!(
            get_exit_code(&err),
            EXIT_ERROR,
            "Internal error should yield exit code 1"
        );
    }

    #[test]
    fn test_exit_code_plain_anyhow_default() {
        // A plain anyhow error with no recognizable keywords falls back to EXIT_ERROR
        let err = anyhow::anyhow!("something completely unexpected happened");
        assert_eq!(
            get_exit_code(&err),
            EXIT_ERROR,
            "Unrecognized plain anyhow error should yield exit code 1"
        );
    }

    #[test]
    fn test_exit_code_string_fallback_config() {
        // Plain anyhow with "config" in the message triggers string fallback
        let err = anyhow::anyhow!("config file not found");
        assert_eq!(
            get_exit_code(&err),
            EXIT_CONFIG_ERROR,
            "String containing 'config' should fallback to exit code 2"
        );
    }

    #[test]
    fn test_exit_code_string_fallback_api_error() {
        let err = anyhow::anyhow!("api error: rate limited");
        assert_eq!(
            get_exit_code(&err),
            EXIT_API_ERROR,
            "String containing 'api error' should fallback to exit code 4"
        );
    }

    #[test]
    fn test_exit_code_string_fallback_network() {
        let err = anyhow::anyhow!("network connection refused");
        assert_eq!(
            get_exit_code(&err),
            EXIT_API_ERROR,
            "String containing 'network' should fallback to exit code 4"
        );
    }

    #[test]
    fn test_exit_code_string_fallback_safety() {
        let err = anyhow::anyhow!("safety violation detected");
        assert_eq!(
            get_exit_code(&err),
            EXIT_SAFETY_ERROR,
            "String containing 'safety' should fallback to exit code 5"
        );
    }

    #[test]
    fn test_exit_code_string_fallback_blocked() {
        let err = anyhow::anyhow!("operation blocked by policy");
        assert_eq!(
            get_exit_code(&err),
            EXIT_SAFETY_ERROR,
            "String containing 'blocked' should fallback to exit code 5"
        );
    }

    #[test]
    fn test_exit_code_constants() {
        assert_eq!(EXIT_SUCCESS, 0);
        assert_eq!(EXIT_ERROR, 1);
        assert_eq!(EXIT_CONFIG_ERROR, 2);
        assert_eq!(EXIT_API_ERROR, 4);
        assert_eq!(EXIT_SAFETY_ERROR, 5);
        assert_eq!(EXIT_CONFIRMATION_REQUIRED, 6);
    }
}
