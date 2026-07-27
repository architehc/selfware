use super::*;

#[tokio::test]
async fn test_blocked_combo_rejected() {
    let kb = KeyboardController::new();
    let result = kb.key_combo("ctrl+alt+delete").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("blocked"));
}

#[tokio::test]
async fn test_all_blocked_combos_rejected() {
    let kb = KeyboardController::new();
    for combo in &[
        "ctrl+alt+delete",
        "cmd+q",
        "alt+f4",
        "ctrl+alt+f1",
        "ctrl+alt+f2",
        "ctrl+alt+f3",
    ] {
        let result = kb.key_combo(combo).await;
        assert!(result.is_err(), "Expected '{}' to be blocked", combo);
    }
}

#[tokio::test]
async fn test_safe_combo_allowed() {
    let kb = KeyboardController::new();
    let result = kb.key_combo("ctrl+c").await;
    // Linux uses a no-op xdotool stub in tests; unsupported platforms must
    // return an honest error instead of silently succeeding.
    #[cfg(target_os = "linux")]
    {
        assert!(result.is_ok());
        assert!(kb.key_combo("ctrl+v").await.is_ok());
        assert!(kb.key_combo("ctrl+s").await.is_ok());
        assert!(kb.key_combo("ctrl+shift+t").await.is_ok());
    }
    #[cfg(not(target_os = "linux"))]
    assert!(result.is_err());
}

#[tokio::test]
async fn test_type_text_success() {
    let kb = KeyboardController::new();
    let result = kb.type_text("hello world").await;
    #[cfg(target_os = "linux")]
    assert!(result.is_ok());
    // macOS actually attempts the action via osascript; without Accessibility
    // permissions it must error honestly, never silently succeed.
    #[cfg(target_os = "macos")]
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(
            msg.contains("System Events") || msg.contains("osascript"),
            "unexpected error: {msg}"
        );
    }
}

#[tokio::test]
async fn test_type_text_empty() {
    let kb = KeyboardController::new();
    assert!(kb.type_text("").await.is_ok());
}

#[tokio::test]
async fn test_type_text_at_limit() {
    let kb = KeyboardController::new();
    let text = "x".repeat(10_000);
    let result = kb.type_text(&text).await;
    #[cfg(target_os = "linux")]
    assert!(result.is_ok());
    #[cfg(target_os = "macos")]
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(
            msg.contains("System Events") || msg.contains("osascript"),
            "unexpected error: {msg}"
        );
    }
}

#[tokio::test]
async fn test_type_text_length_limit() {
    let kb = KeyboardController::new();
    let long_text = "x".repeat(10_001);
    let result = kb.type_text(&long_text).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("10000"));
}

#[tokio::test]
async fn test_press_key() {
    let kb = KeyboardController::new();
    let result = kb.press_key("Enter").await;
    #[cfg(target_os = "linux")]
    {
        assert!(result.is_ok());
        assert!(kb.press_key("Tab").await.is_ok());
        assert!(kb.press_key("Escape").await.is_ok());
    }
    #[cfg(not(target_os = "linux"))]
    assert!(result.is_err());
}

#[tokio::test]
async fn test_key_down_up() {
    let kb = KeyboardController::new();
    let down = kb.key_down("Shift").await;
    #[cfg(target_os = "linux")]
    {
        assert!(down.is_ok());
        assert!(kb.key_up("Shift").await.is_ok());
    }
    #[cfg(not(target_os = "linux"))]
    assert!(down.is_err());
}

#[test]
fn test_keyboard_controller_default() {
    let kb = KeyboardController::default();
    let _ = format!("{:?}", kb.typing_profile);
}

#[test]
fn test_with_typing_profile() {
    let profile = TypingProfile {
        base_delay_ms: 50,
        variation_ms: 10,
    };
    let kb = KeyboardController::new().with_typing_profile(profile);
    assert_eq!(kb.typing_profile.base_delay_ms, 50);
}

#[tokio::test]
async fn test_type_text_with_delay_profile() {
    let profile = TypingProfile {
        base_delay_ms: 1, // 1ms delay per char for fast test
        variation_ms: 0,
    };
    let kb = KeyboardController::new().with_typing_profile(profile);
    let result = kb.type_text("hi").await;
    #[cfg(target_os = "linux")]
    assert!(result.is_ok());
    #[cfg(target_os = "macos")]
    if let Err(e) = &result {
        let msg = e.to_string();
        assert!(
            msg.contains("System Events") || msg.contains("osascript"),
            "unexpected error: {msg}"
        );
    }
}

// ---- macOS: key presses/combos must error honestly, never silently succeed ----

#[cfg(target_os = "macos")]
mod macos_honesty {
    use super::*;

    #[tokio::test]
    async fn press_key_errors_with_action_name() {
        let kb = KeyboardController::new();
        let err = kb.press_key("Enter").await.unwrap_err().to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("press_key"), "{err}");
    }

    #[tokio::test]
    async fn key_combo_errors_with_action_name() {
        let kb = KeyboardController::new();
        let err = kb.key_combo("ctrl+c").await.unwrap_err().to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("key_combo"), "{err}");
    }

    #[tokio::test]
    async fn key_down_up_error_with_action_name() {
        let kb = KeyboardController::new();
        let err = kb.key_down("Shift").await.unwrap_err().to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("key_down"), "{err}");
        let err = kb.key_up("Shift").await.unwrap_err().to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("key_up"), "{err}");
    }
}

// ---- Command construction tests ----

#[test]
fn test_map_key_name_common() {
    assert_eq!(map_key_name("Enter"), "Return");
    assert_eq!(map_key_name("enter"), "Return");
    assert_eq!(map_key_name("Return"), "Return");
    assert_eq!(map_key_name("Tab"), "Tab");
    assert_eq!(map_key_name("Escape"), "Escape");
    assert_eq!(map_key_name("esc"), "Escape");
    assert_eq!(map_key_name("Backspace"), "BackSpace");
    assert_eq!(map_key_name("Delete"), "Delete");
    assert_eq!(map_key_name("space"), "space");
}

#[test]
fn test_map_key_name_arrows() {
    assert_eq!(map_key_name("up"), "Up");
    assert_eq!(map_key_name("down"), "Down");
    assert_eq!(map_key_name("left"), "Left");
    assert_eq!(map_key_name("right"), "Right");
}

#[test]
fn test_map_key_name_modifiers() {
    assert_eq!(map_key_name("ctrl"), "ctrl");
    assert_eq!(map_key_name("control"), "ctrl");
    assert_eq!(map_key_name("alt"), "alt");
    assert_eq!(map_key_name("shift"), "shift");
    assert_eq!(map_key_name("super"), "super");
    assert_eq!(map_key_name("meta"), "super");
    assert_eq!(map_key_name("cmd"), "super");
}

#[test]
fn test_map_key_name_function_keys() {
    for i in 1..=12 {
        let key = format!("f{}", i);
        let expected = format!("F{}", i);
        assert_eq!(map_key_name(&key), expected);
    }
}

#[test]
fn test_map_key_name_unknown_passthrough() {
    assert_eq!(map_key_name("a"), "a");
    assert_eq!(map_key_name("z"), "z");
    assert_eq!(map_key_name("SomeWeirdKey"), "SomeWeirdKey");
}

#[test]
fn test_build_xdotool_type_cmd() {
    let (prog, args) = build_xdotool_type_cmd("hello world");
    assert_eq!(prog, "xdotool");
    assert_eq!(args, vec!["type", "--clearmodifiers", "--", "hello world"]);
}

#[test]
fn test_build_xdotool_type_cmd_special_chars() {
    let (_, args) = build_xdotool_type_cmd("echo \"test\" && rm -rf /");
    assert_eq!(args[3], "echo \"test\" && rm -rf /");
}

#[test]
fn test_build_xdotool_key_cmd() {
    let (prog, args) = build_xdotool_key_cmd("Return");
    assert_eq!(prog, "xdotool");
    assert_eq!(args, vec!["key", "Return"]);
}

#[test]
fn test_build_xdotool_keydown_cmd() {
    let (prog, args) = build_xdotool_keydown_cmd("shift");
    assert_eq!(prog, "xdotool");
    assert_eq!(args, vec!["keydown", "shift"]);
}

#[test]
fn test_build_xdotool_keyup_cmd() {
    let (prog, args) = build_xdotool_keyup_cmd("shift");
    assert_eq!(prog, "xdotool");
    assert_eq!(args, vec!["keyup", "shift"]);
}

#[test]
fn test_build_xdotool_combo() {
    assert_eq!(build_xdotool_combo("ctrl+c"), "ctrl+c");
    assert_eq!(build_xdotool_combo("ctrl+shift+t"), "ctrl+shift+t");
    assert_eq!(build_xdotool_combo("ctrl+v"), "ctrl+v");
}

#[test]
fn test_build_xdotool_combo_maps_keys() {
    assert_eq!(build_xdotool_combo("ctrl+Enter"), "ctrl+Return");
    assert_eq!(build_xdotool_combo("alt+Backspace"), "alt+BackSpace");
    assert_eq!(
        build_xdotool_combo("cmd+shift+Escape"),
        "super+shift+Escape"
    );
}

#[test]
fn test_build_xdotool_combo_with_spaces() {
    assert_eq!(build_xdotool_combo("ctrl + c"), "ctrl+c");
    assert_eq!(build_xdotool_combo("ctrl + shift + t"), "ctrl+shift+t");
}

// ---- SendKeys / PowerShell fallback tests ----

#[test]
fn test_map_key_to_sendkeys_common() {
    assert_eq!(map_key_to_sendkeys("Enter"), "{ENTER}");
    assert_eq!(map_key_to_sendkeys("return"), "{ENTER}");
    assert_eq!(map_key_to_sendkeys("Tab"), "{TAB}");
    assert_eq!(map_key_to_sendkeys("Escape"), "{ESC}");
    assert_eq!(map_key_to_sendkeys("Backspace"), "{BACKSPACE}");
    assert_eq!(map_key_to_sendkeys("Delete"), "{DELETE}");
    assert_eq!(map_key_to_sendkeys("space"), " ");
}

#[test]
fn test_map_key_to_sendkeys_arrows() {
    assert_eq!(map_key_to_sendkeys("up"), "{UP}");
    assert_eq!(map_key_to_sendkeys("down"), "{DOWN}");
    assert_eq!(map_key_to_sendkeys("left"), "{LEFT}");
    assert_eq!(map_key_to_sendkeys("right"), "{RIGHT}");
}

#[test]
fn test_map_key_to_sendkeys_navigation() {
    assert_eq!(map_key_to_sendkeys("home"), "{HOME}");
    assert_eq!(map_key_to_sendkeys("end"), "{END}");
    assert_eq!(map_key_to_sendkeys("pageup"), "{PGUP}");
    assert_eq!(map_key_to_sendkeys("pagedown"), "{PGDN}");
    assert_eq!(map_key_to_sendkeys("insert"), "{INSERT}");
}

#[test]
fn test_map_key_to_sendkeys_function_keys() {
    for i in 1..=12 {
        let key = format!("f{}", i);
        let expected = format!("{{F{}}}", i);
        assert_eq!(map_key_to_sendkeys(&key), expected);
    }
}

#[test]
fn test_map_key_to_sendkeys_passthrough() {
    assert_eq!(map_key_to_sendkeys("a"), "a");
    assert_eq!(map_key_to_sendkeys("z"), "z");
}

#[test]
fn test_escape_sendkeys_char_special() {
    assert_eq!(escape_sendkeys_char("+"), "{+}");
    assert_eq!(escape_sendkeys_char("^"), "{^}");
    assert_eq!(escape_sendkeys_char("%"), "{%}");
    assert_eq!(escape_sendkeys_char("~"), "{~}");
    assert_eq!(escape_sendkeys_char("("), "{(}");
    assert_eq!(escape_sendkeys_char(")"), "{)}");
}

#[test]
fn test_escape_sendkeys_text() {
    assert_eq!(escape_sendkeys_text("hello"), "hello");
    assert_eq!(escape_sendkeys_text("a+b"), "a{+}b");
    assert_eq!(escape_sendkeys_text("100%"), "100{%}");
    assert_eq!(escape_sendkeys_text("line1\nline2"), "line1{ENTER}line2");
    assert_eq!(escape_sendkeys_text("col1\tcol2"), "col1{TAB}col2");
}

#[test]
fn test_build_sendkeys_combo() {
    assert_eq!(build_sendkeys_combo("ctrl+c"), "^c");
    assert_eq!(build_sendkeys_combo("ctrl+v"), "^v");
    assert_eq!(build_sendkeys_combo("alt+tab"), "%{TAB}");
    assert_eq!(build_sendkeys_combo("ctrl+shift+t"), "^+t");
    assert_eq!(build_sendkeys_combo("shift+Enter"), "+{ENTER}");
}

#[test]
fn test_build_sendkeys_combo_with_spaces() {
    assert_eq!(build_sendkeys_combo("ctrl + c"), "^c");
    assert_eq!(build_sendkeys_combo("ctrl + shift + t"), "^+t");
}
