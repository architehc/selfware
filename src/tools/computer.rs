//! Computer control tools: mouse, keyboard, screen capture, window management.
//!
//! These tools wrap the `src/computer/` module and register as executable tools
//! in the ToolRegistry, allowing the agent to control desktop applications.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::Tool;

/// Mouse control tool — move, click, scroll, drag.
pub struct ComputerMouseTool;

#[async_trait]
impl Tool for ComputerMouseTool {
    fn name(&self) -> &str {
        "computer_mouse"
    }

    fn description(&self) -> &str {
        "Control the mouse: move, click, scroll, drag. Actions: move_to, click, double_click, right_click, scroll, drag."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["move_to", "click", "double_click", "right_click", "scroll", "drag"],
                    "description": "Mouse action to perform"
                },
                "x": { "type": "integer", "description": "X coordinate" },
                "y": { "type": "integer", "description": "Y coordinate" },
                "end_x": { "type": "integer", "description": "End X for drag" },
                "end_y": { "type": "integer", "description": "End Y for drag" },
                "delta_x": { "type": "integer", "description": "Scroll X delta" },
                "delta_y": { "type": "integer", "description": "Scroll Y delta (positive=up, negative=down)" },
                "expected_visual": {
                    "type": "string",
                    "description": "Optional expected on-screen result after the action. Used for post-action visual verification."
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' field"))?;
        let x = args["x"].as_i64().unwrap_or(0) as i32;
        let y = args["y"].as_i64().unwrap_or(0) as i32;

        let controller = crate::computer::MouseController::new();

        match action {
            "move_to" => {
                controller.move_to(x, y).await?;
                Ok(json!({"status": "ok", "action": "move_to", "x": x, "y": y}))
            }
            "click" => {
                controller
                    .click_at(x, y, crate::computer::mouse::MouseButton::Left)
                    .await?;
                Ok(json!({"status": "ok", "action": "click", "x": x, "y": y}))
            }
            "double_click" => {
                controller.move_to(x, y).await?;
                controller.double_click().await?;
                Ok(json!({"status": "ok", "action": "double_click", "x": x, "y": y}))
            }
            "right_click" => {
                controller
                    .click_at(x, y, crate::computer::mouse::MouseButton::Right)
                    .await?;
                Ok(json!({"status": "ok", "action": "right_click", "x": x, "y": y}))
            }
            "scroll" => {
                let delta_x = args["delta_x"].as_i64().unwrap_or(0) as i32;
                let delta_y = args["delta_y"].as_i64().unwrap_or(0) as i32;
                controller.scroll(delta_x, delta_y).await?;
                Ok(
                    json!({"status": "ok", "action": "scroll", "delta_x": delta_x, "delta_y": delta_y}),
                )
            }
            "drag" => {
                let end_x = args["end_x"].as_i64().unwrap_or(0) as i32;
                let end_y = args["end_y"].as_i64().unwrap_or(0) as i32;
                let from = crate::computer::mouse::Point::new(x, y);
                let to = crate::computer::mouse::Point::new(end_x, end_y);
                controller
                    .drag(from, to, crate::computer::mouse::MouseButton::Left)
                    .await?;
                Ok(json!({"status": "ok", "action": "drag", "from": [x, y], "to": [end_x, end_y]}))
            }
            other => anyhow::bail!("Unknown mouse action: {}", other),
        }
    }
}

/// Keyboard control tool — type text, press keys, key combos.
pub struct ComputerKeyboardTool;

#[async_trait]
impl Tool for ComputerKeyboardTool {
    fn name(&self) -> &str {
        "computer_keyboard"
    }

    fn description(&self) -> &str {
        "Control the keyboard: type text, press keys, key combinations. Actions: type, press, combo."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["type", "press", "combo"],
                    "description": "Keyboard action to perform"
                },
                "text": { "type": "string", "description": "Text to type (for 'type' action)" },
                "key": { "type": "string", "description": "Key name (for 'press' action)" },
                "keys": { "type": "string", "description": "Key combo like 'ctrl+c' (for 'combo' action)" },
                "expected_visual": {
                    "type": "string",
                    "description": "Optional expected on-screen result after the action. Used for post-action visual verification."
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' field"))?;

        let controller = crate::computer::KeyboardController::new();

        match action {
            "type" => {
                let text = args["text"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'text' for type action"))?;
                controller.type_text(text).await?;
                Ok(json!({"status": "ok", "action": "type", "chars": text.len()}))
            }
            "press" => {
                let key = args["key"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'key' for press action"))?;
                controller.press_key(key).await?;
                Ok(json!({"status": "ok", "action": "press", "key": key}))
            }
            "combo" => {
                let keys = args["keys"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'keys' for combo action"))?;
                controller.key_combo(keys).await?;
                Ok(json!({"status": "ok", "action": "combo", "keys": keys}))
            }
            other => anyhow::bail!("Unknown keyboard action: {}", other),
        }
    }
}

/// Screen capture tool — full screen or region capture.
pub struct ComputerScreenTool;

#[async_trait]
impl Tool for ComputerScreenTool {
    fn name(&self) -> &str {
        "computer_screen"
    }

    fn description(&self) -> &str {
        "Capture the full screen or a region (with a Windows fallback under WSL). \
         Saves the PNG to a per-session directory outside the workspace (or `output_path`) \
         and returns {path, width, height, bytes}; set inline=true to also get base64_png \
         (attached as an image for vision models; capped ~3.75 MB, use a region for more). \
         Same arguments and output as `screen_capture`, which spells the mode `target` \
         (screen|window|region) and can also capture a single window by title."
    }

    fn schema(&self) -> Value {
        let mut props = serde_json::Map::new();
        props.insert(
            "action".to_string(),
            json!({
                "type": "string",
                "enum": ["full", "region"],
                "description": "Capture mode ('screen' is accepted as 'full'). For a single window use `screen_capture` with target=window."
            }),
        );
        props.insert(
            "x".to_string(),
            json!({ "type": "integer", "description": "Region X (alternative to region.x)" }),
        );
        props.insert(
            "y".to_string(),
            json!({ "type": "integer", "description": "Region Y (alternative to region.y)" }),
        );
        props.insert(
            "width".to_string(),
            json!({ "type": "integer", "description": "Region width (alternative to region.width)" }),
        );
        props.insert(
            "height".to_string(),
            json!({ "type": "integer", "description": "Region height (alternative to region.height)" }),
        );
        props.extend(super::screen_capture::shared_output_schema_properties());
        json!({
            "type": "object",
            "properties": props,
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        use super::screen_capture::{
            parse_region, run_blocking, screenshot_session_dir, shape_capture_output,
            CaptureOutputOptions,
        };

        // Relative paths resolve against the agent's workspace root.
        let args = crate::tools::workspace_root::anchor_json(args, &["output_path"]);
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' field"))?;
        let opts = CaptureOutputOptions::from_args(&args);

        let (target, captured) = match action {
            "full" | "screen" => (
                "screen",
                crate::computer::ScreenCapture::capture_full().await?,
            ),
            "region" => {
                let (x, y, width, height) = parse_region(&args)?;
                let region = crate::computer::screen::ScreenRegion::new(x, y, width, height);
                (
                    "region",
                    crate::computer::ScreenCapture::capture_region(region).await?,
                )
            }
            "window" => anyhow::bail!(
                "computer_screen cannot capture a single window; use `screen_capture` with \
                 target=window and window_name=<title substring>"
            ),
            other => anyhow::bail!("Unknown screen action: {}", other),
        };

        // The capture backend hands back base64; decode once so the shared
        // shaping writes real PNG bytes and measures their true size.
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            captured.base64_png.as_bytes(),
        )
        .map_err(|e| anyhow::anyhow!("Screen capture returned invalid base64 PNG: {}", e))?;
        let dir = screenshot_session_dir();
        run_blocking(|| {
            shape_capture_output(
                self.name(),
                target,
                &png,
                captured.width,
                captured.height,
                &opts,
                &dir,
            )
        })
    }
}

/// Window management tool — list, focus, launch, get active window.
///
/// Carries the `[computer]` config so every `WindowManager` it builds applies
/// the configured `[computer.window_policy]` (ownership gate, visible region,
/// request scale). `ComputerWindowTool::default()` means "no policy"
/// (unrestricted), matching an absent `[computer.window_policy]` block.
#[derive(Debug, Clone, Default)]
pub struct ComputerWindowTool {
    config: crate::config::ComputerConfig,
}

impl ComputerWindowTool {
    /// A window tool enforcing the `[computer]` config (its window policy).
    pub fn from_config(config: &crate::config::ComputerConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// The window manager this tool drives, built from its config.
    fn window_manager(&self) -> crate::computer::WindowManager {
        crate::computer::WindowManager::from_config(&self.config)
    }
}

#[async_trait]
impl Tool for ComputerWindowTool {
    fn name(&self) -> &str {
        "computer_window"
    }

    fn description(&self) -> &str {
        "Manage desktop windows: list visible windows, focus a window, get active window, launch applications."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "focus", "active", "launch"],
                    "description": "Window action to perform"
                },
                "window_id": { "type": "integer", "description": "Window ID (for focus)" },
                "app_name": { "type": "string", "description": "Application name (for launch)" },
                "expected_visual": {
                    "type": "string",
                    "description": "Optional expected on-screen result after the action. Used for post-action visual verification."
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' field"))?;

        let wm = self.window_manager();

        match action {
            "list" => {
                let windows = wm.list_windows().await?;
                Ok(json!({"status": "ok", "windows": windows}))
            }
            "active" => {
                let window = wm.get_active_window().await?;
                Ok(json!({"status": "ok", "window": window}))
            }
            "focus" => {
                let id = args["window_id"]
                    .as_u64()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'window_id'"))?;
                wm.focus_window(&crate::computer::window::WindowId(id))
                    .await?;
                Ok(json!({"status": "ok", "action": "focus", "window_id": id}))
            }
            "launch" => {
                let app_name = args["app_name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing 'app_name'"))?;
                wm.launch_app(app_name).await?;
                Ok(json!({"status": "ok", "action": "launch", "app_name": app_name}))
            }
            other => anyhow::bail!("Unknown window action: {}", other),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tools/computer/computer_test.rs"]
mod tests;
