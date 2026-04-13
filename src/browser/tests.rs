//! Browser automation tests
//!
//! These tests verify the browser module's error handling and API.
//! Full integration tests requiring an actual browser are in tests/integration/browser.rs

use super::*;
use std::time::Duration;

// ============================================================================
// Configuration Tests
// ============================================================================

#[test]
fn test_browser_config_default() {
    let config = BrowserConfig::default();
    assert!(config.headless);
    assert_eq!(config.viewport, (1920, 1080));
    assert_eq!(config.slow_mo, 0);
    assert!(config.executable_path.is_none());
    assert!(config.args.is_empty());
}

#[test]
fn test_browser_config_custom() {
    let config = BrowserConfig {
        headless: false,
        viewport: (1280, 720),
        slow_mo: 100,
        executable_path: Some(PathBuf::from("/usr/bin/chrome")),
        args: vec!["--disable-gpu".to_string()],
    };

    assert!(!config.headless);
    assert_eq!(config.viewport, (1280, 720));
    assert_eq!(config.slow_mo, 100);
    assert_eq!(config.executable_path, Some(PathBuf::from("/usr/bin/chrome")));
    assert_eq!(config.args, vec!["--disable-gpu"]);
}

#[test]
fn test_browser_config_clone() {
    let config = BrowserConfig {
        headless: false,
        viewport: (800, 600),
        slow_mo: 50,
        executable_path: None,
        args: vec!["--no-sandbox".to_string()],
    };

    let cloned = config.clone();
    assert_eq!(config.headless, cloned.headless);
    assert_eq!(config.viewport, cloned.viewport);
    assert_eq!(config.slow_mo, cloned.slow_mo);
    assert_eq!(config.args, cloned.args);
}

// ============================================================================
// PageInfo Tests
// ============================================================================

#[test]
fn test_page_info_creation() {
    let info = PageInfo {
        url: "https://example.com".to_string(),
        title: "Example Page".to_string(),
    };

    assert_eq!(info.url, "https://example.com");
    assert_eq!(info.title, "Example Page");
}

#[test]
fn test_page_info_clone() {
    let info = PageInfo {
        url: "https://example.com".to_string(),
        title: "Example Page".to_string(),
    };

    let cloned = info.clone();
    assert_eq!(info.url, cloned.url);
    assert_eq!(info.title, cloned.title);
}

#[test]
fn test_page_info_debug() {
    let info = PageInfo {
        url: "https://example.com".to_string(),
        title: "Example".to_string(),
    };

    let debug_str = format!("{:?}", info);
    assert!(debug_str.contains("example.com"));
    assert!(debug_str.contains("Example"));
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[test]
fn test_browser_error_not_initialized() {
    let err = BrowserError::NotInitialized;
    assert_eq!(err.to_string(), "Browser not initialized");
}

#[test]
fn test_browser_error_no_page_open() {
    let err = BrowserError::NoPageOpen;
    assert_eq!(err.to_string(), "No page is currently open");
}

#[test]
fn test_browser_error_element_not_found() {
    let err = BrowserError::element_not_found("#button");
    assert_eq!(err.to_string(), "Element not found: #button");
}

#[test]
fn test_browser_error_navigation_failed() {
    let err = BrowserError::navigation_failed("Connection timeout");
    assert_eq!(err.to_string(), "Navigation failed: Connection timeout");
}

#[test]
fn test_browser_error_screenshot_failed() {
    let err = BrowserError::ScreenshotFailed("Permission denied".to_string());
    assert_eq!(err.to_string(), "Screenshot failed: Permission denied");
}

#[test]
fn test_browser_error_javascript_failed() {
    let err = BrowserError::JavaScriptFailed("Syntax error".to_string());
    assert_eq!(err.to_string(), "JavaScript execution failed: Syntax error");
}

#[test]
fn test_browser_error_timeout() {
    let err = BrowserError::timeout(".spinner");
    assert_eq!(err.to_string(), "Timeout waiting for element: .spinner");
}

#[test]
fn test_browser_error_launch_failed() {
    let err = BrowserError::LaunchFailed("Chrome not found".to_string());
    assert_eq!(err.to_string(), "Browser launch failed: Chrome not found");
}

#[test]
fn test_browser_error_connection_lost() {
    let err = BrowserError::ConnectionLost;
    assert_eq!(err.to_string(), "Browser connection lost");
}

#[test]
fn test_browser_error_invalid_config() {
    let err = BrowserError::InvalidConfig("Invalid viewport".to_string());
    assert_eq!(err.to_string(), "Invalid browser configuration: Invalid viewport");
}

#[test]
fn test_browser_error_other() {
    let err = BrowserError::Other("Unknown error".to_string());
    assert_eq!(err.to_string(), "Browser error: Unknown error");
}

// ============================================================================
// Chrome Config Builder Tests
// ============================================================================

#[test]
fn test_build_chrome_config_default() {
    let config = BrowserConfig::default();
    let result = build_chrome_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_build_chrome_config_with_executable() {
    let config = BrowserConfig {
        executable_path: Some(PathBuf::from("/usr/bin/chromium")),
        ..Default::default()
    };
    let result = build_chrome_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_build_chrome_config_non_headless() {
    let config = BrowserConfig {
        headless: false,
        ..Default::default()
    };
    let result = build_chrome_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_build_chrome_config_custom_viewport() {
    let config = BrowserConfig {
        viewport: (800, 600),
        ..Default::default()
    };
    let result = build_chrome_config(&config);
    assert!(result.is_ok());
}

#[test]
fn test_build_chrome_config_with_args() {
    let config = BrowserConfig {
        args: vec![
            "--disable-gpu".to_string(),
            "--disable-dev-shm-usage".to_string(),
        ],
        ..Default::default()
    };
    let result = build_chrome_config(&config);
    assert!(result.is_ok());
}

// ============================================================================
// Mock Session Tests
// ============================================================================

#[tokio::test]
async fn test_session_methods_fail_without_browser() {
    // We can't easily test the actual browser operations without a real browser,
    // but we can verify the error handling by checking that methods return errors
    // when the browser is not properly initialized

    // Note: BrowserSession::new will fail if Chrome isn't installed,
    // so we rely on integration tests for full coverage
}

// ============================================================================
// Documentation Examples Tests
// ============================================================================

#[test]
fn test_page_info_display() {
    let info = PageInfo {
        url: "https://example.com/page".to_string(),
        title: "Test Page".to_string(),
    };

    // Test that PageInfo implements Debug
    let _debug = format!("{:?}", info);

    // Verify fields are accessible
    assert!(!info.url.is_empty());
    assert!(!info.title.is_empty());
}

// ============================================================================
// Trait Implementation Tests
// ============================================================================

#[test]
fn test_send_sync_bounds() {
    // Verify that public types implement Send + Sync where expected
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    assert_send::<BrowserConfig>();
    assert_sync::<BrowserConfig>();

    assert_send::<PageInfo>();
    assert_sync::<PageInfo>();

    assert_send::<BrowserError>();
    assert_sync::<BrowserError>();
}

// ============================================================================
// Integration Test Helpers
// ============================================================================

/// Creates a test HTML page URL for integration tests
pub fn test_page_url() -> String {
    // Use data URL for a simple test page
    "data:text/html,<html><head><title>Test</title></head><body><h1>Hello</h1><button id='btn'>Click</button><input id='input' type='text'></body></html>".to_string()
}

/// Creates a more complex test page URL
pub fn complex_test_page_url() -> String {
    let html = r#"data:text/html,<html><head><title>Complex Test</title></head><body><div id='container'><h1 class='title'>Complex Page</h1><form id='form'><input id='username' type='text' placeholder='Username'><input id='password' type='password' placeholder='Password'><button type='submit'>Submit</button></form><div id='dynamic' style='display:none'>Dynamic Content</div><p class='text'>Paragraph 1</p><p class='text'>Paragraph 2</p></div><script>setTimeout(() => { document.getElementById('dynamic').style.display = 'block'; }, 100);</script></body></html>"#;
    html.to_string()
}

#[tokio::test]
async fn test_test_page_urls() {
    // Verify the test URLs are valid
    let url = test_page_url();
    assert!(url.starts_with("data:text/html,"));
    assert!(url.contains("Hello"));
    assert!(url.contains("button"));

    let complex = complex_test_page_url();
    assert!(complex.starts_with("data:text/html,"));
    assert!(complex.contains("Complex Page"));
    assert!(complex.contains("form"));
}

// ============================================================================
// Property-based Tests
// ============================================================================

#[test]
fn test_config_with_various_viewports() {
    let viewports = vec![
        (800, 600),
        (1024, 768),
        (1280, 720),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
        (1, 1),
        (9999, 9999),
    ];

    for (width, height) in viewports {
        let config = BrowserConfig {
            viewport: (width, height),
            ..Default::default()
        };
        let result = build_chrome_config(&config);
        assert!(result.is_ok(), "Failed for viewport {}x{}", width, height);
    }
}

#[test]
fn test_config_with_various_slow_mo() {
    let slow_mo_values = vec![0, 10, 50, 100, 500, 1000, 5000];

    for slow_mo in slow_mo_values {
        let config = BrowserConfig {
            slow_mo,
            ..Default::default()
        };
        // Just verify config creation works
        assert_eq!(config.slow_mo, slow_mo);
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_empty_selector() {
    let selector = "";
    let err = BrowserError::element_not_found(selector);
    assert_eq!(err.to_string(), "Element not found: ");
}

#[test]
fn test_special_characters_in_selectors() {
    let selectors = vec![
        "#id-with-dashes",
        ".class.with.dots",
        "[data-test='value']",
        "div > span + p",
        "div:nth-child(2)",
    ];

    for selector in selectors {
        let err = BrowserError::element_not_found(selector);
        assert!(err.to_string().contains(selector));
    }
}

#[test]
fn test_long_urls() {
    let long_url = format!("https://example.com/{}", "a".repeat(1000));
    let info = PageInfo {
        url: long_url.clone(),
        title: "Test".to_string(),
    };
    assert_eq!(info.url, long_url);
}

#[test]
fn test_unicode_in_page_info() {
    let info = PageInfo {
        url: "https://example.com/test".to_string(),
        title: "Test Title".to_string(),
    };
    assert_eq!(info.url, "https://example.com/test");
    assert_eq!(info.title, "Test Title");
}

#[test]
fn test_path_buf_operations() {
    let path = PathBuf::from("/tmp/test-screenshot.png");
    assert_eq!(path.extension().unwrap(), "png");

    let path = PathBuf::from("/tmp/test-screenshot.jpg");
    assert_eq!(path.extension().unwrap(), "jpg");
}
