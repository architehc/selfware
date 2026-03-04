//! Screen capture tool for general-purpose screenshot functionality.
//!
//! Uses the `xcap` crate to capture the screen, specific windows, or regions.
//! Returns base64-encoded PNG data for direct use in multimodal messages.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use xcap::image;

use super::Tool;

/// Capture a screenshot of the screen, a window, or a region.
pub struct ScreenCapture;

#[async_trait]
impl Tool for ScreenCapture {
    fn name(&self) -> &str {
        "screen_capture"
    }

    fn description(&self) -> &str {
        "Capture a screenshot of the screen, a specific window, or a region. \
         Returns base64-encoded PNG data suitable for vision model analysis."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["screen", "window", "region"],
                    "description": "What to capture. 'screen' captures the primary monitor, \
                                    'window' captures a specific window by title, \
                                    'region' captures a screen region by coordinates. Default: screen"
                },
                "window_name": {
                    "type": "string",
                    "description": "Window title substring to match (required for target=window)"
                },
                "region": {
                    "type": "object",
                    "properties": {
                        "x": {"type": "integer", "description": "Left edge X coordinate"},
                        "y": {"type": "integer", "description": "Top edge Y coordinate"},
                        "width": {"type": "integer", "description": "Width in pixels"},
                        "height": {"type": "integer", "description": "Height in pixels"}
                    },
                    "description": "Screen region coordinates (required for target=region)"
                },
                "output_path": {
                    "type": "string",
                    "description": "Optional file path to save the PNG. If omitted, returns base64 data."
                }
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let target = args
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("screen");
        let output_path = args.get("output_path").and_then(|v| v.as_str());

        let image = match target {
            "screen" => capture_screen()?,
            "window" => {
                let window_name = args
                    .get("window_name")
                    .and_then(|v| v.as_str())
                    .context("window_name is required when target=window")?;
                capture_window(window_name)?
            }
            "region" => {
                let region = args
                    .get("region")
                    .context("region is required when target=region")?;
                let x = region.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let y = region.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let width = region
                    .get("width")
                    .and_then(|v| v.as_u64())
                    .context("region.width is required")? as u32;
                let height = region
                    .get("height")
                    .and_then(|v| v.as_u64())
                    .context("region.height is required")? as u32;
                capture_region(x, y, width, height)?
            }
            other => anyhow::bail!(
                "Unknown target: '{}'. Use 'screen', 'window', or 'region'.",
                other
            ),
        };

        let (img_width, img_height) = (image.width(), image.height());

        // Encode to PNG bytes
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .context("Failed to encode screenshot as PNG")?;

        // Either save to file or return base64
        if let Some(path) = output_path {
            std::fs::write(path, &png_bytes)
                .with_context(|| format!("Failed to write screenshot to {}", path))?;
            Ok(json!({
                "success": true,
                "target": target,
                "width": img_width,
                "height": img_height,
                "output_path": path,
                "file_size_bytes": png_bytes.len(),
            }))
        } else {
            let base64_data =
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &png_bytes);
            Ok(json!({
                "success": true,
                "target": target,
                "width": img_width,
                "height": img_height,
                "base64_png": base64_data,
                "size_bytes": png_bytes.len(),
            }))
        }
    }
}

/// Capture the primary monitor.
fn capture_screen() -> Result<image::RgbaImage> {
    let monitors = xcap::Monitor::all().context("Failed to enumerate monitors")?;
    let monitor = monitors
        .into_iter()
        .find(|m| m.is_primary())
        .or_else(|| xcap::Monitor::all().ok().and_then(|m| m.into_iter().next()))
        .context("No monitors found")?;
    monitor
        .capture_image()
        .context("Failed to capture screen image")
}

/// Capture a window whose title contains `name_substr`.
fn capture_window(name_substr: &str) -> Result<image::RgbaImage> {
    let windows = xcap::Window::all().context("Failed to enumerate windows")?;
    let needle = name_substr.to_lowercase();
    let window = windows
        .into_iter()
        .find(|w| w.title().to_lowercase().contains(&needle))
        .with_context(|| format!("No window found matching '{}'", name_substr))?;
    window
        .capture_image()
        .context("Failed to capture window image")
}

/// Capture a region of the primary monitor.
fn capture_region(x: i32, y: i32, width: u32, height: u32) -> Result<image::RgbaImage> {
    let full = capture_screen()?;
    let cropped = image::imageops::crop_imm(
        &full,
        x.max(0) as u32,
        y.max(0) as u32,
        width.min(full.width()),
        height.min(full.height()),
    )
    .to_image();
    Ok(cropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_capture_schema() {
        let tool = ScreenCapture;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["target"].is_object());
        assert!(schema["properties"]["window_name"].is_object());
        assert!(schema["properties"]["region"].is_object());
        assert!(schema["properties"]["output_path"].is_object());
    }

    #[test]
    fn test_screen_capture_name() {
        let tool = ScreenCapture;
        assert_eq!(tool.name(), "screen_capture");
    }
}
