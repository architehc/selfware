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

use crate::api::merge_extra_body as merge_request_extra_body;

use super::Tool;

const VISION_SYSTEM_PROMPT: &str =
    "You are a precise multimodal assistant. Follow the user request exactly and answer directly.";

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
                    "description": "Vision model API endpoint. OPTIONAL — the harness fills it from the configured vision profile; only supply to override. Localhost is allowed by default; set SELFWARE_ALLOW_PRIVATE_NETWORK=1 for private LAN hosts."
                },
                "model": {
                    "type": "string",
                    "description": "Vision model name. OPTIONAL — filled from the configured vision profile; only supply to override."
                },
                "api_key": {
                    "type": "string",
                    "description": "Optional bearer token for authenticated vision endpoints."
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
            // Only the prompt is truly required: endpoint/model are injected
            // from the configured vision profile at dispatch time (a TB4
            // cad-model trial died pixel-peeping a schematic because the
            // model was asked to GUESS infrastructure it cannot know).
            "required": ["prompt"]
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
        let api_key = args.get("api_key").and_then(|v| v.as_str());
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
            "messages": [
                {
                    "role": "system",
                    "content": VISION_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": prompt },
                        { "type": "image_url", "image_url": { "url": data_uri, "detail": detail } }
                    ]
                }
            ],
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": false
        });
        merge_extra_body(&mut body, args.get("extra_body"))?;

        let response = call_vision_endpoint(endpoint, api_key, &body).await?;

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
                "api_key": {
                    "type": "string",
                    "description": "Optional bearer token for authenticated semantic comparison endpoints."
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
        let api_key = args.get("api_key").and_then(|v| v.as_str());
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
                "messages": [
                    {
                        "role": "system",
                        "content": VISION_SYSTEM_PROMPT
                    },
                    {
                        "role": "user",
                        "content": [
                            { "type": "text", "text": "Compare these two images. Describe the visual differences between image 1 and image 2. Be specific about layout, color, typography, and content differences." },
                            { "type": "image_url", "image_url": { "url": uri_a, "detail": detail } },
                            { "type": "image_url", "image_url": { "url": uri_b, "detail": detail } }
                        ]
                    }
                ],
                "max_tokens": max_tokens,
                "temperature": temperature,
                "stream": false
            });
            merge_extra_body(&mut body, args.get("extra_body"))?;

            match call_vision_endpoint(endpoint, api_key, &body).await {
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
    merge_request_extra_body(body, Some(extra_obj), "vision tool request")
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
pub(crate) async fn call_vision_endpoint(
    endpoint: &str,
    api_key: Option<&str>,
    body: &Value,
) -> Result<Value> {
    let url = format!("{}/chat/completions", endpoint.trim_end_matches('/'));
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .context("Failed to build HTTP client")?;

    // Route through the shared authenticated-request factory: it refuses to
    // send the key over plaintext HTTP to a remote host (or via a userinfo URL)
    // and attaches the bearer token.
    let request = crate::config::api_key::authorize_request(
        client.post(&url).header("Content-Type", "application/json"),
        endpoint,
        api_key,
    )?;

    let response = request
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
#[path = "../../tests/unit/tools/vision/vision_test.rs"]
mod tests;
