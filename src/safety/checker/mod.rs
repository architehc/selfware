//! Safety Layer - Tool Call Validation
//!
//! Validates tool calls before execution to prevent dangerous operations.
//! Checks include:
//! - Path traversal prevention
//! - Protected path enforcement
//! - Command blacklisting
//! - Symlink attack prevention

pub mod types;
pub mod validation;

pub use types::SafetyChecker;

#[cfg(test)]
mod tests;
