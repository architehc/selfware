//! Error types for Selfware

use thiserror::Error;

/// Main error type for Selfware operations
#[derive(Error, Debug)]
pub enum SelfwareError {
    /// Checkpoint-related errors
    #[error("Checkpoint error: {0}")]
    Checkpoint(#[from] CheckpointError),
    
    /// Resource management errors
    #[error("Resource error: {0}")]
    Resource(#[from] ResourceError),
    
    /// LLM inference errors
    #[error("LLM error: {0}")]
    LLM(#[from] LLMError),
    
    /// Supervision/recovery errors
    #[error("Supervision error: {0}")]
    Supervision(#[from] SupervisionError),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Serialization errors
    #[error("Serialization error: {0}")]
    Serialization(#[from] Box<bincode::ErrorKind>),
    
    /// Storage errors
    #[error("Storage error: {0}")]
    Storage(String),
    
    /// Timeout errors
    #[error("Operation timed out")]
    Timeout,
    
    /// Cancelled errors
    #[error("Operation cancelled")]
    Cancelled,
    
    /// Unknown errors
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Checkpoint-specific errors
#[derive(Error, Debug)]
pub enum CheckpointError {
    #[error("Failed to create checkpoint: {0}")]
    CreationFailed(String),
    
    #[error("Failed to restore checkpoint: {0}")]
    RestoreFailed(String),
    
    #[error("Checkpoint not found: {0}")]
    NotFound(String),
    
    #[error("Checkpoint corrupted: {0}")]
    Corrupted(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Compression error: {0}")]
    Compression(String),
    
    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),
}

/// Resource management errors
#[derive(Error, Debug)]
pub enum ResourceError {
    #[error("GPU error: {0}")]
    Gpu(String),
    
    #[error("Memory exhausted: {0}")]
    MemoryExhausted(String),
    
    #[error("Disk space exhausted: {0}")]
    DiskExhausted(String),
    
    #[error("Quota exceeded: {resource} ({used} > {limit})")]
    QuotaExceeded {
        resource: String,
        used: u64,
        limit: u64,
    },
    
    #[error("Resource unavailable: {0}")]
    Unavailable(String),
}

/// LLM inference errors
#[derive(Error, Debug)]
pub enum LLMError {
    #[error("Model loading failed: {0}")]
    ModelLoadFailed(String),
    
    #[error("Inference failed: {0}")]
    InferenceFailed(String),
    
    #[error("Context limit exceeded: {used} > {limit}")]
    ContextLimitExceeded { used: usize, limit: usize },
    
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    
    #[error("GPU out of memory")]
    OutOfMemory,
    
    #[error("Request cancelled")]
    Cancelled,
    
    #[error("Request timeout")]
    Timeout,
}

/// Supervision errors
#[derive(Error, Debug)]
pub enum SupervisionError {
    #[error("Child process crashed: {0}")]
    ChildCrashed(String),
    
    #[error("Max restarts exceeded: {child_id}")]
    MaxRestartsExceeded { child_id: String },
    
    #[error("Health check failed: {0}")]
    HealthCheckFailed(String),
    
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
    
    #[error("Watchdog timeout")]
    WatchdogTimeout,
}

/// Trait for errors that can be recovered from
pub trait Recoverable {
    /// Check if this error is recoverable
    fn is_recoverable(&self) -> bool;
    
    /// Get the recommended recovery action
    fn recovery_action(&self) -> RecoveryAction;
}

/// Recovery actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Retry immediately
    RetryImmediate,
    /// Retry with backoff
    RetryWithBackoff,
    /// Restart component
    RestartComponent,
    /// Restart system
    RestartSystem,
    /// Escalate to human
    Escalate,
    /// Cannot recover
    Fatal,
}

impl Recoverable for SelfwareError {
    fn is_recoverable(&self) -> bool {
        match self {
            Self::Checkpoint(e) => matches!(e,
                CheckpointError::Storage(_) |
                CheckpointError::Compression(_)
            ),
            Self::Resource(e) => matches!(e,
                ResourceError::MemoryExhausted(_) |
                ResourceError::DiskExhausted(_) |
                ResourceError::Unavailable(_)
            ),
            Self::LLM(e) => matches!(e,
                LLMError::InferenceFailed(_) |
                LLMError::OutOfMemory |
                LLMError::Timeout
            ),
            Self::Supervision(e) => matches!(e,
                SupervisionError::ChildCrashed(_) |
                SupervisionError::HealthCheckFailed(_)
            ),
            Self::Io(_) => true,
            Self::Timeout => true,
            Self::Cancelled => false,
            _ => false,
        }
    }
    
    fn recovery_action(&self) -> RecoveryAction {
        match self {
            Self::Checkpoint(_) => RecoveryAction::RetryWithBackoff,
            Self::Resource(ResourceError::MemoryExhausted(_)) => RecoveryAction::RestartComponent,
            Self::Resource(ResourceError::DiskExhausted(_)) => RecoveryAction::Escalate,
            Self::LLM(LLMError::OutOfMemory) => RecoveryAction::RestartComponent,
            Self::LLM(LLMError::Timeout) => RecoveryAction::RetryWithBackoff,
            Self::Supervision(SupervisionError::MaxRestartsExceeded { .. }) => RecoveryAction::RestartSystem,
            Self::Io(_) => RecoveryAction::RetryImmediate,
            Self::Timeout => RecoveryAction::RetryWithBackoff,
            _ => RecoveryAction::Fatal,
        }
    }
}
