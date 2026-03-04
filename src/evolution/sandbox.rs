//! Sandbox — Isolated Docker-based Evaluation Environments
//!
//! Each hypothesis gets its own container with resource limits.
//! Containers are ephemeral — spun up, evaluated, and destroyed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Docker image to use (should be pre-built from selfware's Dockerfile)
    pub image: String,
    /// CPU limit per container (e.g., "2" for 2 cores)
    pub cpus: String,
    /// Memory limit per container (e.g., "4g")
    pub memory: String,
    /// Maximum wall-clock time per evaluation
    pub timeout: Duration,
    /// Network access (disable for safety)
    pub network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            image: "selfware:latest".to_string(),
            cpus: "2".to_string(),
            memory: "4g".to_string(),
            timeout: Duration::from_secs(3600),
            network: false,
        }
    }
}

#[derive(Debug)]
pub struct Sandbox {
    pub container_id: String,
    pub container_name: String,
    pub config: SandboxConfig,
    pub created_at: Instant,
}

#[derive(Debug)]
pub struct SandboxResult {
    pub compiled: bool,
    pub compile_duration: Duration,
    pub tests_passed: usize,
    pub tests_total: usize,
    pub test_duration: Duration,
    pub peak_memory_bytes: u64,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl Sandbox {
    /// Create and start a new sandbox container
    pub fn create(
        name: &str,
        repo_root: &Path,
        config: SandboxConfig,
    ) -> Result<Self, SandboxError> {
        let container_name = format!("selfware-arena-{}", name);

        let mut args = vec![
            "run".to_string(),
            "-d".to_string(),
            "--name".to_string(),
            container_name.clone(),
            format!("--cpus={}", config.cpus),
            format!("--memory={}", config.memory),
            "-v".to_string(),
            format!("{}:/workspace:ro", repo_root.display()),
            "-v".to_string(),
            format!("{}/target:/workspace/target", repo_root.display()), // Share target cache
        ];

        if !config.network {
            args.push("--network=none".to_string());
        }

        args.push(config.image.clone());
        args.push("sleep".to_string());
        args.push("infinity".to_string());

        let output = Command::new("docker")
            .args(&args)
            .output()
            .map_err(|e| SandboxError::DockerFailed(e.to_string()))?;

        if !output.status.success() {
            return Err(SandboxError::DockerFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let container_id = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(Sandbox {
            container_id,
            container_name,
            config,
            created_at: Instant::now(),
        })
    }

    /// Execute a command inside the sandbox
    pub fn exec(&self, cmd: &str) -> Result<ExecResult, SandboxError> {
        let start = Instant::now();

        let output = Command::new("docker")
            .args(["exec", &self.container_name, "bash", "-c", cmd])
            .output()
            .map_err(|e| SandboxError::ExecFailed(e.to_string()))?;

        Ok(ExecResult {
            success: output.status.success(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration: start.elapsed(),
        })
    }

    /// Apply a patch to the workspace inside the container
    pub fn apply_patch(&self, patch: &str) -> Result<bool, SandboxError> {
        // Write patch to a temp file, copy into container, apply
        let patch_file = format!("/tmp/mutation-{}.patch", self.container_name);
        std::fs::write(&patch_file, patch).map_err(|e| SandboxError::IoError(e.to_string()))?;

        let _ = Command::new("docker")
            .args([
                "cp",
                &patch_file,
                &format!("{}:/tmp/mutation.patch", self.container_name),
            ])
            .output();

        let result = self.exec("cd /workspace && git apply /tmp/mutation.patch")?;
        let _ = std::fs::remove_file(&patch_file);

        Ok(result.success)
    }

    /// Run the full evaluation pipeline: compile → test → bench
    pub fn evaluate(&self) -> Result<SandboxResult, SandboxError> {
        // Step 1: Compile
        let compile = self.exec("cd /workspace && cargo build --release 2>&1")?;
        if !compile.success {
            return Ok(SandboxResult {
                compiled: false,
                compile_duration: compile.duration,
                tests_passed: 0,
                tests_total: 0,
                test_duration: Duration::ZERO,
                peak_memory_bytes: 0,
                stdout: compile.stdout,
                stderr: compile.stderr,
                exit_code: compile.exit_code,
            });
        }

        // Step 2: Run tests
        let test = self.exec("cd /workspace && cargo test --all-features 2>&1 | tail -20")?;

        let (passed, total) = parse_test_counts(&test.stdout);

        // Step 3: Get memory stats
        let stats = self.get_stats()?;

        Ok(SandboxResult {
            compiled: true,
            compile_duration: compile.duration,
            tests_passed: passed,
            tests_total: total,
            test_duration: test.duration,
            peak_memory_bytes: stats.peak_memory_bytes,
            stdout: format!("{}\n---\n{}", compile.stdout, test.stdout),
            stderr: format!("{}\n---\n{}", compile.stderr, test.stderr),
            exit_code: test.exit_code,
        })
    }

    /// Get container resource usage stats
    fn get_stats(&self) -> Result<ContainerStats, SandboxError> {
        let output = Command::new("docker")
            .args([
                "stats",
                "--no-stream",
                "--format",
                "{{.MemUsage}}",
                &self.container_name,
            ])
            .output()
            .map_err(|e| SandboxError::DockerFailed(e.to_string()))?;

        let mem_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let peak = parse_memory_string(&mem_str);

        Ok(ContainerStats {
            peak_memory_bytes: peak,
        })
    }

    /// Destroy the sandbox container
    pub fn destroy(self) -> Result<(), SandboxError> {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.container_name])
            .output();
        Ok(())
    }

    /// Check if the sandbox has exceeded its timeout
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.config.timeout
    }
}

#[derive(Debug)]
pub struct ExecResult {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

#[derive(Debug)]
struct ContainerStats {
    peak_memory_bytes: u64,
}

fn parse_test_counts(output: &str) -> (usize, usize) {
    // Parse "test result: ok. X passed; Y failed; Z ignored"
    for line in output.lines().rev() {
        if line.contains("test result:") {
            let mut passed = 0;
            let mut failed = 0;
            let mut ignored = 0;

            for part in line.split(';') {
                let part = part.trim();
                if part.contains("passed") {
                    passed = part
                        .split_whitespace()
                        .filter_map(|w| w.parse().ok())
                        .next()
                        .unwrap_or(0);
                } else if part.contains("failed") {
                    failed = part
                        .split_whitespace()
                        .filter_map(|w| w.parse().ok())
                        .next()
                        .unwrap_or(0);
                } else if part.contains("ignored") {
                    ignored = part
                        .split_whitespace()
                        .filter_map(|w| w.parse().ok())
                        .next()
                        .unwrap_or(0);
                }
            }

            return (passed, passed + failed + ignored);
        }
    }
    (0, 0)
}

fn parse_memory_string(mem: &str) -> u64 {
    // Docker stats format: "123.4MiB / 4GiB"
    let usage = mem.split('/').next().unwrap_or("0").trim();
    if usage.contains("GiB") {
        let n: f64 = usage.replace("GiB", "").trim().parse().unwrap_or(0.0);
        (n * 1024.0 * 1024.0 * 1024.0) as u64
    } else if usage.contains("MiB") {
        let n: f64 = usage.replace("MiB", "").trim().parse().unwrap_or(0.0);
        (n * 1024.0 * 1024.0) as u64
    } else if usage.contains("KiB") {
        let n: f64 = usage.replace("KiB", "").trim().parse().unwrap_or(0.0);
        (n * 1024.0) as u64
    } else {
        0
    }
}

#[derive(Debug)]
pub enum SandboxError {
    DockerFailed(String),
    ExecFailed(String),
    IoError(String),
    Timeout,
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DockerFailed(msg) => write!(f, "Docker operation failed: {}", msg),
            Self::ExecFailed(msg) => write!(f, "Container exec failed: {}", msg),
            Self::IoError(msg) => write!(f, "IO error: {}", msg),
            Self::Timeout => write!(f, "Sandbox evaluation timed out"),
        }
    }
}

impl std::error::Error for SandboxError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_test_counts() {
        let output =
            "test result: ok. 5198 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out";
        let (passed, total) = parse_test_counts(output);
        assert_eq!(passed, 5198);
        assert_eq!(total, 5200);
    }

    #[test]
    fn test_parse_memory_string() {
        assert_eq!(parse_memory_string("256.5MiB / 4GiB"), 268_959_744);
        assert_eq!(parse_memory_string("1.5GiB / 4GiB"), 1_610_612_736);
        assert_eq!(parse_memory_string("512KiB / 4GiB"), 524_288);
    }

    #[test]
    fn test_sandbox_config_default() {
        let cfg = SandboxConfig::default();
        assert_eq!(cfg.cpus, "2");
        assert!(!cfg.network);
        assert_eq!(cfg.memory, "4g");
        assert_eq!(cfg.image, "selfware:latest");
        assert_eq!(cfg.timeout, Duration::from_secs(3600));
    }

    #[test]
    fn test_parse_test_counts_no_result_line() {
        let output = "running 10 tests\ntest foo ... ok\ntest bar ... ok";
        let (passed, total) = parse_test_counts(output);
        assert_eq!(passed, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_parse_test_counts_all_passed() {
        let output = "test result: ok. 100 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
        let (passed, total) = parse_test_counts(output);
        assert_eq!(passed, 100);
        assert_eq!(total, 100);
    }

    #[test]
    fn test_parse_test_counts_all_failed() {
        let output =
            "test result: FAILED. 0 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out";
        let (passed, total) = parse_test_counts(output);
        assert_eq!(passed, 0);
        assert_eq!(total, 5);
    }

    #[test]
    fn test_parse_test_counts_with_ignored() {
        let output =
            "test result: ok. 80 passed; 0 failed; 20 ignored; 0 measured; 0 filtered out";
        let (passed, total) = parse_test_counts(output);
        assert_eq!(passed, 80);
        assert_eq!(total, 100); // 80 + 0 + 20
    }

    #[test]
    fn test_parse_test_counts_empty_output() {
        let (passed, total) = parse_test_counts("");
        assert_eq!(passed, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_parse_memory_string_unknown_unit() {
        // "100B / 4GiB" — no recognized unit prefix → 0
        assert_eq!(parse_memory_string("100B / 4GiB"), 0);
    }

    #[test]
    fn test_parse_memory_string_empty() {
        assert_eq!(parse_memory_string(""), 0);
    }

    #[test]
    fn test_parse_memory_string_no_slash() {
        // "256MiB" without the " / limit" part
        let result = parse_memory_string("256MiB");
        let expected = (256.0 * 1024.0 * 1024.0) as u64;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_parse_test_counts_multiple_result_lines() {
        // Multiple test result lines — should use the last one (rev iteration)
        let output = "\
test result: ok. 50 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 100 passed; 2 failed; 3 ignored; 0 measured; 0 filtered out";
        let (passed, total) = parse_test_counts(output);
        // Last line: 100 passed, 2 failed, 3 ignored
        assert_eq!(passed, 100);
        assert_eq!(total, 105);
    }

    #[test]
    fn test_sandbox_error_display() {
        assert!(format!("{}", SandboxError::DockerFailed("no docker".into()))
            .contains("no docker"));
        assert!(
            format!("{}", SandboxError::ExecFailed("cmd failed".into())).contains("cmd failed")
        );
        assert!(format!("{}", SandboxError::IoError("disk full".into())).contains("disk full"));
        assert!(format!("{}", SandboxError::Timeout).contains("timed out"));
    }
}
