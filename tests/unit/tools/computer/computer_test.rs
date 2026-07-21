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
    let active_window = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok());
    let Some(window_id) = active_window else {
        eprintln!("Skipping test: no active window");
        return;
    };
    let result = tool
        .execute(json!({"action": "focus", "window_id": window_id}))
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
