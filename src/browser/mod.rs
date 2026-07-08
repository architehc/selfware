//! Browser Automation Module
//!
//! **Browser automation is NOT implemented in this build.**  The public types
//! and method signatures are retained for API compatibility, but every action
//! method (`goto`, `screenshot`, `click`, `fill`) returns a clear
//! `BrowserError` explaining that browser automation is unavailable.  Callers
//! should check the error and handle the absence of a browser gracefully
//! rather than relying on fabricated results.

pub mod error;
pub mod tests;

pub use error::BrowserError;

use anyhow::Result;
use std::path::PathBuf;
use tracing::debug;

/// Browser automation configuration.
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Run browser in headless mode (no visible window).
    pub headless: bool,
    /// Viewport dimensions `(width, height)` in CSS pixels.
    pub viewport: (u32, u32),
    /// Artificial delay between actions, in milliseconds (useful for debugging).
    pub slow_mo: u64,
    /// Optional path to a specific browser executable.  When `None` the module
    /// searches common locations for Chrome/Chromium.
    pub executable_path: Option<PathBuf>,
    /// Extra command-line arguments to pass to the browser process.
    pub args: Vec<String>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            viewport: (1920, 1080),
            slow_mo: 0,
            executable_path: None,
            args: Vec::new(),
        }
    }
}

/// Information about a loaded page.
#[derive(Debug, Clone)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
}

/// A browser session handle.
///
/// Browser automation is **not implemented** in this build.  All action
/// methods return a `BrowserError` explaining that automation is unavailable.
/// The type is kept so that downstream code compiles and can handle the error
/// gracefully.
pub struct BrowserSession {
    /// Kept for API compatibility; no browser is launched.
    #[allow(dead_code)]
    config: BrowserConfig,
}

/// Error message used by every action method to report that browser
/// automation is not available.
const NOT_IMPLEMENTED_MSG: &str =
    "Browser automation is not implemented/available in this build. \
     No real browser driver is bundled; all action methods (goto, screenshot, \
     click, fill) return this error instead of fabricating results.";

impl BrowserSession {
    /// Create a new browser session from the given configuration.
    ///
    /// Construction is cheap and side-effect free; no browser process is
    /// launched.  Since automation is not implemented, all subsequent action
    /// methods will return an error.
    pub fn new(config: BrowserConfig) -> Self {
        debug!(
            "BrowserSession created (headless={}, viewport={}x{}) — \
             automation not implemented",
            config.headless, config.viewport.0, config.viewport.1
        );
        Self { config }
    }

    /// Navigate to the given URL and return basic page information.
    ///
    /// **Browser automation is not implemented in this build.**  This method
    /// always returns an error explaining that automation is unavailable
    /// rather than fabricating a `PageInfo`.
    pub async fn goto(&mut self, url: &str) -> Result<PageInfo> {
        debug!(
            "BrowserSession::goto({}) — browser automation not implemented, \
             returning error",
            url
        );
        Err(anyhow::anyhow!(
            "{}",
            BrowserError::Other(NOT_IMPLEMENTED_MSG.to_string())
        ))
    }

    /// Take a screenshot and save it to `path`.
    ///
    /// **Browser automation is not implemented in this build.**  This method
    /// always returns an error explaining that automation is unavailable
    /// rather than writing a placeholder file.
    pub async fn screenshot(&self, path: &PathBuf) -> Result<()> {
        debug!(
            "BrowserSession::screenshot({:?}) — browser automation not implemented, \
             returning error",
            path
        );
        Err(anyhow::anyhow!(
            "{}",
            BrowserError::Other(NOT_IMPLEMENTED_MSG.to_string())
        ))
    }

    /// Click an element matching the given CSS selector.
    ///
    /// **Browser automation is not implemented in this build.**  This method
    /// always returns an error explaining that automation is unavailable
    /// rather than silently succeeding.
    pub async fn click(&self, selector: &str) -> Result<()> {
        debug!(
            "BrowserSession::click({}) — browser automation not implemented, \
             returning error",
            selector
        );
        Err(anyhow::anyhow!(
            "{}",
            BrowserError::Other(NOT_IMPLEMENTED_MSG.to_string())
        ))
    }

    /// Fill a form field matching `selector` with `text`.
    ///
    /// **Browser automation is not implemented in this build.**  This method
    /// always returns an error explaining that automation is unavailable
    /// rather than silently succeeding.
    pub async fn fill(&self, selector: &str, text: &str) -> Result<()> {
        debug!(
            "BrowserSession::fill({}, {}) — browser automation not implemented, \
             returning error",
            selector, text
        );
        Err(anyhow::anyhow!(
            "{}",
            BrowserError::Other(NOT_IMPLEMENTED_MSG.to_string())
        ))
    }
}

/// Build a vector of Chrome command-line arguments from a [`BrowserConfig`].
///
/// This is exposed primarily for testing and for callers that want to launch
/// Chrome themselves with the same flags this module would use.
pub fn build_chrome_config(
    config: &BrowserConfig,
) -> std::result::Result<Vec<String>, BrowserError> {
    if config.viewport.0 == 0 || config.viewport.1 == 0 {
        return Err(BrowserError::InvalidConfig(format!(
            "viewport dimensions must be non-zero, got {}x{}",
            config.viewport.0, config.viewport.1
        )));
    }

    let mut args = vec![
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
        "--disable-gpu".to_string(),
        "--disable-dev-shm-usage".to_string(),
        format!("--window-size={},{}", config.viewport.0, config.viewport.1),
    ];

    if config.headless {
        args.push("--headless=new".to_string());
    }

    if config.slow_mo > 0 {
        args.push(format!("--slow-mo={}", config.slow_mo));
    }

    // User-supplied extra args appended last so they can override defaults.
    args.extend(config.args.iter().cloned());

    Ok(args)
}
