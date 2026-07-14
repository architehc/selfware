//! Benchmark task definitions and pluggable evaluation.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::api::types::Message;

/// Result of evaluating a model response against expected criteria.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    /// Overall score (0.0–1.0).
    pub score: f64,
    /// Whether the task passed (score >= threshold).
    pub passed: bool,
    /// Per-criterion breakdown.
    pub details: Vec<EvalDetail>,
}

/// A single evaluation criterion result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalDetail {
    pub criterion: String,
    pub score: f64,
    pub passed: bool,
    pub message: String,
}

/// Trait for pluggable task evaluation. Implement this to score model responses
/// against different criteria (keyword matching, JSON extraction, visual comparison, etc.).
pub trait TaskEvaluator: Send + Sync {
    /// Evaluate a model response and return a scored result.
    fn evaluate(&self, response: &str) -> EvalResult;
}

/// A benchmark task: messages to send + evaluator for the response.
pub struct BenchTask {
    /// Unique task identifier.
    pub id: String,
    /// Human-readable task description.
    pub description: String,
    /// Messages to send to the model.
    pub messages: Vec<Message>,
    /// Evaluator for scoring the response.
    pub evaluator: Box<dyn TaskEvaluator>,
}

impl fmt::Debug for BenchTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BenchTask")
            .field("id", &self.id)
            .field("description", &self.description)
            .field("messages_count", &self.messages.len())
            .finish()
    }
}

/// Result from a single stream execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamResult {
    /// Task that was executed.
    pub task_id: String,
    /// Which stream (0..max_concurrent) executed this task.
    pub stream_id: usize,
    /// Whether the task completed successfully (no errors).
    pub success: bool,
    /// Whether the HTTP request completed AND returned a well-formed, parseable
    /// success response. Distinct from `success` (which is eval.passed): a
    /// response can transport fine yet fail evaluation. Latency stats use this.
    pub transport_succeeded: bool,
    /// Raw model response text.
    pub response: String,
    /// Tokens used (from usage field in API response).
    pub prompt_tokens: u64,
    /// Completion tokens generated.
    pub completion_tokens: u64,
    /// Wall-clock latency in milliseconds.
    pub latency_ms: u64,
    /// Evaluation score (if evaluation succeeded).
    pub eval: Option<EvalResult>,
    /// Error message (if the request failed).
    pub error: Option<String>,
}

// --- Built-in evaluators ---

/// Evaluator that checks for keyword presence in the response.
pub struct KeywordEvaluator {
    pub keywords: Vec<String>,
    pub threshold: f64,
}

impl KeywordEvaluator {
    pub fn new(keywords: Vec<String>) -> Self {
        Self {
            keywords,
            threshold: 0.5,
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }
}

impl TaskEvaluator for KeywordEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        let lower = response.to_lowercase();
        let mut details = Vec::new();
        let mut hits = 0;

        for kw in &self.keywords {
            let found = lower.contains(&kw.to_lowercase());
            if found {
                hits += 1;
            }
            details.push(EvalDetail {
                criterion: format!("keyword:{kw}"),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    format!("Found '{kw}'")
                } else {
                    format!("Missing '{kw}'")
                },
            });
        }

        let score = if self.keywords.is_empty() {
            1.0
        } else {
            hits as f64 / self.keywords.len() as f64
        };

        EvalResult {
            score,
            passed: score >= self.threshold,
            details,
        }
    }
}

/// Evaluator that checks for valid JSON in the response and optionally
/// validates specific fields.
pub struct JsonEvaluator {
    /// Fields that must be present in the JSON response.
    pub required_fields: Vec<String>,
}

impl JsonEvaluator {
    pub fn new(required_fields: Vec<String>) -> Self {
        Self { required_fields }
    }
}

impl TaskEvaluator for JsonEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        // Try to extract JSON from the response (may be wrapped in markdown fences)
        let json_str = extract_json_block(response);

        let parsed = match serde_json::from_str::<serde_json::Value>(json_str) {
            Ok(v) => v,
            Err(e) => {
                return EvalResult {
                    score: 0.0,
                    passed: false,
                    details: vec![EvalDetail {
                        criterion: "valid_json".into(),
                        score: 0.0,
                        passed: false,
                        message: format!("Invalid JSON: {e}"),
                    }],
                };
            }
        };

        let mut details = vec![EvalDetail {
            criterion: "valid_json".into(),
            score: 1.0,
            passed: true,
            message: "Valid JSON".into(),
        }];

        let mut hits = 1; // valid JSON counts
        let total = 1 + self.required_fields.len();

        for field in &self.required_fields {
            let found = parsed.get(field).is_some();
            if found {
                hits += 1;
            }
            details.push(EvalDetail {
                criterion: format!("field:{field}"),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    format!("Field '{field}' present")
                } else {
                    format!("Field '{field}' missing")
                },
            });
        }

        let score = hits as f64 / total as f64;
        EvalResult {
            score,
            passed: score >= 0.5,
            details,
        }
    }
}

/// Evaluator that always passes — for throughput/latency-only benchmarks.
pub struct NoopEvaluator;

impl TaskEvaluator for NoopEvaluator {
    fn evaluate(&self, _response: &str) -> EvalResult {
        EvalResult {
            score: 1.0,
            passed: true,
            details: vec![],
        }
    }
}

/// Extract a JSON block from a response that may contain markdown fences.
fn extract_json_block(text: &str) -> &str {
    // Try ```json ... ``` first
    if let Some(start) = text.find("```json") {
        let json_start = start + 7;
        if let Some(end) = text[json_start..].find("```") {
            return text[json_start..json_start + end].trim();
        }
    }
    // Try ``` ... ```
    if let Some(start) = text.find("```") {
        let inner_start = start + 3;
        if let Some(end) = text[inner_start..].find("```") {
            let block = text[inner_start..inner_start + end].trim();
            // Skip language identifier on first line
            if let Some(newline) = block.find('\n') {
                let first_line = &block[..newline];
                if !first_line.starts_with('{') && !first_line.starts_with('[') {
                    return block[newline + 1..].trim();
                }
            }
            return block;
        }
    }
    // Try raw JSON
    text.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_evaluator_all_present() {
        let eval = KeywordEvaluator::new(vec!["hello".into(), "world".into()]);
        let result = eval.evaluate("Hello, World!");
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
        assert_eq!(result.details.len(), 2);
    }

    #[test]
    fn test_keyword_evaluator_partial() {
        let eval = KeywordEvaluator::new(vec!["hello".into(), "missing".into()]);
        let result = eval.evaluate("Hello!");
        assert!((result.score - 0.5).abs() < f64::EPSILON);
        assert!(result.passed); // 0.5 >= default threshold 0.5
    }

    #[test]
    fn test_keyword_evaluator_none() {
        let eval = KeywordEvaluator::new(vec!["missing".into()]).with_threshold(0.5);
        let result = eval.evaluate("nothing here");
        assert_eq!(result.score, 0.0);
        assert!(!result.passed);
    }

    #[test]
    fn test_json_evaluator_valid() {
        let eval = JsonEvaluator::new(vec!["name".into(), "value".into()]);
        let result = eval.evaluate(r#"{"name": "test", "value": 42}"#);
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
    }

    #[test]
    fn test_json_evaluator_markdown_fence() {
        let eval = JsonEvaluator::new(vec!["name".into()]);
        let result = eval.evaluate("Here is the result:\n```json\n{\"name\": \"test\"}\n```\n");
        assert!(result.passed);
    }

    #[test]
    fn test_json_evaluator_invalid() {
        let eval = JsonEvaluator::new(vec![]);
        let result = eval.evaluate("not json at all");
        assert_eq!(result.score, 0.0);
        assert!(!result.passed);
    }

    #[test]
    fn test_noop_evaluator() {
        let eval = NoopEvaluator;
        let result = eval.evaluate("anything");
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
    }

    #[test]
    fn test_extract_json_block() {
        assert_eq!(extract_json_block(r#"{"a":1}"#), r#"{"a":1}"#);
        assert_eq!(extract_json_block("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
        assert_eq!(
            extract_json_block("Some text\n```\n{\"a\":1}\n```\nmore"),
            r#"{"a":1}"#
        );
    }
}
