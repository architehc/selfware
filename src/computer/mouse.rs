//! Mouse control for desktop automation.
//!
//! Provides programmatic mouse movement, clicking, scrolling, and dragging.
//! On Linux, uses xdotool for actual input. When running under WSL2 without
//! xdotool, falls back to PowerShell with Win32 `mouse_event` and
//! `System.Windows.Forms.Cursor` via `powershell.exe`.
//! On other platforms, stubs with logging.

#[cfg(all(target_os = "linux", not(test)))]
use anyhow::Context;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
#[cfg(not(target_os = "macos"))]
use tracing::debug;
#[cfg(target_os = "macos")]
use tracing::{debug, warn};

use super::{ActionRateLimiter, MovementProfile};

/// Backend used for mouse control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseBackend {
    Xdotool,
    WindowsWsl,
}

impl MouseBackend {
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
            Self::Xdotool => "Mouse control available via xdotool",
            Self::WindowsWsl => "Mouse control available via Windows fallback (WSL)",
        }
    }
}

/// Detect the mouse backend once and cache it.
fn detect_backend() -> Option<MouseBackend> {
    static BACKEND: OnceLock<Option<MouseBackend>> = OnceLock::new();
    *BACKEND.get_or_init(|| {
        if xdotool_available() {
            Some(MouseBackend::Xdotool)
        } else if can_use_wsl_powershell() {
            Some(MouseBackend::WindowsWsl)
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

/// Return the available mouse backend (public for doctor checks).
#[allow(dead_code)]
pub(crate) fn available_backend() -> Option<MouseBackend> {
    detect_backend()
}

/// Mouse button types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl MouseButton {
    /// Return the xdotool button number.
    #[allow(dead_code)]
    fn xdotool_button(&self) -> u8 {
        match self {
            MouseButton::Left => 1,
            MouseButton::Right => 3,
            MouseButton::Middle => 2,
        }
    }
}

/// A 2D coordinate on the screen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Mouse controller with rate limiting and movement profiles.
pub struct MouseController {
    rate_limiter: ActionRateLimiter,
    movement_profile: MovementProfile,
}

/// Build xdotool args for mouse move.
#[allow(dead_code)]
fn build_mousemove_args(x: i32, y: i32) -> Vec<String> {
    vec!["mousemove".to_string(), x.to_string(), y.to_string()]
}

/// Build xdotool args for a click.
#[allow(dead_code)]
fn build_click_args(button: u8) -> Vec<String> {
    vec!["click".to_string(), button.to_string()]
}

/// Build xdotool args for a double-click.
#[allow(dead_code)]
fn build_double_click_args() -> Vec<String> {
    vec![
        "click".to_string(),
        "--repeat".to_string(),
        "2".to_string(),
        "1".to_string(),
    ]
}

/// Build xdotool args for a scroll action.
/// button 4 = scroll up, button 5 = scroll down.
/// Repeats `amount` times for the given direction.
#[allow(dead_code)]
fn build_scroll_args(delta_x: i32, delta_y: i32) -> Vec<Vec<String>> {
    let mut commands = Vec::new();

    // Vertical scroll: negative delta_y = scroll up (button 4), positive = scroll down (button 5)
    if delta_y != 0 {
        let button = if delta_y < 0 { "4" } else { "5" };
        let count = delta_y.unsigned_abs();
        for _ in 0..count {
            commands.push(vec!["click".to_string(), button.to_string()]);
        }
    }

    // Horizontal scroll: positive delta_x = scroll right (button 7), negative = scroll left (button 6)
    if delta_x != 0 {
        let button = if delta_x < 0 { "6" } else { "7" };
        let count = delta_x.unsigned_abs();
        for _ in 0..count {
            commands.push(vec!["click".to_string(), button.to_string()]);
        }
    }

    commands
}

/// Build xdotool args for a drag operation.
#[allow(dead_code)]
fn build_drag_args(from: Point, to: Point, button: u8) -> Vec<String> {
    vec![
        "mousemove".to_string(),
        from.x.to_string(),
        from.y.to_string(),
        "mousedown".to_string(),
        button.to_string(),
        "mousemove".to_string(),
        to.x.to_string(),
        to.y.to_string(),
        "mouseup".to_string(),
        button.to_string(),
    ]
}

// ---------------------------------------------------------------------------
// PowerShell mouse helpers (WSL fallback)
// ---------------------------------------------------------------------------

/// PowerShell script for mouse_event via Win32 P/Invoke.
/// We `Add-Type` once with the necessary signatures.
#[allow(dead_code)]
const POWERSHELL_MOUSE_PREAMBLE: &str = r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinMouse {
    [DllImport("user32.dll")]
    public static extern void mouse_event(uint dwFlags, int dx, int dy, int dwData, IntPtr dwExtraInfo);
    public const uint MOUSEEVENTF_LEFTDOWN   = 0x0002;
    public const uint MOUSEEVENTF_LEFTUP     = 0x0004;
    public const uint MOUSEEVENTF_RIGHTDOWN  = 0x0008;
    public const uint MOUSEEVENTF_RIGHTUP    = 0x0010;
    public const uint MOUSEEVENTF_MIDDLEDOWN = 0x0020;
    public const uint MOUSEEVENTF_MIDDLEUP   = 0x0040;
    public const uint MOUSEEVENTF_WHEEL      = 0x0800;
    public const uint MOUSEEVENTF_HWHEEL     = 0x1000;
}
"@
"#;

/// Build a PowerShell script that moves the cursor to (x, y).
#[allow(dead_code)]
fn ps_move_to(x: i32, y: i32) -> String {
    format!(
        "{}\n[System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point({}, {})",
        POWERSHELL_MOUSE_PREAMBLE, x, y
    )
}

/// Build a PowerShell script for a mouse button click at the current position.
#[allow(dead_code)]
fn ps_click(button: &MouseButton) -> String {
    let (down, up) = match button {
        MouseButton::Left => (
            "WinMouse::MOUSEEVENTF_LEFTDOWN",
            "WinMouse::MOUSEEVENTF_LEFTUP",
        ),
        MouseButton::Right => (
            "WinMouse::MOUSEEVENTF_RIGHTDOWN",
            "WinMouse::MOUSEEVENTF_RIGHTUP",
        ),
        MouseButton::Middle => (
            "WinMouse::MOUSEEVENTF_MIDDLEDOWN",
            "WinMouse::MOUSEEVENTF_MIDDLEUP",
        ),
    };
    format!(
        "{preamble}\n\
         [WinMouse]::mouse_event([WinMouse]::{down}, 0, 0, 0, [IntPtr]::Zero)\n\
         [WinMouse]::mouse_event([WinMouse]::{up}, 0, 0, 0, [IntPtr]::Zero)",
        preamble = POWERSHELL_MOUSE_PREAMBLE,
        down = down.strip_prefix("WinMouse::").unwrap_or(down),
        up = up.strip_prefix("WinMouse::").unwrap_or(up),
    )
}

/// Build a PowerShell script for a double-click (left button).
#[allow(dead_code)]
fn ps_double_click() -> String {
    format!(
        "{}\n\
         [WinMouse]::mouse_event([WinMouse]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [IntPtr]::Zero)\n\
         [WinMouse]::mouse_event([WinMouse]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [IntPtr]::Zero)\n\
         Start-Sleep -Milliseconds 50\n\
         [WinMouse]::mouse_event([WinMouse]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [IntPtr]::Zero)\n\
         [WinMouse]::mouse_event([WinMouse]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [IntPtr]::Zero)",
        POWERSHELL_MOUSE_PREAMBLE
    )
}

/// Build a PowerShell script for mouse wheel scrolling.
#[allow(dead_code)]
fn ps_scroll(delta_x: i32, delta_y: i32) -> String {
    let mut script = String::new();
    script.push_str(POWERSHELL_MOUSE_PREAMBLE);
    script.push('\n');

    if delta_y != 0 {
        let dw_data = delta_y * -120;
        script.push_str(&format!(
            "[WinMouse]::mouse_event(0x0800, 0, 0, {}, [IntPtr]::Zero)\n",
            dw_data
        ));
    }

    if delta_x != 0 {
        let dw_data = delta_x * 120;
        script.push_str(&format!(
            "[WinMouse]::mouse_event(0x1000, 0, 0, {}, [IntPtr]::Zero)\n",
            dw_data
        ));
    }

    script
}

/// Build a PowerShell script for a click-and-drag operation.
#[allow(dead_code)]
fn ps_drag(from: Point, to: Point, button: &MouseButton) -> String {
    let (down, up) = match button {
        MouseButton::Left => ("MOUSEEVENTF_LEFTDOWN", "MOUSEEVENTF_LEFTUP"),
        MouseButton::Right => ("MOUSEEVENTF_RIGHTDOWN", "MOUSEEVENTF_RIGHTUP"),
        MouseButton::Middle => ("MOUSEEVENTF_MIDDLEDOWN", "MOUSEEVENTF_MIDDLEUP"),
    };
    format!(
        "{preamble}\n\
         [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point({fx}, {fy})\n\
         Start-Sleep -Milliseconds 50\n\
         [WinMouse]::mouse_event([WinMouse]::{down}, 0, 0, 0, [IntPtr]::Zero)\n\
         Start-Sleep -Milliseconds 50\n\
         [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point({tx}, {ty})\n\
         Start-Sleep -Milliseconds 50\n\
         [WinMouse]::mouse_event([WinMouse]::{up}, 0, 0, 0, [IntPtr]::Zero)",
        preamble = POWERSHELL_MOUSE_PREAMBLE,
        fx = from.x,
        fy = from.y,
        tx = to.x,
        ty = to.y,
        down = down,
        up = up,
    )
}

/// Run a PowerShell script via powershell.exe.
#[cfg(all(target_os = "linux", not(test)))]
async fn run_powershell_script(script: &str) -> Result<()> {
    use tokio::process::Command;

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", script])
        .output()
        .await
        .context("Failed to execute powershell.exe for mouse control")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "PowerShell mouse command failed (exit {}): {}",
            output.status,
            stderr.trim()
        );
    }

    Ok(())
}

/// No-op PowerShell stub for tests.
#[cfg(all(target_os = "linux", test))]
async fn run_powershell_script(_script: &str) -> Result<()> {
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

impl MouseController {
    pub fn new() -> Self {
        Self {
            rate_limiter: ActionRateLimiter::default(),
            movement_profile: MovementProfile::default(),
        }
    }

    pub fn with_movement_profile(mut self, profile: MovementProfile) -> Self {
        self.movement_profile = profile;
        self
    }

    /// Move mouse to absolute screen coordinates.
    pub async fn move_to(&self, x: i32, y: i32) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        self.validate_coordinates(x, y)?;

        debug!("Mouse move to ({}, {})", x, y);

        #[cfg(target_os = "linux")]
        {
            match detect_backend() {
                Some(MouseBackend::Xdotool) | None => {
                    let args = build_mousemove_args(x, y);
                    run_xdotool(&args).await?;
                }
                Some(MouseBackend::WindowsWsl) => {
                    let script = ps_move_to(x, y);
                    run_powershell_script(&script).await?;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Placeholder for macOS — would use CoreGraphics CGEventCreateMouseEvent
            warn!(
                "Mouse move to ({}, {}) — macOS not yet implemented, requires Accessibility permissions",
                x, y
            );
        }

        Ok(())
    }

    /// Click at current position.
    pub async fn click(&self, button: MouseButton) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        debug!("Mouse click: {:?}", button);

        #[cfg(target_os = "linux")]
        {
            match detect_backend() {
                Some(MouseBackend::Xdotool) | None => {
                    let args = build_click_args(button.xdotool_button());
                    run_xdotool(&args).await?;
                }
                Some(MouseBackend::WindowsWsl) => {
                    let script = ps_click(&button);
                    run_powershell_script(&script).await?;
                }
            }
        }

        Ok(())
    }

    /// Double-click at current position.
    pub async fn double_click(&self) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        debug!("Mouse double click");

        #[cfg(target_os = "linux")]
        {
            match detect_backend() {
                Some(MouseBackend::Xdotool) | None => {
                    let args = build_double_click_args();
                    run_xdotool(&args).await?;
                }
                Some(MouseBackend::WindowsWsl) => {
                    let script = ps_double_click();
                    run_powershell_script(&script).await?;
                }
            }
        }

        Ok(())
    }

    /// Click at specific coordinates.
    pub async fn click_at(&self, x: i32, y: i32, button: MouseButton) -> Result<()> {
        self.move_to(x, y).await?;
        self.click(button).await
    }

    /// Scroll the mouse wheel.
    pub async fn scroll(&self, delta_x: i32, delta_y: i32) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        debug!("Mouse scroll: dx={}, dy={}", delta_x, delta_y);

        #[cfg(target_os = "linux")]
        {
            match detect_backend() {
                Some(MouseBackend::Xdotool) | None => {
                    let commands = build_scroll_args(delta_x, delta_y);
                    for args in &commands {
                        run_xdotool(args).await?;
                    }
                }
                Some(MouseBackend::WindowsWsl) => {
                    let script = ps_scroll(delta_x, delta_y);
                    run_powershell_script(&script).await?;
                }
            }
        }

        Ok(())
    }

    /// Drag from one point to another.
    pub async fn drag(&self, from: Point, to: Point, button: MouseButton) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        self.validate_coordinates(from.x, from.y)?;
        self.validate_coordinates(to.x, to.y)?;
        debug!(
            "Mouse drag from ({}, {}) to ({}, {})",
            from.x, from.y, to.x, to.y
        );

        #[cfg(target_os = "linux")]
        {
            match detect_backend() {
                Some(MouseBackend::Xdotool) | None => {
                    let args = build_drag_args(from, to, button.xdotool_button());
                    run_xdotool(&args).await?;
                }
                Some(MouseBackend::WindowsWsl) => {
                    let script = ps_drag(from, to, &button);
                    run_powershell_script(&script).await?;
                }
            }
        }

        Ok(())
    }

    /// Validate coordinates are within reasonable screen bounds.
    fn validate_coordinates(&self, x: i32, y: i32) -> Result<()> {
        // Allow negative coordinates (multi-monitor setups) but cap at reasonable bounds
        if x.abs() > 32768 || y.abs() > 32768 {
            bail!(
                "Mouse coordinates ({}, {}) exceed maximum screen bounds",
                x,
                y
            );
        }
        Ok(())
    }
}

impl Default for MouseController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/computer/mouse/mouse_test.rs"]
mod tests;
