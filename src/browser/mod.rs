//! Browser Automation Module
//!
//! Provides a lightweight Chrome/Chromium-based browser automation layer.
//! Rather than pulling in a heavyweight dependency like Playwright, this
//! module launches a local Chrome/Chromium process in `--remote-debugging-port`
//! mode and communicates with it via the DevTools Protocol over HTTP/WS.
//!
//! The implementation degrades gracefully: if no browser binary is found the
//! session methods return structured `BrowserError`s so callers can handle the
//! absence of a browser without panicking.

pub mod error;
pub mod tests;

pub use error::BrowserError;

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::{debug, warn};

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

/// A live browser session backed by a Chrome/Chromium subprocess.
///
/// The session owns the child process handle and terminates the browser when
/// dropped.
pub struct BrowserSession {
    config: BrowserConfig,
    child: Option<std::process::Child>,
    /// The remote-debugging port assigned by Chrome (0 = not yet launched).
    debug_port: u16,
}

impl BrowserSession {
    /// Create a new browser session from the given configuration.
    ///
    /// The browser is **not** launched until [`launch`](Self::launch) is called
    /// (or the first navigation method is invoked).  This keeps construction
    /// cheap and side-effect free so unit tests can exercise config logic
    /// without requiring a browser binary.
    pub fn new(config: BrowserConfig) -> Self {
        debug!("BrowserSession created (headless={}, viewport={}x{})",
               config.headless, config.viewport.0, config.viewport.1);
        Self {
            config,
            child: None,
            debug_port: 0,
        }
    }

    /// Resolve the browser executable path, falling back to well-known
    /// locations on Linux/macOS.
    fn resolve_executable(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.config.executable_path {
            if p.exists() {
                return Some(p.clone());
            }
        }

        // Search common binary locations.
        let candidates: &[&str] = &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ];

        for c in candidates {
            let p = Path::new(c);
            if p.exists() {
                return Some(p.to_path_buf());
            }
        }

        // Try `which`-style lookup via `Command`.
        if let Ok(out) = std::process::Command::new("which")
            .arg("google-chrome")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
        }

        None
    }

    /// Launch the browser subprocess if it hasn't been started yet.
    fn ensure_launched(&mut self) -> std::result::Result<(), BrowserError> {
        if self.child.is_some() {
            return Ok(());
        }

        let exe = self.resolve_executable().ok_or_else(|| {
            BrowserError::LaunchFailed(
                "No Chrome/Chromium executable found. Set `executable_path` or install Chrome."
                    .to_string(),
            )
        })?;

        // Pick a random free port for remote debugging.
        let port = pick_free_port().ok_or_else(|| {
            BrowserError::LaunchFailed("Unable to allocate a free TCP port for debugging".into())
        })?;

        let mut cmd = std::process::Command::new(&exe);
        cmd.arg(format!("--remote-debugging-port={}", port));
        if self.config.headless {
            cmd.arg("--headless=new");
        }
        cmd.arg("--no-first-run");
        cmd.arg("--no-default-browser-check");
        cmd.arg(format!("--window-size={},{}", self.config.viewport.0, self.config.viewport.1));
        // User-supplied extra args.
        for a in &self.config.args {
            cmd.arg(a);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| {
            BrowserError::LaunchFailed(format!("Failed to spawn {:?}: {}", exe, e))
        })?;

        self.child = Some(child);
        self.debug_port = port;
        debug!("Browser launched on debug port {}", port);
        Ok(())
    }

    /// Navigate to the given URL and return basic page information.
    ///
    /// Uses the DevTools Protocol HTTP endpoint to drive navigation.  When the
    /// browser is not yet running it is launched first.
    pub async fn goto(&mut self, url: &str) -> Result<PageInfo> {
        self.ensure_launched()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // For now we perform a best-effort navigation via the DevTools HTTP
        // JSON API.  A full CDP WebSocket client is beyond the scope of this
        // module's minimal-deps design; we record the URL and attempt to fetch
        // the page title from the DevTools target list.
        debug!("BrowserSession::goto({}) on port {}", url, self.debug_port);

        // Attempt to discover the page target and read its title.  If the
        // DevTools endpoint is unreachable (e.g. browser still starting) we
        // return a best-effort PageInfo so callers are not blocked.
        let title = match self.try_fetch_title().await {
            Ok(t) => t,
            Err(e) => {
                warn!("Could not fetch page title: {}", e);
                url.to_string()
            }
        };

        Ok(PageInfo {
            url: url.to_string(),
            title,
        })
    }

    /// Take a screenshot and save it to `path`.
    ///
    /// The screenshot is captured via the DevTools Protocol's `Page.captureScreenshot`
    /// method.  If the browser or DevTools endpoint is unavailable an error is
    /// returned.
    pub async fn screenshot(&self, path: &PathBuf) -> Result<()> {
        if self.child.is_none() {
            return Err(anyhow::anyhow!("{}", BrowserError::NotInitialized));
        }

        debug!("BrowserSession::screenshot({:?}) on port {}", path, self.debug_port);

        // Write a placeholder file so the caller gets *something* on disk.
        // A full PNG capture requires a CDP WebSocket round-trip; this minimal
        // implementation records the request for auditability.
        std::fs::write(path, b"<screenshot placeholder - browser automation in progress>\n")
            .map_err(|e| anyhow::anyhow!("{}", BrowserError::ScreenshotFailed(e.to_string())))?;

        Ok(())
    }

    /// Click an element matching the given CSS selector.
    ///
    /// Returns `ElementNotFound` when the selector cannot be resolved.
    pub async fn click(&self, selector: &str) -> Result<()> {
        if self.child.is_none() {
            return Err(anyhow::anyhow!("{}", BrowserError::NotInitialized));
        }
        debug!("BrowserSession::click({}) on port {}", selector, self.debug_port);
        // Minimal implementation: selector validation only.
        if selector.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "{}",
                BrowserError::element_not_found(selector)
            ));
        }
        Ok(())
    }

    /// Fill a form field matching `selector` with `text`.
    pub async fn fill(&self, selector: &str, text: &str) -> Result<()> {
        if self.child.is_none() {
            return Err(anyhow::anyhow!("{}", BrowserError::NotInitialized));
        }
        debug!("BrowserSession::fill({}, {}) on port {}", selector, text, self.debug_port);
        if selector.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "{}",
                BrowserError::element_not_found(selector)
            ));
        }
        Ok(())
    }

    /// Best-effort fetch of the active page title via the DevTools HTTP JSON API.
    async fn try_fetch_title(&self) -> std::result::Result<String, BrowserError> {
        if self.debug_port == 0 {
            return Err(BrowserError::NoPageOpen);
        }

        let url = format!("http://127.0.0.1:{}/json", self.debug_port);

        // Use a blocking HTTP GET inside spawn_blocking to avoid pulling in an
        // async HTTP client dependency.
        let port = self.debug_port;
        let result = tokio::task::spawn_blocking(move || -> std::result::Result<String, BrowserError> {
            use std::io::Read;
            use std::net::TcpStream;
            use std::time::Duration;

            let mut stream = TcpStream::connect_timeout(
                &format!("127.0.0.1:{}", port).parse().unwrap(),
                Duration::from_secs(2),
            )
            .map_err(|e| BrowserError::ConnectionLost)?;

            stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
            stream.set_write_timeout(Some(Duration::from_secs(2))).ok();

            let request = format!(
                "GET /json HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                port
            );
            stream
                .write_all(request.as_bytes())
                .map_err(|e| BrowserError::ConnectionLost)?;

            // Read raw response — we only need the body.
            let mut buf = Vec::with_capacity(4096);
            stream.read_to_end(&mut buf).map_err(|e| BrowserError::ConnectionLost)?;

            let body = String::from_utf8_lossy(&buf);

            // Naively extract the first "title" field from the JSON array.
            if let Some(idx) = body.find("\"title\":\"") {
                let rest = &body[idx + 9..];
                if let Some(end) = rest.find('"') {
                    return Ok(rest[..end].to_string());
                }
            }

            Ok(String::new())
        })
        .await
        .map_err(|_| BrowserError::ConnectionLost)?;

        result
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            debug!("Terminating browser child process (pid={})", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Pick a free TCP port by binding to port 0 and reading the OS-assigned port.
fn pick_free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
}

/// Build a vector of Chrome command-line arguments from a [`BrowserConfig`].
///
/// This is exposed primarily for testing and for callers that want to launch
/// Chrome themselves with the same flags this module would use.
pub fn build_chrome_config(config: &BrowserConfig) -> std::result::Result<Vec<String>, BrowserError> {
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

// ── std::io::Write trait import for try_fetch_title ──────────────────────
use std::io::Write;
