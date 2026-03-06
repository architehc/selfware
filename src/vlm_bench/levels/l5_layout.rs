//! L5: Visual Mockup to TUI Layout (Extreme)
//!
//! Input: Hand-drawn or ASCII mockup of a TUI layout.
//! Tasks: Generate ratatui Rust code, identify widget types, specify constraints.
//! Scoring: Widget type identification, layout constraint accuracy, code compilability.
//! Pass threshold: 40% structural accuracy.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.40;

/// Level 5: Visual Mockup to TUI Layout.
pub struct L5Layout {
    fixtures_dir: PathBuf,
}

impl Default for L5Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl L5Layout {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/l5_layout"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for L5Layout {
    fn name(&self) -> &str {
        "L5 Layout"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::Extreme
    }

    fn description(&self) -> &str {
        "Visual mockup to code translation: identify widget types from ASCII/visual mockups, \
         generate ratatui layout code, specify correct constraint percentages."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "l5_simple_split".into(),
                description: "Translate a simple horizontal split layout to ratatui code".into(),
                image_path: self.fixtures_dir.join("simple_split.png"),
                prompt: "This mockup shows a terminal UI with a horizontal split layout. \
                         Identify the widget types and generate the ratatui layout code. \
                         Respond with JSON: {\"widgets\": [{\"type\": \"<widget type>\", \
                         \"position\": \"<left|right|top|bottom>\", \
                         \"constraint\": \"<Percentage(N) or Min(N)>\"}], \
                         \"layout_direction\": \"<Horizontal|Vertical>\", \
                         \"code_snippet\": \"<ratatui layout code>\"}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "layout".into(),
                    "horizontal".into(),
                    "constraint".into(),
                    "percentage".into(),
                ]),
            },
            BenchScenario {
                id: "l5_dashboard_grid".into(),
                description: "Translate a grid-based dashboard mockup to code".into(),
                image_path: self.fixtures_dir.join("dashboard_grid.png"),
                prompt: "This mockup shows a dashboard with a grid layout: \
                         header bar at top, sidebar on left, main content area, \
                         and status bar at bottom. Describe the layout hierarchy \
                         and widget types. Respond with JSON: \
                         {\"layout_tree\": {\"direction\": \"Vertical\", \
                         \"children\": [...]}, \
                         \"widget_types\": [\"<type>\", ...]}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "vertical".into(),
                    "horizontal".into(),
                    "header".into(),
                    "sidebar".into(),
                ]),
            },
            BenchScenario {
                id: "l5_complex_nested".into(),
                description: "Translate a complex nested layout with multiple widget types".into(),
                image_path: self.fixtures_dir.join("complex_nested.png"),
                prompt: "This mockup shows a complex TUI with nested layouts: \
                         tab bar, split panes with different widget types (list, chart, text), \
                         and a floating popup. Describe the full layout structure and \
                         identify all widget types. Respond with JSON: \
                         {\"widgets\": [{\"type\": \"<type>\", \"name\": \"<name>\"}], \
                         \"nesting_depth\": <N>, \"has_popup\": true/false, \
                         \"code_outline\": \"<pseudocode or ratatui code>\"}".into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "tab".into(),
                    "list".into(),
                    "chart".into(),
                    "popup".into(),
                    "nested".into(),
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
    fn test_l5_metadata() {
        let level = L5Layout::new();
        assert_eq!(level.name(), "L5 Layout");
        assert_eq!(level.difficulty(), Difficulty::Extreme);
    }

    #[test]
    fn test_l5_scenarios_count() {
        let level = L5Layout::new();
        assert_eq!(level.scenarios().len(), 3);
    }

    #[test]
    fn test_l5_evaluate_partial() {
        let level = L5Layout::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec![
                "layout".into(),
                "horizontal".into(),
                "constraint".into(),
                "percentage".into(),
            ]),
        };
        // Only 2/4 keywords
        let response = "The layout uses a horizontal split";
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 0.5).abs() < f64::EPSILON);
    }
}
