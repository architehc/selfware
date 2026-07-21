use super::*;

#[test]
fn test_point() {
    let p = Point::new(100, 200);
    assert_eq!(p.x, 100);
    assert_eq!(p.y, 200);
}

#[test]
fn test_point_negative() {
    let p = Point::new(-50, -100);
    assert_eq!(p.x, -50);
    assert_eq!(p.y, -100);
}

#[test]
fn test_point_serde_roundtrip() {
    let p = Point::new(42, 84);
    let json = serde_json::to_string(&p).unwrap();
    let parsed: Point = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.x, 42);
    assert_eq!(parsed.y, 84);
}

#[test]
fn test_validate_coordinates() {
    let mouse = MouseController::new();
    assert!(mouse.validate_coordinates(100, 200).is_ok());
    assert!(mouse.validate_coordinates(-100, 200).is_ok());
    assert!(mouse.validate_coordinates(50000, 200).is_err());
}

#[test]
fn test_validate_coordinates_boundary() {
    let mouse = MouseController::new();
    // Exactly at the 32768 boundary
    assert!(mouse.validate_coordinates(32768, 0).is_ok());
    assert!(mouse.validate_coordinates(0, 32768).is_ok());
    // Just over
    assert!(mouse.validate_coordinates(32769, 0).is_err());
    assert!(mouse.validate_coordinates(0, 32769).is_err());
    // Negative boundary
    assert!(mouse.validate_coordinates(-32768, 0).is_ok());
    assert!(mouse.validate_coordinates(-32769, 0).is_err());
}

#[test]
fn test_validate_coordinates_both_out_of_range() {
    let mouse = MouseController::new();
    assert!(mouse.validate_coordinates(50000, 50000).is_err());
}

#[tokio::test]
async fn test_move_to_valid() {
    let mouse = MouseController::new();
    assert!(mouse.move_to(100, 200).await.is_ok());
}

#[tokio::test]
async fn test_move_to_out_of_bounds() {
    let mouse = MouseController::new();
    assert!(mouse.move_to(50000, 200).await.is_err());
}

#[tokio::test]
async fn test_click() {
    let mouse = MouseController::new();
    assert!(mouse.click(MouseButton::Left).await.is_ok());
    assert!(mouse.click(MouseButton::Right).await.is_ok());
    assert!(mouse.click(MouseButton::Middle).await.is_ok());
}

#[tokio::test]
async fn test_double_click() {
    let mouse = MouseController::new();
    assert!(mouse.double_click().await.is_ok());
}

#[tokio::test]
async fn test_click_at() {
    let mouse = MouseController::new();
    assert!(mouse.click_at(100, 200, MouseButton::Left).await.is_ok());
}

#[tokio::test]
async fn test_click_at_out_of_bounds() {
    let mouse = MouseController::new();
    assert!(mouse.click_at(50000, 200, MouseButton::Left).await.is_err());
}

#[tokio::test]
async fn test_scroll() {
    let mouse = MouseController::new();
    assert!(mouse.scroll(0, -3).await.is_ok());
    assert!(mouse.scroll(5, 5).await.is_ok());
    assert!(mouse.scroll(0, 0).await.is_ok());
}

#[tokio::test]
async fn test_drag_valid() {
    let mouse = MouseController::new();
    let from = Point::new(10, 10);
    let to = Point::new(200, 200);
    assert!(mouse.drag(from, to, MouseButton::Left).await.is_ok());
}

#[tokio::test]
async fn test_drag_out_of_bounds() {
    let mouse = MouseController::new();
    let from = Point::new(10, 10);
    let to = Point::new(50000, 200);
    assert!(mouse.drag(from, to, MouseButton::Left).await.is_err());
}

#[test]
fn test_mouse_controller_default() {
    let mouse = MouseController::default();
    assert!(mouse.validate_coordinates(0, 0).is_ok());
}

#[test]
fn test_with_movement_profile() {
    let mouse = MouseController::new().with_movement_profile(super::super::MovementProfile::Bezier);
    assert!(mouse.validate_coordinates(0, 0).is_ok());
}

#[test]
fn test_mouse_button_serde() {
    let buttons = vec![MouseButton::Left, MouseButton::Right, MouseButton::Middle];
    for button in buttons {
        let json = serde_json::to_string(&button).unwrap();
        let parsed: MouseButton = serde_json::from_str(&json).unwrap();
        let _ = format!("{:?}", parsed);
    }
}

// ---- Command construction tests ----

#[test]
fn test_mouse_button_xdotool_numbers() {
    assert_eq!(MouseButton::Left.xdotool_button(), 1);
    assert_eq!(MouseButton::Middle.xdotool_button(), 2);
    assert_eq!(MouseButton::Right.xdotool_button(), 3);
}

#[test]
fn test_build_mousemove_args() {
    let args = build_mousemove_args(100, 200);
    assert_eq!(args, vec!["mousemove", "100", "200"]);
}

#[test]
fn test_build_mousemove_args_negative() {
    let args = build_mousemove_args(-50, -100);
    assert_eq!(args, vec!["mousemove", "-50", "-100"]);
}

#[test]
fn test_build_click_args() {
    assert_eq!(build_click_args(1), vec!["click", "1"]);
    assert_eq!(build_click_args(2), vec!["click", "2"]);
    assert_eq!(build_click_args(3), vec!["click", "3"]);
}

#[test]
fn test_build_double_click_args() {
    let args = build_double_click_args();
    assert_eq!(args, vec!["click", "--repeat", "2", "1"]);
}

#[test]
fn test_build_scroll_args_down() {
    let commands = build_scroll_args(0, 3);
    assert_eq!(commands.len(), 3);
    for cmd in &commands {
        assert_eq!(cmd, &vec!["click".to_string(), "5".to_string()]);
    }
}

#[test]
fn test_build_scroll_args_up() {
    let commands = build_scroll_args(0, -2);
    assert_eq!(commands.len(), 2);
    for cmd in &commands {
        assert_eq!(cmd, &vec!["click".to_string(), "4".to_string()]);
    }
}

#[test]
fn test_build_scroll_args_horizontal() {
    // Scroll right
    let commands = build_scroll_args(2, 0);
    assert_eq!(commands.len(), 2);
    for cmd in &commands {
        assert_eq!(cmd, &vec!["click".to_string(), "7".to_string()]);
    }

    // Scroll left
    let commands = build_scroll_args(-1, 0);
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0], vec!["click".to_string(), "6".to_string()]);
}

#[test]
fn test_build_scroll_args_zero() {
    let commands = build_scroll_args(0, 0);
    assert!(commands.is_empty());
}

#[test]
fn test_build_scroll_args_both_axes() {
    let commands = build_scroll_args(1, -1);
    // 1 vertical (up) + 1 horizontal (right)
    assert_eq!(commands.len(), 2);
    assert_eq!(commands[0], vec!["click".to_string(), "4".to_string()]); // up
    assert_eq!(commands[1], vec!["click".to_string(), "7".to_string()]); // right
}

#[test]
fn test_build_drag_args() {
    let from = Point::new(10, 20);
    let to = Point::new(300, 400);
    let args = build_drag_args(from, to, 1);
    assert_eq!(
        args,
        vec![
            "mousemove",
            "10",
            "20",
            "mousedown",
            "1",
            "mousemove",
            "300",
            "400",
            "mouseup",
            "1"
        ]
    );
}

#[test]
fn test_build_drag_args_right_button() {
    let from = Point::new(0, 0);
    let to = Point::new(100, 100);
    let args = build_drag_args(from, to, 3);
    assert_eq!(args[4], "3"); // mousedown button
    assert_eq!(args[9], "3"); // mouseup button
}

// ---- PowerShell script construction tests ----

#[test]
fn test_ps_move_to_contains_coordinates() {
    let script = ps_move_to(100, 200);
    assert!(script.contains("Point(100, 200)"));
    assert!(script.contains("Cursor"));
}

#[test]
fn test_ps_click_left() {
    let script = ps_click(&MouseButton::Left);
    assert!(script.contains("MOUSEEVENTF_LEFTDOWN"));
    assert!(script.contains("MOUSEEVENTF_LEFTUP"));
}

#[test]
fn test_ps_click_right() {
    let script = ps_click(&MouseButton::Right);
    assert!(script.contains("MOUSEEVENTF_RIGHTDOWN"));
    assert!(script.contains("MOUSEEVENTF_RIGHTUP"));
}

#[test]
fn test_ps_click_middle() {
    let script = ps_click(&MouseButton::Middle);
    assert!(script.contains("MOUSEEVENTF_MIDDLEDOWN"));
    assert!(script.contains("MOUSEEVENTF_MIDDLEUP"));
}

#[test]
fn test_ps_double_click_has_two_pairs() {
    let script = ps_double_click();
    // The preamble defines the constants (1 occurrence each), plus 2 calls each
    assert_eq!(script.matches("MOUSEEVENTF_LEFTDOWN").count(), 3);
    assert_eq!(script.matches("MOUSEEVENTF_LEFTUP").count(), 3);
}

#[test]
fn test_ps_scroll_down() {
    let script = ps_scroll(0, 3);
    // delta_y=3 (scroll down) -> wheel_amount = -3*120 = -360
    assert!(script.contains("-360"));
    assert!(script.contains("MOUSEEVENTF_WHEEL"));
}

#[test]
fn test_ps_scroll_up() {
    let script = ps_scroll(0, -2);
    // delta_y=-2 (scroll up) -> wheel_amount = 2*120 = 240
    assert!(script.contains("240"));
}

#[test]
fn test_ps_scroll_zero() {
    let script = ps_scroll(0, 0);
    // The preamble defines MOUSEEVENTF_WHEEL as a constant, but there
    // should be no *call* to mouse_event with WHEEL when both deltas are 0.
    // Count: preamble has 1 occurrence in the const definition, no extra calls.
    assert_eq!(
        script.matches("MOUSEEVENTF_WHEEL").count(),
        1,
        "Only the preamble constant definition should mention WHEEL"
    );
}

#[test]
fn test_ps_drag_contains_coordinates() {
    let from = Point::new(10, 20);
    let to = Point::new(300, 400);
    let script = ps_drag(from, to, &MouseButton::Left);
    assert!(script.contains("Point(10, 20)"));
    assert!(script.contains("Point(300, 400)"));
    assert!(script.contains("MOUSEEVENTF_LEFTDOWN"));
    assert!(script.contains("MOUSEEVENTF_LEFTUP"));
}
