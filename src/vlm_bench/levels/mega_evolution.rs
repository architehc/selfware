//! Mega: Multi-Image Visual Evolution Pipeline
//!
//! Input: Sequence of 3-5 TUI screenshots showing progressive iterations.
//! Tasks: Score each iteration, identify changes, rate trajectory, predict next.
//! Scoring: Correlation between VLM scores and ground-truth human ratings.
//! Pass threshold: Correlation > 0.5.

use async_trait::async_trait;
use std::path::PathBuf;

use crate::vlm_bench::scoring::{self, LevelScore, Rating};
use crate::vlm_bench::{BenchScenario, Difficulty, ExpectedAnswer, VlmBenchLevel};

const PASS_THRESHOLD: f64 = 0.50;

/// Mega Level: Multi-Image Visual Evolution Pipeline.
pub struct MegaEvolution {
    fixtures_dir: PathBuf,
}

impl Default for MegaEvolution {
    fn default() -> Self {
        Self::new()
    }
}

impl MegaEvolution {
    pub fn new() -> Self {
        Self {
            fixtures_dir: PathBuf::from("vlm_fixtures/mega_evolution"),
        }
    }

    pub fn with_fixtures_dir(dir: PathBuf) -> Self {
        Self { fixtures_dir: dir }
    }
}

#[async_trait]
impl VlmBenchLevel for MegaEvolution {
    fn name(&self) -> &str {
        "Mega Evolution"
    }

    fn difficulty(&self) -> Difficulty {
        Difficulty::Mega
    }

    fn description(&self) -> &str {
        "Multi-image visual evolution: score iterative TUI improvements across \
         composition, hierarchy, readability, consistency, and accessibility dimensions. \
         Compare VLM ratings against ground-truth human scores."
    }

    fn scenarios(&self) -> Vec<BenchScenario> {
        vec![
            BenchScenario {
                id: "mega_iteration_scoring".into(),
                description: "Score a single TUI iteration on the 5 visual dimensions".into(),
                image_path: self.fixtures_dir.join("iteration_01.png"),
                prompt: "Score this terminal UI screenshot on five visual quality dimensions. \
                         Rate each from 0-100. Respond with ONLY a JSON object: \
                         {\"composition\": <0-100>, \"hierarchy\": <0-100>, \
                         \"readability\": <0-100>, \"consistency\": <0-100>, \
                         \"accessibility\": <0-100>, \
                         \"suggestions\": [\"<improvement 1>\", ...]}"
                    .into(),
                expected: ExpectedAnswer::VisualScores(vec![75.0, 70.0, 80.0, 72.0, 68.0]),
            },
            BenchScenario {
                id: "mega_progression_analysis".into(),
                description: "Analyze visual improvement between two iterations".into(),
                image_path: self.fixtures_dir.join("progression_pair.png"),
                prompt:
                    "These two screenshots show before (left) and after (right) of a TUI update. \
                         Score each version on composition, hierarchy, readability, consistency, \
                         and accessibility (0-100). Identify what changed and whether it improved. \
                         Respond with JSON: {\"before\": {\"composition\": <N>, ...}, \
                         \"after\": {\"composition\": <N>, ...}, \
                         \"improvements\": [\"<what improved>\"], \
                         \"regressions\": [\"<what got worse>\"], \
                         \"trajectory\": \"<improving|stable|declining>\"}"
                        .into(),
                expected: ExpectedAnswer::Keywords(vec![
                    "composition".into(),
                    "hierarchy".into(),
                    "readability".into(),
                    "improving".into(),
                ]),
            },
            BenchScenario {
                id: "mega_rating_prediction".into(),
                description: "Predict the GenerationRating for a visual iteration".into(),
                image_path: self.fixtures_dir.join("iteration_03.png"),
                prompt: "Based on this TUI screenshot, assign a garden-themed rating: \
                         BLOOM (excellent visual quality), GROW (good, improving), \
                         WILT (mediocre, needs work), or FROST (poor quality). \
                         Also score the five dimensions. Respond with JSON: \
                         {\"rating\": \"BLOOM|GROW|WILT|FROST\", \
                         \"composition\": <0-100>, \"hierarchy\": <0-100>, \
                         \"readability\": <0-100>, \"consistency\": <0-100>, \
                         \"accessibility\": <0-100>, \"justification\": \"<why>\"}"
                    .into(),
                expected: ExpectedAnswer::Keywords(vec!["bloom".into(), "grow".into()]),
            },
        ]
    }

    fn evaluate(&self, scenario: &BenchScenario, response: &str) -> LevelScore {
        match &scenario.expected {
            ExpectedAnswer::VisualScores(ground_truth) => {
                evaluate_visual_scores(response, ground_truth)
            }
            ExpectedAnswer::Keywords(keywords) => {
                let acc = scoring::keyword_accuracy(response, keywords);
                let details = keywords
                    .iter()
                    .map(|kw| {
                        let found = response.to_lowercase().contains(&kw.to_lowercase());
                        (kw.clone(), if found { 1.0 } else { 0.0 })
                    })
                    .collect();
                let rating = Rating::from_accuracy(acc, PASS_THRESHOLD);
                LevelScore {
                    accuracy: acc,
                    detail_scores: details,
                    response_tokens: 0,
                    latency_ms: 0,
                    rating,
                }
            }
            _ => LevelScore {
                accuracy: 0.0,
                detail_scores: vec![],
                response_tokens: 0,
                latency_ms: 0,
                rating: Rating::Frost,
            },
        }
    }
}

/// Evaluate VLM visual dimension scores against ground-truth.
///
/// Extracts the 5 dimension scores from the response JSON and computes
/// Pearson correlation with the ground-truth scores.
fn evaluate_visual_scores(response: &str, ground_truth: &[f64]) -> LevelScore {
    let dimensions = [
        "composition",
        "hierarchy",
        "readability",
        "consistency",
        "accessibility",
    ];

    // Try to parse scores from response
    let predicted = extract_dimension_scores(response, &dimensions);

    let mut details = Vec::new();
    let mut accuracy = 0.0;

    if predicted.len() == ground_truth.len() && !predicted.is_empty() {
        // Compute Pearson correlation
        let correlation = scoring::pearson_correlation(&predicted, ground_truth);
        // Normalize correlation from [-1, 1] to [0, 1]
        accuracy = (correlation + 1.0) / 2.0;

        for (i, dim) in dimensions.iter().enumerate() {
            if i < predicted.len() && i < ground_truth.len() {
                // Score based on how close the prediction is (within ±15 points)
                let diff = (predicted[i] - ground_truth[i]).abs();
                let dim_score = (1.0 - diff / 50.0).max(0.0);
                details.push((dim.to_string(), dim_score));
            }
        }
    } else {
        for dim in &dimensions {
            details.push((dim.to_string(), 0.0));
        }
    }

    let rating = Rating::from_accuracy(accuracy, PASS_THRESHOLD);

    LevelScore {
        accuracy,
        detail_scores: details,
        response_tokens: 0,
        latency_ms: 0,
        rating,
    }
}

/// Extract visual dimension scores from a VLM response.
fn extract_dimension_scores(response: &str, dimensions: &[&str]) -> Vec<f64> {
    // Try JSON parse first
    let trimmed = response.trim();
    let json_str = if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            &trimmed[start..=end]
        } else {
            return vec![];
        }
    } else {
        return vec![];
    };

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    dimensions
        .iter()
        .filter_map(|dim| parsed.get(dim).and_then(|v| v.as_f64()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mega_metadata() {
        let level = MegaEvolution::new();
        assert_eq!(level.name(), "Mega Evolution");
        assert_eq!(level.difficulty(), Difficulty::Mega);
    }

    #[test]
    fn test_mega_scenarios_count() {
        let level = MegaEvolution::new();
        assert_eq!(level.scenarios().len(), 3);
    }

    #[test]
    fn test_extract_dimension_scores_valid() {
        let response = r#"{"composition": 80, "hierarchy": 70, "readability": 90, "consistency": 75, "accessibility": 85}"#;
        let dims = [
            "composition",
            "hierarchy",
            "readability",
            "consistency",
            "accessibility",
        ];
        let scores = extract_dimension_scores(response, &dims);
        assert_eq!(scores.len(), 5);
        assert!((scores[0] - 80.0).abs() < f64::EPSILON);
        assert!((scores[2] - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_dimension_scores_with_text() {
        let response = r#"Here are my scores: {"composition": 60, "hierarchy": 55, "readability": 70, "consistency": 65, "accessibility": 50} That's my analysis."#;
        let dims = [
            "composition",
            "hierarchy",
            "readability",
            "consistency",
            "accessibility",
        ];
        let scores = extract_dimension_scores(response, &dims);
        assert_eq!(scores.len(), 5);
    }

    #[test]
    fn test_extract_dimension_scores_no_json() {
        let response = "The composition is good and hierarchy is clear";
        let dims = ["composition", "hierarchy"];
        let scores = extract_dimension_scores(response, &dims);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_evaluate_visual_scores_perfect_correlation() {
        let response = r#"{"composition": 75, "hierarchy": 70, "readability": 80, "consistency": 72, "accessibility": 68}"#;
        let ground_truth = vec![75.0, 70.0, 80.0, 72.0, 68.0];
        let score = evaluate_visual_scores(response, &ground_truth);
        // Perfect match should have high correlation
        assert!(score.accuracy > 0.9);
        assert_eq!(score.rating, Rating::Bloom);
    }

    #[test]
    fn test_evaluate_visual_scores_no_response() {
        let ground_truth = vec![75.0, 70.0, 80.0, 72.0, 68.0];
        let score = evaluate_visual_scores("No JSON here", &ground_truth);
        assert!((score.accuracy - 0.0).abs() < f64::EPSILON);
        assert_eq!(score.rating, Rating::Frost);
    }

    #[test]
    fn test_mega_evaluate_keywords() {
        let level = MegaEvolution::new();
        let scenario = BenchScenario {
            id: "test".into(),
            description: "test".into(),
            image_path: PathBuf::from("test.png"),
            prompt: "test".into(),
            expected: ExpectedAnswer::Keywords(vec![
                "composition".into(),
                "hierarchy".into(),
                "readability".into(),
                "improving".into(),
            ]),
        };
        let response = "The composition and hierarchy are strong, readability is improving";
        let score = level.evaluate(&scenario, response);
        assert!((score.accuracy - 1.0).abs() < f64::EPSILON);
    }
}
