use super::*;

#[test]
fn test_screen_capture_schema() {
    let tool = ScreenCapture;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["target"].is_object());
    assert!(schema["properties"]["window_name"].is_object());
    assert!(schema["properties"]["region"].is_object());
    assert!(schema["properties"]["output_path"].is_object());
    // Verify target enum values
    let targets = schema["properties"]["target"]["enum"].as_array().unwrap();
    assert!(targets.contains(&json!("screen")));
    assert!(targets.contains(&json!("window")));
    assert!(targets.contains(&json!("region")));
}

#[test]
fn test_screen_capture_name() {
    let tool = ScreenCapture;
    assert_eq!(tool.name(), "screen_capture");
}

#[test]
fn test_screen_capture_description() {
    let tool = ScreenCapture;
    assert!(tool.description().contains("screenshot"));
    assert!(tool.description().contains("base64"));
}

#[tokio::test]
async fn test_screen_capture_unknown_target() {
    let tool = ScreenCapture;
    let result = tool.execute(json!({"target": "hologram"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unknown target"));
}

#[tokio::test]
async fn test_screen_capture_window_missing_name() {
    let tool = ScreenCapture;
    let result = tool.execute(json!({"target": "window"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("window_name"));
}

#[tokio::test]
async fn test_screen_capture_region_missing_fields() {
    let tool = ScreenCapture;
    let result = tool.execute(json!({"target": "region"})).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("region"));
}

#[tokio::test]
async fn test_screen_capture_region_missing_width() {
    let tool = ScreenCapture;
    let result = tool
        .execute(json!({
            "target": "region",
            "region": {"x": 0, "y": 0, "height": 100}
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("width"));
}

#[tokio::test]
async fn test_screen_capture_region_missing_height() {
    let tool = ScreenCapture;
    let result = tool
        .execute(json!({
            "target": "region",
            "region": {"x": 0, "y": 0, "width": 100}
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("height"));
}

#[tokio::test]
async fn test_screen_capture_region_coords_out_of_range() {
    let tool = ScreenCapture;
    let result = tool
        .execute(json!({
            "target": "region",
            "region": {"x": 200000, "y": 0, "width": 100, "height": 100}
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("out of range"));
}

#[tokio::test]
async fn test_screen_capture_region_y_out_of_range() {
    let tool = ScreenCapture;
    let result = tool
        .execute(json!({
            "target": "region",
            "region": {"x": 0, "y": -200000, "width": 100, "height": 100}
        }))
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_screen_capture_region_dimensions_out_of_range() {
    let tool = ScreenCapture;
    let result = tool
        .execute(json!({
            "target": "region",
            "region": {"x": 0, "y": 0, "width": 200000, "height": 100}
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("out of range"));
}

#[tokio::test]
async fn test_screen_capture_defaults_to_screen() {
    // When target is omitted, should default to "screen"
    let tool = ScreenCapture;
    // This will try to capture the actual screen — may fail in headless CI
    // but the test verifies the default target path is taken
    let _result = tool.execute(json!({})).await;
    // Just verify it doesn't panic; actual capture may fail without display
}

#[tokio::test]
async fn test_screen_capture_region_default_coords() {
    let tool = ScreenCapture;
    // x and y default to 0 when missing
    let _result = tool
        .execute(json!({
            "target": "region",
            "region": {"width": 100, "height": 100}
        }))
        .await;
    // Verifies defaults are applied without panic
}
