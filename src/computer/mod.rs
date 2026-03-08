#![allow(dead_code, unused_imports, unused_variables)]
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

pub use keyboard::KeyboardController;
pub use mouse::MouseController;
pub use screen::ScreenCapture;
pub use window::WindowManager;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

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

        let last = self.last_action_ms.load(Ordering::Relaxed);
        let window_ms = 1000; // 1 second window

        if now_ms - last > window_ms {
            // New window
            self.last_action_ms.store(now_ms, Ordering::Relaxed);
            self.actions_in_window.store(1, Ordering::Relaxed);
            true
        } else {
            let count = self.actions_in_window.fetch_add(1, Ordering::Relaxed) + 1;
            count <= self.max_actions_per_sec as u64
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
    "cmd+q",        // Force quit on macOS
    "alt+f4",       // Close window on Windows/Linux
    "ctrl+alt+f1",  // Switch to TTY on Linux
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
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_first_action() {
        let limiter = ActionRateLimiter::new(5);
        assert!(limiter.check());
    }

    #[test]
    fn test_blocked_combos() {
        assert!(is_blocked_combo("ctrl+alt+delete"));
        assert!(is_blocked_combo("Ctrl+Alt+Delete"));
        assert!(is_blocked_combo("cmd+q"));
        assert!(!is_blocked_combo("ctrl+c"));
        assert!(!is_blocked_combo("ctrl+v"));
        assert!(!is_blocked_combo("ctrl+s"));
    }

    #[test]
    fn test_movement_profile_default() {
        let profile = MovementProfile::default();
        assert!(matches!(profile, MovementProfile::Linear));
    }

    #[test]
    fn test_typing_profile_default() {
        let profile = TypingProfile::default();
        assert_eq!(profile.base_delay_ms, 0);
    }
}
