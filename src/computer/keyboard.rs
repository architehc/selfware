//! Keyboard control for desktop automation.
//!
//! Provides programmatic typing, key presses, and key combinations.
//! On Linux, uses xdotool for actual input. When running under WSL2 without
//! xdotool, falls back to PowerShell `SendKeys` via `powershell.exe`.
//! On macOS, typing goes through `osascript` (System Events); key presses and
//! combos return an explicit "not supported" error rather than silently
//! succeeding without performing the action.

#[cfg(any(all(target_os = "linux", not(test)), target_os = "macos"))]
use anyhow::Context;
use anyhow::{bail, Result};

use std::sync::OnceLock;
use tracing::debug;
#[cfg(target_os = "linux")]
use tracing::warn;

#[cfg(target_os = "linux")]
use super::xdotool::run_xdotool;
use super::xdotool::{can_use_wsl_powershell, xdotool_available};
use super::{is_blocked_combo, ActionRateLimiter, TypingProfile};

/// Backend used for keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyboardBackend {
    Xdotool,
    WindowsWsl,
}

impl KeyboardBackend {
    #[allow(dead_code)]
    pub(crate) fn doctor_name(self) -> &'static str {
        match self {
            Self::Xdotool => "xdotool",
            Self::WindowsWsl => "windows_wsl",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn doctor_message(self) -> &'static str {
        match self {
            Self::Xdotool => "Keyboard control available via xdotool",
            Self::WindowsWsl => "Keyboard control available via Windows fallback (WSL)",
        }
    }
}

/// Detect the keyboard backend once and cache it.
fn detect_backend() -> Option<KeyboardBackend> {
    static BACKEND: OnceLock<Option<KeyboardBackend>> = OnceLock::new();
    *BACKEND.get_or_init(|| {
        if xdotool_available() {
            Some(KeyboardBackend::Xdotool)
        } else if can_use_wsl_powershell() {
            Some(KeyboardBackend::WindowsWsl)
        } else {
            None
        }
    })
}

/// Return the available keyboard backend (public for doctor checks).
#[allow(dead_code)]
pub(crate) fn available_backend() -> Option<KeyboardBackend> {
    detect_backend()
}

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
#[allow(dead_code)]
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

#[allow(dead_code)]
fn build_xdotool_key_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["key".to_string(), key.to_string()])
}

#[allow(dead_code)]
fn build_xdotool_keydown_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["keydown".to_string(), key.to_string()])
}

#[allow(dead_code)]
fn build_xdotool_keyup_cmd(key: &str) -> (&'static str, Vec<String>) {
    ("xdotool", vec!["keyup".to_string(), key.to_string()])
}

/// Map a key name to the PowerShell `SendKeys` notation.
#[allow(dead_code)]
fn map_key_to_sendkeys(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "enter" | "return" => "{ENTER}".to_string(),
        "tab" => "{TAB}".to_string(),
        "escape" | "esc" => "{ESC}".to_string(),
        "backspace" => "{BACKSPACE}".to_string(),
        "delete" | "del" => "{DELETE}".to_string(),
        "space" => " ".to_string(),
        "up" => "{UP}".to_string(),
        "down" => "{DOWN}".to_string(),
        "left" => "{LEFT}".to_string(),
        "right" => "{RIGHT}".to_string(),
        "home" => "{HOME}".to_string(),
        "end" => "{END}".to_string(),
        "pageup" | "page_up" | "prior" => "{PGUP}".to_string(),
        "pagedown" | "page_down" | "next" => "{PGDN}".to_string(),
        "insert" => "{INSERT}".to_string(),
        "f1" => "{F1}".to_string(),
        "f2" => "{F2}".to_string(),
        "f3" => "{F3}".to_string(),
        "f4" => "{F4}".to_string(),
        "f5" => "{F5}".to_string(),
        "f6" => "{F6}".to_string(),
        "f7" => "{F7}".to_string(),
        "f8" => "{F8}".to_string(),
        "f9" => "{F9}".to_string(),
        "f10" => "{F10}".to_string(),
        "f11" => "{F11}".to_string(),
        "f12" => "{F12}".to_string(),
        // Single printable characters go through as-is but must be escaped
        // if they are special SendKeys characters.
        other => escape_sendkeys_char(other),
    }
}

/// Escape characters that have special meaning in SendKeys.
#[allow(dead_code)]
fn escape_sendkeys_char(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for ch in s.chars() {
        match ch {
            '+' | '^' | '%' | '~' | '(' | ')' | '{' | '}' | '[' | ']' => {
                out.push('{');
                out.push(ch);
                out.push('}');
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Escape text for SendKeys (batch of printable characters).
#[allow(dead_code)]
fn escape_sendkeys_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    for ch in text.chars() {
        match ch {
            '+' | '^' | '%' | '~' | '(' | ')' | '{' | '}' | '[' | ']' => {
                out.push('{');
                out.push(ch);
                out.push('}');
            }
            '\n' => out.push_str("{ENTER}"),
            '\t' => out.push_str("{TAB}"),
            _ => out.push(ch),
        }
    }
    out
}

/// Build a PowerShell SendKeys combo string from "ctrl+shift+t" style input.
/// SendKeys modifiers: ^ = Ctrl, % = Alt, + = Shift.
#[allow(dead_code)]
fn build_sendkeys_combo(combo: &str) -> String {
    let mut prefix = String::new();
    let mut key_part = String::new();

    for part in combo.split('+') {
        let trimmed = part.trim();
        match trimmed.to_lowercase().as_str() {
            "ctrl" | "control" => prefix.push('^'),
            "alt" => prefix.push('%'),
            "shift" => prefix.push('+'),
            "super" | "meta" | "cmd" | "command" => prefix.push('^'), // best-effort: map super to ctrl on Windows
            _ => key_part = map_key_to_sendkeys(trimmed),
        }
    }

    format!("{}{}", prefix, key_part)
}

/// Run a PowerShell SendKeys command via powershell.exe.
#[cfg(all(target_os = "linux", not(test)))]
async fn run_powershell_sendkeys(sendkeys_sequence: &str) -> Result<()> {
    use tokio::process::Command;

    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('{}')",
        sendkeys_sequence.replace('\'', "''")
    );

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .await
        .context("Failed to execute powershell.exe for keyboard input")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "PowerShell SendKeys failed (exit {}): {}",
            output.status,
            stderr.trim()
        );
    }

    Ok(())
}

/// No-op PowerShell stub for tests.
#[cfg(all(target_os = "linux", test))]
async fn run_powershell_sendkeys(_sendkeys_sequence: &str) -> Result<()> {
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

            let backend = detect_backend();

            if self.typing_profile.base_delay_ms > 0 {
                // Human-like typing: type char by char with delays
                for ch in text.chars() {
                    let delay = self.typing_profile.base_delay_ms;
                    let char_str = ch.to_string();
                    match backend {
                        Some(KeyboardBackend::Xdotool) | None => {
                            let (_, args) = build_xdotool_type_cmd(&char_str);
                            run_xdotool(&args).await?;
                        }
                        Some(KeyboardBackend::WindowsWsl) => {
                            let escaped = escape_sendkeys_text(&char_str);
                            run_powershell_sendkeys(&escaped).await?;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
            } else {
                // Instant typing
                match backend {
                    Some(KeyboardBackend::Xdotool) | None => {
                        let (_, args) = build_xdotool_type_cmd(text);
                        run_xdotool(&args).await?;
                    }
                    Some(KeyboardBackend::WindowsWsl) => {
                        let escaped = escape_sendkeys_text(text);
                        run_powershell_sendkeys(&escaped).await?;
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            if !text.is_empty() {
                let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
                let script = format!(
                    "tell application \"System Events\" to keystroke \"{}\"",
                    escaped
                );
                let output = tokio::process::Command::new("osascript")
                    .arg("-e")
                    .arg(&script)
                    .output()
                    .await
                    .context("failed to run osascript for macOS keyboard typing")?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    bail!(
                        "macOS keyboard typing via System Events failed (exit {}): {} — grant Accessibility permissions to the terminal app",
                        output.status,
                        stderr.trim()
                    );
                }
            }
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            if !text.is_empty() {
                if self.typing_profile.base_delay_ms > 0 {
                    for ch in text.chars() {
                        let delay = self.typing_profile.base_delay_ms;
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        debug!("Typed: '{}'", ch);
                    }
                }
                // Never silently succeed without typing anything.
                bail!("keyboard type_text is not supported on this platform");
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
            match detect_backend() {
                Some(KeyboardBackend::Xdotool) | None => {
                    let (_, args) = build_xdotool_key_cmd(mapped);
                    run_xdotool(&args).await?;
                }
                Some(KeyboardBackend::WindowsWsl) => {
                    let sendkeys = map_key_to_sendkeys(key);
                    run_powershell_sendkeys(&sendkeys).await?;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // No key-name → macOS key-code mapping is implemented; never
            // silently succeed without pressing anything.
            bail!(
                "keyboard press_key('{}') is not supported on macOS: no key-code backend implemented (requires Accessibility permissions)",
                key
            );
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            bail!("keyboard press_key is not supported on this platform");
        }

        #[cfg(target_os = "linux")]
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
            match detect_backend() {
                Some(KeyboardBackend::Xdotool) | None => {
                    let (_, args) = build_xdotool_key_cmd(&xdotool_combo);
                    run_xdotool(&args).await?;
                }
                Some(KeyboardBackend::WindowsWsl) => {
                    let sendkeys = build_sendkeys_combo(combo);
                    debug!("Key combo via SendKeys: {}", sendkeys);
                    run_powershell_sendkeys(&sendkeys).await?;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            bail!(
                "keyboard key_combo('{}') is not supported on macOS: no key-code backend implemented (requires Accessibility permissions)",
                combo
            );
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            bail!("keyboard key_combo is not supported on this platform");
        }

        #[cfg(target_os = "linux")]
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
            match detect_backend() {
                Some(KeyboardBackend::Xdotool) | None => {
                    let (_, args) = build_xdotool_keydown_cmd(mapped);
                    run_xdotool(&args).await?;
                }
                Some(KeyboardBackend::WindowsWsl) => {
                    // SendKeys does not support hold-down semantics; log a warning
                    // and send a single key press as best-effort.
                    warn!(
                        "key_down('{}') via WSL PowerShell has no hold semantics; \
                         sending a single key press instead",
                        key
                    );
                    let sendkeys = map_key_to_sendkeys(key);
                    run_powershell_sendkeys(&sendkeys).await?;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            bail!(
                "keyboard key_down('{}') is not supported on macOS: no key-code backend implemented (requires Accessibility permissions)",
                key
            );
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            bail!("keyboard key_down is not supported on this platform");
        }

        #[cfg(target_os = "linux")]
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
            match detect_backend() {
                Some(KeyboardBackend::Xdotool) | None => {
                    let (_, args) = build_xdotool_keyup_cmd(mapped);
                    run_xdotool(&args).await?;
                }
                Some(KeyboardBackend::WindowsWsl) => {
                    // SendKeys does not support key-up; this is a no-op on WSL.
                    warn!(
                        "key_up('{}') via WSL PowerShell is a no-op (SendKeys has no hold semantics)",
                        key
                    );
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            bail!(
                "keyboard key_up('{}') is not supported on macOS: no key-code backend implemented (requires Accessibility permissions)",
                key
            );
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            bail!("keyboard key_up is not supported on this platform");
        }

        #[cfg(target_os = "linux")]
        Ok(())
    }
}

impl Default for KeyboardController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/computer/keyboard/keyboard_test.rs"]
mod tests;
