#![allow(dead_code, unused_imports, unused_variables)]
//! Visual verification using Vision Language Models (VLMs).
//!
//! Provides automated visual testing by sending screenshots to a VLM endpoint
//! and parsing structured verification results. Integrates with the existing
//! [`VerificationGate`](super::verification::VerificationGate) pipeline.
//!
//! # Example
//!
//! ```rust,ignore
//! use selfware::testing::visual_verification::{VisualVerifier, UiElement};
//!
//! # async fn example() -> anyhow::Result<()> {
//! let verifier = VisualVerifier::new(
//!     "http://localhost:1234/v1",
//!     "qwen2-vl-7b",
//! );
//! let result = verifier.verify_screenshot(
//!     "<base64 png data>",
//!     "A login form with email and password fields and a blue submit button",
//! ).await?;
//! assert!(result.passed);
//! # Ok(())
//! # }
//! ```

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Result of verifying a screenshot against an expected description.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualVerificationResult {
    /// Whether the screenshot matches the expected description.
    pub passed: bool,
    /// Confidence score from the VLM (0.0 to 1.0).
    pub confidence: f64,
    /// What the VLM actually sees in the screenshot.
    pub description: String,
    /// Any problems or mismatches found.
    pub issues: Vec<String>,
}

/// Result of comparing two screenshots for expected changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualDiffResult {
    /// Whether any visual changes were detected between the two screenshots.
    pub changes_detected: bool,
    /// Whether the specific expected change was found.
    pub expected_change_found: bool,
    /// Description of what changed between the screenshots.
    pub description: String,
    /// Any changes that were not part of the expected description.
    pub unexpected_changes: Vec<String>,
}

/// A UI element to verify in a screenshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    /// Human-readable name for the element (e.g. "Login Button").
    pub name: String,
    /// Element type: "button", "text", "input", "image", "icon", etc.
    pub element_type: String,
    /// Expected text content, if applicable.
    pub expected_text: Option<String>,
    /// Expected location: "top-left", "top-center", "top-right", "center-left",
    /// "center", "center-right", "bottom-left", "bottom-center", "bottom-right".
    pub expected_location: Option<String>,
}

/// Verification result for a single UI element.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementVerification {
    /// The element that was checked.
    pub element: UiElement,
    /// Whether the element was found in the screenshot.
    pub found: bool,
    /// Where the element was actually located, if found.
    pub location: Option<String>,
    /// Actual text content observed, if applicable.
    pub actual_text: Option<String>,
}

/// Analysis of page layout quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutAnalysis {
    /// Overall layout quality: "good", "fair", or "poor".
    pub overall_quality: String,
    /// Detected alignment issues (e.g. "Header text is not centered").
    pub alignment_issues: Vec<String>,
    /// Detected spacing issues (e.g. "Buttons are too close together").
    pub spacing_issues: Vec<String>,
    /// Notes about responsive design or layout adaptability.
    pub responsive_notes: Vec<String>,
}

/// Configuration for visual verification, loadable from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualVerificationConfig {
    /// Enable visual verification in the QA pipeline.
    #[serde(default)]
    pub enabled: bool,
    /// VLM API endpoint (e.g. "http://localhost:1234/v1").
    #[serde(default = "default_visual_endpoint")]
    pub endpoint: String,
    /// Vision model name.
    #[serde(default = "default_visual_model")]
    pub model: String,
    /// Request timeout in seconds.
    #[serde(default = "default_visual_timeout")]
    pub timeout_secs: u64,
    /// Minimum confidence threshold for passing (0.0 to 1.0).
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
}

fn default_visual_endpoint() -> String {
    "http://localhost:1234/v1".to_string()
}

fn default_visual_model() -> String {
    "qwen2-vl-7b".to_string()
}

fn default_visual_timeout() -> u64 {
    120
}

fn default_confidence_threshold() -> f64 {
    0.7
}

impl Default for VisualVerificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_visual_endpoint(),
            model: default_visual_model(),
            timeout_secs: default_visual_timeout(),
            confidence_threshold: default_confidence_threshold(),
        }
    }
}

// ---------------------------------------------------------------------------
// VisualVerifier
// ---------------------------------------------------------------------------

/// Sends screenshots to a VLM for automated visual verification.
///
/// Works with any OpenAI-compatible vision endpoint (LM Studio, vLLM, ollama,
/// etc.) that accepts base64 image content in chat completion messages.
pub struct VisualVerifier {
    /// VLM API endpoint (e.g. "http://localhost:1234/v1").
    endpoint: String,
    /// Vision model identifier.
    model: String,
    /// Optional bearer token for authenticated vision endpoints.
    api_key: Option<crate::config::RedactedString>,
    /// HTTP request timeout in seconds.
    timeout_secs: u64,
    /// Default max response tokens for ad hoc requests.
    default_max_tokens: usize,
    /// Deterministic JSON prompts use a low temperature by default.
    temperature: f64,
    /// Detail level passed to OpenAI-compatible vision endpoints.
    image_detail: String,
    /// Backend-specific request overrides, e.g. SGLang `chat_template_kwargs`.
    extra_body: Option<serde_json::Map<String, serde_json::Value>>,
}

const VERIFICATION_MAX_TOKENS: usize = 192;
const DIFF_MAX_TOKENS: usize = 160;
const JSON_TEMPERATURE: f64 = 0.0;
const DEFAULT_IMAGE_DETAIL: &str = "low";

impl VisualVerifier {
    /// Create a new verifier with the given endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            api_key: None,
            timeout_secs: default_visual_timeout(),
            default_max_tokens: 4096,
            temperature: JSON_TEMPERATURE,
            image_detail: DEFAULT_IMAGE_DETAIL.to_string(),
            extra_body: None,
        }
    }

    /// Create a verifier from a [`VisualVerificationConfig`].
    pub fn from_config(config: &VisualVerificationConfig) -> Self {
        Self::new(&config.endpoint, &config.model).with_timeout(config.timeout_secs)
    }

    /// Create a verifier from a named model profile.
    pub fn from_model_profile(profile: &crate::config::ModelProfile) -> Self {
        Self::new(&profile.endpoint, &profile.model)
            .with_api_key(profile.api_key.clone())
            .with_generation(profile.max_tokens, profile.temperature as f64)
            .with_extra_body(profile.extra_body.clone())
    }

    /// Create a verifier from the main application config.
    ///
    /// Prefers `models.vision` when present, otherwise falls back to the
    /// effective default model profile synthesized from the top-level config.
    pub fn from_app_config(config: &crate::config::Config) -> Self {
        let mut verifier = config
            .models
            .get("vision")
            .map(Self::from_model_profile)
            .or_else(|| config.resolve_model(None).map(Self::from_model_profile))
            .unwrap_or_else(|| {
                Self::new(&config.endpoint, &config.model)
                    .with_api_key(config.api_key.clone())
                    .with_generation(config.max_tokens, config.temperature as f64)
                    .with_extra_body(config.extra_body.clone())
            });

        verifier.timeout_secs = config.agent.step_timeout_secs.max(1);
        verifier
    }

    /// Override the request timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Override default generation settings for shared multimodal requests.
    pub fn with_generation(mut self, max_tokens: usize, temperature: f64) -> Self {
        self.default_max_tokens = max_tokens.max(1);
        self.temperature = temperature;
        self
    }

    /// Attach a bearer token for authenticated endpoints.
    pub fn with_api_key(mut self, api_key: Option<crate::config::RedactedString>) -> Self {
        self.api_key = api_key;
        self
    }

    /// Override the image detail level sent to the backend.
    pub fn with_image_detail(mut self, detail: impl Into<String>) -> Self {
        self.image_detail = detail.into();
        self
    }

    /// Attach backend-specific request overrides.
    pub fn with_extra_body(
        mut self,
        extra_body: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Self {
        self.extra_body = extra_body;
        self
    }

    // -----------------------------------------------------------------------
    // Core verification methods
    // -----------------------------------------------------------------------

    /// Verify that a screenshot matches an expected description.
    ///
    /// Sends the image and description to the VLM and parses a structured
    /// pass/fail response with confidence and issue details.
    pub async fn verify_screenshot(
        &self,
        image_base64: &str,
        expected: &str,
    ) -> Result<VisualVerificationResult> {
        let prompt = build_verify_prompt(expected);
        let body = self.build_single_image_request_with_options(
            &prompt,
            image_base64,
            VERIFICATION_MAX_TOKENS,
        )?;
        let raw = self.call_vlm(&body).await?;
        parse_verification_response(&raw)
    }

    /// Compare two screenshots and verify that a described change occurred.
    ///
    /// Both images are sent to the VLM in a single request so the model can
    /// reason about the differences.
    pub async fn compare_screenshots(
        &self,
        before: &str,
        after: &str,
        change_description: &str,
    ) -> Result<VisualDiffResult> {
        let prompt = build_compare_prompt(change_description);
        let body =
            self.build_two_image_request_with_options(&prompt, before, after, DIFF_MAX_TOKENS)?;
        let raw = self.call_vlm(&body).await?;
        parse_diff_response(&raw)
    }

    /// Verify that specific UI elements are present and visible.
    pub async fn verify_ui_elements(
        &self,
        image_base64: &str,
        elements: &[UiElement],
    ) -> Result<Vec<ElementVerification>> {
        let prompt = build_elements_prompt(elements);
        let body = self.build_single_image_request(&prompt, image_base64)?;
        let raw = self.call_vlm(&body).await?;
        parse_elements_response(&raw, elements)
    }

    // -----------------------------------------------------------------------
    // Convenience methods
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Integration with VerificationGate
    // -----------------------------------------------------------------------

    /// Run a visual check that can be plugged into the verification pipeline.
    ///
    /// Returns a [`super::verification::CheckResult`] compatible with the
    /// existing verification gate.
    pub async fn visual_check(
        &self,
        image_base64: &str,
        expected: &str,
    ) -> Result<super::verification::CheckResult> {
        let start = std::time::Instant::now();
        let result = self.verify_screenshot(image_base64, expected).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(vr) => {
                let errors = vr
                    .issues
                    .iter()
                    .map(|issue| super::verification::VerificationError {
                        file: String::new(),
                        line: None,
                        column: None,
                        message: issue.clone(),
                        code: None,
                        severity: super::verification::ErrorSeverity::Error,
                        suggestion: None,
                    })
                    .collect();

                Ok(super::verification::CheckResult {
                    check_type: super::verification::CheckType::Custom,
                    passed: vr.passed,
                    duration_ms,
                    output: vr.description,
                    errors,
                    warnings: vec![],
                    suggestions: if !vr.passed {
                        vec!["Visual verification failed -- review screenshot against expected layout".to_string()]
                    } else {
                        vec![]
                    },
                })
            }
            Err(e) => Ok(super::verification::CheckResult {
                check_type: super::verification::CheckType::Custom,
                passed: false,
                duration_ms,
                output: format!("Visual verification error: {}", e),
                errors: vec![super::verification::VerificationError {
                    file: String::new(),
                    line: None,
                    column: None,
                    message: e.to_string(),
                    code: None,
                    severity: super::verification::ErrorSeverity::Error,
                    suggestion: None,
                }],
                warnings: vec![],
                suggestions: vec![
                    "Ensure VLM endpoint is reachable and the model supports vision".to_string(),
                ],
            }),
        }
    }

    // -----------------------------------------------------------------------
    // VLM HTTP helpers
    // -----------------------------------------------------------------------

    /// Build a chat-completion request body with a single image.
    fn build_single_image_request(&self, prompt: &str, image_base64: &str) -> Result<Value> {
        self.build_single_image_request_with_options(prompt, image_base64, self.default_max_tokens)
    }

    fn build_single_image_request_with_options(
        &self,
        prompt: &str,
        image_base64: &str,
        max_tokens: usize,
    ) -> Result<Value> {
        let data_uri = format!("data:image/png;base64,{}", image_base64);
        let mut body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a precise visual verification assistant. Follow the user instruction exactly and answer directly."
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": prompt },
                        { "type": "image_url", "image_url": { "url": data_uri, "detail": self.image_detail } }
                    ]
                }
            ],
            "max_tokens": self.clamp_max_tokens(max_tokens),
            "temperature": self.temperature,
            "stream": false
        });
        self.merge_extra_body(&mut body)?;
        Ok(body)
    }

    /// Build a chat-completion request body with two images (before/after).
    fn build_two_image_request(
        &self,
        prompt: &str,
        before_base64: &str,
        after_base64: &str,
    ) -> Result<Value> {
        self.build_two_image_request_with_options(
            prompt,
            before_base64,
            after_base64,
            self.default_max_tokens,
        )
    }

    fn build_two_image_request_with_options(
        &self,
        prompt: &str,
        before_base64: &str,
        after_base64: &str,
        max_tokens: usize,
    ) -> Result<Value> {
        let uri_before = format!("data:image/png;base64,{}", before_base64);
        let uri_after = format!("data:image/png;base64,{}", after_base64);
        let mut body = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a precise visual verification assistant. Follow the user instruction exactly and answer directly."
                },
                {
                    "role": "user",
                    "content": [
                        { "type": "text", "text": prompt },
                        { "type": "image_url", "image_url": { "url": uri_before, "detail": self.image_detail } },
                        { "type": "image_url", "image_url": { "url": uri_after, "detail": self.image_detail } }
                    ]
                }
            ],
            "max_tokens": self.clamp_max_tokens(max_tokens),
            "temperature": self.temperature,
            "stream": false
        });
        self.merge_extra_body(&mut body)?;
        Ok(body)
    }

    fn clamp_max_tokens(&self, max_tokens: usize) -> usize {
        max_tokens.min(self.default_max_tokens.max(1))
    }

    fn merge_extra_body(&self, body: &mut Value) -> Result<()> {
        let Some(extra_body) = &self.extra_body else {
            return Ok(());
        };
        crate::api::merge_extra_body(body, Some(extra_body), "visual verification request")
    }

    /// Send a request to the VLM endpoint and extract the response text.
    async fn call_vlm(&self, body: &Value) -> Result<String> {
        let url = format!("{}/chat/completions", self.endpoint.trim_end_matches('/'));
        debug!("Calling VLM endpoint: {}", url);

        let client = Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        // Shared authenticated-request factory: refuses plaintext-HTTP/userinfo
        // credential exposure, then attaches the bearer token.
        let request = crate::config::api_key::authorize_request(
            client.post(&url).header("Content-Type", "application/json"),
            &self.endpoint,
            self.api_key.as_ref().map(|k| k.expose()),
        )?;

        let response = request
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to connect to VLM endpoint: {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!(
                "VLM API returned HTTP {}: {}",
                status.as_u16(),
                text.chars().take(500).collect::<String>()
            );
        }

        let json_resp: Value = response
            .json()
            .await
            .context("Failed to parse VLM response as JSON")?;

        let content = json_resp["choices"][0]["message"]["content"]
            .as_str()
            .or_else(|| json_resp["choices"][0]["message"]["reasoning_content"].as_str())
            .or_else(|| json_resp["choices"][0]["message"]["reasoning"].as_str())
            .unwrap_or("")
            .to_string();

        if content.is_empty() {
            warn!("VLM returned empty content");
        }

        Ok(content)
    }
}

// ---------------------------------------------------------------------------
// Prompt builders
// ---------------------------------------------------------------------------

/// Build the system prompt for screenshot verification.
fn build_verify_prompt(expected: &str) -> String {
    format!(
        "You are a strict visual verification assistant. Analyze the provided screenshot \
         and determine if it matches the following expected description:\n\n\
         EXPECTED: {}\n\n\
         Respond ONLY with a JSON object (no markdown fences, no extra text) with these fields:\n\
         - \"passed\": boolean, true if the screenshot matches the expected description\n\
         - \"confidence\": number between 0.0 and 1.0 indicating your confidence\n\
         - \"description\": short string, at most 16 words, describing what you actually see\n\
         - \"issues\": array of at most 3 short strings listing mismatches or problems\n\n\
         Keep the response compact. If everything matches, set \"passed\" to true and \"issues\" \
         to an empty array.",
        expected
    )
}

/// Build the prompt for comparing two screenshots.
fn build_compare_prompt(change_description: &str) -> String {
    format!(
        "You are a strict visual diff assistant. Compare the two screenshots (image 1 = BEFORE, \
         image 2 = AFTER) and determine whether the following expected change occurred:\n\n\
         EXPECTED CHANGE: {}\n\n\
         Respond ONLY with a JSON object (no markdown fences, no extra text) with these fields:\n\
         - \"changes_detected\": boolean, true if the images differ\n\
         - \"expected_change_found\": boolean, true if the specific expected change is visible\n\
         - \"description\": short string, at most 16 words, describing the main difference\n\
         - \"unexpected_changes\": array of at most 3 short strings listing changes NOT described above\n\n\
         Keep the response compact and specific.",
        change_description
    )
}

/// Build the prompt for verifying specific UI elements.
fn build_elements_prompt(elements: &[UiElement]) -> String {
    let elements_desc: Vec<String> = elements
        .iter()
        .enumerate()
        .map(|(i, el)| {
            let mut desc = format!("{}. \"{}\" (type: {})", i + 1, el.name, el.element_type);
            if let Some(ref text) = el.expected_text {
                desc.push_str(&format!(", expected text: \"{}\"", text));
            }
            if let Some(ref loc) = el.expected_location {
                desc.push_str(&format!(", expected location: {}", loc));
            }
            desc
        })
        .collect();

    format!(
        "You are a UI element verification assistant. Analyze the screenshot and check \
         for the presence of each of the following UI elements:\n\n{}\n\n\
         Respond ONLY with a JSON array (no markdown fences, no extra text). Each element \
         in the array should be a JSON object with these fields:\n\
         - \"name\": string, the element name from the list above\n\
         - \"found\": boolean, true if the element is visible in the screenshot\n\
         - \"location\": string or null, where the element appears (e.g. \"top-left\", \"center\")\n\
         - \"actual_text\": string or null, the actual text content if applicable",
        elements_desc.join("\n")
    )
}

// ---------------------------------------------------------------------------
// Response parsers
// ---------------------------------------------------------------------------

/// Extract JSON from a VLM response that may contain markdown fences or preamble.
fn extract_json_from_response(raw: &str) -> &str {
    let trimmed = raw.trim();

    // Strip markdown code fences if present
    if let Some(start) = trimmed.find("```json") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        if let Some(end) = after_fence.find("```") {
            return after_fence[..end].trim();
        }
    }

    // Try to find the first JSON object or array, whichever comes first
    let obj_start = trimmed.find('{');
    let arr_start = trimmed.find('[');

    match (obj_start, arr_start) {
        (Some(o), Some(a)) if a < o => {
            // Array starts before object -- prefer array
            if let Some(end) = trimmed.rfind(']') {
                return &trimmed[a..=end];
            }
        }
        (Some(o), _) => {
            if let Some(end) = trimmed.rfind('}') {
                return &trimmed[o..=end];
            }
        }
        (None, Some(a)) => {
            if let Some(end) = trimmed.rfind(']') {
                return &trimmed[a..=end];
            }
        }
        (None, None) => {}
    }

    trimmed
}

/// Parse a verification response from the VLM.
fn parse_verification_response(raw: &str) -> Result<VisualVerificationResult> {
    let json_str = extract_json_from_response(raw);
    let parsed: Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse VLM verification response as JSON: {}",
            &raw[..raw.len().min(200)]
        )
    })?;

    Ok(VisualVerificationResult {
        passed: parsed["passed"].as_bool().unwrap_or(false),
        confidence: parsed["confidence"]
            .as_f64()
            .unwrap_or_else(|| {
                if parsed["passed"].as_bool().unwrap_or(false) {
                    1.0
                } else {
                    0.0
                }
            })
            .clamp(0.0, 1.0),
        description: parsed["description"]
            .as_str()
            .or_else(|| parsed["summary"].as_str())
            .unwrap_or("")
            .to_string(),
        issues: parsed["issues"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Parse a diff comparison response from the VLM.
fn parse_diff_response(raw: &str) -> Result<VisualDiffResult> {
    let json_str = extract_json_from_response(raw);
    let parsed: Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse VLM diff response as JSON: {}",
            &raw[..raw.len().min(200)]
        )
    })?;

    let changes_detected = parsed["changes_detected"]
        .as_bool()
        .or_else(|| parsed["changed"].as_bool())
        .unwrap_or(false);
    let expected_change_found = parsed["expected_change_found"]
        .as_bool()
        .or_else(|| parsed["changed"].as_bool())
        .unwrap_or(false);
    let description = parsed["description"]
        .as_str()
        .map(String::from)
        .or_else(|| {
            parsed["change_kind"].as_str().map(|kind| {
                let diffs: Vec<&str> = parsed["differences"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).take(2).collect())
                    .unwrap_or_default();
                if diffs.is_empty() {
                    kind.to_string()
                } else {
                    format!("{}: {}", kind, diffs.join("; "))
                }
            })
        })
        .unwrap_or_default();

    Ok(VisualDiffResult {
        changes_detected,
        expected_change_found,
        description,
        unexpected_changes: parsed["unexpected_changes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

/// Parse element verification responses from the VLM.
fn parse_elements_response(raw: &str, elements: &[UiElement]) -> Result<Vec<ElementVerification>> {
    let json_str = extract_json_from_response(raw);
    let parsed: Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse VLM elements response as JSON: {}",
            &raw[..raw.len().min(200)]
        )
    })?;

    let arr = parsed
        .as_array()
        .with_context(|| "Expected a JSON array from VLM elements response")?;

    // Build results by matching VLM output back to our element list.
    // If the VLM returns fewer items than we asked for, mark the rest as not found.
    let mut results: Vec<ElementVerification> = Vec::with_capacity(elements.len());

    for element in elements {
        // Try to find a matching entry in the VLM response
        let matched = arr.iter().find(|item| {
            item["name"]
                .as_str()
                .map(|n| n == element.name)
                .unwrap_or(false)
        });

        match matched {
            Some(item) => {
                results.push(ElementVerification {
                    element: element.clone(),
                    found: item["found"].as_bool().unwrap_or(false),
                    location: item["location"].as_str().map(String::from),
                    actual_text: item["actual_text"].as_str().map(String::from),
                });
            }
            None => {
                results.push(ElementVerification {
                    element: element.clone(),
                    found: false,
                    location: None,
                    actual_text: None,
                });
            }
        }
    }

    Ok(results)
}

/// Parse a layout analysis response from the VLM.
fn parse_layout_response(raw: &str) -> Result<LayoutAnalysis> {
    let json_str = extract_json_from_response(raw);
    let parsed: Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse VLM layout response as JSON: {}",
            &raw[..raw.len().min(200)]
        )
    })?;

    Ok(LayoutAnalysis {
        overall_quality: parsed["overall_quality"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        alignment_issues: parsed["alignment_issues"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        spacing_issues: parsed["spacing_issues"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        responsive_notes: parsed["responsive_notes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

// ---------------------------------------------------------------------------
// Visual Stuck-Loop Detection
// ---------------------------------------------------------------------------

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Tracks screenshot history to detect stuck loops
#[derive(Debug, Clone)]
pub struct VisualStateTracker {
    /// Ring buffer of recent screenshot states
    history: VecDeque<ScreenshotState>,
    /// Maximum history size
    max_history: usize,
    /// Threshold for considering screenshots "similar" (0.0-1.0)
    similarity_threshold: f32,
    /// How many consecutive similar states before declaring "stuck"
    stuck_threshold: usize,
    /// Minimum hash similarity to consider screens "same" (0.0-1.0)
    hash_similarity_threshold: f32,
}

/// A recorded screenshot state for loop detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotState {
    /// Perceptual hash of the screenshot (for fast compare)
    pub hash: String,
    /// Semantic description of what's visible
    pub semantic_description: String,
    /// Timestamp when this state was recorded
    pub timestamp: DateTime<Utc>,
    /// Action that was taken from this state
    pub action_taken: String,
    /// Whether the action succeeded
    pub action_succeeded: bool,
    /// Optional screenshot path for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<PathBuf>,
}

/// Result of stuck-loop detection
#[derive(Debug, Clone)]
pub enum LoopDetectionResult {
    /// No loop detected, proceed normally
    Proceed,
    /// Possible loop forming - warn
    Warning {
        similar_states: Vec<ScreenshotState>,
    },
    /// Stuck loop confirmed - need recovery
    Stuck {
        loop_pattern: Vec<ScreenshotState>,
        suggested_recovery: RecoveryStrategy,
    },
}

/// Recovery strategy when a stuck loop is detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecoveryStrategy {
    /// Try a different action on same screen
    TryDifferentAction { alternatives: Vec<String> },
    /// Reset to known good state
    ResetToCheckpoint,
    /// Escalate to user
    EscalateToUser { reason: String },
    /// Wait and retry (for transient states)
    WaitAndRetry { delay_ms: u64 },
    /// Take a screenshot to reassess the current state
    ReassessWithScreenshot,
    /// Change input method (e.g., keyboard instead of mouse)
    ChangeInputMethod { suggestion: String },
}

impl std::fmt::Display for RecoveryStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryStrategy::TryDifferentAction { alternatives } => {
                write!(
                    f,
                    "Try different action. Alternatives: {}",
                    alternatives.join(", ")
                )
            }
            RecoveryStrategy::ResetToCheckpoint => write!(f, "Reset to checkpoint"),
            RecoveryStrategy::EscalateToUser { reason } => {
                write!(f, "Escalate to user: {}", reason)
            }
            RecoveryStrategy::WaitAndRetry { delay_ms } => {
                write!(f, "Wait {}ms and retry", delay_ms)
            }
            RecoveryStrategy::ReassessWithScreenshot => write!(f, "Reassess with fresh screenshot"),
            RecoveryStrategy::ChangeInputMethod { suggestion } => {
                write!(f, "Change input method: {}", suggestion)
            }
        }
    }
}

impl VisualStateTracker {
    /// Create a new visual state tracker with default settings
    pub fn new(max_history: usize, stuck_threshold: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            similarity_threshold: 0.85,
            stuck_threshold,
            hash_similarity_threshold: 0.90,
        }
    }

    /// Create with custom similarity thresholds
    pub fn with_thresholds(
        max_history: usize,
        stuck_threshold: usize,
        similarity_threshold: f32,
        hash_similarity_threshold: f32,
    ) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            similarity_threshold,
            stuck_threshold,
            hash_similarity_threshold,
        }
    }

    /// Create a default configuration (20 history, 2 repeats = stuck)
    pub fn default_config() -> Self {
        Self::new(20, 2)
    }

    /// Record a new state and check for loops
    pub fn record_state(
        &mut self,
        screenshot_path: &Path,
        semantic_description: String,
        action_taken: String,
        action_succeeded: bool,
    ) -> Result<LoopDetectionResult> {
        let hash = compute_perceptual_hash(screenshot_path)?;
        let state = ScreenshotState {
            hash,
            semantic_description,
            timestamp: Utc::now(),
            action_taken: action_taken.clone(),
            action_succeeded,
            screenshot_path: Some(screenshot_path.to_path_buf()),
        };

        // Check for similar states in history
        let similar: Vec<_> = self
            .history
            .iter()
            .filter(|h| self.is_same_screen(&state, h))
            .cloned()
            .collect();

        let result = if similar.len() >= self.stuck_threshold {
            // We're stuck in a loop
            let strategy = self.determine_recovery_strategy(&similar, action_succeeded);
            LoopDetectionResult::Stuck {
                loop_pattern: similar,
                suggested_recovery: strategy,
            }
        } else if !similar.is_empty() {
            // Possible loop forming
            LoopDetectionResult::Warning {
                similar_states: similar,
            }
        } else {
            LoopDetectionResult::Proceed
        };

        // Add to history
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(state);

        Ok(result)
    }

    /// Record state with pre-computed hash (for efficiency when hash is already known)
    pub fn record_state_with_hash(
        &mut self,
        screenshot_hash: String,
        semantic_description: String,
        action_taken: String,
        action_succeeded: bool,
    ) -> LoopDetectionResult {
        let state = ScreenshotState {
            hash: screenshot_hash,
            semantic_description,
            timestamp: Utc::now(),
            action_taken: action_taken.clone(),
            action_succeeded,
            screenshot_path: None,
        };

        // Check for similar states in history
        let similar: Vec<_> = self
            .history
            .iter()
            .filter(|h| self.is_same_screen(&state, h))
            .cloned()
            .collect();

        let result = if similar.len() >= self.stuck_threshold {
            let strategy = self.determine_recovery_strategy(&similar, action_succeeded);
            LoopDetectionResult::Stuck {
                loop_pattern: similar,
                suggested_recovery: strategy,
            }
        } else if !similar.is_empty() {
            LoopDetectionResult::Warning {
                similar_states: similar,
            }
        } else {
            LoopDetectionResult::Proceed
        };

        // Add to history
        if self.history.len() >= self.max_history {
            self.history.pop_front();
        }
        self.history.push_back(state);

        result
    }

    /// Check if two screenshot states represent the same screen with same failed action
    fn is_same_screen(&self, state1: &ScreenshotState, state2: &ScreenshotState) -> bool {
        let hash_sim = compute_hash_similarity(&state1.hash, &state2.hash);
        let action_same = state1.action_taken == state2.action_taken;
        let both_failed = !state1.action_succeeded && !state2.action_succeeded;

        // Same screen = high visual similarity + same action + both failed
        hash_sim >= self.hash_similarity_threshold && action_same && both_failed
    }

    /// Determine the best recovery strategy based on the loop pattern
    fn determine_recovery_strategy(
        &self,
        pattern: &[ScreenshotState],
        _last_action_succeeded: bool,
    ) -> RecoveryStrategy {
        let action = pattern
            .last()
            .map(|s| s.action_taken.clone())
            .unwrap_or_default();

        // Analyze the pattern to suggest appropriate recovery
        if pattern.len() >= 3 {
            // Persistent loop - try fundamentally different approach
            RecoveryStrategy::TryDifferentAction {
                alternatives: vec![
                    format!("Use keyboard shortcut instead of: {}", action),
                    "Wait for animation to complete before next action".to_string(),
                    "Refresh the page/application and retry".to_string(),
                    "Try a different UI element or location".to_string(),
                ],
            }
        } else if pattern.len() == 2 {
            // Early loop detection - suggest alternatives
            RecoveryStrategy::TryDifferentAction {
                alternatives: vec![
                    format!("Retry '{}' after brief pause", action),
                    "Check if element is interactable".to_string(),
                    "Try alternative selector or coordinates".to_string(),
                ],
            }
        } else {
            // Single repeat - wait and retry
            RecoveryStrategy::WaitAndRetry { delay_ms: 1000 }
        }
    }

    /// Get the current history size
    pub fn history_size(&self) -> usize {
        self.history.len()
    }

    /// Clear all history
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Get a reference to the history
    pub fn history(&self) -> &VecDeque<ScreenshotState> {
        &self.history
    }

    /// Check if a similar state exists in recent history (for quick checks)
    pub fn has_similar_state(&self, hash: &str, action: &str) -> bool {
        self.history.iter().any(|h| {
            let hash_sim = compute_hash_similarity(&h.hash, hash);
            hash_sim >= self.hash_similarity_threshold
                && h.action_taken == action
                && !h.action_succeeded
        })
    }
}

/// Compute perceptual hash (dhash) for a screenshot
///
/// Uses difference hash algorithm:
/// 1. Resize image to 9x8
/// 2. Compute gradient between adjacent pixels
/// 3. Return hex string of hash
pub fn compute_perceptual_hash(screenshot_path: &Path) -> Result<String> {
    use image::GenericImageView;

    // Open and decode the image
    let img = image::open(screenshot_path)
        .with_context(|| format!("Failed to open screenshot: {}", screenshot_path.display()))?;

    // Convert to grayscale and resize to 9x8 for dhash
    let gray = img.to_luma8();
    let resized = image::imageops::resize(&gray, 9, 8, image::imageops::FilterType::Lanczos3);

    // Compute difference hash
    let mut hash_bits = Vec::with_capacity(64);
    for y in 0..8 {
        for x in 0..8 {
            let left = resized.get_pixel(x, y)[0];
            let right = resized.get_pixel(x + 1, y)[0];
            hash_bits.push(if right > left { 1 } else { 0 });
        }
    }

    // Convert bits to hex string
    let mut hex_string = String::with_capacity(16);
    for chunk in hash_bits.chunks(4) {
        let nibble = chunk.iter().fold(0u8, |acc, &bit| (acc << 1) | bit);
        let hex_char = match nibble {
            0..=9 => (b'0' + nibble) as char,
            10..=15 => (b'a' + nibble - 10) as char,
            _ => '0',
        };
        hex_string.push(hex_char);
    }

    Ok(hex_string)
}

/// Compare two perceptual hashes using Hamming distance
/// Returns similarity score 0.0-1.0 where 1.0 is identical
pub fn compute_hash_similarity(hash1: &str, hash2: &str) -> f32 {
    if hash1.len() != hash2.len() {
        // Different lengths - compute similarity based on common prefix
        let min_len = hash1.len().min(hash2.len());
        let max_len = hash1.len().max(hash2.len());
        let common = hash1
            .bytes()
            .zip(hash2.bytes())
            .take(min_len)
            .filter(|(a, b)| a == b)
            .count();
        return common as f32 / max_len as f32;
    }

    let max_distance = hash1.len() * 4; // Each hex char = 4 bits
    let distance = hamming_distance(hash1, hash2);

    // Convert to similarity: 1.0 - normalized distance
    1.0 - (distance as f32 / max_distance as f32)
}

/// Compute Hamming distance between two hex strings
fn hamming_distance(s1: &str, s2: &str) -> usize {
    s1.bytes()
        .zip(s2.bytes())
        .map(|(b1, b2)| {
            let n1 = hex_to_nibble(b1);
            let n2 = hex_to_nibble(b2);
            (n1 ^ n2).count_ones() as usize
        })
        .sum()
}

/// Convert hex character to nibble
fn hex_to_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

/// Configuration for visual stuck-loop detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualLoopConfig {
    /// Maximum number of states to keep in history
    pub max_history: usize,
    /// Number of similar states required to trigger stuck detection
    pub stuck_threshold: usize,
    /// Hash similarity threshold (0.0-1.0)
    pub hash_similarity_threshold: f32,
    /// Semantic similarity threshold (0.0-1.0) - for future use with VLM
    pub semantic_similarity_threshold: f32,
    /// Enable automatic recovery when stuck
    pub auto_recovery: bool,
}

impl Default for VisualLoopConfig {
    fn default() -> Self {
        Self {
            max_history: 20,
            stuck_threshold: 2,
            hash_similarity_threshold: 0.90,
            semantic_similarity_threshold: 0.85,
            auto_recovery: true,
        }
    }
}

impl VisualLoopConfig {
    /// Create a new tracker from this config
    pub fn create_tracker(&self) -> VisualStateTracker {
        VisualStateTracker::with_thresholds(
            self.max_history,
            self.stuck_threshold,
            self.semantic_similarity_threshold,
            self.hash_similarity_threshold,
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../../tests/unit/testing/visual_verification/visual_verification_test.rs"]
mod tests;
