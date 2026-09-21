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

/// Host-side watchdog on a whole SAB run.
///
/// The runner enforces per-scenario ceilings (a 300 s stall kill, and an
/// absolute ceiling of the scenario timeout × 3), but nothing on this side
/// bounded the *runner*: a wedged `timeout` child or a stuck container left the
/// `try_wait` poll below sleeping forever, hanging the daemon until SIGTERM.
/// The default is deliberately generous — it exists to bound the run, not to
/// pace it — so it only fires once the runner's own ceilings have failed.
/// Override with [`SAB_DEADLINE_ENV`]; `0` disables the watchdog.
const DEFAULT_SAB_DEADLINE: Duration = Duration::from_secs(4 * 3600);

/// Env override for [`DEFAULT_SAB_DEADLINE`], in seconds (`0` disables).
const SAB_DEADLINE_ENV: &str = "SELFWARE_SAB_DEADLINE_SECS";

fn sab_deadline() -> Option<Duration> {
    parse_sab_deadline(std::env::var(SAB_DEADLINE_ENV).ok().as_deref())
}

/// Pure parsing core of [`sab_deadline`]: unset or unparseable falls back to the
/// default, and `0` disables the watchdog.
fn parse_sab_deadline(value: Option<&str>) -> Option<Duration> {
    match value {
        None => Some(DEFAULT_SAB_DEADLINE),
        Some(raw) => match raw.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => {
                tracing::warn!("ignoring unparseable {SAB_DEADLINE_ENV}={raw:?}");
                Some(DEFAULT_SAB_DEADLINE)
            }
        },
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
    /// Target model identifier reported by the benchmark runner.
    pub model: Option<String>,
    /// Target endpoint URL reported by the benchmark runner.
    pub endpoint: Option<String>,
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
            // 2. Check if active/leased by an in-flight run.
            // Treat the flock as authoritative: NEVER unlink a lease based solely on
            // recorded PID liveness, because surviving child processes can still hold the lock.
            let lease_path = path.join(".lease");
            let lease_pid_path = path.join(".lease_pid");
            // Check for completion marker or valid structured report
            let is_completed = path.join(".completed").exists()
                || path.join("sab_report.json").exists()
                || path.join("report.json").exists();

            let age = std::time::SystemTime::now()
                .duration_since(mtime)
                .unwrap_or_default();
            let is_recent = age < Duration::from_secs(7200);

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
                                // Currently locked by an active process (parent or child) -> in-flight!
                                return None;
                            }
                            // Lock acquired by us! No process actively holds this lease lock.
                            if is_completed || !is_recent {
                                // Completed run, or uncompleted run past the 2-hour backstop:
                                // a surviving parent daemon must NOT exempt an abandoned or finished
                                // run from retention limits. Clean up leftover lease files and proceed.
                                let _ = std::fs::remove_file(&lease_pid_path);
                                let _ = std::fs::remove_file(&lease_path);
                                unsafe {
                                    nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
                                }
                            } else {
                                // Not completed and recent (< 2h): check if the recorded PID is dead.
                                let mut pid_is_dead = false;
                                if lease_pid_path.exists() {
                                    if let Ok(pid_str) = std::fs::read_to_string(&lease_pid_path) {
                                        if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                            let res = unsafe { nix::libc::kill(pid, 0) };
                                            if res != 0 {
                                                let errno = std::io::Error::last_os_error()
                                                    .raw_os_error()
                                                    .unwrap_or(0);
                                                if errno == nix::libc::ESRCH {
                                                    pid_is_dead = true;
                                                }
                                            }
                                        }
                                    }
                                }
                                if pid_is_dead {
                                    // Lock is held by us and the recorded PID is dead.
                                    // Clean up stale lease files before releasing the lock.
                                    let _ = std::fs::remove_file(&lease_pid_path);
                                    let _ = std::fs::remove_file(&lease_path);
                                }
                                unsafe {
                                    nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
                                }
                                if !pid_is_dead && lease_pid_path.exists() {
                                    // In-flight/uncompleted run with live process: exempt this directory
                                    return None;
                                }
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
            } else if lease_pid_path.exists() {
                if is_completed || !is_recent {
                    let _ = std::fs::remove_file(&lease_pid_path);
                } else {
                    #[cfg(unix)]
                    {
                        if let Ok(pid_str) = std::fs::read_to_string(&lease_pid_path) {
                            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                let res = unsafe { nix::libc::kill(pid, 0) };
                                if res == 0 {
                                    return None;
                                }
                                let errno =
                                    std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                                if errno == nix::libc::ESRCH {
                                    let _ = std::fs::remove_file(&lease_pid_path);
                                } else {
                                    return None;
                                }
                            }
                        }
                    }
                }
            }
            // If not completed and less than 2 hours old, consider it in-flight and protect it
            if !is_completed && is_recent {
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

/// RAII lease guard to ensure `.lease_pid` and `.lease` are cleaned up on all exit paths,
/// including early returns and error conditions.
pub(crate) struct LeaseGuard {
    pub(crate) dir: PathBuf,
    pub(crate) lease_file: Option<std::fs::File>,
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.dir.join(".lease_pid"));
        if let Some(file) = self.lease_file.take() {
            #[cfg(unix)]
            unsafe {
                use std::os::fd::AsRawFd;
                nix::libc::flock(file.as_raw_fd(), nix::libc::LOCK_UN);
            }
            drop(file);
        }
        let _ = std::fs::remove_file(self.dir.join(".lease"));
    }
}

/// RAII guard to terminate child benchmark process and its entire process group on drop,
/// ensuring that cancellation or early returns clean up running benchmark subprocesses.
struct SabProcessGroupGuard {
    child: Option<std::process::Child>,
}

impl Drop for SabProcessGroupGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            #[cfg(unix)]
            {
                let pid = child.id();
                use nix::sys::signal::{killpg, Signal};
                use nix::unistd::Pid;
                let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGTERM);
            }
            let _ = child.kill();
            let _ = child.wait();
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

    // Hold an exclusive non-blocking lock on .lease for the duration of the run
    let lease_file = std::fs::File::create(unique_out_dir.join(".lease"))
        .map_err(|e| FitnessError::SabRunFailed(e.to_string()))?;
    #[cfg(unix)]
    unsafe {
        use std::os::fd::AsRawFd;
        let rc = nix::libc::flock(
            lease_file.as_raw_fd(),
            nix::libc::LOCK_EX | nix::libc::LOCK_NB,
        );
        if rc != 0 {
            return Err(FitnessError::SabRunFailed(format!(
                "failed to acquire exclusive lease lock: rc={rc}"
            )));
        }
    }
    let _ = std::fs::write(
        unique_out_dir.join(".lease_pid"),
        std::process::id().to_string(),
    );
    let lease_guard = LeaseGuard {
        dir: unique_out_dir.clone(),
        lease_file: Some(lease_file),
    };

    if !selfware_binary.exists() {
        return Err(FitnessError::BinaryNotFound(selfware_binary.to_path_buf()));
    }

    // Pin the requested binary into unique_out_dir before launching the runner.
    // This decouples the evaluation from host builds (e.g. `cargo build --release`),
    // preventing concurrent builds from overwriting the binary mid-run during long suites.
    let pinned_binary = unique_out_dir.join("pinned_selfware_bin");
    std::fs::copy(selfware_binary, &pinned_binary).map_err(|e| {
        FitnessError::SabRunFailed(format!(
            "failed to pin binary '{}' into '{}': {e}",
            selfware_binary.display(),
            pinned_binary.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&pinned_binary) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&pinned_binary, perms);
        }
    }

    // Set up environment for SAB runner. Sanitize FIRST (before the
    // task-specific vars below): the runner script is project-controlled and
    // a bare child would inherit every credential on the box. Only the vars
    // the runner explicitly needs (scores, endpoint, model, pinned binary,
    // timeouts) are re-added; nothing else from the parent env crosses over.
    let mut cmd = Command::new("bash");
    crate::safety::process_env::sanitize_std_command_env_preserve(&mut cmd, &[]);
    cmd.arg(&config.runner_script)
        .env("OUT_DIR", &unique_out_dir)
        .env("SELFWARE_LEASE_HELD", "1")
        .env("ENDPOINT", &config.endpoint)
        .env("MODEL", &config.model)
        .env("MAX_PARALLEL", config.max_parallel.to_string())
        .env("SELFWARE_BINARY", pinned_binary.to_string_lossy().as_ref())
        .env("TIMEOUT", config.scenario_timeout.as_secs().to_string());
    if let Some(ref filter) = config.scenario_filter {
        if !filter.is_empty() {
            let filter_str = filter.join(",");
            cmd.env("SAB_FILTER", &filter_str);
            cmd.arg(&filter_str);
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let stdout_path = unique_out_dir.join("sab_stdout.log");
    let stderr_path = unique_out_dir.join("sab_stderr.log");
    let stdout_file = std::fs::File::create(&stdout_path)
        .map_err(|e| FitnessError::SabRunFailed(format!("failed to create stdout log: {e}")))?;
    let stderr_file = std::fs::File::create(&stderr_path)
        .map_err(|e| FitnessError::SabRunFailed(format!("failed to create stderr log: {e}")))?;

    cmd.stdin(std::process::Stdio::null())
        .stdout(stdout_file)
        .stderr(stderr_file);

    let child = cmd
        .spawn()
        .map_err(|e| FitnessError::SabRunFailed(e.to_string()))?;

    let mut child_guard = SabProcessGroupGuard { child: Some(child) };

    let deadline = sab_deadline().map(|window| start + window);
    let exit_status = loop {
        if let Some(ref mut c) = child_guard.child {
            match c.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(e) => break Err(FitnessError::SabRunFailed(e.to_string())),
            }
        }
        if crate::is_shutdown_requested() {
            return Err(FitnessError::SabRunFailed(
                "cancelled by shutdown signal".to_string(),
            ));
        }
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                // Returning drops `child_guard`, whose Drop kills the runner's
                // whole process group — so the hung tree goes with us.
                return Err(FitnessError::SabRunFailed(format!(
                    "SAB run exceeded the host watchdog deadline of {}s; the runner's own \
                     per-scenario ceilings should have fired first (override with {SAB_DEADLINE_ENV}, \
                     0 disables)",
                    (deadline - start).as_secs()
                )));
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }?;

    child_guard.child = None;

    let wall_clock = start.elapsed();

    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();

    if !exit_status.success() {
        return Err(FitnessError::SabRunFailed(stderr));
    }

    if let Err(e) = std::fs::write(unique_out_dir.join(".completed"), b"") {
        tracing::warn!(
            "Failed to write .completed marker to {}: {e}",
            unique_out_dir.display()
        );
    }
    drop(lease_guard);

    // Parse SAB output — the runner produces JSON reports evaluated against the pinned binary
    parse_sab_output(&stdout, wall_clock, &pinned_binary)
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

    let requested = sha256_of(requested_binary)?;
    let mut result = parse_sab_report_content(&content, Some(&requested))?;
    result.wall_clock = wall_clock;
    result.report_path = PathBuf::from(report_path);
    Ok(result)
}

/// Parse and strictly validate SAB report JSON content according to `sab-report/1` schema.
///
/// Shared canonical parser used across candidate evaluation (`fitness.rs`) and
/// investigative review / audit verification (`investigate.rs`).
pub fn parse_sab_report_content(
    content: &str,
    expected_binary_sha: Option<&str>,
) -> Result<SabResult, FitnessError> {
    let json: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| FitnessError::ReportParseFailed(format!("corrupted JSON: {e}")))?;

    match json.get("schema").and_then(|v| v.as_str()) {
        Some("sab-report/1") => {}
        Some(other) => {
            return Err(FitnessError::ReportParseFailed(format!(
                "schema mismatch (found '{other}', expected 'sab-report/1')"
            )));
        }
        None => {
            return Err(FitnessError::ReportParseFailed(
                "corrupted: missing schema and scenarios array".into(),
            ));
        }
    }

    // Did the runner score the build we asked it to score?
    let evaluated = json
        .get("binary_sha256")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if let Some(expected) = expected_binary_sha {
        if evaluated.is_empty() || evaluated != expected {
            return Err(FitnessError::WrongBinaryEvaluated {
                requested: expected.to_string(),
                evaluated: evaluated.to_string(),
            });
        }
    }

    let expected = json["scenarios_expected"]
        .as_u64()
        .ok_or_else(|| FitnessError::IncompleteReport("scenarios_expected".into()))?;
    let scenarios = json["scenarios"]
        .as_array()
        .ok_or_else(|| FitnessError::IncompleteReport("scenarios".into()))?;
    if scenarios.is_empty() {
        return Err(FitnessError::IncompleteReport(
            "no scenarios in report; a suite with 0 scenarios is incomplete".into(),
        ));
    }
    if scenarios.len() as u64 != expected {
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
                difficulty: match s
                    .get("difficulty")
                    .and_then(|v| v.as_str())
                    .unwrap_or("medium")
                {
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

    let aggregate =
        scenario_scores.iter().map(|s| s.score).sum::<f64>() / scenario_scores.len() as f64;

    if let Some(agg_score) = json.get("aggregate_score").and_then(|v| v.as_f64()) {
        if (agg_score - aggregate).abs() > 0.05 {
            return Err(FitnessError::ReportParseFailed(format!(
                "aggregate score mismatch (reported {agg_score:.2}, scenario average {aggregate:.2})"
            )));
        }
    }

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

    let model = json
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let endpoint = json
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Ok(SabResult {
        aggregate_score: aggregate,
        scenario_scores,
        total_tokens_used,
        wall_clock: Duration::ZERO,
        rating,
        binary_sha256: evaluated.to_string(),
        model,
        endpoint,
        run_id: json["run_id"].as_str().unwrap_or("unknown").to_string(),
        report_path: PathBuf::new(),
    })
}

/// DarwinX Non-Regression Invariant Violation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DarwinXViolation {
    IdentityMismatch {
        baseline_model: Option<String>,
        candidate_model: Option<String>,
        baseline_endpoint: Option<String>,
        candidate_endpoint: Option<String>,
    },
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
            DarwinXViolation::IdentityMismatch {
                baseline_model,
                candidate_model,
                baseline_endpoint,
                candidate_endpoint,
            } => write!(
                f,
                "DarwinX Evaluation Identity violated: model/endpoint mismatch between baseline ({:?}, {:?}) and candidate ({:?}, {:?})",
                baseline_model, baseline_endpoint, candidate_model, candidate_endpoint
            ),
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
    /// the suite of scenarios evaluated must be identical, and the evaluation
    /// target identity (model and endpoint) must match.
    pub fn check_darwinx_non_regression(
        &self,
        candidate: &SabResult,
    ) -> Result<(), DarwinXViolation> {
        let is_valid_ident = |val: &Option<String>| -> bool {
            match val {
                Some(s) => !s.trim().is_empty(),
                None => false,
            }
        };

        if !is_valid_ident(&self.model)
            || !is_valid_ident(&candidate.model)
            || !is_valid_ident(&self.endpoint)
            || !is_valid_ident(&candidate.endpoint)
            || self.model != candidate.model
            || self.endpoint != candidate.endpoint
        {
            return Err(DarwinXViolation::IdentityMismatch {
                baseline_model: self.model.clone(),
                candidate_model: candidate.model.clone(),
                baseline_endpoint: self.endpoint.clone(),
                candidate_endpoint: candidate.endpoint.clone(),
            });
        }
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

    // Test pass percentage from pass rate
    let test_pass_pct = if total_tests > 0 {
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
        test_pass_pct,
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
