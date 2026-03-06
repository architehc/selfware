//! L1: Terminal State & Aesthetic Recognition (Easy)
//!
//! Input: Screenshots of selfware's ratatui TUI in various states.
//! Tasks: Identify active panel, read status bar text, detect color theme,
//!        count visible widgets.
//! Scoring: Keyword match, structured JSON parse.
//! Pass threshold: 80% keyword accuracy.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.80;

/// Level 1: Terminal State & Aesthetic Recognition.
pub struct L1TuiState {
    fixtures_dir: PathBuf,
}

impl Default for L1TuiState {
    fn default() -> Self {
        Self::new()
    }
}

impl L1TuiState {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/l1_tui_state"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for L1TuiState {
    fn name(&self) -> &str {
        "L1 TUI State"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::Easy
    }

    fn description(&self) -> &str {
        "Terminal state recognition: identify panels, read status text, \
         detect color themes, and count visible widgets in TUI screenshots."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "l1_dashboard_normal".into(),
                description: "Identify the active panel and status in a normal dashboard view"
                    .into(),
                image_path: self.fixtures_dir.join("dashboard_normal.png"),
                prompt: "Analyze this terminal UI screenshot. Respond with a JSON object containing: \
                         {\"active_panel\": \"<panel name>\", \"status\": \"<status bar text>\", \
                         \"widget_count\": <number of visible widgets>, \"theme\": \"<dark or light>\"}".into(),
                expected: ExpectedAnswer::JsonFields(serde_json::json!({
                    "active_panel": "dashboard",
                    "theme": "dark"
                })),
            },
            BenchScenario {
                id: "l1_dashboard_error".into(),
                description: "Detect error state in a dashboard with error indicators".into(),
                image_path: self.fixtures_dir.join("dashboard_error.png"),
                prompt: "Analyze this terminal UI screenshot showing an error state. \
                         What error or warning is visible? Respond with JSON: \
                         {\"has_error\": true/false, \"error_type\": \"<type>\", \
                         \"active_panel\": \"<panel name>\"}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "error".into(),
                    "true".into(),
                ]),
            },
            BenchScenario {
                id: "l1_help_panel".into(),
                description: "Read content from a help/documentation panel".into(),
                image_path: self.fixtures_dir.join("help_panel.png"),
                prompt: "This screenshot shows a help panel in a terminal UI. \
                         List the keyboard shortcuts or commands visible. \
                         Respond with JSON: {\"panel_type\": \"help\", \
                         \"shortcuts\": [\"<shortcut1>\", ...]}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "help".into(),
                    "shortcut".into(),
                ]),
            },
            BenchScenario {
                id: "l1_loading_state".into(),
                description: "Identify a loading/spinner state".into(),
                image_path: self.fixtures_dir.join("loading_state.png"),
                prompt: "Analyze this terminal UI screenshot. Is the interface in a loading state? \
                         What visual indicators suggest loading? Respond with JSON: \
                         {\"is_loading\": true/false, \"indicators\": [\"<indicator>\", ...], \
                         \"active_panel\": \"<panel name>\"}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "loading".into(),
                    "true".into(),
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
    fn test_l1_metadata() {
        let level = L1TuiState::new();
        assert_eq!(level.name(), "L1 TUI State");
        assert_eq!(level.difficulty(), Difficulty::Easy);
        assert!(!level.description().is_empty());
    }

    #[test]
    fn test_l1_scenarios_count() {
        let level = L1TuiState::new();
        assert_eq!(level.scenarios().len(), 4);
    }

    #[test]
    fn test_l1_scenarios_unique_ids() {
        let level = L1TuiState::new();
        let scenarios = level.scenarios();
        let mut ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), scenarios.len());
    }

    #[test]
    fn test_l1_evaluate_keywords_perfect() {
        let level = L1TuiState::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec!["error".into(), "true".into()]),
        };
        let score = level.evaluate(&scenario, "There is an error, has_error: true");
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
        assert_eq!(score.rating, Rating::Bloom);
    }

    #[test]
    fn test_l1_evaluate_keywords_none() {
        let level = L1TuiState::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec!["error".into(), "true".into()]),
        };
        let score = level.evaluate(&scenario, "Everything is fine");
        assert!(score.accuracy < 0.5);
        assert_eq!(score.rating, Rating::Frost);
    }

    #[test]
    fn test_l1_evaluate_json_fields() {
        let level = L1TuiState::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::JsonFields(serde_json::json!({
                "active_panel": "dashboard",
                "theme": "dark"
            })),
        };
        let response = r#"{"active_panel": "dashboard", "theme": "dark", "widget_count": 5}"#;
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
        assert_eq!(score.rating, Rating::Bloom);
    }
}
