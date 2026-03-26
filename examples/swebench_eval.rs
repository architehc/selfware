//! SWE-bench Lite evaluation: send problem statements to the LLM with 16
//! concurrent streams and evaluate the generated patches against gold patches.
//!
//! Run with: cargo run --features bench-harness --example swebench_eval

use std::path::Path;

use selfware::api::types::Message;
use selfware::bench_harness::*;

/// A SWE-bench task loaded from JSON.
#[derive(Debug, Clone, serde::Deserialize)]
struct SWETask {
    repo: String,
    instance_id: String,
    problem_statement: String,
    hints_text: String,
    patch: String,
    #[serde(default)]
    test_patch: String,
    #[serde(default)]
    version: String,
    #[serde(rename = "FAIL_TO_PASS", default)]
    fail_to_pass: String,
}

/// Evaluator that checks if the generated patch touches the same files as the gold patch.
struct PatchEvaluator {
    gold_files: Vec<String>,
    gold_patch: String,
}

impl TaskEvaluator for PatchEvaluator {
    fn evaluate(&self, response: &str) -> EvalResult {
        let mut details = Vec::new();

        // 1. Check if response contains a diff/patch
        let has_diff =
            response.contains("diff --git") || response.contains("---") || response.contains("@@");
        details.push(EvalDetail {
            criterion: "contains_patch".into(),
            score: if has_diff { 1.0 } else { 0.0 },
            passed: has_diff,
            message: if has_diff {
                "Response contains a patch/diff".into()
            } else {
                "No patch/diff found in response".into()
            },
        });

        // 2. Check if the right files are modified
        let mut files_hit = 0;
        for file in &self.gold_files {
            let short = file.split('/').last().unwrap_or(file);
            let found = response.contains(file.as_str()) || response.contains(short);
            if found {
                files_hit += 1;
            }
            details.push(EvalDetail {
                criterion: format!("file:{}", short),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    format!("Modifies {file}")
                } else {
                    format!("Does not modify {file}")
                },
            });
        }

        // 3. Check for key code patterns from the gold patch
        let gold_lines: Vec<&str> = self
            .gold_patch
            .lines()
            .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
            .map(|l| l.trim_start_matches('+').trim())
            .filter(|l| l.len() > 10) // Skip trivial lines
            .take(5) // Check up to 5 key added lines
            .collect();

        let mut code_hits = 0;
        for line in &gold_lines {
            // Check if the response contains something similar
            // (exact match or key identifiers from the line)
            let key_tokens: Vec<&str> = line
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .filter(|t| t.len() > 3)
                .take(3)
                .collect();

            let found = key_tokens.iter().filter(|t| response.contains(**t)).count()
                >= key_tokens.len().max(1) / 2;

            if found {
                code_hits += 1;
            }
            details.push(EvalDetail {
                criterion: format!("code_pattern:{}", &line[..line.len().min(40)]),
                score: if found { 1.0 } else { 0.0 },
                passed: found,
                message: if found {
                    "Key code pattern found".into()
                } else {
                    "Key code pattern missing".into()
                },
            });
        }

        // Overall score
        let total = details.len();
        let passed = details.iter().filter(|d| d.passed).count();
        let score = if total > 0 {
            passed as f64 / total as f64
        } else {
            0.0
        };

        EvalResult {
            score,
            passed: score >= 0.3, // Lenient: 30% threshold
            details,
        }
    }
}

/// Extract modified file paths from a unified diff.
fn extract_diff_files(patch: &str) -> Vec<String> {
    patch
        .lines()
        .filter(|l| l.starts_with("diff --git"))
        .filter_map(|l| {
            // "diff --git a/path/to/file b/path/to/file"
            let parts: Vec<&str> = l.split_whitespace().collect();
            parts.get(3).map(|p| p.trim_start_matches("b/").to_string())
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware::bench_harness=debug")
        .init();

    // Load tasks
    let task_file = "bench_results/swebench_lite_20.json";
    if !Path::new(task_file).exists() {
        eprintln!("Task file not found: {task_file}");
        eprintln!("Run the Python export script first.");
        return Ok(());
    }

    let content = std::fs::read_to_string(task_file)?;
    let swe_tasks: Vec<SWETask> = serde_json::from_str(&content)?;

    let config = HarnessConfig {
        endpoint: "http://localhost:8000/v1".into(),
        model: "qwen3.5-27b".into(),
        max_concurrent: 16,
        max_tokens: 8192,
        temperature: 0.2,
        timeout_secs: 300,
        output_dir: "bench_results/swebench".into(),
        extra_body: serde_json::json!({
            "chat_template_kwargs": {"enable_thinking": false}
        }),
    };

    let runner = HarnessRunner::new(config.clone())?;

    eprintln!("\n=== SWE-bench Lite Evaluation ===");
    eprintln!("Endpoint: {}", config.endpoint);
    eprintln!("Model: {}", config.model);
    eprintln!("Tasks: {}", swe_tasks.len());
    eprintln!("Concurrency: {}", config.max_concurrent);
    eprintln!();

    // Build benchmark tasks
    let bench_tasks: Vec<BenchTask> = swe_tasks
        .iter()
        .map(|task| {
            let gold_files = extract_diff_files(&task.patch);
            let gold_patch = task.patch.clone();

            let system_msg = format!(
                "You are an expert software engineer. You are given a bug report for the {} repository (version {}). \
                 Generate a COMPLETE unified diff patch that fixes the issue. \
                 The patch MUST be a valid unified diff that can be applied with `git apply`. \
                 Include correct line numbers and sufficient context lines (3 lines before and after changes). \
                 Make sure every line ends with a newline character. \
                 Output ONLY the diff inside a code fence, no explanation before or after:\n\
                 ```diff\n\
                 diff --git a/path/to/file b/path/to/file\n\
                 --- a/path/to/file\n\
                 +++ b/path/to/file\n\
                 @@ -start,count +start,count @@\n\
                 <context lines>\n\
                 -<removed lines>\n\
                 +<added lines>\n\
                 <context lines>\n\
                 ```\n\
                 IMPORTANT: The patch must be complete and not truncated. Include ALL necessary changes.",
                task.repo, task.version,
            );

            let user_msg = format!(
                "## Bug Report\n\n{}\n\n{}",
                task.problem_statement,
                if task.hints_text.is_empty() {
                    String::new()
                } else {
                    format!("## Hints\n\n{}", task.hints_text)
                },
            );

            BenchTask {
                id: task.instance_id.clone(),
                description: format!("{}: {}", task.repo, &task.instance_id),
                messages: vec![
                    Message::system(system_msg),
                    Message::user(user_msg),
                ],
                evaluator: Box::new(PatchEvaluator { gold_files, gold_patch }),
            }
        })
        .collect();

    let report = runner.run(bench_tasks).await?;

    // Print detailed results
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("SWE-BENCH LITE RESULTS");
    eprintln!("{}", "=".repeat(70));
    eprintln!(
        "Tasks:       {}/{} passed (patch quality >= 30%)",
        report.tasks_passed, report.tasks_total
    );
    eprintln!("Avg score:   {:.1}%", report.avg_score * 100.0);
    eprintln!("Throughput:  {:.0} tok/s", report.tokens_per_sec);
    eprintln!("Duration:    {:.1}s", report.total_duration_secs);
    eprintln!("Latency p50: {:.1}s", report.latency_p50_ms as f64 / 1000.0);
    eprintln!("Latency p95: {:.1}s", report.latency_p95_ms as f64 / 1000.0);
    eprintln!("{}", "=".repeat(70));

    // Per-task breakdown
    eprintln!(
        "\n{:<45} {:>6} {:>8} {:>10}",
        "Instance", "Score", "Tokens", "Latency"
    );
    eprintln!("{}", "-".repeat(75));
    for r in &report.results {
        let score_str = r
            .eval
            .as_ref()
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

        // Show eval details for failures
        if !r.success {
            if let Some(eval) = &r.eval {
                for d in &eval.details {
                    if !d.passed {
                        eprintln!("    x {}: {}", d.criterion, d.message);
                    }
                }
            }
            if let Some(err) = &r.error {
                eprintln!("    ! {}", &err[..err.len().min(80)]);
            }
        }
    }

    // Write reports
    report.write_to_dir(Path::new("bench_results/swebench"))?;
    eprintln!("\nReports written to bench_results/swebench/");

    Ok(())
}
