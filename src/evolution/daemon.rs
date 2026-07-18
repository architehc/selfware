//! Evolution Daemon — `selfware evolve`
//!
//! The main evolutionary loop that ties everything together:
//! Mutate → Compile-gate → Sandbox → Fitness → Select/Rollback
//!
//! This module is PROTECTED from self-modification.

#![allow(dead_code, unused_imports, unused_variables)]

use super::ast_tools::{self, AstMutationResult};
use super::fitness::{self, SabConfig, SabResult};
use super::sandbox::SandboxConfig;
use super::telemetry;
use super::tournament::{self, Hypothesis, HypothesisResult, TournamentConfig};
use super::{
    is_protected, EvolutionConfig, FitnessMetrics, FitnessWeights, GenerationRating, LlmConfig,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// The hall of fame — tracks every successful mutation across generations
#[derive(Debug, Clone)]
pub struct GenerationWinner {
    pub generation: usize,
    pub description: String,
    pub composite_score: f64,
    pub sab_delta: f64,
    pub token_delta: f64,
    pub patch: String,
    pub git_tag: Option<String>,
}

/// Summary of the evolution run
#[derive(Debug)]
pub struct EvolutionResult {
    pub generations_run: usize,
    pub improvements: Vec<GenerationWinner>,
    pub final_sab_score: f64,
    pub initial_sab_score: f64,
    pub total_duration: std::time::Duration,
}

const DEFAULT_TOKEN_BUDGET: u64 = 500_000;
const DEFAULT_TIMEOUT_SECS: f64 = 3600.0;
const EVOLVE_FEATURES: &[&str] = &["self-improvement"];

fn rating_from_score(score: f64) -> GenerationRating {
    match score as u32 {
        85..=100 => GenerationRating::Bloom,
        60..=84 => GenerationRating::Grow,
        30..=59 => GenerationRating::Wilt,
        _ => GenerationRating::Frost,
    }
}

fn first_usize(s: &str) -> usize {
    s.split_whitespace()
        .filter_map(|w| w.parse().ok())
        .next()
        .unwrap_or(0)
}

/// Parse the combined cargo test output and return `(passed, total)`.
///
/// Sums every `test result:` line so multi-crate / multi-target runs are
/// handled correctly. Ignored tests are excluded from the total because they
/// are not run.
pub fn parse_test_summary(output: &str) -> (usize, usize) {
    let mut passed = 0usize;
    let mut failed = 0usize;
    for line in output.lines() {
        if line.contains("test result:") {
            for part in line.split(';') {
                let part = part.trim();
                if part.contains("passed") {
                    passed += first_usize(part);
                } else if part.contains("failed") {
                    failed += first_usize(part);
                }
            }
        }
    }
    (passed, passed + failed)
}

fn features_arg(features: &[&str]) -> String {
    features.join(",")
}

/// Measure baseline fitness from real compile / test / fmt / clippy / build
/// metrics. This replaces the previous synthetic baseline score of 50.
fn measure_compile_test_baseline(
    dir: &Path,
    features: &[&str],
    timeout_secs: f64,
) -> Result<FitnessMetrics, String> {
    let start = Instant::now();
    let feat = features_arg(features);

    let mut check_cmd = Command::new("cargo");
    check_cmd.arg("check").current_dir(dir);
    if !features.is_empty() {
        check_cmd.arg("--features").arg(&feat);
    }
    let check = check_cmd
        .output()
        .map_err(|e| format!("cargo check failed to run: {e}"))?;
    if !check.status.success() {
        return Err(format!(
            "cargo check failed:\n{}",
            String::from_utf8_lossy(&check.stderr)
        ));
    }

    let mut test_cmd = Command::new("cargo");
    test_cmd.arg("test").current_dir(dir);
    if !features.is_empty() {
        test_cmd.arg("--features").arg(&feat);
    }
    let test = test_cmd
        .output()
        .map_err(|e| format!("cargo test failed to run: {e}"))?;
    let test_stdout = String::from_utf8_lossy(&test.stdout);
    let test_stderr = String::from_utf8_lossy(&test.stderr);
    let full_output = format!("{}\n{}", test_stdout, test_stderr);
    let (tests_passed, tests_total) = parse_test_summary(&full_output);

    let mut fmt_cmd = Command::new("cargo");
    fmt_cmd.args(["fmt", "--", "--check"]).current_dir(dir);
    let fmt_ok = fmt_cmd
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mut clippy_cmd = Command::new("cargo");
    clippy_cmd.arg("clippy").current_dir(dir);
    if !features.is_empty() {
        clippy_cmd.arg("--features").arg(&feat);
    }
    clippy_cmd.args(["--", "-D", "warnings"]);
    let clippy_ok = clippy_cmd
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let mut build_cmd = Command::new("cargo");
    build_cmd.args(["build", "--release"]).current_dir(dir);
    if !features.is_empty() {
        build_cmd.arg("--features").arg(&feat);
    }
    let build = build_cmd
        .output()
        .map_err(|e| format!("cargo build --release failed to run: {e}"))?;
    if !build.status.success() {
        return Err(format!(
            "cargo build --release failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        ));
    }
    let binary_path = dir.join("target/release/selfware");
    let binary_size_mb = std::fs::metadata(&binary_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    let pass_ratio = if tests_total > 0 {
        tests_passed as f64 / tests_total as f64
    } else {
        0.0
    };
    let fmt_factor = if fmt_ok { 1.0 } else { 0.95 };
    let clippy_factor = if clippy_ok { 1.0 } else { 0.90 };
    let sab_score = 100.0 * pass_ratio * fmt_factor * clippy_factor;

    Ok(FitnessMetrics {
        sab_score,
        tokens_used: 0,
        token_budget: DEFAULT_TOKEN_BUDGET,
        wall_clock_secs: start.elapsed().as_secs_f64(),
        timeout_secs,
        test_coverage_pct: pass_ratio * 100.0,
        binary_size_mb,
        max_binary_size_mb: 50.0,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    })
}

/// Build FitnessMetrics for a candidate that has already passed compile, test,
/// fmt and clippy. Runs a release build to capture real binary size.
fn build_candidate_metrics(
    worktree: &Path,
    test_output: &std::process::Output,
    test_duration: std::time::Duration,
    features: &[&str],
    config: &EvolutionConfig,
) -> Option<FitnessMetrics> {
    let stdout = String::from_utf8_lossy(&test_output.stdout);
    let stderr = String::from_utf8_lossy(&test_output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    let (tests_passed, tests_total) = parse_test_summary(&combined);

    let feat = features_arg(features);
    let mut build_cmd = Command::new("cargo");
    build_cmd.args(["build", "--release"]).current_dir(worktree);
    if !features.is_empty() {
        build_cmd.arg("--features").arg(&feat);
    }
    let build = build_cmd.output().ok()?;
    if !build.status.success() {
        return None;
    }
    let binary_path = worktree.join("target/release/selfware");
    let binary_size_mb = std::fs::metadata(&binary_path)
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    let pass_ratio = if tests_total > 0 {
        tests_passed as f64 / tests_total as f64
    } else {
        0.0
    };

    Some(FitnessMetrics {
        sab_score: pass_ratio * 100.0,
        tokens_used: 0,
        token_budget: DEFAULT_TOKEN_BUDGET,
        wall_clock_secs: test_duration.as_secs_f64(),
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        test_coverage_pct: pass_ratio * 100.0,
        binary_size_mb,
        max_binary_size_mb: config.safety.max_binary_size_mb,
        tests_passed,
        tests_total,
        visual_score: 0.0,
    })
}

fn synthetic_baseline_metrics() -> FitnessMetrics {
    FitnessMetrics {
        sab_score: 50.0,
        tokens_used: 0,
        token_budget: DEFAULT_TOKEN_BUDGET,
        wall_clock_secs: 0.0,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        test_coverage_pct: 50.0,
        binary_size_mb: 15.0,
        max_binary_size_mb: 50.0,
        tests_passed: 0,
        tests_total: 0,
        visual_score: 0.0,
    }
}

/// Convert a SAB result into FitnessMetrics, preserving the real SAB score.
fn metrics_from_sab_result(
    sab: &SabResult,
    binary_path: &Path,
    max_binary_size_mb: f64,
) -> FitnessMetrics {
    let tests_passed = sab
        .scenario_scores
        .iter()
        .filter(|s| s.tests_passed)
        .count();
    let tests_total = sab.scenario_scores.len();
    let mut metrics = fitness::build_fitness_metrics(
        sab,
        DEFAULT_TOKEN_BUDGET,
        DEFAULT_TIMEOUT_SECS,
        binary_path,
        tests_passed,
        tests_total,
        max_binary_size_mb,
    );
    metrics.sab_score = sab.aggregate_score;
    metrics
}

/// Run the evolution daemon
pub async fn evolve(config: EvolutionConfig, repo_root: &Path) -> EvolutionResult {
    let start = Instant::now();
    let mut hall_of_fame: Vec<GenerationWinner> = Vec::new();
    let mut generation: usize = 0;

    // Clear previous log
    let _ = std::fs::write(repo_root.join(".evolution-log.jsonl"), "");
    log_event(
        repo_root,
        &serde_json::json!({
            "event": "start",
            "timestamp": chrono_now(),
            "generations": config.generations,
            "population_size": config.population_size,
            "endpoint": config.llm.endpoint,
            "model": config.llm.model,
        }),
    );

    // ═══════════════════════════════════════════════════════
    // MEASURE BASELINE
    // ═══════════════════════════════════════════════════════

    log_phase("Measuring baseline fitness...");
    let sab_config = SabConfig::default();
    let sab_mode = std::env::var("SELFWARE_EVOLVE_SAB").is_ok();

    // Only run SAB baseline if explicitly requested via env var
    // (SAB runs all 12 scenarios and takes 30+ minutes). Otherwise use real
    // compile / test / fmt / clippy / binary-size metrics.
    let baseline_metrics = if sab_mode {
        let selfware_binary = repo_root.join("target/release/selfware");
        match fitness::run_sab(&selfware_binary, &sab_config) {
            Ok(r) => {
                metrics_from_sab_result(&r, &selfware_binary, config.safety.max_binary_size_mb)
            }
            Err(e) => {
                log_warning(&format!(
                    "SAB baseline failed ({}), using synthetic baseline",
                    e
                ));
                synthetic_baseline_metrics()
            }
        }
    } else {
        log_phase("Using compile+test fitness (set SELFWARE_EVOLVE_SAB=1 for full SAB)");
        match measure_compile_test_baseline(repo_root, EVOLVE_FEATURES, DEFAULT_TIMEOUT_SECS) {
            Ok(mut m) => {
                m.max_binary_size_mb = config.safety.max_binary_size_mb;
                m
            }
            Err(e) => {
                log_warning(&format!(
                    "Compile/test baseline failed ({}), using synthetic baseline",
                    e
                ));
                synthetic_baseline_metrics()
            }
        }
    };

    let initial_sab = baseline_metrics.sab_score;
    let mut current_baseline_metrics = baseline_metrics;

    log_baseline(&current_baseline_metrics, sab_mode);

    // ═══════════════════════════════════════════════════════
    // MAIN EVOLUTIONARY LOOP
    // ═══════════════════════════════════════════════════════

    loop {
        generation += 1;
        if config.generations > 0 && generation > config.generations {
            break;
        }

        log_generation_start(generation);
        let gen_start = Instant::now();
        log_event(
            repo_root,
            &serde_json::json!({
                "event": "generation_start",
                "timestamp": chrono_now(),
                "generation": generation,
            }),
        );

        // ─── Step 1: Capture telemetry (sensory data for the agent) ───
        let telemetry_snapshot = telemetry::capture(repo_root, "sab_full").ok();
        let telemetry_prompt = telemetry_snapshot
            .as_ref()
            .map(telemetry::to_agent_prompt)
            .unwrap_or_default();

        let history_prompt = format_evolution_history(&hall_of_fame);

        // ─── Step 2: Generate hypotheses via agent swarm ───
        let llm_start = Instant::now();
        let hypotheses =
            generate_hypotheses(&config, &telemetry_prompt, &history_prompt, repo_root).await;

        log_event(
            repo_root,
            &serde_json::json!({
                "event": "hypotheses_generated",
                "timestamp": chrono_now(),
                "generation": generation,
                "count": hypotheses.len(),
                "descriptions": hypotheses.iter().map(|h| &h.description).collect::<Vec<_>>(),
                "llm_duration_secs": llm_start.elapsed().as_secs_f64(),
            }),
        );

        if hypotheses.is_empty() {
            log_warning("No valid hypotheses generated, retrying...");
            // Backoff to avoid 100% CPU busy-loop when the LLM returns nothing.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        // ─── Step 3: Safety filter ───
        // Gate on the paths the patch ACTUALLY edits, not the LLM-declared
        // target_files metadata (free-form JSON, never cross-checked — an
        // empty array passed trivially). See hypothesis_touches_protected.
        let valid: Vec<_> = hypotheses
            .into_iter()
            .filter(|h| {
                if hypothesis_touches_protected(h) {
                    log_warning(&format!(
                        "Hypothesis '{}' touches protected files, rejected",
                        h.id
                    ));
                    return false;
                }
                true
            })
            .collect();

        if valid.is_empty() {
            log_warning("All hypotheses rejected by safety filter");
            // Backoff to avoid 100% CPU busy-loop.
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            continue;
        }

        log_phase(&format!("Evaluating {} hypotheses...", valid.len()));

        // ─── Step 4: Evaluate each hypothesis (apply → check → test) ───
        let sab_available =
            sab_config.runner_script.exists() && std::env::var("SELFWARE_EVOLVE_SAB").is_ok();
        let mut generation_winner: Option<(Hypothesis, FitnessMetrics, String)> = None;

        for hypothesis in &valid {
            log_phase(&format!(
                "  Testing '{}' [{}]...",
                hypothesis.description, hypothesis.id
            ));

            // Create worktree. The guard removes it on EVERY exit path from
            // this iteration — success, early `continue`, and panic unwind —
            // where the old manual `cleanup_worktree` calls leaked worktrees
            // under `.worktrees/` whenever a panic (e.g. the UTF-8 byte-slice
            // panics) aborted the iteration.
            let worktree = match ast_tools::create_shadow_worktree(repo_root) {
                Ok(w) => w,
                Err(e) => {
                    log_warning(&format!("  Worktree failed: {}", e));
                    continue;
                }
            };
            let _worktree_guard = WorktreeGuard::new(repo_root, worktree.clone());

            // Apply edits (search-and-replace or unified diff)
            if !apply_patch_to_worktree(&worktree, &hypothesis.patch) {
                log_frost(generation, &format!("Patch failed: {}", hypothesis.id));
                // Log the first 500 chars of the edit data for debugging
                let preview = truncate_char_boundary(&hypothesis.patch, 500);
                log_warning(&format!("  Edit preview:\n{}", preview));
                continue;
            }

            // Compile check
            let mut check_cmd = Command::new("cargo");
            check_cmd
                .arg("check")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .current_dir(&worktree);
            let check = check_cmd.output();

            if check.map(|o| !o.status.success()).unwrap_or(true) {
                log_frost(generation, &format!("Compile failed: {}", hypothesis.id));
                continue;
            }

            // Run tests
            let test_start = Instant::now();
            let mut test_cmd = Command::new("cargo");
            test_cmd
                .arg("test")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .current_dir(&worktree);
            let test = test_cmd.output();

            let test_output = match test {
                Ok(o) => o,
                Err(e) => {
                    log_warning(&format!("  Test execution failed: {}", e));
                    continue;
                }
            };

            let test_passed = test_output.status.success();
            let test_duration = test_start.elapsed();

            if !test_passed {
                let stderr = String::from_utf8_lossy(&test_output.stderr);
                let fail_count = stderr
                    .lines()
                    .find(|l| l.contains("test result:"))
                    .unwrap_or("unknown");
                log_frost(
                    generation,
                    &format!("Tests failed: {} — {}", hypothesis.id, fail_count),
                );
                continue;
            }

            // Format check — prevent committing code that violates `cargo fmt`
            let fmt_check = Command::new("cargo")
                .args(["fmt", "--", "--check"])
                .current_dir(&worktree)
                .output();

            if fmt_check.map(|o| !o.status.success()).unwrap_or(true) {
                log_warning(&format!(
                    "  {} failed fmt check — auto-formatting",
                    hypothesis.id
                ));
                // Auto-fix: run cargo fmt to correct formatting
                let _ = Command::new("cargo")
                    .args(["fmt"])
                    .current_dir(&worktree)
                    .output();
            }

            // Clippy lint gate — reject code with clippy warnings
            let mut clippy_cmd = Command::new("cargo");
            clippy_cmd
                .arg("clippy")
                .arg("--features")
                .arg(features_arg(EVOLVE_FEATURES))
                .current_dir(&worktree);
            clippy_cmd.args(["--", "-D", "warnings"]);
            let clippy = clippy_cmd.output();

            if clippy.map(|o| !o.status.success()).unwrap_or(true) {
                log_frost(generation, &format!("Clippy failed: {}", hypothesis.id));
                continue;
            }

            // Compute real fitness metrics. If SAB is available, run the full
            // benchmark; otherwise derive compile/test/binary-size metrics.
            let winner_metrics = if sab_available {
                let build = Command::new("cargo")
                    .args(["build", "--release", "--features", "self-improvement"])
                    .current_dir(&worktree)
                    .output();

                if build.map(|o| !o.status.success()).unwrap_or(true) {
                    log_frost(
                        generation,
                        &format!("Release build failed: {}", hypothesis.id),
                    );
                    continue;
                }

                let mutated_binary = worktree.join("target/release/selfware");
                match fitness::run_sab(&mutated_binary, &sab_config) {
                    Ok(r) => metrics_from_sab_result(
                        &r,
                        &mutated_binary,
                        config.safety.max_binary_size_mb,
                    ),
                    Err(e) => {
                        log_warning(&format!("  SAB failed: {}", e));
                        continue;
                    }
                }
            } else {
                match build_candidate_metrics(
                    &worktree,
                    &test_output,
                    test_duration,
                    EVOLVE_FEATURES,
                    &config,
                ) {
                    Some(m) => m,
                    None => {
                        log_frost(
                            generation,
                            &format!("Release build failed: {}", hypothesis.id),
                        );
                        continue;
                    }
                }
            };

            // Capture the EXACT tested state as a diff against HEAD before the
            // worktree guard cleans up. This includes the `cargo fmt` auto-fix
            // above, which the raw LLM patch lacks — committing this diff (not
            // the raw patch) is what makes the committed state match the
            // tested state, and strict-applying it later avoids `patch -F3`
            // fuzz landing hunks somewhere other than where they were tested.
            let tested_diff = match capture_tested_diff(&worktree) {
                Some(d) if !d.trim().is_empty() => d,
                _ => {
                    log_frost(
                        generation,
                        &format!("No effective diff after evaluation: {}", hypothesis.id),
                    );
                    continue;
                }
            };

            log_phase(&format!(
                "  ✓ '{}' passed (score: {:.0}, {:.1}s)",
                hypothesis.description, winner_metrics.sab_score, winner_metrics.wall_clock_secs
            ));

            // Keep the first passing hypothesis as winner
            if generation_winner.is_none() {
                generation_winner = Some((hypothesis.clone(), winner_metrics, tested_diff));
            }
        }

        // ─── Step 5: EMERGE OR DIE ───
        let (winner, winner_metrics, tested_diff) = match generation_winner {
            Some(w) => w,
            None => {
                log_frost(generation, "No hypotheses survived evaluation");
                log_event(
                    repo_root,
                    &serde_json::json!({
                        "event": "generation_end",
                        "timestamp": chrono_now(),
                        "generation": generation,
                        "outcome": "frost",
                        "reason": "no hypotheses survived",
                        "duration_secs": gen_start.elapsed().as_secs_f64(),
                    }),
                );
                continue;
            }
        };

        let baseline_composite = config.fitness_weights.composite(&current_baseline_metrics);
        let winner_composite = config.fitness_weights.composite(&winner_metrics);

        if winner_composite > baseline_composite {
            log_bloom(
                generation,
                &winner.description,
                current_baseline_metrics.sab_score,
                winner_metrics.sab_score,
            );

            let commit_msg = format!(
                "🧬 Gen {} BLOOM: {:.0} → {:.0} | {}",
                generation,
                current_baseline_metrics.sab_score,
                winner_metrics.sab_score,
                winner.description
            );
            // Apply the EXACT tested diff (not the raw LLM patch) and commit
            // ONLY the paths it edits — never `git add -A`, which swept every
            // dirty edit and untracked file (.env, scratch, credentials) into
            // the BLOOM commit on whatever branch was checked out.
            if commit_winner_to_repo(repo_root, &tested_diff, &commit_msg) {
                let git_tag = if generation.is_multiple_of(config.checkpoint_interval) {
                    let tag = format!("evolve-gen-{}", generation);
                    let _ = Command::new("git")
                        .args(["tag", &tag])
                        .current_dir(repo_root)
                        .output();
                    Some(tag)
                } else {
                    None
                };

                hall_of_fame.push(GenerationWinner {
                    generation,
                    description: winner.description.clone(),
                    composite_score: winner_composite,
                    sab_delta: winner_metrics.sab_score - current_baseline_metrics.sab_score,
                    token_delta: winner_metrics.tokens_used as f64
                        - current_baseline_metrics.tokens_used as f64,
                    // The tested diff actually committed (incl. fmt fixes),
                    // not the raw LLM patch.
                    patch: tested_diff.clone(),
                    git_tag,
                });

                log_event(
                    repo_root,
                    &serde_json::json!({
                        "event": "generation_end",
                        "timestamp": chrono_now(),
                        "generation": generation,
                        "outcome": "bloom",
                        "description": winner.description,
                        "score_before": current_baseline_metrics.sab_score,
                        "score_after": winner_metrics.sab_score,
                        "composite": winner_composite,
                        "duration_secs": gen_start.elapsed().as_secs_f64(),
                        "improvements_total": hall_of_fame.len(),
                    }),
                );

                current_baseline_metrics = winner_metrics;
            }
        } else {
            let rating = if winner_composite < baseline_composite * 0.9 {
                GenerationRating::Frost
            } else {
                GenerationRating::Wilt
            };
            log_reject(
                generation,
                &rating,
                winner_metrics.sab_score,
                current_baseline_metrics.sab_score,
            );
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "generation_end",
                    "timestamp": chrono_now(),
                    "generation": generation,
                    "outcome": format!("{}", rating),
                    "description": winner.description,
                    "winner_score": winner_metrics.sab_score,
                    "baseline_score": current_baseline_metrics.sab_score,
                    "duration_secs": gen_start.elapsed().as_secs_f64(),
                }),
            );
        }
    }

    EvolutionResult {
        generations_run: generation,
        improvements: hall_of_fame,
        final_sab_score: current_baseline_metrics.sab_score,
        initial_sab_score: initial_sab,
        total_duration: start.elapsed(),
    }
}

// ═══════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════

async fn generate_hypotheses(
    config: &EvolutionConfig,
    telemetry_prompt: &str,
    history_prompt: &str,
    repo_root: &Path,
) -> Vec<Hypothesis> {
    // Check if we should use micro mode for small models
    let use_micro_mode = super::micro_mode::is_micro_model(&config.llm.model);

    if use_micro_mode {
        log_phase("Micro mode: Using simplified prompts for small model");
        generate_micro_hypotheses(config, telemetry_prompt, history_prompt, repo_root).await
    } else {
        generate_standard_hypotheses(config, telemetry_prompt, history_prompt, repo_root).await
    }
}

async fn generate_standard_hypotheses(
    config: &EvolutionConfig,
    telemetry_prompt: &str,
    history_prompt: &str,
    repo_root: &Path,
) -> Vec<Hypothesis> {
    let source_context = read_mutation_targets(&config.mutation_targets, repo_root);
    if source_context.is_empty() {
        log_warning("No mutation target files found or readable");
        return vec![];
    }

    let system_prompt = build_system_prompt(config.population_size);
    let user_prompt = build_user_prompt(telemetry_prompt, history_prompt, &source_context);

    match call_llm(&config.llm, &system_prompt, &user_prompt).await {
        Ok(response) => {
            log_phase(&format!(
                "LLM response ({} chars): {}",
                response.len(),
                truncate_char_boundary(&response, 200)
            ));
            parse_hypotheses_response(&response)
        }
        Err(e) => {
            log_warning(&format!("LLM call failed: {}", e));
            vec![]
        }
    }
}

async fn generate_micro_hypotheses(
    config: &EvolutionConfig,
    telemetry_prompt: &str,
    history_prompt: &str,
    repo_root: &Path,
) -> Vec<Hypothesis> {
    use super::micro_mode;

    // Collect all target paths
    let all_paths: Vec<PathBuf> = config
        .mutation_targets
        .prompt_logic
        .iter()
        .chain(config.mutation_targets.tool_code.iter())
        .chain(config.mutation_targets.cognitive.iter())
        .cloned()
        .collect();

    // Select small subset of files
    let selected = micro_mode::select_micro_targets(&all_paths, repo_root);
    if selected.is_empty() {
        log_warning("Micro mode: No suitable target files found");
        return vec![];
    }

    log_phase(&format!(
        "Micro mode: Using {} files ({} chars)",
        selected.len(),
        selected.iter().map(|(_, c)| c.len()).sum::<usize>()
    ));

    let source_context = micro_mode::build_micro_context(&selected);
    let micro_population = config.population_size.min(3); // Clamp for micro mode

    let system_prompt = micro_mode::build_micro_system_prompt(micro_population);
    let user_prompt = build_user_prompt(telemetry_prompt, history_prompt, &source_context);

    match call_llm(&config.llm, &system_prompt, &user_prompt).await {
        Ok(response) => {
            log_phase(&format!(
                "Micro mode: LLM response ({} chars)",
                response.len()
            ));
            let hypotheses = parse_hypotheses_response(&response);

            // Validate micro-safety
            hypotheses
                .into_iter()
                .filter(|h| match micro_mode::validate_micro_hypothesis(h) {
                    Ok(()) => true,
                    Err(e) => {
                        log_warning(&format!("Micro mode: Rejected hypothesis: {}", e));
                        false
                    }
                })
                .take(micro_population)
                .collect()
        }
        Err(e) => {
            log_warning(&format!("Micro mode: LLM call failed: {}", e));
            vec![]
        }
    }
}

/// Max total source context characters
const MAX_CONTEXT_CHARS: usize = 45_000;

/// Extract function signatures from Rust source code
fn extract_function_signatures(source: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let mut in_impl_block = false;
    let mut impl_context = String::new();

    for line in source.lines() {
        let trimmed = line.trim();

        // Track impl blocks for context
        if trimmed.starts_with("impl ") || trimmed.starts_with("pub impl ") {
            in_impl_block = true;
            impl_context = trimmed.to_string();
            continue;
        }
        if trimmed == "}" && in_impl_block {
            in_impl_block = false;
            impl_context.clear();
            continue;
        }

        // Match function signatures (fn or pub fn)
        if (trimmed.starts_with("fn ") || trimmed.starts_with("pub fn "))
            && !trimmed.starts_with("fn main()")
        // Skip main functions in tests
        {
            let mut sig = trimmed.to_string();

            // Add impl context if available
            if !impl_context.is_empty() {
                sig = format!("// In: {}\n{}", impl_context, sig);
            }

            // Extract just the signature line (stop at opening brace)
            if let Some(brace_pos) = sig.find('{') {
                sig = sig[..brace_pos].to_string();
            }

            if !sig.is_empty() {
                signatures.push(sig);
            }
        }
    }

    signatures
}

/// Get recent git changes for context
fn get_recent_git_changes(repo_root: &Path, max_commits: usize) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "--no-merges"])
        .arg(format!("-{}", max_commits))
        .current_dir(repo_root)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            let log = String::from_utf8_lossy(&o.stdout);
            if log.trim().is_empty() {
                None
            } else {
                Some(format!("Recent commits:\n{}", log))
            }
        }
        _ => None,
    }
}

/// Resolve `candidate` against `base`, guaranteeing the result stays INSIDE
/// `base`. Returns `None` for an absolute path or any `..` sequence that would
/// escape. Used to contain both config-supplied mutation targets (read) and
/// model-produced edit paths (write) so `selfware evolve` can never read or
/// overwrite files outside the repository. Purely lexical (no canonicalize) so
/// it also works for not-yet-existing write targets.
fn contained_path(base: &Path, candidate: &Path) -> Option<PathBuf> {
    use std::path::Component;
    if candidate.is_absolute() {
        return None;
    }
    let mut result = base.to_path_buf();
    for comp in candidate.components() {
        match comp {
            Component::Normal(c) => result.push(c),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() || !result.starts_with(base) {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    result.starts_with(base).then_some(result)
}

pub fn read_mutation_targets(targets: &super::MutationTargets, repo_root: &Path) -> String {
    // Collect all files with their sizes, then sort smallest-first so we
    // maximise the number of files sent in full within the context budget.
    let all_paths: Vec<&PathBuf> = targets
        .prompt_logic
        .iter()
        .chain(targets.tool_code.iter())
        .chain(targets.cognitive.iter())
        .collect();

    let mut file_entries: Vec<(&PathBuf, String, usize, Vec<String>)> = Vec::new();
    for file in &all_paths {
        // Contain the target inside the repo: an `[evolution]` config entry that
        // is absolute or uses `..` must not read arbitrary host files into the
        // model prompt.
        let Some(full_path) = contained_path(repo_root, file) else {
            log_warning(&format!(
                "Refusing mutation target outside the repository: {}",
                file.display()
            ));
            continue;
        };
        match std::fs::read_to_string(&full_path) {
            Ok(contents) => {
                let len = contents.len();
                let signatures = extract_function_signatures(&contents);
                file_entries.push((file, contents, len, signatures));
            }
            Err(e) => {
                log_warning(&format!("Could not read {}: {}", file.display(), e));
            }
        }
    }

    // Sort by size ascending — small files go in full, big files get truncated
    file_entries.sort_by_key(|(_, _, len, _)| *len);

    let mut context = String::new();
    let mut files_full = 0usize;
    let mut files_truncated = 0usize;
    let mut total_signatures = 0usize;

    // Add recent git changes at the top for context
    if let Some(recent_changes) = get_recent_git_changes(repo_root, 10) {
        context.push_str(&format!("## Recent Git Changes\n{}\n\n", recent_changes));
    }

    for (file, contents, _len, signatures) in &file_entries {
        let remaining = MAX_CONTEXT_CHARS.saturating_sub(context.len());
        if remaining < 500 {
            log_warning(&format!(
                "Context limit reached ({} chars), skipping remaining files",
                context.len()
            ));
            break;
        }

        // Add line numbers to source — helps the LLM generate accurate @@ hunk headers
        let numbered = add_line_numbers(contents);

        // Budget for this file: overhead for the header + fences + signatures (~200 chars)
        let sig_overhead = signatures.len() * 50;
        let overhead = 200 + file.display().to_string().len() + sig_overhead;
        let budget = remaining.saturating_sub(overhead);

        let (display_content, was_truncated) = if numbered.len() <= budget {
            (numbered, false)
        } else {
            // Truncate to budget on a line boundary
            let truncated = truncate_to_line_boundary(&numbered, budget);
            let total_lines = contents.lines().count();
            let shown_lines = truncated.lines().count();
            (
                format!(
                    "{}\n// ... [truncated at line {}/{}, {} total chars]",
                    truncated,
                    shown_lines,
                    total_lines,
                    contents.len()
                ),
                true,
            )
        };

        if was_truncated {
            files_truncated += 1;
        } else {
            files_full += 1;
        }

        total_signatures += signatures.len();

        // Add function signatures as a summary before the source
        let sig_summary = if !signatures.is_empty() {
            let sigs: String = signatures
                .iter()
                .take(20) // Limit to 20 signatures per file
                .map(|s| format!("  {}", s))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "\n// Function signatures ({} total):\n{}\n",
                signatures.len(),
                sigs
            )
        } else {
            String::new()
        };

        context.push_str(&format!(
            "\n### {}\n{}\n```rust\n{}\n```\n",
            file.display(),
            sig_summary,
            display_content
        ));
    }
    log_phase(&format!(
        "Source context: {} chars from {} files ({} full, {} truncated, {} signatures)",
        context.len(),
        files_full + files_truncated,
        files_full,
        files_truncated,
        total_signatures,
    ));
    context
}

/// Add line numbers to source code (e.g. "  1| fn main() {")
fn add_line_numbers(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let width = format!("{}", lines.len()).len();
    let mut out = String::with_capacity(source.len() + lines.len() * (width + 2));
    for (i, line) in lines.iter().enumerate() {
        out.push_str(&format!("{:>width$}| {}\n", i + 1, line, width = width));
    }
    out
}

/// Truncate a string to at most `max_chars` on a line boundary
fn truncate_to_line_boundary(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }
    // Back off to a UTF-8 char boundary FIRST: a raw `&s[..max_chars]` slice
    // panics when max_chars lands mid-codepoint (multibyte source), which used
    // to abort the whole evolve run before any LLM call was even made.
    let s = truncate_char_boundary(s, max_chars);
    // Find the last newline before the limit
    match s.rfind('\n') {
        Some(pos) => &s[..pos],
        None => s,
    }
}

/// Byte-truncate `s` to at most `max_bytes`, backing off to a UTF-8 char
/// boundary. A raw `&s[..n]` byte slice PANICS when `n` lands mid-codepoint —
/// the same class of bug fixed in `agent/checkpointing.rs`
/// (`truncate_bytes_char_boundary`); duplicated here because that helper is
/// feature-gated and private to the checkpointing module.
fn truncate_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn build_system_prompt(population_size: usize) -> String {
    format!(
        r#"You are an evolution engine that generates code mutation hypotheses for a Rust project called selfware.

Your task is to propose exactly {n} mutation hypotheses as improvements. Each hypothesis uses search-and-replace edits.

SOURCE CODE FORMAT:
- Each file is shown with line numbers like "  42| fn foo() {{"
- Line numbers are for your reference only — do NOT include them in search/replace strings
- Some files are truncated — only modify code you can see in full
- CRITICAL: The search string must match EXACT line content from the source files, NOT paraphrased or reformatted code

EDIT FORMAT (critical — edits that can't be found are discarded):
- Each hypothesis has an "edits" array of search-and-replace operations
- "search" must be an EXACT substring of the target file (copy-paste accuracy required)
- "replace" is what replaces that exact substring
- Keep edits small and focused — change the minimum necessary code
- The search string must be unique in the file (not ambiguous)
- Use \n for newlines inside strings (JSON escaped)
- Do NOT include line number prefixes (like "42| ") in search/replace strings
- CRITICAL: When constructing search strings, copy the EXACT text from the source code including:
  - Exact whitespace (spaces vs tabs, indentation level)
  - Exact punctuation and formatting
  - Exact line breaks
  - Do NOT reformat or paraphrase the code you're searching for

RULES:
1. Each hypothesis must target files from the provided source code
2. Never modify files under src/evolution/, src/safety/, system_tests/, or benches/sab_
3. Focus on: bug fixes, performance improvements, correctness, reducing allocations
4. Each hypothesis must be independent — do not assume other hypotheses are applied
5. Only modify code you can fully see — never guess at truncated content

Respond with a JSON array of exactly {n} objects:
- "description": string — what the change does and why
- "edits": array of {{"file": "relative/path.rs", "search": "exact old text", "replace": "new text"}}
- "target_files": string array — relative paths of files changed
- "property_test": string or null — optional property test

Return ONLY the JSON array. No markdown, no commentary, no thinking.

/no_think"#,
        n = population_size
    )
}

pub fn build_user_prompt(telemetry: &str, history: &str, source_context: &str) -> String {
    let mut prompt = String::new();

    if !telemetry.is_empty() {
        prompt.push_str("## Current Telemetry\n\n");
        prompt.push_str(telemetry);
        prompt.push_str("\n\n");
    }

    if !history.is_empty() {
        prompt.push_str(history);
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Source Code (mutation targets)\n");
    prompt.push_str(source_context);

    prompt
}

async fn call_llm(
    llm: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    crate::config::api_key::assert_credential_endpoint_safe(&llm.endpoint, llm.api_key.is_some())
        .map_err(|e| e.to_string())?;
    let url = format!("{}/chat/completions", llm.endpoint.trim_end_matches('/'));

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    if let Some(ref key) = llm.api_key {
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_str(&format!("Bearer {}", key))
                .map_err(|e| format!("Invalid API key header: {}", e))?,
        );
    }

    let body = serde_json::json!({
        "model": llm.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": llm.max_tokens,
        "temperature": llm.temperature,
        // Disable Qwen3's thinking mode to maximize output tokens for JSON
        "chat_template_kwargs": {"enable_thinking": false},
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("LLM API returned {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse LLM response JSON: {}", e))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in LLM response".to_string())
}

pub fn parse_hypotheses_response(response: &str) -> Vec<Hypothesis> {
    // Find JSON array in the response — handles markdown fences, thinking, preamble
    let json_str = match extract_json_array(response) {
        Some(s) => s,
        None => {
            log_warning("Could not find JSON array in LLM response");
            return vec![];
        }
    };

    let parsed: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log_warning(&format!("Failed to parse hypotheses JSON: {}", e));
            return vec![];
        }
    };

    parsed
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let description = v["description"].as_str()?.to_string();
            let target_files: Vec<PathBuf> = v["target_files"]
                .as_array()?
                .iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect();
            let property_test = v["property_test"].as_str().map(|s| s.to_string());

            // Support both formats:
            // 1. New: "edits" array of {file, search, replace}
            // 2. Legacy: "patch" string (unified diff)
            let patch = if let Some(edits) = v["edits"].as_array() {
                // Serialize edits as JSON for the patch field
                serde_json::to_string(edits).ok()?
            } else {
                // Fallback to legacy unified diff format
                v["patch"].as_str()?.to_string()
            };

            if patch.is_empty() {
                return None;
            }

            Some(Hypothesis {
                id: format!("hyp-{}", i),
                description,
                patch,
                target_files,
                property_test,
            })
        })
        .collect()
}

fn extract_json_array(text: &str) -> Option<String> {
    // Try to find a JSON array, handling markdown fences
    let text = text.trim();

    // Strip markdown code fences if present
    let stripped = if text.contains("```") {
        let mut inside_fence = false;
        let mut content = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                inside_fence = !inside_fence;
                continue;
            }
            if inside_fence {
                content.push_str(line);
                content.push('\n');
            }
        }
        if content.is_empty() {
            text.to_string()
        } else {
            content
        }
    } else {
        text.to_string()
    };

    // Find the first '[' and its matching ']'
    let start = stripped.find('[')?;
    let mut depth = 0;
    let mut end = None;
    for (i, ch) in stripped[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i + 1);
                    break;
                }
            }
            _ => {}
        }
    }

    end.map(|e| stripped[start..e].to_string())
}

pub fn format_evolution_history(hall_of_fame: &[GenerationWinner]) -> String {
    if hall_of_fame.is_empty() {
        return String::from("No evolution history yet. This is generation 1.");
    }

    let mut prompt = String::from("## Evolution History (most recent first)\n\n");
    for winner in hall_of_fame.iter().rev().take(10) {
        prompt.push_str(&format!(
            "- Gen {}: {} (SAB +{:.1}, tokens {:.0})\n",
            winner.generation, winner.description, winner.sab_delta, winner.token_delta
        ));
    }
    prompt
}

/// Strip line-number prefixes the LLM may have left in the patch.
/// Matches patterns like "  42| " at the start of context/add/delete lines.
fn sanitize_patch(patch: &str) -> String {
    let mut out = String::with_capacity(patch.len());
    for line in patch.lines() {
        // Hunk headers, file headers — pass through unchanged
        if line.starts_with("@@")
            || line.starts_with("---")
            || line.starts_with("+++")
            || line.starts_with("diff ")
        {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        // Context/add/delete lines: strip line-number prefix if present
        // Patterns:  " 123| code", "+  45| code", "- 789| code"
        let (prefix, rest) =
            if let Some(r) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) {
                (&line[..1], r)
            } else if let Some(r) = line.strip_prefix(' ') {
                (" ", r)
            } else {
                // Unrecognized line — pass through
                out.push_str(line);
                out.push('\n');
                continue;
            };

        // Check if `rest` looks like "  NNN| actual_code"
        let stripped = rest.trim_start();
        if let Some(pipe_pos) = stripped.find('|') {
            let before_pipe = &stripped[..pipe_pos];
            if !before_pipe.is_empty() && before_pipe.chars().all(|c| c.is_ascii_digit()) {
                // It's a line-number prefix — strip "NNN| " and keep the rest
                let after_pipe = &stripped[pipe_pos + 1..];
                // The format is "NNN| code" — there's exactly one space after |
                let code = after_pipe.strip_prefix(' ').unwrap_or(after_pipe);
                out.push_str(prefix);
                out.push_str(code);
                out.push('\n');
                continue;
            }
        }

        // No line-number prefix — pass through unchanged
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Extract the paths a mutation payload ACTUALLY edits, for the
/// protected-path gate. The hypothesis `target_files` field is LLM-declared
/// metadata that is never cross-checked against the payload — an empty or
/// benign list passes trivially while the patch edits anything (whole-repo
/// review, Evolution P1). This parses the payload itself:
/// 1. Search-and-replace JSON edits: every entry's `file` field.
/// 2. Unified diffs: every `+++ b/<path>` header, plus `--- a/<path>` for
///    pure deletions (whose `+++` target is `/dev/null`).
fn patch_edited_paths(patch: &str) -> Vec<PathBuf> {
    // Search-and-replace format (mirrors apply_edits' dispatch).
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            return edits
                .iter()
                .filter_map(|e| e["file"].as_str().map(PathBuf::from))
                .collect();
        }
    }

    // Unified diff.
    let mut paths = Vec::new();
    for line in patch.lines() {
        let (header, prefix) = if let Some(rest) = line.strip_prefix("+++ ") {
            (rest, "b/")
        } else if let Some(rest) = line.strip_prefix("--- ") {
            (rest, "a/")
        } else {
            continue;
        };
        let p = header.trim().trim_start_matches(prefix);
        if !p.is_empty() && p != "/dev/null" {
            let path = PathBuf::from(p);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// The protected-path gate for a hypothesis. Enforced on the paths the
/// patch ACTUALLY edits (see [`patch_edited_paths`]); the LLM-declared
/// `target_files` metadata is kept only as an additional signal.
fn hypothesis_touches_protected(h: &Hypothesis) -> bool {
    h.target_files.iter().any(|f| is_protected(f))
        || patch_edited_paths(&h.patch).iter().any(|f| is_protected(f))
}

/// Apply edits to a directory. The `patch` field may be:
/// 1. A JSON array of {file, search, replace} edits (new format)
/// 2. A unified diff string (legacy format)
pub fn apply_edits(dir: &Path, patch: &str) -> bool {
    // Try search-and-replace format first
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            return apply_search_replace(dir, &edits);
        }
    }

    // Fall back to unified diff with progressive strategies
    apply_unified_diff(dir, patch)
}

/// Apply search-and-replace edits: for each edit, find the `search` string
/// in the file and replace it with `replace`. Supports fuzzy whitespace matching.
fn apply_search_replace(dir: &Path, edits: &[serde_json::Value]) -> bool {
    // Collect all edits per file, then apply them all at once
    let mut file_edits: std::collections::HashMap<String, Vec<(&str, &str)>> =
        std::collections::HashMap::new();

    for edit in edits {
        let file = match edit["file"].as_str() {
            Some(f) => f,
            None => return false,
        };
        let search = match edit["search"].as_str() {
            Some(s) => s,
            None => return false,
        };
        let replace = match edit["replace"].as_str() {
            Some(r) => r,
            None => return false,
        };
        file_edits
            .entry(file.to_string())
            .or_default()
            .push((search, replace));
    }

    for (file, edits) in &file_edits {
        // Contain the model-produced edit path inside the working dir: an
        // absolute or `../` path must not read or overwrite host files.
        let Some(path) = contained_path(dir, Path::new(file)) else {
            log_warning(&format!(
                "Refusing edit to path outside the working directory: {}",
                file
            ));
            return false;
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        let mut modified = content.clone();
        for (search, replace) in edits {
            // Try exact match first
            if modified.contains(search) {
                let count = modified.matches(search).count();
                if count > 1 {
                    log_warning(&format!(
                        "  Ambiguous search string in {} ({} matches): {:?}...",
                        file,
                        count,
                        truncate_char_boundary(search, 80)
                    ));
                    return false;
                }
                modified = modified.replacen(search, replace, 1);
                continue;
            }

            // Fuzzy match: try matching by trimmed line content (ignores whitespace diffs)
            match fuzzy_find_and_replace(&modified, search, replace) {
                Some(new_content) => {
                    modified = new_content;
                    continue;
                }
                None => {
                    log_warning(&format!(
                        "  Search string not found in {}: {:?}...",
                        file,
                        truncate_char_boundary(search, 80)
                    ));
                    return false;
                }
            }
        }

        if modified == content {
            log_warning(&format!("  No changes made to {}", file));
            return false;
        }

        if std::fs::write(&path, &modified).is_err() {
            return false;
        }
    }
    true
}

/// Find `search` in `content` with fuzzy whitespace matching, then replace
/// with `replace` (adjusted to match the file's original indentation).
/// Returns the modified content, or None if no match found.
fn fuzzy_find_and_replace(content: &str, search: &str, replace: &str) -> Option<String> {
    let search_lines: Vec<&str> = search.lines().collect();
    if search_lines.is_empty() {
        return None;
    }

    let content_lines: Vec<&str> = content.lines().collect();
    let first_trimmed = search_lines[0].trim();
    if first_trimmed.is_empty() {
        return None;
    }

    // Scan content lines for a match
    for start_idx in 0..content_lines.len() {
        let content_trimmed = content_lines[start_idx].trim();
        if content_trimmed != first_trimmed {
            continue;
        }

        // Check if all subsequent search lines match (trimmed)
        if start_idx + search_lines.len() > content_lines.len() {
            continue;
        }

        let mut all_match = true;
        for (j, search_line) in search_lines.iter().enumerate() {
            let cl = content_lines[start_idx + j].trim();
            let sl = search_line.trim();
            if cl != sl {
                all_match = false;
                break;
            }
        }

        if !all_match {
            continue;
        }

        // Found a match! Now compute the indentation offset.
        // The file's indentation for the first matched line vs the search's indentation.
        let file_indent = leading_whitespace(content_lines[start_idx]);
        let search_indent = leading_whitespace(search_lines[0]);

        // Build the replacement with adjusted indentation
        let replace_lines: Vec<&str> = replace.lines().collect();
        let mut adjusted_replace = String::new();
        for (k, rline) in replace_lines.iter().enumerate() {
            let rline_trimmed_start = rline.trim_start();
            if rline_trimmed_start.is_empty() {
                adjusted_replace.push('\n');
                continue;
            }
            let replace_indent = leading_whitespace(rline);
            // If the replace line has the search indent as a base, rebase to file indent
            let new_indent = if let Some(extra) = replace_indent.strip_prefix(search_indent) {
                format!("{}{}", file_indent, extra)
            } else {
                // Can't rebase — use file_indent for first line, original for rest
                if k == 0 {
                    file_indent.to_string()
                } else {
                    replace_indent.to_string()
                }
            };
            adjusted_replace.push_str(&new_indent);
            adjusted_replace.push_str(rline_trimmed_start);
            adjusted_replace.push('\n');
        }

        // Remove trailing newline if the search didn't end with one
        if !search.ends_with('\n') && adjusted_replace.ends_with('\n') {
            adjusted_replace.pop();
        }

        // Build the result: lines before + adjusted replace + lines after
        let mut result = String::new();
        for line in &content_lines[..start_idx] {
            result.push_str(line);
            result.push('\n');
        }
        result.push_str(&adjusted_replace);
        let end_idx = start_idx + search_lines.len();
        if end_idx < content_lines.len() {
            if !result.ends_with('\n') {
                result.push('\n');
            }
            for (k, line) in content_lines[end_idx..].iter().enumerate() {
                result.push_str(line);
                if end_idx + k + 1 < content_lines.len() {
                    result.push('\n');
                }
            }
        }

        // Preserve trailing newline if original had one
        if content.ends_with('\n') && !result.ends_with('\n') {
            result.push('\n');
        }

        return Some(result);
    }

    None
}

/// Extract the leading whitespace of a line
fn leading_whitespace(line: &str) -> &str {
    let trimmed = line.trim_start();
    &line[..line.len() - trimmed.len()]
}

/// Apply a unified diff with progressive fallback strategies:
/// 1. `git apply` (strict)
/// 2. `git apply --ignore-whitespace -C1`
/// 3. `patch -p1 -F3` (fuzz factor 3)
fn apply_unified_diff(dir: &Path, patch: &str) -> bool {
    let patch_file = dir.join(".evolution-patch");
    if std::fs::write(&patch_file, patch).is_err() {
        return false;
    }

    // Strategy 1: strict git apply
    let strict = Command::new("git")
        .args(["apply", ".evolution-patch"])
        .current_dir(dir)
        .output();
    if strict.map(|o| o.status.success()).unwrap_or(false) {
        let _ = std::fs::remove_file(&patch_file);
        return true;
    }

    // Strategy 2: git apply with relaxed whitespace and reduced context
    let relaxed = Command::new("git")
        .args(["apply", "--ignore-whitespace", "-C1", ".evolution-patch"])
        .current_dir(dir)
        .output();
    if relaxed.map(|o| o.status.success()).unwrap_or(false) {
        let _ = std::fs::remove_file(&patch_file);
        return true;
    }

    // Strategy 3: patch -p1 with fuzz factor 3
    let fuzz = Command::new("patch")
        .args([
            "-p1",
            "-F3",
            "--batch",
            "--silent",
            "-i",
            ".evolution-patch",
        ])
        .current_dir(dir)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    fuzz.map(|o| o.status.success()).unwrap_or(false)
}

fn apply_patch_to_worktree(worktree: &Path, patch: &str) -> bool {
    apply_edits(worktree, patch)
}

fn apply_patch_to_repo(repo_root: &Path, patch: &str) -> bool {
    // Final protected-path gate at the highest-blast-radius point: refuse
    // to apply ANY patch that edits protected files to the real repo, even
    // if a future caller bypasses the hypothesis safety filter.
    let edited = patch_edited_paths(patch);
    if let Some(p) = edited.iter().find(|f| is_protected(f)) {
        log_error(&format!(
            "Refusing to apply patch to repo: edits protected path {}",
            p.display()
        ));
        return false;
    }
    apply_edits(repo_root, patch)
}

/// RAII guard that removes a shadow worktree when the evaluation iteration
/// ends — on success, on early `continue`, AND on panic unwind. The old flow
/// called `cleanup_worktree` manually at each exit, so a panic anywhere in
/// the iteration leaked worktrees under `.worktrees/`.
struct WorktreeGuard<'a> {
    repo_root: &'a Path,
    path: PathBuf,
}

impl<'a> WorktreeGuard<'a> {
    fn new(repo_root: &'a Path, path: PathBuf) -> Self {
        Self { repo_root, path }
    }
}

impl Drop for WorktreeGuard<'_> {
    fn drop(&mut self) {
        let _ = ast_tools::cleanup_worktree(self.repo_root, &self.path);
    }
}

/// Capture the exact tested worktree state as a unified diff against HEAD.
///
/// Runs inside the throwaway shadow worktree, so `git add -A` stages into the
/// worktree's OWN index — never the user's. The captured diff includes the
/// `cargo fmt` auto-fix applied during evaluation, which the raw LLM patch
/// lacks. Applying THIS diff to the repo is what makes the committed state
/// byte-identical to the state that passed the gates.
fn capture_tested_diff(worktree: &Path) -> Option<String> {
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if !add.status.success() {
        return None;
    }
    let diff = Command::new("git")
        .args(["diff", "--cached", "HEAD"])
        .current_dir(worktree)
        .output()
        .ok()?;
    if !diff.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&diff.stdout).into_owned())
}

/// Apply the tested diff to the repo and commit ONLY the paths it edits.
/// On commit failure the apply is reverted so the user's worktree returns to
/// its pre-apply state instead of being left half-winner'd. Returns true only
/// when the winner is fully applied AND committed.
fn commit_winner_to_repo(repo_root: &Path, tested_diff: &str, commit_msg: &str) -> bool {
    if !apply_tested_diff_to_repo(repo_root, tested_diff) {
        return false;
    }
    let edited = patch_edited_paths(tested_diff);
    warn_unrelated_dirty_paths(repo_root, &edited);
    if commit_scoped_paths(repo_root, &edited, commit_msg) {
        return true;
    }
    log_error("Winner commit failed — reverting the applied diff to keep the worktree clean");
    revert_applied_diff(repo_root, tested_diff);
    false
}

/// Apply the tested diff to the real repository with STRICT `git apply` — no
/// fuzzy `patch -F3` fallback. Fuzz is what let the old flow land hunks in
/// different spots than the tested worktree; a strict apply either reproduces
/// the tested byte content exactly or fails (and failure skips the commit).
/// The protected-path gate runs on the tested diff itself.
fn apply_tested_diff_to_repo(repo_root: &Path, tested_diff: &str) -> bool {
    let edited = patch_edited_paths(tested_diff);
    if let Some(p) = edited.iter().find(|f| is_protected(f)) {
        log_error(&format!(
            "Refusing to apply tested diff to repo: edits protected path {}",
            p.display()
        ));
        return false;
    }
    let patch_file = repo_root.join(".evolution-tested.patch");
    if std::fs::write(&patch_file, tested_diff).is_err() {
        return false;
    }
    let applied = Command::new("git")
        .args(["apply", ".evolution-tested.patch"])
        .current_dir(repo_root)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    match applied {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            log_warning(&format!(
                "  Tested diff does not apply cleanly to the repo (worktree drift?): {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
            false
        }
        Err(e) => {
            log_warning(&format!("  Failed to run git apply: {}", e));
            false
        }
    }
}

/// Reverse-apply a previously applied tested diff — used when the scoped
/// commit fails — so the user's worktree returns to its pre-apply state.
fn revert_applied_diff(repo_root: &Path, tested_diff: &str) {
    let patch_file = repo_root.join(".evolution-tested.patch");
    if std::fs::write(&patch_file, tested_diff).is_err() {
        return;
    }
    let _ = Command::new("git")
        .args(["apply", "-R", ".evolution-tested.patch"])
        .current_dir(repo_root)
        .output();
    let _ = std::fs::remove_file(&patch_file);
}

/// Warn loudly about dirty/untracked paths in the user's worktree that are
/// UNRELATED to the winner patch. They are left uncommitted — the evolution
/// commit only ever stages the paths the tested diff edits.
fn warn_unrelated_dirty_paths(repo_root: &Path, edited: &[PathBuf]) {
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output();
    let Ok(status) = status else { return };
    if !status.status.success() {
        return;
    }
    let stdout = String::from_utf8_lossy(&status.stdout);
    let unrelated: Vec<&str> = stdout
        .lines()
        // Porcelain lines are `XY <path>` (path starts at byte 3); renames
        // show `orig -> new` — good enough for a warning list.
        .filter_map(|line| line.get(3..))
        .filter(|p| !p.is_empty())
        .filter(|p| !edited.iter().any(|e| e == Path::new(p)))
        .collect();
    if !unrelated.is_empty() {
        log_warning(&format!(
            "  {} unrelated dirty/untracked path(s) NOT included in the evolution commit: {}",
            unrelated.len(),
            unrelated
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

/// Stage and commit ONLY the given paths — never `git add -A`, which used to
/// sweep every dirty edit and untracked file (scratch, `.env`, credentials)
/// into the "🧬 Gen N BLOOM" commit on the user's current branch. The
/// pathspec form of `git commit` also keeps previously-staged unrelated
/// changes out of the commit.
fn commit_scoped_paths(repo_root: &Path, paths: &[PathBuf], commit_msg: &str) -> bool {
    if paths.is_empty() {
        log_warning("  Winner patch edits no paths — nothing to commit");
        return false;
    }
    let mut add = Command::new("git");
    add.arg("add").arg("--");
    for p in paths {
        add.arg(p);
    }
    match add.current_dir(repo_root).output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            log_warning(&format!(
                "  git add of winner paths failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
            return false;
        }
        Err(e) => {
            log_warning(&format!("  Failed to run git add: {}", e));
            return false;
        }
    }

    let mut commit = Command::new("git");
    commit.arg("commit").arg("-m").arg(commit_msg).arg("--");
    for p in paths {
        commit.arg(p);
    }
    match commit.current_dir(repo_root).output() {
        Ok(o) if o.status.success() => true,
        Ok(o) => {
            log_warning(&format!(
                "  git commit of winner paths failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ));
            false
        }
        Err(e) => {
            log_warning(&format!("  Failed to run git commit: {}", e));
            false
        }
    }
}

// ═══════════════════════════════════════════════════════
// LOGGING (using the selfware garden aesthetic)
// ═══════════════════════════════════════════════════════

fn log_phase(msg: &str) {
    eprintln!("  🌱 {}", msg);
}

fn log_warning(msg: &str) {
    eprintln!("  🥀 {}", msg);
}

fn log_error(msg: &str) {
    eprintln!("  ❄️  {}", msg);
}

fn log_baseline(metrics: &FitnessMetrics, sab_mode: bool) {
    let label = if sab_mode { "SAB" } else { "compile/test" };
    eprintln!(
        "  📊 Baseline: {} {:.0}/100 ({}) | {} tokens | {:.0}s",
        label,
        metrics.sab_score,
        rating_from_score(metrics.sab_score),
        metrics.tokens_used,
        metrics.wall_clock_secs
    );
}

fn log_generation_start(gen: usize) {
    eprintln!(
        "\n╭─── Generation {} ───────────────────────────────────╮",
        gen
    );
}

fn log_bloom(_gen: usize, description: &str, old_sab: f64, new_sab: f64) {
    eprintln!(
        "│  🌸 BLOOM! SAB {:.0} → {:.0} (+{:.1})",
        old_sab,
        new_sab,
        new_sab - old_sab
    );
    eprintln!("│  📝 {}", description);
    eprintln!("╰────────────────────────────────────────────────────╯");
}

fn chrono_now() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}.{:03}", d.as_secs(), d.subsec_millis())
}

/// Append a structured JSONL event to .evolution-log.jsonl for real-time visualization.
fn log_event(repo_root: &Path, event: &serde_json::Value) {
    use std::io::Write;
    let log_path = repo_root.join(".evolution-log.jsonl");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(f, "{}", event);
    }
}

fn log_frost(_gen: usize, reason: &str) {
    eprintln!("│  ❄️  FROST: {}", reason);
    eprintln!("╰────────────────────────────────────────────────────╯");
}

fn log_reject(_gen: usize, rating: &GenerationRating, winner_sab: f64, baseline_sab: f64) {
    eprintln!(
        "│  {} SAB {:.0} vs baseline {:.0} — rejected",
        rating, winner_sab, baseline_sab
    );
    eprintln!("╰────────────────────────────────────────────────────╯");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contained_path_contains_and_rejects_escapes() {
        let base = Path::new("/repo");
        // Normal in-repo paths resolve inside base.
        assert_eq!(
            contained_path(base, Path::new("src/main.rs")),
            Some(PathBuf::from("/repo/src/main.rs"))
        );
        assert_eq!(
            contained_path(base, Path::new("a/../b")),
            Some(PathBuf::from("/repo/b"))
        );
        // Escapes are rejected: absolute, parent traversal, root.
        assert_eq!(contained_path(base, Path::new("/etc/passwd")), None);
        assert_eq!(contained_path(base, Path::new("../secret")), None);
        assert_eq!(contained_path(base, Path::new("../../etc/passwd")), None);
        assert_eq!(contained_path(base, Path::new("a/../../b")), None);
    }

    #[test]
    fn test_format_empty_history() {
        let history = format_evolution_history(&[]);
        assert!(history.contains("generation 1"));
    }

    #[test]
    fn test_format_history_with_entries() {
        let winners = vec![
            GenerationWinner {
                generation: 1,
                description: "Optimized token counting".into(),
                composite_score: 0.85,
                sab_delta: 3.0,
                token_delta: -50000.0,
                patch: String::new(),
                git_tag: None,
            },
            GenerationWinner {
                generation: 5,
                description: "Rewrote XML parser".into(),
                composite_score: 0.91,
                sab_delta: 5.0,
                token_delta: -30000.0,
                patch: String::new(),
                git_tag: Some("evolve-gen-5".into()),
            },
        ];
        let history = format_evolution_history(&winners);
        assert!(history.contains("Rewrote XML parser")); // Most recent first
        assert!(history.contains("Gen 5"));
    }

    #[test]
    fn test_semantic_summary_grep_search() {
        // Test that grep_search can find public functions in the codebase
        // Scope the search to this module tree so the test stays fast even in
        // large worktrees with populated `target/` directories.
        #[allow(unused_imports)]
        use crate::tools::search::grep_search;
        let results = grep_search(
            "pub fn format_evolution_history",
            "src/evolution",
            true,
            10,
            0,
        );
        assert!(
            results.total_matches > 0,
            "Should find at least one match for pub fn format_evolution_history"
        );
        let match_found = results
            .matches
            .iter()
            .any(|m| m.content.contains("format_evolution_history"));
        assert!(
            match_found,
            "Should find the format_evolution_history function definition"
        );
    }

    #[test]
    fn test_parse_test_summary_sums_results() {
        let output = "\n\
            test result: ok. 10 passed; 0 failed; 0 ignored\n\
            some other line\n\
            test result: ok. 3 passed; 1 failed; 0 ignored\n";
        let (passed, total) = parse_test_summary(output);
        assert_eq!(passed, 13);
        assert_eq!(total, 14);
    }

    #[test]
    fn test_parse_test_summary_no_results() {
        let (passed, total) = parse_test_summary("no test output here");
        assert_eq!(passed, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_metrics_from_sab_result() {
        let sab = SabResult {
            aggregate_score: 88.5,
            scenario_scores: vec![],
            total_tokens_used: 250_000,
            wall_clock: std::time::Duration::from_secs(1200),
            rating: GenerationRating::Bloom,
        };
        let metrics = metrics_from_sab_result(&sab, Path::new("target/release/selfware"), 50.0);
        assert_eq!(metrics.sab_score, 88.5);
        assert_eq!(metrics.tokens_used, 250_000);
        assert_eq!(metrics.token_budget, DEFAULT_TOKEN_BUDGET);
        assert!((metrics.wall_clock_secs - 1200.0).abs() < 0.01);
        assert_eq!(metrics.max_binary_size_mb, 50.0);
    }

    #[test]
    fn test_format_history_caps_at_10() {
        let winners: Vec<GenerationWinner> = (1..=15)
            .map(|i| GenerationWinner {
                generation: i,
                description: format!("Mutation {}", i),
                composite_score: 0.80 + i as f64 * 0.01,
                sab_delta: 1.0,
                token_delta: -1000.0,
                patch: String::new(),
                git_tag: None,
            })
            .collect();
        let history = format_evolution_history(&winners);
        // Should contain gen 15 (most recent) but not gen 1 (oldest, beyond top 10)
        assert!(history.contains("Gen 15"));
        assert!(history.contains("Gen 6")); // 15..=6 is the top 10 reversed
                                            // Gen 5 should NOT appear (it's the 11th from the end)
        assert!(!history.contains("Gen 5"));
    }

    #[test]
    fn test_apply_patch_to_worktree_nonexistent_dir() {
        let result = apply_patch_to_worktree(Path::new("/nonexistent/dir/12345"), "some patch");
        assert!(!result, "Should fail gracefully for nonexistent directory");
    }

    #[test]
    fn test_apply_patch_to_repo_bad_patch() {
        let tmp = std::env::temp_dir().join("selfware-test-bad-patch");
        let _ = std::fs::create_dir_all(&tmp);
        // Initialize a git repo so `git apply` can run
        let _ = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        let result = apply_patch_to_repo(&tmp, "this is not a valid patch format");
        assert!(!result, "Should fail gracefully for bad patch content");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ─── Protected-path gate on ACTUAL patch paths (Evolution P1) ───

    fn make_hypothesis(patch: &str, target_files: &[&str]) -> Hypothesis {
        Hypothesis {
            id: "hyp-test".to_string(),
            description: "test".to_string(),
            patch: patch.to_string(),
            target_files: target_files.iter().map(PathBuf::from).collect(),
            property_test: None,
        }
    }

    #[test]
    fn test_patch_edited_paths_unified_diff() {
        let diff = "diff --git a/src/tools/file.rs b/src/tools/file.rs\n\
                    --- a/src/tools/file.rs\n\
                    +++ b/src/tools/file.rs\n\
                    @@ -1 +1 @@\n-old\n+new\n";
        assert_eq!(
            patch_edited_paths(diff),
            vec![PathBuf::from("src/tools/file.rs")]
        );
        // New file (--- is /dev/null): only the +++ path is extracted.
        let new_file = "--- /dev/null\n+++ b/src/new.rs\n@@ -0,0 +1 @@\n+x\n";
        assert_eq!(
            patch_edited_paths(new_file),
            vec![PathBuf::from("src/new.rs")]
        );
        // Deletion (+++ is /dev/null): the --- path is still caught.
        let deleted = "--- a/src/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
        assert_eq!(
            patch_edited_paths(deleted),
            vec![PathBuf::from("src/old.rs")]
        );
        // Garbage yields nothing.
        assert!(patch_edited_paths("not a patch").is_empty());
    }

    #[test]
    fn test_patch_edited_paths_search_replace_json() {
        let edits = r#"[{"file":"src/a.rs","search":"x","replace":"y"},
                        {"file":"src/b.rs","search":"p","replace":"q"}]"#;
        assert_eq!(
            patch_edited_paths(edits),
            vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]
        );
    }

    #[test]
    fn test_hypothesis_gate_rejects_protected_diff_with_empty_targets() {
        // The review's bypass: declared target_files is EMPTY, but the diff
        // edits src/safety/ — must be refused.
        let h = make_hypothesis(
            "--- a/src/safety/checker.rs\n+++ b/src/safety/checker.rs\n@@ -1 +1 @@\n-a\n+b\n",
            &[],
        );
        assert!(hypothesis_touches_protected(&h));
    }

    #[test]
    fn test_hypothesis_gate_rejects_protected_diff_with_benign_targets() {
        // Declared metadata says src/tools/, the patch edits src/evolution/.
        let h = make_hypothesis(
            "--- a/src/evolution/daemon.rs\n+++ b/src/evolution/daemon.rs\n@@ -1 +1 @@\n-a\n+b\n",
            &["src/tools/file.rs"],
        );
        assert!(hypothesis_touches_protected(&h));
    }

    #[test]
    fn test_hypothesis_gate_rejects_protected_search_replace_edits() {
        let h = make_hypothesis(
            r#"[{"file":"src/safety/mod.rs","search":"x","replace":"y"}]"#,
            &[],
        );
        assert!(hypothesis_touches_protected(&h));
    }

    #[test]
    fn test_hypothesis_gate_rejects_protected_deletion() {
        let h = make_hypothesis(
            "--- a/src/safety/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n",
            &[],
        );
        assert!(hypothesis_touches_protected(&h));
    }

    #[test]
    fn test_hypothesis_gate_allows_benign_patch() {
        let h = make_hypothesis(
            "--- a/src/tools/file.rs\n+++ b/src/tools/file.rs\n@@ -1 +1 @@\n-a\n+b\n",
            &["src/tools/file.rs"],
        );
        assert!(!hypothesis_touches_protected(&h));
        // Declared-metadata signal still works on its own.
        let h2 = make_hypothesis(
            "--- a/src/tools/file.rs\n+++ b/src/tools/file.rs\n@@ -1 +1 @@\n-a\n+b\n",
            &["src/safety/checker.rs"],
        );
        assert!(hypothesis_touches_protected(&h2));
    }

    #[test]
    fn test_apply_patch_to_repo_refuses_protected_patch() {
        // Final gate at the highest-blast-radius point: a patch editing
        // protected files is refused before touching the repo (no fs setup
        // needed — the refusal happens before any apply attempt).
        let diff =
            "--- a/src/safety/checker.rs\n+++ b/src/safety/checker.rs\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(!apply_patch_to_repo(Path::new("/nonexistent"), diff));
        let edits = r#"[{"file":"src/evolution/daemon.rs","search":"x","replace":"y"}]"#;
        assert!(!apply_patch_to_repo(Path::new("/nonexistent"), edits));
    }

    #[test]
    fn test_generation_winner_fields() {
        let winner = GenerationWinner {
            generation: 42,
            description: "Cache optimization".to_string(),
            composite_score: 0.92,
            sab_delta: 7.5,
            token_delta: -25000.0,
            patch: "--- a/src/cache.rs\n+++ b/src/cache.rs".to_string(),
            git_tag: Some("evolve-gen-42".to_string()),
        };
        assert_eq!(winner.generation, 42);
        assert!(winner.sab_delta > 0.0);
        assert!(winner.token_delta < 0.0);
        assert!(winner.git_tag.as_ref().unwrap().contains("42"));
    }

    #[test]
    fn test_evolution_result_fields() {
        let result = EvolutionResult {
            generations_run: 0,
            improvements: vec![],
            final_sab_score: 0.0,
            initial_sab_score: 0.0,
            total_duration: std::time::Duration::from_secs(1),
        };
        assert_eq!(result.generations_run, 0);
        assert!(result.improvements.is_empty());
    }

    #[test]
    fn test_parse_hypotheses_valid_json() {
        let json = r#"[
            {
                "description": "Cache token count lookups",
                "patch": "--- a/src/token.rs\n+++ b/src/token.rs\n@@ -1,3 +1,4 @@\n+use std::collections::HashMap;\n fn count() {}",
                "target_files": ["src/token.rs"],
                "property_test": null
            },
            {
                "description": "Optimize string allocation",
                "patch": "--- a/src/alloc.rs\n+++ b/src/alloc.rs\n@@ -1 +1 @@\n-let s = String::new();\n+let s = String::with_capacity(64);",
                "target_files": ["src/alloc.rs"],
                "property_test": "assert!(true)"
            }
        ]"#;
        let hypotheses = parse_hypotheses_response(json);
        assert_eq!(hypotheses.len(), 2);
        assert_eq!(hypotheses[0].description, "Cache token count lookups");
        assert_eq!(hypotheses[0].id, "hyp-0");
        assert_eq!(
            hypotheses[0].target_files,
            vec![PathBuf::from("src/token.rs")]
        );
        assert!(hypotheses[0].property_test.is_none());
        assert_eq!(hypotheses[1].description, "Optimize string allocation");
        assert_eq!(hypotheses[1].id, "hyp-1");
        assert_eq!(
            hypotheses[1].property_test.as_deref(),
            Some("assert!(true)")
        );
    }

    #[test]
    fn test_parse_hypotheses_markdown_fences() {
        let response = r#"Here are my suggestions:

```json
[
    {
        "description": "Use Vec::with_capacity",
        "patch": "--- a/src/lib.rs\n+++ b/src/lib.rs",
        "target_files": ["src/lib.rs"],
        "property_test": null
    }
]
```

These changes should improve performance."#;
        let hypotheses = parse_hypotheses_response(response);
        assert_eq!(hypotheses.len(), 1);
        assert_eq!(hypotheses[0].description, "Use Vec::with_capacity");
    }

    #[test]
    fn test_parse_hypotheses_malformed() {
        let malformed = "This is not JSON at all, just some text.";
        let hypotheses = parse_hypotheses_response(malformed);
        assert!(hypotheses.is_empty());
    }

    #[test]
    fn test_parse_hypotheses_partial_objects() {
        // Missing required fields — should be filtered out
        let json = r#"[
            {"description": "Good one", "patch": "diff", "target_files": ["a.rs"], "property_test": null},
            {"description": "Missing patch"},
            {"patch": "diff but no desc"}
        ]"#;
        let hypotheses = parse_hypotheses_response(json);
        assert_eq!(hypotheses.len(), 1);
        assert_eq!(hypotheses[0].description, "Good one");
    }

    #[test]
    fn test_build_system_prompt_contains_population() {
        let prompt = build_system_prompt(5);
        assert!(prompt.contains("exactly 5"));
    }

    #[test]
    fn test_build_user_prompt_shape() {
        let prompt = build_user_prompt(
            "cpu: 80%",
            "Gen 1: improved X",
            "```rust\nfn main() {}\n```",
        );
        assert!(prompt.contains("## Current Telemetry"));
        assert!(prompt.contains("cpu: 80%"));
        assert!(prompt.contains("Gen 1: improved X"));
        assert!(prompt.contains("## Source Code"));
        assert!(prompt.contains("fn main()"));
    }

    #[test]
    fn test_build_user_prompt_empty_telemetry() {
        let prompt = build_user_prompt("", "some history", "source");
        assert!(!prompt.contains("## Current Telemetry"));
        assert!(prompt.contains("some history"));
    }

    #[test]
    fn test_llm_config_default() {
        let cfg = LlmConfig::default();
        assert_eq!(cfg.max_tokens, 16384);
        assert!((cfg.temperature - 0.7).abs() < f32::EPSILON);
        assert!(cfg.api_key.is_none());
        assert!(!cfg.endpoint.is_empty());
        assert!(!cfg.model.is_empty());
    }

    #[test]
    fn test_extract_json_array_plain() {
        let input = r#"[{"a": 1}]"#;
        let result = extract_json_array(input);
        assert_eq!(result.unwrap(), r#"[{"a": 1}]"#);
    }

    #[test]
    fn test_extract_json_array_with_preamble() {
        let input = "Here is the result:\n[{\"x\": 1}]";
        let result = extract_json_array(input);
        assert_eq!(result.unwrap(), r#"[{"x": 1}]"#);
    }

    #[test]
    fn test_extract_json_array_nested() {
        let input = r#"[{"a": [1, 2]}, {"b": 3}]"#;
        let result = extract_json_array(input);
        assert_eq!(result.unwrap(), input);
    }

    #[test]
    fn test_extract_json_array_none() {
        assert!(extract_json_array("no array here").is_none());
    }

    #[test]
    fn test_add_line_numbers() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        let numbered = add_line_numbers(src);
        assert!(numbered.contains("1| fn main() {"));
        assert!(numbered.contains("2|     println!(\"hello\");"));
        assert!(numbered.contains("3| }"));
    }

    #[test]
    fn test_add_line_numbers_width() {
        // 100+ lines should get 3-digit width
        let src: String = (1..=150).map(|i| format!("line {}\n", i)).collect();
        let numbered = add_line_numbers(&src);
        assert!(numbered.contains("  1| line 1"));
        assert!(numbered.contains("150| line 150"));
    }

    #[test]
    fn test_truncate_to_line_boundary() {
        let text = "line one\nline two\nline three\nline four\n";
        let trunc = truncate_to_line_boundary(text, 20);
        assert_eq!(trunc, "line one\nline two");
    }

    #[test]
    fn test_truncate_to_line_boundary_fits() {
        let text = "short";
        assert_eq!(truncate_to_line_boundary(text, 100), "short");
    }

    #[test]
    fn test_parse_hypotheses_edits_format() {
        let json = r#"[
            {
                "description": "Optimize token counting",
                "edits": [
                    {"file": "src/token.rs", "search": "old_code()", "replace": "new_code()"}
                ],
                "target_files": ["src/token.rs"],
                "property_test": null
            }
        ]"#;
        let hypotheses = parse_hypotheses_response(json);
        assert_eq!(hypotheses.len(), 1);
        assert_eq!(hypotheses[0].description, "Optimize token counting");
        // patch should contain the serialized edits JSON
        assert!(hypotheses[0].patch.contains("old_code()"));
        assert!(hypotheses[0].patch.contains("new_code()"));
    }

    #[test]
    fn test_apply_search_replace_basic() {
        let tmp = std::env::temp_dir().join("selfware-test-sr");
        let _ = std::fs::create_dir_all(&tmp);
        let test_file = tmp.join("test.rs");
        std::fs::write(&test_file, "fn old_func() {\n    println!(\"hello\");\n}\n").unwrap();

        let edits = vec![serde_json::json!({
            "file": "test.rs",
            "search": "fn old_func()",
            "replace": "fn new_func()"
        })];

        let result = apply_search_replace(&tmp, &edits);
        assert!(result);

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("fn new_func()"));
        assert!(!content.contains("fn old_func()"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_search_replace_not_found() {
        let tmp = std::env::temp_dir().join("selfware-test-sr-notfound");
        let _ = std::fs::create_dir_all(&tmp);
        let test_file = tmp.join("test.rs");
        std::fs::write(&test_file, "fn foo() {}\n").unwrap();

        let edits = vec![serde_json::json!({
            "file": "test.rs",
            "search": "fn nonexistent()",
            "replace": "fn bar()"
        })];

        let result = apply_search_replace(&tmp, &edits);
        assert!(!result);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_fuzzy_find_and_replace_exact() {
        let content = "fn foo() {\n    old_code();\n}\n";
        let result = fuzzy_find_and_replace(content, "    old_code();", "    new_code();");
        assert!(result.is_some());
        assert!(result.unwrap().contains("new_code()"));
    }

    #[test]
    fn test_fuzzy_find_and_replace_indent_mismatch() {
        // File has 8-space indent, search has 4-space indent
        let content = "fn foo() {\n        old_code();\n}\n";
        let result = fuzzy_find_and_replace(content, "    old_code();", "    new_code();");
        assert!(result.is_some());
        let r = result.unwrap();
        // Should preserve the file's 8-space indent
        assert!(r.contains("        new_code();"), "got: {}", r);
    }

    #[test]
    fn test_fuzzy_find_and_replace_multiline() {
        let content = "    fn foo() {\n        let x = 1;\n        let y = 2;\n    }\n";
        let search = "let x = 1;\n  let y = 2;";
        let replace = "let x = 10;\n  let y = 20;";
        let result = fuzzy_find_and_replace(content, search, replace);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.contains("let x = 10;"), "got: {}", r);
        assert!(r.contains("let y = 20;"), "got: {}", r);
    }

    #[test]
    fn test_build_system_prompt_mentions_line_numbers() {
        let prompt = build_system_prompt(4);
        assert!(prompt.contains("line numbers"));
        assert!(prompt.contains("exactly 4"));
        assert!(prompt.contains("search"));
        assert!(prompt.contains("replace"));
    }

    #[test]
    fn test_sanitize_patch_strips_line_numbers() {
        let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -10,3 +10,3 @@
  10| fn foo() {
- 11|     old_code();
+ 11|     new_code();
  12| }
";
        let clean = sanitize_patch(patch);
        assert!(clean.contains(" fn foo() {\n"));
        assert!(clean.contains("-    old_code();\n"));
        assert!(clean.contains("+    new_code();\n"));
        assert!(clean.contains(" }\n"));
        assert!(!clean.contains("10|"));
    }

    #[test]
    fn test_sanitize_patch_preserves_clean_patch() {
        let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -10,3 +10,3 @@
 fn foo() {
-    old_code();
+    new_code();
 }
";
        let clean = sanitize_patch(patch);
        assert_eq!(clean, patch);
    }

    #[test]
    fn test_sanitize_patch_handles_pipes_in_code() {
        // Pipe in code (e.g. match arms, closures) should NOT be stripped
        let patch = "\
--- a/src/foo.rs
+++ b/src/foo.rs
@@ -1,3 +1,3 @@
 match x {
-    Some(v) | None => {}
+    Some(v) | None => { v }
 }
";
        let clean = sanitize_patch(patch);
        assert!(clean.contains("    Some(v) | None => {}"));
    }

    // ── leading_whitespace tests ──

    #[test]
    fn test_leading_whitespace_spaces() {
        assert_eq!(leading_whitespace("    code"), "    ");
    }

    #[test]
    fn test_leading_whitespace_tabs() {
        assert_eq!(leading_whitespace("\t\tcode"), "\t\t");
    }

    #[test]
    fn test_leading_whitespace_none() {
        assert_eq!(leading_whitespace("code"), "");
    }

    #[test]
    fn test_leading_whitespace_all_spaces() {
        assert_eq!(leading_whitespace("    "), "    ");
    }

    // ── apply_edits dispatch tests ──

    #[test]
    fn test_apply_edits_dispatches_to_search_replace() {
        let tmp = std::env::temp_dir().join("selfware-test-dispatch-sr");
        let _ = std::fs::create_dir_all(&tmp);
        // Init git repo for the function
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        let test_file = tmp.join("test.rs");
        std::fs::write(&test_file, "fn old() {}\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&tmp)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&tmp)
            .output();

        // JSON array with search/replace → should dispatch to apply_search_replace
        let edits_json = serde_json::json!([
            {"file": "test.rs", "search": "fn old() {}", "replace": "fn new() {}"}
        ]);
        let patch = serde_json::to_string(&edits_json).unwrap();
        assert!(apply_edits(&tmp, &patch));

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("fn new()"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_edits_dispatches_to_unified_diff() {
        let tmp = std::env::temp_dir().join("selfware-test-dispatch-ud");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        let test_file = tmp.join("test.rs");
        std::fs::write(&test_file, "fn old() {}\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&tmp)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&tmp)
            .output();

        // A plain unified diff string → should dispatch to apply_unified_diff
        let patch = "--- a/test.rs\n+++ b/test.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        assert!(apply_edits(&tmp, patch));

        let content = std::fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("fn new()"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_edits_bad_json_falls_to_diff() {
        // Not valid JSON → falls through to unified diff (which will also fail for gibberish)
        let tmp = std::env::temp_dir().join("selfware-test-dispatch-bad");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        std::fs::write(tmp.join("x.rs"), "code\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&tmp)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&tmp)
            .output();

        assert!(!apply_edits(&tmp, "not json and not a patch"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── apply_search_replace edge cases ──

    #[test]
    fn test_apply_search_replace_ambiguous() {
        let tmp = std::env::temp_dir().join("selfware-test-sr-ambig");
        let _ = std::fs::create_dir_all(&tmp);
        // File with duplicate pattern
        std::fs::write(tmp.join("dup.rs"), "fn foo() {}\nfn foo() {}\n").unwrap();

        let edits = vec![serde_json::json!({
            "file": "dup.rs",
            "search": "fn foo() {}",
            "replace": "fn bar() {}"
        })];
        // Should reject because search is ambiguous (2 matches)
        assert!(!apply_search_replace(&tmp, &edits));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_search_replace_multiple_edits_same_file() {
        let tmp = std::env::temp_dir().join("selfware-test-sr-multi");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(
            tmp.join("multi.rs"),
            "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n",
        )
        .unwrap();

        let edits = vec![
            serde_json::json!({"file": "multi.rs", "search": "fn alpha() {}", "replace": "fn alpha_v2() {}"}),
            serde_json::json!({"file": "multi.rs", "search": "fn gamma() {}", "replace": "fn gamma_v2() {}"}),
        ];
        assert!(apply_search_replace(&tmp, &edits));

        let content = std::fs::read_to_string(tmp.join("multi.rs")).unwrap();
        assert!(content.contains("fn alpha_v2()"));
        assert!(content.contains("fn beta()")); // unchanged
        assert!(content.contains("fn gamma_v2()"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_search_replace_missing_file() {
        let tmp = std::env::temp_dir().join("selfware-test-sr-nofile");
        let _ = std::fs::create_dir_all(&tmp);

        let edits = vec![serde_json::json!({
            "file": "nonexistent.rs",
            "search": "a",
            "replace": "b"
        })];
        assert!(!apply_search_replace(&tmp, &edits));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_search_replace_noop_rejected() {
        let tmp = std::env::temp_dir().join("selfware-test-sr-noop");
        let _ = std::fs::create_dir_all(&tmp);
        std::fs::write(tmp.join("noop.rs"), "fn foo() {}\n").unwrap();

        // search == replace → no change → should be rejected
        let edits = vec![serde_json::json!({
            "file": "noop.rs",
            "search": "fn foo() {}",
            "replace": "fn foo() {}"
        })];
        assert!(!apply_search_replace(&tmp, &edits));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── fuzzy_find_and_replace edge cases ──

    #[test]
    fn test_fuzzy_find_and_replace_no_match() {
        let content = "fn foo() {\n    code();\n}\n";
        let result = fuzzy_find_and_replace(content, "fn bar() {", "fn baz() {");
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_find_and_replace_empty_search() {
        let content = "fn foo() {}\n";
        let result = fuzzy_find_and_replace(content, "", "something");
        assert!(result.is_none());
    }

    #[test]
    fn test_fuzzy_find_and_replace_preserves_trailing_newline() {
        let content = "fn foo() {\n    old();\n}\n";
        let result = fuzzy_find_and_replace(content, "    old();", "    new();");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(
            r.ends_with('\n'),
            "Should preserve trailing newline: {:?}",
            r
        );
    }

    #[test]
    fn test_fuzzy_find_and_replace_at_end_of_file() {
        let content = "line1\nline2\ntarget_line\n";
        let result = fuzzy_find_and_replace(content, "target_line", "replaced_line");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.contains("replaced_line"));
        assert!(r.contains("line1"));
        assert!(r.contains("line2"));
    }

    #[test]
    fn test_fuzzy_find_and_replace_at_start_of_file() {
        let content = "target_line\nline2\nline3\n";
        let result = fuzzy_find_and_replace(content, "target_line", "replaced_line");
        assert!(result.is_some());
        let r = result.unwrap();
        assert!(r.starts_with("replaced_line"));
    }

    // ── read_mutation_targets tests ──

    #[test]
    fn test_read_mutation_targets_sorts_by_size() {
        let tmp = std::env::temp_dir().join("selfware-test-rmt");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        // Create files of different sizes
        std::fs::write(tmp.join("src/big.rs"), "x".repeat(5000)).unwrap();
        std::fs::write(tmp.join("src/small.rs"), "y".repeat(100)).unwrap();
        std::fs::write(tmp.join("src/medium.rs"), "z".repeat(1000)).unwrap();

        let targets = super::super::MutationTargets {
            prompt_logic: vec![
                PathBuf::from("src/big.rs"),
                PathBuf::from("src/small.rs"),
                PathBuf::from("src/medium.rs"),
            ],
            tool_code: vec![],
            cognitive: vec![],
            config_keys: vec![],
        };

        let context = read_mutation_targets(&targets, &tmp);
        // small.rs should appear before big.rs (sorted by size ascending)
        let small_pos = context.find("src/small.rs").unwrap();
        let medium_pos = context.find("src/medium.rs").unwrap();
        let big_pos = context.find("src/big.rs").unwrap();
        assert!(small_pos < medium_pos, "small should come before medium");
        assert!(medium_pos < big_pos, "medium should come before big");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_read_mutation_targets_includes_line_numbers() {
        let tmp = std::env::temp_dir().join("selfware-test-rmt-ln");
        let _ = std::fs::create_dir_all(tmp.join("src"));
        std::fs::write(
            tmp.join("src/test.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();

        let targets = super::super::MutationTargets {
            prompt_logic: vec![PathBuf::from("src/test.rs")],
            tool_code: vec![],
            cognitive: vec![],
            config_keys: vec![],
        };

        let context = read_mutation_targets(&targets, &tmp);
        assert!(
            context.contains("1| fn main()"),
            "Should contain line numbers: {}",
            truncate_char_boundary(&context, 200)
        );
        assert!(context.contains("2|     println!"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_read_mutation_targets_empty() {
        let tmp = std::env::temp_dir().join("selfware-test-rmt-empty");
        let _ = std::fs::create_dir_all(&tmp);

        let targets = super::super::MutationTargets {
            prompt_logic: vec![],
            tool_code: vec![],
            cognitive: vec![],
            config_keys: vec![],
        };

        let context = read_mutation_targets(&targets, &tmp);
        assert!(context.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_read_mutation_targets_missing_file() {
        let tmp = std::env::temp_dir().join("selfware-test-rmt-missing");
        let _ = std::fs::create_dir_all(&tmp);

        let targets = super::super::MutationTargets {
            prompt_logic: vec![PathBuf::from("nonexistent.rs")],
            tool_code: vec![],
            cognitive: vec![],
            config_keys: vec![],
        };

        let context = read_mutation_targets(&targets, &tmp);
        // Should gracefully skip missing files
        assert!(context.is_empty() || !context.contains("```rust"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── log_event tests ──

    #[test]
    fn test_log_event_writes_jsonl() {
        let tmp = std::env::temp_dir().join("selfware-test-logevent");
        let _ = std::fs::create_dir_all(&tmp);

        let event = serde_json::json!({"event": "test", "value": 42});
        log_event(&tmp, &event);
        log_event(&tmp, &serde_json::json!({"event": "second"}));

        let log_path = tmp.join(".evolution-log.jsonl");
        let content = std::fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed["event"], "test");
        assert_eq!(parsed["value"], 42);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── apply_unified_diff tests ──

    #[test]
    fn test_apply_unified_diff_valid_patch() {
        let tmp = std::env::temp_dir().join("selfware-test-ud-valid");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        std::fs::write(tmp.join("file.rs"), "fn old() {}\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&tmp)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&tmp)
            .output();

        let patch = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-fn old() {}\n+fn new() {}\n";
        assert!(apply_unified_diff(&tmp, patch));

        let content = std::fs::read_to_string(tmp.join("file.rs")).unwrap();
        assert!(content.contains("fn new()"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_unified_diff_invalid_patch() {
        let tmp = std::env::temp_dir().join("selfware-test-ud-invalid");
        let _ = std::fs::create_dir_all(&tmp);
        let _ = Command::new("git")
            .args(["init"])
            .current_dir(&tmp)
            .output();
        std::fs::write(tmp.join("file.rs"), "fn foo() {}\n").unwrap();
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&tmp)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&tmp)
            .output();

        assert!(!apply_unified_diff(&tmp, "garbage patch content"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── parse_hypotheses edge cases ──

    #[test]
    fn test_parse_hypotheses_mixed_formats() {
        // One with edits, one with patch (legacy), one missing both → 2 parsed
        let json = r#"[
            {
                "description": "Edit format",
                "edits": [{"file": "a.rs", "search": "old", "replace": "new"}],
                "target_files": ["a.rs"],
                "property_test": null
            },
            {
                "description": "Patch format",
                "patch": "--- a/b.rs\n+++ b/b.rs",
                "target_files": ["b.rs"],
                "property_test": null
            },
            {
                "description": "Missing both",
                "target_files": ["c.rs"],
                "property_test": null
            }
        ]"#;
        let hypotheses = parse_hypotheses_response(json);
        assert_eq!(hypotheses.len(), 2);
        assert_eq!(hypotheses[0].description, "Edit format");
        assert!(hypotheses[0].patch.contains("old")); // serialized edits JSON
        assert_eq!(hypotheses[1].description, "Patch format");
    }

    #[test]
    fn test_parse_hypotheses_empty_edits_rejected() {
        let json = r#"[{
            "description": "Empty edits",
            "edits": [],
            "target_files": ["a.rs"],
            "property_test": null
        }]"#;
        let hypotheses = parse_hypotheses_response(json);
        // Empty edits serializes to "[]" which is not empty string, but apply_edits
        // would reject it. parse should still create the hypothesis.
        assert_eq!(hypotheses.len(), 1);
    }

    // ── chrono_now test ──

    #[test]
    fn test_chrono_now_format() {
        let ts = chrono_now();
        // Should be "seconds.milliseconds" format
        assert!(ts.contains('.'), "Timestamp should contain '.': {}", ts);
        let parts: Vec<&str> = ts.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[0].parse::<u64>().is_ok());
        assert!(parts[1].parse::<u64>().is_ok());
    }

    // ── UTF-8 char-boundary truncation (whole-repo review, Evolution P1) ──

    #[test]
    fn test_truncate_char_boundary_never_splits_multibyte() {
        // 199 ASCII bytes + a 3-byte char (€) straddling byte 200.
        let mut s = "a".repeat(199);
        s.push('€'); // bytes 199..202
        s.push_str("tail");
        let out = truncate_char_boundary(&s, 200);
        assert_eq!(out.len(), 199, "must back off to the char boundary");

        // Exactly at a boundary: no backoff.
        let s2 = format!("{}€", "b".repeat(197)); // 197 + 3 = exactly 200
        assert_eq!(truncate_char_boundary(&s2, 200).len(), 200);

        // Shorter than the limit: untouched.
        assert_eq!(truncate_char_boundary("short", 200), "short");
    }

    #[test]
    fn test_truncate_to_line_boundary_multibyte_does_not_panic() {
        // Regression: `s[..max_chars]` panicked when max_chars landed inside
        // a multibyte char, aborting the whole evolve run before any LLM call.
        // CJK chars are 3 bytes; byte 20 lands mid-char (18 is the boundary).
        let text = format!("{}\nsecond line\n", "日".repeat(10));
        let trunc = truncate_to_line_boundary(&text, 20);
        assert_eq!(trunc, "日".repeat(6), "backs off to the char boundary");

        // Newline right after a multibyte run: cut lands on the newline.
        let text2 = format!("{}\nsecond line\n", "日".repeat(6)); // 18 bytes + \n
        let trunc2 = truncate_to_line_boundary(&text2, 20);
        assert_eq!(trunc2, "日".repeat(6));
    }

    // ── Winner-commit helpers (whole-repo review, Evolution P1) ──

    /// Run git in `dir`, asserting success.
    fn git_ok(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn git_stdout(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// A throwaway repo with one committed source file, an unrelated dirty
    /// edit, and an untracked `.env` — the exact cocktail `git add -A` used
    /// to sweep into the BLOOM commit.
    fn setup_winner_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn f() -> usize { 1 }\n").unwrap();
        std::fs::write(root.join("notes.txt"), "clean notes\n").unwrap();
        git_ok(root, &["init"]);
        git_ok(root, &["config", "user.email", "evo@test"]);
        git_ok(root, &["config", "user.name", "Evo Test"]);
        git_ok(root, &["add", "."]);
        git_ok(root, &["commit", "-m", "initial"]);
        // Unrelated dirty edit + untracked secret — must NEVER be committed
        // by the evolution daemon.
        std::fs::write(root.join("notes.txt"), "user work in progress\n").unwrap();
        std::fs::write(root.join(".env"), "SECRET=hunter2\n").unwrap();
        dir
    }

    #[test]
    fn test_capture_tested_diff_includes_new_files_and_edits() {
        let dir = setup_winner_repo();
        let root = dir.path();
        let worktree = ast_tools::create_shadow_worktree(root).unwrap();
        // Simulate the winner patch + a cargo-fmt auto-fix in the worktree.
        std::fs::write(worktree.join("src/lib.rs"), "pub fn f() -> usize { 2 }\n").unwrap();
        std::fs::write(worktree.join("src/new.rs"), "pub fn g() {}\n").unwrap();

        let diff = capture_tested_diff(&worktree).expect("diff should capture");
        assert!(diff.contains("src/lib.rs"), "edit captured: {}", diff);
        assert!(diff.contains("pub fn f() -> usize { 2 }"));
        assert!(diff.contains("src/new.rs"), "new file captured: {}", diff);
        assert_eq!(patch_edited_paths(&diff).len(), 2);

        ast_tools::cleanup_worktree(root, &worktree).unwrap();
    }

    #[test]
    fn test_worktree_guard_cleans_up_on_drop() {
        let dir = setup_winner_repo();
        let root = dir.path();
        let worktree = ast_tools::create_shadow_worktree(root).unwrap();
        assert!(worktree.exists());
        {
            let _guard = WorktreeGuard::new(root, worktree.clone());
        } // guard dropped here — including on the panic path
        assert!(
            !worktree.exists(),
            "guard drop must remove the shadow worktree"
        );
    }

    #[test]
    fn test_commit_scoped_paths_excludes_unrelated_dirty_and_env() {
        let dir = setup_winner_repo();
        let root = dir.path();
        // Winner change to src/lib.rs (as if applied from the tested diff).
        std::fs::write(root.join("src/lib.rs"), "pub fn f() -> usize { 2 }\n").unwrap();

        let paths = vec![PathBuf::from("src/lib.rs")];
        assert!(commit_scoped_paths(
            root,
            &paths,
            "🧬 Gen 1 BLOOM: 50 → 60 | test"
        ));

        // The commit contains ONLY src/lib.rs.
        let names = git_stdout(root, &["show", "--name-only", "--format=", "HEAD"]);
        assert!(names.contains("src/lib.rs"));
        assert!(!names.contains("notes.txt"), "unrelated edit not committed");
        assert!(!names.contains(".env"), "secret not committed");

        // Unrelated dirty edit and .env are still there, uncommitted.
        let status = git_stdout(root, &["status", "--porcelain"]);
        assert!(
            status.contains("?? .env"),
            "env stays untracked: {}",
            status
        );
        assert!(
            status.contains(" M notes.txt"),
            "unrelated edit stays dirty: {}",
            status
        );
        let notes = std::fs::read_to_string(root.join("notes.txt")).unwrap();
        assert_eq!(notes, "user work in progress\n");
    }

    #[test]
    fn test_commit_scoped_paths_handles_new_and_deleted_files() {
        let dir = setup_winner_repo();
        let root = dir.path();
        std::fs::write(root.join("src/new.rs"), "pub fn g() {}\n").unwrap();
        std::fs::remove_file(root.join("notes.txt")).unwrap();
        // notes.txt is BOTH the deleted winner path and was dirty — reset it
        // to committed state first so the deletion is the only change.
        git_ok(root, &["checkout", "--", "notes.txt"]);
        std::fs::remove_file(root.join("notes.txt")).unwrap();

        let paths = vec![PathBuf::from("src/new.rs"), PathBuf::from("notes.txt")];
        assert!(commit_scoped_paths(root, &paths, "🧬 Gen 2 BLOOM"));

        let names = git_stdout(root, &["show", "--name-status", "--format=", "HEAD"]);
        assert!(names.contains("A\tsrc/new.rs"), "new file added: {}", names);
        assert!(
            names.contains("D\tnotes.txt"),
            "deletion committed: {}",
            names
        );
        assert!(!names.contains(".env"));
    }

    #[test]
    fn test_commit_winner_to_repo_applies_tested_diff_exactly() {
        let dir = setup_winner_repo();
        let root = dir.path();

        // Build a tested diff in a shadow worktree (patch + "fmt fix").
        let worktree = ast_tools::create_shadow_worktree(root).unwrap();
        std::fs::write(
            worktree.join("src/lib.rs"),
            "pub fn f() -> usize {\n    2 // fmt: reformatted\n}\n",
        )
        .unwrap();
        let tested_diff = capture_tested_diff(&worktree).unwrap();
        ast_tools::cleanup_worktree(root, &worktree).unwrap();

        assert!(commit_winner_to_repo(root, &tested_diff, "🧬 Gen 3 BLOOM"));
        // The committed content is byte-identical to the TESTED worktree
        // content (fmt fix included), not the raw LLM patch.
        let content = std::fs::read_to_string(root.join("src/lib.rs")).unwrap();
        assert_eq!(
            content,
            "pub fn f() -> usize {\n    2 // fmt: reformatted\n}\n"
        );
        let committed = git_stdout(root, &["show", "HEAD:src/lib.rs"]);
        assert_eq!(committed, content);

        // Unrelated files untouched and uncommitted.
        let status = git_stdout(root, &["status", "--porcelain"]);
        assert!(status.contains("?? .env"));
        assert!(status.contains(" M notes.txt"));
    }

    #[test]
    fn test_apply_tested_diff_refuses_protected_paths() {
        let dir = setup_winner_repo();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/evolution")).unwrap();
        let diff = "--- a/src/evolution/daemon.rs\n+++ b/src/evolution/daemon.rs\n@@ -1 +1 @@\n-old\n+new\n";
        assert!(!apply_tested_diff_to_repo(root, diff));
        assert!(!root.join("src/evolution/daemon.rs").exists());
    }
}
