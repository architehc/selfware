use super::*;

#[test]
fn test_window_id() {
    let id = WindowId(42);
    assert_eq!(id.0, 42);
}

// AGENTS.md Rule 6 pure helpers: halving transform, visible-region bounds,
// clamping, ownership, and the wmctrl -lG geometry parser. All platform-free
// so they are exercised without touching a real desktop.
#[test]
fn wmctrl_request_position_halves_for_mutter() {
    // mutter doubles wmctrl -e position requests: request = target / 2.
    assert_eq!(wmctrl_request_position(1920, 800), (960, 400));
    assert_eq!(wmctrl_request_position(0, 768), (0, 384));
    assert_eq!(wmctrl_request_position(7680, 2928), (3840, 1464));
    // Odd targets floor — at most one device pixel short, never past target.
    assert_eq!(wmctrl_request_position(1921, 799), (960, 399));
}

#[test]
fn placement_bounds_use_the_rule6_visible_region() {
    // Visible area: x 0-7680, y 768-2928 device coords. The small VGA monitor
    // sits ABOVE at y < 768 (x 3840-4864) and must never receive a placement.
    assert!(placement_inside_visible_region(0, 768, 1920, 1080));
    assert!(
        !placement_inside_visible_region(0, 767, 1920, 1080),
        "above the visible top edge"
    );
    assert!(
        !placement_inside_visible_region(3844, 100, 800, 600),
        "on the small VGA monitor"
    );
    assert!(
        !placement_inside_visible_region(6000, 768, 1800, 400),
        "x + width beyond 7680"
    );
    assert!(
        !placement_inside_visible_region(0, 768, 100, 3000),
        "y + height beyond 2928"
    );
    assert!(
        !placement_inside_visible_region(-10, 768, 100, 100),
        "negative x"
    );
}

#[test]
fn clamp_to_visible_region_pulls_wayward_placements_back() {
    // A target above the visible top edge is exactly the failure the halving
    // fix (and its missing sanity check) used to produce: y=0 device requests
    // landed above the visible region.
    let (cx, cy) = clamp_to_visible_region(1920, 200, 1200, 800);
    assert_eq!(cy, 768, "clamped onto the visible top edge");
    assert_eq!(cx, 1920, "in-bounds x is untouched");
    let (cx, cy) = clamp_to_visible_region(9000, 4000, 1200, 800);
    assert!(cx + 1200 <= 7680 && cy >= 768 && cy + 800 <= 2928);
    // A window wider than the display still clamps x without panicking.
    let (cx, _) = clamp_to_visible_region(100, 768, 9000, 400);
    assert_eq!(cx, 0);
}

#[test]
fn session_owns_window_matches_sw_prefix_only() {
    assert!(session_owns_window("sw-1: study terminal"));
    assert!(session_owns_window(" sw-arena-7 "));
    assert!(!session_owns_window("Firefox"));
    assert!(!session_owns_window("Terminal — sw-2")); // prefix, not contains
    assert!(!session_owns_window(""));
}

#[test]
fn wmctrl_lg_geometry_is_extracted_for_the_target_id() {
    let listing = b"0x04600003  0 12345 1920 800 1200 800 hostname sw-1: study\n0x03200001  1 54321 3844 100 800 600 hostname Firefox\n";
    assert_eq!(
        window_geometry_from_wmctrl_lg(listing, 0x04600003),
        Some((1920, 800, 1200, 800))
    );
    assert_eq!(
        window_geometry_from_wmctrl_lg(listing, 0x03200001),
        Some((3844, 100, 800, 600))
    );
    assert_eq!(window_geometry_from_wmctrl_lg(listing, 0xdeadbeef), None);
    // Malformed lines must not poison the scan for the target.
    let garbled =
        b"garbage\n0x1  1 2 3 4 5 6 host title\n0x04600003  0 1 100 800 900 600 host sw-1\n";
    assert_eq!(
        window_geometry_from_wmctrl_lg(garbled, 0x04600003),
        Some((100, 800, 900, 600))
    );
    assert_eq!(window_geometry_from_wmctrl_lg(b"", 1), None);
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

    #[tokio::test]
    async fn test_focus_known_window_id_succeeds_in_test_mode() {
        let wm = WindowManager::new();
        {
            let mut map = wm.window_id_to_app.lock().unwrap();
            map.insert(WindowId(42), "Safari".to_string());
        }
        let result = wm.focus_window(&WindowId(42)).await;
        assert!(
            result.is_ok(),
            "focusing known window id must succeed via test stub without raising desktop windows"
        );
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
