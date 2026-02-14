//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Threat modeling
//! - Sandboxing
//! - Execution control modes

pub mod checker;
pub mod scanner;
pub mod threat_modeling;
pub mod sandbox;
pub mod autonomy;
pub mod redact;

#[cfg(feature = "execution-modes")]
pub mod confirm;
#[cfg(feature = "execution-modes")]
pub mod dry_run;
#[cfg(feature = "execution-modes")]
pub mod yolo;

// Re-exports for convenience
pub use checker::SafetyChecker;
pub use scanner::{SecurityScanner, SecuritySeverity, SecurityCategory, SecurityFinding, SecretScanner};
pub use sandbox::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
pub use autonomy::{AutonomyLevel, AutonomyController, AutonomyContext};
pub use threat_modeling::{StrideCategory, Threat, Asset, SecurityControl};
