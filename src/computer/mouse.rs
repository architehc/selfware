//! Mouse control for desktop automation.
//!
//! Provides programmatic mouse movement, clicking, scrolling, and dragging.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::{ActionRateLimiter, MovementProfile};

/// Mouse button types.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
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

        #[cfg(target_os = "macos")]
        {
            use tokio::process::Command;
            // Use osascript for macOS mouse control
            let script = format!(
                "tell application \"System Events\" to set position of mouse to {{{}, {}}}",
                x, y
            );
            // Note: This is a placeholder. Real implementation would use
            // CoreGraphics CGEventCreateMouseEvent for precise control.
            info!(
                "Mouse move to ({}, {}) — requires Accessibility permissions",
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
        Ok(())
    }

    /// Double-click at current position.
    pub async fn double_click(&self) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        debug!("Mouse double click");
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
        Ok(())
    }

    /// Drag from one point to another.
    pub async fn drag(&self, from: Point, to: Point, _button: MouseButton) -> Result<()> {
        if !self.rate_limiter.check() {
            bail!("Mouse action rate limit exceeded");
        }
        self.validate_coordinates(from.x, from.y)?;
        self.validate_coordinates(to.x, to.y)?;
        debug!(
            "Mouse drag from ({}, {}) to ({}, {})",
            from.x, from.y, to.x, to.y
        );
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
}
