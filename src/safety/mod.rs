//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Execution control modes

pub mod audit;
pub mod checker;
pub mod context_guard;
pub mod path_validator;
pub mod permissions;
pub mod process_env;
pub mod redact;
pub mod scanner;
pub mod tool_metadata;

#[cfg(feature = "execution-modes")]
pub mod confirm;
#[cfg(feature = "execution-modes")]
pub mod dry_run;
pub mod yolo;

// Re-exports for convenience
pub use checker::validation::{
    is_private_or_internal, normalize_shell_command, split_shell_commands, PinnedDnsResolver,
};
pub use checker::SafetyChecker;
pub use context_guard::{
    ContextGuard, ContextPollutionKind, ContextSourceProvenance, ContextTraceabilityReport,
    TaintLevel,
};
pub use scanner::{
    SecretScanner, SecurityCategory, SecurityFinding, SecurityScanner, SecuritySeverity,
};
pub use tool_metadata::{
    classify_tool_metadata, default_tool_metadata, normal_mode_needs_confirmation, ExecutionMode,
    PermissionChecker, PermissionResult, RiskLevel, ToolMetadata,
};

#[cfg(test)]
#[path = "../../tests/unit/safety/mod_test.rs"]
mod tests;
