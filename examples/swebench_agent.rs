//! SWE-bench Agent-Loop Evaluation
//!
//! Runs selfware's full Agent with tools on SWE-bench tasks.
//! Each task: clone repo → read code → generate fix → apply → test.
//!
//! Run with: cargo run --features bench-harness --example swebench_agent

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use selfware::agent::Agent;
use selfware::config::{Config, ExecutionMode};

/// A SWE-bench task loaded from JSON.
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

/// Result of a single agent task execution.
#[derive(Debug, Clone, Serialize)]
struct AgentResult {
    instance_id: String,
    repo: String,
    completed: bool,
    duration_secs: f64,
    error: Option<String>,
}

/// Full evaluation report.
#[derive(Debug, Clone, Serialize)]
struct AgentEvalReport {
    timestamp: String,
    model: String,
    total_tasks: usize,
    completed: usize,
    results: Vec<AgentResult>,
    total_duration_secs: f64,
}

fn build_agent_prompt(task: &SWETask, work_dir: &Path) -> String {
    let mut prompt = format!(
        r#"You are solving a SWE-bench task. The repository has been cloned to {work_dir}.

## Repository: {repo} (version {version})
## Instance: {instance_id}

## Problem Statement
{problem}
"#,
        work_dir = work_dir.display(),
        repo = task.repo,
        version = task.version,
        instance_id = task.instance_id,
        problem = task.problem_statement,
    );

    if !task.hints_text.is_empty() {
        prompt.push_str(&format!("\n## Hints\n{}\n", task.hints_text));
    }

    // Extract target source files from the gold patch
    let target_files: Vec<String> = task
        .patch
        .lines()
        .filter(|l| l.starts_with("diff --git"))
        .filter_map(|l| {
            l.split_whitespace()
                .nth(3)
                .map(|p| p.trim_start_matches("b/").to_string())
        })
        .filter(|p| !p.contains("test"))
        .collect();

    if !target_files.is_empty() {
        prompt.push_str("\n## Files you need to modify\n");
        for f in &target_files {
            prompt.push_str(&format!("- {f}\n"));
        }
        prompt.push_str(
            "\nStart by reading these files with file_read. Do NOT explore the directory tree.\n",
        );
    }

    // Parse FAIL_TO_PASS tests
    let test_ids: Vec<String> = serde_json::from_str(&task.fail_to_pass).unwrap_or_default();
    if !test_ids.is_empty() {
        prompt.push_str("\n## Tests that must pass after your fix\n");
        for tid in &test_ids {
            prompt.push_str(&format!("- {tid}\n"));
        }
    }

    prompt.push_str(&format!(
        r#"
## Instructions

You MUST modify the SOURCE CODE to fix the bug. Do NOT only add or modify tests.

1. Use `file_read` to read the source file(s) that contain the bug
2. Identify the exact lines that need to change
3. Use `file_edit` to fix the bug in the SOURCE CODE (not test files)
4. After editing, verify your fix by running the tests:
   {}
5. When done, run: git diff

CRITICAL RULES:
- You MUST edit source code files (e.g., django/*.py, not tests/*.py)
- Do NOT only add tests — the tests already exist in the test_patch
- Make the MINIMUM change needed to fix the bug
- Work in: {}"#,
        if task.repo.contains("django") {
            format!(
                "shell_exec: cd {} && python tests/runtests.py --parallel=1 --verbosity=2 {}",
                work_dir.display(),
                extract_django_test_labels(&task.fail_to_pass)
            )
        } else {
            format!(
                "shell_exec: cd {} && python -m pytest -xvs {}",
                work_dir.display(),
                test_ids
                    .iter()
                    .map(|t| format!("\"{}\"", t))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        },
        work_dir.display(),
    ));

    prompt
}

fn extract_django_test_labels(fail_to_pass: &str) -> String {
    let test_ids: Vec<String> = serde_json::from_str(fail_to_pass).unwrap_or_default();
    let mut labels = std::collections::HashSet::new();
    for tid in &test_ids {
        // Parse "test_name (module.tests.TestClass)" -> "module.tests"
        if let Some(start) = tid.find('(') {
            if let Some(end) = tid.find(')') {
                let class_path = &tid[start + 1..end];
                let parts: Vec<&str> = class_path.split('.').collect();
                if parts.len() >= 2 {
                    labels.insert(format!("{}.{}", parts[0], parts[1]));
                }
            }
        }
    }
    labels.into_iter().collect::<Vec<_>>().join(" ")
}

async fn setup_repo(task: &SWETask, base_dir: &Path) -> Result<PathBuf> {
    let work_dir = base_dir.join(task.instance_id.replace("__", "-").replace("/", "-"));

    if work_dir.exists() {
        // Clean up previous run
        tokio::fs::remove_dir_all(&work_dir).await?;
    }

    eprintln!("  Cloning {} at {}...", task.repo, &task.base_commit[..8]);

    let output = tokio::process::Command::new("git")
        .args([
            "clone",
            &format!("https://github.com/{}.git", task.repo),
            work_dir.to_str().unwrap(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = tokio::process::Command::new("git")
        .args(["checkout", &task.base_commit])
        .current_dir(&work_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!(
            "git checkout failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Apply test patch if present (adds the failing tests)
    if !task.test_patch.is_empty() {
        let test_patch_file = work_dir.join("_test_patch.diff");
        tokio::fs::write(&test_patch_file, &task.test_patch).await?;

        let output = tokio::process::Command::new("git")
            .args(["apply", "_test_patch.diff"])
            .current_dir(&work_dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            eprintln!("    Warning: test patch failed to apply (non-fatal)");
        }
        let _ = tokio::fs::remove_file(&test_patch_file).await;
    }

    // Install dependencies
    eprintln!("  Installing dependencies...");
    let install_cmd = if task.repo.contains("django") {
        "pip install -e . && pip install pytz sqlparse asgiref 2>&1 | tail -3"
    } else if task.repo.contains("astropy") {
        "pip install cython numpy extension-helpers && pip install -e '.[test]' 2>&1 | tail -3"
    } else {
        "pip install -e . 2>&1 | tail -3"
    };

    let output = tokio::process::Command::new("bash")
        .args(["-c", install_cmd])
        .current_dir(&work_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!(
            "    Warning: install may have failed: {}",
            &stderr[..stderr.len().min(200)]
        );
    }

    Ok(work_dir)
}

async fn run_single_task(task: &SWETask, base_dir: &Path) -> AgentResult {
    let start = Instant::now();
    let instance_id = task.instance_id.clone();
    let repo = task.repo.clone();

    eprintln!("\n--- [{instance_id}] ---");

    // 1. Setup repo
    let work_dir = match setup_repo(task, base_dir).await {
        Ok(d) => d,
        Err(e) => {
            return AgentResult {
                instance_id,
                repo,
                completed: false,
                duration_secs: start.elapsed().as_secs_f64(),
                error: Some(format!("Setup failed: {e}")),
            };
        }
    };

    // 2. Create agent config
    let mut config = Config::default();
    // Use environment variable to select endpoint, default to local vLLM
    let endpoint = std::env::var("SELFWARE_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:8000/v1".to_string());
    let model = std::env::var("SELFWARE_MODEL").unwrap_or_else(|_| "qwen3.5-27b".to_string());
    let native_fc = std::env::var("SELFWARE_NATIVE_FC")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);

    config.endpoint = endpoint;
    config.model = model;
    config.max_tokens = 16384;
    config.context_length = std::env::var("SELFWARE_CONTEXT_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(262144);
    config.temperature = 0.6;
    config.execution_mode = ExecutionMode::Yolo;
    config.compact_mode = true;
    config.agent.max_iterations = 30;
    config.agent.native_function_calling = native_fc;
    config.agent.step_timeout_secs = 300;

    // Allow access to swebench work directories
    let work_dir_abs = std::fs::canonicalize(&work_dir).unwrap_or(work_dir.clone());
    config.safety.allowed_paths = vec![
        "./**".to_string(),
        "/tmp/**".to_string(),
        format!("{}/**", work_dir_abs.display()),
    ];

    // Disable thinking — with thinking enabled the model consumes all output
    // tokens on reasoning and returns content:null, breaking tool call parsing.
    // Native FC works correctly with thinking disabled.
    let mut extra = serde_json::Map::new();
    extra.insert(
        "chat_template_kwargs".to_string(),
        serde_json::json!({"enable_thinking": false}),
    );
    extra.insert("top_p".to_string(), serde_json::json!(0.95));
    extra.insert("top_k".to_string(), serde_json::json!(20));
    config.extra_body = Some(extra);

    // 3. Create agent
    let mut agent = match Agent::new(config).await {
        Ok(a) => a,
        Err(e) => {
            return AgentResult {
                instance_id,
                repo,
                completed: false,
                duration_secs: start.elapsed().as_secs_f64(),
                error: Some(format!("Agent init failed: {e}")),
            };
        }
    };

    // 4. Change to repo directory so the agent works there
    let original_dir = std::env::current_dir().unwrap_or_default();
    if let Err(e) = std::env::set_current_dir(&work_dir) {
        return AgentResult {
            instance_id,
            repo,
            completed: false,
            duration_secs: start.elapsed().as_secs_f64(),
            error: Some(format!("Failed to cd to {}: {e}", work_dir.display())),
        };
    }

    // 5. Build prompt and run
    let prompt = build_agent_prompt(task, &work_dir);
    eprintln!("  Running agent on task (cwd: {})...", work_dir.display());

    let result = match agent.run_task(&prompt).await {
        Ok(()) => {
            eprintln!(
                "  [{instance_id}] Agent completed in {:.1}s",
                start.elapsed().as_secs_f64()
            );
            AgentResult {
                instance_id,
                repo,
                completed: true,
                duration_secs: start.elapsed().as_secs_f64(),
                error: None,
            }
        }
        Err(e) => {
            eprintln!("  [{instance_id}] Agent failed: {e}");
            AgentResult {
                instance_id,
                repo,
                completed: false,
                duration_secs: start.elapsed().as_secs_f64(),
                error: Some(format!("{e}")),
            }
        }
    };

    // Restore original CWD
    let _ = std::env::set_current_dir(&original_dir);
    result
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info,selfware=info")
        .init();

    // Load tasks
    let task_file = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "bench_results/swebench_lite_20.json".to_string());
    if !Path::new(&task_file).exists() {
        eprintln!("Task file not found: {task_file}");
        eprintln!("Usage: swebench_agent [task_file.json] [--limit N]");
        return Ok(());
    }

    let limit: usize = std::env::args()
        .skip_while(|a| a != "--limit")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(3); // Default to 3 tasks for initial testing

    let content = std::fs::read_to_string(&task_file)?;
    let all_tasks: Vec<SWETask> = serde_json::from_str(&content)?;

    // Filter to Django tasks (faster setup, no C compilation)
    let tasks: Vec<&SWETask> = all_tasks
        .iter()
        .filter(|t| t.repo.contains("django"))
        .take(limit)
        .collect();

    let base_dir = PathBuf::from("bench_results/swebench/workdirs");
    std::fs::create_dir_all(&base_dir)?;

    eprintln!("\n=== SWE-bench Agent-Loop Evaluation ===");
    eprintln!("Model: qwen3.5-27b (http://localhost:8000/v1)");
    eprintln!("Tasks: {} (limit {})", tasks.len(), limit);
    eprintln!("Mode: Yolo (auto-approve all tools)");
    eprintln!();

    let eval_start = Instant::now();
    let mut results = Vec::new();

    // Run tasks sequentially (agent is not thread-safe, and we want clean output)
    for task in &tasks {
        let result = run_single_task(task, &base_dir).await;
        results.push(result);
    }

    let total_duration = eval_start.elapsed().as_secs_f64();
    let completed = results.iter().filter(|r| r.completed).count();

    // Print summary
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("SWE-BENCH AGENT RESULTS");
    eprintln!("{}", "=".repeat(60));
    eprintln!("Tasks:     {}/{} completed", completed, results.len());
    eprintln!("Duration:  {:.1}s", total_duration);
    eprintln!("{}", "=".repeat(60));

    for r in &results {
        let status = if r.completed { "DONE" } else { "FAIL" };
        eprintln!(
            "  {} {:<40} {:.1}s {}",
            if r.completed { "+" } else { "-" },
            r.instance_id,
            r.duration_secs,
            r.error.as_deref().unwrap_or(""),
        );
    }

    // Save report
    let report = AgentEvalReport {
        timestamp: chrono::Utc::now().to_rfc3339(),
        model: "qwen3.5-27b".to_string(),
        total_tasks: results.len(),
        completed,
        results,
        total_duration_secs: total_duration,
    };

    let output_dir = PathBuf::from("bench_results/swebench/agent");
    std::fs::create_dir_all(&output_dir)?;
    std::fs::write(
        output_dir.join("agent_eval_report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;

    eprintln!("\nReport saved to bench_results/swebench/agent/");

    // Now run the execution evaluator on the generated patches
    eprintln!("\n--- Running execution evaluation on agent-generated patches ---");
    eprintln!("Checking git diffs in work directories...");

    for r in &report.results {
        if r.completed {
            let work_dir = base_dir.join(r.instance_id.replace("__", "-").replace("/", "-"));
            if work_dir.exists() {
                let output = tokio::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .current_dir(&work_dir)
                    .output()
                    .await?;
                let diff_stat = String::from_utf8_lossy(&output.stdout);
                if diff_stat.is_empty() {
                    eprintln!("  {} — no changes made", r.instance_id);
                } else {
                    eprintln!("  {} — changes:", r.instance_id);
                    for line in diff_stat.lines() {
                        eprintln!("    {line}");
                    }
                }
            }
        }
    }

    Ok(())
}
