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
        "Capture the screen: full screen or a specific region. Returns base64 PNG."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["full", "region"],
                    "description": "Capture mode"
                },
                "x": { "type": "integer", "description": "Region X (for 'region')" },
                "y": { "type": "integer", "description": "Region Y (for 'region')" },
                "width": { "type": "integer", "description": "Region width (for 'region')" },
                "height": { "type": "integer", "description": "Region height (for 'region')" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'action' field"))?;

        match action {
            "full" => {
                let captured = crate::computer::ScreenCapture::capture_full().await?;
                Ok(json!({
                    "status": "ok",
                    "width": captured.width,
                    "height": captured.height,
                    "base64_png": captured.base64_png
                }))
            }
            "region" => {
                let x = args["x"].as_i64().unwrap_or(0) as i32;
                let y = args["y"].as_i64().unwrap_or(0) as i32;
                let width = args["width"].as_u64().unwrap_or(100) as u32;
                let height = args["height"].as_u64().unwrap_or(100) as u32;
                let region = crate::computer::screen::ScreenRegion::new(x, y, width, height);
                let captured = crate::computer::ScreenCapture::capture_region(region).await?;
                Ok(json!({
                    "status": "ok",
                    "width": captured.width,
                    "height": captured.height,
                    "base64_png": captured.base64_png
                }))
            }
            other => anyhow::bail!("Unknown screen action: {}", other),
        }
    }
}

/// Window management tool — list, focus, launch, get active window.
pub struct ComputerWindowTool;

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

        let wm = crate::computer::WindowManager::new();

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
mod tests {
    use super::*;
    use serde_json::json;

    // ── Mouse tool tests ──────────────────────────────────────────────

    #[test]
    fn test_mouse_tool_name() {
        let tool = ComputerMouseTool;
        assert_eq!(tool.name(), "computer_mouse");
    }

    #[test]
    fn test_mouse_tool_description() {
        let tool = ComputerMouseTool;
        assert!(tool.description().contains("mouse"));
        assert!(tool.description().contains("click"));
    }

    #[test]
    fn test_mouse_tool_schema() {
        let tool = ComputerMouseTool;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&json!("move_to")));
        assert!(actions.contains(&json!("click")));
        assert!(actions.contains(&json!("double_click")));
        assert!(actions.contains(&json!("right_click")));
        assert!(actions.contains(&json!("scroll")));
        assert!(actions.contains(&json!("drag")));
        assert!(schema["properties"]["expected_visual"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[tokio::test]
    async fn test_mouse_tool_missing_action() {
        let tool = ComputerMouseTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("action"));
    }

    #[tokio::test]
    async fn test_mouse_tool_unknown_action() {
        let tool = ComputerMouseTool;
        let result = tool.execute(json!({"action": "teleport"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown mouse action"));
    }

    #[tokio::test]
    async fn test_mouse_tool_move_to() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({"action": "move_to", "x": 100, "y": 200}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "move_to");
        assert_eq!(result["x"], 100);
        assert_eq!(result["y"], 200);
    }

    #[tokio::test]
    async fn test_mouse_tool_click() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({"action": "click", "x": 50, "y": 50}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "click");
    }

    #[tokio::test]
    async fn test_mouse_tool_double_click() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({"action": "double_click", "x": 50, "y": 50}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "double_click");
    }

    #[tokio::test]
    async fn test_mouse_tool_right_click() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({"action": "right_click", "x": 10, "y": 20}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "right_click");
    }

    #[tokio::test]
    async fn test_mouse_tool_scroll() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({"action": "scroll", "delta_x": 0, "delta_y": -3}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "scroll");
        assert_eq!(result["delta_y"], -3);
    }

    #[tokio::test]
    async fn test_mouse_tool_scroll_defaults() {
        let tool = ComputerMouseTool;
        let result = tool.execute(json!({"action": "scroll"})).await.unwrap();
        assert_eq!(result["delta_x"], 0);
        assert_eq!(result["delta_y"], 0);
    }

    #[tokio::test]
    async fn test_mouse_tool_drag() {
        let tool = ComputerMouseTool;
        let result = tool
            .execute(json!({
                "action": "drag", "x": 10, "y": 20, "end_x": 100, "end_y": 200
            }))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["action"], "drag");
    }

    #[tokio::test]
    async fn test_mouse_tool_defaults_to_zero_coords() {
        let tool = ComputerMouseTool;
        let result = tool.execute(json!({"action": "move_to"})).await.unwrap();
        assert_eq!(result["x"], 0);
        assert_eq!(result["y"], 0);
    }

    // ── Keyboard tool tests ───────────────────────────────────────────

    #[test]
    fn test_keyboard_tool_name() {
        let tool = ComputerKeyboardTool;
        assert_eq!(tool.name(), "computer_keyboard");
    }

    #[test]
    fn test_keyboard_tool_description() {
        let tool = ComputerKeyboardTool;
        assert!(tool.description().contains("keyboard"));
    }

    #[test]
    fn test_keyboard_tool_schema() {
        let tool = ComputerKeyboardTool;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&json!("type")));
        assert!(actions.contains(&json!("press")));
        assert!(actions.contains(&json!("combo")));
        assert!(schema["properties"]["expected_visual"].is_object());
    }

    #[tokio::test]
    async fn test_keyboard_tool_missing_action() {
        let tool = ComputerKeyboardTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_keyboard_tool_unknown_action() {
        let tool = ComputerKeyboardTool;
        let result = tool.execute(json!({"action": "smash"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown keyboard action"));
    }

    #[tokio::test]
    async fn test_keyboard_tool_type() {
        let tool = ComputerKeyboardTool;
        let result = tool
            .execute(json!({"action": "type", "text": "hello"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["chars"], 5);
    }

    #[tokio::test]
    async fn test_keyboard_tool_type_missing_text() {
        let tool = ComputerKeyboardTool;
        let result = tool.execute(json!({"action": "type"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("text"));
    }

    #[tokio::test]
    async fn test_keyboard_tool_press() {
        let tool = ComputerKeyboardTool;
        let result = tool
            .execute(json!({"action": "press", "key": "Enter"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["key"], "Enter");
    }

    #[tokio::test]
    async fn test_keyboard_tool_press_missing_key() {
        let tool = ComputerKeyboardTool;
        let result = tool.execute(json!({"action": "press"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_keyboard_tool_combo() {
        let tool = ComputerKeyboardTool;
        let result = tool
            .execute(json!({"action": "combo", "keys": "ctrl+s"}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["keys"], "ctrl+s");
    }

    #[tokio::test]
    async fn test_keyboard_tool_combo_missing_keys() {
        let tool = ComputerKeyboardTool;
        let result = tool.execute(json!({"action": "combo"})).await;
        assert!(result.is_err());
    }

    // ── Screen tool tests ─────────────────────────────────────────────

    #[test]
    fn test_screen_tool_name() {
        let tool = ComputerScreenTool;
        assert_eq!(tool.name(), "computer_screen");
    }

    #[test]
    fn test_screen_tool_description() {
        let tool = ComputerScreenTool;
        assert!(tool.description().contains("screen"));
        assert!(tool.description().contains("base64"));
    }

    #[test]
    fn test_screen_tool_schema() {
        let tool = ComputerScreenTool;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&json!("full")));
        assert!(actions.contains(&json!("region")));
    }

    #[tokio::test]
    async fn test_screen_tool_missing_action() {
        let tool = ComputerScreenTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_screen_tool_unknown_action() {
        let tool = ComputerScreenTool;
        let result = tool.execute(json!({"action": "3d_scan"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown screen action"));
    }

    // ── Window tool tests ─────────────────────────────────────────────

    #[test]
    fn test_window_tool_name() {
        let tool = ComputerWindowTool;
        assert_eq!(tool.name(), "computer_window");
    }

    #[test]
    fn test_window_tool_description() {
        let tool = ComputerWindowTool;
        assert!(tool.description().contains("window"));
    }

    #[test]
    fn test_window_tool_schema() {
        let tool = ComputerWindowTool;
        let schema = tool.schema();
        assert_eq!(schema["type"], "object");
        let actions = schema["properties"]["action"]["enum"].as_array().unwrap();
        assert!(actions.contains(&json!("list")));
        assert!(actions.contains(&json!("focus")));
        assert!(actions.contains(&json!("active")));
        assert!(actions.contains(&json!("launch")));
        assert!(schema["properties"]["expected_visual"].is_object());
    }

    #[tokio::test]
    async fn test_window_tool_missing_action() {
        let tool = ComputerWindowTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_window_tool_unknown_action() {
        let tool = ComputerWindowTool;
        let result = tool.execute(json!({"action": "explode"})).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown window action"));
    }

    #[tokio::test]
    async fn test_window_tool_list() {
        let tool = ComputerWindowTool;
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert_eq!(result["status"], "ok");
        assert!(result["windows"].is_array());
    }

    #[tokio::test]
    async fn test_window_tool_focus_missing_id() {
        let tool = ComputerWindowTool;
        let result = tool.execute(json!({"action": "focus"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("window_id"));
    }

    #[tokio::test]
    async fn test_window_tool_focus_with_id() {
        // Skip test if no working X11 display / window manager is available
        if std::env::var("DISPLAY").is_err()
            || std::process::Command::new("xdotool")
                .arg("getactivewindow")
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true)
        {
            eprintln!("Skipping test: no working X11 display");
            return;
        }
        // Skip test if window management tools aren't available
        if std::process::Command::new("wmctrl")
            .arg("-l")
            .output()
            .is_err()
            && std::process::Command::new("xdotool")
                .args(["search", "--class", "."])
                .output()
                .is_err()
        {
            eprintln!("Skipping test: neither wmctrl nor xdotool available");
            return;
        }
        let tool = ComputerWindowTool;
        let result = tool
            .execute(json!({"action": "focus", "window_id": 1}))
            .await
            .unwrap();
        assert_eq!(result["status"], "ok");
    }

    #[tokio::test]
    async fn test_window_tool_launch_missing_name() {
        let tool = ComputerWindowTool;
        let result = tool.execute(json!({"action": "launch"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("app_name"));
    }
}
