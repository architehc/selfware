//! Browser Automation Tools
//!
//! Tools for web automation using headless browsers.
//! Supports Chromium via chrome/chromium CLI or playwright if available.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::process::Command;

use super::Tool;

// ============================================================================
// Browser Detection
// ============================================================================

/// Detected browser for automation
#[derive(Debug, Clone)]
pub enum BrowserType {
    Chrome(String), // Path to chrome/chromium
    Playwright,     // Use playwright CLI
    Curl,           // Fallback to curl for simple fetches
}

/// Detect available browser for automation
async fn detect_browser() -> Result<BrowserType> {
    // Try Chrome/Chromium first
    for browser in &[
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ] {
        if Command::new(browser)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
        {
            return Ok(BrowserType::Chrome(browser.to_string()));
        }
    }

    // Try playwright
    if Command::new("npx")
        .args(["playwright", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(BrowserType::Playwright);
    }

    // Fallback to curl
    if Command::new("curl")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return Ok(BrowserType::Curl);
    }

    Err(anyhow::anyhow!(
        "No browser automation tool found. Install Chrome, Chromium, or Playwright."
    ))
}

// ============================================================================
// Browser Fetch - Get page content
// ============================================================================

/// Fetch a web page and return its content
pub struct BrowserFetch;

#[async_trait]
impl Tool for BrowserFetch {
    fn name(&self) -> &str {
        "browser_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page and return its HTML content (uses headless browser or curl)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch"
                },
                "wait_for": {
                    "type": "string",
                    "description": "CSS selector to wait for before returning (Chrome only)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                },
                "javascript": {
                    "type": "boolean",
                    "description": "Enable JavaScript rendering (default: true for Chrome, false for curl)"
                },
                "user_agent": {
                    "type": "string",
                    "description": "Custom user agent string"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("url is required"))?;

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);
        let user_agent = args.get("user_agent").and_then(|v| v.as_str());

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                fetch_with_chrome(&chrome_path, url, timeout_secs, user_agent, &args).await
            }
            BrowserType::Playwright => {
                fetch_with_playwright(url, timeout_secs, user_agent, &args).await
            }
            BrowserType::Curl => fetch_with_curl(url, timeout_secs, user_agent).await,
        }
    }
}

async fn fetch_with_chrome(
    chrome_path: &str,
    url: &str,
    timeout_secs: u64,
    user_agent: Option<&str>,
    args: &Value,
) -> Result<Value> {
    let _wait_for = args.get("wait_for").and_then(|v| v.as_str());

    // Create a temporary file for output
    let temp_dir = std::env::temp_dir();
    let output_file = temp_dir.join(format!("browser_fetch_{}.html", std::process::id()));

    let mut cmd = Command::new(chrome_path);
    cmd.args([
        "--headless",
        "--disable-gpu",
        "--no-sandbox",
        "--disable-dev-shm-usage",
        "--disable-extensions",
        &format!("--timeout={}", timeout_secs * 1000),
    ]);

    if let Some(ua) = user_agent {
        cmd.arg(format!("--user-agent={}", ua));
    }

    // Use dump-dom to get rendered HTML
    cmd.arg("--dump-dom");
    cmd.arg(url);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs + 10),
        cmd.output(),
    )
    .await
    .context("Browser fetch timed out")?
    .context("Failed to run Chrome")?;

    let html = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Clean up temp file if it exists
    let _ = tokio::fs::remove_file(&output_file).await;

    // Extract text content from HTML for easier processing
    let text_content = extract_text_from_html(&html);

    Ok(json!({
        "success": output.status.success(),
        "browser": "chrome",
        "url": url,
        "html": truncate_output(&html, 10000),
        "text": truncate_output(&text_content, 5000),
        "html_length": html.len(),
        "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
    }))
}

/// Escape a string for safe embedding in a JavaScript single-quoted string literal.
fn escape_js_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '\\' => escaped.push_str("\\\\"),
            '\'' => escaped.push_str("\\'"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            '`' => escaped.push_str("\\`"),
            '$' => escaped.push_str("\\$"),
            other => escaped.push(other),
        }
    }
    escaped
}

async fn fetch_with_playwright(
    url: &str,
    timeout_secs: u64,
    user_agent: Option<&str>,
    _args: &Value,
) -> Result<Value> {
    let safe_url = escape_js_string(url);
    let ua_option = user_agent
        .map(|ua| format!("userAgent: '{}'", escape_js_string(ua)))
        .unwrap_or_default();
    let script = format!(
        r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const browser = await chromium.launch({{ headless: true }});
    const context = await browser.newContext({{
        {}
    }});
    const page = await context.newPage();
    await page.goto('{}', {{ timeout: {} }});
    const html = await page.content();
    console.log(html);
    await browser.close();
}})();
"#,
        ua_option,
        safe_url,
        timeout_secs * 1000
    );

    let mut cmd = Command::new("node");
    cmd.arg("-e");
    cmd.arg(&script);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs + 10),
        cmd.output(),
    )
    .await
    .context("Playwright fetch timed out")?
    .context("Failed to run Playwright")?;

    let html = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text_content = extract_text_from_html(&html);

    Ok(json!({
        "success": output.status.success(),
        "browser": "playwright",
        "url": url,
        "html": truncate_output(&html, 10000),
        "text": truncate_output(&text_content, 5000),
        "html_length": html.len(),
        "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
    }))
}

async fn fetch_with_curl(url: &str, timeout_secs: u64, user_agent: Option<&str>) -> Result<Value> {
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-L", // Follow redirects
        "--max-time",
        &timeout_secs.to_string(),
    ]);

    if let Some(ua) = user_agent {
        cmd.args(["-A", ua]);
    } else {
        cmd.args(["-A", "Mozilla/5.0 (compatible; Selfware/1.0)"]);
    }

    cmd.arg(url);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().await.context("Failed to run curl")?;

    let html = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text_content = extract_text_from_html(&html);

    Ok(json!({
        "success": output.status.success(),
        "browser": "curl",
        "url": url,
        "html": truncate_output(&html, 10000),
        "text": truncate_output(&text_content, 5000),
        "html_length": html.len(),
        "note": "JavaScript not rendered (curl fallback)",
        "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
    }))
}

// ============================================================================
// Browser Screenshot
// ============================================================================

/// Take a screenshot of a web page
pub struct BrowserScreenshot;

#[async_trait]
impl Tool for BrowserScreenshot {
    fn name(&self) -> &str {
        "browser_screenshot"
    }

    fn description(&self) -> &str {
        "Take a screenshot of a web page (requires Chrome/Chromium)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to screenshot"
                },
                "output_path": {
                    "type": "string",
                    "description": "Path to save screenshot (default: /tmp/screenshot.png)"
                },
                "width": {
                    "type": "integer",
                    "description": "Viewport width (default: 1920)"
                },
                "height": {
                    "type": "integer",
                    "description": "Viewport height (default: 1080)"
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full page (default: false)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("url is required"))?;

        let output_path = args
            .get("output_path")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp/screenshot.png");

        let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(1920);
        let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(1080);
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                let mut cmd = Command::new(&chrome_path);
                cmd.args([
                    "--headless",
                    "--disable-gpu",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                    &format!("--window-size={},{}", width, height),
                    &format!("--screenshot={}", output_path),
                    &format!("--timeout={}", timeout_secs * 1000),
                    url,
                ]);

                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let output = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs + 10),
                    cmd.output(),
                )
                .await
                .context("Screenshot timed out")?
                .context("Failed to take screenshot")?;

                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                // Check if file was created
                let file_exists = tokio::fs::metadata(output_path).await.is_ok();
                let file_size = if file_exists {
                    tokio::fs::metadata(output_path).await.ok().map(|m| m.len())
                } else {
                    None
                };

                Ok(json!({
                    "success": output.status.success() && file_exists,
                    "browser": "chrome",
                    "url": url,
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "dimensions": format!("{}x{}", width, height),
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            BrowserType::Playwright => {
                let safe_url = escape_js_string(url);
                let safe_output_path = escape_js_string(output_path);
                let full_page = args
                    .get("full_page")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let script = format!(
                    r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const browser = await chromium.launch({{ headless: true }});
    const page = await browser.newPage({{ viewport: {{ width: {}, height: {} }} }});
    await page.goto('{}', {{ timeout: {} }});
    await page.screenshot({{ path: '{}', fullPage: {} }});
    await browser.close();
    console.log('Screenshot saved');
}})();
"#,
                    width,
                    height,
                    safe_url,
                    timeout_secs * 1000,
                    safe_output_path,
                    full_page
                );

                let mut cmd = Command::new("node");
                cmd.arg("-e");
                cmd.arg(&script);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let output = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs + 10),
                    cmd.output(),
                )
                .await
                .context("Screenshot timed out")?
                .context("Failed to take screenshot")?;

                let file_exists = tokio::fs::metadata(output_path).await.is_ok();
                let file_size = if file_exists {
                    tokio::fs::metadata(output_path).await.ok().map(|m| m.len())
                } else {
                    None
                };

                Ok(json!({
                    "success": output.status.success() && file_exists,
                    "browser": "playwright",
                    "url": url,
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "dimensions": format!("{}x{}", width, height)
                }))
            }
            BrowserType::Curl => Err(anyhow::anyhow!(
                "Screenshots require Chrome or Playwright. Curl cannot take screenshots."
            )),
        }
    }
}

// ============================================================================
// Browser PDF
// ============================================================================

/// Save a web page as PDF
pub struct BrowserPdf;

#[async_trait]
impl Tool for BrowserPdf {
    fn name(&self) -> &str {
        "browser_pdf"
    }

    fn description(&self) -> &str {
        "Save a web page as PDF (requires Chrome/Chromium)"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to save as PDF"
                },
                "output_path": {
                    "type": "string",
                    "description": "Path to save PDF (default: /tmp/page.pdf)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("url is required"))?;

        let output_path = args
            .get("output_path")
            .and_then(|v| v.as_str())
            .unwrap_or("/tmp/page.pdf");

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                let mut cmd = Command::new(&chrome_path);
                cmd.args([
                    "--headless",
                    "--disable-gpu",
                    "--no-sandbox",
                    "--disable-dev-shm-usage",
                    &format!("--print-to-pdf={}", output_path),
                    &format!("--timeout={}", timeout_secs * 1000),
                    url,
                ]);

                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let output = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs + 10),
                    cmd.output(),
                )
                .await
                .context("PDF generation timed out")?
                .context("Failed to generate PDF")?;

                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                let file_exists = tokio::fs::metadata(output_path).await.is_ok();
                let file_size = if file_exists {
                    tokio::fs::metadata(output_path).await.ok().map(|m| m.len())
                } else {
                    None
                };

                Ok(json!({
                    "success": output.status.success() && file_exists,
                    "browser": "chrome",
                    "url": url,
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            _ => Err(anyhow::anyhow!("PDF generation requires Chrome/Chromium")),
        }
    }
}

// ============================================================================
// Browser Execute JavaScript
// ============================================================================

/// Execute JavaScript on a page and return result
pub struct BrowserEval;

#[async_trait]
impl Tool for BrowserEval {
    fn name(&self) -> &str {
        "browser_eval"
    }

    fn description(&self) -> &str {
        "Load a page and execute JavaScript, returning the result"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to load"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript code to execute (should return a value)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                }
            },
            "required": ["url", "script"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("url is required"))?;

        let script = args
            .get("script")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("script is required"))?;

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Playwright => {
                // Pass user script as a JSON-encoded string and evaluate via
                // new Function() to avoid interpolating untrusted code into the
                // Node source template.
                let safe_url = escape_js_string(url);
                let script_json = serde_json::to_string(script)
                    .context("Failed to JSON-encode script")?;

                let node_script = format!(
                    r#"
const {{ chromium }} = require('playwright');
(async () => {{
    const browser = await chromium.launch({{ headless: true }});
    const page = await browser.newPage();
    await page.goto('{}', {{ timeout: {} }});
    const userScript = {};
    const result = await page.evaluate((s) => {{
        return new Function(s)();
    }}, userScript);
    console.log(JSON.stringify(result));
    await browser.close();
}})();
"#,
                    safe_url,
                    timeout_secs * 1000,
                    script_json
                );

                let mut cmd = Command::new("node");
                cmd.arg("-e");
                cmd.arg(&node_script);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let output = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs + 10),
                    cmd.output(),
                )
                .await
                .context("Script execution timed out")?
                .context("Failed to execute script")?;

                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

                // Try to parse result as JSON
                let result: Value = serde_json::from_str(stdout.trim()).unwrap_or(json!(stdout.trim()));

                Ok(json!({
                    "success": output.status.success(),
                    "browser": "playwright",
                    "url": url,
                    "result": result,
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            BrowserType::Chrome(_) => {
                Err(anyhow::anyhow!(
                    "JavaScript evaluation requires Playwright. Chrome headless has limited eval support."
                ))
            }
            BrowserType::Curl => {
                Err(anyhow::anyhow!("JavaScript evaluation requires a browser (Playwright recommended)"))
            }
        }
    }
}

// ============================================================================
// Browser Extract Links
// ============================================================================

/// Extract all links from a web page
pub struct BrowserLinks;

#[async_trait]
impl Tool for BrowserLinks {
    fn name(&self) -> &str {
        "browser_links"
    }

    fn description(&self) -> &str {
        "Extract all links from a web page"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to extract links from"
                },
                "filter": {
                    "type": "string",
                    "description": "Filter links containing this string"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        // First fetch the page
        let fetch_tool = BrowserFetch;
        let fetch_result = fetch_tool.execute(args.clone()).await?;

        let html = fetch_result
            .get("html")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let filter = args.get("filter").and_then(|v| v.as_str());

        // Extract links using regex
        let link_regex = regex::Regex::new(r#"href=["']([^"']+)["']"#).unwrap();
        let mut links: Vec<String> = link_regex
            .captures_iter(html)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .filter(|link| {
                if let Some(f) = filter {
                    link.contains(f)
                } else {
                    true
                }
            })
            .collect();

        // Deduplicate
        links.sort();
        links.dedup();

        Ok(json!({
            "success": fetch_result.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
            "url": args.get("url").and_then(|v| v.as_str()),
            "links": links,
            "count": links.len(),
            "filter": filter
        }))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Extract text content from HTML (simple implementation)
fn extract_text_from_html(html: &str) -> String {
    // Remove script and style tags
    let script_regex = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    let style_regex = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    let tag_regex = regex::Regex::new(r"<[^>]+>").unwrap();
    let whitespace_regex = regex::Regex::new(r"\s+").unwrap();

    let text = script_regex.replace_all(html, "");
    let text = style_regex.replace_all(&text, "");
    let text = tag_regex.replace_all(&text, " ");
    let text = whitespace_regex.replace_all(&text, " ");

    // Decode common HTML entities
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// Truncate output to max length
fn truncate_output(output: &str, max_len: usize) -> String {
    if output.len() <= max_len {
        output.to_string()
    } else {
        format!(
            "{}... [truncated, {} total chars]",
            &output[..max_len],
            output.len()
        )
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_browser_fetch_schema() {
        let tool = BrowserFetch;
        let schema = tool.schema();
        assert!(schema["properties"].get("url").is_some());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!("url")));
    }

    #[test]
    fn test_browser_screenshot_schema() {
        let tool = BrowserScreenshot;
        let schema = tool.schema();
        assert!(schema["properties"].get("url").is_some());
        assert!(schema["properties"].get("output_path").is_some());
        assert!(schema["properties"].get("width").is_some());
    }

    #[test]
    fn test_browser_eval_schema() {
        let tool = BrowserEval;
        let schema = tool.schema();
        assert!(schema["properties"].get("url").is_some());
        assert!(schema["properties"].get("script").is_some());
    }

    #[test]
    fn test_tool_names() {
        assert_eq!(BrowserFetch.name(), "browser_fetch");
        assert_eq!(BrowserScreenshot.name(), "browser_screenshot");
        assert_eq!(BrowserPdf.name(), "browser_pdf");
        assert_eq!(BrowserEval.name(), "browser_eval");
        assert_eq!(BrowserLinks.name(), "browser_links");
    }

    #[test]
    fn test_tool_descriptions() {
        assert!(!BrowserFetch.description().is_empty());
        assert!(BrowserFetch.description().contains("web page"));
        assert!(BrowserScreenshot.description().contains("screenshot"));
    }

    #[test]
    fn test_extract_text_from_html() {
        let html =
            "<html><body><script>alert('hi')</script><p>Hello <b>World</b></p></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World"));
        assert!(!text.contains("alert"));
        assert!(!text.contains("<"));
    }

    #[test]
    fn test_extract_text_entities() {
        let html = "<p>Hello &amp; World &lt;test&gt;</p>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello & World <test>"));
    }

    #[test]
    fn test_truncate_output() {
        let short = "hello";
        assert_eq!(truncate_output(short, 100), short);

        let long = "a".repeat(200);
        let result = truncate_output(&long, 50);
        assert!(result.contains("truncated"));
        assert!(result.len() < 200);
    }

    #[tokio::test]
    async fn test_browser_fetch_no_url() {
        let tool = BrowserFetch;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url is required"));
    }

    #[tokio::test]
    async fn test_browser_eval_no_script() {
        let tool = BrowserEval;
        let result = tool.execute(json!({"url": "http://example.com"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("script is required"));
    }

    #[test]
    fn test_browser_pdf_schema() {
        let tool = BrowserPdf;
        let schema = tool.schema();
        assert!(schema["properties"].get("url").is_some());
        assert!(schema["properties"].get("output_path").is_some());
        assert!(schema["properties"].get("timeout_secs").is_some());
    }

    #[test]
    fn test_browser_links_schema() {
        let tool = BrowserLinks;
        let schema = tool.schema();
        assert!(schema["properties"].get("url").is_some());
        assert!(schema["properties"].get("filter").is_some());
    }

    #[test]
    fn test_extract_text_removes_scripts() {
        let html = "<html><script>var x = 1;</script><body>Hello</body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello"));
        assert!(!text.contains("var x"));
    }

    #[test]
    fn test_extract_text_removes_styles() {
        let html = "<html><style>.foo { color: red; }</style><body>World</body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("World"));
        assert!(!text.contains("color"));
    }

    #[test]
    fn test_extract_text_preserves_content() {
        let html = "<div><p>Paragraph 1</p><p>Paragraph 2</p></div>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Paragraph 1"));
        assert!(text.contains("Paragraph 2"));
    }

    #[test]
    fn test_extract_text_entity_nbsp() {
        let html = "Hello&nbsp;World";
        let text = extract_text_from_html(html);
        assert!(text.contains("Hello World"));
    }

    #[test]
    fn test_extract_text_entity_quote() {
        let html = "&quot;quoted&quot;";
        let text = extract_text_from_html(html);
        assert!(text.contains("\"quoted\""));
    }

    #[test]
    fn test_extract_text_entity_apostrophe() {
        let html = "it&#39;s";
        let text = extract_text_from_html(html);
        assert!(text.contains("it's"));
    }

    #[test]
    fn test_truncate_output_exact() {
        let s = "12345";
        assert_eq!(truncate_output(s, 5), "12345");
    }

    #[test]
    fn test_truncate_output_with_info() {
        let s = "a".repeat(100);
        let result = truncate_output(&s, 20);
        assert!(result.contains("100 total chars"));
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_browser_type_debug() {
        let chrome = BrowserType::Chrome("/usr/bin/chrome".to_string());
        let debug_str = format!("{:?}", chrome);
        assert!(debug_str.contains("Chrome"));

        let playwright = BrowserType::Playwright;
        let debug_str = format!("{:?}", playwright);
        assert!(debug_str.contains("Playwright"));

        let curl = BrowserType::Curl;
        let debug_str = format!("{:?}", curl);
        assert!(debug_str.contains("Curl"));
    }

    #[test]
    fn test_browser_type_clone() {
        let chrome = BrowserType::Chrome("/usr/bin/chrome".to_string());
        let cloned = chrome.clone();
        if let BrowserType::Chrome(path) = cloned {
            assert_eq!(path, "/usr/bin/chrome");
        } else {
            panic!("Clone should preserve variant");
        }
    }

    #[tokio::test]
    async fn test_browser_screenshot_no_url() {
        let tool = BrowserScreenshot;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url is required"));
    }

    #[tokio::test]
    async fn test_browser_pdf_no_url() {
        let tool = BrowserPdf;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url is required"));
    }

    #[tokio::test]
    async fn test_browser_eval_no_url() {
        let tool = BrowserEval;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("url is required"));
    }

    #[test]
    fn test_all_tool_descriptions_non_empty() {
        assert!(!BrowserFetch.description().is_empty());
        assert!(!BrowserScreenshot.description().is_empty());
        assert!(!BrowserPdf.description().is_empty());
        assert!(!BrowserEval.description().is_empty());
        assert!(!BrowserLinks.description().is_empty());
    }

    #[test]
    fn test_browser_fetch_schema_complete() {
        let tool = BrowserFetch;
        let schema = tool.schema();
        assert!(schema["properties"].get("wait_for").is_some());
        assert!(schema["properties"].get("timeout_secs").is_some());
        assert!(schema["properties"].get("javascript").is_some());
        assert!(schema["properties"].get("user_agent").is_some());
    }

    #[test]
    fn test_browser_screenshot_schema_complete() {
        let tool = BrowserScreenshot;
        let schema = tool.schema();
        assert!(schema["properties"].get("height").is_some());
        assert!(schema["properties"].get("full_page").is_some());
        assert!(schema["properties"].get("timeout_secs").is_some());
    }

    #[test]
    fn test_extract_text_empty_html() {
        let text = extract_text_from_html("");
        assert!(text.is_empty());
    }

    #[test]
    fn test_extract_text_whitespace_collapse() {
        let html = "Hello     World\n\n\nTest";
        let text = extract_text_from_html(html);
        // Multiple whitespaces should be collapsed
        assert!(!text.contains("     "));
    }

    #[test]
    fn test_truncate_output_empty() {
        assert_eq!(truncate_output("", 10), "");
    }

    #[test]
    fn test_extract_text_nested_tags() {
        let html = "<div><span><b><i>Nested</i></b></span></div>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Nested"));
    }

    #[test]
    fn test_extract_text_multiline_script() {
        let html = r#"<script>
            function test() {
                return "hidden";
            }
        </script>
        <p>Visible</p>"#;
        let text = extract_text_from_html(html);
        assert!(text.contains("Visible"));
        assert!(!text.contains("hidden"));
        assert!(!text.contains("function"));
    }
}
