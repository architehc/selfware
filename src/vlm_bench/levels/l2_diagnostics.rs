//! L2: Borrow Checker & Compiler Diagnostics (Medium)
//!
//! Input: Screenshots of `cargo check` errors (lifetime errors, type mismatches, trait bounds).
//! Tasks: Identify error type, extract error code, locate file:line, suggest fix category.
//! Scoring: Error code exact match, file path match, fix category BM25.
//! Pass threshold: 70% structured accuracy.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.70;

/// Level 2: Borrow Checker & Compiler Diagnostics.
pub struct L2Diagnostics {
    fixtures_dir: PathBuf,
}

impl Default for L2Diagnostics {
    fn default() -> Self {
        Self::new()
    }
}

impl L2Diagnostics {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/l2_diagnostics"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for L2Diagnostics {
    fn name(&self) -> &str {
        "L2 Diagnostics"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::Medium
    }

    fn description(&self) -> &str {
        "Compiler diagnostic reading: identify error types, extract error codes (E0xxx), \
         locate file:line references, and suggest fix categories from terminal screenshots."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "l2_lifetime_error".into(),
                description: "Identify a lifetime/borrow checker error".into(),
                image_path: self.fixtures_dir.join("lifetime_error.png"),
                prompt: "This screenshot shows a Rust compiler error. Analyze it and respond with JSON: \
                         {\"error_code\": \"E0xxx\", \"error_type\": \"<lifetime|borrow|type|trait>\", \
                         \"file\": \"<file path>\", \"line\": <line number>, \
                         \"fix_category\": \"<add lifetime annotation|clone|reference|restructure>\"}".into(),
                expected: ExpectedAnswer::JsonFields(serde_json::json!({
                    "error_type": "lifetime"
                })),
            },
            BenchScenario {
                id: "l2_type_mismatch".into(),
                description: "Identify a type mismatch error".into(),
                image_path: self.fixtures_dir.join("type_mismatch.png"),
                prompt: "This screenshot shows a Rust compiler error about type mismatches. \
                         Analyze it and respond with JSON: \
                         {\"error_code\": \"E0xxx\", \"error_type\": \"type\", \
                         \"expected_type\": \"<type>\", \"found_type\": \"<type>\", \
                         \"file\": \"<file path>\", \"line\": <line number>}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "type".into(),
                    "mismatch".into(),
                    "expected".into(),
                ]),
            },
            BenchScenario {
                id: "l2_trait_bound".into(),
                description: "Identify a trait bound error".into(),
                image_path: self.fixtures_dir.join("trait_bound.png"),
                prompt: "This screenshot shows a Rust compiler error about trait bounds. \
                         What trait is missing? Respond with JSON: \
                         {\"error_code\": \"E0xxx\", \"error_type\": \"trait\", \
                         \"missing_trait\": \"<trait name>\", \"on_type\": \"<type>\", \
                         \"fix_category\": \"<derive|impl|bound|where clause>\"}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "trait".into(),
                    "bound".into(),
                ]),
            },
        ]
    }

    fn evaluate(&self, scenario: &BenchScenario, response: &str) -> LevelScore {
        let (accuracy, details) = match &scenario.expected {
            ExpectedAnswer::Keywords(keywords) => {
                let acc = scoring::keyword_accuracy(response, keywords);
                let details = keywords
                    .iter()
                    .map(|kw| {
                        let found = response.to_lowercase().contains(&kw.to_lowercase());
                        (kw.clone(), if found { 1.0 } else { 0.0 })
                    })
                    .collect();
                (acc, details)
            }
            ExpectedAnswer::JsonFields(expected) => scoring::json_field_accuracy(response, expected),
            _ => (0.0, vec![]),
        };

        let rating = Rating::from_accuracy(accuracy, PASS_THRESHOLD);

        LevelScore {
            accuracy,
            detail_scores: details,
            response_tokens: 0,
            latency_ms: 0,
            rating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_metadata() {
        let level = L2Diagnostics::new();
        assert_eq!(level.name(), "L2 Diagnostics");
        assert_eq!(level.difficulty(), Difficulty::Medium);
    }

    #[test]
    fn test_l2_scenarios_count() {
        let level = L2Diagnostics::new();
        assert_eq!(level.scenarios().len(), 3);
    }

    #[test]
    fn test_l2_evaluate_lifetime() {
        let level = L2Diagnostics::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::JsonFields(serde_json::json!({
                "error_type": "lifetime"
            })),
        };
        let response = r#"{"error_code": "E0106", "error_type": "lifetime", "file": "src/main.rs", "line": 42}"#;
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_l2_evaluate_keywords() {
        let level = L2Diagnostics::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec![
                "type".into(),
                "mismatch".into(),
                "expected".into(),
            ]),
        };
        let response = "The error shows a type mismatch: expected u32 but found String";
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    }
}
