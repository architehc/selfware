//! Fitness Function — Meta-SAB Scoring Engine
//!
//! Wraps the existing SAB (Selfware Agentic Benchmark) as a fitness function
//! for evolutionary evaluation. This module is PROTECTED — the evolution
//! daemon cannot modify it, preventing reward hacking.

use super::{FitnessMetrics, FitnessWeights, GenerationRating};
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
    /// `None` unless EVERY scenario reported usage. A partial sum is not a
    /// total, and presenting one as a total understates cost.
    pub total_tokens_used: Option<u64>,
    pub wall_clock: Duration,
    pub rating: GenerationRating,
    /// SHA-256 of the executable the runner actually ran.
    pub binary_sha256: String,
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct ScenarioScore {
    pub name: String,
    pub difficulty: Difficulty,
    pub score: f64,
    pub tests_passed: bool,
    pub broken_tests_fixed: bool,
    pub clean_exit: bool,
    /// `None` when the runner did not observe token usage.
    ///
    /// This was `u64` defaulting to 0, and zero tokens reads as perfect
    /// efficiency to the fitness weights — an unmeasured run scored better than
    /// a measured one.
    pub tokens_used: Option<u64>,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
    Expert,
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
    parse_sab_output(&stdout, wall_clock, selfware_binary)
}

/// SHA-256 of a file, so a report can be checked against the binary requested.
fn sha256_of(path: &Path) -> Result<String, FitnessError> {
    use sha2::{Digest, Sha256};
    let bytes =
        std::fs::read(path).map_err(|_| FitnessError::BinaryNotFound(path.to_path_buf()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Parse SAB runner output into structured results.
///
/// The runner prints `SAB_REPORT_JSON=<path>`; that file is the only accepted
/// source. The previous version searched stdout for any line containing
/// "reports/" and ".json", and when it found none fell back to scraping
/// `name: NN/100` lines out of the text and INVENTING outcomes from score
/// thresholds — `tests_passed = score >= 70`, `clean_exit = score >= 10`. Those
/// flags then fed promotion decisions. A parser that manufactures the facts it
/// reports cannot establish which scenarios passed, so it is gone: a run that
/// produces no structured report is an error, not a low score.
fn parse_sab_output(
    output: &str,
    wall_clock: Duration,
    requested_binary: &Path,
) -> Result<SabResult, FitnessError> {
    let report_path = output
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("SAB_REPORT_JSON="))
        .ok_or_else(|| {
            FitnessError::ReportParseFailed(
                "runner printed no SAB_REPORT_JSON= line; refusing to guess scores from text"
                    .into(),
            )
        })?;

    let content = std::fs::read_to_string(report_path)
        .map_err(|e| FitnessError::ReportParseFailed(format!("{report_path}: {e}")))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| FitnessError::ReportParseFailed(e.to_string()))?;

    match json["schema"].as_str() {
        Some("sab-report/1") => {}
        other => {
            return Err(FitnessError::ReportParseFailed(format!(
                "unsupported report schema: {other:?}"
            )))
        }
    }

    // Did the runner score the build we asked it to score?
    let evaluated = json["binary_sha256"]
        .as_str()
        .ok_or_else(|| FitnessError::IncompleteReport("binary_sha256".into()))?;
    let requested = sha256_of(requested_binary)?;
    if evaluated != requested {
        return Err(FitnessError::WrongBinaryEvaluated {
            requested,
            evaluated: evaluated.to_string(),
        });
    }

    let expected = json["scenarios_expected"]
        .as_u64()
        .ok_or_else(|| FitnessError::IncompleteReport("scenarios_expected".into()))?;
    let scenarios = json["scenarios"]
        .as_array()
        .ok_or_else(|| FitnessError::IncompleteReport("scenarios".into()))?;
    if scenarios.len() as u64 != expected {
        // A partial run averaged over the scenarios that finished, so a
        // candidate that crashed most of the suite could outscore one that
        // completed it.
        return Err(FitnessError::IncompleteReport(format!(
            "{} of {expected} scenarios reported; a partial suite is not a score",
            scenarios.len()
        )));
    }

    let scenario_scores = scenarios
        .iter()
        .map(|s| {
            let field = |name: &str| -> Result<&serde_json::Value, FitnessError> {
                s.get(name)
                    .ok_or_else(|| FitnessError::IncompleteReport(name.to_string()))
            };
            Ok(ScenarioScore {
                name: field("name")?.as_str().unwrap_or("unknown").to_string(),
                difficulty: match field("difficulty")?.as_str().unwrap_or("medium") {
                    "easy" => Difficulty::Easy,
                    "medium" => Difficulty::Medium,
                    "hard" => Difficulty::Hard,
                    "expert" => Difficulty::Expert,
                    _ => Difficulty::Medium,
                },
                score: field("score")?
                    .as_f64()
                    .ok_or_else(|| FitnessError::IncompleteReport("score".into()))?,
                // Measured, not derived from a score threshold.
                tests_passed: field("tests_passed")?
                    .as_bool()
                    .ok_or_else(|| FitnessError::IncompleteReport("tests_passed".into()))?,
                broken_tests_fixed: field("broken_tests_fixed")?
                    .as_bool()
                    .ok_or_else(|| FitnessError::IncompleteReport("broken_tests_fixed".into()))?,
                clean_exit: field("clean_exit")?
                    .as_bool()
                    .ok_or_else(|| FitnessError::IncompleteReport("clean_exit".into()))?,
                // Explicit null means the runner did not observe it.
                tokens_used: s.get("tokens_used").and_then(|v| v.as_u64()),
                duration: Duration::from_secs(
                    field("duration_secs")?
                        .as_u64()
                        .ok_or_else(|| FitnessError::IncompleteReport("duration_secs".into()))?,
                ),
            })
        })
        .collect::<Result<Vec<_>, FitnessError>>()?;

    let aggregate = if scenario_scores.is_empty() {
        return Err(FitnessError::IncompleteReport(
            "no scenarios in report".into(),
        ));
    } else {
        scenario_scores.iter().map(|s| s.score).sum::<f64>() / scenario_scores.len() as f64
    };

    // A total only exists if every scenario reported. Summing the ones that did
    // and calling it the total understates cost by however many were missing.
    let total_tokens_used = scenario_scores
        .iter()
        .map(|s| s.tokens_used)
        .try_fold(0u64, |acc, t| t.map(|t| acc + t));

    let rating = match aggregate as u32 {
        85..=100 => GenerationRating::Bloom,
        60..=84 => GenerationRating::Grow,
        30..=59 => GenerationRating::Wilt,
        _ => GenerationRating::Frost,
    };

    Ok(SabResult {
        aggregate_score: aggregate,
        scenario_scores,
        total_tokens_used,
        wall_clock,
        rating,
        binary_sha256: evaluated.to_string(),
        run_id: json["run_id"].as_str().unwrap_or("unknown").to_string(),
    })
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
        full_evaluation_secs: Some(sab.wall_clock.as_secs_f64()),
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
    /// The runner scored a different executable than the one requested.
    ///
    /// The reason this exists: the runner ignored `SELFWARE_BINARY` and always
    /// built and ran its OWN repository's release binary, so every candidate
    /// was evaluated using the baseline build. Fitness could not tell the two
    /// apart. A mismatch must fail loudly rather than produce a number.
    WrongBinaryEvaluated {
        requested: String,
        evaluated: String,
    },
    /// The report omitted a field whose absence cannot be defaulted.
    IncompleteReport(String),
}

impl std::fmt::Display for FitnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SabRunFailed(msg) => write!(f, "SAB run failed: {}", msg),
            Self::ReportParseFailed(msg) => write!(f, "Failed to parse SAB report: {}", msg),
            Self::BinaryNotFound(p) => write!(f, "Binary not found: {}", p.display()),
            Self::WrongBinaryEvaluated {
                requested,
                evaluated,
            } => write!(
                f,
                "SAB scored the wrong executable: asked for {requested}, runner ran {evaluated}. \
                 The score describes a different build and must not be used."
            ),
            Self::IncompleteReport(msg) => {
                write!(f, "SAB report is missing required data: {msg}")
            }
        }
    }
}

impl std::error::Error for FitnessError {}

#[cfg(test)]
#[path = "../../tests/unit/evolution/fitness/fitness_test.rs"]
mod tests;
