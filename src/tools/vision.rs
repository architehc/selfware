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
use std::sync::Arc;
use std::time::Duration;

use crate::api::merge_extra_body as merge_request_extra_body;
use crate::safety::checker::validation::contains_outbound_credential_shape;
use crate::safety::PinnedDnsResolver;

use super::{net_policy, Tool};

const VISION_SYSTEM_PROMPT: &str =
    "You are a precise multimodal assistant. Follow the user request exactly and answer directly.";

/// Request fields that are posted to the (caller-chosen) vision endpoint:
/// `prompt`, `model` and `detail` verbatim in the chat body, `extra_body`
/// merged into it, `api_key` as the bearer token. `image_base64` is validated
/// as an image payload instead (`resolve_image_data_uri`) — the shape oracle is
/// case-insensitive and `AKIA`/`ASIA`/`AIza` occur in ordinary base64.
const VISION_OUTBOUND_FIELDS: &[&str] = &["prompt", "model", "detail", "api_key", "extra_body"];

fn vision_argument_contains_credential(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_outbound_credential_shape(text),
        Value::Array(values) => values.iter().any(vision_argument_contains_credential),
        Value::Object(values) => values.values().any(vision_argument_contains_credential),
        _ => false,
    }
}

/// Enforce the outbound-content policy where the request is built, so a
/// direct Tool invocation gets the same protection as an agent-dispatched
/// call (the safety checker applies the identical oracle to tool calls).
pub(crate) fn reject_vision_bound_credential_shapes(args: &Value) -> Result<()> {
    for field in VISION_OUTBOUND_FIELDS {
        if args
            .get(*field)
            .is_some_and(vision_argument_contains_credential)
        {
            anyhow::bail!("Refusing vision request: credential-shaped value in {field}");
        }
    }
    Ok(())
}

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
        // Relative paths resolve against the agent's workspace root.
        let args = crate::tools::workspace_root::anchor_json(args, &["image_path"]);
        reject_vision_bound_credential_shapes(&args)?;
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

        let safety = crate::tools::file::resolve_safety_config(None);
        let data_uri = if let Some(path) = args.get("image_path").and_then(|v| v.as_str()) {
            let bytes = read_image_bytes_checked(path, &safety)?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let mime = guess_mime(path);
            format!("data:{};base64,{}", mime, b64)
        } else {
            resolve_image_data_uri(&args)?
        };

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
        merge_extra_body(&mut body, args.get("extra_body"), Some(endpoint))?;

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
        // Relative paths resolve against the agent's workspace root.
        let args = crate::tools::workspace_root::anchor_json(args, &["image_a", "image_b"]);
        reject_vision_bound_credential_shapes(&args)?;
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

        // Validate and read both images via O_NONBLOCK | O_NOFOLLOW descriptor check
        let safety = crate::tools::file::resolve_safety_config(None);
        let bytes_a = read_image_bytes_checked(path_a, &safety)
            .with_context(|| format!("Failed to open image_a: {}", path_a))?;
        let bytes_b = read_image_bytes_checked(path_b, &safety)
            .with_context(|| format!("Failed to open image_b: {}", path_b))?;

        let decode_img = |bytes: &[u8], label: &str, path: &str| -> Result<image::RgbaImage> {
            let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .with_context(|| format!("Failed to read {label}: {path}"))?;
            reader.limits(image::Limits::default());
            Ok(reader
                .decode()
                .with_context(|| format!("Failed to decode {label}: {path}"))?
                .to_rgba8())
        };
        let img_a = decode_img(&bytes_a, "image_a", path_a)?;
        let img_b = decode_img(&bytes_b, "image_b", path_b)?;

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
            let b64_a = base64::engine::general_purpose::STANDARD.encode(&bytes_a);
            let b64_b = base64::engine::general_purpose::STANDARD.encode(&bytes_b);
            let uri_a = format!("data:{};base64,{}", guess_mime(path_a), b64_a);
            let uri_b = format!("data:{};base64,{}", guess_mime(path_b), b64_b);

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
            merge_extra_body(&mut body, args.get("extra_body"), Some(endpoint))?;

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

/// Maximum image file size (20 MB).
const MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024;

/// Maximum VLM response body size (10 MB) — matches `http_request` / `web_fetch`
/// so a model-selected `endpoint` cannot stream an unbounded response into memory.
const MAX_VISION_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

async fn read_bounded_bytes(mut response: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if let Some(len) = response.content_length() {
        if len > max_bytes as u64 {
            anyhow::bail!(
                "Vision API response too large ({} bytes, {} byte limit)",
                len,
                max_bytes
            );
        }
    }
    let mut buf = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Failed to read vision API response chunk")?
    {
        if buf.len() + chunk.len() > max_bytes {
            anyhow::bail!("Vision API response exceeded {} byte limit", max_bytes);
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

fn merge_extra_body(
    body: &mut Value,
    extra_body: Option<&Value>,
    endpoint: Option<&str>,
) -> Result<()> {
    let Some(extra_body) = extra_body else {
        return Ok(());
    };
    let Some(extra_obj) = extra_body.as_object() else {
        anyhow::bail!("extra_body must be an object");
    };
    merge_request_extra_body(body, Some(extra_obj), "vision tool request", endpoint)
}

/// Resolve an image to a data URI from either `image_path` or `image_base64`.
fn resolve_image_data_uri(args: &Value) -> Result<String> {
    if let Some(path) = args.get("image_path").and_then(|v| v.as_str()) {
        let b64 = encode_image_file(path)?;
        let mime = guess_mime(path);
        Ok(format!("data:{};base64,{}", mime, b64))
    } else if let Some(b64) = args.get("image_base64").and_then(|v| v.as_str()) {
        let (uri, payload) = if let Some(rest) = b64.strip_prefix("data:") {
            let Some((meta, payload)) = rest.split_once(";base64,") else {
                anyhow::bail!("image_base64 data URI must be data:image/<format>;base64,<payload>");
            };
            if !meta.starts_with("image/") {
                anyhow::bail!("image_base64 data URI must have an image/* MIME type, got {meta}");
            }
            (b64.to_string(), payload)
        } else {
            (format!("data:image/png;base64,{}", b64), b64)
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload.trim())
            .context("image_base64 is not valid base64")?;
        if bytes.len() as u64 > MAX_IMAGE_SIZE {
            anyhow::bail!(
                "image_base64 payload too large: {} bytes (max {} MB)",
                bytes.len(),
                MAX_IMAGE_SIZE / (1024 * 1024)
            );
        }
        validate_image_magic(&bytes, "image_base64")?;
        Ok(uri)
    } else {
        anyhow::bail!("Either image_path or image_base64 must be provided")
    }
}

/// Validate a file path with tool path policy, open via `O_NONBLOCK | O_NOFOLLOW`
/// descriptor check, enforce the 20MB size cap, and verify image magic bytes.
fn read_image_bytes_checked(path: &str, safety: &crate::config::SafetyConfig) -> Result<Vec<u8>> {
    crate::tools::file::validate_tool_path(path, safety)?;
    let file = crate::tools::file::open_checked_regular(path, safety)?;
    read_checked_image_handle(file, path)
}

fn read_checked_image_handle(mut file: std::fs::File, path: &str) -> Result<Vec<u8>> {
    use std::io::Read;
    let metadata = file
        .metadata()
        .with_context(|| format!("Image file not found: {}", path))?;
    if !metadata.is_file() {
        anyhow::bail!("Image path is not a regular file: {}", path);
    }
    if metadata.len() > MAX_IMAGE_SIZE {
        anyhow::bail!(
            "Image file too large: {} bytes (max {} MB)",
            metadata.len(),
            MAX_IMAGE_SIZE / (1024 * 1024)
        );
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    (&mut file)
        .take(MAX_IMAGE_SIZE + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read image file: {}", path))?;
    if bytes.len() as u64 > MAX_IMAGE_SIZE {
        anyhow::bail!(
            "Image file too large: {} bytes (max {} MB)",
            bytes.len(),
            MAX_IMAGE_SIZE / (1024 * 1024)
        );
    }
    validate_image_magic(&bytes, path)?;
    Ok(bytes)
}

/// Read an image file, validate it, and return base64-encoded data.
pub fn encode_image_file(path: &str) -> Result<String> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .with_context(|| format!("Image file not found: {}", path))?
    };
    #[cfg(not(unix))]
    let file =
        std::fs::File::open(path).with_context(|| format!("Image file not found: {}", path))?;

    let bytes = read_checked_image_handle(file, path)?;
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
    let parsed = reqwest::Url::parse(&url).context("Invalid vision endpoint URL")?;
    let allow_private = std::env::var("SELFWARE_ALLOW_PRIVATE_NETWORK").unwrap_or_default() == "1";
    let policy = net_policy::validate_url_target(&parsed, allow_private)
        .context("Vision endpoint rejected by network policy")?;
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .dns_resolver(Arc::new(PinnedDnsResolver::with_localhost(
            policy.allow_private,
            policy.allow_localhost,
        )))
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() > 10 {
                return attempt.error("Too many redirects");
            }
            match net_policy::validate_redirect_target(attempt.url(), &policy) {
                Ok(_) => attempt.follow(),
                Err(error) => attempt.error(error),
            }
        }))
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

    let status = response.status();
    let bytes = read_bounded_bytes(response, MAX_VISION_RESPONSE_BYTES).await?;

    if !status.is_success() {
        let text = String::from_utf8_lossy(&bytes);
        anyhow::bail!(
            "Vision API returned HTTP {}: {}",
            status.as_u16(),
            text.chars().take(500).collect::<String>()
        );
    }

    serde_json::from_slice(&bytes).context("Failed to parse vision API response as JSON")
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
