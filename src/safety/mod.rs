//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Threat modeling
//! - Sandboxing
//! - Execution control modes

pub mod audit;
pub mod autonomy;
pub mod checker;
pub mod path_validator;
pub mod permissions;
pub mod redact;
pub mod sandbox;
pub mod scanner;
pub mod threat_modeling;
pub mod tool_metadata;

#[cfg(feature = "execution-modes")]
pub mod confirm;
#[cfg(feature = "execution-modes")]
pub mod dry_run;
#[cfg(feature = "execution-modes")]
pub mod yolo;

// Re-exports for convenience
pub use autonomy::{AutonomyContext, AutonomyController, AutonomyLevel};
pub use checker::validation::{
    is_private_or_internal, normalize_shell_command, split_shell_commands, PinnedDnsResolver,
};
pub use checker::SafetyChecker;
pub use sandbox::{FilesystemPolicy, NetworkPolicy, ResourceLimits};
pub use scanner::{
    SecretScanner, SecurityCategory, SecurityFinding, SecurityScanner, SecuritySeverity,
};
pub use threat_modeling::{Asset, SecurityControl, StrideCategory, Threat};
pub use tool_metadata::{
    default_tool_metadata, ExecutionMode, PermissionChecker, PermissionResult, RiskLevel,
    ToolMetadata,
};
