//! Keyboard control for desktop automation.
//!
//! Provides programmatic typing, key presses, and key combinations.
//! On Linux, uses xdotool for actual input. When running under WSL2 without
//! xdotool, falls back to PowerShell `SendKeys` via `powershell.exe`.
//! On other platforms, stubs with logging.

#[cfg(all(target_os = "linux", not(test)))]
use anyhow::Context;
use anyhow::{bail, Result};

use std::sync::OnceLock;
use tracing::{debug, warn};

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

fn xdotool_available() -> bool {
    std::process::Command::new("which")
        .arg("xdotool")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn can_use_wsl_powershell() -> bool {
    is_wsl_environment()
        && std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Write-Output ok"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
}

fn is_wsl_environment() -> bool {
    std::env::var_os("WSL_INTEROP").is_some()
        || std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|r| r.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
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

/// Map a key name to the PowerShell `SendKeys` notation.
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

/// Execute an xdotool command. Returns error if xdotool is not available.
#[cfg(all(target_os = "linux", not(test)))]
async fn run_xdotool(args: &[String]) -> Result<()> {
    use tokio::process::Command;

    // If no input backend was detected, fail with a clear message instead of
    // shelling out to a missing xdotool binary (which produces a raw ENOENT).
    if detect_backend().is_none() {
        bail!(
            "no input backend available: install xdotool (Linux X11) or run \
             under a supported environment"
        );
    }

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

        #[cfg(not(target_os = "linux"))]
        {
            if self.typing_profile.base_delay_ms > 0 {
                for ch in text.chars() {
                    let delay = self.typing_profile.base_delay_ms;
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    debug!("Typed: '{}'", ch);
                }
            } else {
                debug!(
                    "Typed {} chars instantly (stub — no xdotool on this platform)",
                    text.len()
                );
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
        assert_eq!(
            build_xdotool_combo("cmd+shift+Escape"),
            "super+shift+Escape"
        );
    }

    #[test]
    fn test_build_xdotool_combo_with_spaces() {
        assert_eq!(build_xdotool_combo("ctrl + c"), "ctrl+c");
        assert_eq!(build_xdotool_combo("ctrl + shift + t"), "ctrl+shift+t");
    }

    // ---- SendKeys / PowerShell fallback tests ----

    #[test]
    fn test_map_key_to_sendkeys_common() {
        assert_eq!(map_key_to_sendkeys("Enter"), "{ENTER}");
        assert_eq!(map_key_to_sendkeys("return"), "{ENTER}");
        assert_eq!(map_key_to_sendkeys("Tab"), "{TAB}");
        assert_eq!(map_key_to_sendkeys("Escape"), "{ESC}");
        assert_eq!(map_key_to_sendkeys("Backspace"), "{BACKSPACE}");
        assert_eq!(map_key_to_sendkeys("Delete"), "{DELETE}");
        assert_eq!(map_key_to_sendkeys("space"), " ");
    }

    #[test]
    fn test_map_key_to_sendkeys_arrows() {
        assert_eq!(map_key_to_sendkeys("up"), "{UP}");
        assert_eq!(map_key_to_sendkeys("down"), "{DOWN}");
        assert_eq!(map_key_to_sendkeys("left"), "{LEFT}");
        assert_eq!(map_key_to_sendkeys("right"), "{RIGHT}");
    }

    #[test]
    fn test_map_key_to_sendkeys_navigation() {
        assert_eq!(map_key_to_sendkeys("home"), "{HOME}");
        assert_eq!(map_key_to_sendkeys("end"), "{END}");
        assert_eq!(map_key_to_sendkeys("pageup"), "{PGUP}");
        assert_eq!(map_key_to_sendkeys("pagedown"), "{PGDN}");
        assert_eq!(map_key_to_sendkeys("insert"), "{INSERT}");
    }

    #[test]
    fn test_map_key_to_sendkeys_function_keys() {
        for i in 1..=12 {
            let key = format!("f{}", i);
            let expected = format!("{{F{}}}", i);
            assert_eq!(map_key_to_sendkeys(&key), expected);
        }
    }

    #[test]
    fn test_map_key_to_sendkeys_passthrough() {
        assert_eq!(map_key_to_sendkeys("a"), "a");
        assert_eq!(map_key_to_sendkeys("z"), "z");
    }

    #[test]
    fn test_escape_sendkeys_char_special() {
        assert_eq!(escape_sendkeys_char("+"), "{+}");
        assert_eq!(escape_sendkeys_char("^"), "{^}");
        assert_eq!(escape_sendkeys_char("%"), "{%}");
        assert_eq!(escape_sendkeys_char("~"), "{~}");
        assert_eq!(escape_sendkeys_char("("), "{(}");
        assert_eq!(escape_sendkeys_char(")"), "{)}");
    }

    #[test]
    fn test_escape_sendkeys_text() {
        assert_eq!(escape_sendkeys_text("hello"), "hello");
        assert_eq!(escape_sendkeys_text("a+b"), "a{+}b");
        assert_eq!(escape_sendkeys_text("100%"), "100{%}");
        assert_eq!(escape_sendkeys_text("line1\nline2"), "line1{ENTER}line2");
        assert_eq!(escape_sendkeys_text("col1\tcol2"), "col1{TAB}col2");
    }

    #[test]
    fn test_build_sendkeys_combo() {
        assert_eq!(build_sendkeys_combo("ctrl+c"), "^c");
        assert_eq!(build_sendkeys_combo("ctrl+v"), "^v");
        assert_eq!(build_sendkeys_combo("alt+tab"), "%{TAB}");
        assert_eq!(build_sendkeys_combo("ctrl+shift+t"), "^+t");
        assert_eq!(build_sendkeys_combo("shift+Enter"), "+{ENTER}");
    }

    #[test]
    fn test_build_sendkeys_combo_with_spaces() {
        assert_eq!(build_sendkeys_combo("ctrl + c"), "^c");
        assert_eq!(build_sendkeys_combo("ctrl + shift + t"), "^+t");
    }
}
