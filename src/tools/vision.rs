//! Vision tools for analyzing and comparing images via vision-capable LLMs.
//!
//! These tools send images to a VLM (vision-language model) endpoint for
//! analysis, comparison, and structured evaluation.  They work with any
//! OpenAI-compatible vision API (LM Studio, vLLM, ollama, etc.).

use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::Tool;

// ───────────────────────────────────────────────────────────────────────────
// vision_analyze
// ───────────────────────────────────────────────────────────────────────────

/// Analyze an image using a vision-capable LLM.
///
/// Accepts an image from a file path or inline base64, sends it along with
/// a prompt to the configured VLM endpoint, and returns the model's analysis.
pub struct VisionAnalyze;

#[async_trait]
impl Tool for VisionAnalyze {
    fn name(&self) -> &str {
        "vision_analyze"
    }

    fn description(&self) -> &str {
        "Analyze an image using a vision-capable LLM. Send an image (from file \
         or base64) with a prompt and receive the model's visual analysis. \
         Requires a vision model endpoint."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image_path": {
                    "type": "string",
                    "description": "Path to an image file (PNG, JPG, WEBP, GIF). Mutually exclusive with image_base64."
                },
                "image_base64": {
                    "type": "string",
                    "description": "Base64-encoded image data. Mutually exclusive with image_path."
                },
                "prompt": {
                    "type": "string",
                    "description": "What to analyze in the image. Be specific about what you want to know."
                },
                "endpoint": {
                    "type": "string",
                    "description": "Vision model API endpoint (e.g. 'http://localhost:1234/v1'). Required. Localhost is allowed by default; set SELFWARE_ALLOW_PRIVATE_NETWORK=1 for private LAN hosts."
                },
                "model": {
                    "type": "string",
                    "description": "Vision model name. Required."
                },
                "detail": {
                    "type": "string",
                    "enum": ["low", "high", "auto"],
                    "description": "Image detail level for token usage. Default: auto"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Max response tokens. Default: 4096"
                },
                "temperature": {
                    "type": "number",
                    "description": "Sampling temperature. Default: 0.2"
                },
                "extra_body": {
                    "type": "object",
                    "description": "Optional extra request fields merged into the chat-completion body, e.g. chat_template_kwargs."
                }
            },
            "required": ["prompt", "endpoint", "model"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .context("prompt is required")?;
        let endpoint = args
            .get("endpoint")
            .and_then(|v| v.as_str())
            .context("endpoint is required")?;
        let model = args
            .get("model")
            .and_then(|v| v.as_str())
            .context("model is required")?;
        let detail = args
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(4096) as usize;
        let temperature = args
            .get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.2);

        let data_uri = resolve_image_data_uri(&args)?;

        // Build the multimodal message array (OpenAI vision format)
        let mut body = json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": data_uri, "detail": detail } }
                ]
            }],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": false
        });
        merge_extra_body(&mut body, args.get("extra_body"))?;

        let response = call_vision_endpoint(endpoint, &body).await?;

        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let usage = &response["usage"];

        Ok(json!({
            "success": true,
            "analysis": content,
            "model": model,
            "usage": {
                "prompt_tokens": usage["prompt_tokens"],
                "completion_tokens": usage["completion_tokens"],
                "total_tokens": usage["total_tokens"]
            }
        }))
    }
}

// ───────────────────────────────────────────────────────────────────────────
// vision_compare
// ───────────────────────────────────────────────────────────────────────────

/// Compare two images and report differences.
///
/// Performs both a pixel-level structural similarity comparison and,
/// optionally, a VLM-based semantic comparison.
pub struct VisionCompare;

#[async_trait]
impl Tool for VisionCompare {
    fn name(&self) -> &str {
        "vision_compare"
    }

    fn description(&self) -> &str {
        "Compare two images pixel-by-pixel and return a similarity score (0-100). \
         Optionally send both images to a vision LLM for semantic comparison. \
         Useful for visual regression testing and design verification."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image_a": {
                    "type": "string",
                    "description": "Path to the first image (reference/expected)"
                },
                "image_b": {
                    "type": "string",
                    "description": "Path to the second image (actual/candidate)"
                },
                "threshold": {
                    "type": "number",
                    "description": "Similarity threshold (0-100). Below this is a 'fail'. Default: 90"
                },
                "endpoint": {
                    "type": "string",
                    "description": "Optional vision model endpoint for semantic comparison. Localhost is allowed by default; set SELFWARE_ALLOW_PRIVATE_NETWORK=1 for private LAN hosts."
                },
                "model": {
                    "type": "string",
                    "description": "Optional vision model name for semantic comparison"
                },
                "detail": {
                    "type": "string",
                    "enum": ["low", "high", "auto"],
                    "description": "Optional image detail level for semantic comparison. Default: auto"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Optional max response tokens for semantic comparison. Default: 2048"
                },
                "temperature": {
                    "type": "number",
                    "description": "Optional sampling temperature for semantic comparison. Default: 0.2"
                },
                "extra_body": {
                    "type": "object",
                    "description": "Optional extra request fields merged into the semantic compare request body."
                }
            },
            "required": ["image_a", "image_b"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let path_a = args
            .get("image_a")
            .and_then(|v| v.as_str())
            .context("image_a is required")?;
        let path_b = args
            .get("image_b")
            .and_then(|v| v.as_str())
            .context("image_b is required")?;
        let threshold = args
            .get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(90.0);
        let detail = args
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");
        let max_tokens = args
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(2048) as usize;
        let temperature = args
            .get("temperature")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.2);

        // Load both images
        let img_a = image::open(path_a)
            .with_context(|| format!("Failed to open image_a: {}", path_a))?
            .to_rgba8();
        let img_b = image::open(path_b)
            .with_context(|| format!("Failed to open image_b: {}", path_b))?
            .to_rgba8();

        let (w_a, h_a) = img_a.dimensions();
        let (w_b, h_b) = img_b.dimensions();

        // Resize image_b to match image_a if dimensions differ
        let img_b = if (w_a, h_a) != (w_b, h_b) {
            image::imageops::resize(&img_b, w_a, h_a, image::imageops::FilterType::Lanczos3)
        } else {
            img_b
        };

        // Compute pixel-level similarity (mean absolute error → similarity %)
        let pixel_similarity = compute_pixel_similarity(&img_a, &img_b);
        let passed = pixel_similarity >= threshold;

        let mut result = json!({
            "success": true,
            "pixel_similarity": round2(pixel_similarity),
            "threshold": threshold,
            "passed": passed,
            "dimensions_a": { "width": w_a, "height": h_a },
            "dimensions_b": { "width": w_b, "height": h_b },
            "dimensions_matched": (w_a, h_a) == (w_b, h_b),
        });

        // If VLM endpoint provided, also do semantic comparison
        let endpoint = args.get("endpoint").and_then(|v| v.as_str());
        let model = args.get("model").and_then(|v| v.as_str());
        if let (Some(endpoint), Some(model)) = (endpoint, model) {
            let b64_a = encode_image_file(path_a)?;
            let b64_b = encode_image_file(path_b)?;
            let uri_a = format!("data:image/png;base64,{}", b64_a);
            let uri_b = format!("data:image/png;base64,{}", b64_b);

            let mut body = json!({
                "model": model,
                "messages": [{
                    "role": "user",
                    "content": [
                        { "type": "text", "text": "Compare these two images. Describe the visual differences between image 1 and image 2. Be specific about layout, color, typography, and content differences." },
                        { "type": "image_url", "image_url": { "url": uri_a, "detail": detail } },
                        { "type": "image_url", "image_url": { "url": uri_b, "detail": detail } }
                    ]
                }],
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stream": false
            });
            merge_extra_body(&mut body, args.get("extra_body"))?;

            match call_vision_endpoint(endpoint, &body).await {
                Ok(response) => {
                    let analysis = response["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    result["semantic_comparison"] = json!(analysis);
                }
                Err(e) => {
                    result["semantic_comparison_error"] = json!(e.to_string());
                }
            }
        }

        Ok(result)
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Shared helpers
// ───────────────────────────────────────────────────────────────────────────

/// Maximum image file size (50 MB).
const MAX_IMAGE_SIZE: u64 = 50 * 1024 * 1024;

fn merge_extra_body(body: &mut Value, extra_body: Option<&Value>) -> Result<()> {
    let Some(extra_body) = extra_body else {
        return Ok(());
    };
    let Some(extra_obj) = extra_body.as_object() else {
        anyhow::bail!("extra_body must be an object");
    };
    let Some(body_obj) = body.as_object_mut() else {
        return Ok(());
    };
    for (key, value) in extra_obj {
        body_obj.insert(key.clone(), value.clone());
    }
    Ok(())
}

/// Resolve an image to a data URI from either `image_path` or `image_base64`.
fn resolve_image_data_uri(args: &Value) -> Result<String> {
    if let Some(path) = args.get("image_path").and_then(|v| v.as_str()) {
        let b64 = encode_image_file(path)?;
        let mime = guess_mime(path);
        Ok(format!("data:{};base64,{}", mime, b64))
    } else if let Some(b64) = args.get("image_base64").and_then(|v| v.as_str()) {
        // Assume PNG if no prefix given
        if b64.starts_with("data:") {
            Ok(b64.to_string())
        } else {
            Ok(format!("data:image/png;base64,{}", b64))
        }
    } else {
        anyhow::bail!("Either image_path or image_base64 must be provided")
    }
}

/// Read an image file, validate it, and return base64-encoded data.
pub fn encode_image_file(path: &str) -> Result<String> {
    let metadata =
        std::fs::metadata(path).with_context(|| format!("Image file not found: {}", path))?;

    if metadata.len() > MAX_IMAGE_SIZE {
        anyhow::bail!(
            "Image file too large: {} bytes (max {} MB)",
            metadata.len(),
            MAX_IMAGE_SIZE / (1024 * 1024)
        );
    }

    let bytes =
        std::fs::read(path).with_context(|| format!("Failed to read image file: {}", path))?;

    // Validate it's actually an image by checking magic bytes
    validate_image_magic(&bytes, path)?;

    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

/// Check the first few bytes to verify this is a real image file.
pub(crate) fn validate_image_magic(bytes: &[u8], path: &str) -> Result<()> {
    if bytes.len() < 4 {
        anyhow::bail!("File too small to be a valid image: {}", path);
    }
    let is_valid = bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])  // PNG
        || bytes.starts_with(&[0xFF, 0xD8, 0xFF])                 // JPEG
        || bytes.starts_with(b"GIF8")                              // GIF
        || bytes.starts_with(b"RIFF") && bytes.len() > 11 && &bytes[8..12] == b"WEBP"  // WEBP
        || bytes.starts_with(b"BM"); // BMP
    if !is_valid {
        anyhow::bail!(
            "File does not appear to be a valid image (unrecognized magic bytes): {}",
            path
        );
    }
    Ok(())
}

/// Guess MIME type from file extension.
pub(crate) fn guess_mime(path: &str) -> &'static str {
    match path.rsplit('.').next().map(|e| e.to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    }
}

/// Send a request to an OpenAI-compatible vision endpoint.
pub(crate) async fn call_vision_endpoint(endpoint: &str, body: &Value) -> Result<Value> {
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .with_context(|| format!("Failed to connect to vision endpoint: {}", url))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Vision API returned HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        );
    }

    response
        .json::<Value>()
        .await
        .context("Failed to parse vision API response as JSON")
}

/// Compute pixel-level similarity between two same-sized RGBA images.
/// Returns a percentage (0.0–100.0) where 100 = identical.
pub(crate) fn compute_pixel_similarity(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
    let pixels_a = a.as_raw();
    let pixels_b = b.as_raw();
    if pixels_a.len() != pixels_b.len() || pixels_a.is_empty() {
        return 0.0;
    }

    let total_error: u64 = pixels_a
        .iter()
        .zip(pixels_b.iter())
        .map(|(&pa, &pb)| (pa as i32 - pb as i32).unsigned_abs() as u64)
        .sum();

    let max_error = pixels_a.len() as u64 * 255;
    let mae_ratio = total_error as f64 / max_error as f64;
    (1.0 - mae_ratio) * 100.0
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

use xcap::image;

#[cfg(test)]
mod tests {
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
        assert!(schema["properties"]["extra_body"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("prompt")));
        assert!(required.contains(&json!("endpoint")));
        assert!(required.contains(&json!("model")));
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
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52,
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
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52,
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
        let result = call_vision_endpoint("http://localhost:1/v1", &body).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_call_vision_endpoint_trailing_slash_normalization() {
        // Should strip trailing slash — will still fail to connect, but tests the URL building
        let body = json!({"model": "test", "messages": []});
        let result = call_vision_endpoint("http://localhost:1/v1/", &body).await;
        assert!(result.is_err());
        // Error should mention the URL (not double slash)
    }
}
