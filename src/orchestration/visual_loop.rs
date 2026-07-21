//! Autonomous Visual Feedback Loop
//!
//! Orchestrates an act → capture → evaluate → iterate cycle where selfware
//! modifies code/assets, renders the result, captures a screenshot, sends it
//! to a vision-capable LLM for structured scoring, and repeats until quality
//! meets the threshold — all without human intervention.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// How to capture the visual output for evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CaptureMethod {
    /// Full primary-monitor screenshot.
    Screen,
    /// Capture a window whose title contains this substring.
    Window(String),
    /// Screenshot a URL via the browser_screenshot tool.
    BrowserUrl(String),
}

/// Per-dimension visual quality scores returned by the VLM critic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisualScore {
    /// Layout balance and visual weight distribution (0–100).
    pub composition: f64,
    /// Clear visual hierarchy — headings, sections, focal points (0–100).
    pub hierarchy: f64,
    /// Text legibility, font sizing, line lengths (0–100).
    pub readability: f64,
    /// Consistent spacing, colors, typography across elements (0–100).
    pub consistency: f64,
    /// Contrast ratios, color-blind friendliness, focus indicators (0–100).
    pub accessibility: f64,
    /// Weighted average of all dimensions (0–100).
    pub overall: f64,
    /// Concrete suggestions for the next iteration.
    #[serde(default)]
    pub suggestions: Vec<String>,
}

impl VisualScore {
    /// Compute `overall` as the weighted average of the five dimensions.
    pub fn compute_overall(&mut self) {
        self.overall = self.composition * 0.20
            + self.hierarchy * 0.20
            + self.readability * 0.25
            + self.consistency * 0.15
            + self.accessibility * 0.20;
    }
}

/// Configuration for an autonomous visual feedback loop.
#[derive(Debug, Clone)]
pub struct VisualFeedbackLoop {
    /// Maximum iterations before giving up.
    pub max_iterations: usize,
    /// Overall score (0.0–1.0) that terminates the loop as "good enough".
    pub quality_threshold: f64,
    /// Key into `Config.models` pointing to a vision-capable model.
    pub vision_model_id: String,
    /// How to capture the visual output each iteration.
    pub capture_method: CaptureMethod,
}

impl Default for VisualFeedbackLoop {
    fn default() -> Self {
        Self {
            max_iterations: 5,
            quality_threshold: 0.8,
            vision_model_id: "vision".to_string(),
            capture_method: CaptureMethod::Screen,
        }
    }
}

/// Result of a completed visual feedback loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualLoopResult {
    /// Number of iterations actually performed.
    pub iterations: usize,
    /// Whether the quality threshold was met.
    pub threshold_met: bool,
    /// Score history (one per iteration).
    pub score_history: Vec<VisualScore>,
    /// Final score.
    pub final_score: VisualScore,
}

/// Build the VLM critic prompt that requests structured JSON scoring.
///
/// `context` provides the task description so the critic understands intent.
/// `previous_score` includes the prior iteration's score for trend awareness.
pub fn build_critic_prompt(
    context: &str,
    previous_score: Option<&VisualScore>,
    iteration: usize,
) -> String {
    let mut prompt = format!(
        "You are evaluating a visual design for the following task:\n\n{}\n\n\
         This is iteration {} of the visual feedback loop.\n\n",
        context,
        iteration + 1
    );

    if let Some(prev) = previous_score {
        prompt.push_str(&format!(
            "Previous iteration scores:\n\
             - Composition: {:.0}\n\
             - Hierarchy: {:.0}\n\
             - Readability: {:.0}\n\
             - Consistency: {:.0}\n\
             - Accessibility: {:.0}\n\
             - Overall: {:.0}\n\
             Previous suggestions: {}\n\n",
            prev.composition,
            prev.hierarchy,
            prev.readability,
            prev.consistency,
            prev.accessibility,
            prev.overall,
            prev.suggestions.join("; "),
        ));
    }

    prompt.push_str(
        "Analyze the screenshot and respond with ONLY a JSON object (no markdown, no explanation):\n\
         ```json\n\
         {\n  \
           \"composition\": <0-100>,\n  \
           \"hierarchy\": <0-100>,\n  \
           \"readability\": <0-100>,\n  \
           \"consistency\": <0-100>,\n  \
           \"accessibility\": <0-100>,\n  \
           \"overall\": <weighted average>,\n  \
           \"suggestions\": [\"specific improvement 1\", \"specific improvement 2\"]\n\
         }\n\
         ```"
    );

    prompt
}

/// Parse the VLM critic's JSON response into a `VisualScore`.
///
/// Tolerates markdown code fences and leading/trailing text.
pub fn parse_critic_response(response: &str) -> Result<VisualScore> {
    // Strip markdown code fences if present
    let trimmed = response.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    serde_json::from_str(json_str)
        .context("Failed to parse VLM critic response as VisualScore JSON")
}

#[cfg(test)]
#[path = "../../tests/unit/orchestration/visual_loop/visual_loop_test.rs"]
mod tests;
