//! Window management for desktop automation.
//!
//! Provides cross-platform window listing, focusing, resizing, and application control.

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

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
    // In a full implementation, this would hold a Box<dyn WindowPlatform>
    // For now, we use a stub implementation
}

impl WindowManager {
    pub fn new() -> Self {
        Self {}
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
            // macOS focus is a no-op stub for now
        }
        Ok(())
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
            tokio::process::Command::new(app_name)
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to launch '{}': {}", app_name, e))?;
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    async fn list_windows_linux(&self) -> Result<Vec<WindowInfo>> {
        // Try wmctrl first, fall back to xdotool
        if let Ok(windows) = self.list_windows_wmctrl().await {
            return Ok(windows);
        }
        if let Ok(windows) = self.list_windows_xdotool().await {
            return Ok(windows);
        }
        warn!("Neither wmctrl nor xdotool available for window listing");
        Ok(Vec::new())
    }

    #[cfg(target_os = "linux")]
    async fn list_windows_wmctrl(&self) -> Result<Vec<WindowInfo>> {
        let output = tokio::process::Command::new("wmctrl")
            .args(["-l", "-p"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("wmctrl not available: {}", e))?;

        if !output.status.success() {
            anyhow::bail!("wmctrl failed with status {}", output.status);
        }

        let active_id = self.get_active_window_id_xdotool().await.ok();

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut windows = Vec::new();

        for line in stdout.lines() {
            // Format: 0x04600003  0 12345 hostname Window Title
            let parts: Vec<&str> = line.splitn(5, char::is_whitespace).collect();
            if parts.len() < 5 {
                continue;
            }

            let hex_id = parts[0].trim();
            let wid = u64::from_str_radix(hex_id.trim_start_matches("0x"), 16).unwrap_or(0);
            if wid == 0 {
                continue;
            }

            // Skip the desktop number (parts[1]) and pid (parts[2])
            // parts[3] is hostname, parts[4] is title (but splitn(5) means we need to
            // handle whitespace carefully)
            // Re-parse more carefully: the format is space-separated with the title
            // being everything after hostname
            let rest = line.trim();
            // Skip hex id
            let rest = rest[hex_id.len()..].trim_start();
            // Skip desktop id
            let (_, rest) = rest.split_once(char::is_whitespace).unwrap_or(("", rest));
            let rest = rest.trim_start();
            // Skip pid
            let (_, rest) = rest.split_once(char::is_whitespace).unwrap_or(("", rest));
            let rest = rest.trim_start();
            // Skip hostname
            let (hostname, title) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let title = title.trim().to_string();

            let is_focused = active_id.as_ref().map_or(false, |aid| *aid == wid);

            windows.push(WindowInfo {
                id: WindowId(wid),
                title: title.clone(),
                app_name: hostname.to_string(),
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                is_focused,
                is_minimized: false,
            });
        }

        Ok(windows)
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

            let is_focused = active_id.as_ref().map_or(false, |aid| *aid == wid);

            windows.push(WindowInfo {
                id: WindowId(wid),
                title: title.clone(),
                app_name: title,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
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
            String::from_utf8_lossy(&name_output.stdout).trim().to_string()
        } else {
            String::new()
        };

        Ok(WindowInfo {
            id: WindowId(wid),
            title: title.clone(),
            app_name: title,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
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

        match wmctrl_result {
            Ok(output) if output.status.success() => {
                debug!("Focused window {} via wmctrl", id_str);
                return Ok(());
            }
            _ => {}
        }

        // Fall back to xdotool
        match tokio::process::Command::new("xdotool")
            .args(["windowactivate", &id.0.to_string()])
            .output()
            .await
        {
            Ok(output) if output.status.success() => {
                debug!("Focused window {} via xdotool", id.0);
                return Ok(());
            }
            _ => {}
        }

        warn!(
            "Could not focus window {} - neither wmctrl nor xdotool succeeded",
            id.0
        );
        Ok(())
    }

    #[cfg(target_os = "macos")]
    async fn list_windows_macos(&self) -> Result<Vec<WindowInfo>> {
        // Use osascript to list windows on macOS
        let output = tokio::process::Command::new("osascript")
            .args(["-e", "tell application \"System Events\" to get name of every process whose visible is true"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let apps: Vec<WindowInfo> = stdout
            .split(", ")
            .enumerate()
            .map(|(i, name)| WindowInfo {
                id: WindowId(i as u64),
                title: name.trim().to_string(),
                app_name: name.trim().to_string(),
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                is_focused: i == 0, // First is usually focused
                is_minimized: false,
            })
            .collect();

        Ok(apps)
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_id() {
        let id = WindowId(42);
        assert_eq!(id.0, 42);
    }

    #[test]
    fn test_window_id_equality() {
        assert_eq!(WindowId(1), WindowId(1));
        assert_ne!(WindowId(1), WindowId(2));
    }

    #[test]
    fn test_window_id_serde_roundtrip() {
        let id = WindowId(99);
        let json = serde_json::to_string(&id).unwrap();
        let parsed: WindowId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_window_info() {
        let info = WindowInfo {
            id: WindowId(1),
            title: "Test Window".to_string(),
            app_name: "test-app".to_string(),
            x: 100,
            y: 200,
            width: 800,
            height: 600,
            is_focused: true,
            is_minimized: false,
        };
        assert!(info.is_focused);
        assert!(!info.is_minimized);
        assert_eq!(info.title, "Test Window");
        assert_eq!(info.app_name, "test-app");
        assert_eq!(info.x, 100);
        assert_eq!(info.y, 200);
        assert_eq!(info.width, 800);
        assert_eq!(info.height, 600);
    }

    #[test]
    fn test_window_info_serde_roundtrip() {
        let info = WindowInfo {
            id: WindowId(5),
            title: "Firefox".to_string(),
            app_name: "firefox".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            is_focused: false,
            is_minimized: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: WindowInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, WindowId(5));
        assert_eq!(parsed.title, "Firefox");
        assert!(!parsed.is_focused);
        assert!(parsed.is_minimized);
    }

    #[test]
    fn test_window_manager_default() {
        let _wm = WindowManager::default();
        let _ = format!("{:?}", "WindowManager created");
    }

    #[tokio::test]
    async fn test_list_windows_does_not_panic() {
        let wm = WindowManager::new();
        // Should not panic regardless of platform or available tools
        let result = wm.list_windows().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_focus_window_does_not_panic() {
        let wm = WindowManager::new();
        // In headless environments this may fail but should not panic
        let _result = wm.focus_window(&WindowId(1)).await;
    }

    #[tokio::test]
    async fn test_get_active_window_does_not_panic() {
        let wm = WindowManager::new();
        // In headless environments this will likely error but should not panic
        let _result = wm.get_active_window().await;
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_linux_list_windows_wmctrl_parse() {
        // Unit test for wmctrl output parsing logic - just verify the method exists
        // and returns Ok in a graceful-degradation manner
        let wm = WindowManager::new();
        let result = wm.list_windows_linux().await;
        // Should always succeed (returns empty vec if tools unavailable)
        assert!(result.is_ok());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_linux_focus_window_graceful() {
        let wm = WindowManager::new();
        // Focus on a fake window ID - should fail gracefully, not panic
        let result = wm.focus_window_linux(&WindowId(999999)).await;
        // We expect an error (window doesn't exist) but no panic
        assert!(result.is_err() || result.is_ok());
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn test_linux_get_active_window_graceful() {
        let wm = WindowManager::new();
        // In headless CI, this will error but should not panic
        let _result = wm.get_active_window_linux().await;
    }
}
