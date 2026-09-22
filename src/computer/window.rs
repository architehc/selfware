//! Window management for desktop automation.
//!
//! Provides cross-platform window listing, focusing, resizing, and application control.
//!
//! # Linux Implementation
//!
//! Uses a hybrid approach supporting both X11 and Wayland:
//! - **X11**: Uses `wmctrl` and `xdotool` commands
//! - **Wayland**: Uses `wlr-randr` for wlroots compositors, with fallbacks
//! - Detects display server via `$XDG_SESSION_TYPE` environment variable
//!
//! Required tools (install via package manager):
//! - `wmctrl` - Window listing and focus control
//! - `xdotool` - Window manipulation and input simulation
//! - `xprop` - Window property queries (optional, for better app name detection)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
#[cfg(target_os = "linux")]
use tracing::warn;
use tracing::{debug, info};

use super::SESSION_ENV_VARS;
use crate::safety::process_env::sanitize_command_env_preserve;

// ==========================================================================
// AGENTS.md Rule 6 — window placement: stay inside the visible region, touch
// only your own windows.
//
// The X screen is 7680x2928 (GNOME X11, 200% scale). The visible area of the
// main display is device coords x 0..=7680, y 768..=2928; the small VGA
// monitor sits ABOVE it (x 3840..=4864, y 0..=768) and must never receive a
// placement. mutter doubles `wmctrl -e` position requests: request = target/2
// (sizes pass through 1:1). Only windows this session owns (sw-* study
// terminals) may be repositioned, and every placement must be sanity-checked
// after the fact with `wmctrl -lG` and fixed immediately if it lands outside.
// These helpers are pure so they can be unit-tested without touching real
// windows.
// ==========================================================================

/// Visible region of the main display, device coordinates (AGENTS.md Rule 6).
const MAIN_DISPLAY_X_MAX: i32 = 7680;
const MAIN_DISPLAY_Y_MIN: i32 = 768;
const MAIN_DISPLAY_Y_MAX: i32 = 2928;

/// Title prefix of the study terminals this session owns. Only these windows
/// may be repositioned.
const SESSION_WINDOW_TITLE_PREFIX: &str = "sw-";

/// mutter doubles `wmctrl -e` position requests: the value sent must be the
/// device target divided by two. Floor division is exact for even targets and
/// at most one device pixel short for odd ones — inside the sanity check's
/// tolerance, and never past the target.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn wmctrl_request_position(x: i32, y: i32) -> (i32, i32) {
    (x / 2, y / 2)
}

/// Rule 6 bounds check, applied to the `wmctrl -lG` listing AFTER a
/// placement: the window must land at y >= 768 and x + width <= 7680 (the
/// small VGA monitor at x 3840-4864, y 0-768 sits above the visible area, so
/// the y bound keeps placements clear of it entirely).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn placement_inside_visible_region(x: i32, y: i32, width: u32, height: u32) -> bool {
    x >= 0
        && x.saturating_add(width as i32) <= MAIN_DISPLAY_X_MAX
        && y >= MAIN_DISPLAY_Y_MIN
        && y.saturating_add(height as i32) <= MAIN_DISPLAY_Y_MAX
}

/// Clamp a placement target into the visible region, for the "fix it
/// immediately" correction step.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn clamp_to_visible_region(x: i32, y: i32, width: u32, height: u32) -> (i32, i32) {
    let x_max = (MAIN_DISPLAY_X_MAX - width as i32).max(0);
    let y_max = (MAIN_DISPLAY_Y_MAX - height as i32).max(MAIN_DISPLAY_Y_MIN);
    (x.clamp(0, x_max), y.clamp(MAIN_DISPLAY_Y_MIN, y_max))
}

/// Whether a window title belongs to this session (the sw-* study-terminals
/// convention). Only these windows may be repositioned (AGENTS.md Rule 6).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn session_owns_window(title: &str) -> bool {
    title.trim().starts_with(SESSION_WINDOW_TITLE_PREFIX)
}

/// Extract (x, y, width, height) for `id` from `wmctrl -lG` output — the
/// post-placement sanity check's data source. Lines are
/// `0xID desktop pid x y w h hostname title`; one line per window.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn window_geometry_from_wmctrl_lg(stdout: &[u8], id: u64) -> Option<(i32, i32, u32, u32)> {
    for line in String::from_utf8_lossy(stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        let Ok(wid) = u64::from_str_radix(parts[0].trim_start_matches("0x"), 16) else {
            continue;
        };
        if wid != id {
            continue;
        }
        // Malformed geometry on the relevant line means the check cannot
        // verify — `None` for this window, not a poison for the whole list.
        let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
            parts[3].parse(),
            parts[4].parse(),
            parts[5].parse(),
            parts[6].parse(),
        ) else {
            continue;
        };
        return Some((x, y, width, height));
    }
    None
}

#[cfg(target_os = "macos")]
use anyhow::Context;
#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};

/// Unique window identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// Information about a window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Platform-specific window ID.
    pub id: WindowId,
    /// Window title.
    pub title: String,
    /// Application/process name.
    pub app_name: String,
    /// Window position (x, y).
    pub x: i32,
    pub y: i32,
    /// Window size.
    pub width: u32,
    pub height: u32,
    /// Whether the window is currently focused.
    pub is_focused: bool,
    /// Whether the window is minimized.
    pub is_minimized: bool,
}

/// Display server type for Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    /// X11 display server
    X11,
    /// Wayland display server
    Wayland,
    /// Unknown or unsupported display server
    Unknown,
}

impl DisplayServer {
    /// Detect the current display server from environment variables.
    pub fn detect() -> Self {
        // Check XDG_SESSION_TYPE first
        match env::var("XDG_SESSION_TYPE").as_deref() {
            Ok("wayland") => return DisplayServer::Wayland,
            Ok("x11") => return DisplayServer::X11,
            _ => {}
        }

        // Fallback: Check if WAYLAND_DISPLAY is set
        if env::var("WAYLAND_DISPLAY").is_ok() {
            return DisplayServer::Wayland;
        }

        // Fallback: Check if DISPLAY is set (X11)
        if env::var("DISPLAY").is_ok() {
            return DisplayServer::X11;
        }

        DisplayServer::Unknown
    }

    /// Check if this display server is supported for window management.
    pub fn is_supported(&self) -> bool {
        matches!(self, DisplayServer::X11 | DisplayServer::Wayland)
    }
}

/// Window manager that uses the appropriate platform backend.
pub struct WindowManager {
    /// Detected display server (Linux only)
    #[cfg(target_os = "linux")]
    display_server: DisplayServer,
    /// Mapping from WindowId to app name for macOS focus operations
    #[cfg(target_os = "macos")]
    window_id_to_app: Arc<Mutex<HashMap<WindowId, String>>>,
}

impl WindowManager {
    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            let display_server = DisplayServer::detect();
            info!("Detected display server: {:?}", display_server);
            Self { display_server }
        }

        #[cfg(target_os = "macos")]
        {
            Self {
                window_id_to_app: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Self {}
        }
    }

    /// Get the detected display server (Linux only).
    #[cfg(target_os = "linux")]
    pub fn display_server(&self) -> DisplayServer {
        self.display_server
    }

    /// Check if window management is available on this platform.
    pub async fn is_available(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            if !self.display_server.is_supported() {
                return false;
            }
            // Check if required tools are available
            self.check_tool_available("wmctrl").await || self.check_tool_available("xdotool").await
        }

        #[cfg(target_os = "macos")]
        {
            self.check_tool_available("osascript").await
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            false
        }
    }

    async fn check_tool_available(&self, tool: &str) -> bool {
        let mut cmd = tokio::process::Command::new("which");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        cmd.arg(tool)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// List all visible windows.
    pub async fn list_windows(&self) -> Result<Vec<WindowInfo>> {
        #[cfg(target_os = "macos")]
        {
            self.list_windows_macos().await
        }
        #[cfg(target_os = "linux")]
        {
            self.list_windows_linux().await
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            Ok(Vec::new())
        }
    }

    /// Get the currently focused window.
    pub async fn get_active_window(&self) -> Result<WindowInfo> {
        #[cfg(target_os = "linux")]
        {
            self.get_active_window_linux().await
        }
        #[cfg(not(target_os = "linux"))]
        {
            let windows = self.list_windows().await?;
            windows
                .into_iter()
                .find(|w| w.is_focused)
                .ok_or_else(|| anyhow::anyhow!("No active window found"))
        }
    }

    /// Focus a window by ID.
    pub async fn focus_window(&self, id: &WindowId) -> Result<()> {
        debug!("Focusing window: {:?}", id);
        #[cfg(target_os = "linux")]
        {
            self.focus_window_linux(id).await?;
        }
        #[cfg(target_os = "macos")]
        {
            self.focus_window_macos(id).await?;
        }
        Ok(())
    }

    /// Resize a window by ID.
    ///
    /// # Platform Support
    /// - **Linux**: Fully implemented using `wmctrl` and `xdotool`
    /// - **macOS**: Not implemented (requires Accessibility permissions and
    ///   AppleScript or CoreGraphics API integration) — returns an error
    ///   instead of silently succeeding.
    pub async fn resize_window(&self, id: &WindowId, width: u32, height: u32) -> Result<()> {
        debug!("Resizing window {:?} to {}x{}", id, width, height);
        #[cfg(target_os = "linux")]
        {
            self.resize_window_linux(id, width, height).await?;
        }
        #[cfg(target_os = "macos")]
        {
            anyhow::bail!(
                "window resize_window({:?}, {}x{}) is not supported on macOS: requires Accessibility permissions and an AppleScript/CoreGraphics backend that is not implemented",
                id,
                width,
                height
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }

    /// Move a window to specific coordinates.
    ///
    /// # Platform Support
    /// - **Linux**: Fully implemented using `wmctrl` and `xdotool`
    /// - **macOS**: Not implemented (requires Accessibility permissions and
    ///   AppleScript or CoreGraphics API integration) — returns an error
    ///   instead of silently succeeding.
    pub async fn move_window(&self, id: &WindowId, x: i32, y: i32) -> Result<()> {
        debug!("Moving window {:?} to position {}, {}", id, x, y);
        #[cfg(target_os = "linux")]
        {
            self.move_window_linux(id, x, y).await?;
        }
        #[cfg(target_os = "macos")]
        {
            anyhow::bail!(
                "window move_window({:?}, {}, {}) is not supported on macOS: requires Accessibility permissions and an AppleScript/CoreGraphics backend that is not implemented",
                id,
                x,
                y
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }

    /// Minimize a window.
    ///
    /// # Platform Support
    /// - **Linux**: Fully implemented using `xdotool` and `wmctrl`
    /// - **macOS**: Not implemented (requires Accessibility permissions) —
    ///   returns an error instead of silently succeeding.
    pub async fn minimize_window(&self, id: &WindowId) -> Result<()> {
        debug!("Minimizing window {:?}", id);
        #[cfg(target_os = "linux")]
        {
            self.minimize_window_linux(id).await?;
        }
        #[cfg(target_os = "macos")]
        {
            anyhow::bail!(
                "window minimize_window({:?}) is not supported on macOS: requires Accessibility permissions and an AppleScript/CoreGraphics backend that is not implemented",
                id
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }

    /// Close a window.
    ///
    /// # Platform Support
    /// - **Linux**: Fully implemented using `wmctrl` and `xdotool`
    /// - **macOS**: Not implemented (requires Accessibility permissions) —
    ///   returns an error instead of silently succeeding.
    pub async fn close_window(&self, id: &WindowId) -> Result<()> {
        debug!("Closing window {:?}", id);
        #[cfg(target_os = "linux")]
        {
            self.close_window_linux(id).await?;
        }
        #[cfg(target_os = "macos")]
        {
            anyhow::bail!(
                "window close_window({:?}) is not supported on macOS: requires Accessibility permissions and an AppleScript/CoreGraphics backend that is not implemented",
                id
            );
        }
        #[cfg(not(target_os = "macos"))]
        {
            Ok(())
        }
    }

    /// Launch an application.
    pub async fn launch_app(&self, app_name: &str) -> Result<()> {
        info!("Launching application: {}", app_name);

        #[cfg(target_os = "macos")]
        {
            let mut cmd = tokio::process::Command::new("open");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            cmd.arg("-a")
                .arg(app_name)
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
        }

        #[cfg(target_os = "linux")]
        {
            // Try to find the application in PATH
            let mut cmd = tokio::process::Command::new("which");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let which_output = cmd.arg(app_name).output().await;

            let exists = matches!(which_output, Ok(ref o) if o.status.success());

            if !exists {
                // Try common desktop file locations
                let desktop_file = format!("/usr/share/applications/{}.desktop", app_name);
                if tokio::fs::try_exists(&desktop_file).await.unwrap_or(false) {
                    let mut cmd = tokio::process::Command::new("gtk-launch");
                    sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
                    cmd.arg(app_name)
                        .spawn()
                        .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
                    return Ok(());
                }

                anyhow::bail!("Application '{}' not found in PATH", app_name);
            }

            let mut cmd = tokio::process::Command::new(app_name);
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            cmd.spawn()
                .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
        }

        Ok(())
    }

    // ==================== Linux Implementations ====================

    #[cfg(target_os = "linux")]
    async fn list_windows_linux(&self) -> Result<Vec<WindowInfo>> {
        match self.display_server {
            DisplayServer::X11 => {
                // Try wmctrl first (has better position/size info)
                if let Ok(windows) = self.list_windows_wmctrl().await {
                    return Ok(windows);
                }
                // Fall back to xdotool
                if let Ok(windows) = self.list_windows_xdotool().await {
                    return Ok(windows);
                }
                warn!("Neither wmctrl nor xdotool available for window listing");
                Ok(Vec::new())
            }
            DisplayServer::Wayland => {
                // Wayland has limited window management support
                // Try wlr-randr for wlroots-based compositors
                warn!("Wayland window management is limited; trying available tools");
                if let Ok(windows) = self.list_windows_wmctrl().await {
                    // wmctrl might still work under XWayland
                    return Ok(windows);
                }
                Ok(Vec::new())
            }
            DisplayServer::Unknown => {
                warn!("Unknown display server, cannot list windows");
                Ok(Vec::new())
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn list_windows_wmctrl(&self) -> Result<Vec<WindowInfo>> {
        // Try with geometry info first (-G flag)
        let mut cmd = tokio::process::Command::new("wmctrl");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["-l", "-G", "-p"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("wmctrl not available: {}", e))?;

        if !output.status.success() {
            // Try without -G flag for older wmctrl versions
            let mut cmd = tokio::process::Command::new("wmctrl");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let output_no_geom = cmd
                .args(["-l", "-p"])
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("wmctrl not available: {}", e))?;

            if !output_no_geom.status.success() {
                anyhow::bail!("wmctrl failed with status {}", output_no_geom.status);
            }

            return self
                .parse_wmctrl_output_no_geom(&output_no_geom.stdout)
                .await;
        }

        self.parse_wmctrl_output_with_geom(&output.stdout).await
    }

    #[cfg(target_os = "linux")]
    async fn parse_wmctrl_output_with_geom(&self, stdout: &[u8]) -> Result<Vec<WindowInfo>> {
        let active_id = self.get_active_window_id_xdotool().await.ok();
        let stdout_str = String::from_utf8_lossy(stdout);
        let mut windows = Vec::new();

        for line in stdout_str.lines() {
            // Format with -G: 0x04600003  0 12345 100 200 800 600 hostname Window Title
            // Fields: id, desktop, pid, x, y, width, height, hostname, title...
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }

            let hex_id = parts[0].trim();
            let wid = u64::from_str_radix(hex_id.trim_start_matches("0x"), 16).unwrap_or(0);
            if wid == 0 {
                continue;
            }

            let x = parts[3].parse::<i32>().unwrap_or(0);
            let y = parts[4].parse::<i32>().unwrap_or(0);
            let width = parts[5].parse::<u32>().unwrap_or(0);
            let height = parts[6].parse::<u32>().unwrap_or(0);
            let hostname = parts[7];

            // Title is everything after hostname
            let title_start = line.find(hostname).unwrap_or(0) + hostname.len();
            let title = line[title_start..].trim().to_string();

            let is_focused = active_id.as_ref() == Some(&wid);

            // Try to get better app name using xprop
            let app_name = self.get_window_class_xprop(wid).await.unwrap_or_else(|_| {
                // Fall back to using hostname or parsing title
                hostname.to_string()
            });

            windows.push(WindowInfo {
                id: WindowId(wid),
                title,
                app_name,
                x,
                y,
                width,
                height,
                is_focused,
                is_minimized: false, // wmctrl doesn't provide this info
            });
        }

        Ok(windows)
    }

    #[cfg(target_os = "linux")]
    async fn parse_wmctrl_output_no_geom(&self, stdout: &[u8]) -> Result<Vec<WindowInfo>> {
        let active_id = self.get_active_window_id_xdotool().await.ok();
        let stdout_str = String::from_utf8_lossy(stdout);
        let mut windows = Vec::new();

        for line in stdout_str.lines() {
            // Format without -G: 0x04600003  0 12345 hostname Window Title
            // Fields: id, desktop, pid, hostname, title...
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }

            let hex_id = parts[0].trim();
            let wid = u64::from_str_radix(hex_id.trim_start_matches("0x"), 16).unwrap_or(0);
            if wid == 0 {
                continue;
            }

            let hostname = parts[3];

            // Title is everything after hostname
            let title_start = line.find(hostname).unwrap_or(0) + hostname.len();
            let title = line[title_start..].trim().to_string();

            let is_focused = active_id.as_ref() == Some(&wid);

            // Try to get geometry from xdotool
            let (x, y, width, height) = self
                .get_window_geometry_xdotool(wid)
                .await
                .unwrap_or((0, 0, 0, 0));

            // Try to get better app name using xprop
            let app_name = self
                .get_window_class_xprop(wid)
                .await
                .unwrap_or_else(|_| hostname.to_string());

            windows.push(WindowInfo {
                id: WindowId(wid),
                title,
                app_name,
                x,
                y,
                width,
                height,
                is_focused,
                is_minimized: false,
            });
        }

        Ok(windows)
    }

    #[cfg(target_os = "linux")]
    async fn get_window_class_xprop(&self, wid: u64) -> Result<String> {
        let mut cmd = tokio::process::Command::new("xprop");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["-id", &format!("0x{:x}", wid), "WM_CLASS"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xprop not available: {}", e))?;

        if !output.status.success() {
            anyhow::bail!("xprop failed");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // Parse: WM_CLASS(STRING) = "instance", "class"
        if let Some(eq_pos) = stdout.find(" = \"") {
            // Parse WM_CLASS(STRING) = "instance", "class"
            let rest = &stdout[eq_pos + 3..];
            let quoted: Vec<&str> = rest
                .split('"')
                .enumerate()
                .filter(|(i, _)| i % 2 == 1)
                .map(|(_, s)| s)
                .collect();
            if let Some(class) = quoted.get(1) {
                return Ok(class.to_string());
            } else if let Some(instance) = quoted.first() {
                return Ok(instance.to_string());
            }
        }

        anyhow::bail!("Could not parse WM_CLASS")
    }

    #[cfg(target_os = "linux")]
    async fn get_window_geometry_xdotool(&self, wid: u64) -> Result<(i32, i32, u32, u32)> {
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["getwindowgeometry", &wid.to_string()])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if !output.status.success() {
            anyhow::bail!("xdotool getwindowgeometry failed");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut x = 0i32;
        let mut y = 0i32;
        let mut width = 0u32;
        let mut height = 0u32;

        for line in stdout.lines() {
            if line.contains("Position:") {
                // Format: Position: 100,200 (screen: 0)
                if let Some(pos_start) = line.find("Position: ") {
                    let pos_str = &line[pos_start + 10..];
                    if let Some(comma_pos) = pos_str.find(',') {
                        if let Some(paren_pos) = pos_str.find('(') {
                            x = pos_str[..comma_pos].trim().parse().unwrap_or(0);
                            y = pos_str[comma_pos + 1..paren_pos]
                                .trim()
                                .parse()
                                .unwrap_or(0);
                        }
                    }
                }
            } else if line.contains("Geometry:") {
                // Format: Geometry: 800x600
                if let Some(geom_start) = line.find("Geometry: ") {
                    let geom_str = &line[geom_start + 10..];
                    if let Some(x_pos) = geom_str.find('x') {
                        width = geom_str[..x_pos].trim().parse().unwrap_or(0);
                        height = geom_str[x_pos + 1..].trim().parse().unwrap_or(0);
                    }
                }
            }
        }

        Ok((x, y, width, height))
    }

    #[cfg(target_os = "linux")]
    async fn list_windows_xdotool(&self) -> Result<Vec<WindowInfo>> {
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["search", "--onlyvisible", "--name", ""])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if !output.status.success() {
            anyhow::bail!("xdotool search failed");
        }

        let active_id = self.get_active_window_id_xdotool().await.ok();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut windows = Vec::new();

        for line in stdout.lines() {
            let wid: u64 = match line.trim().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let mut cmd = tokio::process::Command::new("xdotool");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let name_output = cmd.args(["getwindowname", &wid.to_string()]).output().await;

            let title = match name_output {
                Ok(out) if out.status.success() => {
                    String::from_utf8_lossy(&out.stdout).trim().to_string()
                }
                _ => String::new(),
            };

            // Skip windows with no title
            if title.is_empty() {
                continue;
            }

            let is_focused = active_id.as_ref() == Some(&wid);

            // Get geometry
            let (x, y, width, height) = self
                .get_window_geometry_xdotool(wid)
                .await
                .unwrap_or((0, 0, 0, 0));

            // Get app name
            let app_name = self
                .get_window_class_xprop(wid)
                .await
                .unwrap_or_else(|_| title.clone());

            windows.push(WindowInfo {
                id: WindowId(wid),
                title: title.clone(),
                app_name,
                x,
                y,
                width,
                height,
                is_focused,
                is_minimized: false,
            });
        }

        Ok(windows)
    }

    #[cfg(target_os = "linux")]
    async fn get_active_window_id_xdotool(&self) -> Result<u64> {
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .arg("getactivewindow")
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if !output.status.success() {
            anyhow::bail!("xdotool getactivewindow failed");
        }

        let id_str = String::from_utf8_lossy(&output.stdout);
        id_str
            .trim()
            .parse::<u64>()
            .map_err(|e| anyhow::anyhow!("Failed to parse window id: {}", e))
    }

    #[cfg(target_os = "linux")]
    async fn get_active_window_linux(&self) -> Result<WindowInfo> {
        let wid = self.get_active_window_id_xdotool().await?;

        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let name_output = cmd
            .args(["getwindowname", &wid.to_string()])
            .output()
            .await?;

        let title = if name_output.status.success() {
            String::from_utf8_lossy(&name_output.stdout)
                .trim()
                .to_string()
        } else {
            String::new()
        };

        let app_name = self
            .get_window_class_xprop(wid)
            .await
            .unwrap_or_else(|_| title.clone());

        let (x, y, width, height) = self
            .get_window_geometry_xdotool(wid)
            .await
            .unwrap_or((0, 0, 0, 0));

        Ok(WindowInfo {
            id: WindowId(wid),
            title,
            app_name,
            x,
            y,
            width,
            height,
            is_focused: true,
            is_minimized: false,
        })
    }

    #[cfg(target_os = "linux")]
    async fn focus_window_linux(&self, id: &WindowId) -> Result<()> {
        let id_str = format!("0x{:x}", id.0);

        // Try wmctrl first
        let mut cmd = tokio::process::Command::new("wmctrl");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let wmctrl_result = cmd.args(["-i", "-a", &id_str]).output().await;

        if let Ok(output) = &wmctrl_result {
            if output.status.success() {
                debug!("Focused window {} via wmctrl", id_str);
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("wmctrl focus failed: {}", stderr);
        }
        if let Err(e) = &wmctrl_result {
            debug!("wmctrl not available: {}", e);
        }

        // Fall back to xdotool
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        match cmd
            .args(["windowactivate", &id.0.to_string()])
            .output()
            .await
        {
            Ok(output) if output.status.success() => {
                debug!("Focused window {} via xdotool", id.0);
                Ok(())
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("xdotool windowactivate failed: {}", stderr);
            }
            Err(e) => {
                anyhow::bail!("Neither wmctrl nor xdotool succeeded: {}", e);
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn resize_window_linux(&self, id: &WindowId, width: u32, height: u32) -> Result<()> {
        // Try wmctrl first (uses resize/move)
        let mut cmd = tokio::process::Command::new("wmctrl");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args([
                "-i",
                "-r",
                &format!("0x{:x}", id.0),
                "-e",
                &format!("0,-1,-1,{}, {}", width, height),
            ])
            .output()
            .await;

        if let Ok(ref output) = result {
            if output.status.success() {
                debug!("Resized window {} to {}x{}", id.0, width, height);
                return Ok(());
            }
        }

        // Fall back to xdotool
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args([
                "windowsize",
                &id.0.to_string(),
                &width.to_string(),
                &height.to_string(),
            ])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if result.status.success() {
            debug!(
                "Resized window {} to {}x{} via xdotool",
                id.0, width, height
            );
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            anyhow::bail!("Failed to resize window: {}", stderr)
        }
    }

    #[cfg(target_os = "linux")]
    async fn move_window_linux(&self, id: &WindowId, x: i32, y: i32) -> Result<()> {
        // AGENTS.md Rule 6 ownership gate: only windows this session owns
        // (sw-* study terminals) may be repositioned. The list also proves
        // the window exists, so the post-placement check below has a real
        // target instead of verifying a ghost.
        let windows = self.list_windows_linux().await?;
        let Some(target) = windows.iter().find(|w| w.id == *id) else {
            anyhow::bail!(
                "Refusing to move window 0x{:x}: not in the current window list \
                 (AGENTS.md Rule 6 — only sw-* study terminals may be repositioned)",
                id.0
            );
        };
        if !session_owns_window(&target.title) {
            anyhow::bail!(
                "Refusing to move window 0x{:x} ('{}'): not a session-owned sw-* window \
                 (AGENTS.md Rule 6 — only your own windows may be repositioned)",
                id.0,
                target.title
            );
        }

        // Place, then verify against the `wmctrl -lG` listing and fix
        // immediately if the landing is outside the visible region. mutter
        // doubles `wmctrl -e` positions, so the request is the target / 2.
        let mut attempt: u32 = 0;
        let mut target_x = x;
        let mut target_y = y;
        loop {
            attempt += 1;
            let (req_x, req_y) = wmctrl_request_position(target_x, target_y);
            let mut cmd = tokio::process::Command::new("wmctrl");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let result = cmd
                .args([
                    "-i",
                    "-r",
                    &format!("0x{:x}", id.0),
                    "-e",
                    &format!("0,{}, {}, -1,-1", req_x, req_y),
                ])
                .output()
                .await;

            // The compositor refused the request: try the xdotool fallback.
            if !result.as_ref().is_ok_and(|o| o.status.success()) {
                warn!("Failed to move window {} via wmctrl", id.0);
                return self.move_window_xdotool(id, x, y).await;
            }

            match self.verify_wmctrl_placement(id).await {
                Ok(Some((lx, ly, lw, lh))) if placement_inside_visible_region(lx, ly, lw, lh) => {
                    debug!("Moved window {} to {}, {}", id.0, lx, ly);
                    return Ok(());
                }
                Ok(geometry) => {
                    // Out of bounds (or not listable). Fix immediately: clamp
                    // into the visible region and try once more; a second
                    // failure is reported loudly, never left as an off-screen
                    // window (the previous bug parked windows above the
                    // visible top edge by doubling unhalved requests).
                    if attempt >= 2 {
                        let where_at = geometry
                            .map(|(gx, gy, gw, gh)| {
                                format!("landed at device ({gx}, {gy}) {gw}x{gh}")
                            })
                            .unwrap_or_else(|| "no geometry in wmctrl -lG".to_string());
                        anyhow::bail!(
                            "window 0x{:x} did not land in the visible region \
                             (x 0-7680, y 768-2928) — {where_at}; refusing to leave it \
                             off-screen (AGENTS.md Rule 6)",
                            id.0
                        );
                    }
                    let (clamped_x, clamped_y) = match geometry {
                        Some((_, _, gw, gh)) => clamp_to_visible_region(target_x, target_y, gw, gh),
                        None => {
                            clamp_to_visible_region(target_x, target_y, target.width, target.height)
                        }
                    };
                    warn!(
                        "window 0x{:x} landed outside the visible region; correcting to \
                         ({}, {})",
                        id.0, clamped_x, clamped_y
                    );
                    target_x = clamped_x;
                    target_y = clamped_y;
                    continue;
                }
                Err(_) => {
                    // `wmctrl -lG` is unavailable/failed, so the placement
                    // cannot be verified. Fail closed per Rule 6 via the
                    // xdotool path, whose own sanity check bails loudly rather
                    // than accept an unverified placement.
                    warn!(
                        "wmctrl -lG verification failed after moving window {}",
                        id.0
                    );
                    return self.move_window_xdotool(id, x, y).await;
                }
            }
        }
    }

    /// Run `wmctrl -lG` and return the geometry of `id` — the Rule 6
    /// post-placement sanity check's data.
    #[cfg(target_os = "linux")]
    async fn verify_wmctrl_placement(&self, id: &WindowId) -> Result<Option<(i32, i32, u32, u32)>> {
        let mut cmd = tokio::process::Command::new("wmctrl");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["-l", "-G"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("wmctrl not available: {}", e))?;
        if !output.status.success() {
            anyhow::bail!("wmctrl -lG failed with status {}", output.status);
        }
        Ok(window_geometry_from_wmctrl_lg(&output.stdout, id.0))
    }

    /// xdotool fallback for a move: xdotool does NOT double positions (that
    /// is a wmctrl/mutter interaction), so the coordinates pass through
    /// directly. The same Rule 6 sanity check runs afterwards.
    #[cfg(target_os = "linux")]
    async fn move_window_xdotool(&self, id: &WindowId, x: i32, y: i32) -> Result<()> {
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args([
                "windowmove",
                &id.0.to_string(),
                &x.to_string(),
                &y.to_string(),
            ])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            anyhow::bail!("Failed to move window: {}", stderr)
        }
        // Rule 6 sanity check after placement by any means.
        let geometry = self.verify_wmctrl_placement(id).await?;
        match geometry {
            Some((lx, ly, lw, lh)) if placement_inside_visible_region(lx, ly, lw, lh) => {
                debug!("Moved window {} to {}, {} via xdotool", id.0, lx, ly);
                Ok(())
            }
            Some((lx, ly, lw, lh)) => anyhow::bail!(
                "window 0x{:x} landed at device ({lx}, {ly}) {lw}x{lh} — outside the visible \
                 region (AGENTS.md Rule 6); refusing to leave it off-screen",
                id.0
            ),
            None => anyhow::bail!(
                "window 0x{:x} not found in `wmctrl -lG` after the xdotool move — cannot \
                 verify the placement (AGENTS.md Rule 6)",
                id.0
            ),
        }
    }

    #[cfg(target_os = "linux")]
    async fn minimize_window_linux(&self, id: &WindowId) -> Result<()> {
        // xdotool is the best option for minimize
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args(["windowminimize", &id.0.to_string()])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if result.status.success() {
            debug!("Minimized window {}", id.0);
            Ok(())
        } else {
            // Try alternative: wmctrl with iconify
            let mut cmd = tokio::process::Command::new("wmctrl");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let result = cmd
                .args(["-i", "-r", &format!("0x{:x}", id.0), "-b", "add,hidden"])
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => {
                    debug!("Minimized window {} via wmctrl", id.0);
                    Ok(())
                }
                _ => {
                    let stderr = result
                        .as_ref()
                        .map(|o| String::from_utf8_lossy(&o.stderr).to_string())
                        .unwrap_or_default();
                    anyhow::bail!("Failed to minimize window: {}", stderr)
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    async fn close_window_linux(&self, id: &WindowId) -> Result<()> {
        // Try wmctrl first (graceful close)
        let mut cmd = tokio::process::Command::new("wmctrl");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args(["-i", "-c", &format!("0x{:x}", id.0)])
            .output()
            .await;

        match result {
            Ok(output) if output.status.success() => {
                debug!("Closed window {} via wmctrl", id.0);
                return Ok(());
            }
            _ => {}
        }

        // Fall back to xdotool
        let mut cmd = tokio::process::Command::new("xdotool");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let result = cmd
            .args(["windowclose", &id.0.to_string()])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if result.status.success() {
            debug!("Closed window {} via xdotool", id.0);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            anyhow::bail!("Failed to close window: {}", stderr)
        }
    }

    // ==================== macOS Implementations ====================

    #[cfg(target_os = "macos")]
    async fn list_windows_macos(&self) -> Result<Vec<WindowInfo>> {
        // Use osascript to list windows on macOS
        // First get visible processes
        let mut cmd = tokio::process::Command::new("osascript");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let output = cmd
            .args(["-e", "tell application \"System Events\" to get name of every process whose visible is true"])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Check for accessibility permissions error
            if stderr.contains("not allowed") || stderr.contains("assistive") {
                anyhow::bail!(
                    "Accessibility permissions required. \
                     Please enable System Settings > Privacy & Security > Accessibility for this application."
                );
            }
            anyhow::bail!("osascript failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let app_names: Vec<&str> = stdout
            .split(", ")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        // Get the frontmost process
        let mut cmd = tokio::process::Command::new("osascript");
        sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
        let frontmost_output = cmd
            .args(["-e", "tell application \"System Events\" to get name of first process whose frontmost is true"])
            .output()
            .await;

        let frontmost_app = match frontmost_output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => String::new(),
        };

        // Build the new id->app mapping locally so we don't hold the std::sync::Mutex
        // guard across the .await below (MutexGuard is !Send, which would make the
        // resulting future !Send and break callers like ToolRegistry that require
        // Send futures).
        let mut new_id_to_app: HashMap<WindowId, String> = HashMap::new();
        let mut windows = Vec::new();

        for (i, app_name) in app_names.iter().enumerate() {
            let window_id = WindowId(i as u64);
            new_id_to_app.insert(window_id.clone(), app_name.to_string());

            // Get window info for each app
            let mut cmd = tokio::process::Command::new("osascript");
            sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
            let window_output = cmd
                .args([
                    "-e",
                    &format!(
                        "tell application \"System Events\" to tell process \"{}\" to get {{position, size}} of window 1",
                        app_name
                    ),
                ])
                .output()
                .await;

            let (x, y, width, height) = match window_output {
                Ok(out) if out.status.success() => {
                    let text = String::from_utf8_lossy(&out.stdout);
                    // Parse: {100, 200}, {800, 600}
                    let parts: Vec<i32> = text
                        .trim()
                        .split(|c: char| c == '{' || c == '}' || c == ',' || c.is_whitespace())
                        .filter(|s| !s.is_empty())
                        .filter_map(|s| s.parse().ok())
                        .collect();

                    if parts.len() >= 4 {
                        (parts[0], parts[1], parts[2] as u32, parts[3] as u32)
                    } else {
                        (0, 0, 0, 0)
                    }
                }
                _ => (0, 0, 0, 0),
            };

            let is_focused = *app_name == frontmost_app;

            windows.push(WindowInfo {
                id: window_id,
                title: app_name.to_string(),
                app_name: app_name.to_string(),
                x,
                y,
                width,
                height,
                is_focused,
                is_minimized: false,
            });
        }

        // Now publish the new mapping; the lock is held only briefly and never
        // across an await.
        if let Ok(mut id_to_app) = self.window_id_to_app.lock() {
            *id_to_app = new_id_to_app;
        }

        Ok(windows)
    }

    #[cfg(target_os = "macos")]
    async fn focus_window_macos(&self, id: &WindowId) -> Result<()> {
        let app_name = self.resolve_macos_app_name(id).await?;
        let escaped_app_name = escape_applescript_string(&app_name);

        debug!("Focusing macOS window for app: {}", app_name);

        // First bring the owning app to the foreground, then raise its front window.
        let focus_script = format!(
            r#"tell application "{app}" to activate
tell application "System Events"
    tell process "{app}"
        set frontmost to true
        try
            perform action "AXRaise" of window 1
        end try
    end tell
end tell"#,
            app = escaped_app_name
        );

        run_macos_focus_script(&focus_script).await?;

        info!("Focused macOS window for app: {}", app_name);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn resolve_macos_app_name(&self, id: &WindowId) -> Result<String> {
        if let Some(app_name) = self.lookup_macos_app_name(id)? {
            return Ok(app_name);
        }

        // Refresh the cache once in case the caller is using a stale/new
        // manager instance. We swallow the error here on purpose: in headless
        // CI runners osascript fails because no display / Accessibility
        // permissions are available, and surfacing that low-level error
        // instead of "Unknown window ID" hides the real cause from callers.
        let _ = self.list_windows_macos().await;
        self.lookup_macos_app_name(id)?
            .context("Unknown window ID. Call list_windows() first to refresh the window list.")
    }

    #[cfg(target_os = "macos")]
    fn lookup_macos_app_name(&self, id: &WindowId) -> Result<Option<String>> {
        let id_to_app = self
            .window_id_to_app
            .lock()
            .map_err(|e| anyhow::anyhow!("macOS window ID map poisoned: {}", e))?;
        Ok(id_to_app.get(id).cloned())
    }
}

#[cfg(target_os = "macos")]
fn escape_applescript_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Run an AppleScript focus snippet via osascript.
#[cfg(all(target_os = "macos", not(test)))]
async fn run_macos_focus_script(script: &str) -> Result<()> {
    let mut cmd = tokio::process::Command::new("osascript");
    sanitize_command_env_preserve(&mut cmd, SESSION_ENV_VARS);
    let output = cmd.args(["-e", script]).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not allowed") || stderr.contains("assistive") {
            anyhow::bail!(
                "Accessibility permissions required. \
                 Please enable System Settings > Privacy & Security > Accessibility for this application."
            );
        }
        anyhow::bail!("Failed to focus window: {}", stderr);
    }
    Ok(())
}

/// No-op osascript stub for tests.
///
/// Without this, tests running on macOS actually activate and raise real desktop
/// application windows, violating Rule 6 ("stay inside visible region, touch only your own windows").
#[cfg(all(target_os = "macos", test))]
async fn run_macos_focus_script(_script: &str) -> Result<()> {
    Ok(())
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/computer/window/window_test.rs"]
mod tests;
