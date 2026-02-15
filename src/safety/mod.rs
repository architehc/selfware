//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Threat modeling
//! - Sandboxing
//! - Execution control modes

pub mod autonomy;
pub mod checker;
pub mod redact;
pub mod sandbox;
pub mod scanner;
pub mod threat_modeling;

#[cfg(feature = "execution-modes")]
pub mod confirm;
#[cfg(feature = "execution-modes")]
pub mod dry_run;
#[cfg(feature = "execution-modes")]
pub mod yolo;

// Re-exports for convenience
pub use autonomy::{AutonomyContext, AutonomyController, AutonomyLevel};
pub use checker::SafetyChecker;
pub use sandbox::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
pub use scanner::{
    SecretScanner, SecurityCategory, SecurityFinding, SecurityScanner, SecuritySeverity,
};
pub use threat_modeling::{Asset, SecurityControl, StrideCategory, Threat};
