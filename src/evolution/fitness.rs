//! Fitness Function — Meta-SAB Scoring Engine
//!
//! Wraps the existing SAB (Selfware Agentic Benchmark) as a fitness function
//! for evolutionary evaluation. This module is PROTECTED — the evolution
//! daemon cannot modify it, preventing reward hacking.

use super::{FitnessMetrics, FitnessWeights, GenerationRating};
use crate::orchestration::visual_loop::CaptureMethod;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// SAB evaluation configuration
#[derive(Debug, Clone)]
pub struct SabConfig {
    /// Path to the SAB runner script
    pub runner_script: PathBuf,
    /// LLM endpoint for SAB evaluation
    pub endpoint: String,
    /// Model name
    pub model: String,
    /// Maximum parallel scenarios
    pub max_parallel: usize,
    /// Timeout per scenario
    pub scenario_timeout: Duration,
    /// Which scenarios to run (None = all 12)
    pub scenario_filter: Option<Vec<String>>,
}

impl Default for SabConfig {
    fn default() -> Self {
        Self {
            runner_script: PathBuf::from("system_tests/projecte2e/run_full_sab.sh"),
            endpoint: "http://localhost:8000/v1".to_string(),
            model: "Qwen/Qwen3-Coder-Next-FP8".to_string(),
            max_parallel: 6,
            scenario_timeout: Duration::from_secs(3600),
            scenario_filter: None,
        }
    }
}

/// Result of a full SAB evaluation
#[derive(Debug, Clone)]
pub struct SabResult {
    pub aggregate_score: f64,
    pub scenario_scores: Vec<ScenarioScore>,
    pub total_tokens_used: u64,
    pub wall_clock: Duration,
    pub rating: GenerationRating,
}

#[derive(Debug, Clone)]
pub struct ScenarioScore {
    pub name: String,
    pub difficulty: Difficulty,
    pub score: f64,
    pub tests_passed: bool,
    pub broken_tests_fixed: bool,
    pub clean_exit: bool,
    pub tokens_used: u64,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

/// A visual scenario for the Visual-SAB benchmark track.
///
/// Each scenario describes a visual design task, an optional golden reference
/// image, and a quality threshold.  During SAB evaluation the agent generates
/// visual output, a screenshot is captured, and the VLM critic scores it.
#[derive(Debug, Clone)]
pub struct VisualScenario {
    /// Human-readable scenario name (e.g. `"visual_landing_page"`).
    pub name: String,
    /// Description / prompt given to the agent.
    pub description: String,
    /// Optional golden reference image for comparison.
    pub reference_image: Option<PathBuf>,
    /// Minimum overall score (0.0–1.0) to pass.
    pub quality_threshold: f64,
    /// How to capture the agent's visual output.
    pub capture_method: CaptureMethod,
}

/// Built-in visual SAB scenario stubs.
///
/// These are placeholders — the actual prompt files and reference images live
/// in `system_tests/projecte2e/`.
pub fn visual_sab_scenarios() -> Vec<VisualScenario> {
    vec![
        VisualScenario {
            name: "visual_landing_page".into(),
            description: "Generate a responsive landing page with hero section, \
                          feature cards, and a call-to-action. Score visual quality."
                .into(),
            reference_image: None,
            quality_threshold: 0.7,
            capture_method: CaptureMethod::BrowserUrl("http://localhost:3000".into()),
        },
        VisualScenario {
            name: "visual_dashboard".into(),
            description: "Create a data dashboard with a chart, a stats bar, \
                          and a table. Score layout and readability."
                .into(),
            reference_image: None,
            quality_threshold: 0.7,
            capture_method: CaptureMethod::BrowserUrl("http://localhost:3000".into()),
        },
        VisualScenario {
            name: "visual_game_ui".into(),
            description: "Build a simple game HUD with health bar, score counter, \
                          mini-map, and inventory slots. Score composition and hierarchy."
                .into(),
            reference_image: None,
            quality_threshold: 0.65,
            capture_method: CaptureMethod::Screen,
        },
    ]
}

/// Run the full SAB benchmark and return structured results
pub fn run_sab(selfware_binary: &Path, config: &SabConfig) -> Result<SabResult, FitnessError> {
    let start = Instant::now();

    // Set up environment for SAB runner
    let output = Command::new("bash")
        .arg(&config.runner_script)
        .env("ENDPOINT", &config.endpoint)
        .env("MODEL", &config.model)
        .env("MAX_PARALLEL", config.max_parallel.to_string())
        .env(
            "SELFWARE_BINARY",
            selfware_binary.to_string_lossy().as_ref(),
        )
        .env("TIMEOUT", config.scenario_timeout.as_secs().to_string())
        .output()
        .map_err(|e| FitnessError::SabRunFailed(e.to_string()))?;

    let wall_clock = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(FitnessError::SabRunFailed(stderr.to_string()));
    }

    // Parse SAB output — the runner produces JSON reports
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_sab_output(&stdout, wall_clock)
}

/// Parse SAB runner output into structured results
fn parse_sab_output(output: &str, wall_clock: Duration) -> Result<SabResult, FitnessError> {
    // Look for the JSON report path in output
    let report_path = output
        .lines()
        .rev()
        .find(|l| l.contains("reports/") && l.contains(".json"))
        .map(|l| l.trim().to_string());

    let scenario_scores = if let Some(path) = report_path {
        parse_report_json(&path)?
    } else {
        // Fallback: parse structured text output
        parse_text_output(output)?
    };

    let aggregate = if scenario_scores.is_empty() {
        0.0
    } else {
        scenario_scores.iter().map(|s| s.score).sum::<f64>() / scenario_scores.len() as f64
    };

    let total_tokens: u64 = scenario_scores.iter().map(|s| s.tokens_used).sum();

    let rating = match aggregate as u32 {
        85..=100 => GenerationRating::Bloom,
        60..=84 => GenerationRating::Grow,
        30..=59 => GenerationRating::Wilt,
        _ => GenerationRating::Frost,
    };

    Ok(SabResult {
        aggregate_score: aggregate,
        scenario_scores,
        total_tokens_used: total_tokens,
        wall_clock,
        rating,
    })
}

fn parse_report_json(path: &str) -> Result<Vec<ScenarioScore>, FitnessError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| FitnessError::ReportParseFailed(e.to_string()))?;

    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| FitnessError::ReportParseFailed(e.to_string()))?;

    let scenarios = json["scenarios"]
        .as_array()
        .ok_or_else(|| FitnessError::ReportParseFailed("No scenarios array".into()))?;

    scenarios
        .iter()
        .map(|s| {
            Ok(ScenarioScore {
                name: s["name"].as_str().unwrap_or("unknown").to_string(),
                difficulty: match s["difficulty"].as_str().unwrap_or("medium") {
                    "easy" => Difficulty::Easy,
                    "medium" => Difficulty::Medium,
                    "hard" => Difficulty::Hard,
                    "expert" => Difficulty::Expert,
                    _ => Difficulty::Medium,
                },
                score: s["score"].as_f64().unwrap_or(0.0),
                tests_passed: s["tests_passed"].as_bool().unwrap_or(false),
                broken_tests_fixed: s["broken_tests_fixed"].as_bool().unwrap_or(false),
                clean_exit: s["clean_exit"].as_bool().unwrap_or(false),
                tokens_used: s["tokens_used"].as_u64().unwrap_or(0),
                duration: Duration::from_secs(s["duration_secs"].as_u64().unwrap_or(0)),
            })
        })
        .collect()
}

fn parse_text_output(output: &str) -> Result<Vec<ScenarioScore>, FitnessError> {
    // Minimal text parser for when JSON isn't available
    let mut scores = Vec::new();
    for line in output.lines() {
        // Look for lines like: "easy_calculator: 100/100 BLOOM"
        if let Some((name, rest)) = line.split_once(':') {
            let name = name.trim();
            if let Some(score_str) = rest.split('/').next() {
                if let Ok(score) = score_str.trim().parse::<f64>() {
                    scores.push(ScenarioScore {
                        name: name.to_string(),
                        difficulty: infer_difficulty(name),
                        score,
                        tests_passed: score >= 70.0,
                        broken_tests_fixed: score >= 90.0,
                        clean_exit: score >= 10.0,
                        tokens_used: 0, // Unknown from text output
                        duration: Duration::ZERO,
                    });
                }
            }
        }
    }
    Ok(scores)
}

fn infer_difficulty(name: &str) -> Difficulty {
    if name.starts_with("easy_") {
        Difficulty::Easy
    } else if name.starts_with("medium_")
        || name.starts_with("testgen_")
        || name.starts_with("refactor_")
    {
        Difficulty::Medium
    } else if name.starts_with("expert_") {
        Difficulty::Expert
    } else {
        Difficulty::Hard
    }
}

/// Build a complete FitnessMetrics from SAB result + system measurements
pub fn build_fitness_metrics(
    sab: &SabResult,
    token_budget: u64,
    timeout_secs: f64,
    binary_path: &Path,
    test_count: usize,
    total_tests: usize,
    max_binary_mb: f64,
) -> FitnessMetrics {
    let binary_size_mb = std::fs::metadata(binary_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    // Approximate test coverage from pass rate
    let test_coverage_pct = if total_tests > 0 {
        (test_count as f64 / total_tests as f64) * 100.0
    } else {
        0.0
    };

    FitnessMetrics {
        sab_score: sab.aggregate_score,
        tokens_used: sab.total_tokens_used,
        token_budget,
        wall_clock_secs: sab.wall_clock.as_secs_f64(),
        timeout_secs,
        test_coverage_pct,
        binary_size_mb,
        max_binary_size_mb: max_binary_mb,
        tests_passed: test_count,
        tests_total: total_tests,
        visual_score: 0.0,
    }
}

/// Compare two fitness snapshots and return the delta
pub fn fitness_delta(
    baseline: &FitnessMetrics,
    candidate: &FitnessMetrics,
    weights: &FitnessWeights,
) -> f64 {
    weights.composite(candidate) - weights.composite(baseline)
}

#[derive(Debug)]
pub enum FitnessError {
    SabRunFailed(String),
    ReportParseFailed(String),
    BinaryNotFound(PathBuf),
}

impl std::fmt::Display for FitnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SabRunFailed(msg) => write!(f, "SAB run failed: {}", msg),
            Self::ReportParseFailed(msg) => write!(f, "Failed to parse SAB report: {}", msg),
            Self::BinaryNotFound(p) => write!(f, "Binary not found: {}", p.display()),
        }
    }
}

impl std::error::Error for FitnessError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rating_thresholds() {
        let make_result = |score: f64| SabResult {
            aggregate_score: score,
            scenario_scores: vec![],
            total_tokens_used: 0,
            wall_clock: Duration::ZERO,
            rating: match score as u32 {
                85..=100 => GenerationRating::Bloom,
                60..=84 => GenerationRating::Grow,
                30..=59 => GenerationRating::Wilt,
                _ => GenerationRating::Frost,
            },
        };

        assert_eq!(make_result(95.0).rating, GenerationRating::Bloom);
        assert_eq!(make_result(85.0).rating, GenerationRating::Bloom);
        assert_eq!(make_result(70.0).rating, GenerationRating::Grow);
        assert_eq!(make_result(45.0).rating, GenerationRating::Wilt);
        assert_eq!(make_result(20.0).rating, GenerationRating::Frost);
    }

    #[test]
    fn test_difficulty_inference() {
        assert_eq!(infer_difficulty("easy_calculator"), Difficulty::Easy);
        assert_eq!(infer_difficulty("medium_bitset"), Difficulty::Medium);
        assert_eq!(infer_difficulty("testgen_ringbuf"), Difficulty::Medium);
        assert_eq!(infer_difficulty("hard_scheduler"), Difficulty::Hard);
        assert_eq!(infer_difficulty("expert_async_race"), Difficulty::Expert);
    }

    #[test]
    fn test_fitness_delta_positive_improvement() {
        let weights = FitnessWeights::default();
        let baseline = FitnessMetrics {
            sab_score: 90.0,
            tokens_used: 300_000,
            token_budget: 500_000,
            wall_clock_secs: 1800.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 82.0,
            binary_size_mb: 15.0,
            max_binary_size_mb: 50.0,
            tests_passed: 5200,
            tests_total: 5200,
            visual_score: 0.0,
        };
        let better = FitnessMetrics {
            sab_score: 95.0,
            tokens_used: 200_000,
            ..baseline.clone()
        };
        assert!(fitness_delta(&baseline, &better, &weights) > 0.0);
    }

    #[test]
    fn test_parse_sab_output_json_path() {
        // Output containing a report path — should try to parse as JSON file
        // (which won't exist), then fall back
        let output = "Running SAB...\nreports/sab_2024.json\nDone.";
        let result = parse_sab_output(output, Duration::from_secs(60));
        // The report file doesn't exist, so this returns an error
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_text_output_multiple_scenarios() {
        let output = "\
easy_calculator: 95/100 BLOOM
medium_bitset: 72/100 GROW
hard_scheduler: 45/100 WILT
expert_async_race: 30/100 FROST";
        let scores = parse_text_output(output).unwrap();
        assert_eq!(scores.len(), 4);
        assert_eq!(scores[0].name, "easy_calculator");
        assert_eq!(scores[0].score, 95.0);
        assert_eq!(scores[0].difficulty, Difficulty::Easy);
        assert_eq!(scores[1].name, "medium_bitset");
        assert_eq!(scores[1].difficulty, Difficulty::Medium);
        assert_eq!(scores[2].name, "hard_scheduler");
        assert_eq!(scores[2].difficulty, Difficulty::Hard);
        assert_eq!(scores[3].name, "expert_async_race");
        assert_eq!(scores[3].difficulty, Difficulty::Expert);
    }

    #[test]
    fn test_parse_text_output_empty() {
        let scores = parse_text_output("").unwrap();
        assert!(scores.is_empty());
    }

    #[test]
    fn test_parse_text_output_malformed() {
        let output = "easy_calculator: not_a_number/100\nrandom line\n: /100";
        let scores = parse_text_output(output).unwrap();
        // "not_a_number" can't be parsed as f64, so no score for that line
        assert!(scores.is_empty());
    }

    #[test]
    fn test_rating_boundary_84_is_grow() {
        // 84 is in the Grow range (60..=84)
        let result = parse_sab_output("test_scenario: 84/100 OK", Duration::from_secs(10)).unwrap();
        assert_eq!(result.rating, GenerationRating::Grow);
    }

    #[test]
    fn test_rating_boundary_85_is_bloom() {
        let result = parse_sab_output("test_scenario: 85/100 OK", Duration::from_secs(10)).unwrap();
        assert_eq!(result.rating, GenerationRating::Bloom);
    }

    #[test]
    fn test_rating_boundary_59_is_wilt() {
        let result = parse_sab_output("test_scenario: 59/100 OK", Duration::from_secs(10)).unwrap();
        assert_eq!(result.rating, GenerationRating::Wilt);
    }

    #[test]
    fn test_rating_boundary_29_is_frost() {
        let result = parse_sab_output("test_scenario: 29/100 OK", Duration::from_secs(10)).unwrap();
        assert_eq!(result.rating, GenerationRating::Frost);
    }

    #[test]
    fn test_fitness_delta_negative() {
        let weights = FitnessWeights::default();
        let baseline = FitnessMetrics {
            sab_score: 90.0,
            tokens_used: 200_000,
            token_budget: 500_000,
            wall_clock_secs: 1000.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 85.0,
            binary_size_mb: 10.0,
            max_binary_size_mb: 50.0,
            tests_passed: 5200,
            tests_total: 5200,
            visual_score: 0.0,
        };
        let worse = FitnessMetrics {
            sab_score: 60.0,
            tokens_used: 450_000,
            wall_clock_secs: 3500.0,
            test_coverage_pct: 50.0,
            binary_size_mb: 45.0,
            ..baseline.clone()
        };
        let delta = fitness_delta(&baseline, &worse, &weights);
        assert!(delta < 0.0, "Delta should be negative for worse candidate");
    }

    #[test]
    fn test_fitness_delta_equal() {
        let weights = FitnessWeights::default();
        let metrics = FitnessMetrics {
            sab_score: 80.0,
            tokens_used: 200_000,
            token_budget: 500_000,
            wall_clock_secs: 1800.0,
            timeout_secs: 3600.0,
            test_coverage_pct: 82.0,
            binary_size_mb: 15.0,
            max_binary_size_mb: 50.0,
            tests_passed: 5200,
            tests_total: 5200,
            visual_score: 0.0,
        };
        let delta = fitness_delta(&metrics, &metrics, &weights);
        assert!(
            delta.abs() < f64::EPSILON,
            "Delta should be 0 for identical metrics"
        );
    }

    #[test]
    fn test_build_fitness_metrics_missing_binary() {
        let sab = SabResult {
            aggregate_score: 75.0,
            scenario_scores: vec![],
            total_tokens_used: 100_000,
            wall_clock: Duration::from_secs(600),
            rating: GenerationRating::Grow,
        };
        let metrics = build_fitness_metrics(
            &sab,
            500_000,
            3600.0,
            std::path::Path::new("/nonexistent/binary"),
            5000,
            5200,
            50.0,
        );
        assert_eq!(metrics.binary_size_mb, 0.0); // File doesn't exist → 0.0
        assert_eq!(metrics.sab_score, 75.0);
        assert_eq!(metrics.tokens_used, 100_000);
        assert_eq!(metrics.tests_passed, 5000);
        assert_eq!(metrics.tests_total, 5200);
    }

    #[test]
    fn test_sab_config_default() {
        let cfg = SabConfig::default();
        assert!(cfg.runner_script.to_str().unwrap().contains("run_full_sab"));
        assert_eq!(cfg.model, "Qwen/Qwen3-Coder-Next-FP8");
        assert_eq!(cfg.max_parallel, 6);
        assert_eq!(cfg.scenario_timeout, Duration::from_secs(3600));
        assert!(cfg.scenario_filter.is_none());
    }

    #[test]
    fn test_parse_sab_output_empty_gives_frost() {
        let result = parse_sab_output("", Duration::from_secs(10)).unwrap();
        assert_eq!(result.aggregate_score, 0.0);
        assert_eq!(result.rating, GenerationRating::Frost);
        assert!(result.scenario_scores.is_empty());
    }

    #[test]
    fn test_infer_difficulty_refactor_prefix() {
        assert_eq!(infer_difficulty("refactor_module"), Difficulty::Medium);
    }

    #[test]
    fn test_infer_difficulty_unknown_prefix() {
        assert_eq!(infer_difficulty("custom_scenario"), Difficulty::Hard);
    }

    #[test]
    fn test_fitness_error_display() {
        let e1 = FitnessError::SabRunFailed("timeout".to_string());
        assert!(format!("{}", e1).contains("timeout"));

        let e2 = FitnessError::ReportParseFailed("bad json".to_string());
        assert!(format!("{}", e2).contains("bad json"));

        let e3 = FitnessError::BinaryNotFound(PathBuf::from("/tmp/missing"));
        assert!(format!("{}", e3).contains("/tmp/missing"));
    }

    #[test]
    fn test_scenario_score_derived_fields() {
        let output = "easy_calculator: 95/100 BLOOM";
        let scores = parse_text_output(output).unwrap();
        assert_eq!(scores.len(), 1);
        assert!(scores[0].tests_passed); // 95 >= 70
        assert!(scores[0].broken_tests_fixed); // 95 >= 90
        assert!(scores[0].clean_exit); // 95 >= 10
    }

    #[test]
    fn test_scenario_score_low_score_flags() {
        let output = "bad_scenario: 5/100 FROST";
        let scores = parse_text_output(output).unwrap();
        assert_eq!(scores.len(), 1);
        assert!(!scores[0].tests_passed); // 5 < 70
        assert!(!scores[0].broken_tests_fixed); // 5 < 90
        assert!(!scores[0].clean_exit); // 5 < 10
    }
}
