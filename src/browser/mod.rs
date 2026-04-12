//! Browser Automation Module (STUB - NOT IMPLEMENTED)
//!
//! ⚠️ WARNING: This module is a complete stub. All methods only log and return Ok(()).
//! NO ACTUAL BROWSER AUTOMATION IS PERFORMED.
//! TODO: Implement actual browser automation using playwright or similar.

use anyhow::Result;
use std::path::PathBuf;
use tracing::warn;

/// Browser automation configuration
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    pub headless: bool,
    pub viewport: (u32, u32),
    pub slow_mo: u64,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            viewport: (1920, 1080),
            slow_mo: 0,
        }
    }
}

/// Browser session for automation (STUB - NOT IMPLEMENTED)
pub struct BrowserSession {
    _config: BrowserConfig,
}

impl BrowserSession {
    pub fn new(config: BrowserConfig) -> Self {
        warn!("STUB: BrowserSession::new - browser automation not implemented");
        Self { _config: config }
    }

    /// STUB: Does not actually navigate to URL
    pub async fn goto(&self, url: &str) -> Result<PageInfo> {
        warn!("STUB: BrowserSession::goto({}) - no actual navigation", url);
        Ok(PageInfo {
            url: url.to_string(),
            title: "STUB: Not actually loaded".to_string(),
        })
    }

    /// STUB: Does not actually take a screenshot
    pub async fn screenshot(&self, path: &PathBuf) -> Result<()> {
        warn!("STUB: BrowserSession::screenshot({:?}) - no screenshot taken", path);
        Ok(())
    }

    /// STUB: Does not actually click anything
    pub async fn click(&self, selector: &str) -> Result<()> {
        warn!("STUB: BrowserSession::click({}) - no click performed", selector);
        Ok(())
    }

    /// STUB: Does not actually fill any form
    pub async fn fill(&self, selector: &str, text: &str) -> Result<()> {
        warn!("STUB: BrowserSession::fill({}, {}) - no form filled", selector, text);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PageInfo {
    pub url: String,
    pub title: String,
}
