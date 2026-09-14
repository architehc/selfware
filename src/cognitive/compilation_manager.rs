use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tracing::{error, info};

/// Configuration for the compilation sandbox (RAII guard: cleans up on drop).
#[derive(Debug)]
pub struct CompilationSandbox {
    _original_dir: PathBuf,
    work_dir: PathBuf,
    owns_work_dir: bool,
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
        let work_dir = original_dir.join(format!(
            ".selfware-sandbox-{}",
            uuid::Uuid::new_v4().simple()
        ));

        info!("Setting up compilation sandbox at {:?}", work_dir);

        let cleanup_on_fail = |err: anyhow::Error| -> anyhow::Error {
            if work_dir.exists() {
                let _ = std::fs::remove_dir_all(&work_dir);
            }
            err
        };

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
            .status()
            .map_err(|e| cleanup_on_fail(anyhow!("Failed to spawn git clone: {e}")))?;

        if !status.success() {
            return Err(cleanup_on_fail(anyhow!(
                "Failed to clone repository into sandbox"
            )));
        }

        // Carry over uncommitted changes (staged + unstaged) so the sandbox
        // reflects the exact working tree, not just the last commit.
        let diff_output = Command::new("git")
            .args(["diff", "HEAD", "--binary"])
            .current_dir(&original_dir)
            .output()
            .map_err(|e| cleanup_on_fail(anyhow!("Failed to run git diff: {e}")))?;

        if !diff_output.status.success() {
            let stderr = String::from_utf8_lossy(&diff_output.stderr);
            return Err(cleanup_on_fail(anyhow!(
                "Failed to compute working tree diff: {stderr}"
            )));
        }

        if !diff_output.stdout.is_empty() {
            let mut apply = Command::new("git")
                .args(["apply", "--allow-empty"])
                .current_dir(&work_dir)
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| cleanup_on_fail(anyhow!("Failed to spawn git apply: {e}")))?;

            if let Some(mut stdin) = apply.stdin.take() {
                use std::io::Write;
                stdin.write_all(&diff_output.stdout).map_err(|e| {
                    cleanup_on_fail(anyhow!("Failed to write patch to git apply stdin: {e}"))
                })?;
            }

            let apply_output = apply
                .wait_with_output()
                .map_err(|e| cleanup_on_fail(anyhow!("Failed waiting for git apply: {e}")))?;
            if !apply_output.status.success() {
                let stderr = String::from_utf8_lossy(&apply_output.stderr);
                return Err(cleanup_on_fail(anyhow!(
                    "Failed to apply uncommitted changes to sandbox: {stderr}"
                )));
            }
        }

        // Carry over untracked, non-ignored source files so new files in the working
        // tree are part of the evaluated sandbox snapshot. Use NUL-delimited output
        // so filenames with spaces, quotes, or special characters are safely preserved.
        let untracked_output = Command::new("git")
            .args(["ls-files", "-z", "--others", "--exclude-standard"])
            .current_dir(&original_dir)
            .output()
            .map_err(|e| cleanup_on_fail(anyhow!("Failed to list untracked files: {e}")))?;

        if !untracked_output.status.success() {
            let stderr = String::from_utf8_lossy(&untracked_output.stderr);
            return Err(cleanup_on_fail(anyhow!(
                "Failed to list untracked files in original repository: {stderr}"
            )));
        }

        const MAX_UNTRACKED_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB per file

        for item in untracked_output.stdout.split(|&b| b == 0) {
            if item.is_empty() {
                continue;
            }
            #[cfg(unix)]
            use std::os::unix::ffi::OsStrExt;
            #[cfg(unix)]
            let rel = Path::new(std::ffi::OsStr::from_bytes(item));
            #[cfg(not(unix))]
            let item_str = match std::str::from_utf8(item) {
                Ok(s) => s,
                Err(_) => continue,
            };
            #[cfg(not(unix))]
            let rel = Path::new(item_str);

            if rel.is_absolute() || rel.starts_with("..") {
                continue;
            }
            if rel.to_string_lossy().starts_with(".selfware-sandbox-") {
                continue;
            }
            let src = original_dir.join(rel);
            let dst = work_dir.join(rel);
            let meta = src.symlink_metadata().map_err(|e| {
                cleanup_on_fail(anyhow!(
                    "Failed reading metadata for untracked entry {:?}: {e}",
                    rel
                ))
            })?;

            if meta.is_dir() {
                continue;
            }

            if meta.file_type().is_symlink() {
                #[cfg(unix)]
                {
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            cleanup_on_fail(anyhow!(
                                "Failed creating parent directory for untracked symlink {:?}: {e}",
                                rel
                            ))
                        })?;
                    }
                    let target = std::fs::read_link(&src).map_err(|e| {
                        cleanup_on_fail(anyhow!(
                            "Failed reading untracked symlink target {:?}: {e}",
                            rel
                        ))
                    })?;
                    std::os::unix::fs::symlink(&target, &dst).map_err(|e| {
                        cleanup_on_fail(anyhow!(
                            "Failed replicating untracked symlink {:?}: {e}",
                            rel
                        ))
                    })?;
                }
                #[cfg(not(unix))]
                {
                    return Err(cleanup_on_fail(anyhow!(
                        "Untracked symlinks are unsupported on this platform: {:?}",
                        rel
                    )));
                }
            } else if meta.is_file() {
                if meta.len() > MAX_UNTRACKED_FILE_SIZE {
                    return Err(cleanup_on_fail(anyhow!(
                        "Untracked file {:?} exceeds size limit ({} bytes > {} bytes)",
                        rel,
                        meta.len(),
                        MAX_UNTRACKED_FILE_SIZE
                    )));
                }
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        cleanup_on_fail(anyhow!(
                            "Failed creating parent directory for untracked file {:?}: {e}",
                            rel
                        ))
                    })?;
                }
                std::fs::copy(&src, &dst).map_err(|e| {
                    cleanup_on_fail(anyhow!("Failed copying untracked file {:?}: {e}", rel))
                })?;
            } else {
                return Err(cleanup_on_fail(anyhow!(
                    "Unsupported untracked entry type for {:?}: expected regular file or symlink",
                    rel
                )));
            }
        }

        Ok(Self {
            _original_dir: original_dir,
            work_dir,
            owns_work_dir: true,
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

    /// Cleanup the sandbox manually (also cleaned up automatically on drop via RAII).
    pub fn cleanup(self) -> Result<()> {
        drop(self);
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

impl Drop for CompilationSandbox {
    fn drop(&mut self) {
        if self.owns_work_dir && self.work_dir.exists() {
            let _ = std::fs::remove_dir_all(&self.work_dir);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cognitive/compilation_manager/compilation_manager_test.rs"]
mod tests;
