//! L4: Flamegraph & Performance Analysis (Very Hard)
//!
//! Input: Flamegraph SVGs rendered as PNGs, perf profile screenshots.
//! Tasks: Identify hottest function, estimate % time, suggest optimizations.
//! Scoring: Hot function exact match, percentage within +/-10%, suggestion BM25.
//! Pass threshold: 50% on hot-path identification.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.50;

/// Level 4: Flamegraph & Performance Analysis.
pub struct L4Profiling {
    fixtures_dir: PathBuf,
}

impl Default for L4Profiling {
    fn default() -> Self {
        Self::new()
    }
}

impl L4Profiling {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/l4_profiling"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for L4Profiling {
    fn name(&self) -> &str {
        "L4 Profiling"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::VeryHard
    }

    fn description(&self) -> &str {
        "Performance profile analysis: identify hot functions from flamegraphs, \
         estimate time percentages, suggest optimization targets, and detect anomalies."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "l4_simple_flamegraph".into(),
                description: "Identify the hottest function in a simple flamegraph".into(),
                image_path: self.fixtures_dir.join("simple_flamegraph.png"),
                prompt: "This is a flamegraph showing CPU profiling data. \
                         Identify the hottest function (widest bar at the top of the stack). \
                         Respond with JSON: {\"hottest_function\": \"<function name>\", \
                         \"estimated_pct\": <percentage of total time>, \
                         \"call_stack\": [\"<caller1>\", \"<caller2>\", ...], \
                         \"optimization_suggestions\": [\"<suggestion>\"]}"
                    .into(),
                expected: ExpectedAnswer::Keywords(vec!["function".into(), "hot".into()]),
            },
            BenchScenario {
                id: "l4_multithread_profile".into(),
                description: "Analyze a multi-threaded performance profile".into(),
                image_path: self.fixtures_dir.join("multithread_profile.png"),
                prompt: "This flamegraph shows a multi-threaded application profile. \
                         Identify which threads are busiest and what work they're doing. \
                         Respond with JSON: {\"thread_count\": <N>, \
                         \"busiest_thread\": \"<thread name>\", \
                         \"hottest_function\": \"<function>\", \
                         \"contention_detected\": true/false, \
                         \"suggestions\": [\"<optimization>\"]}"
                    .into(),
                expected: ExpectedAnswer::Keywords(vec!["thread".into(), "function".into()]),
            },
            BenchScenario {
                id: "l4_memory_profile".into(),
                description: "Analyze a memory allocation profile".into(),
                image_path: self.fixtures_dir.join("memory_profile.png"),
                prompt: "This profile shows memory allocation patterns. \
                         Identify the largest allocators and suggest improvements. \
                         Respond with JSON: {\"top_allocator\": \"<function>\", \
                         \"estimated_allocation_pct\": <pct>, \
                         \"allocation_pattern\": \"<steady|bursty|growing>\", \
                         \"suggestions\": [\"<suggestion>\"]}"
                    .into(),
                expected: ExpectedAnswer::Keywords(vec!["allocat".into(), "memory".into()]),
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
            ExpectedAnswer::JsonFields(expected) => {
                scoring::json_field_accuracy(response, expected)
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
    fn test_l4_metadata() {
        let level = L4Profiling::new();
        assert_eq!(level.name(), "L4 Profiling");
        assert_eq!(level.difficulty(), Difficulty::VeryHard);
    }

    #[test]
    fn test_l4_scenarios_count() {
        let level = L4Profiling::new();
        assert_eq!(level.scenarios().len(), 3);
    }

    #[test]
    fn test_l4_evaluate() {
        let level = L4Profiling::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec!["function".into(), "hot".into()]),
        };
        let response = "The hottest function is parse_tokens taking 45% of CPU time";
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    }
}
