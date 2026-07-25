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

    #[error("Shell error: {0}")]
    Shell(#[from] ShellError),

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

impl From<serde_json::Error> for SelfwareError {
    fn from(e: serde_json::Error) -> Self {
        SelfwareError::Internal(format!("JSON error: {}", e))
    }
}

impl From<url::ParseError> for SelfwareError {
    fn from(e: url::ParseError) -> Self {
        SelfwareError::Internal(format!("URL parse error: {}", e))
    }
}

impl From<std::io::Error> for SelfwareError {
    fn from(e: std::io::Error) -> Self {
        SelfwareError::Internal(format!("IO error: {}", e))
    }
}

impl From<glob::PatternError> for SelfwareError {
    fn from(e: glob::PatternError) -> Self {
        SelfwareError::Safety(SafetyError::Internal(format!(
            "Invalid glob pattern: {}",
            e
        )))
    }
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

    #[error("Task failed: {message}")]
    TaskFailed { message: String },

    #[error("Visual assertion failed: {description}. Expected: {expected}, Got: {actual}. Recovery hint: {recovery_hint}")]
    VisualAssertionFailed {
        description: String,
        expected: String,
        actual: String,
        recovery_hint: String,
    },

    #[error("Visual verification error: {0}")]
    VisualVerificationError(String),

    #[error("Visual stuck loop detected: same screen and failed action repeated {count} times. Recovery: {recovery_hint}")]
    VisualStuckLoop {
        count: usize,
        recovery_hint: String,
        last_screenshot: PathBuf,
    },
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

    #[error("Context overflow: {0}")]
    ContextOverflow(String),

    #[error("Invalid token usage from API: {0}")]
    InvalidUsage(String),
}

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Tool '{name}' failed: {message}")]
    Execution { name: String, message: String },

    #[error("Tool '{name}' not found")]
    NotFound { name: String },

    #[error("Invalid tool call for '{name}': {message}")]
    InvalidToolCall { name: String, message: String },

    #[error("Invalid arguments for tool '{name}': {message}")]
    InvalidArguments { name: String, message: String },

    #[error("Tool execution timed out")]
    Timeout,

    // File-specific errors
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Path is a directory, not a file: {path}")]
    PathIsDirectory { path: String },

    #[error("File too large to read: {size} bytes (limit: {limit} bytes)")]
    FileTooLarge { size: u64, limit: u64 },

    #[error("Content too large to write: {size} bytes (limit: {limit} bytes)")]
    WriteTooLarge { size: usize, limit: usize },

    #[error("old_str not found in file")]
    EditStringNotFound,

    #[error("old_str matches {count} times, expected exactly 1")]
    EditStringMultiple { count: usize },

    #[error("old_str and new_str are identical — this is a no-op edit")]
    EditNoOp,

    #[error("Rust syntax validation failed for {path}: {message}")]
    InvalidRustSyntax { path: String, message: String },

    #[error("Test mode only valid for test fixtures, got: {path}")]
    TestModeInvalidPath { path: String },

    #[error("Test fixture not found or not in allowed directory: {path}")]
    TestFixtureNotFound { path: String },

    #[error("File {path} changed on disk since you last read it. Re-read the file and try again.")]
    FileStale { path: String },
}

#[derive(Error, Debug)]
pub enum ShellError {
    #[error("Invalid working directory '{path}': {reason}")]
    InvalidCwd { path: String, reason: String },
}

#[derive(Error, Debug)]
pub enum SafetyError {
    // Path validation errors
    #[error("Path blocked by safety policy: {path}")]
    BlockedPath { path: String },

    #[error("Path traversal attempt detected: {path}")]
    PathTraversal { path: String },

    #[error("Path contains null bytes")]
    PathNullBytes,

    #[error("Path contains suspicious Unicode character: {character} (U+{codepoint:04X}) - possible homoglyph bypass attempt")]
    PathSuspiciousUnicode { character: String, codepoint: u32 },

    #[error(
        "Path component '{component}' contains suspicious mix of ASCII and non-ASCII characters"
    )]
    PathSuspiciousMix { component: String },

    #[error("Path not in allowed list: {path}")]
    PathNotAllowed { path: String },

    #[error("Path '{path}' is outside working directory and no allowed_paths configured")]
    PathOutsideWorkspace { path: String },

    #[error("Failed to canonicalize path safely: {path}")]
    PathCanonicalizationFailed { path: String },

    #[error("Path matches denied pattern: {pattern}")]
    PathDeniedPattern { pattern: String },

    #[error("Access to protected system path is not allowed: {path}")]
    PathProtectedSystem { path: String },

    // Symlink validation errors
    #[error("Symlink loop detected: {path}")]
    SymlinkLoop { path: String },

    #[error("Symlink points to protected system path: {symlink} -> {target}")]
    SymlinkProtectedTarget { symlink: String, target: String },

    #[error("Symlink chain too deep (possible attack): {path}")]
    SymlinkChainTooDeep { path: String },

    // Command validation errors
    #[error("Dangerous command blocked: {command} (reason: {reason})")]
    BlockedCommand { command: String, reason: String },

    #[error("Dangerous command blocked: {description}")]
    DangerousCommandPattern { description: String },

    #[error("Dangerous command blocked: base64-encoded command execution")]
    BlockedBase64Command,

    #[error("Dangerous command blocked: hex-encoded command execution")]
    BlockedHexCommand,

    #[error("Dangerous command blocked: encoded command execution")]
    BlockedEncodedCommand,

    #[error("Dangerous command blocked: environment variable injection detected")]
    BlockedEnvInjection,

    // Git safety errors
    #[error("Force push is blocked for safety. Use --no-force or confirm manually")]
    BlockedForcePush,

    #[error("Push to protected branch '{branch}' is blocked by the safety checker (protected_branches: {protected:?})")]
    BlockedProtectedBranchPush {
        branch: String,
        protected: Vec<String>,
    },

    // Tool registration errors
    #[error("Unregistered tool '{tool}' blocked by safety checker. Register it in checker.rs to allow execution")]
    UnregisteredTool { tool: String },

    // Secret detection errors
    #[error("Potential secret detected in content: {finding}")]
    SecretDetected { finding: String },

    // Container security errors
    #[error("Dangerous container volume mount blocked: {mount} (mounts sensitive SSH material)")]
    ContainerSshMount { mount: String },

    #[error(
        "Dangerous container volume mount blocked: {mount} (mounts system directory {directory})"
    )]
    ContainerSystemMount { mount: String, directory: String },

    // Network policy errors
    #[error("Blocked request: only http/https schemes are allowed (got {scheme})")]
    BlockedUrlScheme { scheme: String },

    #[error("Blocked request to cloud metadata endpoint: {host}")]
    BlockedCloudMetadata { host: String },

    #[error("Blocked request to encoded cloud metadata endpoint (bypass attempt)")]
    BlockedEncodedMetadata,

    #[error("Blocked request to link-local address range (169.254.x.x)")]
    BlockedLinkLocal,

    #[error("Blocked request to private network address: {ip}")]
    BlockedPrivateNetwork { ip: String },

    #[error("Suspicious browser eval blocked: potential data exfiltration")]
    BlockedBrowserEval,

    #[error("Network policy violation: {reason}")]
    NetworkPolicyViolation { reason: String },

    // General confirmation
    #[error("Action requires manual confirmation: {action}")]
    ConfirmationRequired { action: String },

    // Internal errors (for wrapping lower-level errors)
    #[error("Internal safety error: {0}")]
    Internal(String),
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

/// Check if an anyhow error is a "no action" error (agent failed to take action after multiple prompts)
/// These are fatal errors - the agent is stuck in a loop describing intent without executing.
pub fn is_no_action_error(e: &anyhow::Error) -> bool {
    // Check if wrapped as SelfwareError::Agent(AgentError::TaskFailed)
    if let Some(SelfwareError::Agent(AgentError::TaskFailed { message })) =
        e.downcast_ref::<SelfwareError>()
    {
        return message.contains("failed to take action after");
    }

    // Also check if AgentError was returned directly into anyhow
    if let Some(AgentError::TaskFailed { message }) = e.downcast_ref::<AgentError>() {
        return message.contains("failed to take action after");
    }

    // Check the error message directly
    let error_string = e.to_string();
    error_string.contains("failed to take action after")
        || error_string.contains("Agent failed to take action")
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
#[path = "../tests/unit/errors/errors_test.rs"]
mod tests;
