use super::*;
use std::path::Path;

#[test]
fn test_navigate_maps_to_goto() {
    let action = WebAction::Navigate {
        url: "https://example.com".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "goto");
    assert_eq!(cmd["url"], "https://example.com");
    assert_eq!(cmd["wait_until"], "load");
}

#[test]
fn test_click_maps_to_click() {
    let action = WebAction::Click {
        selector: "#button".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "click");
    assert_eq!(cmd["selector"], "#button");
}

#[test]
fn test_fill_maps_to_fill_with_text_field() {
    let action = WebAction::Fill {
        selector: "input[name='q']".into(),
        value: "hello world".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "fill");
    assert_eq!(cmd["selector"], "input[name='q']");
    assert_eq!(cmd["text"], "hello world");
    // The tool field is "text", not "value"
    assert!(cmd.get("value").is_none());
}

#[test]
fn test_extract_maps_to_text() {
    let action = WebAction::Extract {
        selector: ".result".into(),
        expected: "Rust".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "text");
    assert_eq!(cmd["selector"], ".result");
    // Expected is not part of the command
    assert!(cmd.get("expected").is_none());
}

#[test]
fn test_screenshot_maps_to_screenshot() {
    let action = WebAction::Screenshot {
        label: "result_page".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "screenshot");
    assert_eq!(cmd["full_page"], true);
    let path = cmd["path"].as_str().unwrap();
    assert!(
        path.ends_with("result_page.png"),
        "path should end with result_page.png, got: {path}"
    );
}

#[test]
fn test_waitfor_maps_to_wait_for_visible() {
    let action = WebAction::WaitFor {
        selector: "#content".into(),
        timeout_ms: 5000,
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "wait_for");
    assert_eq!(cmd["selector"], "#content");
    assert_eq!(cmd["state"], "visible");
    assert_eq!(cmd["timeout_ms"], 5000);
}

#[test]
fn test_scroll_down() {
    let action = WebAction::Scroll {
        direction: ScrollDirection::Down,
        amount: 300,
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "evaluate");
    assert_eq!(
        cmd["expression"].as_str().unwrap(),
        "window.scrollBy(0, 300)"
    );
}

#[test]
fn test_scroll_up() {
    let action = WebAction::Scroll {
        direction: ScrollDirection::Up,
        amount: 300,
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "evaluate");
    assert_eq!(
        cmd["expression"].as_str().unwrap(),
        "window.scrollBy(0, -300)"
    );
}

#[test]
fn test_scroll_left() {
    let action = WebAction::Scroll {
        direction: ScrollDirection::Left,
        amount: 300,
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "evaluate");
    assert_eq!(
        cmd["expression"].as_str().unwrap(),
        "window.scrollBy(-300, 0)"
    );
}

#[test]
fn test_scroll_right() {
    let action = WebAction::Scroll {
        direction: ScrollDirection::Right,
        amount: 300,
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "evaluate");
    assert_eq!(
        cmd["expression"].as_str().unwrap(),
        "window.scrollBy(300, 0)"
    );
}

#[test]
fn test_press_maps_to_press() {
    let action = WebAction::Press {
        key: "Enter".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "press");
    assert_eq!(cmd["key"], "Enter");
}

#[test]
fn test_hover_maps_to_hover() {
    let action = WebAction::Hover {
        selector: ".menu-item".into(),
    };
    let cmd = web_action_to_command(&action, Path::new("/tmp/ss"));
    assert_eq!(cmd["action"], "hover");
    assert_eq!(cmd["selector"], ".menu-item");
}

#[test]
fn result_is_visible_accepts_bool_and_object() {
    assert!(result_is_visible(&json!(true)));
    assert!(!result_is_visible(&json!(false)));
    // the bridge returns an object
    assert!(result_is_visible(&json!({"visible": true})));
    assert!(!result_is_visible(&json!({"visible": false})));
    // missing / wrong shape -> not visible
    assert!(!result_is_visible(&json!({})));
    assert!(!result_is_visible(&json!("nope")));
}

#[test]
fn result_to_string_unwraps_bridge_text_fields() {
    use serde_json::json;
    assert_eq!(super::result_to_string(&json!("plain")), "plain");
    assert_eq!(super::result_to_string(&json!({"text": "hello"})), "hello");
    assert_eq!(
        super::result_to_string(&json!({"url": "http://x/y"})),
        "http://x/y"
    );
    assert_eq!(
        super::result_to_string(&json!({"texts": ["a", "b"]})),
        "a\nb"
    );
    // Unknown object shape still falls back to a JSON dump (not empty).
    assert!(super::result_to_string(&json!({"other": 1})).contains("other"));
}

#[tokio::test]
async fn execute_all_empty_is_empty() {
    let dir = std::env::temp_dir().join(format!("bx_empty_{}", std::process::id()));
    let ex = BrowserTaskExecutor::new(dir.clone()).unwrap();
    let traces = ex.execute_all(&[], 4).await;
    assert!(traces.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
