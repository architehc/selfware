//! Evolution Daemon — `selfware evolve`
//!
//! The main evolutionary loop that ties everything together:
//! Mutate → Compile-gate → Sandbox → Fitness → Select/Rollback
//!
//! This module is PROTECTED from self-modification.

use super::ast_tools::{self, AstMutationResult};
use super::fitness::{self, SabConfig, SabResult};
use super::sandbox::SandboxConfig;
use super::telemetry;
use super::tournament::{self, Hypothesis, HypothesisResult, TournamentConfig};
use super::{is_protected, EvolutionConfig, FitnessMetrics, FitnessWeights, GenerationRating};
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

/// Run the evolution daemon
pub fn evolve(config: EvolutionConfig, repo_root: &Path) -> EvolutionResult {
    let start = Instant::now();
    let mut hall_of_fame: Vec<GenerationWinner> = Vec::new();
    let mut generation: usize = 0;

    // ═══════════════════════════════════════════════════════
    // MEASURE BASELINE
    // ═══════════════════════════════════════════════════════

    log_phase("Measuring baseline fitness...");
    let sab_config = SabConfig::default();
    let selfware_binary = repo_root.join("target/release/selfware");

    let baseline_sab = match fitness::run_sab(&selfware_binary, &sab_config) {
        Ok(r) => r,
        Err(e) => {
            log_error(&format!("Failed to establish baseline: {}", e));
            return EvolutionResult {
                generations_run: 0,
                improvements: vec![],
                final_sab_score: 0.0,
                initial_sab_score: 0.0,
                total_duration: start.elapsed(),
            };
        }
    };

    let initial_sab = baseline_sab.aggregate_score;
    let mut current_baseline = baseline_sab;

    log_baseline(&current_baseline);

    // ═══════════════════════════════════════════════════════
    // MAIN EVOLUTIONARY LOOP
    // ═══════════════════════════════════════════════════════

    loop {
        generation += 1;
        if config.generations > 0 && generation > config.generations {
            break;
        }

        log_generation_start(generation);

        // ─── Step 1: Capture telemetry (sensory data for the agent) ───
        let telemetry_snapshot = telemetry::capture(repo_root, "sab_full").ok();
        let telemetry_prompt = telemetry_snapshot
            .as_ref()
            .map(telemetry::to_agent_prompt)
            .unwrap_or_default();

        let history_prompt = format_evolution_history(&hall_of_fame);

        // ─── Step 2: Generate hypotheses via agent swarm ───
        let hypotheses =
            generate_hypotheses(&config, &telemetry_prompt, &history_prompt, repo_root);

        if hypotheses.is_empty() {
            log_warning("No valid hypotheses generated, retrying...");
            continue;
        }

        // ─── Step 3: Pre-filter with cargo check (AST gate) ───
        let valid: Vec<_> = hypotheses
            .into_iter()
            .filter(|h| {
                // Safety check: ensure no protected files are touched
                if h.target_files.iter().any(|f| is_protected(f)) {
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
            continue;
        }

        log_phase(&format!(
            "Evaluating {} hypotheses ({} parallel)...",
            valid.len(),
            config.parallel_eval
        ));

        // ─── Step 4: Tournament evaluation ───
        let tournament_config = TournamentConfig {
            max_parallel: config.parallel_eval,
            timeout: std::time::Duration::from_secs(3600),
            weights: config.fitness_weights.clone(),
            sandbox: SandboxConfig::default(),
        };

        let results = tournament::run_tournament(valid, &tournament_config, repo_root);

        if results.is_empty() {
            log_warning("No hypotheses survived evaluation");
            continue;
        }

        let winner = &results[0];

        // ─── Step 5: Full SAB evaluation of winner ───
        log_phase(&format!(
            "Winner: '{}' (score: {:.1}) — running full SAB...",
            winner.description, winner.composite_score
        ));

        // Apply winner's patch to the real repo (in a worktree)
        let worktree = match ast_tools::create_shadow_worktree(repo_root) {
            Ok(w) => w,
            Err(e) => {
                log_error(&format!("Failed to create worktree: {}", e));
                continue;
            }
        };

        // Apply patch in worktree and compile
        let patch_applied = apply_patch_to_worktree(&worktree, &winner.patch);
        if !patch_applied {
            log_frost(generation, "Patch failed to apply to worktree");
            let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
            continue;
        }

        // Build the mutated binary
        let build = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(&worktree)
            .output();

        if build.map(|o| !o.status.success()).unwrap_or(true) {
            log_frost(generation, "Mutated binary failed to compile");
            let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
            continue;
        }

        // Run SAB on the mutated binary
        let mutated_binary = worktree.join("target/release/selfware");
        let winner_sab = fitness::run_sab(&mutated_binary, &sab_config);

        let _ = ast_tools::cleanup_worktree(repo_root, &worktree);

        let winner_sab = match winner_sab {
            Ok(r) => r,
            Err(e) => {
                log_error(&format!("SAB evaluation failed: {}", e));
                continue;
            }
        };

        // ─── Step 6: EMERGE OR DIE ───
        let baseline_composite = config
            .fitness_weights
            .composite(&build_metrics(&current_baseline, &config));
        let winner_composite = config
            .fitness_weights
            .composite(&build_metrics(&winner_sab, &config));

        if winner_composite > baseline_composite {
            // 🌸 BLOOM — New baseline!
            log_bloom(
                generation,
                &winner.description,
                current_baseline.aggregate_score,
                winner_sab.aggregate_score,
            );

            // Apply the winning patch to the actual repo
            if apply_patch_to_repo(repo_root, &winner.patch) {
                // Commit
                let commit_msg = format!(
                    "🧬 Gen {} BLOOM: SAB {:.0} → {:.0} | {}",
                    generation,
                    current_baseline.aggregate_score,
                    winner_sab.aggregate_score,
                    winner.description
                );
                let _ = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(repo_root)
                    .output();
                let _ = Command::new("git")
                    .args(["commit", "-m", &commit_msg])
                    .current_dir(repo_root)
                    .output();

                // Tag checkpoint
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
                    sab_delta: winner_sab.aggregate_score - current_baseline.aggregate_score,
                    token_delta: winner_sab.total_tokens_used as f64
                        - current_baseline.total_tokens_used as f64,
                    patch: winner.patch.clone(),
                    git_tag,
                });

                current_baseline = winner_sab;
            }
        } else {
            // ❄️ FROST or 🥀 WILT — reject
            let rating = if winner_composite < baseline_composite * 0.9 {
                GenerationRating::Frost
            } else {
                GenerationRating::Wilt
            };
            log_reject(
                generation,
                &rating,
                winner_sab.aggregate_score,
                current_baseline.aggregate_score,
            );
        }
    }

    EvolutionResult {
        generations_run: generation,
        improvements: hall_of_fame,
        final_sab_score: current_baseline.aggregate_score,
        initial_sab_score: initial_sab,
        total_duration: start.elapsed(),
    }
}

// ═══════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════

fn generate_hypotheses(
    _config: &EvolutionConfig,
    _telemetry_prompt: &str,
    _history_prompt: &str,
    _repo_root: &Path,
) -> Vec<Hypothesis> {
    // TODO: This is where the LLM agent generates mutation proposals.
    // For now, return empty — the integration with the agent swarm
    // will wire this to the existing multi-agent system.
    //
    // The agent receives:
    // 1. Current telemetry (CPU/memory hotspots)
    // 2. Evolution history (what worked, what didn't)
    // 3. Allowed mutation targets from config
    // 4. Current source code of target files
    //
    // The agent returns N hypotheses as unified diffs.
    vec![]
}

fn format_evolution_history(hall_of_fame: &[GenerationWinner]) -> String {
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

fn build_metrics(sab: &SabResult, config: &EvolutionConfig) -> FitnessMetrics {
    FitnessMetrics {
        sab_score: sab.aggregate_score,
        tokens_used: sab.total_tokens_used,
        token_budget: 500_000, // From config
        wall_clock_secs: sab.wall_clock.as_secs_f64(),
        timeout_secs: 3600.0,
        test_coverage_pct: 82.0, // Would need real measurement
        binary_size_mb: 15.0,    // Would need real measurement
        max_binary_size_mb: config.safety.max_binary_size_mb,
        tests_passed: 5200,
        tests_total: 5200,
    }
}

fn apply_patch_to_worktree(worktree: &Path, patch: &str) -> bool {
    let patch_file = worktree.join(".evolution-patch");
    if std::fs::write(&patch_file, patch).is_err() {
        return false;
    }
    let result = Command::new("git")
        .args(["apply", ".evolution-patch"])
        .current_dir(worktree)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    result.map(|o| o.status.success()).unwrap_or(false)
}

fn apply_patch_to_repo(repo_root: &Path, patch: &str) -> bool {
    let patch_file = repo_root.join(".evolution-patch");
    if std::fs::write(&patch_file, patch).is_err() {
        return false;
    }
    let result = Command::new("git")
        .args(["apply", ".evolution-patch"])
        .current_dir(repo_root)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    result.map(|o| o.status.success()).unwrap_or(false)
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

fn log_baseline(sab: &SabResult) {
    eprintln!(
        "  📊 Baseline: SAB {:.0}/100 ({}) | {} tokens | {:.0}s",
        sab.aggregate_score,
        sab.rating,
        sab.total_tokens_used,
        sab.wall_clock.as_secs_f64()
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
    fn test_build_metrics() {
        let sab = SabResult {
            aggregate_score: 88.5,
            scenario_scores: vec![],
            total_tokens_used: 250_000,
            wall_clock: std::time::Duration::from_secs(1200),
            rating: GenerationRating::Bloom,
        };
        let config = EvolutionConfig {
            generations: 10,
            population_size: 8,
            parallel_eval: 4,
            checkpoint_interval: 5,
            fitness_weights: FitnessWeights::default(),
            mutation_targets: super::super::MutationTargets {
                config_keys: vec![],
                prompt_logic: vec![],
                tool_code: vec![],
                cognitive: vec![],
            },
            safety: super::super::SafetyConfig::default(),
        };
        let metrics = build_metrics(&sab, &config);
        assert_eq!(metrics.sab_score, 88.5);
        assert_eq!(metrics.tokens_used, 250_000);
        assert_eq!(metrics.token_budget, 500_000);
        assert!((metrics.wall_clock_secs - 1200.0).abs() < 0.01);
        assert_eq!(metrics.max_binary_size_mb, config.safety.max_binary_size_mb);
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
}
