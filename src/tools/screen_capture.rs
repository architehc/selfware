//! Screen capture tool for general-purpose screenshot functionality.
//!
//! Uses the `xcap` crate to capture the screen, specific windows, or regions.
//!
//! Output (shared with `computer_screen`, see [`shape_capture_output`]): by
//! default the PNG is written to a per-session directory OUTSIDE the
//! workspace and the result is `{path, width, height, bytes}`. A 4–6 MB
//! base64 string is only returned when explicitly requested with
//! `inline: true`, and is capped at [`MAX_INLINE_PNG_BYTES`] — above that the
//! call fails honestly (with the capture saved to disk) instead of flooding
//! the conversation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{json, Value};
use xcap::image;

use super::Tool;

/// Largest PNG (raw bytes) returned inline as `base64_png`. Base64 inflates
/// by 4/3, so this keeps the encoded image at <= 5,000,000 bytes — the
/// per-image ceiling of the strictest common vision API — while a
/// full-resolution desktop capture (typically 4–6 MB of base64) must be
/// narrowed to a region or read from its saved `path`.
pub(crate) const MAX_INLINE_PNG_BYTES: usize = 3_750_000;

/// Output options shared by `screen_capture` and `computer_screen`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CaptureOutputOptions {
    /// Return `base64_png` inline (opt-in; capped at [`MAX_INLINE_PNG_BYTES`]).
    pub inline: bool,
    /// Caller-chosen destination (workspace-relative paths are anchored and
    /// safety-validated). When absent the per-session directory is used.
    pub output_path: Option<String>,
}

impl CaptureOutputOptions {
    /// Read `inline` / `output_path` from tool arguments.
    pub(crate) fn from_args(args: &Value) -> Self {
        Self {
            inline: args.get("inline").and_then(Value::as_bool).unwrap_or(false),
            output_path: args
                .get("output_path")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string),
        }
    }
}

/// JSON-schema properties shared by both capture tools, so a model that
/// learned one tool's output arguments can use the other's.
pub(crate) fn shared_output_schema_properties() -> serde_json::Map<String, Value> {
    let mut props = serde_json::Map::new();
    props.insert(
        "region".to_string(),
        json!({
            "type": "object",
            "properties": {
                "x": {"type": "integer", "description": "Left edge X coordinate"},
                "y": {"type": "integer", "description": "Top edge Y coordinate"},
                "width": {"type": "integer", "description": "Width in pixels"},
                "height": {"type": "integer", "description": "Height in pixels"}
            },
            "description": "Screen region to capture (for region mode). Top-level x/y/width/height are also accepted."
        }),
    );
    props.insert(
        "output_path".to_string(),
        json!({
            "type": "string",
            "description": "Optional workspace path to save the PNG to. Default: a per-session directory outside the workspace. The result's `path` says where it went."
        }),
    );
    props.insert(
        "inline".to_string(),
        json!({
            "type": "boolean",
            "description": "Also return the PNG as `base64_png` (attached as an image for vision-capable models). Default false. Fails above ~3.75 MB PNG — capture a smaller region instead."
        }),
    );
    props
}

/// Parse a capture region from either a `region` object or top-level
/// `x`/`y`/`width`/`height` (both tools accept both shapes). `x`/`y` default
/// to 0; `width`/`height` are required.
pub(crate) fn parse_region(args: &Value) -> Result<(i32, i32, u32, u32)> {
    const MAX_COORD: i64 = 100_000;
    const MAX_DIMENSION: u64 = 100_000;

    let source = match args.get("region") {
        Some(region) if region.is_object() => region,
        _ if args.get("width").is_some() || args.get("height").is_some() => args,
        _ => anyhow::bail!(
            "region is required for region capture: pass region {{x, y, width, height}}"
        ),
    };

    let x_val = source.get("x").and_then(Value::as_i64).unwrap_or(0);
    let y_val = source.get("y").and_then(Value::as_i64).unwrap_or(0);
    if x_val.abs() > MAX_COORD || y_val.abs() > MAX_COORD {
        anyhow::bail!("Region coordinates out of range (max {})", MAX_COORD);
    }
    let w_val = source
        .get("width")
        .and_then(Value::as_u64)
        .context("region.width is required")?;
    let h_val = source
        .get("height")
        .and_then(Value::as_u64)
        .context("region.height is required")?;
    if w_val > MAX_DIMENSION || h_val > MAX_DIMENSION {
        anyhow::bail!("Region dimensions out of range (max {})", MAX_DIMENSION);
    }
    if w_val == 0 || h_val == 0 {
        anyhow::bail!("Region width and height must be greater than 0");
    }
    Ok((x_val as i32, y_val as i32, w_val as u32, h_val as u32))
}

/// Per-session screenshot directory, outside any workspace:
/// `<data_local_dir>/selfware/tool_results/screenshots/session-<pid>-<start>`
/// (falls back to the OS temp dir when no data dir exists).
///
/// Unit-test builds root it in the OS temp dir instead, so display-dependent
/// tests never drop real desktop captures into the user's data directory.
pub(crate) fn screenshot_session_dir() -> PathBuf {
    #[cfg(not(test))]
    let base = dirs::data_local_dir();
    #[cfg(test)]
    let base = Some(std::env::temp_dir().join("selfware-unit-tests"));
    screenshot_session_dir_in(base)
}

/// [`screenshot_session_dir`] rooted at `base` (`None` = OS temp dir).
pub(crate) fn screenshot_session_dir_in(base: Option<PathBuf>) -> PathBuf {
    static SESSION: OnceLock<String> = OnceLock::new();
    let session = SESSION.get_or_init(|| {
        let started = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("session-{}-{}", std::process::id(), started)
    });
    base.unwrap_or_else(std::env::temp_dir)
        .join("selfware")
        .join("tool_results")
        .join("screenshots")
        .join(session)
}

/// Write `png` to a fresh file in `dir` (created 0700 on Unix; screenshots
/// can contain anything on screen) and return its path.
fn write_to_session_dir(dir: &Path, tool: &str, png: &[u8]) -> Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create screenshot directory {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%3f");
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("{tool}-{stamp}-{n}.png"));
    std::fs::write(&path, png)
        .with_context(|| format!("Failed to write screenshot to {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(path)
}

/// Write to a caller-chosen path after the same safety validation file tools use.
fn write_to_output_path(tool: &str, path: &str, png: &[u8]) -> Result<()> {
    let safety = crate::tools::file::resolve_safety_config(None);
    crate::tools::file::validate_tool_path(path, &safety)
        .map_err(|e| anyhow::anyhow!("{} output_path validation failed: {}", tool, e))?;
    std::fs::write(path, png).with_context(|| format!("Failed to write screenshot to {}", path))
}

/// Shape a capture into the tool result shared by `screen_capture` and
/// `computer_screen`. Performs blocking file I/O (callers on a multi-thread
/// runtime go through [`run_blocking`]).
///
/// - default: PNG saved to `session_dir`; result `{path, width, height, bytes}`.
/// - `output_path`: PNG saved there instead (safety-validated).
/// - `inline: true`: additionally `base64_png` (which the agent loop attaches
///   as an image); above [`MAX_INLINE_PNG_BYTES`] the call errors, naming
///   where the capture was saved.
pub(crate) fn shape_capture_output(
    tool: &str,
    target: &str,
    png: &[u8],
    width: u32,
    height: u32,
    opts: &CaptureOutputOptions,
    session_dir: &Path,
) -> Result<Value> {
    let saved: String = match &opts.output_path {
        Some(path) => {
            write_to_output_path(tool, path, png)?;
            path.clone()
        }
        None => write_to_session_dir(session_dir, tool, png)?
            .display()
            .to_string(),
    };

    let mut result = json!({
        "success": true,
        "status": "ok",
        "tool": tool,
        "target": target,
        "width": width,
        "height": height,
        "bytes": png.len(),
        "path": saved,
        "inline": false,
    });

    if opts.inline {
        if png.len() > MAX_INLINE_PNG_BYTES {
            anyhow::bail!(
                "{tool}: inline PNG is {} bytes, above the {} byte inline limit; \
                 not returned inline. The capture was saved to {} ({}x{}). \
                 Capture a smaller region for inline use, or omit `inline`.",
                png.len(),
                MAX_INLINE_PNG_BYTES,
                saved,
                width,
                height
            );
        }
        let base64_png = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
        result["inline"] = json!(true);
        result["base64_png"] = json!(base64_png);
    }
    Ok(result)
}

/// Run blocking I/O without stalling a multi-thread runtime worker; on a
/// current-thread runtime (or none) `block_in_place` would panic, so run inline.
pub(crate) fn run_blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current().map(|h| h.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(f),
        _ => f(),
    }
}

/// Capture a screenshot of the screen, a window, or a region.
pub struct ScreenCapture;

#[async_trait]
impl Tool for ScreenCapture {
    fn name(&self) -> &str {
        "screen_capture"
    }

    fn description(&self) -> &str {
        "Capture a screenshot of the primary screen, a window (by title), or a region. \
         Saves the PNG to a per-session directory outside the workspace (or `output_path`) \
         and returns {path, width, height, bytes}; set inline=true to also get base64_png \
         (attached as an image for vision models; capped ~3.75 MB, use a region for more). \
         Same arguments and output as `computer_screen`, which spells the mode `action` \
         (full|region) and has a WSL fallback but cannot capture windows."
    }

    fn schema(&self) -> Value {
        let mut props = serde_json::Map::new();
        props.insert(
            "target".to_string(),
            json!({
                "type": "string",
                "enum": ["screen", "window", "region"],
                "description": "What to capture. 'screen' captures the primary monitor ('full' is accepted too), \
                                'window' captures a specific window by title, \
                                'region' captures a screen region. Default: screen, or region when `region` is given"
            }),
        );
        props.insert(
            "window_name".to_string(),
            json!({
                "type": "string",
                "description": "Window title substring to match (required for target=window)"
            }),
        );
        props.extend(shared_output_schema_properties());
        json!({ "type": "object", "properties": props })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        // Relative paths resolve against the agent's workspace root.
        let args = crate::tools::workspace_root::anchor_json(args, &["output_path"]);
        let default_target = if args.get("region").is_some_and(Value::is_object) {
            "region"
        } else {
            "screen"
        };
        let target = args
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or(default_target);
        let opts = CaptureOutputOptions::from_args(&args);

        let (target, image) = match target {
            "screen" | "full" => ("screen", capture_screen()?),
            "window" => {
                let window_name = args
                    .get("window_name")
                    .and_then(|v| v.as_str())
                    .context("window_name is required when target=window")?;
                ("window", capture_window(window_name)?)
            }
            "region" => {
                let (x, y, width, height) = parse_region(&args)?;
                ("region", capture_region(x, y, width, height)?)
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

        let dir = screenshot_session_dir();
        run_blocking(|| {
            shape_capture_output(
                self.name(),
                target,
                &png_bytes,
                img_width,
                img_height,
                &opts,
                &dir,
            )
        })
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
#[path = "../../tests/unit/tools/screen_capture/screen_capture_test.rs"]
mod tests;
