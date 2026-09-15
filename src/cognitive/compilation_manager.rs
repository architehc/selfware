use anyhow::{anyhow, Result};
use std::collections::HashSet;
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
    /// Creates a new compilation sandbox by copying the current project root to an isolated working directory.
    ///
    /// ### Contract
    /// - **Size Limits**: Enforces a 10 MB per-file limit (`MAX_UNTRACKED_FILE_SIZE`) and a 50 MB
    ///   aggregate limit (`MAX_AGGREGATE_UNTRACKED_SIZE`) on untracked files copied into the sandbox.
    /// - **Symlink Containment**: Untracked internal absolute symlinks are rewritten to sandbox-relative
    ///   paths; untracked symlinks targeting paths outside the repository are rejected (fail-closed).
    ///   Tracked internal absolute symlinks are rebased to the sandbox root, and all symlinks (including
    ///   intermediate chains) are verified to resolve strictly within the sandbox boundaries.
    /// - **Directory Exclusion**: Only the active ephemeral sandbox directory
    ///   (`.selfware-sandbox-<uuid>`) is excluded from untracked file replication, preserving legitimate user
    ///   files that happen to share the prefix.
    pub fn new(project_root: impl AsRef<Path>) -> Result<Self> {
        let original_dir = project_root.as_ref().to_path_buf();
        let work_dir_name = format!(".selfware-sandbox-{}", uuid::Uuid::new_v4().simple());
        let work_dir = original_dir.join(&work_dir_name);

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
        //
        // Note on security boundary: Tracked symlinks checked in to git history
        // are checked out directly by git during clone/apply. Untracked symlinks
        // in the working tree are screened for containment: external targets
        // are rejected (fail-closed) to prevent sandbox escape, while internal
        // absolute targets are rewritten to sandbox-relative paths so they evaluate
        // against sandbox copies rather than host repository files.
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
        const MAX_AGGREGATE_UNTRACKED_SIZE: u64 = 50 * 1024 * 1024; // 50 MB total

        let mut aggregate_untracked_size: u64 = 0;

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
            // Only skip the specific active sandbox directory created for this instance;
            // do not silently drop user files that happen to begin with the prefix.
            if rel == Path::new(&work_dir_name) || rel.starts_with(&work_dir_name) {
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
                    let target = std::fs::read_link(&src).map_err(|e| {
                        cleanup_on_fail(anyhow!(
                            "Failed reading untracked symlink target {:?}: {e}",
                            rel
                        ))
                    })?;
                    let rewritten_target =
                        match rewrite_symlink_target_for_sandbox(&original_dir, &src, &target) {
                            Some(t) => t,
                            None => {
                                return Err(cleanup_on_fail(anyhow!(
                                    "Untracked symlink {:?} targets path outside repository: {:?}",
                                    rel,
                                    target
                                )));
                            }
                        };
                    if let Some(parent) = dst.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            cleanup_on_fail(anyhow!(
                                "Failed creating parent directory for untracked symlink {:?}: {e}",
                                rel
                            ))
                        })?;
                    }
                    std::os::unix::fs::symlink(&rewritten_target, &dst).map_err(|e| {
                        cleanup_on_fail(anyhow!(
                            "Failed replicating untracked symlink {:?} -> {:?}: {e}",
                            rel,
                            rewritten_target
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
                aggregate_untracked_size = aggregate_untracked_size.saturating_add(meta.len());
                if aggregate_untracked_size > MAX_AGGREGATE_UNTRACKED_SIZE {
                    return Err(cleanup_on_fail(anyhow!(
                        "Aggregate untracked file size limit exceeded ({} bytes > {} bytes) at {:?}",
                        aggregate_untracked_size,
                        MAX_AGGREGATE_UNTRACKED_SIZE,
                        rel
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

        #[cfg(unix)]
        rebase_and_validate_sandbox_symlinks(&work_dir, &original_dir)
            .map_err(|e| cleanup_on_fail(anyhow!("Sandbox symlink validation failed: {e}")))?;

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

fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut components = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                } else if !path.is_absolute() {
                    components.push(Component::ParentDir);
                }
            }
            c => components.push(c),
        }
    }
    components.into_iter().collect()
}

pub(crate) fn make_relative_path(from_dir: &Path, to_file: &Path) -> PathBuf {
    let from_comps: Vec<_> = from_dir
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();
    let to_comps: Vec<_> = to_file
        .components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect();

    let mut common = 0;
    while common < from_comps.len()
        && common < to_comps.len()
        && from_comps[common] == to_comps[common]
    {
        common += 1;
    }

    let mut rel = PathBuf::new();
    for _ in common..from_comps.len() {
        rel.push("..");
    }
    for comp in &to_comps[common..] {
        rel.push(comp.as_os_str());
    }
    if rel.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        rel
    }
}

pub(crate) fn rewrite_symlink_target_for_sandbox(
    base_dir: &Path,
    symlink_file: &Path,
    target: &Path,
) -> Option<PathBuf> {
    let parent = symlink_file.parent().unwrap_or(base_dir);
    let resolved = if target.is_absolute() {
        target.to_path_buf()
    } else {
        parent.join(target)
    };

    let norm_base = lexical_normalize(base_dir);
    let c_base = base_dir.canonicalize().ok();

    // Determine if resolved target is contained in base_dir, and which base representation matched.
    // Keying off the matching base prevents macOS /var vs /private/var canonical alias escapes.
    let (is_contained, matched_base_is_canonical) =
        if let (Some(ref cb), Ok(ct)) = (&c_base, resolved.canonicalize()) {
            if ct.starts_with(cb) {
                (true, true)
            } else {
                (false, false)
            }
        } else {
            let norm_target = lexical_normalize(&resolved);
            if let Some(ref cb) = c_base {
                if norm_target.starts_with(cb) {
                    (true, true)
                } else if norm_target.starts_with(&norm_base) {
                    (true, false)
                } else {
                    (false, false)
                }
            } else if norm_target.starts_with(&norm_base) {
                (true, false)
            } else {
                (false, false)
            }
        };

    if !is_contained {
        return None;
    }

    // Relative targets within the repository resolve inside the sandbox identically,
    // provided lexical normalization does not escape the repository boundary.
    if target.is_relative() {
        let norm = lexical_normalize(&parent.join(target));
        let base_to_check = if matched_base_is_canonical {
            c_base.as_deref().unwrap_or(&norm_base)
        } else {
            &norm_base
        };
        let stays_inside = norm.starts_with(base_to_check)
            || (c_base.is_some() && norm.starts_with(c_base.as_ref().unwrap()));
        if stays_inside {
            return Some(target.to_path_buf());
        }
    }

    // Absolute internal targets must be rewritten to sandbox-relative so they resolve
    // to files within the sandbox copy instead of pointing back to the host repository.
    // Key BOTH rel_symlink_dir and rel_target_file off the EXACT base form that matched.
    let matched_base = if matched_base_is_canonical {
        c_base.as_deref().unwrap_or(&norm_base)
    } else {
        &norm_base
    };

    let parent_path = if matched_base_is_canonical {
        parent
            .canonicalize()
            .unwrap_or_else(|_| lexical_normalize(parent))
    } else {
        lexical_normalize(parent)
    };

    let target_path = if matched_base_is_canonical {
        resolved
            .canonicalize()
            .unwrap_or_else(|_| lexical_normalize(&resolved))
    } else {
        lexical_normalize(&resolved)
    };

    let rel_symlink_dir = parent_path
        .strip_prefix(matched_base)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| make_relative_path(matched_base, &parent_path));

    let rel_target_file = target_path
        .strip_prefix(matched_base)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| make_relative_path(matched_base, &target_path));

    Some(make_relative_path(&rel_symlink_dir, &rel_target_file))
}

#[cfg(unix)]
fn rebase_and_validate_sandbox_symlinks(work_dir: &Path, original_dir: &Path) -> Result<()> {
    let c_orig = original_dir.canonicalize().ok();
    let norm_orig = lexical_normalize(original_dir);
    let c_work = work_dir.canonicalize().ok();
    let norm_work = lexical_normalize(work_dir);

    // Recursively collect all symlinks in work_dir
    let mut symlinks = Vec::new();
    fn collect_symlinks(dir: &Path, symlinks: &mut Vec<PathBuf>) -> Result<()> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => return Err(anyhow!("Failed reading dir {:?}: {e}", dir)),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                collect_symlinks(&path, symlinks)?;
            } else if ft.is_symlink() {
                symlinks.push(path);
            }
        }
        Ok(())
    }

    collect_symlinks(work_dir, &mut symlinks)?;

    // 1. Rebase any tracked internal absolute symlinks pointing to original_dir
    for link_path in &symlinks {
        let raw_target = std::fs::read_link(link_path)?;
        if raw_target.is_absolute() {
            let points_to_orig = if let Some(ref co) = c_orig {
                raw_target.starts_with(co) || raw_target.starts_with(&norm_orig)
            } else {
                raw_target.starts_with(&norm_orig)
            };
            if points_to_orig {
                let rel_sub = if let Some(ref co) = c_orig {
                    raw_target
                        .strip_prefix(co)
                        .ok()
                        .or_else(|| raw_target.strip_prefix(&norm_orig).ok())
                } else {
                    raw_target.strip_prefix(&norm_orig).ok()
                };
                if let Some(sub) = rel_sub {
                    let new_abs = work_dir.join(sub);
                    let parent = link_path.parent().unwrap_or(work_dir);
                    let new_rel = make_relative_path(
                        &lexical_normalize(parent),
                        &lexical_normalize(&new_abs),
                    );
                    std::fs::remove_file(link_path)?;
                    std::os::unix::fs::symlink(&new_rel, link_path)?;
                }
            }
        }
    }

    // 2. Validate resolution of all symlinks in work_dir
    for link_path in &symlinks {
        validate_single_symlink_containment(work_dir, link_path, c_work.as_deref(), &norm_work)?;
    }

    Ok(())
}

#[cfg(unix)]
fn validate_single_symlink_containment(
    work_dir: &Path,
    symlink_path: &Path,
    c_work: Option<&Path>,
    norm_work: &Path,
) -> Result<()> {
    let mut current = symlink_path.to_path_buf();
    let mut visited = HashSet::new();

    for _ in 0..32 {
        if !visited.insert(current.clone()) {
            return Err(anyhow!(
                "Symlink cycle detected in sandbox at {:?}",
                symlink_path
            ));
        }

        let meta = match current.symlink_metadata() {
            Ok(m) => m,
            Err(_) => {
                // Target does not exist (dangling symlink). Verify lexical resolution does not escape work_dir
                let norm = lexical_normalize(&current);
                let contained = if let Some(cw) = c_work {
                    norm.starts_with(cw) || norm.starts_with(norm_work)
                } else {
                    norm.starts_with(norm_work)
                };
                if !contained {
                    return Err(anyhow!(
                        "Dangling symlink {:?} resolves outside sandbox to {:?}",
                        symlink_path,
                        norm
                    ));
                }
                return Ok(());
            }
        };

        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&current)?;
            if target.is_absolute() {
                let contained = if let Some(cw) = c_work {
                    target.starts_with(cw) || target.starts_with(norm_work)
                } else {
                    target.starts_with(norm_work)
                };
                if !contained {
                    return Err(anyhow!(
                        "Symlink {:?} targets path outside sandbox: {:?}",
                        symlink_path,
                        target
                    ));
                }
                current = target;
            } else {
                let parent = current.parent().unwrap_or(work_dir);
                let next = parent.join(&target);
                let norm = lexical_normalize(&next);
                let contained = if let Some(cw) = c_work {
                    norm.starts_with(cw) || norm.starts_with(norm_work)
                } else {
                    norm.starts_with(norm_work)
                };
                if !contained {
                    return Err(anyhow!(
                        "Symlink {:?} targets path outside sandbox: {:?}",
                        symlink_path,
                        norm
                    ));
                }
                current = next;
            }
        } else {
            // Reached non-symlink target; verify final canonical location
            if let Ok(canon) = current.canonicalize() {
                let contained = if let Some(cw) = c_work {
                    canon.starts_with(cw) || canon.starts_with(norm_work)
                } else {
                    canon.starts_with(norm_work)
                };
                if !contained {
                    return Err(anyhow!(
                        "Symlink {:?} resolves outside sandbox to {:?}",
                        symlink_path,
                        canon
                    ));
                }
            }
            return Ok(());
        }
    }

    Err(anyhow!(
        "Symlink resolution exceeded maximum hop depth (32) at {:?}",
        symlink_path
    ))
}

#[cfg(test)]
pub(crate) fn symlink_target_is_contained(
    base_dir: &Path,
    symlink_file: &Path,
    target: &Path,
) -> bool {
    rewrite_symlink_target_for_sandbox(base_dir, symlink_file, target).is_some()
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
