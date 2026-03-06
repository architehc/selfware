//! L3: Architecture Diagram to Code Mapping (Hard)
//!
//! Input: ASCII/rendered architecture diagrams + code screenshots.
//! Tasks: Identify components, trace data flow, map diagram boxes to Rust modules.
//! Scoring: Component identification accuracy, relationship mapping.
//! Pass threshold: 60% component mapping accuracy.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.60;

/// Level 3: Architecture Diagram to Code Mapping.
pub struct L3Architecture {
    fixtures_dir: PathBuf,
}

impl Default for L3Architecture {
    fn default() -> Self {
        Self::new()
    }
}

impl L3Architecture {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/l3_architecture"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for L3Architecture {
    fn name(&self) -> &str {
        "L3 Architecture"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::Hard
    }

    fn description(&self) -> &str {
        "Architecture diagram comprehension: identify components from diagrams, \
         trace data flow between modules, and map diagram boxes to Rust module paths."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "l3_evolution_engine".into(),
                description: "Map the evolution engine architecture diagram to code modules".into(),
                image_path: self.fixtures_dir.join("evolution_diagram.png"),
                prompt: "This diagram shows the architecture of an evolution engine. \
                         Identify each component box and map it to a Rust module path. \
                         Respond with JSON: {\"components\": [{\"name\": \"<box label>\", \
                         \"module\": \"<rust module path>\", \"role\": \"<brief description>\"}], \
                         \"data_flow\": [{\"from\": \"<component>\", \"to\": \"<component>\", \
                         \"data\": \"<what flows>\"}]}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "daemon".into(),
                    "sandbox".into(),
                    "fitness".into(),
                    "mutation".into(),
                    "tournament".into(),
                ]),
            },
            BenchScenario {
                id: "l3_agent_pipeline".into(),
                description: "Trace the agent pipeline from diagram to implementation".into(),
                image_path: self.fixtures_dir.join("agent_pipeline.png"),
                prompt: "This diagram shows an AI agent's execution pipeline. \
                         Identify the stages and their corresponding Rust modules. \
                         Respond with JSON: {\"stages\": [{\"name\": \"<stage>\", \
                         \"module\": \"<path>\", \"inputs\": [\"<input>\"], \
                         \"outputs\": [\"<output>\"]}]}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "agent".into(),
                    "tool".into(),
                    "parser".into(),
                    "context".into(),
                ]),
            },
            BenchScenario {
                id: "l3_safety_layers".into(),
                description: "Identify the layered safety architecture from a diagram".into(),
                image_path: self.fixtures_dir.join("safety_layers.png"),
                prompt: "This diagram shows a multi-layered safety architecture. \
                         List each layer from outermost to innermost and describe \
                         what it validates. Respond with JSON: \
                         {\"layers\": [{\"name\": \"<layer>\", \"validates\": \"<what>\", \
                         \"module\": \"<path>\"}]}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "safety".into(),
                    "validation".into(),
                    "path".into(),
                    "command".into(),
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
            ExpectedAnswer::KeyValuePairs(pairs) => {
                let mut total = 0.0;
                let mut details = Vec::new();
                for (key, value) in pairs {
                    let score = if response.to_lowercase().contains(&key.to_lowercase()) {
                        scoring::keyword_overlap_score(response, value)
                    } else {
                        0.0
                    };
                    details.push((key.clone(), score));
                    total += score;
                }
                let acc = if pairs.is_empty() {
                    1.0
                } else {
                    total / pairs.len() as f64
                };
                (acc, details)
            }
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
    fn test_l3_metadata() {
        let level = L3Architecture::new();
        assert_eq!(level.name(), "L3 Architecture");
        assert_eq!(level.difficulty(), Difficulty::Hard);
    }

    #[test]
    fn test_l3_scenarios_count() {
        let level = L3Architecture::new();
        assert_eq!(level.scenarios().len(), 3);
    }

    #[test]
    fn test_l3_evaluate_all_keywords() {
        let level = L3Architecture::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec![
                "daemon".into(),
                "sandbox".into(),
                "fitness".into(),
            ]),
        };
        let response = "The daemon orchestrates sandbox evaluation using fitness metrics";
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
        assert_eq!(score.rating, Rating::Bloom);
    }
}
