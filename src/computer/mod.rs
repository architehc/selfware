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

pub mod keyboard;
pub mod mouse;
pub mod screen;
pub mod window;
pub(crate) mod xdotool;

pub use keyboard::KeyboardController;
pub use mouse::MouseController;
pub use screen::ScreenCapture;
pub use window::WindowManager;

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
    "SSH_AUTH_SOCK",
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

/// Blocked key combinations (dangerous system keys).
const BLOCKED_COMBOS: &[&str] = &[
    "ctrl+alt+delete",
    "cmd+q",       // Force quit on macOS
    "alt+f4",      // Close window on Windows/Linux
    "ctrl+alt+f1", // Switch to TTY on Linux
    "ctrl+alt+f2",
    "ctrl+alt+f3",
];

/// Check if a key combination is blocked for safety.
pub fn is_blocked_combo(combo: &str) -> bool {
    let normalized = combo.to_lowercase().replace(' ', "");
    BLOCKED_COMBOS
        .iter()
        .any(|blocked| normalized == blocked.replace(' ', ""))
}

#[cfg(test)]
#[path = "../../tests/unit/computer/mod_test.rs"]
mod tests;
