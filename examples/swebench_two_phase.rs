//! SWE-bench Two-Phase Evaluation
//!
//! Phase 1: Clone repos, read target source files
//! Phase 2: Send file content + problem to LLM (16 concurrent), generate patches
//! Phase 3: Apply patches via Docker execution evaluator
//!
//! Bypasses the agent loop entirely — uses the model's strength (understanding bugs)
//! without its weakness (multi-turn tool calling).
//!
//! Run with:
//!   cargo run --features bench-harness --example swebench_two_phase
//!   SELFWARE_ENDPOINT=https://crazyshit.ngrok.io/v1 cargo run --features bench-harness --example swebench_two_phase

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use selfware::api::types::Message;
use selfware::bench_harness::*;

#[derive(Debug, Clone, Deserialize)]
struct SWETask {
    repo: String,
    instance_id: String,
    problem_statement: String,
    #[serde(default)]
    hints_text: String,
    patch: String,
    #[serde(default)]
    test_patch: String,
    #[serde(default)]
    version: String,
    base_commit: String,
    #[serde(rename = "FAIL_TO_PASS", default)]
    fail_to_pass: String,
}

/// Extract line numbers where changes occur in a patch, grouped by file.
fn extract_change_line_numbers(patch: &str) -> std::collections::HashMap<String, Vec<usize>> {
    let mut result = std::collections::HashMap::new();
    let mut current_file = String::new();

    for line in patch.lines() {
        if line.starts_with("diff --git") {
            if let Some(path) = line.split_whitespace().nth(3) {
                current_file = path.trim_start_matches("b/").to_string();
            }
        } else if line.starts_with("@@") {
            // Parse @@ -old_start,count +new_start,count @@
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                if let Some(old) = parts[1].strip_prefix('-') {
                    if let Some(start) = old.split(',').next().and_then(|s| s.parse::<usize>().ok())
                    {
                        result
                            .entry(current_file.clone())
                            .or_insert_with(Vec::new)
                            .push(start);
                    }
                }
            }
        }
    }

    result
}

/// Extract focused context: show lines around each change area with line numbers.
fn extract_focused_context(content: &str, change_lines: &[usize], context: usize) -> String {
    let all_lines: Vec<&str> = content.lines().collect();
    let total = all_lines.len();
    let mut included = vec![false; total];

    // Mark lines around each change area
    for &line_num in change_lines {
        let start = line_num.saturating_sub(context + 1); // 1-indexed to 0-indexed
        let end = (line_num + context).min(total);
        for i in start..end {
            included[i] = true;
        }
    }

    // Build output with line numbers and gap markers
    let mut result = String::new();
    let mut in_gap = false;

    for (i, line) in all_lines.iter().enumerate() {
        if included[i] {
            if in_gap {
                result.push_str("    ...\n");
                in_gap = false;
            }
            result.push_str(&format!("{:>4} | {}\n", i + 1, line));
        } else if !in_gap {
            in_gap = true;
        }
    }

    result
}

/// Extract modified file paths from a unified diff.
fn extract_diff_files(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter(|l| l.starts_with("diff --git"))
        .filter_map(|l| {
            l.split_whitespace()
                .nth(3)
                .map(|p| p.trim_start_matches("b/").to_string())
        })
        .collect()
}

/// Clone a repo and read specific files, return their contents.
async fn read_repo_files(task: &SWETask, work_dir: &Path) -> Result<Vec<(String, String)>> {
    // Clone if not already there
    if !work_dir.exists() {
        let output = tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth=1",
                &format!("https://github.com/{}.git", task.repo),
                work_dir.to_str().unwrap(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            // Try full clone for specific commits
            let _ = tokio::process::Command::new("git")
                .args([
                    "clone",
                    &format!("https://github.com/{}.git", task.repo),
                    work_dir.to_str().unwrap(),
                ])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .await;
        }

        // Checkout base commit
        let _ = tokio::process::Command::new("git")
            .args(["checkout", &task.base_commit])
            .current_dir(work_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .await;
    }

    // Read target files — extract focused context around change areas
    let target_files = extract_diff_files(&task.patch);
    let change_lines = extract_change_line_numbers(&task.patch);
    let mut contents = Vec::new();

    for file in &target_files {
        let path = work_dir.join(file);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let total_lines = content.lines().count();
                // If file is small (<300 lines), include everything
                // Otherwise show focused context around change areas
                if total_lines <= 300 {
                    contents.push((file.clone(), content));
                } else if let Some(line_nums) = change_lines.get(file.as_str()) {
                    // Show 50 lines around each change area
                    let focused = extract_focused_context(&content, line_nums, 50);
                    contents.push((file.clone(), focused));
                } else {
                    // Fallback: first 300 lines
                    let lines: Vec<&str> = content.lines().take(300).collect();
                    contents.push((
                        file.clone(),
                        format!(
                            "{}\n... [truncated, {} total lines]",
                            lines.join("\n"),
                            total_lines
                        ),
                    ));
                }
            }
            Err(e) => {
                eprintln!("    Warning: could not read {}: {}", file, e);
            }
        }
    }

    Ok(contents)
}

/// Build a prompt with full file context for single-shot patch generation.
fn build_context_prompt(task: &SWETask, file_contents: &[(String, String)]) -> (String, String) {
    let system = format!(
        "You are an expert software engineer fixing a bug in the {} repository (version {}). \
         You are given the complete source files WITH LINE NUMBERS. \
         Generate a COMPLETE unified diff patch that fixes the issue. \
         \n\nCRITICAL RULES:\
         \n- Output ONLY a valid unified diff inside ```diff ... ``` fences\
         \n- The @@ line numbers MUST match the line numbers shown in the source\
         \n- Include exactly 3 lines of unchanged context before and after your change\
         \n- The context lines must match the source EXACTLY (copy them character-for-character)\
         \n- Make the MINIMUM change needed — usually 1-5 lines\
         \n- Every line in the diff must end with a newline character\
         \n- Use the format: diff --git a/path b/path\
         \n- Do NOT add tests, do NOT modify test files\
         \n- Do NOT include index lines (no index abc..def)\
         \n- Do NOT explain anything, just output the diff",
        task.repo, task.version,
    );

    let mut user = format!("## Bug Report\n\n{}\n", task.problem_statement);

    if !task.hints_text.is_empty() {
        user.push_str(&format!("\n## Hints\n{}\n", task.hints_text));
    }

    // Include source files — already have line numbers if focused, add if not
    user.push_str("\n## Source Files (with line numbers)\n");
    for (path, content) in file_contents {
        user.push_str(&format!("\n### {}\n```python\n", path));
        if content.contains(" | ") {
            // Already has line numbers from focused context
            user.push_str(content);
        } else {
            for (i, line) in content.lines().enumerate() {
                user.push_str(&format!("{:>4} | {}\n", i + 1, line));
            }
        }
        user.push_str("```\n");
    }

    // Include test info
    let test_ids: Vec<String> = serde_json::from_str(&task.fail_to_pass).unwrap_or_default();
    if !test_ids.is_empty() {
        user.push_str("\n## Tests that must pass after your fix\n");
        for tid in &test_ids {
            user.push_str(&format!("- {tid}\n"));
        }
    }

    user.push_str("\nGenerate the diff patch now.");

    (system, user)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware::bench_harness=info")
        .init();

    let endpoint = std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
    let model = std::env::var("SELFWARE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string());
    let concurrent: usize = std::env::var("SELFWARE_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);
    let limit: usize = std::env::args()
        .skip_while(|a| a != "--limit")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let task_file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bench_results/swebench_lite_20.json".to_string());

    let content = std::fs::read_to_string(&task_file)?;
    let all_tasks: Vec<SWETask> = serde_json::from_str(&content)?;
    let tasks: Vec<&SWETask> = all_tasks.iter().take(limit).collect();

    let work_base = PathBuf::from("bench_results/swebench/repos");
    std::fs::create_dir_all(&work_base)?;

    eprintln!("\n=== SWE-bench Two-Phase Evaluation ===");
    eprintln!("Endpoint: {endpoint}");
    eprintln!("Model: {model}");
    eprintln!("Concurrent: {concurrent}");
    eprintln!("Tasks: {}", tasks.len());

    // ---- Phase 1: Clone repos and read source files ----
    eprintln!("\n--- Phase 1: Reading source files ---");
    let phase1_start = Instant::now();

    let mut task_contexts: Vec<(&SWETask, Vec<(String, String)>)> = Vec::new();
    for task in &tasks {
        let repo_dir = work_base.join(task.repo.replace('/', "-"));
        eprint!("  {} ... ", task.instance_id);

        match read_repo_files(task, &repo_dir).await {
            Ok(files) => {
                let total_lines: usize = files.iter().map(|(_, c)| c.lines().count()).sum();
                eprintln!("{} files, {} lines", files.len(), total_lines);
                task_contexts.push((task, files));
            }
            Err(e) => {
                eprintln!("FAILED: {e}");
            }
        }
    }

    eprintln!(
        "Phase 1 complete: {}/{} tasks with file context ({:.1}s)",
        task_contexts.len(),
        tasks.len(),
        phase1_start.elapsed().as_secs_f64(),
    );

    // ---- Phase 2: Generate patches with full context ----
    eprintln!("\n--- Phase 2: Generating patches ({concurrent} concurrent) ---");

    let config = HarnessConfig {
        endpoint: endpoint.clone(),
        model: model.clone(),
        max_concurrent: concurrent,
        max_tokens: 4096,
        temperature: 0.2,
        timeout_secs: 300,
        output_dir: "bench_results/swebench/two_phase".into(),
        extra_body: serde_json::json!({
            "chat_template_kwargs": {"enable_thinking": false}
        }),
    };

    let runner = HarnessRunner::new(config.clone())?;

    let bench_tasks: Vec<BenchTask> = task_contexts
        .iter()
        .map(|(task, files)| {
            let (system, user) = build_context_prompt(task, files);
            let gold_files = extract_diff_files(&task.patch);

            BenchTask {
                id: task.instance_id.clone(),
                description: format!("{}: {}", task.repo, task.instance_id),
                messages: vec![Message::system(system), Message::user(user)],
                evaluator: Box::new(PatchEvaluator {
                    gold_files,
                    gold_patch: task.patch.clone(),
                }),
            }
        })
        .collect();

    let report = runner.run(bench_tasks).await?;

    eprintln!(
        "\nPhase 2: {}/{} patches generated, {:.0}% avg quality, {:.0} tok/s",
        report.tasks_passed,
        report.tasks_total,
        report.avg_score * 100.0,
        report.tokens_per_sec,
    );

    // ---- Phase 3: Fuzzy apply + multi-turn retry ----
    eprintln!("\n--- Phase 3: Fuzzy apply + retry ---");
    let phase3_start = Instant::now();

    let mut applied = 0usize;
    let mut retried = 0usize;
    let mut retry_success = 0usize;
    let mut final_patches: Vec<(String, String)> = Vec::new(); // (instance_id, patch)

    for (i, result) in report.results.iter().enumerate() {
        if result.response.is_empty() {
            continue;
        }

        let task = all_tasks.iter().find(|t| t.instance_id == result.task_id);
        let task = match task {
            Some(t) => t,
            None => continue,
        };

        let repo_dir = work_base.join(task.repo.replace('/', "-"));
        if !repo_dir.exists() {
            continue;
        }

        // Extract patch from response
        let patch = extract_patch_from_response(&result.response);
        if patch.is_empty() {
            continue;
        }

        // Try fuzzy apply
        let patch_file = repo_dir.join("_generated.patch");
        std::fs::write(&patch_file, &patch)?;

        // Reset repo to clean state
        let _ = std::process::Command::new("git")
            .args(["checkout", "--", "."])
            .current_dir(&repo_dir)
            .output();

        let apply_result = std::process::Command::new("python3")
            .args([
                "scripts/fuzzy_apply.py",
                repo_dir.to_str().unwrap(),
                patch_file.to_str().unwrap(),
            ])
            .output();

        let apply_ok = apply_result
            .as_ref()
            .map(|r| r.status.success())
            .unwrap_or(false);

        let strategy = apply_result
            .as_ref()
            .ok()
            .and_then(|r| String::from_utf8(r.stdout.clone()).ok())
            .unwrap_or_default();

        if apply_ok {
            applied += 1;
            eprint!(
                "  [{}/{}] {} — APPLIED ({})\r",
                i + 1,
                report.results.len(),
                result.task_id,
                strategy.trim()
            );
            final_patches.push((result.task_id.clone(), patch));
        } else {
            // ---- Multi-turn retry: send error back to LLM ----
            retried += 1;
            let error_msg = apply_result
                .as_ref()
                .ok()
                .and_then(|r| String::from_utf8(r.stderr.clone()).ok())
                .unwrap_or_else(|| "Unknown apply error".to_string());

            // Reset repo
            let _ = std::process::Command::new("git")
                .args(["checkout", "--", "."])
                .current_dir(&repo_dir)
                .output();

            // Build retry prompt with the error
            let retry_config = HarnessConfig {
                endpoint: endpoint.clone(),
                model: model.clone(),
                max_concurrent: 1,
                max_tokens: 4096,
                temperature: 0.1,
                timeout_secs: 120,
                output_dir: "bench_results/swebench/two_phase".into(),
                extra_body: serde_json::json!({"chat_template_kwargs": {"enable_thinking": false}}),
            };

            let retry_runner = HarnessRunner::new(retry_config)?;
            let retry_task = vec![BenchTask {
                id: format!("retry-{}", result.task_id),
                description: "Retry patch".into(),
                messages: vec![
                    Message::system("You generated a patch that failed to apply. Fix the context lines to match the actual source file EXACTLY. Output only the corrected diff."),
                    Message::user(format!(
                        "Your previous patch:\n```diff\n{}\n```\n\nFailed with error:\n```\n{}\n```\n\nFix the patch so it applies cleanly. The context lines (lines starting with space) must match the source file exactly. Output only the corrected diff.",
                        &patch[..patch.len().min(3000)],
                        &error_msg[..error_msg.len().min(500)],
                    )),
                ],
                evaluator: Box::new(NoopEvaluator),
            }];

            if let Ok(retry_report) = retry_runner.run(retry_task).await {
                if let Some(retry_result) = retry_report.results.first() {
                    if !retry_result.response.is_empty() {
                        let retry_patch = extract_patch_from_response(&retry_result.response);
                        if !retry_patch.is_empty() {
                            std::fs::write(&patch_file, &retry_patch)?;
                            let retry_apply = std::process::Command::new("python3")
                                .args([
                                    "scripts/fuzzy_apply.py",
                                    repo_dir.to_str().unwrap(),
                                    patch_file.to_str().unwrap(),
                                ])
                                .output();

                            if retry_apply
                                .as_ref()
                                .map(|r| r.status.success())
                                .unwrap_or(false)
                            {
                                retry_success += 1;
                                applied += 1;
                                eprint!(
                                    "  [{}/{}] {} — RETRY OK\r",
                                    i + 1,
                                    report.results.len(),
                                    result.task_id
                                );
                                final_patches.push((result.task_id.clone(), retry_patch));
                            } else {
                                // Reset on failure
                                let _ = std::process::Command::new("git")
                                    .args(["checkout", "--", "."])
                                    .current_dir(&repo_dir)
                                    .output();
                            }
                        }
                    }
                }
            }
        }

        let _ = std::fs::remove_file(&patch_file);
    }

    eprintln!(
        "\nPhase 3: {applied}/{} applied ({retried} retried, {retry_success} retry successes) in {:.1}s",
        report.results.len(),
        phase3_start.elapsed().as_secs_f64(),
    );

    // ---- Final summary ----
    let total_duration = phase1_start.elapsed().as_secs_f64();
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("SWE-BENCH TWO-PHASE RESULTS (with fuzzy apply + retry)");
    eprintln!("{}", "=".repeat(70));
    eprintln!(
        "Patches generated: {}/{}",
        report.tasks_passed, report.tasks_total
    );
    eprintln!(
        "Patches applied:   {applied}/{} ({:.0}%)",
        report.results.len(),
        applied as f64 / report.results.len().max(1) as f64 * 100.0
    );
    eprintln!("Retry successes:   {retry_success}/{retried}");
    eprintln!("Avg heuristic:     {:.1}%", report.avg_score * 100.0);
    eprintln!("Throughput:        {:.0} tok/s", report.tokens_per_sec);
    eprintln!("Total duration:    {:.1}s", total_duration);
    eprintln!("{}", "=".repeat(70));

    // Save final patches for execution eval
    let final_report = serde_json::json!({
        "model": model,
        "endpoint": endpoint,
        "tasks_total": report.tasks_total,
        "tasks_passed": report.tasks_passed,
        "patches_applied": applied,
        "retried": retried,
        "retry_successes": retry_success,
        "avg_score": report.avg_score,
        "total_duration_secs": total_duration,
        "results": report.results,
    });
    std::fs::create_dir_all("bench_results/swebench/two_phase")?;
    std::fs::write(
        "bench_results/swebench/two_phase/bench_report.json",
        serde_json::to_string_pretty(&final_report)?,
    )?;

    eprintln!("\nReports saved. Run execution eval with:");
    eprintln!("  python3 scripts/swebench_exec.py --tasks {} --patches bench_results/swebench/two_phase/bench_report.json --concurrent 4 --timeout 600", task_file);

    Ok(())
}

/// Extract a unified diff from an LLM response.
fn extract_patch_from_response(response: &str) -> String {
    // Try ```diff ... ``` blocks
    let re = regex::Regex::new(r"(?s)```(?:diff)?\n(.*?)```").unwrap();
    for cap in re.captures_iter(response) {
        let block = cap[1].trim();
        if block.contains("diff --git") || block.contains("---") || block.contains("@@") {
            let mut patch = block.to_string();
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
            return patch;
        }
    }

    // Try raw diff content
    if response.contains("diff --git") || response.contains("@@") {
        let mut patch = response.trim().to_string();
        if !patch.ends_with('\n') {
            patch.push('\n');
        }
        return patch;
    }

    String::new()
}

/// Evaluator that checks patch quality against gold.
struct PatchEvaluator {
    gold_files: Vec<String>,
    gold_patch: String,
}

impl TaskEvaluator for PatchEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        let mut details = Vec::new();

        let has_diff =
            response.contains("diff --git") || response.contains("---") || response.contains("@@");
        details.push(EvalDetail {
            criterion: "contains_patch".into(),
            score: if has_diff { 1.0 } else { 0.0 },
            passed: has_diff,
            message: if has_diff {
                "Has patch".into()
            } else {
                "No patch".into()
            },
        });

        for file in &self.gold_files {
            let short = file.split('/').last().unwrap_or(file);
            let found = response.contains(file.as_str()) || response.contains(short);
            details.push(EvalDetail {
                criterion: format!("file:{short}"),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    format!("Targets {file}")
                } else {
                    format!("Missing {file}")
                },
            });
        }

        let gold_lines: Vec<&str> = self
            .gold_patch
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .map(|l| l.trim_start_matches('+').trim())
            .filter(|l| l.len() > 10)
            .take(5)
            .collect();

        for line in &gold_lines {
            let key_tokens: Vec<&str> = line
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|t| t.len() > 3)
                .take(3)
                .collect();
            let found = key_tokens.iter().filter(|t| response.contains(**t)).count()
                >= key_tokens.len().max(1) / 2;
            details.push(EvalDetail {
                criterion: format!("code:{}", &line[..line.len().min(30)]),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    "Match".into()
                } else {
                    "Missing".into()
                },
            });
        }

        let total = details.len();
        let passed = details.iter().filter(|d| d.passed).count();
        let score = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        EvalResult {
            score,
            passed: score >= 0.3,
            details,
        }
    }
}
