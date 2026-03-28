//! Keyboard control for desktop automation.
//!
//! Provides programmatic typing, key presses, and key combinations.
//! On Linux, uses xdotool for actual input. On other platforms, stubs with logging.

use anyhow::{bail, Result};
#[cfg(all(target_os = "linux", not(test)))]
use anyhow::Context;

use tracing::debug;

use super::{is_blocked_combo, ActionRateLimiter, TypingProfile};

/// Keyboard controller with rate limiting and typing profiles.
pub struct KeyboardController {
    rate_limiter: ActionRateLimiter,
    typing_profile: TypingProfile,
}

/// Map common key names to xdotool key names.
fn map_key_name(key: &str) -> &str {
    match key.to_lowercase().as_str() {
        "enter" | "return" => "Return",
        "tab" => "Tab",
        "escape" | "esc" => "Escape",
        "backspace" => "BackSpace",
        "delete" | "del" => "Delete",
        "space" => "space",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "pageup" | "page_up" => "Prior",
        "pagedown" | "page_down" => "Next",
        "insert" => "Insert",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        "shift" => "shift",
        "ctrl" | "control" => "ctrl",
        "alt" => "alt",
        "super" | "meta" | "cmd" | "command" => "super",
        _ => key,
    }
}

/// Build the xdotool key string for a combo like "ctrl+shift+t".
fn build_xdotool_combo(combo: &str) -> String {
    combo
        .split('+')
        .map(|part| map_key_name(part.trim()))
        .collect::<Vec<_>>()
        .join("+")
}

/// Build the command args for an xdotool invocation.
/// Returns (program, args) tuple for testability.
fn build_xdotool_type_cmd(text: &str) -> (&'static str, Vec<String>) {
    (
        "xdotool",
        vec![
            "type".to_string(),
            "--clearmodifiers".to_string(),
            "--".to_string(),
            text.to_string(),
        ],
    )
}

fn build_xdotool_key_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["key".to_string(), key.to_string()])
}

fn build_xdotool_keydown_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["keydown".to_string(), key.to_string()])
}

fn build_xdotool_keyup_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["keyup".to_string(), key.to_string()])
}

/// Execute an xdotool command. Returns error if xdotool is not available.
#[cfg(all(target_os = "linux", not(test)))]
async fn run_xdotool(args: &[String]) -> Result<()> {
    use tokio::process::Command;

    let output = Command::new("xdotool")
        .args(args)
        .output()
        .await
        .context("Failed to execute xdotool. Is xdotool installed? (apt install xdotool)")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("xdotool failed (exit {}): {}", output.status, stderr.trim());
    }

    Ok(())
}

/// No-op xdotool stub for tests (avoids requiring xdotool in CI).
#[cfg(all(target_os = "linux", test))]
async fn run_xdotool(_args: &[String]) -> Result<()> {
    Ok(())
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

        #[cfg(target_os = "linux")]
        {
            if text.is_empty() {
                return Ok(());
            }

            if self.typing_profile.base_delay_ms > 0 {
                // Human-like typing: type char by char with delays
                for ch in text.chars() {
                    let delay = self.typing_profile.base_delay_ms;
                    let char_str = ch.to_string();
                    let (_, args) = build_xdotool_type_cmd(&char_str);
                    run_xdotool(&args).await?;
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            } else {
                // Instant typing
                let (_, args) = build_xdotool_type_cmd(text);
                run_xdotool(&args).await?;
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            if self.typing_profile.base_delay_ms > 0 {
                for ch in text.chars() {
                    let delay = self.typing_profile.base_delay_ms;
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    debug!("Typed: '{}'", ch);
                }
            } else {
                debug!("Typed {} chars instantly (stub — no xdotool on this platform)", text.len());
            }
        }

        Ok(())
    }

    /// Press a single key.
    pub async fn press_key(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }

        let mapped = map_key_name(key);
        debug!("Key press: {} (mapped: {})", key, mapped);

        #[cfg(target_os = "linux")]
        {
            let (_, args) = build_xdotool_key_cmd(mapped);
            run_xdotool(&args).await?;
        }

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

        let xdotool_combo = build_xdotool_combo(combo);
        debug!("Key combo: {} (xdotool: {})", combo, xdotool_combo);

        #[cfg(target_os = "linux")]
        {
            let (_, args) = build_xdotool_key_cmd(&xdotool_combo);
            run_xdotool(&args).await?;
        }

        Ok(())
    }

    /// Press and hold a key.
    pub async fn key_down(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }

        let mapped = map_key_name(key);
        debug!("Key down: {} (mapped: {})", key, mapped);

        #[cfg(target_os = "linux")]
        {
            let (_, args) = build_xdotool_keydown_cmd(mapped);
            run_xdotool(&args).await?;
        }

        Ok(())
    }

    /// Release a held key.
    pub async fn key_up(&self, key: &str) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Keyboard action rate limit exceeded");
        }

        let mapped = map_key_name(key);
        debug!("Key up: {} (mapped: {})", key, mapped);

        #[cfg(target_os = "linux")]
        {
            let (_, args) = build_xdotool_keyup_cmd(mapped);
            run_xdotool(&args).await?;
        }

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
    async fn test_all_blocked_combos_rejected() {
        let kb = KeyboardController::new();
        for combo in &[
            "ctrl+alt+delete",
            "cmd+q",
            "alt+f4",
            "ctrl+alt+f1",
            "ctrl+alt+f2",
            "ctrl+alt+f3",
        ] {
            let result = kb.key_combo(combo).await;
            assert!(result.is_err(), "Expected '{}' to be blocked", combo);
        }
    }

    #[tokio::test]
    async fn test_safe_combo_allowed() {
        let kb = KeyboardController::new();
        assert!(kb.key_combo("ctrl+c").await.is_ok());
        assert!(kb.key_combo("ctrl+v").await.is_ok());
        assert!(kb.key_combo("ctrl+s").await.is_ok());
        assert!(kb.key_combo("ctrl+shift+t").await.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_success() {
        let kb = KeyboardController::new();
        assert!(kb.type_text("hello world").await.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_empty() {
        let kb = KeyboardController::new();
        assert!(kb.type_text("").await.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_at_limit() {
        let kb = KeyboardController::new();
        let text = "x".repeat(10_000);
        assert!(kb.type_text(&text).await.is_ok());
    }

    #[tokio::test]
    async fn test_type_text_length_limit() {
        let kb = KeyboardController::new();
        let long_text = "x".repeat(10_001);
        let result = kb.type_text(&long_text).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("10000"));
    }

    #[tokio::test]
    async fn test_press_key() {
        let kb = KeyboardController::new();
        assert!(kb.press_key("Enter").await.is_ok());
        assert!(kb.press_key("Tab").await.is_ok());
        assert!(kb.press_key("Escape").await.is_ok());
    }

    #[tokio::test]
    async fn test_key_down_up() {
        let kb = KeyboardController::new();
        assert!(kb.key_down("Shift").await.is_ok());
        assert!(kb.key_up("Shift").await.is_ok());
    }

    #[test]
    fn test_keyboard_controller_default() {
        let kb = KeyboardController::default();
        let _ = format!("{:?}", kb.typing_profile);
    }

    #[test]
    fn test_with_typing_profile() {
        let profile = TypingProfile {
            base_delay_ms: 50,
            variation_ms: 10,
        };
        let kb = KeyboardController::new().with_typing_profile(profile);
        assert_eq!(kb.typing_profile.base_delay_ms, 50);
    }

    #[tokio::test]
    async fn test_type_text_with_delay_profile() {
        let profile = TypingProfile {
            base_delay_ms: 1, // 1ms delay per char for fast test
            variation_ms: 0,
        };
        let kb = KeyboardController::new().with_typing_profile(profile);
        assert!(kb.type_text("hi").await.is_ok());
    }

    // ---- Command construction tests ----

    #[test]
    fn test_map_key_name_common() {
        assert_eq!(map_key_name("Enter"), "Return");
        assert_eq!(map_key_name("enter"), "Return");
        assert_eq!(map_key_name("Return"), "Return");
        assert_eq!(map_key_name("Tab"), "Tab");
        assert_eq!(map_key_name("Escape"), "Escape");
        assert_eq!(map_key_name("esc"), "Escape");
        assert_eq!(map_key_name("Backspace"), "BackSpace");
        assert_eq!(map_key_name("Delete"), "Delete");
        assert_eq!(map_key_name("space"), "space");
    }

    #[test]
    fn test_map_key_name_arrows() {
        assert_eq!(map_key_name("up"), "Up");
        assert_eq!(map_key_name("down"), "Down");
        assert_eq!(map_key_name("left"), "Left");
        assert_eq!(map_key_name("right"), "Right");
    }

    #[test]
    fn test_map_key_name_modifiers() {
        assert_eq!(map_key_name("ctrl"), "ctrl");
        assert_eq!(map_key_name("control"), "ctrl");
        assert_eq!(map_key_name("alt"), "alt");
        assert_eq!(map_key_name("shift"), "shift");
        assert_eq!(map_key_name("super"), "super");
        assert_eq!(map_key_name("meta"), "super");
        assert_eq!(map_key_name("cmd"), "super");
    }

    #[test]
    fn test_map_key_name_function_keys() {
        for i in 1..=12 {
            let key = format!("f{}", i);
            let expected = format!("F{}", i);
            assert_eq!(map_key_name(&key), expected);
        }
    }

    #[test]
    fn test_map_key_name_unknown_passthrough() {
        assert_eq!(map_key_name("a"), "a");
        assert_eq!(map_key_name("z"), "z");
        assert_eq!(map_key_name("SomeWeirdKey"), "SomeWeirdKey");
    }

    #[test]
    fn test_build_xdotool_type_cmd() {
        let (prog, args) = build_xdotool_type_cmd("hello world");
        assert_eq!(prog, "xdotool");
        assert_eq!(args, vec!["type", "--clearmodifiers", "--", "hello world"]);
    }

    #[test]
    fn test_build_xdotool_type_cmd_special_chars() {
        let (_, args) = build_xdotool_type_cmd("echo \"test\" && rm -rf /");
        assert_eq!(args[3], "echo \"test\" && rm -rf /");
    }

    #[test]
    fn test_build_xdotool_key_cmd() {
        let (prog, args) = build_xdotool_key_cmd("Return");
        assert_eq!(prog, "xdotool");
        assert_eq!(args, vec!["key", "Return"]);
    }

    #[test]
    fn test_build_xdotool_keydown_cmd() {
        let (prog, args) = build_xdotool_keydown_cmd("shift");
        assert_eq!(prog, "xdotool");
        assert_eq!(args, vec!["keydown", "shift"]);
    }

    #[test]
    fn test_build_xdotool_keyup_cmd() {
        let (prog, args) = build_xdotool_keyup_cmd("shift");
        assert_eq!(prog, "xdotool");
        assert_eq!(args, vec!["keyup", "shift"]);
    }

    #[test]
    fn test_build_xdotool_combo() {
        assert_eq!(build_xdotool_combo("ctrl+c"), "ctrl+c");
        assert_eq!(build_xdotool_combo("ctrl+shift+t"), "ctrl+shift+t");
        assert_eq!(build_xdotool_combo("ctrl+v"), "ctrl+v");
    }

    #[test]
    fn test_build_xdotool_combo_maps_keys() {
        assert_eq!(build_xdotool_combo("ctrl+Enter"), "ctrl+Return");
        assert_eq!(build_xdotool_combo("alt+Backspace"), "alt+BackSpace");
        assert_eq!(build_xdotool_combo("cmd+shift+Escape"), "super+shift+Escape");
    }

    #[test]
    fn test_build_xdotool_combo_with_spaces() {
        assert_eq!(build_xdotool_combo("ctrl + c"), "ctrl+c");
        assert_eq!(build_xdotool_combo("ctrl + shift + t"), "ctrl+shift+t");
    }
}
