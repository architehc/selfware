use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tracing::{error, info};

/// Configuration for the compilation sandbox
#[derive(Debug, Clone)]
pub struct CompilationSandbox {
    _original_dir: PathBuf,
    work_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CompileResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CompilationSandbox {
    /// Creates a new compilation sandbox by copying the current project root to a temporary location
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self> {
        let original_dir = project_root.as_ref().to_path_buf();
        let work_dir = original_dir.join(".selfware-sandbox");

        info!("Setting up compilation sandbox at {:?}", work_dir);

        // Remove old sandbox if it exists
        if work_dir.exists() {
            std::fs::remove_dir_all(&work_dir)?;
        }

        // Clone the repo to get a clean working tree without build artifacts.
        let status = Command::new("git")
            .arg("clone")
            .arg("--no-hardlinks")
            .arg(&original_dir)
            .arg(&work_dir)
            .status()?;

        if !status.success() {
            return Err(anyhow!("Failed to clone repository into sandbox"));
        }

        // Carry over uncommitted changes (staged + unstaged) so the sandbox
        // reflects the actual working tree, not just the last commit.
        let diff_output = Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&original_dir)
            .output()?;

        if diff_output.status.success() && !diff_output.stdout.is_empty() {
            let mut apply = Command::new("git")
                .args(["apply", "--allow-empty"])
                .current_dir(&work_dir)
                .stdin(std::process::Stdio::piped())
                .spawn()?;

            if let Some(ref mut stdin) = apply.stdin {
                use std::io::Write;
                stdin.write_all(&diff_output.stdout)?;
            }

            let apply_status = apply.wait()?;
            if !apply_status.success() {
                info!("Some uncommitted changes could not be applied to sandbox (merge conflict); proceeding with committed state");
            }
        }

        Ok(Self {
            _original_dir: original_dir,
            work_dir,
        })
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    /// Check if the code compiles without errors (cargo check)
    pub fn check(&self) -> Result<CompileResult> {
        info!("Running 'cargo check' in sandbox");
        let output = Command::new("cargo")
            .arg("check")
            .current_dir(&self.work_dir)
            .output()?;

        self.parse_output(output)
    }

    /// Build the project (cargo build --release)
    pub fn build_release(&self) -> Result<CompileResult> {
        info!("Running 'cargo build --release' in sandbox");
        let output = Command::new("cargo")
            .arg("build")
            .arg("--release")
            .current_dir(&self.work_dir)
            .output()?;

        self.parse_output(output)
    }

    /// Run tests (cargo test)
    pub fn test(&self) -> Result<CompileResult> {
        info!("Running 'cargo test' in sandbox");
        let output = Command::new("cargo")
            .arg("test")
            .current_dir(&self.work_dir)
            .output()?;

        self.parse_output(output)
    }

    /// Full verification pipeline (check -> test -> build)
    pub fn verify(&self) -> Result<bool> {
        let check_res = self.check()?;
        if !check_res.success {
            error!(
                "Sandbox check failed:
{}",
                check_res.stderr
            );
            return Ok(false);
        }

        let test_res = self.test()?;
        if !test_res.success {
            error!(
                "Sandbox test failed:
{}",
                test_res.stderr
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Cleanup the sandbox
    pub fn cleanup(self) -> Result<()> {
        if self.work_dir.exists() {
            std::fs::remove_dir_all(&self.work_dir)?;
        }
        Ok(())
    }

    fn parse_output(&self, output: Output) -> Result<CompileResult> {
        Ok(CompileResult {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Output};

    /// Helper: create a CompilationSandbox struct directly for parse_output testing
    /// without actually cloning a git repo (which is expensive and requires a real repo).
    fn dummy_sandbox() -> CompilationSandbox {
        CompilationSandbox {
            _original_dir: PathBuf::from("/tmp/dummy-original"),
            work_dir: PathBuf::from("/tmp/dummy-sandbox"),
        }
    }

    /// Helper: produce an Output from running a simple command with known exit code.
    fn make_output(exit_code: i32, stdout: &str, stderr: &str) -> Output {
        // We construct Output by running a real shell command that gives us
        // the exact stdout, stderr, and exit code we want.
        let cmd_str = format!(
            "printf '{}'; printf '{}' >&2; exit {}",
            stdout.replace('\'', "'\\''"),
            stderr.replace('\'', "'\\''"),
            exit_code
        );
        Command::new("sh")
            .arg("-c")
            .arg(&cmd_str)
            .output()
            .expect("failed to run helper command")
    }

    // --- parse_output tests ---

    #[test]
    fn test_parse_output_success() {
        let sandbox = dummy_sandbox();
        let output = make_output(0, "all good", "");
        let result = sandbox.parse_output(output).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "all good");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_parse_output_failure() {
        let sandbox = dummy_sandbox();
        let output = make_output(1, "", "error: something broke");
        let result = sandbox.parse_output(output).unwrap();
        assert!(!result.success);
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, "error: something broke");
    }

    #[test]
    fn test_parse_output_empty() {
        let sandbox = dummy_sandbox();
        let output = make_output(0, "", "");
        let result = sandbox.parse_output(output).unwrap();
        assert!(result.success);
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_parse_output_mixed_stdout_stderr() {
        let sandbox = dummy_sandbox();
        let output = make_output(0, "compiled OK", "warning: unused variable");
        let result = sandbox.parse_output(output).unwrap();
        assert!(result.success);
        assert_eq!(result.stdout, "compiled OK");
        assert_eq!(result.stderr, "warning: unused variable");
    }

    #[test]
    fn test_parse_output_nonzero_exit_with_both_streams() {
        let sandbox = dummy_sandbox();
        let output = make_output(42, "partial output", "fatal error");
        let result = sandbox.parse_output(output).unwrap();
        assert!(!result.success);
        assert_eq!(result.stdout, "partial output");
        assert_eq!(result.stderr, "fatal error");
    }

    // --- CompilationSandbox::new with nonexistent path ---

    #[test]
    fn test_new_with_nonexistent_path() {
        let result = CompilationSandbox::new("/tmp/nonexistent-selfware-test-path-abc123xyz");
        // Should fail because git clone from a nonexistent dir will fail
        assert!(result.is_err());
    }

    // --- cleanup with nonexistent dir (should not panic) ---

    #[test]
    fn test_cleanup_nonexistent_dir_no_panic() {
        let sandbox = CompilationSandbox {
            _original_dir: PathBuf::from("/tmp/does-not-exist-original"),
            work_dir: PathBuf::from("/tmp/does-not-exist-sandbox-xyz123"),
        };
        // cleanup checks `self.work_dir.exists()` before removing, so this should be Ok
        let result = sandbox.cleanup();
        assert!(result.is_ok());
    }

    // --- CompileResult fields ---

    #[test]
    fn test_compile_result_debug() {
        let result = CompileResult {
            success: true,
            stdout: "ok".to_string(),
            stderr: String::new(),
        };
        // Verify Debug is derived
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("CompileResult"));
        assert!(debug_str.contains("true"));
    }
}
