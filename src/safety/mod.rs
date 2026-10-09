//! Security and safety module
//!
//! This module contains security-related functionality including:
//! - Safety checking and validation
//! - Security scanning
//! - Execution control modes

pub mod audit;
pub mod checker;
pub mod confirm_view;
pub mod git_exec;
pub mod killswitch;
pub mod path_validator;
pub mod permissions;
pub mod process_env;
pub mod quarantine;
pub mod recursive_read;
pub mod redact;
pub mod scanner;
pub mod shell_read;
pub(crate) mod source_context;
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
pub use killswitch::{check_killswitch, is_killswitch_active, KillswitchError, KillswitchStatus};
pub use scanner::{
    SecretScanner, SecurityCategory, SecurityFinding, SecurityScanner, SecuritySeverity,
};
pub use tool_metadata::{
    classify_tool_metadata, default_tool_metadata, normal_mode_needs_confirmation, ExecutionMode,
    PermissionChecker, PermissionResult, RiskLevel, ToolMetadata,
};

/// Internal bridge for standalone workspace binaries, which are separate
/// crates and cannot call the crate-private source-context module directly.
#[doc(hidden)]
pub fn broad_discovery_file_is_safe(path: &std::path::Path) -> bool {
    source_context::broad_discovery_file_is_safe(path)
}

#[cfg(test)]
#[path = "../../tests/unit/safety/mod_test.rs"]
mod tests;
