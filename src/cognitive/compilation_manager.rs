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
        // Pin the child's cwd to the source repo. Without this the clone
        // inherits the process-global cwd, which other tests (and the
        // worktree/subagent flows) can move or delete mid-run; git then fails
        // the checkout with "fatal: this operation must be run in a work
        // tree" and the whole clone errors out intermittently.
        let status = Command::new("git")
            .arg("clone")
            .arg("--no-hardlinks")
            .arg(&original_dir)
            .arg(&work_dir)
            .current_dir(&original_dir)
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
#[path = "../../tests/unit/cognitive/compilation_manager/compilation_manager_test.rs"]
mod tests;
