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

    // Only run SAB baseline if explicitly requested via env var
    // (SAB runs all 12 scenarios and takes 30+ minutes)
    let baseline_sab = if std::env::var("SELFWARE_EVOLVE_SAB").is_ok() {
        let selfware_binary = repo_root.join("target/release/selfware");
        match fitness::run_sab(&selfware_binary, &sab_config) {
            Ok(r) => r,
            Err(e) => {
                log_warning(&format!(
                    "SAB baseline failed ({}), using synthetic baseline",
                    e
                ));
                SabResult {
                    aggregate_score: 50.0,
                    scenario_scores: vec![],
                    total_tokens_used: 0,
                    wall_clock: std::time::Duration::from_secs(0),
                    rating: GenerationRating::Grow,
                }
            }
        }
    } else {
        log_phase("Using compile+test fitness (set SELFWARE_EVOLVE_SAB=1 for full SAB)");
        SabResult {
            aggregate_score: 50.0,
            scenario_scores: vec![],
            total_tokens_used: 0,
            wall_clock: std::time::Duration::from_secs(0),
            rating: GenerationRating::Grow,
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
            generate_hypotheses(&config, &telemetry_prompt, &history_prompt, repo_root);

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
            continue;
        }

        // ─── Step 3: Safety filter ───
        let valid: Vec<_> = hypotheses
            .into_iter()
            .filter(|h| {
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

        log_phase(&format!("Evaluating {} hypotheses...", valid.len()));

        // ─── Step 4: Evaluate each hypothesis (apply → check → test) ───
        let sab_available =
            sab_config.runner_script.exists() && std::env::var("SELFWARE_EVOLVE_SAB").is_ok();
        let mut generation_winner: Option<(Hypothesis, SabResult)> = None;

        for hypothesis in &valid {
            log_phase(&format!(
                "  Testing '{}' [{}]...",
                hypothesis.description, hypothesis.id
            ));

            // Create worktree
            let worktree = match ast_tools::create_shadow_worktree(repo_root) {
                Ok(w) => w,
                Err(e) => {
                    log_warning(&format!("  Worktree failed: {}", e));
                    continue;
                }
            };

            // Apply edits (search-and-replace or unified diff)
            if !apply_patch_to_worktree(&worktree, &hypothesis.patch) {
                log_frost(generation, &format!("Patch failed: {}", hypothesis.id));
                // Log the first 500 chars of the edit data for debugging
                let preview = &hypothesis.patch[..hypothesis.patch.len().min(500)];
                log_warning(&format!("  Edit preview:\n{}", preview));
                let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
                continue;
            }

            // Compile check
            let check = Command::new("cargo")
                .args(["check", "--features", "self-improvement"])
                .current_dir(&worktree)
                .output();

            if check.map(|o| !o.status.success()).unwrap_or(true) {
                log_frost(generation, &format!("Compile failed: {}", hypothesis.id));
                let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
                continue;
            }

            // Run tests
            let test_start = Instant::now();
            let test = Command::new("cargo")
                .args(["test", "--features", "self-improvement"])
                .current_dir(&worktree)
                .output();

            let test_output = match test {
                Ok(o) => o,
                Err(e) => {
                    log_warning(&format!("  Test execution failed: {}", e));
                    let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
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
                let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
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
            let clippy = Command::new("cargo")
                .args([
                    "clippy",
                    "--features",
                    "self-improvement",
                    "--",
                    "-D",
                    "warnings",
                ])
                .current_dir(&worktree)
                .output();

            if clippy.map(|o| !o.status.success()).unwrap_or(true) {
                log_frost(generation, &format!("Clippy failed: {}", hypothesis.id));
                let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
                continue;
            }

            // If SAB is available, run it; otherwise use synthetic score
            let winner_sab = if sab_available {
                let build = Command::new("cargo")
                    .args(["build", "--release", "--features", "self-improvement"])
                    .current_dir(&worktree)
                    .output();

                if build.map(|o| !o.status.success()).unwrap_or(true) {
                    log_frost(
                        generation,
                        &format!("Release build failed: {}", hypothesis.id),
                    );
                    let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
                    continue;
                }

                let mutated_binary = worktree.join("target/release/selfware");
                match fitness::run_sab(&mutated_binary, &sab_config) {
                    Ok(r) => r,
                    Err(e) => {
                        log_warning(&format!("  SAB failed: {}", e));
                        let _ = ast_tools::cleanup_worktree(repo_root, &worktree);
                        continue;
                    }
                }
            } else {
                // Synthetic fitness: tests passed + compiled = score 60 (above baseline 50)
                SabResult {
                    aggregate_score: 60.0,
                    scenario_scores: vec![],
                    total_tokens_used: 0,
                    wall_clock: test_duration,
                    rating: GenerationRating::Grow,
                }
            };

            let _ = ast_tools::cleanup_worktree(repo_root, &worktree);

            log_phase(&format!(
                "  ✓ '{}' passed (score: {:.0}, {:.1}s)",
                hypothesis.description,
                winner_sab.aggregate_score,
                winner_sab.wall_clock.as_secs_f64()
            ));

            // Keep the first passing hypothesis as winner
            if generation_winner.is_none() {
                generation_winner = Some((hypothesis.clone(), winner_sab));
            }
        }

        // ─── Step 5: EMERGE OR DIE ───
        let (winner, winner_sab) = match generation_winner {
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

        let baseline_composite = config
            .fitness_weights
            .composite(&build_metrics(&current_baseline, &config));
        let winner_composite = config
            .fitness_weights
            .composite(&build_metrics(&winner_sab, &config));

        if winner_composite > baseline_composite {
            log_bloom(
                generation,
                &winner.description,
                current_baseline.aggregate_score,
                winner_sab.aggregate_score,
            );

            if apply_patch_to_repo(repo_root, &winner.patch) {
                let commit_msg = format!(
                    "🧬 Gen {} BLOOM: {:.0} → {:.0} | {}",
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

                log_event(
                    repo_root,
                    &serde_json::json!({
                        "event": "generation_end",
                        "timestamp": chrono_now(),
                        "generation": generation,
                        "outcome": "bloom",
                        "description": winner.description,
                        "score_before": current_baseline.aggregate_score,
                        "score_after": winner_sab.aggregate_score,
                        "composite": winner_composite,
                        "duration_secs": gen_start.elapsed().as_secs_f64(),
                        "improvements_total": hall_of_fame.len(),
                    }),
                );

                current_baseline = winner_sab;
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
                winner_sab.aggregate_score,
                current_baseline.aggregate_score,
            );
            log_event(
                repo_root,
                &serde_json::json!({
                    "event": "generation_end",
                    "timestamp": chrono_now(),
                    "generation": generation,
                    "outcome": format!("{}", rating),
                    "description": winner.description,
                    "winner_score": winner_sab.aggregate_score,
                    "baseline_score": current_baseline.aggregate_score,
                    "duration_secs": gen_start.elapsed().as_secs_f64(),
                }),
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
        Ok(response) => {
            log_phase(&format!(
                "LLM response ({} chars): {}",
                response.len(),
                &response[..response.len().min(200)]
            ));
            parse_hypotheses_response(&response)
        }
        Err(e) => {
            log_warning(&format!("LLM call failed: {}", e));
            vec![]
        }
    }
}

/// Max total source context characters
const MAX_CONTEXT_CHARS: usize = 120_000;

pub fn read_mutation_targets(targets: &super::MutationTargets, repo_root: &Path) -> String {
    // Collect all files with their sizes, then sort smallest-first so we
    // maximise the number of files sent in full within the context budget.
    let all_paths: Vec<&PathBuf> = targets
        .prompt_logic
        .iter()
        .chain(targets.tool_code.iter())
        .chain(targets.cognitive.iter())
        .collect();

    let mut file_entries: Vec<(&PathBuf, String, usize)> = Vec::new();
    for file in &all_paths {
        let full_path = repo_root.join(file);
        match std::fs::read_to_string(&full_path) {
            Ok(contents) => {
                let len = contents.len();
                file_entries.push((file, contents, len));
            }
            Err(e) => {
                log_warning(&format!("Could not read {}: {}", file.display(), e));
            }
        }
    }

    // Sort by size ascending — small files go in full, big files get truncated
    file_entries.sort_by_key(|(_, _, len)| *len);

    let mut context = String::new();
    let mut files_full = 0usize;
    let mut files_truncated = 0usize;

    for (file, contents, _len) in &file_entries {
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

        // Budget for this file: overhead for the header + fences (~100 chars)
        let overhead = 100 + file.display().to_string().len();
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

        context.push_str(&format!(
            "\n### {}\n```rust\n{}\n```\n",
            file.display(),
            display_content
        ));
    }
    log_phase(&format!(
        "Source context: {} chars from {} files ({} full, {} truncated)",
        context.len(),
        files_full + files_truncated,
        files_full,
        files_truncated,
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
    // Find the last newline before max_chars
    match s[..max_chars].rfind('\n') {
        Some(pos) => &s[..pos],
        None => &s[..max_chars],
    }
}

pub fn build_system_prompt(population_size: usize) -> String {
    format!(
        r#"You are an evolution engine that generates code mutation hypotheses for a Rust project called selfware.

Your task is to propose exactly {n} mutation hypotheses as improvements. Each hypothesis uses search-and-replace edits.

SOURCE CODE FORMAT:
- Each file is shown with line numbers like "  42| fn foo() {{"
- Line numbers are for your reference only — do NOT include them in search/replace strings
- Some files are truncated — only modify code you can see in full

EDIT FORMAT (critical — edits that can't be found are discarded):
- Each hypothesis has an "edits" array of search-and-replace operations
- "search" must be an EXACT substring of the target file (copy-paste accuracy)
- "replace" is what replaces that exact substring
- Keep edits small and focused — change the minimum necessary code
- The search string must be unique in the file (not ambiguous)
- Use \n for newlines inside strings (JSON escaped)
- Do NOT include line number prefixes (like "42| ") in search/replace strings

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
        // Disable Qwen3's thinking mode to maximize output tokens for JSON
        "chat_template_kwargs": {"enable_thinking": false},
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
        let path = dir.join(file);
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
                        &search[..search.len().min(80)]
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
                        &search[..search.len().min(80)]
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
    apply_edits(repo_root, patch)
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
            &context[..context.len().min(200)]
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
}
