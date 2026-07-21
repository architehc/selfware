#![allow(dead_code, unused_imports, unused_variables)]
//! Built-in hook presets for common workflows.
//!
//! These are convenience constructors for frequently used hook configurations.

use super::{HookConfig, HookEvent};

/// Create a format-on-save hook that runs `cargo fmt` after file_write/file_edit.
pub fn format_on_save_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "cargo fmt -- {path}".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 30,
    }
}

/// Create a lint-after-edit hook that runs `cargo clippy` after file changes.
pub fn lint_after_edit_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "cargo clippy --quiet -- -D warnings 2>&1 | head -20".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 60,
    }
}

/// Create a test-on-stop hook that runs `cargo test` when the agent finishes.
pub fn test_on_stop_rust() -> HookConfig {
    HookConfig {
        event: HookEvent::Stop,
        command: "cargo test --quiet 2>&1 | tail -5".to_string(),
        match_tools: vec![],
        timeout_secs: 120,
    }
}

/// Create an auto-commit hook for file changes.
pub fn auto_commit() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "git add {path} && git commit -m \"[selfware] auto-commit: {tool} {path}\" --no-verify 2>/dev/null || true".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

/// Create a Python format-on-save hook.
pub fn format_on_save_python() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "ruff format {path} 2>/dev/null || black {path} 2>/dev/null || true".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

/// Create a Node.js format-on-save hook.
pub fn format_on_save_node() -> HookConfig {
    HookConfig {
        event: HookEvent::PostToolUse,
        command: "npx prettier --write {path} 2>/dev/null || true".to_string(),
        match_tools: vec!["file_write".to_string(), "file_edit".to_string()],
        timeout_secs: 15,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/hooks/builtin/builtin_test.rs"]
mod tests;
