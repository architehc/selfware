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
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;
#[cfg(target_os = "linux")]
use tracing::warn;
use tracing::{debug, info};

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

/// Platform-abstracted window management.
#[async_trait]
pub trait WindowPlatform: Send + Sync {
    /// List all visible windows.
    async fn list_windows(&self) -> Result<Vec<WindowInfo>>;
    /// Focus (bring to front) a specific window.
    async fn focus_window(&self, id: &WindowId) -> Result<()>;
    /// Get the currently active/focused window.
    async fn get_active_window(&self) -> Result<WindowInfo>;
    /// Resize a window.
    async fn resize_window(&self, id: &WindowId, width: u32, height: u32) -> Result<()>;
    /// Move a window to specific coordinates.
    async fn move_window(&self, id: &WindowId, x: i32, y: i32) -> Result<()>;
    /// Minimize a window.
    async fn minimize_window(&self, id: &WindowId) -> Result<()>;
    /// Close a window.
    async fn close_window(&self, id: &WindowId) -> Result<()>;
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
        tokio::process::Command::new("which")
            .arg(tool)
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
            tokio::process::Command::new("open")
                .arg("-a")
                .arg(app_name)
                .output()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
        }

        #[cfg(target_os = "linux")]
        {
            // Try to find the application in PATH
            let which_output = tokio::process::Command::new("which")
                .arg(app_name)
                .output()
                .await;

            let exists = matches!(which_output, Ok(ref o) if o.status.success());

            if !exists {
                // Try common desktop file locations
                let desktop_file = format!("/usr/share/applications/{}.desktop", app_name);
                if tokio::fs::try_exists(&desktop_file).await.unwrap_or(false) {
                    tokio::process::Command::new("gtk-launch")
                        .arg(app_name)
                        .spawn()
                        .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
                    return Ok(());
                }

                anyhow::bail!("Application '{}' not found in PATH", app_name);
            }

            tokio::process::Command::new(app_name)
                .spawn()
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
        let output = tokio::process::Command::new("wmctrl")
            .args(["-l", "-G", "-p"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("wmctrl not available: {}", e))?;

        if !output.status.success() {
            // Try without -G flag for older wmctrl versions
            let output_no_geom = tokio::process::Command::new("wmctrl")
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
        let output = tokio::process::Command::new("xprop")
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
        let output = tokio::process::Command::new("xdotool")
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
        let output = tokio::process::Command::new("xdotool")
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

            let name_output = tokio::process::Command::new("xdotool")
                .args(["getwindowname", &wid.to_string()])
                .output()
                .await;

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
        let output = tokio::process::Command::new("xdotool")
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

        let name_output = tokio::process::Command::new("xdotool")
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
        let wmctrl_result = tokio::process::Command::new("wmctrl")
            .args(["-i", "-a", &id_str])
            .output()
            .await;

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
        match tokio::process::Command::new("xdotool")
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
        let result = tokio::process::Command::new("wmctrl")
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
        let result = tokio::process::Command::new("xdotool")
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
        // Try wmctrl first
        let result = tokio::process::Command::new("wmctrl")
            .args([
                "-i",
                "-r",
                &format!("0x{:x}", id.0),
                "-e",
                &format!("0,{}, {}, -1,-1", x, y),
            ])
            .output()
            .await;

        if let Ok(ref output) = result {
            if output.status.success() {
                debug!("Moved window {} to {}, {}", id.0, x, y);
                return Ok(());
            }
        }

        // Fall back to xdotool
        let result = tokio::process::Command::new("xdotool")
            .args([
                "windowmove",
                &id.0.to_string(),
                &x.to_string(),
                &y.to_string(),
            ])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if result.status.success() {
            debug!("Moved window {} to {}, {} via xdotool", id.0, x, y);
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&result.stderr);
            anyhow::bail!("Failed to move window: {}", stderr)
        }
    }

    #[cfg(target_os = "linux")]
    async fn minimize_window_linux(&self, id: &WindowId) -> Result<()> {
        // xdotool is the best option for minimize
        let result = tokio::process::Command::new("xdotool")
            .args(["windowminimize", &id.0.to_string()])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("xdotool not available: {}", e))?;

        if result.status.success() {
            debug!("Minimized window {}", id.0);
            Ok(())
        } else {
            // Try alternative: wmctrl with iconify
            let result = tokio::process::Command::new("wmctrl")
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
        let result = tokio::process::Command::new("wmctrl")
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
        let result = tokio::process::Command::new("xdotool")
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
        let output = tokio::process::Command::new("osascript")
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
        let frontmost_output = tokio::process::Command::new("osascript")
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
            let window_output = tokio::process::Command::new("osascript")
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

        let output = tokio::process::Command::new("osascript")
            .args(["-e", &focus_script])
            .output()
            .await?;

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

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/computer/window/window_test.rs"]
mod tests;
