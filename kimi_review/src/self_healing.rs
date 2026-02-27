//! Self-healing and error recovery mechanisms

use crate::error::{Recoverable, RecoveryAction, SelfwareError};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Self-healing manager for automatic error recovery
pub struct SelfHealingManager {
    max_retries: u32,
    backoff_base_ms: u64,
}

impl SelfHealingManager {
    /// Create a new self-healing manager
    pub fn new(max_retries: u32, backoff_base_ms: u64) -> Self {
        Self {
            max_retries,
            backoff_base_ms,
        }
    }
    
    /// Execute an operation with automatic recovery
    pub async fn execute_with_recovery<F, Fut, T>(
        &self,
        operation: F,
        context: &str,
    ) -> Result<T, SelfwareError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, SelfwareError>>,
    {
        let mut last_error = None;
        
        for attempt in 0..self.max_retries {
            match operation().await {
                Ok(result) => {
                    if attempt > 0 {
                        info!(
                            context = context,
                            attempt = attempt + 1,
                            "Operation succeeded after recovery"
                        );
                    }
                    return Ok(result);
                }
                Err(e) if e.is_recoverable() && attempt < self.max_retries - 1 => {
                    let action = e.recovery_action();
                    let backoff = self.calculate_backoff(attempt, &action);
                    
                    warn!(
                        context = context,
                        attempt = attempt + 1,
                        error = %e,
                        action = ?action,
                        backoff_ms = backoff.as_millis(),
                        "Operation failed, attempting recovery"
                    );
                    
                    last_error = Some(e);
                    sleep(backoff).await;
                }
                Err(e) => {
                    error!(
                        context = context,
                        error = %e,
                        "Operation failed (unrecoverable)"
                    );
                    return Err(e);
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| {
            SelfwareError::Unknown("Max retries exceeded".to_string())
        }))
    }
    
    /// Calculate backoff duration based on action
    fn calculate_backoff(&self, attempt: u32, action: &RecoveryAction) -> Duration {
        let base = self.backoff_base_ms * (2_u64.pow(attempt.min(10)));
        
        match action {
            RecoveryAction::RetryImmediate => Duration::from_millis(100),
            RecoveryAction::RetryWithBackoff => Duration::from_millis(base),
            RecoveryAction::RestartComponent => Duration::from_secs(5),
            RecoveryAction::RestartSystem => Duration::from_secs(30),
            RecoveryAction::Escalate => Duration::from_secs(60),
            RecoveryAction::Fatal => Duration::from_secs(0),
        }
    }
}

impl Default for SelfHealingManager {
    fn default() -> Self {
        Self {
            max_retries: 5,
            backoff_base_ms: 1000,
        }
    }
}

/// Error classifier for determining recovery strategy
pub struct ErrorClassifier;

impl ErrorClassifier {
    /// Classify an error and determine recovery action
    pub fn classify(error: &SelfwareError) -> ErrorClassification {
        match error {
            SelfwareError::Checkpoint(e) => match e {
                crate::error::CheckpointError::Storage(_) => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: true,
                    action: RecoveryAction::RetryWithBackoff,
                },
                crate::error::CheckpointError::Corrupted(_) => ErrorClassification {
                    severity: ErrorSeverity::Critical,
                    recoverable: true,
                    action: RecoveryAction::RestartComponent,
                },
                _ => ErrorClassification {
                    severity: ErrorSeverity::Medium,
                    recoverable: true,
                    action: RecoveryAction::RetryImmediate,
                },
            },
            SelfwareError::Resource(e) => match e {
                crate::error::ResourceError::MemoryExhausted(_) => ErrorClassification {
                    severity: ErrorSeverity::Critical,
                    recoverable: true,
                    action: RecoveryAction::RestartComponent,
                },
                crate::error::ResourceError::DiskExhausted(_) => ErrorClassification {
                    severity: ErrorSeverity::Critical,
                    recoverable: false,
                    action: RecoveryAction::Escalate,
                },
                crate::error::ResourceError::Gpu(_) => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: true,
                    action: RecoveryAction::RetryWithBackoff,
                },
                _ => ErrorClassification {
                    severity: ErrorSeverity::Medium,
                    recoverable: true,
                    action: RecoveryAction::RetryImmediate,
                },
            },
            SelfwareError::LLM(e) => match e {
                crate::error::LLMError::OutOfMemory => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: true,
                    action: RecoveryAction::RestartComponent,
                },
                crate::error::LLMError::Timeout => ErrorClassification {
                    severity: ErrorSeverity::Medium,
                    recoverable: true,
                    action: RecoveryAction::RetryWithBackoff,
                },
                crate::error::LLMError::InferenceFailed(_) => ErrorClassification {
                    severity: ErrorSeverity::Medium,
                    recoverable: true,
                    action: RecoveryAction::RetryImmediate,
                },
                _ => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: false,
                    action: RecoveryAction::Escalate,
                },
            },
            SelfwareError::Supervision(e) => match e {
                crate::error::SupervisionError::MaxRestartsExceeded { .. } => ErrorClassification {
                    severity: ErrorSeverity::Critical,
                    recoverable: true,
                    action: RecoveryAction::RestartSystem,
                },
                crate::error::SupervisionError::CircuitBreakerOpen => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: true,
                    action: RecoveryAction::RetryWithBackoff,
                },
                _ => ErrorClassification {
                    severity: ErrorSeverity::High,
                    recoverable: true,
                    action: RecoveryAction::RestartComponent,
                },
            },
            SelfwareError::Io(_) => ErrorClassification {
                severity: ErrorSeverity::Low,
                recoverable: true,
                action: RecoveryAction::RetryImmediate,
            },
            SelfwareError::Timeout => ErrorClassification {
                severity: ErrorSeverity::Medium,
                recoverable: true,
                action: RecoveryAction::RetryWithBackoff,
            },
            _ => ErrorClassification {
                severity: ErrorSeverity::High,
                recoverable: false,
                action: RecoveryAction::Fatal,
            },
        }
    }
}

/// Error classification result
#[derive(Debug, Clone)]
pub struct ErrorClassification {
    pub severity: ErrorSeverity,
    pub recoverable: bool,
    pub action: RecoveryAction,
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Recovery strategies
pub struct RecoveryStrategies;

impl RecoveryStrategies {
    /// Attempt to recover from an error
    pub async fn recover(error: &SelfwareError) -> Result<(), SelfwareError> {
        let classification = ErrorClassifier::classify(error);
        
        if !classification.recoverable {
            return Err(error.clone());
        }
        
        match classification.action {
            RecoveryAction::RetryImmediate => {
                info!("Retrying immediately");
                Ok(())
            }
            RecoveryAction::RetryWithBackoff => {
                info!("Will retry with backoff");
                Ok(())
            }
            RecoveryAction::RestartComponent => {
                warn!("Restarting component");
                // Would trigger component restart
                Ok(())
            }
            RecoveryAction::RestartSystem => {
                error!("Restarting system");
                // Would trigger system restart
                Ok(())
            }
            RecoveryAction::Escalate => {
                error!("Escalating to human operator");
                // Would notify human
                Err(error.clone())
            }
            RecoveryAction::Fatal => {
                error!("Fatal error, cannot recover");
                Err(error.clone())
            }
        }
    }
}
