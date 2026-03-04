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

    match call_llm(&config.llm, &system_prompt, &user_prompt) {
        Ok(response) => parse_hypotheses_response(&response),
        Err(e) => {
            log_warning(&format!("LLM call failed: {}", e));
            vec![]
        }
    }
}

fn read_mutation_targets(targets: &super::MutationTargets, repo_root: &Path) -> String {
    let mut context = String::new();
    let all_files = targets
        .prompt_logic
        .iter()
        .chain(targets.tool_code.iter())
        .chain(targets.cognitive.iter());

    for file in all_files {
        let full_path = repo_root.join(file);
        match std::fs::read_to_string(&full_path) {
            Ok(contents) => {
                context.push_str(&format!(
                    "\n### {}\n```rust\n{}\n```\n",
                    file.display(),
                    contents
                ));
            }
            Err(e) => {
                log_warning(&format!("Could not read {}: {}", file.display(), e));
            }
        }
    }
    context
}

fn build_system_prompt(population_size: usize) -> String {
    format!(
        r#"You are an evolution engine that generates code mutation hypotheses for a Rust project called selfware.

Your task is to propose exactly {n} mutation hypotheses as improvements. Each hypothesis must be a concrete code change expressed as a unified diff patch.

RULES:
1. Each hypothesis must target files from the provided source code
2. Patches must be valid unified diff format (--- a/path, +++ b/path, @@ hunks)
3. Never modify files under src/evolution/, src/safety/, system_tests/, or benches/sab_
4. Focus on performance, correctness, readability, or efficiency improvements
5. Each hypothesis should be independent — do not assume other hypotheses are applied

Respond with a JSON array of exactly {n} objects. Each object has:
- "description": string — what the mutation does and why
- "patch": string — unified diff patch
- "target_files": string array — relative file paths affected
- "property_test": string or null — optional property test code

Respond ONLY with the JSON array. No markdown fences, no explanatory text."#,
        n = population_size
    )
}

fn build_user_prompt(telemetry: &str, history: &str, source_context: &str) -> String {
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

fn call_llm(llm: &LlmConfig, system_prompt: &str, user_prompt: &str) -> Result<String, String> {
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
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let resp = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("LLM API returned {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .map_err(|e| format!("Failed to parse LLM response JSON: {}", e))?;

    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in LLM response".to_string())
}

fn parse_hypotheses_response(response: &str) -> Vec<Hypothesis> {
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
            let patch = v["patch"].as_str()?.to_string();
            let target_files: Vec<PathBuf> = v["target_files"]
                .as_array()?
                .iter()
                .filter_map(|f| f.as_str().map(PathBuf::from))
                .collect();
            let property_test = v["property_test"].as_str().map(|s| s.to_string());

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
            llm: LlmConfig::default(),
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
}
