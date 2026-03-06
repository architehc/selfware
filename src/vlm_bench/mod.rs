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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_ordering() {
        assert!(Difficulty::Easy < Difficulty::Medium);
        assert!(Difficulty::Medium < Difficulty::Hard);
        assert!(Difficulty::Hard < Difficulty::VeryHard);
        assert!(Difficulty::VeryHard < Difficulty::Extreme);
        assert!(Difficulty::Extreme < Difficulty::Mega);
    }

    #[test]
    fn test_difficulty_display() {
        assert_eq!(format!("{}", Difficulty::Easy), "Easy");
        assert_eq!(format!("{}", Difficulty::VeryHard), "Very Hard");
        assert_eq!(format!("{}", Difficulty::Mega), "Mega");
    }

    #[test]
    fn test_bench_scenario_serde_roundtrip() {
        let scenario = BenchScenario {
            id: "test_01".into(),
            description: "Test scenario".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "What do you see?".into(),
            expected: ExpectedAnswer::Keywords(vec!["dashboard".into(), "panel".into()]),
        };
        let json = serde_json::to_string(&scenario).unwrap();
        let parsed: BenchScenario = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test_01");
        assert_eq!(parsed.prompt, "What do you see?");
    }

    #[test]
    fn test_expected_answer_variants() {
        let kw = ExpectedAnswer::Keywords(vec!["a".into()]);
        let json_str = serde_json::to_string(&kw).unwrap();
        assert!(json_str.contains("Keywords"));

        let jf = ExpectedAnswer::JsonFields(serde_json::json!({"panel": "dashboard"}));
        let json_str = serde_json::to_string(&jf).unwrap();
        assert!(json_str.contains("JsonFields"));

        let vs = ExpectedAnswer::VisualScores(vec![80.0, 70.0, 90.0]);
        let json_str = serde_json::to_string(&vs).unwrap();
        assert!(json_str.contains("VisualScores"));
    }
}
