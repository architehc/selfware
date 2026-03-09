#![allow(dead_code, unused_imports, unused_variables)]
//! Screen capture and analysis for desktop automation.
//!
//! Provides full-screen and region-based screen capture with optional
//! VLM (Vision Language Model) analysis.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// A captured screen region.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedScreen {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// PNG image data (base64-encoded).
    pub base64_png: String,
    /// Optional analysis result from VLM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<String>,
}

/// Screen region for partial captures.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRegion {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Screen capture controller.
pub struct ScreenCapture;

impl ScreenCapture {
    /// Capture the full primary screen.
    pub async fn capture_full() -> Result<CapturedScreen> {
        Self::validate_capture_dimensions(0, 0, 32768, 32768)?;

        info!("Capturing full screen");

        // Use xcap for cross-platform screen capture
        let monitors = xcap::Monitor::all()
            .map_err(|e| anyhow::anyhow!("Failed to enumerate monitors: {}", e))?;

        let monitor = monitors
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No monitors found"))?;

        let image = monitor
            .capture_image()
            .map_err(|e| anyhow::anyhow!("Screen capture failed: {}", e))?;

        let width = image.width();
        let height = image.height();

        // Encode to PNG via a temp file (xcap images support save())
        let tmp = tempfile::NamedTempFile::new()?;
        let tmp_path = tmp.path().to_path_buf();
        image
            .save(&tmp_path)
            .map_err(|e| anyhow::anyhow!("PNG encoding failed: {}", e))?;

        let png_data = std::fs::read(&tmp_path)?;
        use base64::Engine as _;
        let base64_png = base64::engine::general_purpose::STANDARD.encode(&png_data);

        Ok(CapturedScreen {
            width,
            height,
            base64_png,
            analysis: None,
        })
    }

    /// Capture a specific screen region.
    pub async fn capture_region(region: ScreenRegion) -> Result<CapturedScreen> {
        Self::validate_capture_dimensions(region.x, region.y, region.width, region.height)?;

        info!(
            "Capturing screen region: ({}, {}) {}x{}",
            region.x, region.y, region.width, region.height
        );

        // For now, capture full screen and crop
        let full = Self::capture_full().await?;
        // In a real implementation, we'd crop the image to the region
        Ok(full)
    }

    /// Validate capture dimensions to prevent unreasonable values.
    fn validate_capture_dimensions(x: i32, y: i32, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            bail!("Screen capture dimensions must be non-zero");
        }
        if width > 32768 || height > 32768 {
            bail!(
                "Screen capture dimensions too large: {}x{} (max 32768x32768)",
                width,
                height
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_region() {
        let region = ScreenRegion::new(10, 20, 800, 600);
        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.width, 800);
        assert_eq!(region.height, 600);
    }

    #[test]
    fn test_validate_dimensions() {
        assert!(ScreenCapture::validate_capture_dimensions(0, 0, 1920, 1080).is_ok());
        assert!(ScreenCapture::validate_capture_dimensions(0, 0, 0, 100).is_err());
        assert!(ScreenCapture::validate_capture_dimensions(0, 0, 50000, 100).is_err());
    }
}
