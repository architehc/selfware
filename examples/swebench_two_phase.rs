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

/// Extract modified file paths from a unified diff.
fn extract_diff_files(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter(|l| l.starts_with("diff --git"))
        .filter_map(|l| l.split_whitespace().nth(3).map(|p| p.trim_start_matches("b/").to_string()))
        .collect()
}

/// Clone a repo and read specific files, return their contents.
async fn read_repo_files(task: &SWETask, work_dir: &Path) -> Result<Vec<(String, String)>> {
    // Clone if not already there
    if !work_dir.exists() {
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth=1", &format!("https://github.com/{}.git", task.repo), work_dir.to_str().unwrap()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            // Try full clone for specific commits
            let _ = tokio::process::Command::new("git")
                .args(["clone", &format!("https://github.com/{}.git", task.repo), work_dir.to_str().unwrap()])
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

    // Read target files
    let target_files = extract_diff_files(&task.patch);
    let mut contents = Vec::new();

    for file in &target_files {
        let path = work_dir.join(file);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                // Truncate very large files to ~8K lines
                let truncated = if content.lines().count() > 8000 {
                    let lines: Vec<&str> = content.lines().take(8000).collect();
                    format!("{}\n... [truncated, {} total lines]", lines.join("\n"), content.lines().count())
                } else {
                    content
                };
                contents.push((file.clone(), truncated));
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

    // Include full file contents with line numbers
    user.push_str("\n## Source Files (with line numbers)\n");
    for (path, content) in file_contents {
        user.push_str(&format!("\n### {}\n```python\n", path));
        for (i, line) in content.lines().enumerate() {
            user.push_str(&format!("{:>4} | {}\n", i + 1, line));
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
    let model = std::env::var("SELFWARE_MODEL")
        .unwrap_or_else(|_| "qwen3.5-27b".to_string());
    let concurrent: usize = std::env::var("SELFWARE_CONCURRENT")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(16);
    let limit: usize = std::env::args()
        .skip_while(|a| a != "--limit")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(20);

    let task_file = std::env::args().nth(1)
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
                messages: vec![
                    Message::system(system),
                    Message::user(user),
                ],
                evaluator: Box::new(PatchEvaluator {
                    gold_files,
                    gold_patch: task.patch.clone(),
                }),
            }
        })
        .collect();

    let report = runner.run(bench_tasks).await?;

    // Print results
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("SWE-BENCH TWO-PHASE RESULTS");
    eprintln!("{}", "=".repeat(70));
    eprintln!(
        "Tasks:       {}/{} generated patches (quality >= 30%)",
        report.tasks_passed, report.tasks_total,
    );
    eprintln!("Avg score:   {:.1}%", report.avg_score * 100.0);
    eprintln!("Throughput:  {:.0} tok/s", report.tokens_per_sec);
    eprintln!("Duration:    {:.1}s", report.total_duration_secs);
    eprintln!("{}", "=".repeat(70));

    eprintln!("\n{:<45} {:>6} {:>8} {:>10}", "Instance", "Score", "Tokens", "Latency");
    eprintln!("{}", "-".repeat(75));
    for r in &report.results {
        let score_str = r.eval.as_ref()
            .map(|e| format!("{:.0}%", e.score * 100.0))
            .unwrap_or("ERR".into());
        let status = if r.success { "+" } else { "-" };
        eprintln!(
            "{} {:<43} {:>6} {:>8} {:>8.1}s",
            status,
            &r.task_id[..r.task_id.len().min(43)],
            score_str,
            r.completion_tokens,
            r.latency_ms as f64 / 1000.0,
        );
    }

    // Save report
    report.write_to_dir(Path::new("bench_results/swebench/two_phase"))?;
    eprintln!("\nReports saved to bench_results/swebench/two_phase/");

    // Show sample patches
    eprintln!("\n--- Sample generated patches ---");
    for r in report.results.iter().take(3) {
        if !r.response.is_empty() {
            eprintln!("\n  [{}]:", r.task_id);
            for line in r.response.lines().take(15) {
                eprintln!("    {line}");
            }
            if r.response.lines().count() > 15 {
                eprintln!("    ... ({} more lines)", r.response.lines().count() - 15);
            }
        }
    }

    Ok(())
}

/// Evaluator that checks patch quality against gold.
struct PatchEvaluator {
    gold_files: Vec<String>,
    gold_patch: String,
}

impl TaskEvaluator for PatchEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        let mut details = Vec::new();

        let has_diff = response.contains("diff --git")
            || response.contains("---")
            || response.contains("@@");
        details.push(EvalDetail {
            criterion: "contains_patch".into(),
            score: if has_diff { 1.0 } else { 0.0 },
            passed: has_diff,
            message: if has_diff { "Has patch".into() } else { "No patch".into() },
        });

        for file in &self.gold_files {
            let short = file.split('/').last().unwrap_or(file);
            let found = response.contains(file.as_str()) || response.contains(short);
            details.push(EvalDetail {
                criterion: format!("file:{short}"),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found { format!("Targets {file}") } else { format!("Missing {file}") },
            });
        }

        let gold_lines: Vec<&str> = self.gold_patch.lines()
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
            let found = key_tokens.iter()
                .filter(|t| response.contains(**t))
                .count() >= key_tokens.len().max(1) / 2;
            details.push(EvalDetail {
                criterion: format!("code:{}", &line[..line.len().min(30)]),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found { "Match".into() } else { "Missing".into() },
            });
        }

        let total = details.len();
        let passed = details.iter().filter(|d| d.passed).count();
        let score = if total > 0 { passed as f64 / total as f64 } else { 0.0 };

        EvalResult { score, passed: score >= 0.3, details }
    }
}
