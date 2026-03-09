#![allow(dead_code, unused_imports, unused_variables)]
//! Visual verification using Vision Language Models (VLMs).
//!
//! Provides automated visual testing by sending screenshots to a VLM endpoint
//! and parsing structured verification results. Integrates with the existing
//! [`VerificationGate`](super::verification::VerificationGate) pipeline.
//!
//! # Example
//!
//! ```rust,no_run
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
    /// HTTP request timeout in seconds.
    timeout_secs: u64,
}

impl VisualVerifier {
    /// Create a new verifier with the given endpoint and model.
    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            timeout_secs: default_visual_timeout(),
        }
    }

    /// Create a verifier from a [`VisualVerificationConfig`].
    pub fn from_config(config: &VisualVerificationConfig) -> Self {
        Self {
            endpoint: config.endpoint.clone(),
            model: config.model.clone(),
            timeout_secs: config.timeout_secs,
        }
    }

    /// Override the request timeout.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
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
        let body = self.build_single_image_request(&prompt, image_base64);
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
        let body = self.build_two_image_request(&prompt, before, after);
        let raw = self.call_vlm(&body).await?;
        parse_diff_response(&raw)
    }

    /// Extract visible text from a screenshot using the VLM as an OCR engine.
    pub async fn extract_text_from_screenshot(
        &self,
        image_base64: &str,
    ) -> Result<String> {
        let prompt = "Extract ALL visible text from this screenshot. \
                      Return only the extracted text, preserving line breaks \
                      and layout as much as possible. Do not add commentary.";
        let body = self.build_single_image_request(prompt, image_base64);
        self.call_vlm(&body).await
    }

    /// Verify that specific UI elements are present and visible.
    pub async fn verify_ui_elements(
        &self,
        image_base64: &str,
        elements: &[UiElement],
    ) -> Result<Vec<ElementVerification>> {
        let prompt = build_elements_prompt(elements);
        let body = self.build_single_image_request(&prompt, image_base64);
        let raw = self.call_vlm(&body).await?;
        parse_elements_response(&raw, elements)
    }

    /// Analyze page layout for alignment, spacing, and quality.
    pub async fn analyze_layout(
        &self,
        image_base64: &str,
    ) -> Result<LayoutAnalysis> {
        let prompt = "Analyze the layout of this screenshot. Respond in JSON with these fields:\n\
                      - \"overall_quality\": \"good\", \"fair\", or \"poor\"\n\
                      - \"alignment_issues\": array of strings describing any alignment problems\n\
                      - \"spacing_issues\": array of strings describing any spacing problems\n\
                      - \"responsive_notes\": array of strings with notes about the layout\n\
                      \n\
                      Respond ONLY with the JSON object, no extra text.";
        let body = self.build_single_image_request(prompt, image_base64);
        let raw = self.call_vlm(&body).await?;
        parse_layout_response(&raw)
    }

    // -----------------------------------------------------------------------
    // Convenience methods
    // -----------------------------------------------------------------------

    /// Capture the current screen and verify it against a description.
    ///
    /// Combines [`crate::computer::screen::ScreenCapture::capture_full`]
    /// with [`verify_screenshot`](Self::verify_screenshot).
    pub async fn capture_and_verify(
        &self,
        expected: &str,
    ) -> Result<VisualVerificationResult> {
        let captured = crate::computer::screen::ScreenCapture::capture_full().await?;
        self.verify_screenshot(&captured.base64_png, expected).await
    }

    /// Capture the terminal screen and verify expected text patterns are visible.
    pub async fn verify_terminal_output(
        &self,
        expected_patterns: &[&str],
    ) -> Result<VisualVerificationResult> {
        let captured = crate::computer::screen::ScreenCapture::capture_full().await?;
        let description = format!(
            "A terminal window showing the following text patterns: {}",
            expected_patterns.join(", ")
        );
        self.verify_screenshot(&captured.base64_png, &description)
            .await
    }

    /// Capture a browser page and verify specific UI elements.
    ///
    /// Note: this captures the current screen -- the caller is responsible
    /// for navigating the browser to the target URL first.
    pub async fn verify_browser_page(
        &self,
        _url: &str,
        expected_elements: &[UiElement],
    ) -> Result<Vec<ElementVerification>> {
        let captured = crate::computer::screen::ScreenCapture::capture_full().await?;
        self.verify_ui_elements(&captured.base64_png, expected_elements)
            .await
    }

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
    fn build_single_image_request(&self, prompt: &str, image_base64: &str) -> Value {
        let data_uri = format!("data:image/png;base64,{}", image_base64);
        json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": data_uri } }
                ]
            }],
            "max_tokens": 4096,
            "temperature": 0.2,
            "stream": false
        })
    }

    /// Build a chat-completion request body with two images (before/after).
    fn build_two_image_request(
        &self,
        prompt: &str,
        before_base64: &str,
        after_base64: &str,
    ) -> Value {
        let uri_before = format!("data:image/png;base64,{}", before_base64);
        let uri_after = format!("data:image/png;base64,{}", after_base64);
        json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": uri_before } },
                    { "type": "image_url", "image_url": { "url": uri_after } }
                ]
            }],
            "max_tokens": 4096,
            "temperature": 0.2,
            "stream": false
        })
    }

    /// Send a request to the VLM endpoint and extract the response text.
    async fn call_vlm(&self, body: &Value) -> Result<String> {
        let url = format!(
            "{}/chat/completions",
            self.endpoint.trim_end_matches('/')
        );
        debug!("Calling VLM endpoint: {}", url);

        let client = Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
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
        "You are a visual verification assistant. Analyze the provided screenshot \
         and determine if it matches the following expected description:\n\n\
         EXPECTED: {}\n\n\
         Respond ONLY with a JSON object (no markdown fences, no extra text) with these fields:\n\
         - \"passed\": boolean, true if the screenshot matches the expected description\n\
         - \"confidence\": number between 0.0 and 1.0 indicating your confidence\n\
         - \"description\": string describing what you actually see in the screenshot\n\
         - \"issues\": array of strings listing any mismatches or problems found\n\n\
         If everything matches, set \"passed\" to true and \"issues\" to an empty array.",
        expected
    )
}

/// Build the prompt for comparing two screenshots.
fn build_compare_prompt(change_description: &str) -> String {
    format!(
        "You are a visual diff assistant. Compare the two screenshots (image 1 = BEFORE, \
         image 2 = AFTER) and determine whether the following expected change occurred:\n\n\
         EXPECTED CHANGE: {}\n\n\
         Respond ONLY with a JSON object (no markdown fences, no extra text) with these fields:\n\
         - \"changes_detected\": boolean, true if the images differ\n\
         - \"expected_change_found\": boolean, true if the specific expected change is visible\n\
         - \"description\": string describing the differences between the images\n\
         - \"unexpected_changes\": array of strings listing any changes NOT described above",
        change_description
    )
}

/// Build the prompt for verifying specific UI elements.
fn build_elements_prompt(elements: &[UiElement]) -> String {
    let elements_desc: Vec<String> = elements
        .iter()
        .enumerate()
        .map(|(i, el)| {
            let mut desc = format!(
                "{}. \"{}\" (type: {})",
                i + 1,
                el.name,
                el.element_type
            );
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
        confidence: parsed["confidence"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
        description: parsed["description"]
            .as_str()
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

    Ok(VisualDiffResult {
        changes_detected: parsed["changes_detected"].as_bool().unwrap_or(false),
        expected_change_found: parsed["expected_change_found"]
            .as_bool()
            .unwrap_or(false),
        description: parsed["description"]
            .as_str()
            .unwrap_or("")
            .to_string(),
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
fn parse_elements_response(
    raw: &str,
    elements: &[UiElement],
) -> Result<Vec<ElementVerification>> {
    let json_str = extract_json_from_response(raw);
    let parsed: Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "Failed to parse VLM elements response as JSON: {}",
            &raw[..raw.len().min(200)]
        )
    })?;

    let arr = parsed.as_array().with_context(|| {
        "Expected a JSON array from VLM elements response"
    })?;

    // Build results by matching VLM output back to our element list.
    // If the VLM returns fewer items than we asked for, mark the rest as not found.
    let mut results: Vec<ElementVerification> = Vec::with_capacity(elements.len());

    for element in elements {
        // Try to find a matching entry in the VLM response
        let matched = arr.iter().find(|item| {
            item["name"].as_str().map(|n| n == element.name).unwrap_or(false)
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Result type construction & serialization ----

    #[test]
    fn test_visual_verification_result_serialization() {
        let result = VisualVerificationResult {
            passed: true,
            confidence: 0.95,
            description: "A login page with two input fields".to_string(),
            issues: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: VisualVerificationResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.passed);
        assert!((deserialized.confidence - 0.95).abs() < f64::EPSILON);
        assert!(deserialized.issues.is_empty());
    }

    #[test]
    fn test_visual_verification_result_with_issues() {
        let result = VisualVerificationResult {
            passed: false,
            confidence: 0.4,
            description: "A blank white page".to_string(),
            issues: vec![
                "Expected login form not found".to_string(),
                "No input fields visible".to_string(),
            ],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: VisualVerificationResult = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.passed);
        assert_eq!(deserialized.issues.len(), 2);
    }

    #[test]
    fn test_visual_diff_result_serialization() {
        let result = VisualDiffResult {
            changes_detected: true,
            expected_change_found: true,
            description: "Button color changed from gray to blue".to_string(),
            unexpected_changes: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: VisualDiffResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.changes_detected);
        assert!(deserialized.expected_change_found);
        assert!(deserialized.unexpected_changes.is_empty());
    }

    #[test]
    fn test_ui_element_serialization() {
        let element = UiElement {
            name: "Submit Button".to_string(),
            element_type: "button".to_string(),
            expected_text: Some("Submit".to_string()),
            expected_location: Some("bottom-right".to_string()),
        };
        let json = serde_json::to_string(&element).unwrap();
        let deserialized: UiElement = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "Submit Button");
        assert_eq!(deserialized.element_type, "button");
        assert_eq!(deserialized.expected_text.as_deref(), Some("Submit"));
        assert_eq!(deserialized.expected_location.as_deref(), Some("bottom-right"));
    }

    #[test]
    fn test_element_verification_serialization() {
        let ev = ElementVerification {
            element: UiElement {
                name: "Logo".to_string(),
                element_type: "image".to_string(),
                expected_text: None,
                expected_location: Some("top-left".to_string()),
            },
            found: true,
            location: Some("top-left".to_string()),
            actual_text: None,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let deserialized: ElementVerification = serde_json::from_str(&json).unwrap();
        assert!(deserialized.found);
        assert_eq!(deserialized.location.as_deref(), Some("top-left"));
    }

    #[test]
    fn test_layout_analysis_serialization() {
        let analysis = LayoutAnalysis {
            overall_quality: "good".to_string(),
            alignment_issues: vec![],
            spacing_issues: vec!["Footer too close to content".to_string()],
            responsive_notes: vec!["Sidebar collapses on narrow viewports".to_string()],
        };
        let json = serde_json::to_string(&analysis).unwrap();
        let deserialized: LayoutAnalysis = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.overall_quality, "good");
        assert!(deserialized.alignment_issues.is_empty());
        assert_eq!(deserialized.spacing_issues.len(), 1);
        assert_eq!(deserialized.responsive_notes.len(), 1);
    }

    // ---- Config defaults ----

    #[test]
    fn test_config_defaults() {
        let config = VisualVerificationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.endpoint, "http://localhost:1234/v1");
        assert_eq!(config.model, "qwen2-vl-7b");
        assert_eq!(config.timeout_secs, 120);
        assert!((config.confidence_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = VisualVerificationConfig {
            enabled: true,
            endpoint: "http://example.com/v1".to_string(),
            model: "gpt-4-vision".to_string(),
            timeout_secs: 60,
            confidence_threshold: 0.85,
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: VisualVerificationConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.enabled);
        assert_eq!(deserialized.model, "gpt-4-vision");
    }

    // ---- Prompt construction ----

    #[test]
    fn test_build_verify_prompt() {
        let prompt = build_verify_prompt("A page with a red button");
        assert!(prompt.contains("A page with a red button"));
        assert!(prompt.contains("passed"));
        assert!(prompt.contains("confidence"));
        assert!(prompt.contains("description"));
        assert!(prompt.contains("issues"));
    }

    #[test]
    fn test_build_compare_prompt() {
        let prompt = build_compare_prompt("The header changed from blue to green");
        assert!(prompt.contains("The header changed from blue to green"));
        assert!(prompt.contains("changes_detected"));
        assert!(prompt.contains("expected_change_found"));
        assert!(prompt.contains("unexpected_changes"));
    }

    #[test]
    fn test_build_elements_prompt() {
        let elements = vec![
            UiElement {
                name: "Login Button".to_string(),
                element_type: "button".to_string(),
                expected_text: Some("Log In".to_string()),
                expected_location: Some("center".to_string()),
            },
            UiElement {
                name: "Logo".to_string(),
                element_type: "image".to_string(),
                expected_text: None,
                expected_location: Some("top-left".to_string()),
            },
        ];
        let prompt = build_elements_prompt(&elements);
        assert!(prompt.contains("Login Button"));
        assert!(prompt.contains("button"));
        assert!(prompt.contains("Log In"));
        assert!(prompt.contains("center"));
        assert!(prompt.contains("Logo"));
        assert!(prompt.contains("image"));
        assert!(prompt.contains("top-left"));
    }

    #[test]
    fn test_build_elements_prompt_empty() {
        let prompt = build_elements_prompt(&[]);
        // Should still produce a valid prompt, just no element list
        assert!(prompt.contains("JSON array"));
    }

    // ---- Response parsing ----

    #[test]
    fn test_parse_verification_response_pass() {
        let raw = r#"{"passed": true, "confidence": 0.92, "description": "Login page with form", "issues": []}"#;
        let result = parse_verification_response(raw).unwrap();
        assert!(result.passed);
        assert!((result.confidence - 0.92).abs() < f64::EPSILON);
        assert_eq!(result.description, "Login page with form");
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_parse_verification_response_fail() {
        let raw = r#"{"passed": false, "confidence": 0.3, "description": "Empty page", "issues": ["No form found", "Missing header"]}"#;
        let result = parse_verification_response(raw).unwrap();
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 2);
        assert_eq!(result.issues[0], "No form found");
    }

    #[test]
    fn test_parse_verification_response_with_markdown_fences() {
        let raw = "Here is the result:\n```json\n{\"passed\": true, \"confidence\": 0.88, \"description\": \"OK\", \"issues\": []}\n```\nDone.";
        let result = parse_verification_response(raw).unwrap();
        assert!(result.passed);
        assert!((result.confidence - 0.88).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_verification_response_with_preamble() {
        let raw = "I analyzed the screenshot and here is my assessment:\n{\"passed\": false, \"confidence\": 0.5, \"description\": \"A dashboard\", \"issues\": [\"Missing sidebar\"]}";
        let result = parse_verification_response(raw).unwrap();
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 1);
    }

    #[test]
    fn test_parse_verification_response_clamps_confidence() {
        let raw = r#"{"passed": true, "confidence": 1.5, "description": "Good", "issues": []}"#;
        let result = parse_verification_response(raw).unwrap();
        assert!((result.confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_verification_response_missing_fields() {
        // VLM might omit some fields -- we should handle gracefully
        let raw = r#"{"passed": true}"#;
        let result = parse_verification_response(raw).unwrap();
        assert!(result.passed);
        assert!((result.confidence - 0.0).abs() < f64::EPSILON);
        assert_eq!(result.description, "");
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_parse_diff_response() {
        let raw = r#"{"changes_detected": true, "expected_change_found": true, "description": "Button color changed", "unexpected_changes": ["Font size also changed"]}"#;
        let result = parse_diff_response(raw).unwrap();
        assert!(result.changes_detected);
        assert!(result.expected_change_found);
        assert_eq!(result.unexpected_changes.len(), 1);
    }

    #[test]
    fn test_parse_diff_response_no_changes() {
        let raw = r#"{"changes_detected": false, "expected_change_found": false, "description": "Images appear identical", "unexpected_changes": []}"#;
        let result = parse_diff_response(raw).unwrap();
        assert!(!result.changes_detected);
        assert!(!result.expected_change_found);
    }

    #[test]
    fn test_parse_elements_response() {
        let elements = vec![
            UiElement {
                name: "Login".to_string(),
                element_type: "button".to_string(),
                expected_text: Some("Log In".to_string()),
                expected_location: None,
            },
            UiElement {
                name: "Logo".to_string(),
                element_type: "image".to_string(),
                expected_text: None,
                expected_location: Some("top-left".to_string()),
            },
        ];
        let raw = r#"[
            {"name": "Login", "found": true, "location": "center", "actual_text": "Log In"},
            {"name": "Logo", "found": true, "location": "top-left", "actual_text": null}
        ]"#;
        let results = parse_elements_response(raw, &elements).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].found);
        assert_eq!(results[0].actual_text.as_deref(), Some("Log In"));
        assert!(results[1].found);
        assert_eq!(results[1].location.as_deref(), Some("top-left"));
    }

    #[test]
    fn test_parse_elements_response_missing_element() {
        let elements = vec![
            UiElement {
                name: "Button".to_string(),
                element_type: "button".to_string(),
                expected_text: None,
                expected_location: None,
            },
            UiElement {
                name: "Missing".to_string(),
                element_type: "text".to_string(),
                expected_text: None,
                expected_location: None,
            },
        ];
        // VLM only returned info about "Button", not "Missing"
        let raw = r#"[{"name": "Button", "found": true, "location": "center", "actual_text": null}]"#;
        let results = parse_elements_response(raw, &elements).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].found);
        assert!(!results[1].found); // Missing element marked as not found
    }

    #[test]
    fn test_parse_layout_response() {
        let raw = r#"{"overall_quality": "fair", "alignment_issues": ["Logo off-center"], "spacing_issues": [], "responsive_notes": ["Works on mobile"]}"#;
        let result = parse_layout_response(raw).unwrap();
        assert_eq!(result.overall_quality, "fair");
        assert_eq!(result.alignment_issues.len(), 1);
        assert!(result.spacing_issues.is_empty());
        assert_eq!(result.responsive_notes.len(), 1);
    }

    #[test]
    fn test_parse_layout_response_minimal() {
        let raw = r#"{"overall_quality": "good"}"#;
        let result = parse_layout_response(raw).unwrap();
        assert_eq!(result.overall_quality, "good");
        assert!(result.alignment_issues.is_empty());
    }

    // ---- JSON extraction ----

    #[test]
    fn test_extract_json_from_clean() {
        let raw = r#"{"key": "value"}"#;
        assert_eq!(extract_json_from_response(raw), raw);
    }

    #[test]
    fn test_extract_json_from_markdown_fences() {
        let raw = "Some text\n```json\n{\"key\": \"value\"}\n```\nMore text";
        assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_from_plain_fences() {
        let raw = "```\n{\"key\": \"value\"}\n```";
        assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_with_preamble() {
        let raw = "Here is the result: {\"key\": \"value\"} and more text";
        assert_eq!(extract_json_from_response(raw), r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_array() {
        let raw = "Result: [{\"a\": 1}, {\"b\": 2}]";
        assert_eq!(extract_json_from_response(raw), r#"[{"a": 1}, {"b": 2}]"#);
    }

    // ---- Request body construction ----

    #[test]
    fn test_build_single_image_request() {
        let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
        let body = verifier.build_single_image_request("Describe this", "AAAA");
        assert_eq!(body["model"], "test-model");
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["stream"], false);
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "Describe this");
        assert_eq!(content[1]["type"], "image_url");
        assert!(content[1]["image_url"]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn test_build_two_image_request() {
        let verifier = VisualVerifier::new("http://localhost:1234/v1", "test-model");
        let body = verifier.build_two_image_request("Compare", "BEFORE", "AFTER");
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[2]["type"], "image_url");
        let url1 = content[1]["image_url"]["url"].as_str().unwrap();
        let url2 = content[2]["image_url"]["url"].as_str().unwrap();
        assert!(url1.contains("BEFORE"));
        assert!(url2.contains("AFTER"));
    }

    // ---- Verifier construction ----

    #[test]
    fn test_verifier_new() {
        let v = VisualVerifier::new("http://example.com/v1", "model-x");
        assert_eq!(v.endpoint, "http://example.com/v1");
        assert_eq!(v.model, "model-x");
        assert_eq!(v.timeout_secs, 120);
    }

    #[test]
    fn test_verifier_from_config() {
        let config = VisualVerificationConfig {
            enabled: true,
            endpoint: "http://myhost:5000/v1".to_string(),
            model: "llava".to_string(),
            timeout_secs: 30,
            confidence_threshold: 0.9,
        };
        let v = VisualVerifier::from_config(&config);
        assert_eq!(v.endpoint, "http://myhost:5000/v1");
        assert_eq!(v.model, "llava");
        assert_eq!(v.timeout_secs, 30);
    }

    #[test]
    fn test_verifier_with_timeout() {
        let v = VisualVerifier::new("http://localhost/v1", "m")
            .with_timeout(45);
        assert_eq!(v.timeout_secs, 45);
    }

    // ---- Invalid JSON handling ----

    #[test]
    fn test_parse_verification_response_invalid_json() {
        let raw = "This is not JSON at all";
        assert!(parse_verification_response(raw).is_err());
    }

    #[test]
    fn test_parse_diff_response_invalid_json() {
        let raw = "Not valid";
        assert!(parse_diff_response(raw).is_err());
    }

    #[test]
    fn test_parse_elements_response_not_array() {
        let elements = vec![];
        let raw = r#"{"not": "an array"}"#;
        assert!(parse_elements_response(raw, &elements).is_err());
    }

    #[test]
    fn test_parse_layout_response_invalid() {
        let raw = "garbage";
        assert!(parse_layout_response(raw).is_err());
    }
}
