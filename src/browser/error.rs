//! Browser automation error types

use thiserror::Error;

/// Errors that can occur during browser automation
#[derive(Error, Debug)]
pub enum BrowserError {
    /// Browser not initialized
    #[error("Browser not initialized")]
    NotInitialized,

    /// No page is currently open
    #[error("No page is currently open")]
    NoPageOpen,

    /// Element not found
    #[error("Element not found: {0}")]
    ElementNotFound(String),

    /// Navigation failed
    #[error("Navigation failed: {0}")]
    NavigationFailed(String),

    /// Screenshot failed
    #[error("Screenshot failed: {0}")]
    ScreenshotFailed(String),

    /// JavaScript execution failed
    #[error("JavaScript execution failed: {0}")]
    JavaScriptFailed(String),

    /// Timeout waiting for element
    #[error("Timeout waiting for element: {0}")]
    Timeout(String),

    /// Browser launch failed
    #[error("Browser launch failed: {0}")]
    LaunchFailed(String),

    /// Connection lost
    #[error("Browser connection lost")]
    ConnectionLost,

    /// Invalid configuration
    #[error("Invalid browser configuration: {0}")]
    InvalidConfig(String),

    /// Other errors
    #[error("Browser error: {0}")]
    Other(String),
}

impl BrowserError {
    /// Create a new element not found error
    pub fn element_not_found(selector: impl Into<String>) -> Self {
        Self::ElementNotFound(selector.into())
    }

    /// Create a new navigation failed error
    pub fn navigation_failed(reason: impl Into<String>) -> Self {
        Self::NavigationFailed(reason.into())
    }

    /// Create a new timeout error
    pub fn timeout(selector: impl Into<String>) -> Self {
        Self::Timeout(selector.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_messages() {
        let err = BrowserError::NotInitialized;
        assert_eq!(err.to_string(), "Browser not initialized");

        let err = BrowserError::element_not_found("#button");
        assert_eq!(err.to_string(), "Element not found: #button");

        let err = BrowserError::navigation_failed("Connection refused");
        assert_eq!(err.to_string(), "Navigation failed: Connection refused");

        let err = BrowserError::timeout(".loading");
        assert_eq!(err.to_string(), "Timeout waiting for element: .loading");
    }

    #[test]
    fn test_error_debug() {
        let err = BrowserError::NoPageOpen;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("NoPageOpen"));
    }
}
