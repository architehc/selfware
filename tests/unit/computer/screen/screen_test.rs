use super::*;

#[test]
fn test_screen_region() {
    let region = ScreenRegion::new(10, 20, 800, 600);
    assert_eq!(region.x, 10);
    assert_eq!(region.y, 20);
    assert_eq!(region.width, 800);
    assert_eq!(region.height, 600);
}

#[test]
fn test_screen_region_negative_coords() {
    let region = ScreenRegion::new(-100, -50, 800, 600);
    assert_eq!(region.x, -100);
    assert_eq!(region.y, -50);
}

#[test]
fn test_screen_region_serde_roundtrip() {
    let region = ScreenRegion::new(10, 20, 800, 600);
    let json = serde_json::to_string(&region).unwrap();
    let parsed: ScreenRegion = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.x, 10);
    assert_eq!(parsed.y, 20);
    assert_eq!(parsed.width, 800);
    assert_eq!(parsed.height, 600);
}

#[test]
fn test_validate_dimensions() {
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 1920, 1080).is_ok());
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 0, 100).is_err());
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 50000, 100).is_err());
}

#[test]
fn test_validate_dimensions_zero_width() {
    let result = ScreenCapture::validate_capture_dimensions(0, 0, 0, 600);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("non-zero"));
}

#[test]
fn test_validate_dimensions_zero_height() {
    let result = ScreenCapture::validate_capture_dimensions(0, 0, 800, 0);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("non-zero"));
}

#[test]
fn test_validate_dimensions_at_max() {
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 32768, 32768).is_ok());
}

#[test]
fn test_validate_dimensions_over_max() {
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 32769, 100).is_err());
    assert!(ScreenCapture::validate_capture_dimensions(0, 0, 100, 32769).is_err());
}

#[test]
fn test_validate_dimensions_both_over_max() {
    let result = ScreenCapture::validate_capture_dimensions(0, 0, 40000, 40000);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("too large"));
}

#[test]
fn test_captured_screen_serde() {
    let screen = CapturedScreen {
        width: 1920,
        height: 1080,
        base64_png: "iVBOR...".to_string(),
        analysis: None,
    };
    let json = serde_json::to_string(&screen).unwrap();
    let parsed: CapturedScreen = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.width, 1920);
    assert_eq!(parsed.height, 1080);
    assert!(parsed.analysis.is_none());
    // analysis should be skipped when None
    assert!(!json.contains("analysis"));
}

#[test]
fn test_captured_screen_with_analysis() {
    let screen = CapturedScreen {
        width: 800,
        height: 600,
        base64_png: "data...".to_string(),
        analysis: Some("A terminal window showing code".to_string()),
    };
    let json = serde_json::to_string(&screen).unwrap();
    assert!(json.contains("analysis"));
    let parsed: CapturedScreen = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.analysis.unwrap(), "A terminal window showing code");
}

#[test]
fn test_osrelease_looks_like_wsl() {
    assert!(ScreenCapture::osrelease_looks_like_wsl(
        "5.15.167.4-microsoft-standard-WSL2"
    ));
    assert!(ScreenCapture::osrelease_looks_like_wsl(
        "6.6.87.2-Microsoft"
    ));
    assert!(!ScreenCapture::osrelease_looks_like_wsl("6.8.0-55-generic"));
}

#[test]
fn test_parse_windows_capture_payload() {
    let parsed = ScreenCapture::parse_windows_capture_payload(
        br#"{"width":1920,"height":1080,"base64_png":"ZmFrZV9wbmc="}"#,
    )
    .unwrap();
    assert_eq!(parsed.width, 1920);
    assert_eq!(parsed.height, 1080);
    assert_eq!(parsed.base64_png, "ZmFrZV9wbmc=");
    assert!(parsed.analysis.is_none());
}

#[test]
fn test_parse_windows_capture_payload_rejects_empty_image() {
    let result = ScreenCapture::parse_windows_capture_payload(
        br#"{"width":1920,"height":1080,"base64_png":""}"#,
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty image"));
}
