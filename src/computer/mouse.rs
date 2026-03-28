//! Mouse control for desktop automation.
//!
//! Provides programmatic mouse movement, clicking, scrolling, and dragging.
//! On Linux, uses xdotool for actual input. On other platforms, stubs with logging.

#[cfg(all(target_os = "linux", not(test)))]
use anyhow::Context;
use anyhow::{bail, Result};
#[cfg(target_os = "linux")]
use serde::{Deserialize, Serialize};
#[cfg(not(target_os = "macos"))]
use tracing::debug;
#[cfg(target_os = "macos")]
use tracing::{debug, warn};

use super::{ActionRateLimiter, MovementProfile};

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
fn build_mousemove_args(x: i32, y: i32) -> Vec<String> {
    vec!["mousemove".to_string(), x.to_string(), y.to_string()]
}

/// Build xdotool args for a click.
fn build_click_args(button: u8) -> Vec<String> {
    vec!["click".to_string(), button.to_string()]
}

/// Build xdotool args for a double-click.
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
            let args = build_mousemove_args(x, y);
            run_xdotool(&args).await?;
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
            let args = build_click_args(button.xdotool_button());
            run_xdotool(&args).await?;
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
            let args = build_double_click_args();
            run_xdotool(&args).await?;
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
            let commands = build_scroll_args(delta_x, delta_y);
            for args in &commands {
                run_xdotool(args).await?;
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
            let args = build_drag_args(from, to, button.xdotool_button());
            run_xdotool(&args).await?;
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
mod tests {
    use super::*;

    #[test]
    fn test_point() {
        let p = Point::new(100, 200);
        assert_eq!(p.x, 100);
        assert_eq!(p.y, 200);
    }

    #[test]
    fn test_point_negative() {
        let p = Point::new(-50, -100);
        assert_eq!(p.x, -50);
        assert_eq!(p.y, -100);
    }

    #[test]
    fn test_point_serde_roundtrip() {
        let p = Point::new(42, 84);
        let json = serde_json::to_string(&p).unwrap();
        let parsed: Point = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.x, 42);
        assert_eq!(parsed.y, 84);
    }

    #[test]
    fn test_validate_coordinates() {
        let mouse = MouseController::new();
        assert!(mouse.validate_coordinates(100, 200).is_ok());
        assert!(mouse.validate_coordinates(-100, 200).is_ok());
        assert!(mouse.validate_coordinates(50000, 200).is_err());
    }

    #[test]
    fn test_validate_coordinates_boundary() {
        let mouse = MouseController::new();
        // Exactly at the 32768 boundary
        assert!(mouse.validate_coordinates(32768, 0).is_ok());
        assert!(mouse.validate_coordinates(0, 32768).is_ok());
        // Just over
        assert!(mouse.validate_coordinates(32769, 0).is_err());
        assert!(mouse.validate_coordinates(0, 32769).is_err());
        // Negative boundary
        assert!(mouse.validate_coordinates(-32768, 0).is_ok());
        assert!(mouse.validate_coordinates(-32769, 0).is_err());
    }

    #[test]
    fn test_validate_coordinates_both_out_of_range() {
        let mouse = MouseController::new();
        assert!(mouse.validate_coordinates(50000, 50000).is_err());
    }

    #[tokio::test]
    async fn test_move_to_valid() {
        let mouse = MouseController::new();
        assert!(mouse.move_to(100, 200).await.is_ok());
    }

    #[tokio::test]
    async fn test_move_to_out_of_bounds() {
        let mouse = MouseController::new();
        assert!(mouse.move_to(50000, 200).await.is_err());
    }

    #[tokio::test]
    async fn test_click() {
        let mouse = MouseController::new();
        assert!(mouse.click(MouseButton::Left).await.is_ok());
        assert!(mouse.click(MouseButton::Right).await.is_ok());
        assert!(mouse.click(MouseButton::Middle).await.is_ok());
    }

    #[tokio::test]
    async fn test_double_click() {
        let mouse = MouseController::new();
        assert!(mouse.double_click().await.is_ok());
    }

    #[tokio::test]
    async fn test_click_at() {
        let mouse = MouseController::new();
        assert!(mouse.click_at(100, 200, MouseButton::Left).await.is_ok());
    }

    #[tokio::test]
    async fn test_click_at_out_of_bounds() {
        let mouse = MouseController::new();
        assert!(mouse.click_at(50000, 200, MouseButton::Left).await.is_err());
    }

    #[tokio::test]
    async fn test_scroll() {
        let mouse = MouseController::new();
        assert!(mouse.scroll(0, -3).await.is_ok());
        assert!(mouse.scroll(5, 5).await.is_ok());
        assert!(mouse.scroll(0, 0).await.is_ok());
    }

    #[tokio::test]
    async fn test_drag_valid() {
        let mouse = MouseController::new();
        let from = Point::new(10, 10);
        let to = Point::new(200, 200);
        assert!(mouse.drag(from, to, MouseButton::Left).await.is_ok());
    }

    #[tokio::test]
    async fn test_drag_out_of_bounds() {
        let mouse = MouseController::new();
        let from = Point::new(10, 10);
        let to = Point::new(50000, 200);
        assert!(mouse.drag(from, to, MouseButton::Left).await.is_err());
    }

    #[test]
    fn test_mouse_controller_default() {
        let mouse = MouseController::default();
        assert!(mouse.validate_coordinates(0, 0).is_ok());
    }

    #[test]
    fn test_with_movement_profile() {
        let mouse =
            MouseController::new().with_movement_profile(super::super::MovementProfile::Bezier);
        assert!(mouse.validate_coordinates(0, 0).is_ok());
    }

    #[test]
    fn test_mouse_button_serde() {
        let buttons = vec![MouseButton::Left, MouseButton::Right, MouseButton::Middle];
        for button in buttons {
            let json = serde_json::to_string(&button).unwrap();
            let parsed: MouseButton = serde_json::from_str(&json).unwrap();
            let _ = format!("{:?}", parsed);
        }
    }

    // ---- Command construction tests ----

    #[test]
    fn test_mouse_button_xdotool_numbers() {
        assert_eq!(MouseButton::Left.xdotool_button(), 1);
        assert_eq!(MouseButton::Middle.xdotool_button(), 2);
        assert_eq!(MouseButton::Right.xdotool_button(), 3);
    }

    #[test]
    fn test_build_mousemove_args() {
        let args = build_mousemove_args(100, 200);
        assert_eq!(args, vec!["mousemove", "100", "200"]);
    }

    #[test]
    fn test_build_mousemove_args_negative() {
        let args = build_mousemove_args(-50, -100);
        assert_eq!(args, vec!["mousemove", "-50", "-100"]);
    }

    #[test]
    fn test_build_click_args() {
        assert_eq!(build_click_args(1), vec!["click", "1"]);
        assert_eq!(build_click_args(2), vec!["click", "2"]);
        assert_eq!(build_click_args(3), vec!["click", "3"]);
    }

    #[test]
    fn test_build_double_click_args() {
        let args = build_double_click_args();
        assert_eq!(args, vec!["click", "--repeat", "2", "1"]);
    }

    #[test]
    fn test_build_scroll_args_down() {
        let commands = build_scroll_args(0, 3);
        assert_eq!(commands.len(), 3);
        for cmd in &commands {
            assert_eq!(cmd, &vec!["click".to_string(), "5".to_string()]);
        }
    }

    #[test]
    fn test_build_scroll_args_up() {
        let commands = build_scroll_args(0, -2);
        assert_eq!(commands.len(), 2);
        for cmd in &commands {
            assert_eq!(cmd, &vec!["click".to_string(), "4".to_string()]);
        }
    }

    #[test]
    fn test_build_scroll_args_horizontal() {
        // Scroll right
        let commands = build_scroll_args(2, 0);
        assert_eq!(commands.len(), 2);
        for cmd in &commands {
            assert_eq!(cmd, &vec!["click".to_string(), "7".to_string()]);
        }

        // Scroll left
        let commands = build_scroll_args(-1, 0);
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0], vec!["click".to_string(), "6".to_string()]);
    }

    #[test]
    fn test_build_scroll_args_zero() {
        let commands = build_scroll_args(0, 0);
        assert!(commands.is_empty());
    }

    #[test]
    fn test_build_scroll_args_both_axes() {
        let commands = build_scroll_args(1, -1);
        // 1 vertical (up) + 1 horizontal (right)
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0], vec!["click".to_string(), "4".to_string()]); // up
        assert_eq!(commands[1], vec!["click".to_string(), "7".to_string()]); // right
    }

    #[test]
    fn test_build_drag_args() {
        let from = Point::new(10, 20);
        let to = Point::new(300, 400);
        let args = build_drag_args(from, to, 1);
        assert_eq!(
            args,
            vec![
                "mousemove",
                "10",
                "20",
                "mousedown",
                "1",
                "mousemove",
                "300",
                "400",
                "mouseup",
                "1"
            ]
        );
    }

    #[test]
    fn test_build_drag_args_right_button() {
        let from = Point::new(0, 0);
        let to = Point::new(100, 100);
        let args = build_drag_args(from, to, 3);
        assert_eq!(args[4], "3"); // mousedown button
        assert_eq!(args[9], "3"); // mouseup button
    }
}
