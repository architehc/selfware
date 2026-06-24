//! Deprecated legacy SWE-bench demo API.
//!
//! **Do not use for official scoring.** This module is kept only for source
//! compatibility with older experiments and is gated behind the
//! `legacy-swebench` feature. It no longer executes tasks or produces
//! resolution results.
//!
//! Real benchmark execution and official Docker scoring live in
//! `system_tests/swe_bench_pro/` and behind the `bench-harness` feature in
//! `src/bench_harness/swebench_pro/`. Use the CLI command
//! `selfware bench swebench-pro` or the Python harness for production runs.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// SWE-bench Pro task
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SWEBenchTask {
    /// Repository name (e.g., "django/django")
    pub repo: String,
    /// Issue or PR number
    pub instance_id: String,
    /// Problem statement / issue description
    pub problem_statement: String,
    /// Base commit to work from
    pub base_commit: String,
    /// Expected solution commit
    pub solution_commit: Option<String>,
    /// Test files to run for validation
    pub test_files: Vec<String>,
    /// File paths that should be modified
    pub target_files: Vec<String>,
    /// Task difficulty/complexity
    pub difficulty: TaskDifficulty,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum TaskDifficulty {
    Easy,
    Medium,
    Hard,
    Expert,
}

/// Test result for a SWE-bench task
#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub instance_id: String,
    pub success: bool,
    pub resolved: bool,
    pub duration_secs: f64,
    pub iterations: usize,
    pub tokens_used: usize,
    pub patch_applied: bool,
    pub tests_passed: bool,
    pub error: Option<String>,
    pub trajectory: Vec<TrajectoryStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrajectoryStep {
    pub step: usize,
    pub action: String,
    pub observation: String,
    pub timestamp: String,
}

/// SWE-bench Pro evaluator
pub struct SWEBenchEvaluator {
    /// Path to SWE-bench repository
    pub swebench_path: PathBuf,
    /// Working directory for tests
    pub work_dir: PathBuf,
    /// Maximum iterations per task
    pub max_iterations: usize,
    /// Token budget per task
    pub token_budget: usize,
}

fn string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn string_list_field(value: &serde_json::Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
            .collect(),
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str::<Vec<String>>(s).unwrap_or_else(|_| vec![s.clone()])
        }
        _ => Vec::new(),
    }
}

fn target_files_from_patch(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        let path = line
            .strip_prefix("diff --git ")
            .and_then(|line| line.find(" b/").map(|idx| &line[idx + 3..]))
            .or_else(|| line.strip_prefix("+++ b/"));
        if let Some(path) = path {
            let path = path.trim().to_string();
            if !path.is_empty() && path != "/dev/null" && !files.contains(&path) {
                files.push(path);
            }
        }
    }
    files
}

fn parse_task_value(value: &serde_json::Value) -> Result<SWEBenchTask> {
    let instance_id = string_field(value, "instance_id");
    let repo = string_field(value, "repo");
    let problem_statement = string_field(value, "problem_statement");
    let base_commit = string_field(value, "base_commit");
    if instance_id.is_empty()
        || repo.is_empty()
        || problem_statement.is_empty()
        || base_commit.is_empty()
    {
        bail!(
            "dataset row is missing one of required fields: instance_id, repo, problem_statement, base_commit"
        );
    }

    let mut target_files = string_list_field(value, "target_files");
    if target_files.is_empty() {
        target_files = value
            .get("patch")
            .and_then(|v| v.as_str())
            .map(target_files_from_patch)
            .unwrap_or_default();
    }

    Ok(SWEBenchTask {
        repo,
        instance_id,
        problem_statement,
        base_commit,
        solution_commit: value
            .get("solution_commit")
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
        test_files: string_list_field(value, "selected_test_files_to_run"),
        target_files,
        difficulty: TaskDifficulty::Medium,
    })
}

impl SWEBenchEvaluator {
    /// Create new evaluator
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            swebench_path: PathBuf::from("./swebench"),
            work_dir,
            max_iterations: 50,
            token_budget: 500000,
        }
    }

    /// Load tasks from a local JSON/JSONL dataset file.
    ///
    /// Named datasets like `"public"` used to return a hardcoded fake task.
    /// That behavior is intentionally disabled so no caller can accidentally
    /// report fabricated SWE-bench success.
    pub fn load_tasks(&self, dataset: &str) -> Result<Vec<SWEBenchTask>> {
        let path = Path::new(dataset);
        if !path.exists() {
            bail!(
                "legacy swebench::load_tasks no longer provides mock datasets. \
                 Pass a JSON/JSONL dataset path, or use `selfware bench swebench-pro`."
            );
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading SWE-bench dataset {}", path.display()))?;
        if dataset.ends_with(".jsonl") {
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line)
                        .with_context(|| "parsing SWE-bench JSONL row")?;
                    parse_task_value(&value)
                })
                .collect()
        } else {
            let value: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| format!("parsing SWE-bench dataset {}", path.display()))?;
            match value {
                serde_json::Value::Array(items) => items.iter().map(parse_task_value).collect(),
                other => parse_task_value(&other).map(|task| vec![task]),
            }
        }
    }

    /// Disabled legacy evaluation entrypoint.
    pub async fn evaluate_task(
        &self,
        task: &SWEBenchTask,
        _agent: &crate::agent::Agent,
    ) -> Result<TestResult> {
        bail!(
            "legacy swebench::evaluate_task is disabled for `{}` because it previously returned fabricated success. \
             Use `selfware bench swebench-pro --official-eval` for real evaluation.",
            task.instance_id
        )
    }

    /// Run full evaluation on task set
    pub async fn evaluate_all(
        &self,
        tasks: &[SWEBenchTask],
        agent: &crate::agent::Agent,
    ) -> Result<EvaluationReport> {
        let _ = (tasks, agent);
        bail!(
            "legacy swebench::evaluate_all is disabled. Use `selfware bench swebench-pro --official-eval`."
        )
    }

    /// Generate evaluation report
    pub fn generate_report(&self, report: &EvaluationReport) -> String {
        let mut output = String::new();

        output.push_str("# SWE-bench Pro Evaluation Report\n\n");
        output.push_str(&format!("**Timestamp:** {}\n\n", report.timestamp));
        output.push_str(&format!("**Total Tasks:** {}\n", report.total_tasks));
        output.push_str(&format!("**Resolved:** {}\n", report.resolved));
        output.push_str(&format!(
            "**Resolution Rate:** {:.2}%\n\n",
            report.resolution_rate * 100.0
        ));

        output.push_str("## Results by Task\n\n");
        for result in &report.results {
            let status = if result.resolved {
                "✅ RESOLVED"
            } else {
                "❌ FAILED"
            };
            output.push_str(&format!("### {} - {}\n", result.instance_id, status));
            output.push_str(&format!("- Duration: {:.2}s\n", result.duration_secs));
            output.push_str(&format!("- Tokens: {}\n", result.tokens_used));
            output.push_str(&format!("- Iterations: {}\n\n", result.iterations));
        }

        output
    }
}

/// Full evaluation report
#[derive(Debug, Clone, Serialize)]
pub struct EvaluationReport {
    pub total_tasks: usize,
    pub resolved: usize,
    pub resolution_rate: f64,
    pub results: Vec<TestResult>,
    pub timestamp: String,
}

/// Local development workflow tester
pub struct LocalDevWorkflow {
    pub test_patterns: Vec<String>,
    pub endpoints: Vec<String>,
}

impl LocalDevWorkflow {
    /// Test selfware local development workflow
    pub async fn test_workflow(&self) -> Result<WorkflowReport> {
        info!("Testing local development workflow");

        let tests = vec![
            ("Code generation", test_code_generation()),
            ("File editing", test_file_editing()),
            ("Git operations", test_git_operations()),
            ("Multi-agent swarm", test_multi_agent()),
            ("Visual validation", test_visual_validation()),
            ("Browser automation", test_browser_automation()),
        ];

        let mut results = HashMap::new();
        let mut all_passed = true;

        for (name, result) in tests {
            match result {
                Ok(_) => {
                    info!("✓ {} passed", name);
                    results.insert(name.to_string(), true);
                }
                Err(e) => {
                    warn!("✗ {} failed: {}", name, e);
                    results.insert(name.to_string(), false);
                    all_passed = false;
                }
            }
        }

        Ok(WorkflowReport {
            all_passed,
            results,
            timestamp: chrono::Local::now().to_rfc3339(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowReport {
    pub all_passed: bool,
    pub results: HashMap<String, bool>,
    pub timestamp: String,
}

// STUB Test functions - all no-ops
fn test_code_generation() -> Result<()> {
    warn!("STUB: test_code_generation - no actual test performed");
    Ok(())
}

fn test_file_editing() -> Result<()> {
    warn!("STUB: test_file_editing - no actual test performed");
    Ok(())
}

fn test_git_operations() -> Result<()> {
    warn!("STUB: test_git_operations - no actual test performed");
    Ok(())
}

fn test_multi_agent() -> Result<()> {
    warn!("STUB: test_multi_agent - no actual test performed");
    Ok(())
}

fn test_visual_validation() -> Result<()> {
    warn!("STUB: test_visual_validation - no actual test performed");
    Ok(())
}

fn test_browser_automation() -> Result<()> {
    warn!("STUB: test_browser_automation - no actual test performed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SWEBenchTask ─────────────────────────────────────────────────

    #[test]
    fn test_swebench_task_serde_roundtrip() {
        let task = SWEBenchTask {
            repo: "django/django".to_string(),
            instance_id: "django__django-12345".to_string(),
            problem_statement: "Fix authentication bug".to_string(),
            base_commit: "abc123def".to_string(),
            solution_commit: Some("fed321cba".to_string()),
            test_files: vec!["tests/test_auth.py".to_string()],
            target_files: vec!["django/auth/models.py".to_string()],
            difficulty: TaskDifficulty::Hard,
        };
        let json = serde_json::to_string(&task).unwrap();
        let parsed: SWEBenchTask = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.repo, "django/django");
        assert_eq!(parsed.instance_id, "django__django-12345");
        assert_eq!(parsed.difficulty, TaskDifficulty::Hard);
        assert_eq!(parsed.test_files.len(), 1);
        assert_eq!(parsed.target_files.len(), 1);
        assert!(parsed.solution_commit.is_some());
    }

    #[test]
    fn test_swebench_task_no_solution_commit() {
        let task = SWEBenchTask {
            repo: "repo/name".to_string(),
            instance_id: "id-001".to_string(),
            problem_statement: "A bug".to_string(),
            base_commit: "abc".to_string(),
            solution_commit: None,
            test_files: vec![],
            target_files: vec![],
            difficulty: TaskDifficulty::Easy,
        };
        let json = serde_json::to_string(&task).unwrap();
        let parsed: SWEBenchTask = serde_json::from_str(&json).unwrap();
        assert!(parsed.solution_commit.is_none());
        assert!(parsed.test_files.is_empty());
        assert!(parsed.target_files.is_empty());
    }

    // ── TaskDifficulty ───────────────────────────────────────────────

    #[test]
    fn test_task_difficulty_serde_all_variants() {
        let difficulties = vec![
            TaskDifficulty::Easy,
            TaskDifficulty::Medium,
            TaskDifficulty::Hard,
            TaskDifficulty::Expert,
        ];
        for d in difficulties {
            let json = serde_json::to_string(&d).unwrap();
            let parsed: TaskDifficulty = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, d);
        }
    }

    #[test]
    fn test_task_difficulty_equality() {
        assert_eq!(TaskDifficulty::Easy, TaskDifficulty::Easy);
        assert_ne!(TaskDifficulty::Easy, TaskDifficulty::Hard);
    }

    #[test]
    fn test_task_difficulty_debug() {
        assert!(format!("{:?}", TaskDifficulty::Expert).contains("Expert"));
    }

    // ── TestResult ───────────────────────────────────────────────────

    #[test]
    fn test_test_result_serialize() {
        let result = TestResult {
            instance_id: "test-001".to_string(),
            success: true,
            resolved: true,
            duration_secs: 12.5,
            iterations: 5,
            tokens_used: 15000,
            patch_applied: true,
            tests_passed: true,
            error: None,
            trajectory: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"success\":true"));
        assert!(json.contains("\"resolved\":true"));
        assert!(json.contains("\"duration_secs\":12.5"));
    }

    #[test]
    fn test_test_result_with_error() {
        let result = TestResult {
            instance_id: "test-002".to_string(),
            success: false,
            resolved: false,
            duration_secs: 5.0,
            iterations: 2,
            tokens_used: 3000,
            patch_applied: false,
            tests_passed: false,
            error: Some("Compilation error in auth.py".to_string()),
            trajectory: vec![],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("Compilation error"));
    }

    // ── TrajectoryStep ───────────────────────────────────────────────

    #[test]
    fn test_trajectory_step_serialize() {
        let step = TrajectoryStep {
            step: 1,
            action: "setup_environment".to_string(),
            observation: "Cloning repo".to_string(),
            timestamp: "2026-03-24T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        assert!(json.contains("setup_environment"));
        assert!(json.contains("Cloning repo"));
    }

    #[test]
    fn test_trajectory_step_clone() {
        let step = TrajectoryStep {
            step: 1,
            action: "analyze".to_string(),
            observation: "Found bug".to_string(),
            timestamp: "now".to_string(),
        };
        let cloned = step.clone();
        assert_eq!(cloned.step, 1);
        assert_eq!(cloned.action, "analyze");
    }

    // ── SWEBenchEvaluator ────────────────────────────────────────────

    #[test]
    fn test_swebench_evaluator_creation() {
        let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/test"));
        assert_eq!(evaluator.max_iterations, 50);
        assert_eq!(evaluator.token_budget, 500000);
        assert_eq!(evaluator.work_dir, PathBuf::from("/tmp/test"));
        assert_eq!(evaluator.swebench_path, PathBuf::from("./swebench"));
    }

    #[test]
    fn test_load_tasks() {
        let dir = tempfile::tempdir().unwrap();
        let dataset = dir.path().join("tasks.jsonl");
        std::fs::write(
            &dataset,
            r#"{"repo":"example/repo","instance_id":"test-001","problem_statement":"Fix bug","base_commit":"abc123","selected_test_files_to_run":["tests/test_auth.py"],"patch":"diff --git a/auth/login.py b/auth/login.py\n--- a/auth/login.py\n+++ b/auth/login.py\n"}"#,
        )
        .unwrap();
        let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/test"));
        let tasks = evaluator.load_tasks(dataset.to_str().unwrap()).unwrap();
        assert!(!tasks.is_empty());
        assert_eq!(tasks[0].repo, "example/repo");
        assert_eq!(tasks[0].instance_id, "test-001");
        assert_eq!(tasks[0].target_files, vec!["auth/login.py"]);
        assert_eq!(tasks[0].difficulty, TaskDifficulty::Medium);
    }

    #[test]
    fn test_named_mock_dataset_is_disabled() {
        let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/test"));
        let err = evaluator.load_tasks("public").unwrap_err().to_string();
        assert!(err.contains("no longer provides mock datasets"));
    }

    // ── EvaluationReport ─────────────────────────────────────────────

    #[test]
    fn test_evaluation_report_serialize() {
        let report = EvaluationReport {
            total_tasks: 10,
            resolved: 7,
            resolution_rate: 0.7,
            results: vec![],
            timestamp: "2026-03-24T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"total_tasks\":10"));
        assert!(json.contains("\"resolved\":7"));
        assert!(json.contains("0.7"));
    }

    #[test]
    fn test_generate_report() {
        let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/test"));
        let report = EvaluationReport {
            total_tasks: 2,
            resolved: 1,
            resolution_rate: 0.5,
            results: vec![
                TestResult {
                    instance_id: "task-001".to_string(),
                    success: true,
                    resolved: true,
                    duration_secs: 10.0,
                    iterations: 3,
                    tokens_used: 5000,
                    patch_applied: true,
                    tests_passed: true,
                    error: None,
                    trajectory: vec![],
                },
                TestResult {
                    instance_id: "task-002".to_string(),
                    success: false,
                    resolved: false,
                    duration_secs: 25.0,
                    iterations: 10,
                    tokens_used: 20000,
                    patch_applied: false,
                    tests_passed: false,
                    error: Some("Timeout".to_string()),
                    trajectory: vec![],
                },
            ],
            timestamp: "2026-03-24T12:00:00Z".to_string(),
        };
        let output = evaluator.generate_report(&report);
        assert!(output.contains("SWE-bench Pro Evaluation Report"));
        assert!(output.contains("Total Tasks:** 2"));
        assert!(output.contains("Resolved:** 1"));
        assert!(output.contains("50.00%"));
        assert!(output.contains("task-001"));
        assert!(output.contains("RESOLVED"));
        assert!(output.contains("task-002"));
        assert!(output.contains("FAILED"));
    }

    #[test]
    fn test_generate_report_empty() {
        let evaluator = SWEBenchEvaluator::new(PathBuf::from("/tmp/test"));
        let report = EvaluationReport {
            total_tasks: 0,
            resolved: 0,
            resolution_rate: 0.0,
            results: vec![],
            timestamp: "now".to_string(),
        };
        let output = evaluator.generate_report(&report);
        assert!(output.contains("Total Tasks:** 0"));
    }

    // ── WorkflowReport ───────────────────────────────────────────────

    #[test]
    fn test_workflow_report_serialize() {
        let mut results = HashMap::new();
        results.insert("Code generation".to_string(), true);
        results.insert("Browser automation".to_string(), false);
        let report = WorkflowReport {
            all_passed: false,
            results,
            timestamp: "2026-03-24T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("all_passed"));
        assert!(json.contains("Code generation"));
    }

    // ── LocalDevWorkflow ─────────────────────────────────────────────

    #[test]
    fn test_local_dev_workflow_creation() {
        let workflow = LocalDevWorkflow {
            test_patterns: vec!["*.rs".to_string()],
            endpoints: vec!["http://localhost:8000/v1".to_string()],
        };
        assert_eq!(workflow.test_patterns.len(), 1);
        assert_eq!(workflow.endpoints.len(), 1);
    }

    #[tokio::test]
    async fn test_local_dev_workflow_all_pass() {
        let workflow = LocalDevWorkflow {
            test_patterns: vec![],
            endpoints: vec![],
        };
        let report = workflow.test_workflow().await.unwrap();
        // All stub test functions return Ok(())
        assert!(report.all_passed);
        assert_eq!(report.results.len(), 6);
        for passed in report.results.values() {
            assert!(passed);
        }
    }

    // ── Test helper functions ────────────────────────────────────────

    #[test]
    fn test_stub_functions_all_pass() {
        assert!(test_code_generation().is_ok());
        assert!(test_file_editing().is_ok());
        assert!(test_git_operations().is_ok());
        assert!(test_multi_agent().is_ok());
        assert!(test_visual_validation().is_ok());
        assert!(test_browser_automation().is_ok());
    }
}
