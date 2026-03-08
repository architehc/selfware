#![allow(dead_code, unused_imports, unused_variables)]
//! Hook system for extensible event-driven automation.
//!
//! Hooks allow running custom commands at key points in the agent lifecycle:
//! - **PreToolUse**: Before a tool is executed (can block execution)
//! - **PostToolUse**: After a tool completes (e.g., auto-format, lint)
//! - **Stop**: When the agent finishes a task (e.g., run tests, auto-commit)

pub mod builtin;
pub mod shell_handler;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Events that can trigger hooks.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HookEvent {
    /// Fired before a tool is executed.
    PreToolUse,
    /// Fired after a tool completes successfully.
    PostToolUse,
    /// Fired when the agent completes a task.
    Stop,
}

impl std::fmt::Display for HookEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookEvent::PreToolUse => write!(f, "PreToolUse"),
            HookEvent::PostToolUse => write!(f, "PostToolUse"),
            HookEvent::Stop => write!(f, "Stop"),
        }
    }
}

/// Result of running a hook.
#[derive(Debug, Clone)]
pub enum HookAction {
    /// Continue normal execution.
    Continue,
    /// Skip the current tool execution (only meaningful for PreToolUse).
    Skip { reason: String },
    /// An error occurred running the hook (logged but does not block).
    Error { message: String },
}

/// Context passed to hooks when they fire.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// The event that triggered this hook.
    pub event: HookEvent,
    /// Tool name (for PreToolUse/PostToolUse events).
    pub tool_name: Option<String>,
    /// Tool arguments as JSON string (for PreToolUse/PostToolUse events).
    pub tool_args: Option<String>,
    /// Whether the tool succeeded (for PostToolUse events).
    pub tool_success: Option<bool>,
    /// Tool result string (for PostToolUse events).
    pub tool_result: Option<String>,
    /// File path affected by the tool, if any.
    pub affected_path: Option<String>,
}

impl HookContext {
    pub fn pre_tool(tool_name: &str, tool_args: &str) -> Self {
        let affected_path = extract_path_from_args(tool_args);
        Self {
            event: HookEvent::PreToolUse,
            tool_name: Some(tool_name.to_string()),
            tool_args: Some(tool_args.to_string()),
            tool_success: None,
            tool_result: None,
            affected_path,
        }
    }

    pub fn post_tool(tool_name: &str, tool_args: &str, success: bool, result: &str) -> Self {
        let affected_path = extract_path_from_args(tool_args);
        Self {
            event: HookEvent::PostToolUse,
            tool_name: Some(tool_name.to_string()),
            tool_args: Some(tool_args.to_string()),
            tool_success: Some(success),
            tool_result: Some(result.to_string()),
            affected_path,
        }
    }

    pub fn stop() -> Self {
        Self {
            event: HookEvent::Stop,
            tool_name: None,
            tool_args: None,
            tool_success: None,
            tool_result: None,
            affected_path: None,
        }
    }
}

/// Extract the `path` field from a JSON args string, if present.
fn extract_path_from_args(args: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(args)
        .ok()?
        .get("path")?
        .as_str()
        .map(String::from)
}

/// Configuration for a single hook (loaded from TOML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Which event triggers this hook.
    pub event: HookEvent,
    /// Shell command to execute. Supports `{path}`, `{tool}`, `{description}` placeholders.
    pub command: String,
    /// Only trigger for these tool names (empty = all tools).
    #[serde(default)]
    pub match_tools: Vec<String>,
    /// Timeout in seconds for the hook command (default: 30).
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
}

fn default_hook_timeout() -> u64 {
    30
}

/// Registry that holds all configured hooks and dispatches events.
#[derive(Debug, Clone)]
pub struct HookRegistry {
    hooks: Vec<HookConfig>,
}

impl HookRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Create a registry from configuration.
    pub fn from_config(hooks: &[HookConfig]) -> Self {
        info!("Loaded {} hook(s) from configuration", hooks.len());
        Self {
            hooks: hooks.to_vec(),
        }
    }

    /// Register a new hook at runtime.
    pub fn register(&mut self, hook: HookConfig) {
        info!(
            "Registered hook: {} on {:?} (match_tools: {:?})",
            hook.command, hook.event, hook.match_tools
        );
        self.hooks.push(hook);
    }

    /// Fire all hooks matching the given event and context.
    /// Returns the combined action (Continue or Skip if any hook requests skip).
    pub async fn fire(&self, ctx: &HookContext) -> HookAction {
        let matching: Vec<&HookConfig> = self
            .hooks
            .iter()
            .filter(|h| h.event == ctx.event)
            .filter(|h| {
                if h.match_tools.is_empty() {
                    return true;
                }
                ctx.tool_name
                    .as_ref()
                    .map(|tn| h.match_tools.iter().any(|m| m == tn))
                    .unwrap_or(false)
            })
            .collect();

        if matching.is_empty() {
            return HookAction::Continue;
        }

        debug!(
            "Firing {} hook(s) for event {:?}",
            matching.len(),
            ctx.event
        );

        for hook in matching {
            let result = shell_handler::execute_hook(hook, ctx).await;
            match result {
                HookAction::Skip { ref reason } => {
                    info!("Hook requested skip: {}", reason);
                    return result;
                }
                HookAction::Error { ref message } => {
                    warn!("Hook error (non-fatal): {}", message);
                    // Continue despite hook errors — hooks should not block the agent
                }
                HookAction::Continue => {
                    debug!("Hook completed successfully: {}", hook.command);
                }
            }
        }

        HookAction::Continue
    }

    /// Check if any hooks are registered for the given event.
    pub fn has_hooks_for(&self, event: &HookEvent) -> bool {
        self.hooks.iter().any(|h| &h.event == event)
    }

    /// Return the number of registered hooks.
    pub fn len(&self) -> usize {
        self.hooks.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.hooks.is_empty()
    }
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_display() {
        assert_eq!(HookEvent::PreToolUse.to_string(), "PreToolUse");
        assert_eq!(HookEvent::PostToolUse.to_string(), "PostToolUse");
        assert_eq!(HookEvent::Stop.to_string(), "Stop");
    }

    #[test]
    fn test_extract_path_from_args() {
        let args = r#"{"path": "./src/main.rs", "content": "test"}"#;
        assert_eq!(
            extract_path_from_args(args),
            Some("./src/main.rs".to_string())
        );

        let args = r#"{"command": "cargo test"}"#;
        assert_eq!(extract_path_from_args(args), None);
    }

    #[test]
    fn test_hook_context_constructors() {
        let ctx = HookContext::pre_tool("file_write", r#"{"path": "test.rs"}"#);
        assert_eq!(ctx.event, HookEvent::PreToolUse);
        assert_eq!(ctx.tool_name.as_deref(), Some("file_write"));
        assert_eq!(ctx.affected_path.as_deref(), Some("test.rs"));

        let ctx = HookContext::post_tool("file_edit", r#"{"path": "x.rs"}"#, true, "ok");
        assert_eq!(ctx.event, HookEvent::PostToolUse);
        assert_eq!(ctx.tool_success, Some(true));

        let ctx = HookContext::stop();
        assert_eq!(ctx.event, HookEvent::Stop);
        assert!(ctx.tool_name.is_none());
    }

    #[test]
    fn test_hook_registry_empty() {
        let registry = HookRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(!registry.has_hooks_for(&HookEvent::Stop));
    }

    #[test]
    fn test_hook_registry_from_config() {
        let hooks = vec![HookConfig {
            event: HookEvent::PostToolUse,
            command: "cargo fmt".to_string(),
            match_tools: vec!["file_write".to_string()],
            timeout_secs: 30,
        }];
        let registry = HookRegistry::from_config(&hooks);
        assert_eq!(registry.len(), 1);
        assert!(registry.has_hooks_for(&HookEvent::PostToolUse));
        assert!(!registry.has_hooks_for(&HookEvent::PreToolUse));
    }

    #[tokio::test]
    async fn test_fire_no_matching_hooks() {
        let registry = HookRegistry::new();
        let ctx = HookContext::stop();
        let action = registry.fire(&ctx).await;
        assert!(matches!(action, HookAction::Continue));
    }
}
