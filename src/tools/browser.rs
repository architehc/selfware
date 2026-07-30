//! Browser Automation Tools
//!
//! Tools for web automation using headless browsers.
//! Supports Chromium via chrome/chromium CLI or playwright if available.

use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command;

use super::Tool;
use crate::config::is_local_endpoint;

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

#[derive(Debug, Clone)]
struct PinnedTarget {
    url: String,
    host: String,
    port: u16,
    ip: IpAddr,
    host_is_ip: bool,
    resolver_rule: String,
}

/// Detect available browser for automation
async fn detect_browser() -> Result<BrowserType> {
    if playwright_runtime_available().await {
        return Ok(BrowserType::Playwright);
    }

    // Try Chrome/Chromium first
    for browser in chrome_candidates() {
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
        "browser automation unavailable: no Playwright/Chrome backend found; \
         install a browser backend to use browser_* tools"
    ))
}

fn chrome_candidates() -> &'static [&'static str] {
    &[
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ]
}

fn playwright_chromium_executable() -> Option<String> {
    for env_key in [
        "SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH",
        "SELFWARE_CHROME_EXECUTABLE_PATH",
    ] {
        if let Ok(path) = std::env::var(env_key) {
            if Path::new(&path).exists() {
                return Some(path);
            }
        }
    }

    chrome_candidates()
        .iter()
        .find(|candidate| Path::new(candidate).exists())
        .map(|candidate| (*candidate).to_string())
}

fn configure_playwright_command(cmd: &mut Command) {
    if let Ok(node_path) = std::env::var("SELFWARE_PLAYWRIGHT_NODE_PATH") {
        let merged = match std::env::var("NODE_PATH") {
            Ok(existing) if !existing.is_empty() => format!("{}:{}", node_path, existing),
            _ => node_path,
        };
        cmd.env("NODE_PATH", merged);
    }

    if let Some(executable) = playwright_chromium_executable() {
        cmd.env("SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH", executable);
    }
}

async fn playwright_runtime_available() -> bool {
    let mut cmd = Command::new("node");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    configure_playwright_command(&mut cmd);
    cmd.args([
        "-e",
        "try { require('playwright'); console.log('playwright-ok'); } catch (_) { try { require('playwright-core'); console.log('playwright-core-ok'); } catch (err) { process.exit(1); } }",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    cmd.status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

fn playwright_launch_prelude(extra_args: &str) -> String {
    format!(
        r#"
const fs = require('fs');
let pw;
try {{
    pw = require('playwright');
}} catch (_) {{
    pw = require('playwright-core');
}}
const executablePath = process.env.SELFWARE_PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
const launchOptions = {{ headless: true, args: {} }};
if (executablePath && fs.existsSync(executablePath)) {{
    launchOptions.executablePath = executablePath;
}}
const browser = await pw.chromium.launch(launchOptions);
"#,
        extra_args
    )
}

fn should_stage_chrome_output_for_home(output_path: &Path, home_dir: Option<&Path>) -> bool {
    output_path.is_absolute() && home_dir.is_some_and(|home| !output_path.starts_with(home))
}

fn should_stage_chrome_output(output_path: &Path) -> bool {
    let home_dir = std::env::var_os("HOME").map(PathBuf::from);
    should_stage_chrome_output_for_home(output_path, home_dir.as_deref())
}

fn chrome_staging_output_path(requested_output: &Path) -> Result<PathBuf> {
    let base_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| {
            anyhow::anyhow!("Failed to determine a staging directory for Chrome output")
        })?;

    let file_name = requested_output
        .file_name()
        .filter(|name| !name.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("browser-output"));

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    Ok(base_dir
        .join(".selfware")
        .join("browser-output")
        .join(format!("{}-{}", std::process::id(), unique))
        .join(file_name))
}

async fn prepare_chrome_output_path(output_path: &str) -> Result<(PathBuf, Option<PathBuf>)> {
    let requested_output = PathBuf::from(output_path);
    if !should_stage_chrome_output(&requested_output) {
        ensure_output_parent(&requested_output).await?;
        return Ok((requested_output, None));
    }

    let staged_output = chrome_staging_output_path(&requested_output)?;
    if let Some(parent) = staged_output.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create Chrome staging dir {}", parent.display()))?;
    }

    Ok((staged_output.clone(), Some(staged_output)))
}

fn validate_browser_output_path(output_path: &str, tool_name: &str) -> Result<()> {
    let safety = crate::tools::file::resolve_safety_config(None);
    crate::tools::file::validate_tool_path(output_path, &safety)
        .with_context(|| format!("{tool_name} output_path validation failed"))
}

async fn ensure_output_parent(output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("Failed to create browser output dir {}", parent.display()))?;
    }
    Ok(())
}

async fn finalize_chrome_output_path(
    requested_output: &str,
    staged_output: Option<&Path>,
) -> Result<(bool, Option<u64>)> {
    let requested_output = PathBuf::from(requested_output);

    if let Some(staged_output) = staged_output {
        if tokio::fs::metadata(staged_output).await.is_ok() {
            if let Some(parent) = requested_output
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
            {
                tokio::fs::create_dir_all(parent).await.with_context(|| {
                    format!("Failed to create browser output dir {}", parent.display())
                })?;
            }

            tokio::fs::copy(staged_output, &requested_output)
                .await
                .with_context(|| {
                    format!(
                        "Failed to copy staged browser output from {} to {}",
                        staged_output.display(),
                        requested_output.display()
                    )
                })?;

            let _ = tokio::fs::remove_file(staged_output).await;
        }
    }

    let file_exists = tokio::fs::metadata(&requested_output).await.is_ok();
    let file_size = if file_exists {
        tokio::fs::metadata(&requested_output)
            .await
            .ok()
            .map(|m| m.len())
    } else {
        None
    };

    Ok((file_exists, file_size))
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
        "Fetch a web page and return its HTML content (uses headless browser or curl). \
         Localhost/loopback URLs are allowed by default; set SELFWARE_ALLOW_PRIVATE_NETWORK=1 for private LAN hosts."
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
        let pinned_target = resolve_and_pin_target(url)?;

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                fetch_with_chrome(
                    &chrome_path,
                    &pinned_target,
                    timeout_secs,
                    user_agent,
                    &args,
                )
                .await
            }
            BrowserType::Playwright => {
                fetch_with_playwright(&pinned_target, timeout_secs, user_agent, &args).await
            }
            BrowserType::Curl => fetch_with_curl(&pinned_target, timeout_secs, user_agent).await,
        }
    }
}

async fn fetch_with_chrome(
    chrome_path: &str,
    target: &PinnedTarget,
    timeout_secs: u64,
    user_agent: Option<&str>,
    args: &Value,
) -> Result<Value> {
    let _wait_for = args.get("wait_for").and_then(|v| v.as_str());

    // Create a temporary file for output
    let temp_dir = std::env::temp_dir();
    let output_file = temp_dir.join(format!("browser_fetch_{}.html", std::process::id()));

    let no_sandbox = std::env::var("SELFWARE_BROWSER_NO_SANDBOX").unwrap_or_default() == "1";
    let mut cmd = Command::new(chrome_path);
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.args([
        "--headless",
        "--disable-gpu",
        "--disable-dev-shm-usage",
        "--disable-extensions",
        &format!("--timeout={}", timeout_secs * 1000),
    ]);
    if no_sandbox {
        cmd.arg("--no-sandbox");
    }

    if let Some(ua) = user_agent {
        cmd.arg(format!("--user-agent={}", ua));
    }
    if !target.host_is_ip {
        cmd.arg(format!("--host-resolver-rules={}", target.resolver_rule));
    }

    // Use dump-dom to get rendered HTML
    cmd.arg("--dump-dom");
    cmd.arg(&target.url);

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
        "url": target.url,
        "resolved_ip": target.ip.to_string(),
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
    target: &PinnedTarget,
    timeout_secs: u64,
    user_agent: Option<&str>,
    _args: &Value,
) -> Result<Value> {
    let safe_url = escape_js_string(&target.url);
    let ua_option = user_agent
        .map(|ua| format!("userAgent: '{}'", escape_js_string(ua)))
        .unwrap_or_default();
    let launch_args = if target.host_is_ip {
        "[]".to_string()
    } else {
        format!(
            "['--host-resolver-rules={}']",
            escape_js_string(&target.resolver_rule)
        )
    };
    let launch_prelude = playwright_launch_prelude(&launch_args);
    let script = format!(
        r#"
(async () => {{
    {}
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
        launch_prelude,
        ua_option,
        safe_url,
        timeout_secs * 1000
    );

    let mut cmd = Command::new("node");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    configure_playwright_command(&mut cmd);
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
        "url": target.url,
        "resolved_ip": target.ip.to_string(),
        "html": truncate_output(&html, 10000),
        "text": truncate_output(&text_content, 5000),
        "html_length": html.len(),
        "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
    }))
}

async fn fetch_with_curl(
    target: &PinnedTarget,
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<Value> {
    let mut cmd = Command::new("curl");
    crate::safety::process_env::sanitize_command_env(&mut cmd);
    cmd.args([
        "-s",
        "-L",
        "--max-redirs",
        "5",
        "--proto",
        "=https,http",
        "--max-time",
        &timeout_secs.to_string(),
    ]);
    if !target.host_is_ip {
        cmd.args([
            "--resolve",
            &format!("{}:{}:{}", target.host, target.port, target.ip),
        ]);
    }

    if let Some(ua) = user_agent {
        cmd.args(["-A", ua]);
    } else {
        cmd.args(["-A", "Mozilla/5.0 (compatible; Selfware/1.0)"]);
    }

    cmd.arg(&target.url);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().await.context("Failed to run curl")?;

    let html = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text_content = extract_text_from_html(&html);

    Ok(json!({
        "success": output.status.success(),
        "browser": "curl",
        "url": target.url,
        "resolved_ip": target.ip.to_string(),
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
                    "description": "Path to save screenshot (default: .selfware/browser-output/screenshot.png)"
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
        let pinned_target = resolve_and_pin_target(url)?;

        let output_path = args
            .get("output_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".selfware/browser-output/screenshot.png");
        validate_browser_output_path(output_path, self.name())?;
        let (chrome_output_path, staged_output_path) =
            prepare_chrome_output_path(output_path).await?;

        let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(1920);
        let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(1080);
        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                let no_sandbox =
                    std::env::var("SELFWARE_BROWSER_NO_SANDBOX").unwrap_or_default() == "1";
                let mut cmd = Command::new(&chrome_path);
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                cmd.args([
                    "--headless",
                    "--disable-gpu",
                    "--disable-dev-shm-usage",
                    &format!("--window-size={},{}", width, height),
                    &format!("--screenshot={}", chrome_output_path.display()),
                    &format!("--timeout={}", timeout_secs * 1000),
                ]);
                if no_sandbox {
                    cmd.arg("--no-sandbox");
                }
                if !pinned_target.host_is_ip {
                    cmd.arg(format!(
                        "--host-resolver-rules={}",
                        pinned_target.resolver_rule
                    ));
                }
                cmd.arg(&pinned_target.url);

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

                let (file_exists, file_size) =
                    finalize_chrome_output_path(output_path, staged_output_path.as_deref()).await?;

                Ok(json!({
                    "success": output.status.success() && file_exists,
                    "browser": "chrome",
                    "url": pinned_target.url,
                    "resolved_ip": pinned_target.ip.to_string(),
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "dimensions": format!("{}x{}", width, height),
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            BrowserType::Playwright => {
                let safe_url = escape_js_string(&pinned_target.url);
                let safe_output_path = escape_js_string(output_path);
                ensure_output_parent(Path::new(output_path)).await?;
                let full_page = args
                    .get("full_page")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let launch_args = if pinned_target.host_is_ip {
                    "[]".to_string()
                } else {
                    format!(
                        "['--host-resolver-rules={}']",
                        escape_js_string(&pinned_target.resolver_rule)
                    )
                };
                let launch_prelude = playwright_launch_prelude(&launch_args);
                let script = format!(
                    r#"
(async () => {{
    {}
    const page = await browser.newPage({{ viewport: {{ width: {}, height: {} }} }});
    await page.goto('{}', {{ timeout: {} }});
    await page.screenshot({{ path: '{}', fullPage: {} }});
    await browser.close();
    console.log('Screenshot saved');
}})();
"#,
                    launch_prelude,
                    width,
                    height,
                    safe_url,
                    timeout_secs * 1000,
                    safe_output_path,
                    full_page
                );

                let mut cmd = Command::new("node");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                configure_playwright_command(&mut cmd);
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
                    "url": pinned_target.url,
                    "resolved_ip": pinned_target.ip.to_string(),
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "dimensions": format!("{}x{}", width, height)
                }))
            }
            BrowserType::Curl => Err(anyhow::anyhow!(
                "browser automation unavailable: no Playwright/Chrome backend found; \
                 install a browser backend to use browser_screenshot"
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
                    "description": "Path to save PDF (default: .selfware/browser-output/page.pdf)"
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
        let pinned_target = resolve_and_pin_target(url)?;

        let output_path = args
            .get("output_path")
            .and_then(|v| v.as_str())
            .unwrap_or(".selfware/browser-output/page.pdf");
        validate_browser_output_path(output_path, self.name())?;
        let (chrome_output_path, staged_output_path) =
            prepare_chrome_output_path(output_path).await?;

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30);

        let browser = detect_browser().await?;

        match browser {
            BrowserType::Chrome(chrome_path) => {
                let no_sandbox =
                    std::env::var("SELFWARE_BROWSER_NO_SANDBOX").unwrap_or_default() == "1";
                let mut cmd = Command::new(&chrome_path);
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                cmd.args([
                    "--headless",
                    "--disable-gpu",
                    "--disable-dev-shm-usage",
                    &format!("--print-to-pdf={}", chrome_output_path.display()),
                    &format!("--timeout={}", timeout_secs * 1000),
                ]);
                if no_sandbox {
                    cmd.arg("--no-sandbox");
                }
                if !pinned_target.host_is_ip {
                    cmd.arg(format!(
                        "--host-resolver-rules={}",
                        pinned_target.resolver_rule
                    ));
                }
                cmd.arg(&pinned_target.url);

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
                let (file_exists, file_size) =
                    finalize_chrome_output_path(output_path, staged_output_path.as_deref()).await?;

                Ok(json!({
                    "success": output.status.success() && file_exists,
                    "browser": "chrome",
                    "url": pinned_target.url,
                    "resolved_ip": pinned_target.ip.to_string(),
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            BrowserType::Playwright => {
                let safe_url = escape_js_string(&pinned_target.url);
                let safe_output_path = escape_js_string(output_path);
                ensure_output_parent(Path::new(output_path)).await?;
                let launch_args = if pinned_target.host_is_ip {
                    "[]".to_string()
                } else {
                    format!(
                        "['--host-resolver-rules={}']",
                        escape_js_string(&pinned_target.resolver_rule)
                    )
                };
                let launch_prelude = playwright_launch_prelude(&launch_args);
                let script = format!(
                    r#"
(async () => {{
    {}
    const page = await browser.newPage();
    await page.goto('{}', {{ timeout: {} }});
    await page.pdf({{ path: '{}', format: 'A4' }});
    await browser.close();
    console.log('PDF saved');
}})();
"#,
                    launch_prelude,
                    safe_url,
                    timeout_secs * 1000,
                    safe_output_path
                );

                let mut cmd = Command::new("node");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                configure_playwright_command(&mut cmd);
                cmd.arg("-e");
                cmd.arg(&script);
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
                    "browser": "playwright",
                    "url": pinned_target.url,
                    "resolved_ip": pinned_target.ip.to_string(),
                    "output_path": output_path,
                    "file_exists": file_exists,
                    "file_size": file_size,
                    "stderr": if stderr.is_empty() { None } else { Some(truncate_output(&stderr, 500)) }
                }))
            }
            BrowserType::Curl => Err(anyhow::anyhow!(
                "browser automation unavailable: no Playwright/Chrome backend found; \
                 install a browser backend to use browser_pdf"
            )),
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
        let pinned_target = resolve_and_pin_target(url)?;

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
                let safe_url = escape_js_string(&pinned_target.url);
                let script_json = serde_json::to_string(script)
                    .context("Failed to JSON-encode script")?;
                let launch_args = if pinned_target.host_is_ip {
                    "[]".to_string()
                } else {
                    format!(
                        "['--host-resolver-rules={}']",
                        escape_js_string(&pinned_target.resolver_rule)
                    )
                };
                let launch_prelude = playwright_launch_prelude(&launch_args);

                let node_script = format!(
                    r#"
(async () => {{
    {}
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
                    launch_prelude,
                    safe_url,
                    timeout_secs * 1000,
                    script_json
                );

                let mut cmd = Command::new("node");
                crate::safety::process_env::sanitize_command_env(&mut cmd);
                configure_playwright_command(&mut cmd);
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
                    "url": pinned_target.url,
                    "resolved_ip": pinned_target.ip.to_string(),
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
                Err(anyhow::anyhow!(
                    "browser automation unavailable: no Playwright/Chrome backend found; \
                     install a browser backend to use browser_eval"
                ))
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

        // Extract links using precompiled regex.
        let mut links: Vec<String> = LINK_HREF_REGEX
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

fn resolve_and_pin_target(url: &str) -> Result<PinnedTarget> {
    let parsed = url::Url::parse(url).context("Invalid URL")?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        anyhow::bail!("Only HTTP and HTTPS URLs are allowed");
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("URL host is required"))?
        .to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);
    let host_is_ip = host.parse::<IpAddr>().is_ok();
    let allow_private = std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1";
    let allow_localhost = is_local_endpoint(url) || is_trusted_local_browser_host(&host);

    let ip = if let Ok(ip) = host.parse::<IpAddr>() {
        ip
    } else {
        let addrs: Vec<_> = (host.as_str(), port)
            .to_socket_addrs()
            .with_context(|| format!("Failed to resolve host {}", host))?
            .collect();

        if addrs.is_empty() {
            anyhow::bail!("Host {} did not resolve to any addresses", host);
        }

        if !allow_private
            && !allow_localhost
            && addrs.iter().any(|addr| is_private_network_ip(&addr.ip()))
        {
            anyhow::bail!(
                "DNS rebinding blocked: {} resolves to private/internal address",
                host
            );
        }

        addrs[0].ip()
    };

    if !allow_private && !allow_localhost && is_private_network_ip(&ip) {
        anyhow::bail!(
            "Blocked request to private/internal network address: {}",
            ip
        );
    }
    if allow_private && is_private_network_ip(&ip) {
        tracing::warn!(
            "Allowing browser request to private network (SELFWARE_ALLOW_PRIVATE_NETWORK=1): {} -> {}",
            host,
            ip
        );
    }

    Ok(PinnedTarget {
        url: url.to_string(),
        host: host.clone(),
        port,
        ip,
        host_is_ip,
        resolver_rule: format!("MAP {} {},EXCLUDE localhost", host, ip),
    })
}

fn is_trusted_local_browser_host(host: &str) -> bool {
    let bare_host = host.trim_start_matches('[').trim_end_matches(']');
    matches!(bare_host, "localhost" | "127.0.0.1" | "::1" | "0.0.0.0")
        || bare_host.ends_with(".localhost")
}

fn is_private_network_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified()
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
        }
    }
}

static LINK_HREF_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href=["']([^"']+)["']"#).expect("href regex is valid"));
static SCRIPT_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").expect("script regex is valid"));
static STYLE_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").expect("style regex is valid"));
static HTML_TAG_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<[^>]+>").expect("html tag regex is valid"));
static WHITESPACE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("whitespace regex is valid"));

/// Extract text content from HTML (simple implementation)
fn extract_text_from_html(html: &str) -> String {
    // Regexes are precompiled in LazyLock statics to avoid repeated allocation.
    let text = SCRIPT_TAG_REGEX.replace_all(html, "");
    let text = STYLE_TAG_REGEX.replace_all(&text, "");
    let text = HTML_TAG_REGEX.replace_all(&text, " ");
    let text = WHITESPACE_REGEX.replace_all(&text, " ");

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
    super::truncate_output(output, max_len)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
#[path = "../../tests/unit/tools/browser/browser_test.rs"]
mod tests;
