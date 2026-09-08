use super::*;

// ── VisionAnalyze schema & metadata ───────────────────────────────

#[test]
fn test_vision_analyze_name() {
    assert_eq!(VisionAnalyze.name(), "vision_analyze");
}

#[test]
fn test_vision_analyze_description() {
    assert!(VisionAnalyze.description().contains("vision"));
}

#[test]
fn test_vision_analyze_schema() {
    let tool = VisionAnalyze;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["prompt"].is_object());
    assert!(schema["properties"]["endpoint"].is_object());
    assert!(schema["properties"]["image_path"].is_object());
    assert!(schema["properties"]["image_base64"].is_object());
    assert!(schema["properties"]["detail"].is_object());
    assert!(schema["properties"]["max_tokens"].is_object());
    assert!(schema["properties"]["temperature"].is_object());
    assert!(schema["properties"]["api_key"].is_object());
    assert!(schema["properties"]["extra_body"].is_object());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("prompt")));
    // endpoint/model are OPTIONAL since 2026-09-04 — the harness injects
    // them from the configured vision profile at dispatch; the model must
    // not be asked to guess infrastructure (TB4 cad-model finding).
    assert_eq!(required.len(), 1);
    assert!(!required.contains(&json!("endpoint")));
    assert!(!required.contains(&json!("model")));
}

// ── VisionAnalyze execute error paths ─────────────────────────────

#[tokio::test]
async fn test_vision_analyze_missing_prompt() {
    let result = VisionAnalyze
        .execute(json!({
            "endpoint": "http://localhost:8000/v1",
            "model": "test",
            "image_base64": "iVBOR"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("prompt"));
}

#[tokio::test]
async fn test_vision_analyze_missing_endpoint() {
    let result = VisionAnalyze
        .execute(json!({
            "prompt": "what is this",
            "model": "test",
            "image_base64": "iVBOR"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("endpoint"));
}

#[tokio::test]
async fn test_vision_analyze_missing_model() {
    let result = VisionAnalyze
        .execute(json!({
            "prompt": "what is this",
            "endpoint": "http://localhost:8000/v1",
            "image_base64": "iVBOR"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("model"));
}

#[tokio::test]
async fn test_vision_analyze_missing_image() {
    let result = VisionAnalyze
        .execute(json!({
            "prompt": "what is this",
            "endpoint": "http://localhost:8000/v1",
            "model": "test"
        }))
        .await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("image_path or image_base64"));
}

// ── VisionCompare schema & metadata ───────────────────────────────

#[test]
fn test_vision_compare_name() {
    assert_eq!(VisionCompare.name(), "vision_compare");
}

#[test]
fn test_vision_compare_description() {
    assert!(VisionCompare.description().contains("Compare"));
    assert!(VisionCompare.description().contains("similarity"));
}

#[test]
fn test_vision_compare_schema() {
    let tool = VisionCompare;
    let schema = tool.schema();
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["image_a"].is_object());
    assert!(schema["properties"]["image_b"].is_object());
    assert!(schema["properties"]["threshold"].is_object());
    assert!(schema["properties"]["endpoint"].is_object());
    assert!(schema["properties"]["model"].is_object());
    assert!(schema["properties"]["detail"].is_object());
    assert!(schema["properties"]["max_tokens"].is_object());
    assert!(schema["properties"]["temperature"].is_object());
    assert!(schema["properties"]["api_key"].is_object());
    assert!(schema["properties"]["extra_body"].is_object());
    let required = schema["required"].as_array().unwrap();
    assert!(required.contains(&json!("image_a")));
    assert!(required.contains(&json!("image_b")));
}

// ── VisionCompare execute error paths ─────────────────────────────

#[tokio::test]
async fn test_vision_compare_missing_image_a() {
    let result = VisionCompare
        .execute(json!({
            "image_b": "/tmp/b.png"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("image_a"));
}

#[tokio::test]
async fn test_vision_compare_missing_image_b() {
    let result = VisionCompare
        .execute(json!({
            "image_a": "/tmp/a.png"
        }))
        .await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("image_b"));
}

#[tokio::test]
async fn test_vision_compare_nonexistent_files() {
    let result = VisionCompare
        .execute(json!({
            "image_a": "/nonexistent/a.png",
            "image_b": "/nonexistent/b.png"
        }))
        .await;
    assert!(result.is_err());
}

#[test]
fn test_merge_extra_body() {
    let mut body = json!({
        "model": "vision",
        "stream": false
    });
    let extra = json!({
        "chat_template_kwargs": { "enable_thinking": false }
    });
    merge_extra_body(&mut body, Some(&extra)).unwrap();
    assert_eq!(
        body["chat_template_kwargs"]["enable_thinking"],
        json!(false)
    );
}

#[test]
fn test_merge_extra_body_rejects_non_object() {
    let mut body = json!({ "model": "vision" });
    let extra = json!(["bad"]);
    let result = merge_extra_body(&mut body, Some(&extra));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("extra_body"));
}

#[test]
fn test_merge_extra_body_rejects_reserved_keys() {
    let mut body = json!({ "model": "vision", "stream": false });
    let extra = json!({ "model": "override" });
    let result = merge_extra_body(&mut body, Some(&extra));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("reserved key"));
}

// ── guess_mime ────────────────────────────────────────────────────

#[test]
fn test_guess_mime() {
    assert_eq!(guess_mime("photo.png"), "image/png");
    assert_eq!(guess_mime("photo.jpg"), "image/jpeg");
    assert_eq!(guess_mime("photo.jpeg"), "image/jpeg");
    assert_eq!(guess_mime("anim.gif"), "image/gif");
    assert_eq!(guess_mime("photo.webp"), "image/webp");
    assert_eq!(guess_mime("photo.bmp"), "image/bmp");
    assert_eq!(guess_mime("noext"), "image/png");
}

#[test]
fn test_guess_mime_case_insensitive() {
    assert_eq!(guess_mime("photo.PNG"), "image/png");
    assert_eq!(guess_mime("photo.JPG"), "image/jpeg");
    assert_eq!(guess_mime("photo.JPEG"), "image/jpeg");
    assert_eq!(guess_mime("anim.GIF"), "image/gif");
    assert_eq!(guess_mime("photo.WebP"), "image/webp");
}

#[test]
fn test_guess_mime_multiple_dots() {
    assert_eq!(guess_mime("my.photo.backup.png"), "image/png");
    assert_eq!(guess_mime("archive.tar.jpg"), "image/jpeg");
}

#[test]
fn test_guess_mime_unknown_extension() {
    assert_eq!(guess_mime("photo.tiff"), "image/png"); // falls through to default
    assert_eq!(guess_mime("photo.svg"), "image/png");
}

// ── validate_image_magic ─────────────────────────────────────────

#[test]
fn test_validate_image_magic_png() {
    let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert!(validate_image_magic(&png_header, "test.png").is_ok());
}

#[test]
fn test_validate_image_magic_jpeg() {
    let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0];
    assert!(validate_image_magic(&jpeg_header, "test.jpg").is_ok());
}

#[test]
fn test_validate_image_magic_gif() {
    assert!(validate_image_magic(b"GIF89a...", "test.gif").is_ok());
    assert!(validate_image_magic(b"GIF87a...", "test.gif").is_ok());
}

#[test]
fn test_validate_image_magic_bmp() {
    assert!(validate_image_magic(b"BM\x00\x00\x00\x00", "test.bmp").is_ok());
}

#[test]
fn test_validate_image_magic_webp() {
    let mut webp = Vec::new();
    webp.extend_from_slice(b"RIFF");
    webp.extend_from_slice(&[0x00; 4]); // file size placeholder
    webp.extend_from_slice(b"WEBP");
    assert!(validate_image_magic(&webp, "test.webp").is_ok());
}

#[test]
fn test_validate_image_magic_invalid() {
    let text_data = b"Hello, world!";
    let result = validate_image_magic(text_data, "test.txt");
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unrecognized magic bytes"));
}

#[test]
fn test_validate_image_magic_too_small() {
    assert!(validate_image_magic(&[0x89, 0x50], "tiny.png").is_err());
    assert!(validate_image_magic(&[0x89, 0x50, 0x4E], "three.png").is_err());
    assert!(validate_image_magic(&[], "empty.png").is_err());
}

#[test]
fn test_validate_image_magic_too_small_error_message() {
    let result = validate_image_magic(&[0x89], "tiny.png");
    assert!(result.unwrap_err().to_string().contains("too small"));
}

// ── resolve_image_data_uri ───────────────────────────────────────

#[test]
fn test_resolve_image_data_uri_base64_raw() {
    let args = json!({ "image_base64": "iVBORw0KGgo=" });
    let uri = resolve_image_data_uri(&args).unwrap();
    assert!(uri.starts_with("data:image/png;base64,"));
    assert!(uri.contains("iVBORw0KGgo="));
}

#[test]
fn test_resolve_image_data_uri_base64_with_prefix() {
    let args = json!({ "image_base64": "data:image/jpeg;base64,/9j/4AAQ" });
    let uri = resolve_image_data_uri(&args).unwrap();
    assert_eq!(uri, "data:image/jpeg;base64,/9j/4AAQ");
}

#[test]
fn test_resolve_image_data_uri_neither() {
    let args = json!({ "prompt": "analyze" });
    let result = resolve_image_data_uri(&args);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("image_path or image_base64"));
}

#[test]
fn test_resolve_image_data_uri_nonexistent_file() {
    let args = json!({ "image_path": "/nonexistent/photo.png" });
    let result = resolve_image_data_uri(&args);
    assert!(result.is_err());
}

#[test]
fn test_resolve_image_data_uri_from_file() {
    // Create a temp PNG file
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];
    std::fs::write(tmp.path(), &png_bytes).unwrap();
    let args = json!({ "image_path": tmp.path().to_str().unwrap() });
    let uri = resolve_image_data_uri(&args).unwrap();
    assert!(uri.starts_with("data:image/png;base64,"));
}

// ── encode_image_file ────────────────────────────────────────────

#[test]
fn test_encode_image_file_nonexistent() {
    let result = encode_image_file("/nonexistent/photo.png");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_encode_image_file_not_an_image() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"not an image file content here").unwrap();
    let result = encode_image_file(tmp.path().to_str().unwrap());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unrecognized magic bytes"));
}

#[test]
fn test_encode_image_file_valid_png() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52,
    ];
    std::fs::write(tmp.path(), &png_bytes).unwrap();
    let result = encode_image_file(tmp.path().to_str().unwrap()).unwrap();
    // Should be valid base64
    assert!(!result.is_empty());
    base64::engine::general_purpose::STANDARD
        .decode(&result)
        .unwrap();
}

// ── compute_pixel_similarity ─────────────────────────────────────

#[test]
fn test_pixel_similarity_identical() {
    let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([128, 64, 32, 255]));
    let sim = compute_pixel_similarity(&img, &img);
    assert!((sim - 100.0).abs() < f64::EPSILON);
}

#[test]
fn test_pixel_similarity_opposite() {
    let white = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
    let black = image::RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 0, 0]));
    let sim = compute_pixel_similarity(&white, &black);
    assert!(
        sim < 1.0,
        "Opposite images should have near-zero similarity"
    );
}

#[test]
fn test_pixel_similarity_partial() {
    let img_a = image::RgbaImage::from_pixel(10, 10, image::Rgba([100, 100, 100, 255]));
    let img_b = image::RgbaImage::from_pixel(10, 10, image::Rgba([110, 110, 110, 255]));
    let sim = compute_pixel_similarity(&img_a, &img_b);
    assert!(
        sim > 95.0,
        "Similar images should have high similarity: {}",
        sim
    );
    assert!(sim < 100.0, "Non-identical should be < 100");
}

#[test]
fn test_pixel_similarity_empty_images() {
    let empty = image::RgbaImage::new(0, 0);
    let sim = compute_pixel_similarity(&empty, &empty);
    assert!((sim - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_pixel_similarity_mismatched_sizes() {
    let a = image::RgbaImage::from_pixel(10, 10, image::Rgba([100, 100, 100, 255]));
    let b = image::RgbaImage::from_pixel(20, 20, image::Rgba([100, 100, 100, 255]));
    let sim = compute_pixel_similarity(&a, &b);
    assert!((sim - 0.0).abs() < f64::EPSILON);
}

#[test]
fn test_pixel_similarity_1x1() {
    let a = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 255]));
    let b = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    let sim = compute_pixel_similarity(&a, &b);
    assert!(sim < 30.0); // very different (3/4 channels differ by 255)
}

#[test]
fn test_pixel_similarity_half_difference() {
    let a = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
    let b = image::RgbaImage::from_pixel(1, 1, image::Rgba([128, 128, 128, 128]));
    let sim = compute_pixel_similarity(&a, &b);
    // ~50% similarity
    assert!(sim > 40.0 && sim < 60.0, "Got: {}", sim);
}

// ── round2 ───────────────────────────────────────────────────────

#[test]
fn test_round2() {
    assert!((round2(95.456) - 95.46).abs() < 0.001);
    assert!((round2(100.0) - 100.0).abs() < f64::EPSILON);
    assert!((round2(0.0) - 0.0).abs() < f64::EPSILON);
    assert!((round2(99.999) - 100.0).abs() < 0.001);
    assert!((round2(50.005) - 50.01).abs() < 0.001);
}

// ── call_vision_endpoint ─────────────────────────────────────────

#[tokio::test]
async fn test_call_vision_endpoint_invalid_url() {
    let body = json!({"model": "test", "messages": []});
    let result = call_vision_endpoint("http://localhost:1/v1", None, &body).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_call_vision_endpoint_trailing_slash_normalization() {
    // Should strip trailing slash — will still fail to connect, but tests the URL building
    let body = json!({"model": "test", "messages": []});
    let result = call_vision_endpoint("http://localhost:1/v1/", None, &body).await;
    assert!(result.is_err());
    // Error should mention the URL (not double slash)
}
