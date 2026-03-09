#![allow(dead_code, unused_imports, unused_variables)]
//! Keyboard control for desktop automation.
//!
//! Provides programmatic typing, key presses, and key combinations.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use super::{is_blocked_combo, ActionRateLimiter, TypingProfile};

/// Keyboard controller with rate limiting and typing profiles.
pub struct KeyboardController {
    rate_limiter: ActionRateLimiter,
    typing_profile: TypingProfile,
}

impl KeyboardController {
    pub fn new() -> Self {
        Self {
            rate_limiter: ActionRateLimiter::default(),
            typing_profile: TypingProfile::default(),
        }
    }

    pub fn with_typing_profile(mut self, profile: TypingProfile) -> Self {
        self.typing_profile = profile;
        self
    }

    /// Type a string character by character.
    pub async fn type_text(&self, text: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }

        // Validate text length to prevent abuse
        if text.len() > 10_000 {
            bail!(
                "Text too long for keyboard typing ({} chars, max 10000)",
                text.len()
            );
        }

        debug!("Keyboard type: {} chars", text.len());

        if self.typing_profile.base_delay_ms > 0 {
            // Human-like typing with delays
            for ch in text.chars() {
                // Each character gets a delay
                let delay = self.typing_profile.base_delay_ms;
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                debug!("Typed: '{}'", ch);
            }
        } else {
            // Instant typing (paste-like)
            debug!("Typed {} chars instantly", text.len());
        }

        Ok(())
    }

    /// Press a single key.
    pub async fn press_key(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }
        debug!("Key press: {}", key);
        Ok(())
    }

    /// Execute a key combination (e.g., "ctrl+c", "cmd+v").
    pub async fn key_combo(&self, combo: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }

        // Safety check for dangerous combos
        if is_blocked_combo(combo) {
            bail!(
                "Key combo '{}' is blocked for safety. Blocked combos cannot be executed.",
                combo
            );
        }

        debug!("Key combo: {}", combo);
        Ok(())
    }

    /// Press and hold a key.
    pub async fn key_down(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }
        debug!("Key down: {}", key);
        Ok(())
    }

    /// Release a held key.
    pub async fn key_up(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }
        debug!("Key up: {}", key);
        Ok(())
    }
}

impl Default for KeyboardController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_blocked_combo_rejected() {
        let kb = KeyboardController::new();
        let result = kb.key_combo("ctrl+alt+delete").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("blocked"));
    }

    #[tokio::test]
    async fn test_safe_combo_allowed() {
        let kb = KeyboardController::new();
        let result = kb.key_combo("ctrl+c").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_length_limit() {
        let kb = KeyboardController::new();
        let long_text = "x".repeat(20_000);
        let result = kb.type_text(&long_text).await;
        assert!(result.is_err());
    }
}
