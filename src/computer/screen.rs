//! Screen capture and analysis for desktop automation.
//!
//! Provides full-screen and region-based screen capture with optional
//! VLM (Vision Language Model) analysis.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenCaptureBackend {
    Xcap,
    WindowsWsl,
}

impl ScreenCaptureBackend {
    pub(crate) fn doctor_name(self) -> &'static str {
        match self {
            Self::Xcap => "xcap",
            Self::WindowsWsl => "windows_wsl",
        }
    }

    pub(crate) fn doctor_message(self) -> &'static str {
        match self {
            Self::Xcap => "Screen capture available",
            Self::WindowsWsl => "Screen capture available via Windows fallback (WSL)",
        }
    }
}

#[derive(Debug, Deserialize)]
struct WindowsCapturePayload {
    width: u32,
    height: u32,
    base64_png: String,
}

impl ScreenCapture {
    /// Capture the full primary screen.
    pub async fn capture_full() -> Result<CapturedScreen> {
        Self::validate_capture_dimensions(0, 0, 32768, 32768)?;

        info!("Capturing full screen");

        match Self::capture_full_xcap() {
            Ok(captured) => Ok(captured),
            Err(xcap_error) if Self::can_use_wsl_windows_capture() => {
                warn!(
                    error = %xcap_error,
                    "xcap screen capture failed in WSL; falling back to Windows capture"
                );
                Self::capture_full_windows_wsl().map_err(|fallback_error| {
                    anyhow::anyhow!(
                        "Screen capture failed via xcap ({}) and WSL fallback ({})",
                        xcap_error,
                        fallback_error
                    )
                })
            }
            Err(err) => Err(err),
        }
    }

    fn capture_full_xcap() -> Result<CapturedScreen> {
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

        // Wrap blocking file I/O in block_in_place to avoid blocking the async runtime
        let png_data = tokio::task::block_in_place(|| std::fs::read(&tmp_path))?;
        use base64::Engine as _;
        let base64_png = base64::engine::general_purpose::STANDARD.encode(&png_data);

        Ok(CapturedScreen {
            width,
            height,
            base64_png,
            analysis: None,
        })
    }

    fn capture_full_windows_wsl() -> Result<CapturedScreen> {
        let script = tempfile::Builder::new()
            .prefix("selfware-screen-")
            .suffix(".ps1")
            .tempfile()
            .context("Failed to create temporary PowerShell script")?;
        std::fs::write(script.path(), Self::windows_capture_script())
            .context("Failed to write temporary PowerShell script")?;

        let windows_script_path = Self::wsl_to_windows_path(script.path())?;
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                &windows_script_path,
            ])
            .output()
            .context("Failed to run powershell.exe for WSL screen capture")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "Windows screen capture command failed: {}",
                if stderr.is_empty() {
                    "powershell.exe exited unsuccessfully"
                } else {
                    &stderr
                }
            );
        }

        Self::parse_windows_capture_payload(&output.stdout)
    }

    fn parse_windows_capture_payload(output: &[u8]) -> Result<CapturedScreen> {
        let stdout = String::from_utf8(output.to_vec())
            .context("Windows screen capture output was not valid UTF-8")?;
        let payload: WindowsCapturePayload = serde_json::from_str(stdout.trim())
            .context("Failed to parse Windows screen capture JSON")?;
        Self::validate_capture_dimensions(0, 0, payload.width, payload.height)?;

        if payload.base64_png.is_empty() {
            bail!("Windows screen capture returned an empty image");
        }

        Ok(CapturedScreen {
            width: payload.width,
            height: payload.height,
            base64_png: payload.base64_png,
            analysis: None,
        })
    }

    fn windows_capture_script() -> &'static str {
        r#"
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bmp)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$stream = New-Object System.IO.MemoryStream
$bmp.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
$obj = @{
    width = $bounds.Width
    height = $bounds.Height
    base64_png = [Convert]::ToBase64String($stream.ToArray())
}
$obj | ConvertTo-Json -Compress
$graphics.Dispose()
$bmp.Dispose()
$stream.Dispose()
"#
    }

    fn wsl_to_windows_path(path: &Path) -> Result<String> {
        let output = Command::new("wslpath")
            .args(["-w"])
            .arg(path)
            .output()
            .context("Failed to run wslpath for PowerShell script conversion")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            bail!(
                "wslpath failed while preparing PowerShell capture: {}",
                if stderr.is_empty() {
                    "wslpath exited unsuccessfully"
                } else {
                    &stderr
                }
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub(crate) fn available_backend() -> Option<ScreenCaptureBackend> {
        if Self::xcap_available() {
            Some(ScreenCaptureBackend::Xcap)
        } else if Self::can_use_wsl_windows_capture() {
            Some(ScreenCaptureBackend::WindowsWsl)
        } else {
            None
        }
    }

    fn xcap_available() -> bool {
        xcap::Monitor::all()
            .map(|monitors| !monitors.is_empty())
            .unwrap_or(false)
    }

    fn can_use_wsl_windows_capture() -> bool {
        Self::is_wsl_environment()
            && Self::command_succeeds("wslpath", &["-w", "/tmp"])
            && Self::command_succeeds(
                "powershell.exe",
                &["-NoProfile", "-Command", "Write-Output ok"],
            )
    }

    fn command_succeeds(command: &str, args: &[&str]) -> bool {
        Command::new(command)
            .args(args)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn is_wsl_environment() -> bool {
        std::env::var_os("WSL_INTEROP").is_some()
            || std::env::var_os("WSL_DISTRO_NAME").is_some()
            || std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .map(|release| Self::osrelease_looks_like_wsl(&release))
                .unwrap_or(false)
    }

    fn osrelease_looks_like_wsl(release: &str) -> bool {
        release.to_ascii_lowercase().contains("microsoft")
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
    fn validate_capture_dimensions(_x: i32, _y: i32, width: u32, height: u32) -> Result<()> {
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
#[path = "../../tests/unit/computer/screen/screen_test.rs"]
mod tests;
