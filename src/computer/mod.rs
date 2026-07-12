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
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_first_action() {
        let limiter = ActionRateLimiter::new(5);
        assert!(limiter.check());
    }

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = ActionRateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.check());
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let limiter = ActionRateLimiter::new(2);
        assert!(limiter.check()); // 1st
        assert!(limiter.check()); // 2nd
        assert!(!limiter.check()); // 3rd should fail
    }

    #[test]
    fn test_rate_limiter_default() {
        let limiter = ActionRateLimiter::default();
        assert_eq!(limiter.max_actions_per_sec, 10);
        // First action should always pass
        assert!(limiter.check());
    }

    #[test]
    fn rate_limiter_caps_concurrent_burst() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;
        // Many threads hammering a fresh limiter within one ~1s window must not
        // let a burst through: the CAS window-claim prevents multiple threads
        // from each resetting the counter (the old Relaxed load/store race).
        let limiter = Arc::new(ActionRateLimiter::new(10));
        let allowed = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let l = Arc::clone(&limiter);
            let a = Arc::clone(&allowed);
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    if l.check() {
                        a.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let total = allowed.load(Ordering::Relaxed);
        // 400 attempts complete in microseconds (~1 window). Allow 2 windows of
        // slack for a rare boundary crossing; without the fix this blew far past.
        assert!(
            (1..=20).contains(&total),
            "concurrent burst must be capped near the limit, got {total}"
        );
    }

    #[test]
    fn test_blocked_combos() {
        assert!(is_blocked_combo("ctrl+alt+delete"));
        assert!(is_blocked_combo("Ctrl+Alt+Delete"));
        assert!(is_blocked_combo("cmd+q"));
        assert!(is_blocked_combo("alt+f4"));
        assert!(is_blocked_combo("ctrl+alt+f1"));
        assert!(is_blocked_combo("ctrl+alt+f2"));
        assert!(is_blocked_combo("ctrl+alt+f3"));
        assert!(!is_blocked_combo("ctrl+c"));
        assert!(!is_blocked_combo("ctrl+v"));
        assert!(!is_blocked_combo("ctrl+s"));
        assert!(!is_blocked_combo("ctrl+shift+t"));
        assert!(!is_blocked_combo(""));
    }

    #[test]
    fn test_blocked_combos_whitespace_normalization() {
        // Spaces should be stripped
        assert!(is_blocked_combo("ctrl + alt + delete"));
        assert!(is_blocked_combo("cmd + q"));
    }

    #[test]
    fn test_movement_profile_default() {
        let profile = MovementProfile::default();
        assert!(matches!(profile, MovementProfile::Linear));
    }

    #[test]
    fn test_movement_profile_serde_roundtrip() {
        let profiles = vec![
            MovementProfile::Linear,
            MovementProfile::EaseInOut,
            MovementProfile::Bezier,
        ];
        for profile in profiles {
            let json = serde_json::to_string(&profile).unwrap();
            let parsed: MovementProfile = serde_json::from_str(&json).unwrap();
            let _ = format!("{:?}", parsed);
        }
    }

    #[test]
    fn test_typing_profile_default() {
        let profile = TypingProfile::default();
        assert_eq!(profile.base_delay_ms, 0);
        assert_eq!(profile.variation_ms, 0);
    }

    #[test]
    fn test_typing_profile_serde_roundtrip() {
        let profile = TypingProfile {
            base_delay_ms: 50,
            variation_ms: 10,
        };
        let json = serde_json::to_string(&profile).unwrap();
        let parsed: TypingProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_delay_ms, 50);
        assert_eq!(parsed.variation_ms, 10);
    }

    #[test]
    fn test_typing_profile_serde_defaults() {
        // Missing fields should default to 0
        let parsed: TypingProfile = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.base_delay_ms, 0);
        assert_eq!(parsed.variation_ms, 0);
    }
}
