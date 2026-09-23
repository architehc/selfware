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
    ///
    /// `kind` distinguishes a hook that *decided* to block the tool from a
    /// hook that could not complete at all; both mean the tool must not run.
    Skip { reason: String, kind: SkipKind },
    /// An error occurred running a PostToolUse/Stop hook (logged but does not
    /// block). PreToolUse hooks never surface this from [`HookRegistry::fire`]:
    /// an incomplete policy check fails closed as `Skip { kind: HookFailure }`.
    Error { message: String },
}

/// Why a PreToolUse hook prevented a tool from running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    /// The hook ran to completion and exited non-zero: a policy decision.
    Policy,
    /// The hook could not complete (timed out, failed to start). The policy
    /// check never produced a verdict, so the tool is not run (fail closed),
    /// but this is an infrastructure failure, not a policy decision.
    HookFailure,
}

/// Build the tool-result text, audit reason and failure kind for a tool that
/// a PreToolUse hook prevented from running.
///
/// A [`SkipKind::Policy`] skip keeps the POLICY BLOCK wording (the hook exited
/// non-zero: a real decision). A [`SkipKind::HookFailure`] skip says plainly
/// that the hook could not complete — the tool was still not run (fail closed),
/// but the model must not be told a policy forbade the operation.
pub fn pre_tool_skip_message(
    tool_name: &str,
    reason: &str,
    kind: SkipKind,
) -> (String, String, &'static str) {
    match kind {
        SkipKind::Policy => (
            format!(
                "POLICY BLOCK: Tool '{}' was blocked by PreToolUse hook policy: {}. \
                 You MUST NOT attempt to bypass this policy using shell_exec or alternative tools.",
                tool_name, reason
            ),
            format!("PreToolUse hook: {}", reason),
            "hook_policy",
        ),
        SkipKind::HookFailure => (
            format!(
                "Tool '{}' was not run: its PreToolUse policy hook could not complete ({}). \
                 This is an infrastructure failure of the hook, not a policy decision.",
                tool_name, reason
            ),
            format!("PreToolUse hook could not complete: {}", reason),
            "hook_failure",
        ),
    }
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
    /// Returns the combined action: Continue, or the first Skip. For PreToolUse,
    /// a hook that cannot complete fails closed (`Skip { kind: HookFailure }`).
    ///
    /// Hooks run in the [`crate::tools::workspace_root::current`] root; callers
    /// that own an explicit root (the agent) should use [`Self::fire_in_root`]
    /// so hooks follow an entered worktree even outside a root scope.
    pub async fn fire(&self, ctx: &HookContext) -> HookAction {
        self.fire_in_root(ctx, &crate::tools::workspace_root::current())
            .await
    }

    /// [`Self::fire`], running every hook command in `root`.
    pub async fn fire_in_root(
        &self,
        ctx: &HookContext,
        root: &crate::tools::workspace_root::WorkspaceRoot,
    ) -> HookAction {
        let matching = self.matching(ctx);

        if matching.is_empty() {
            return HookAction::Continue;
        }

        debug!(
            "Firing {} hook(s) for event {:?}",
            matching.len(),
            ctx.event
        );

        for hook in matching {
            let result = shell_handler::execute_hook_in_root(hook, ctx, root).await;
            match result {
                HookAction::Skip {
                    ref reason,
                    kind: SkipKind::Policy,
                } => {
                    info!("Hook requested skip: {}", reason);
                    return result;
                }
                HookAction::Skip {
                    ref reason,
                    kind: SkipKind::HookFailure,
                } => {
                    warn!(
                        "PreToolUse hook could not complete; failing closed: {}",
                        reason
                    );
                    return result;
                }
                HookAction::Error { ref message } if ctx.event == HookEvent::PreToolUse => {
                    // Defense in depth: a PreToolUse hook error means the policy
                    // check never completed. Never fail open.
                    warn!("PreToolUse hook error; failing closed: {}", message);
                    return HookAction::Skip {
                        reason: message.clone(),
                        kind: SkipKind::HookFailure,
                    };
                }
                HookAction::Error { ref message } => {
                    // Post/Stop hook errors are non-fatal.
                    warn!("Hook error (non-fatal): {}", message);
                }
                HookAction::Continue => {
                    debug!("Hook completed successfully: {}", hook.command);
                }
            }
        }

        HookAction::Continue
    }

    /// Hooks that fire for `ctx` (event matches, tool filter matches).
    fn matching(&self, ctx: &HookContext) -> Vec<&HookConfig> {
        self.hooks
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
            .collect()
    }

    /// Whether [`Self::fire`] would run at least one hook for `ctx`.
    pub fn matches_any(&self, ctx: &HookContext) -> bool {
        !self.matching(ctx).is_empty()
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
#[path = "../../tests/unit/hooks/mod_test.rs"]
mod tests;
