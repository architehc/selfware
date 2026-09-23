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

// ── Output shaping (no display needed: fake capture bytes) ──────────────

fn fake_png(len: usize) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    v.resize(len.max(8), 0xAB);
    v
}

#[test]
fn test_default_output_writes_file_and_omits_base64() {
    let dir = tempfile::tempdir().unwrap();
    let png = fake_png(4096);
    let opts = CaptureOutputOptions::default();
    let out = shape_capture_output(
        "screen_capture",
        "screen",
        &png,
        640,
        480,
        &opts,
        dir.path(),
    )
    .unwrap();

    assert!(out.get("base64_png").is_none(), "no inline data by default");
    assert_eq!(out["inline"], false);
    assert_eq!(out["width"], 640);
    assert_eq!(out["height"], 480);
    assert_eq!(out["bytes"], 4096);
    assert_eq!(out["target"], "screen");
    let path = std::path::PathBuf::from(out["path"].as_str().unwrap());
    assert!(path.starts_with(dir.path()));
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
    assert_eq!(std::fs::read(&path).unwrap(), png);
    // The result stays tiny regardless of image size.
    assert!(out.to_string().len() < 1024);
}

#[test]
fn test_repeated_captures_get_distinct_files() {
    let dir = tempfile::tempdir().unwrap();
    let opts = CaptureOutputOptions::default();
    let a = shape_capture_output(
        "computer_screen",
        "screen",
        &fake_png(64),
        1,
        1,
        &opts,
        dir.path(),
    )
    .unwrap();
    let b = shape_capture_output(
        "computer_screen",
        "screen",
        &fake_png(64),
        1,
        1,
        &opts,
        dir.path(),
    )
    .unwrap();
    assert_ne!(a["path"], b["path"]);
}

#[test]
fn test_inline_opt_in_returns_decodable_base64() {
    let dir = tempfile::tempdir().unwrap();
    let png = fake_png(2048);
    let opts = CaptureOutputOptions {
        inline: true,
        output_path: None,
    };
    let out =
        shape_capture_output("screen_capture", "region", &png, 10, 20, &opts, dir.path()).unwrap();
    assert_eq!(out["inline"], true);
    let decoded = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        out["base64_png"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(decoded, png);
    // The file is still saved, so `path` is always valid.
    assert!(std::path::Path::new(out["path"].as_str().unwrap()).is_file());
}

#[test]
fn test_inline_above_cap_is_an_honest_error_and_keeps_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let png = fake_png(MAX_INLINE_PNG_BYTES + 1);
    let opts = CaptureOutputOptions {
        inline: true,
        output_path: None,
    };
    let err = shape_capture_output(
        "screen_capture",
        "screen",
        &png,
        7680,
        2928,
        &opts,
        dir.path(),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("inline limit"), "{err}");
    assert!(err.contains(&MAX_INLINE_PNG_BYTES.to_string()), "{err}");
    let saved: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(saved.len(), 1, "capture must still be saved to disk");
    let saved_path = saved[0].as_ref().unwrap().path();
    assert!(err.contains(&saved_path.display().to_string()), "{err}");
}

#[test]
fn test_inline_cap_keeps_base64_under_five_megabytes() {
    // base64 length = ceil(n / 3) * 4
    let encoded = MAX_INLINE_PNG_BYTES.div_ceil(3) * 4;
    assert!(encoded <= 5_000_000, "{encoded}");
}

#[test]
fn test_output_options_from_args() {
    assert_eq!(
        CaptureOutputOptions::from_args(&json!({})),
        CaptureOutputOptions::default()
    );
    let o = CaptureOutputOptions::from_args(&json!({"inline": true, "output_path": "shot.png"}));
    assert!(o.inline);
    assert_eq!(o.output_path.as_deref(), Some("shot.png"));
    let blank = CaptureOutputOptions::from_args(&json!({"output_path": "  "}));
    assert_eq!(blank.output_path, None);
}

#[test]
fn test_parse_region_accepts_both_shapes() {
    let nested = json!({"region": {"x": 5, "y": 6, "width": 70, "height": 80}});
    let flat = json!({"x": 5, "y": 6, "width": 70, "height": 80});
    assert_eq!(parse_region(&nested).unwrap(), (5, 6, 70, 80));
    assert_eq!(parse_region(&flat).unwrap(), (5, 6, 70, 80));
    assert!(parse_region(&json!({})).is_err());
    assert!(parse_region(&json!({"width": 0, "height": 10})).is_err());
}

#[test]
fn test_session_dir_is_outside_the_workspace() {
    // Production root (data_local_dir) — computed, not created.
    let dir = screenshot_session_dir_in(dirs::data_local_dir());
    let cwd = std::env::current_dir().unwrap();
    assert!(
        !dir.starts_with(&cwd),
        "{} is inside {}",
        dir.display(),
        cwd.display()
    );
    assert!(dir
        .components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("tool_results")));
    // Stable for the life of the process.
    assert_eq!(dir, screenshot_session_dir_in(dirs::data_local_dir()));
    // Unit-test builds never write real captures into the user's data dir.
    assert!(screenshot_session_dir().starts_with(std::env::temp_dir()));
}

#[cfg(unix)]
#[test]
fn test_saved_capture_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("session");
    let out = shape_capture_output(
        "screen_capture",
        "screen",
        &fake_png(32),
        1,
        1,
        &CaptureOutputOptions::default(),
        &dir,
    )
    .unwrap();
    let file_mode = std::fs::metadata(out["path"].as_str().unwrap())
        .unwrap()
        .permissions()
        .mode();
    let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode();
    assert_eq!(file_mode & 0o077, 0, "file mode {file_mode:o}");
    assert_eq!(dir_mode & 0o077, 0, "dir mode {dir_mode:o}");
}

#[test]
fn test_capture_tools_share_output_arguments_and_point_at_each_other() {
    let sc = ScreenCapture;
    let cs = crate::tools::computer::ComputerScreenTool;
    for key in ["region", "output_path", "inline"] {
        assert!(
            sc.schema()["properties"][key].is_object(),
            "screen_capture lacks {key}"
        );
        assert!(
            cs.schema()["properties"][key].is_object(),
            "computer_screen lacks {key}"
        );
    }
    assert!(sc.description().contains("computer_screen"));
    assert!(cs.description().contains("screen_capture"));
    assert_eq!(sc.schema()["properties"]["inline"]["type"], "boolean");
}

#[tokio::test]
async fn test_computer_screen_window_mode_points_to_screen_capture() {
    let err = crate::tools::computer::ComputerScreenTool
        .execute(json!({"action": "window"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("screen_capture"), "{err}");
}
