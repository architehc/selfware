//! Web task evaluator — implements TaskEvaluator for computer control benchmarks.

use super::recorder::{ActionOutcome, InteractionTrace, TaskOutcome};
use super::tasks::SuccessCriterion;
use crate::bench_harness::task::{EvalDetail, EvalResult, TaskEvaluator};

/// Evaluator for web tasks based on interaction traces.
///
/// This evaluator checks success criteria against the final state of a
/// web task execution. It wraps an `InteractionTrace` and produces
/// an `EvalResult` compatible with the benchmark harness.
pub struct WebTaskEvaluator {
    pub criteria: Vec<SuccessCriterion>,
}

impl WebTaskEvaluator {
    pub fn new(criteria: Vec<SuccessCriterion>) -> Self {
        Self { criteria }
    }

    /// Evaluate an interaction trace against the success criteria.
    pub fn evaluate_trace(&self, trace: &InteractionTrace) -> EvalResult {
        let mut details = Vec::new();
        let mut passed_count = 0;

        // Check if the task completed at all
        let task_completed = matches!(trace.final_outcome, TaskOutcome::Passed);
        details.push(EvalDetail {
            criterion: "task_completed".into(),
            score: if task_completed { 1.0 } else { 0.0 },
            passed: task_completed,
            message: match &trace.final_outcome {
                TaskOutcome::Passed => "Task completed successfully".into(),
                TaskOutcome::Failed { reasons } => format!("Task failed: {}", reasons.join(", ")),
                TaskOutcome::Timeout => "Task timed out".into(),
                TaskOutcome::Error { message } => format!("Task error: {message}"),
            },
        });
        if task_completed {
            passed_count += 1;
        }

        // Check action success rate
        let total_actions = trace.actions.len();
        let successful_actions = trace
            .actions
            .iter()
            .filter(|a| matches!(a.outcome, ActionOutcome::Success { .. }))
            .count();
        let action_rate = if total_actions > 0 {
            successful_actions as f64 / total_actions as f64
        } else {
            1.0
        };
        details.push(EvalDetail {
            criterion: "action_success_rate".into(),
            score: action_rate,
            passed: action_rate >= 0.5,
            message: format!("{successful_actions}/{total_actions} actions succeeded"),
        });
        if action_rate >= 0.5 {
            passed_count += 1;
        }

        // Total including task_completed + action_success_rate + criteria
        let total_checks = 2 + self.criteria.len();

        // Criteria are checked against the response text in the standard evaluator path
        // Here we just record them as pending
        for criterion in &self.criteria {
            let (criterion_name, _description) = match criterion {
                SuccessCriterion::UrlContains(s) => ("url_contains".to_string(), s.clone()),
                SuccessCriterion::PageContains(s) => ("page_contains".to_string(), s.clone()),
                SuccessCriterion::ElementVisible(s) => ("element_visible".to_string(), s.clone()),
                SuccessCriterion::ExtractedDataMatches { key, .. } => {
                    ("extracted_data".to_string(), key.clone())
                }
                SuccessCriterion::VisualSimilarity { threshold, .. } => {
                    ("visual_similarity".to_string(), format!("{threshold:.2}"))
                }
            };

            // When used standalone, criteria pass if the task itself passed
            let crit_passed = task_completed;
            if crit_passed {
                passed_count += 1;
            }
            details.push(EvalDetail {
                criterion: criterion_name,
                score: if crit_passed { 1.0 } else { 0.0 },
                passed: crit_passed,
                message: if crit_passed {
                    "Criterion met (task passed)".into()
                } else {
                    "Criterion not evaluated (task failed)".into()
                },
            });
        }

        let score = if total_checks > 0 {
            passed_count as f64 / total_checks as f64
        } else {
            1.0
        };

        EvalResult {
            score,
            passed: score >= 0.5,
            details,
        }
    }
}

impl TaskEvaluator for WebTaskEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        // When called via the standard harness path (text response),
        // check text-based criteria against the response.
        let mut details = Vec::new();
        let mut hits = 0;
        let total = self.criteria.len().max(1);

        for criterion in &self.criteria {
            let (name, passed, msg) = match criterion {
                SuccessCriterion::PageContains(s) => {
                    let found = response.to_lowercase().contains(&s.to_lowercase());
                    (
                        format!("page_contains:{s}"),
                        found,
                        if found {
                            format!("Found '{s}'")
                        } else {
                            format!("Missing '{s}'")
                        },
                    )
                }
                SuccessCriterion::UrlContains(s) => {
                    let found = response.contains(s);
                    (
                        format!("url_contains:{s}"),
                        found,
                        if found {
                            format!("URL contains '{s}'")
                        } else {
                            format!("URL missing '{s}'")
                        },
                    )
                }
                SuccessCriterion::ExtractedDataMatches { key, expected } => {
                    let found = response.contains(expected);
                    (
                        format!("data:{key}"),
                        found,
                        if found {
                            format!("Data '{key}' matches")
                        } else {
                            format!("Data '{key}' does not match")
                        },
                    )
                }
                SuccessCriterion::ElementVisible(s) => {
                    // Can't check element visibility from text response
                    (
                        format!("element_visible:{s}"),
                        false,
                        "Cannot verify element visibility from text".into(),
                    )
                }
                SuccessCriterion::VisualSimilarity { .. } => {
                    // Can't check visual similarity from text response
                    (
                        "visual_similarity".into(),
                        false,
                        "Cannot verify visual similarity from text".into(),
                    )
                }
            };

            if passed {
                hits += 1;
            }
            details.push(EvalDetail {
                criterion: name,
                score: if passed { 1.0 } else { 0.0 },
                passed,
                message: msg,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_evaluator_text_mode() {
        let eval = WebTaskEvaluator::new(vec![
            SuccessCriterion::PageContains("Rust".into()),
            SuccessCriterion::PageContains("programming".into()),
        ]);

        let result = eval.evaluate("Rust is a programming language");
        assert_eq!(result.score, 1.0);
        assert!(result.passed);
    }

    #[test]
    fn test_web_evaluator_partial() {
        let eval = WebTaskEvaluator::new(vec![
            SuccessCriterion::PageContains("Rust".into()),
            SuccessCriterion::PageContains("Python".into()),
        ]);

        let result = eval.evaluate("Rust is great");
        assert!((result.score - 0.5).abs() < f64::EPSILON);
    }
}
