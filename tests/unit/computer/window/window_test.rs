use super::*;

#[test]
fn test_window_id() {
    let id = WindowId(42);
    assert_eq!(id.0, 42);
}

#[test]
fn test_window_id_equality() {
    assert_eq!(WindowId(1), WindowId(1));
    assert_ne!(WindowId(1), WindowId(2));
}

#[test]
fn test_window_id_serde_roundtrip() {
    let id = WindowId(99);
    let json = serde_json::to_string(&id).unwrap();
    let parsed: WindowId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, id);
}

#[test]
fn test_window_info() {
    let info = WindowInfo {
        id: WindowId(1),
        title: "Test Window".to_string(),
        app_name: "test-app".to_string(),
        x: 100,
        y: 200,
        width: 800,
        height: 600,
        is_focused: true,
        is_minimized: false,
    };
    assert!(info.is_focused);
    assert!(!info.is_minimized);
    assert_eq!(info.title, "Test Window");
    assert_eq!(info.app_name, "test-app");
    assert_eq!(info.x, 100);
    assert_eq!(info.y, 200);
    assert_eq!(info.width, 800);
    assert_eq!(info.height, 600);
}

#[test]
fn test_window_info_serde_roundtrip() {
    let info = WindowInfo {
        id: WindowId(5),
        title: "Firefox".to_string(),
        app_name: "firefox".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        is_focused: false,
        is_minimized: true,
    };
    let json = serde_json::to_string(&info).unwrap();
    let parsed: WindowInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, WindowId(5));
    assert_eq!(parsed.title, "Firefox");
    assert!(!parsed.is_focused);
    assert!(parsed.is_minimized);
}

#[test]
fn test_window_manager_default() {
    let _wm = WindowManager::default();
    let _ = format!("{:?}", "WindowManager created");
}

#[cfg(target_os = "macos")]
#[test]
fn test_escape_applescript_string() {
    assert_eq!(
        escape_applescript_string(r#"App "Name"\Test"#),
        r#"App \"Name\"\\Test"#
    );
}

#[test]
fn test_display_server_detection() {
    // Test that detection runs without panicking
    let _server = DisplayServer::detect();
}

#[test]
fn test_display_server_equality() {
    assert_eq!(DisplayServer::X11, DisplayServer::X11);
    assert_eq!(DisplayServer::Wayland, DisplayServer::Wayland);
    assert_ne!(DisplayServer::X11, DisplayServer::Wayland);
    assert_ne!(DisplayServer::X11, DisplayServer::Unknown);
}

#[tokio::test]
async fn test_list_windows_does_not_panic() {
    let wm = WindowManager::new();
    // Should not panic regardless of platform or available tools
    let result = wm.list_windows().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_focus_window_does_not_panic() {
    let wm = WindowManager::new();
    // In headless environments this may fail but should not panic
    let _result = wm.focus_window(&WindowId(1)).await;
}

#[tokio::test]
async fn test_get_active_window_does_not_panic() {
    let wm = WindowManager::new();
    // In headless environments this will likely error but should not panic
    let _result = wm.get_active_window().await;
}

#[tokio::test]
async fn test_resize_window_does_not_panic() {
    let wm = WindowManager::new();
    let _result = wm.resize_window(&WindowId(1), 800, 600).await;
}

#[tokio::test]
async fn test_move_window_does_not_panic() {
    let wm = WindowManager::new();
    let _result = wm.move_window(&WindowId(1), 100, 100).await;
}

#[tokio::test]
async fn test_minimize_window_does_not_panic() {
    let wm = WindowManager::new();
    let _result = wm.minimize_window(&WindowId(1)).await;
}

#[tokio::test]
async fn test_close_window_does_not_panic() {
    let wm = WindowManager::new();
    let _result = wm.close_window(&WindowId(1)).await;
}

#[tokio::test]
async fn test_is_available_does_not_panic() {
    let wm = WindowManager::new();
    let _ = wm.is_available().await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_linux_list_windows_graceful() {
    // Unit test for graceful degradation - should always succeed
    let wm = WindowManager::new();
    let result = wm.list_windows().await;
    assert!(result.is_ok());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_linux_focus_window_graceful() {
    let wm = WindowManager::new();
    // Focus on a fake window ID - should fail gracefully, not panic
    let _result = wm.focus_window(&WindowId(999999)).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_linux_get_active_window_graceful() {
    let wm = WindowManager::new();
    // In headless CI, this will error but should not panic
    let _result = wm.get_active_window().await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_linux_window_operations_graceful() {
    let wm = WindowManager::new();
    // These should all fail gracefully, not panic
    let _ = wm.resize_window(&WindowId(999999), 800, 600).await;
    let _ = wm.move_window(&WindowId(999999), 100, 100).await;
    let _ = wm.minimize_window(&WindowId(999999)).await;
    let _ = wm.close_window(&WindowId(999999)).await;
}

// macOS-specific tests for window ID mapping and script generation
#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    #[test]
    fn test_window_id_to_app_mapping() {
        let wm = WindowManager::new();

        // Initially empty
        {
            let map = wm.window_id_to_app.lock().unwrap();
            assert!(map.is_empty());
        }

        // Insert some mappings
        {
            let mut map = wm.window_id_to_app.lock().unwrap();
            map.insert(WindowId(0), "Safari".to_string());
            map.insert(WindowId(1), "Terminal".to_string());
            map.insert(WindowId(2), "Code".to_string());
        }

        // Verify mappings
        {
            let map = wm.window_id_to_app.lock().unwrap();
            assert_eq!(map.get(&WindowId(0)), Some(&"Safari".to_string()));
            assert_eq!(map.get(&WindowId(1)), Some(&"Terminal".to_string()));
            assert_eq!(map.get(&WindowId(2)), Some(&"Code".to_string()));
            assert_eq!(map.get(&WindowId(999)), None);
        }
    }

    #[test]
    fn test_app_name_quoting_in_scripts() {
        // Test that app names with quotes are properly escaped
        let app_name = r#"My "App" Name"#;
        let expected = r#"My \"App\" Name"#;
        assert_eq!(app_name.replace('"', "\\\""), expected);

        // Test normal app name (no change)
        let app_name = "Safari";
        assert_eq!(app_name.replace('"', "\\\""), "Safari");
    }

    #[tokio::test]
    async fn test_focus_unknown_window_id_errors() {
        let wm = WindowManager::new();
        // Window ID not in map should return error
        let result = wm.focus_window(&WindowId(999)).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown window ID"));
    }

    #[test]
    fn test_window_manager_creation_macos() {
        let wm = WindowManager::new();
        // Should create with empty mapping
        let map = wm.window_id_to_app.lock().unwrap();
        assert!(map.is_empty());
    }

    // Stubs must error honestly with the action name, never silently succeed.
    #[tokio::test]
    async fn test_resize_window_errors_on_macos() {
        let wm = WindowManager::new();
        let err = wm
            .resize_window(&WindowId(1), 800, 600)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("resize_window"), "{err}");
    }

    #[tokio::test]
    async fn test_move_window_errors_on_macos() {
        let wm = WindowManager::new();
        let err = wm
            .move_window(&WindowId(1), 100, 100)
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("move_window"), "{err}");
    }

    #[tokio::test]
    async fn test_minimize_window_errors_on_macos() {
        let wm = WindowManager::new();
        let err = wm
            .minimize_window(&WindowId(1))
            .await
            .unwrap_err()
            .to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("minimize_window"), "{err}");
    }

    #[tokio::test]
    async fn test_close_window_errors_on_macos() {
        let wm = WindowManager::new();
        let err = wm.close_window(&WindowId(1)).await.unwrap_err().to_string();
        assert!(err.contains("not supported on macOS"), "{err}");
        assert!(err.contains("close_window"), "{err}");
    }
}
