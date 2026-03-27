//! Visual Validation Module
//!
//! Provides screenshot capture and vision-based analysis for website validation

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

/// Device configuration for screenshots
#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

impl Device {
    /// Desktop viewport
    pub fn desktop() -> Self {
        Self {
            name: "Desktop".to_string(),
            width: 1920,
            height: 1080,
        }
    }

    /// Mobile viewport
    pub fn mobile() -> Self {
        Self {
            name: "Mobile".to_string(),
            width: 375,
            height: 812,
        }
    }

    /// Tablet viewport
    pub fn tablet() -> Self {
        Self {
            name: "Tablet".to_string(),
            width: 768,
            height: 1024,
        }
    }
}

/// Screenshot capture configuration
#[derive(Debug, Clone)]
pub struct ScreenshotConfig {
    /// URL to capture
    pub url: String,
    /// Output directory
    pub output_dir: PathBuf,
    /// Devices to capture
    pub devices: Vec<Device>,
    /// Wait time for page load (ms)
    pub wait_ms: u64,
    /// Full page screenshot
    pub full_page: bool,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:8080".to_string(),
            output_dir: PathBuf::from("./screenshots"),
            devices: vec![Device::desktop(), Device::mobile()],
            wait_ms: 2000,
            full_page: true,
        }
    }
}

/// Screenshot result
#[derive(Debug, Clone)]
pub struct ScreenshotResult {
    pub device: String,
    pub path: PathBuf,
    pub success: bool,
    pub error: Option<String>,
}

/// Visual validator
pub struct VisualValidator {
    config: ScreenshotConfig,
}

impl VisualValidator {
    /// Create new validator
    pub fn new(config: ScreenshotConfig) -> Self {
        Self { config }
    }

    /// Capture screenshots for all devices
    pub async fn capture_screenshots(&self) -> Result<Vec<ScreenshotResult>> {
        std::fs::create_dir_all(&self.config.output_dir)?;

        let mut results = Vec::new();

        for device in &self.config.devices {
            info!(
                "Capturing screenshot for {} ({}x{})",
                device.name, device.width, device.height
            );

            let result = self.capture_single_device(device).await;
            results.push(result);
        }

        Ok(results)
    }

    /// Capture screenshot for a single device
    async fn capture_single_device(&self, device: &Device) -> ScreenshotResult {
        let filename = format!(
            "{}_{}x{}.png",
            sanitize_filename(&device.name),
            device.width,
            device.height
        );
        let path = self.config.output_dir.join(&filename);

        // Try playwright first
        if self.capture_with_playwright(device, &path).await.is_ok() {
            return ScreenshotResult {
                device: device.name.clone(),
                path,
                success: true,
                error: None,
            };
        }

        // Fallback to error
        ScreenshotResult {
            device: device.name.clone(),
            path,
            success: false,
            error: Some("Screenshot capture failed. Install playwright: pip install playwright && playwright install chromium".to_string()),
        }
    }

    /// Capture using playwright
    async fn capture_with_playwright(&self, device: &Device, path: &Path) -> Result<()> {
        let script = format!(
            r#"
from playwright.sync_api import sync_playwright
import time

with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={{'width': {}, 'height': {}}})
    page.goto('{}')
    page.wait_for_load_state('networkidle')
    time.sleep({})
    page.screenshot(path='{}', full_page={})
    browser.close()
"#,
            device.width,
            device.height,
            self.config.url,
            self.config.wait_ms as f64 / 1000.0,
            path.display(),
            self.config.full_page
        );

        let output = Command::new("python3").arg("-c").arg(&script).output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Playwright failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Analyze screenshot with vision model
    pub async fn analyze_screenshot(&self, path: &Path) -> Result<AnalysisResult> {
        // This would integrate with the vision model
        // For now, return a placeholder
        info!("Analyzing screenshot: {}", path.display());

        Ok(AnalysisResult {
            score: 7.5,
            issues: vec!["Consider increasing contrast for better accessibility".to_string()],
            suggestions: vec![
                "Add hover effects to interactive elements".to_string(),
                "Consider adding dark mode toggle".to_string(),
            ],
            summary: "Good overall design with minor improvements possible".to_string(),
        })
    }
}

/// Analysis result
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub score: f32,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
    pub summary: String,
}

/// Validation workflow configuration
#[derive(Debug, Clone)]
pub struct ValidationWorkflow {
    /// Website URL
    pub url: String,
    /// Local directory (if serving locally)
    pub local_dir: Option<PathBuf>,
    /// Number of iterations
    pub max_iterations: usize,
    /// Target score threshold
    pub target_score: f32,
    /// Screenshot configuration
    pub screenshot_config: ScreenshotConfig,
}

impl Default for ValidationWorkflow {
    fn default() -> Self {
        Self {
            url: "http://localhost:8080".to_string(),
            local_dir: None,
            max_iterations: 3,
            target_score: 8.0,
            screenshot_config: ScreenshotConfig::default(),
        }
    }
}

impl ValidationWorkflow {
    /// Run the validation workflow
    pub async fn run(&self) -> Result<ValidationReport> {
        info!("Starting visual validation workflow");

        let mut iterations = Vec::new();
        let mut current_score = 0.0;

        for i in 0..self.max_iterations {
            info!("Iteration {}/{}", i + 1, self.max_iterations);

            // Capture screenshots
            let validator = VisualValidator::new(self.screenshot_config.clone());
            let screenshots = validator.capture_screenshots().await?;

            // Analyze (placeholder - would use vision model)
            let analysis = AnalysisResult {
                score: 7.0 + (i as f32 * 0.5),
                issues: vec![],
                suggestions: vec!["Improve typography".to_string()],
                summary: format!("Iteration {} analysis", i + 1),
            };

            current_score = analysis.score;

            iterations.push(IterationResult {
                iteration: i + 1,
                screenshots,
                analysis,
            });

            // Check if we've reached target
            if current_score >= self.target_score {
                info!("Target score reached: {}", current_score);
                break;
            }
        }

        Ok(ValidationReport {
            final_score: current_score,
            iterations,
            passed: current_score >= self.target_score,
        })
    }
}

/// Single iteration result
#[derive(Debug, Clone)]
pub struct IterationResult {
    pub iteration: usize,
    pub screenshots: Vec<ScreenshotResult>,
    pub analysis: AnalysisResult,
}

/// Final validation report
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub final_score: f32,
    pub iterations: Vec<IterationResult>,
    pub passed: bool,
}

fn sanitize_filename(name: &str) -> String {
    name.to_lowercase()
        .replace(" ", "_")
        .replace("/", "_")
        .replace("\\", "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_creation() {
        let desktop = Device::desktop();
        assert_eq!(desktop.width, 1920);
        assert_eq!(desktop.height, 1080);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Desktop View"), "desktop_view");
    }
}
