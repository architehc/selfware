//! Long-running test runner.

use super::config::LongRunningConfig;
use super::project::{ProjectResult, ProjectStatus, ProjectType};
use crate::bench_harness::subprocess::{apply_env_allowlist, run_with_group_timeout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A test task definition.
#[derive(Debug, Clone)]
pub struct TestTask {
    pub name: String,
    pub project_type: ProjectType,
    pub setup: TaskSetup,
    pub prompt: String,
}

/// Setup for a test task.
#[derive(Debug, Clone)]
pub enum TaskSetup {
    /// Greenfield Rust project.
    RustGreenfield { name: String },
    /// Python project.
    Python,
    /// Go project.
    Go { module: String },
    /// Template-based project.
    Template { name: String },
}

/// Runner for long-running system tests.
pub struct LongRunningRunner {
    config: LongRunningConfig,
    selfware_path: PathBuf,
    start_time: Instant,
}

impl LongRunningRunner {
    /// Create a new runner with the given configuration.
    pub fn new(config: LongRunningConfig) -> Result<Self, String> {
        config.validate()?;

        // Find selfware binary
        let selfware_path = find_selfware_binary()?;

        Ok(Self {
            config,
            selfware_path,
            start_time: Instant::now(),
        })
    }

    /// Check if the test should continue running.
    pub fn should_continue(&self) -> bool {
        self.start_time.elapsed() < self.config.max_duration
    }

    /// Get remaining time.
    pub fn time_remaining(&self) -> Duration {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.config.max_duration {
            Duration::ZERO
        } else {
            self.config.max_duration - elapsed
        }
    }

    /// Run a single test task.
    pub async fn run_task(&self, task: &TestTask, work_dir: &Path) -> ProjectResult {
        let task_start = Instant::now();

        // Setup project
        if let Err(e) = self.setup_project(&task.setup, work_dir).await {
            return ProjectResult {
                name: task.name.clone(),
                status: ProjectStatus::Fail,
                duration_secs: 0,
                steps: 0,
                src_lines: 0,
                compiles: false,
                tests_passed: 0,
                tests_failed: 0,
                outcome_label: format!("setup_failed: {}", e),
            };
        }

        // Write selfware.toml
        let config_toml = self.config.to_selfware_toml();
        let _ = std::fs::write(work_dir.join("selfware.toml"), config_toml);

        // Run selfware
        let log_path = work_dir
            .parent()
            .unwrap()
            .join(format!("{}.log", task.name));
        let mut cmd = Command::new(&self.selfware_path);
        cmd.arg("-c")
            .arg(work_dir.join("selfware.toml"))
            .arg("-C")
            .arg(work_dir)
            .arg("--yolo")
            .arg("--ascii")
            .arg("--no-color")
            .arg("-p")
            .arg(&task.prompt)
            // Discard stderr: only stdout is consumed (count_steps / extract_
            // outcome / the log), and `run_with_group_timeout` drains ONLY
            // stdout — piping stderr into an undrained pipe would let a chatty
            // child fill it and block, then be falsely reported as timed out.
            .stderr(Stdio::null());
        // Scrub the agent's environment to the shared allowlist (drop the
        // operator's unrelated secrets); the model endpoint comes from the
        // written selfware.toml, not the environment.
        apply_env_allowlist(&mut cmd);

        // Run under a wall-clock timeout in its own process group, so a hung
        // agent — and every git/cargo/tool process it spawned — is killed
        // rather than orphaned. The blocking spawn+wait runs on a blocking
        // thread so the async scheduler isn't stalled.
        let timeout_secs = self.config.timeout_per_project_secs;
        let outcome = match tokio::task::spawn_blocking(move || {
            run_with_group_timeout(cmd, Duration::from_secs(timeout_secs))
        })
        .await
        {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => {
                return self.create_error_result(&task.name, &format!("spawn error: {}", e));
            }
            Err(_) => {
                return self.create_error_result(&task.name, "task panicked");
            }
        };
        if outcome.timed_out {
            return self.create_error_result(&task.name, "timeout");
        }

        // Save log
        let _ = std::fs::write(&log_path, outcome.stdout.as_bytes());

        // Evaluate results
        let duration = task_start.elapsed().as_secs();
        let steps = count_steps(outcome.stdout.as_bytes());
        let outcome_label = extract_outcome(outcome.stdout.as_bytes());

        let (compiles, tests_passed, tests_failed, src_lines) =
            self.evaluate_project(&task.project_type, work_dir);

        let status = ProjectStatus::from_results(compiles, tests_passed, tests_failed, src_lines);

        ProjectResult {
            name: task.name.clone(),
            status,
            duration_secs: duration,
            steps,
            src_lines,
            compiles,
            tests_passed,
            tests_failed,
            outcome_label,
        }
    }

    /// Setup a project based on task setup.
    async fn setup_project(&self, setup: &TaskSetup, dir: &Path) -> std::io::Result<()> {
        use super::project::{scaffold_go, scaffold_python, scaffold_rust, scaffold_template};

        match setup {
            TaskSetup::RustGreenfield { name } => scaffold_rust(name, dir),
            TaskSetup::Python => scaffold_python(dir),
            TaskSetup::Go { module } => scaffold_go(module, dir),
            TaskSetup::Template { name } => {
                scaffold_template(name, &self.config.templates_dir, dir)
            }
        }
    }

    /// Evaluate a project after running.
    fn evaluate_project(
        &self,
        project_type: &ProjectType,
        work_dir: &Path,
    ) -> (bool, usize, usize, usize) {
        let compiles = self.check_compiles(project_type, work_dir);
        let src_lines = self.count_source_lines(project_type, work_dir);

        let (passed, failed) = if compiles {
            self.run_tests(project_type, work_dir)
        } else {
            (0, 0)
        };

        (compiles, passed, failed, src_lines)
    }

    /// Check if the project compiles.
    fn check_compiles(&self, project_type: &ProjectType, work_dir: &Path) -> bool {
        let cmd_parts = project_type.check_command();
        if cmd_parts.is_empty() {
            return false;
        }

        let output = Command::new(cmd_parts[0])
            .args(&cmd_parts[1..])
            .current_dir(work_dir)
            .env("XDG_RUNTIME_DIR", "/tmp/xdg-run-selfware")
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                match project_type {
                    ProjectType::Rust => stdout.contains("Finished") || !stderr.contains("error"),
                    ProjectType::Python => output.status.success(),
                    ProjectType::Go => stdout.contains("ok") || !stderr.contains("build failed"),
                    ProjectType::Template => stdout.contains("Finished"),
                }
            }
            Err(_) => false,
        }
    }

    /// Run tests and count results.
    fn run_tests(&self, project_type: &ProjectType, work_dir: &Path) -> (usize, usize) {
        let cmd_parts = project_type.test_command();
        if cmd_parts.is_empty() {
            return (0, 0);
        }

        let output = Command::new(cmd_parts[0])
            .args(&cmd_parts[1..])
            .current_dir(work_dir)
            .env("XDG_RUNTIME_DIR", "/tmp/xdg-run-selfware")
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);

                match project_type {
                    ProjectType::Rust => {
                        let passed: usize = stdout
                            .lines()
                            .filter_map(|l| l.split("passed").next())
                            .filter_map(|l| l.split_whitespace().next_back())
                            .filter_map(|n| n.parse::<usize>().ok())
                            .sum();
                        let failed: usize = stdout
                            .lines()
                            .filter_map(|l| l.split("failed").next())
                            .filter_map(|l| l.split_whitespace().next_back())
                            .filter_map(|n| n.parse::<usize>().ok())
                            .sum();
                        (passed, failed)
                    }
                    ProjectType::Python => {
                        let passed = stdout.matches(" passed").count();
                        let failed = stdout.matches(" failed").count();
                        (passed, failed)
                    }
                    ProjectType::Go => {
                        let passed = stdout.matches("--- PASS").count();
                        let failed = stdout.matches("--- FAIL").count();
                        (passed, failed)
                    }
                    ProjectType::Template => {
                        let passed: usize = stdout
                            .lines()
                            .filter_map(|l| l.split("passed").next())
                            .filter_map(|l| l.split_whitespace().next_back())
                            .filter_map(|n| n.parse::<usize>().ok())
                            .sum();
                        let failed: usize = stdout
                            .lines()
                            .filter_map(|l| l.split("failed").next())
                            .filter_map(|l| l.split_whitespace().next_back())
                            .filter_map(|n| n.parse::<usize>().ok())
                            .sum();
                        (passed, failed)
                    }
                }
            }
            Err(_) => (0, 0),
        }
    }

    /// Count source lines in the project.
    fn count_source_lines(&self, project_type: &ProjectType, work_dir: &Path) -> usize {
        let ext = project_type.source_extension();
        if ext.is_empty() {
            return 0;
        }

        let mut total = 0;
        if let Ok(entries) = std::fs::read_dir(work_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == ext).unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        total += content.lines().count();
                    }
                }
            }
        }

        // Also check src/ for Rust
        if *project_type == ProjectType::Rust || *project_type == ProjectType::Template {
            if let Ok(entries) = std::fs::read_dir(work_dir.join("src")) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "rs").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            total += content.lines().count();
                        }
                    }
                }
            }
        }

        total
    }

    /// Create an error result.
    fn create_error_result(&self, name: &str, error: &str) -> ProjectResult {
        ProjectResult {
            name: name.to_string(),
            status: ProjectStatus::Fail,
            duration_secs: 0,
            steps: 0,
            src_lines: 0,
            compiles: false,
            tests_passed: 0,
            tests_failed: 0,
            outcome_label: error.to_string(),
        }
    }
}

/// Find the selfware binary.
fn find_selfware_binary() -> Result<PathBuf, String> {
    // Check common locations
    let candidates = [
        "./target/release/selfware",
        "./target/debug/selfware",
        "/usr/local/bin/selfware",
        "/usr/bin/selfware",
    ];

    for path in &candidates {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    // Try which
    if let Ok(output) = Command::new("which").arg("selfware").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err("Could not find selfware binary. Please build with: cargo build --release".into())
}

/// Count steps from log output.
fn count_steps(output: &[u8]) -> usize {
    let text = String::from_utf8_lossy(output);
    text.lines()
        .filter(|l| l.contains("Step") && l.contains("Executing"))
        .count()
}

/// Extract outcome label from log output.
fn extract_outcome(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.lines()
        .rfind(|l| l.contains("Outcome:"))
        .map(|l| {
            l.split("Outcome:")
                .nth(1)
                .unwrap_or("unknown")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| "none".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_steps() {
        let output = b"Step 1 Executing...\nStep 2 Executing...\n";
        assert_eq!(count_steps(output), 2);
    }

    #[test]
    fn test_extract_outcome() {
        let output = b"Some log\nOutcome: task_completed\nMore log\n";
        assert_eq!(extract_outcome(output), "task_completed");
    }
}
