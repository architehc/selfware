//! AST-Aware Mutation Tools
//!
//! Uses the `syn` crate to manipulate Rust code at the AST level rather than
//! as raw strings. Every mutation is gated by a synchronous `cargo check` —
//! the compiler acts as the "laws of physics" that prune invalid mutations
//! before they waste context windows or evaluation cycles.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::evolve::diagnostics::CompilerDiagnostic;

/// Result of an AST mutation attempt
#[derive(Debug)]
pub struct AstMutationResult {
    /// Whether the mutation compiled successfully
    pub success: bool,
    /// Compiler errors (empty if success)
    pub compiler_errors: Vec<CompilerDiagnostic>,
    /// Unified diff of the change
    pub diff: String,
    /// Path to the git worktree containing the mutation
    pub worktree_path: Option<PathBuf>,
}

impl AstMutationResult {
    pub fn compile_failed(errors: Vec<CompilerDiagnostic>) -> Self {
        Self {
            success: false,
            compiler_errors: errors,
            diff: String::new(),
            worktree_path: None,
        }
    }

    pub fn not_found(fn_name: &str) -> Self {
        Self {
            success: false,
            compiler_errors: vec![CompilerDiagnostic {
                level: "error".to_string(),
                code: None,
                message: format!("Function `{}` not found in target file", fn_name),
                rendered: None,
                spans: Vec::new(),
            }],
            diff: String::new(),
            worktree_path: None,
        }
    }

    /// Format errors for injection into agent's working memory
    pub fn error_prompt(&self) -> String {
        if self.success {
            return String::from("Mutation compiled successfully.");
        }
        let mut prompt = String::from("FROST ❄️ — Compiler rejected mutation:\n\n");
        for err in &self.compiler_errors {
            let primary = err
                .spans
                .iter()
                .find(|s| s.is_primary)
                .or(err.spans.first());
            match primary {
                Some(span) => prompt.push_str(&format!(
                    "  [{}] {}:{},{}: {}\n",
                    err.level, span.file, span.line_start, span.column_start, err.message
                )),
                None => prompt.push_str(&format!("  [{}] {}\n", err.level, err.message)),
            }
            if let Some(label) = primary.and_then(|s| s.label.as_deref()) {
                if !label.is_empty() {
                    prompt.push_str(&format!("         | {}\n", label));
                }
            }
        }
        prompt
    }
}

/// Create an isolated git worktree for mutation testing
pub fn create_shadow_worktree(repo_root: &Path) -> Result<PathBuf, WorktreeError> {
    let worktree_name = format!("evolution-{}", uuid_short());
    create_shadow_worktree_named(repo_root, &worktree_name)
}

/// Create an isolated git worktree with an explicit name. Callers that share
/// the `.worktrees/` namespace with other features (e.g. apply staging vs.
/// mutation testing) MUST use a distinguishing prefix so lifecycle pruning
/// never reaps another feature's live worktree.
pub fn create_shadow_worktree_named(
    repo_root: &Path,
    worktree_name: &str,
) -> Result<PathBuf, WorktreeError> {
    let worktree_path = repo_root.join(".worktrees").join(worktree_name);

    let output = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        return Err(WorktreeError::GitFailed(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(worktree_path)
}

/// Remove a git worktree after evaluation
pub fn cleanup_worktree(repo_root: &Path, worktree_path: &Path) -> Result<(), WorktreeError> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_root)
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        // Force cleanup if normal removal fails
        let _ = std::fs::remove_dir_all(worktree_path);
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(repo_root)
            .output();
    }

    Ok(())
}

fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", t % 0xFFFF_FFFF)
}

#[derive(Debug)]
pub enum WorktreeError {
    GitFailed(String),
    IoError(std::io::Error),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitFailed(msg) => write!(f, "Git worktree operation failed: {}", msg),
            Self::IoError(e) => write!(f, "IO error: {}", e),
        }
    }
}

impl std::error::Error for WorktreeError {}

#[cfg(test)]
#[path = "../../tests/unit/evolution/ast_tools/ast_tools_test.rs"]
mod tests;
