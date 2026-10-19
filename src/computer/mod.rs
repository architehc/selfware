//! Computer control module for desktop automation.
//!
//! Provides mouse, keyboard, screen capture, and window management capabilities
//! enabling the agent to control any desktop application.
//!
//! ## Safety
//! - All actions are rate-limited (max 10 actions/second by default)
//! - Dangerous key combos are blocked by default
//! - First use per session requires explicit confirmation
//! - All actions are logged to the audit trail

#![allow(unused_imports)]

pub mod keyboard;
pub mod mouse;
pub mod screen;
pub mod window;
pub(crate) mod xdotool;

pub use keyboard::*;
pub use mouse::*;
pub use screen::*;
pub use window::*;

/// Display-session variables that computer-control child processes
/// (xdotool, wmctrl, xprop, osascript, …) legitimately need to reach the
/// user's display server. Every `Command` spawned by this module passes
/// this list to `sanitize_command_env_preserve` so the child gets a
/// working session without inheriting credentials (`SELFWARE_API_KEY`,
/// `AWS_*`, tokens). Never add credential-bearing names here.
pub(crate) const SESSION_ENV_VARS: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "XAUTHORITY",
];

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Rate limiter for computer control actions.
#[derive(Debug)]
pub struct ActionRateLimiter {
    /// Maximum actions per second.
    max_actions_per_sec: u32,
    /// Timestamp of last action (epoch millis).
    last_action_ms: AtomicU64,
    /// Count of actions in current second window.
    actions_in_window: AtomicU64,
}

impl ActionRateLimiter {
    pub fn new(max_actions_per_sec: u32) -> Self {
        Self {
            max_actions_per_sec,
            last_action_ms: AtomicU64::new(0),
            actions_in_window: AtomicU64::new(0),
        }
    }

    /// Check if an action is allowed. Returns `true` if within rate limit.
    pub fn check(&self) -> bool {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let window_ms = 1000; // 1 second window

        loop {
            let last = self.last_action_ms.load(Ordering::Acquire);
            // saturating_sub avoids underflow to a huge value on clock skew
            // (last > now), which would spuriously open a new window.
            if now_ms.saturating_sub(last) > window_ms {
                // Only the thread that wins this CAS opens the new window and
                // resets the counter; losers retry and count against it.
                if self
                    .last_action_ms
                    .compare_exchange(last, now_ms, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    self.actions_in_window.store(1, Ordering::Release);
                    return true;
                }
                continue;
            } else {
                let count = self.actions_in_window.fetch_add(1, Ordering::AcqRel) + 1;
                return count <= self.max_actions_per_sec as u64;
            }
        }
    }
}

impl Default for ActionRateLimiter {
    fn default() -> Self {
        Self::new(10) // 10 actions per second
    }
}

/// Movement profile for mouse movements.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementProfile {
    /// Instant movement (no animation).
    #[default]
    Linear,
    /// Smooth ease-in/ease-out curve.
    EaseInOut,
    /// Natural bezier curve with slight randomness.
    Bezier,
}

/// Typing profile for keyboard input.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TypingProfile {
    /// Base delay between keystrokes in milliseconds.
    #[serde(default)]
    pub base_delay_ms: u64,
    /// Random variation in delay (±ms).
    #[serde(default)]
    pub variation_ms: u64,
}

/// Blocked key combinations (dangerous system keys, with modifiers sorted alphabetically).
const BLOCKED_COMBOS: &[&str] = &[
    "alt+ctrl+delete",
    "cmd+q",       // Force quit on macOS
    "alt+f4",      // Close window on Windows/Linux
    "alt+ctrl+f1", // Switch to TTY on Linux
    "alt+ctrl+f2",
    "alt+ctrl+f3",
    "alt+ctrl+f4",
    "alt+ctrl+f5",
    "alt+ctrl+f6",
    "alt+ctrl+f7",
    "alt+ctrl+f8",
    "alt+ctrl+f9",
    "alt+ctrl+f10",
    "alt+ctrl+f11",
    "alt+ctrl+f12",
];

fn normalize_key_token(token: &str) -> String {
    match token.trim().to_lowercase().as_str() {
        "control" | "ctl" => "ctrl".to_string(),
        "del" => "delete".to_string(),
        "command" | "super" | "meta" | "win" => "cmd".to_string(),
        "option" | "opt" => "alt".to_string(),
        "return" => "enter".to_string(),
        other => other.to_string(),
    }
}

/// Check if a key combination is blocked for safety.
pub fn is_blocked_combo(combo: &str) -> bool {
    let mut parts: Vec<String> = combo
        .split('+')
        .map(normalize_key_token)
        .filter(|t| !t.is_empty())
        .collect();
    if parts.len() < 2 {
        return false;
    }
    let last_idx = parts.len() - 1;
    parts[..last_idx].sort();
    let normalized = parts.join("+");
    BLOCKED_COMBOS.contains(&normalized.as_str())
}

#[cfg(test)]
#[path = "../../tests/unit/computer/mod_test.rs"]
mod tests;
