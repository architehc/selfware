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
pub use audit::{AuditEvent, AuditEventType, AuditLogger};
pub use checker::validation::{
    is_private_or_internal, normalize_shell_command, split_shell_commands, PinnedDnsResolver,
};
pub use checker::SafetyChecker;
pub use confirm_view::{
    classify_risk, classify_shell_risk, FullDiffView, PermissionAnswer, PermissionPrompt, RiskTag,
};
pub use git_exec::{git_command, git_command_async, GitScope, HardenedGitEnv, SanitizedGitExt};
pub use killswitch::{check_killswitch, is_killswitch_active, KillswitchError, KillswitchStatus};
pub use path_validator::{lexical_normalize_path, PathValidator, ValidatedFile};
pub use permissions::{PermissionGrant, PermissionStore, ShellAllowRule};
pub use process_env::{
    pin_rustup_toolchain, pin_std_rustup_toolchain, protect_process_env, sanitize_command_env,
    sanitize_std_command_env, SanitizedEnvExt,
};
pub use quarantine::{ArmQuarantine, HostToolchain, IsolationReport, QuarantineOptions};
pub use redact::{
    redact_for_model, redact_json_for_model, redact_secrets, ModelRedaction, RedactionContext,
};
pub use scanner::{
    ComplianceChecker, ComplianceRule, Dependency, DependencyAuditor, KnownVulnerability,
    ScanResult, ScannerStats, SecretPattern, SecretScanner, SecurityCategory, SecurityFinding,
    SecurityScanner, SecuritySeverity, VulnerabilityDetector, VulnerabilityPattern,
};
pub use tool_metadata::{
    classify_tool_metadata, default_tool_metadata, normal_mode_needs_confirmation, ExecutionMode,
    PermissionChecker, PermissionResult, RiskLevel, ToolMetadata,
};
pub use yolo::YoloConfig;

/// Internal bridge for standalone workspace binaries, which are separate
/// crates and cannot call the crate-private source-context module directly.
#[doc(hidden)]
pub fn broad_discovery_file_is_safe(path: &std::path::Path) -> bool {
    source_context::broad_discovery_file_is_safe(path)
}

#[cfg(test)]
#[path = "../../tests/unit/safety/mod_test.rs"]
mod tests;
