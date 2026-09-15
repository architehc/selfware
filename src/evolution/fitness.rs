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
    /// How many completed report directories to retain (default 10)
    pub report_retention: usize,
    /// Promoted or baseline report directories that must not be deleted
    pub exempt_reports: Vec<PathBuf>,
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
            report_retention: 10,
            exempt_reports: Vec::new(),
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
    pub report_path: PathBuf,
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

/// Bounded report GC: retain at most `keep_count` newest completed, unleased directories matching `prefix`.
/// Exempts directories in `exempt_paths` (e.g. promoted generation winners or active baselines).
pub fn prune_report_dirs(
    reports_dir: &Path,
    prefix: &str,
    keep_count: usize,
    exempt_paths: &[PathBuf],
) {
    if !reports_dir.exists() {
        return;
    }
    let entries = match std::fs::read_dir(reports_dir) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(
                "Failed to read report directory for pruning {}: {err}",
                reports_dir.display()
            );
            return;
        }
    };
    let mut eligible_dirs: Vec<(std::time::SystemTime, std::path::PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|s| s.starts_with(prefix))
                .unwrap_or(false)
                && e.path().is_dir()
        })
        .filter_map(|e| {
            let path = e.path();
            let mtime = e.metadata().ok()?.modified().ok()?;
            // 1. Check if exempt (e.g. promoted generation winner or baseline report)
            if exempt_paths.iter().any(|exempt| {
                let is_file = exempt.is_file()
                    || (!exempt.is_dir() && exempt.extension().is_some_and(|ext| ext == "json"));
                let exempt_dir = if is_file {
                    exempt.parent().unwrap_or(exempt)
                } else {
                    exempt
                };
                if exempt_dir == reports_dir {
                    return path == *exempt;
                }
                path == *exempt
                    || path == exempt_dir
                    || path.starts_with(exempt)
                    || path.starts_with(exempt_dir)
                    || exempt.starts_with(&path)
            }) {
                return None;
            }
            // 2. Check if active/leased by an in-flight run
            let lease_path = path.join(".lease");
            let lease_pid_path = path.join(".lease_pid");
            if lease_pid_path.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&lease_pid_path) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        #[cfg(unix)]
                        {
                            let res = unsafe { nix::libc::kill(pid, 0) };
                            if res != 0 {
                                let errno =
                                    std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                                if errno == nix::libc::ESRCH {
                                    // Process is dead; clean up stale lease files
                                    let _ = std::fs::remove_file(&lease_pid_path);
                                    let _ = std::fs::remove_file(&lease_path);
                                }
                            }
                        }
                    }
                }
            }
            if lease_path.exists() {
                #[cfg(unix)]
                {
                    use std::os::fd::AsRawFd;
                    match std::fs::File::open(&lease_path) {
                        Ok(file) => {
                            let rc = unsafe {
                                nix::libc::flock(
                                    file.as_raw_fd(),
                                    nix::libc::LOCK_EX | nix::libc::LOCK_NB,
                                )
                            };
                            if rc != 0 {
                                // Currently locked by an active process
                                return None;
                            }
                            unsafe {
                                nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
                            }
                        }
                        Err(err) => {
                            tracing::warn!(
                                "Failed to probe open lease file {}: {err}",
                                lease_path.display()
                            );
                            // Fail safe: treat as active/exempt so we don't delete an in-use dir
                            return None;
                        }
                    }
                }
            }
            // 3. Check for completion marker or valid structured report
            let is_completed = path.join(".completed").exists()
                || path.join("sab_report.json").exists()
                || path.join("report.json").exists();
            let age = std::time::SystemTime::now()
                .duration_since(mtime)
                .unwrap_or_default();
            // If not completed and less than 2 hours old, consider it in-flight and protect it
            if !is_completed && age < Duration::from_secs(7200) {
                return None;
            }
            Some((mtime, path))
        })
        .collect();

    if eligible_dirs.len() > keep_count {
        eligible_dirs.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
        for (_, path) in eligible_dirs.into_iter().skip(keep_count) {
            tracing::info!("Pruning old benchmark report directory: {:?}", path);
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!("Failed to remove pruned report directory {:?}: {}", path, e);
            }
        }
    }
}

/// Run the full SAB benchmark and return structured results
pub fn run_sab(selfware_binary: &Path, config: &SabConfig) -> Result<SabResult, FitnessError> {
    let start = Instant::now();

    let reports_dir = config
        .runner_script
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("reports");
    prune_report_dirs(
        &reports_dir,
        "sab-",
        config.report_retention,
        &config.exempt_reports,
    );

    let unique_out_dir = reports_dir.join(format!("sab-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&unique_out_dir)
        .map_err(|e| FitnessError::SabRunFailed(e.to_string()))?;

    // Hold an exclusive lock on .lease for the duration of the run
    let lease_file = std::fs::File::create(unique_out_dir.join(".lease"))
        .map_err(|e| FitnessError::SabRunFailed(e.to_string()))?;
    #[cfg(unix)]
    unsafe {
        use std::os::fd::AsRawFd;
        let rc = nix::libc::flock(lease_file.as_raw_fd(), nix::libc::LOCK_EX);
        if rc != 0 {
            return Err(FitnessError::SabRunFailed(format!(
                "failed to acquire exclusive lease lock: rc={rc}"
            )));
        }
    }

    // Set up environment for SAB runner
    let output = Command::new("bash")
        .arg(&config.runner_script)
        .env("OUT_DIR", &unique_out_dir)
        .env("SELFWARE_LEASE_HELD", "1")
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

    if let Err(e) = std::fs::write(unique_out_dir.join(".completed"), b"") {
        tracing::warn!(
            "Failed to write .completed marker to {}: {e}",
            unique_out_dir.display()
        );
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

    let mut seen_names = std::collections::HashSet::new();
    for s in &scenario_scores {
        if s.name.trim().is_empty() || s.name == "unknown" {
            return Err(FitnessError::ReportParseFailed(
                "scenario has missing or invalid name".into(),
            ));
        }
        if !s.score.is_finite() || !(0.0..=100.0).contains(&s.score) {
            return Err(FitnessError::ReportParseFailed(format!(
                "scenario {} has invalid score: {}",
                s.name, s.score
            )));
        }
        if !seen_names.insert(&s.name) {
            return Err(FitnessError::ReportParseFailed(format!(
                "duplicate scenario name in report: {}",
                s.name
            )));
        }
    }

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
        report_path: PathBuf::from(report_path),
    })
}

/// DarwinX Non-Regression Invariant Violation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarwinXViolation {
    SuiteMismatch {
        missing_in_candidate: Vec<String>,
        unexpected_in_candidate: Vec<String>,
    },
    Regression {
        regressed_scenarios: Vec<String>,
    },
}

impl std::fmt::Display for DarwinXViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DarwinXViolation::SuiteMismatch {
                missing_in_candidate,
                unexpected_in_candidate,
            } => write!(
                f,
                "DarwinX Suite Identity violated: missing in candidate: {:?}, unexpected: {:?}",
                missing_in_candidate, unexpected_in_candidate
            ),
            DarwinXViolation::Regression {
                regressed_scenarios,
            } => write!(
                f,
                "DarwinX Non-Regression invariant violated: {} baseline-passed scenario(s) failed in candidate: {:?}",
                regressed_scenarios.len(),
                regressed_scenarios
            ),
        }
    }
}

impl std::error::Error for DarwinXViolation {}

impl SabResult {
    /// DarwinX non-regression invariant:
    /// Passed(baseline) ∩ Failed(candidate) = ∅
    ///
    /// Any scenario that passed in the baseline MUST NOT fail in the candidate,
    /// and the suite of scenarios evaluated must be identical.
    pub fn check_darwinx_non_regression(
        &self,
        candidate: &SabResult,
    ) -> Result<(), DarwinXViolation> {
        let baseline_names: std::collections::HashSet<&str> = self
            .scenario_scores
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        let candidate_names: std::collections::HashSet<&str> = candidate
            .scenario_scores
            .iter()
            .map(|s| s.name.as_str())
            .collect();

        let missing: Vec<String> = baseline_names
            .difference(&candidate_names)
            .map(|s| s.to_string())
            .collect();
        let unexpected: Vec<String> = candidate_names
            .difference(&baseline_names)
            .map(|s| s.to_string())
            .collect();

        if !missing.is_empty() || !unexpected.is_empty() {
            return Err(DarwinXViolation::SuiteMismatch {
                missing_in_candidate: missing,
                unexpected_in_candidate: unexpected,
            });
        }

        let candidate_scenarios: std::collections::HashMap<&str, bool> = candidate
            .scenario_scores
            .iter()
            .map(|s| (s.name.as_str(), s.tests_passed))
            .collect();

        let mut regressed = Vec::new();
        for baseline_scenario in &self.scenario_scores {
            if baseline_scenario.tests_passed {
                match candidate_scenarios.get(baseline_scenario.name.as_str()) {
                    Some(&cand_passed) => {
                        if !cand_passed {
                            regressed.push(baseline_scenario.name.clone());
                        }
                    }
                    None => {
                        regressed.push(baseline_scenario.name.clone());
                    }
                }
            }
        }

        if regressed.is_empty() {
            Ok(())
        } else {
            Err(DarwinXViolation::Regression {
                regressed_scenarios: regressed,
            })
        }
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
