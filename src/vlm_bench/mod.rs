//! VLM Benchmark Suite for evaluating visual understanding capabilities.
//!
//! Progressively harder visual reasoning tasks, from basic TUI state recognition
//! to full visual-driven evolution workflows. Designed for local VLM evaluation
//! (e.g., Qwen 3.5 9B on LM Studio).

#![allow(dead_code, unused_imports, unused_variables)]

pub mod config;
pub mod fixtures;
pub mod levels;
pub mod report;
pub mod runner;
pub mod scoring;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Difficulty tiers for benchmark levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    VeryHard,
    Extreme,
    Mega,
}

impl std::fmt::Display for Difficulty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Easy => write!(f, "Easy"),
            Self::Medium => write!(f, "Medium"),
            Self::Hard => write!(f, "Hard"),
            Self::VeryHard => write!(f, "Very Hard"),
            Self::Extreme => write!(f, "Extreme"),
            Self::Mega => write!(f, "Mega"),
        }
    }
}

/// A single benchmark scenario: image + prompt + expected answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchScenario {
    /// Unique identifier for this scenario.
    pub id: String,
    /// Human-readable description.
    pub description: String,
    /// Path to the input image (PNG).
    pub image_path: PathBuf,
    /// Prompt sent alongside the image to the VLM.
    pub prompt: String,
    /// Expected answer keywords or structured data for evaluation.
    pub expected: ExpectedAnswer,
}

/// What we expect from the VLM's response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExpectedAnswer {
    /// Exact keyword matches (case-insensitive).
    Keywords(Vec<String>),
    /// Structured JSON fields that must be present with correct values.
    JsonFields(serde_json::Value),
    /// A set of key-value pairs where keys must appear and values are scored via BM25.
    KeyValuePairs(Vec<(String, String)>),
    /// Ground-truth visual scores for correlation comparison.
    VisualScores(Vec<f64>),
}

/// Trait implemented by each benchmark level.
#[async_trait]
pub trait VlmBenchLevel: Send + Sync {
    /// Short name for this level (e.g., "L1 TUI State").
    fn name(&self) -> &str;

    /// Difficulty tier.
    fn difficulty(&self) -> Difficulty;

    /// Human-readable description of what this level tests.
    fn description(&self) -> &str;

    /// Generate benchmark scenarios (image + prompt + expected answer).
    fn scenarios(&self) -> Vec<BenchScenario>;

    /// Evaluate a VLM response against the expected answer.
    fn evaluate(&self, scenario: &BenchScenario, response: &str) -> scoring::LevelScore;
}

/// Standard keyword / JSON-field scoring shared by the bench levels. The body
/// was previously copied verbatim into each level's `evaluate`; the only thing
/// that varied was the level's own `PASS_THRESHOLD`, now passed as a parameter
/// so behavior is preserved while the duplication is removed.
pub(crate) fn score_response(
    scenario: &BenchScenario,
    response: &str,
    pass_threshold: f64,
) -> scoring::LevelScore {
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
    let rating = scoring::Rating::from_accuracy(accuracy, pass_threshold);
    scoring::LevelScore {
        accuracy,
        detail_scores: details,
        response_tokens: 0,
        latency_ms: 0,
        rating,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/vlm_bench/mod_test.rs"]
mod tests;
