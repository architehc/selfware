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
        .env_remove("GIT_INDEX_FILE")
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
        .env_remove("GIT_INDEX_FILE")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_root)
        .output()
        .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;

    if !output.status.success() {
        // Force cleanup if normal removal fails
        let _ = std::fs::remove_dir_all(worktree_path);
        let _ = Command::new("git")
            .env_remove("GIT_INDEX_FILE")
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
    PatchApplicationFailed(String),
}

impl std::fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitFailed(msg) => write!(f, "Git worktree operation failed: {}", msg),
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::PatchApplicationFailed(msg) => write!(f, "Patch application failed: {}", msg),
        }
    }
}

impl std::error::Error for WorktreeError {}

/// Restore a shadow worktree to the exact source state of a parent attempt
/// by looking up its ancestor chain in the attempts file and sequentially
/// applying their patches in forward chronological order.
pub fn restore_worktree_parent_state(
    worktree: &Path,
    attempts_file: &Path,
    parent_id: Option<&str>,
) -> Result<Vec<String>, WorktreeError> {
    let Some(pid) = parent_id else {
        return Ok(Vec::new());
    };
    if pid.is_empty() || pid == "att-baseline" {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(attempts_file).map_err(WorktreeError::IoError)?;
    let mut nodes_by_id = std::collections::HashMap::new();
    for line in content.lines() {
        if let Ok(node) = serde_json::from_str::<crate::evolution::tree_log::AttemptNode>(line) {
            nodes_by_id.insert(node.id.clone(), node);
        }
    }

    let mut ancestor_chain = Vec::new();
    let mut current = pid.to_string();
    let mut visited = std::collections::HashSet::new();

    while let Some(node) = nodes_by_id.get(&current) {
        if !visited.insert(current.clone()) {
            break; // cycle protection
        }
        ancestor_chain.push(node.clone());
        match node.parent_id.as_deref() {
            None | Some("att-baseline") | Some("") => break,
            Some(parent) => {
                current = parent.to_string();
            }
        }
    }

    if ancestor_chain.is_empty() {
        return Err(WorktreeError::GitFailed(format!(
            "Parent attempt '{pid}' not found in attempts ledger"
        )));
    }

    // ancestor_chain is [pid, parent, ..., root_ancestor].
    // If any node in the chain has status AttemptStatus::Evaluated, its patch is tested_diff
    // relative to HEAD, which already incorporates all its prior ancestors.
    // Prune ancestors older than the latest Evaluated attempt.
    let prune_idx = ancestor_chain
        .iter()
        .position(|n| n.status == crate::evolution::tree_log::AttemptStatus::Evaluated);

    if let Some(idx) = prune_idx {
        ancestor_chain.truncate(idx + 1);
    }

    // Apply from oldest ancestor down to the parent
    ancestor_chain.reverse();
    let mut restored_ids = Vec::new();

    for ancestor in ancestor_chain {
        if let Some(ref patch) = ancestor.patch {
            if !patch.trim().is_empty() {
                let ok = crate::evolution::daemon::apply_edits(worktree, patch);
                if !ok && !is_patch_already_applied(worktree, patch) {
                    return Err(WorktreeError::PatchApplicationFailed(format!(
                        "Failed to apply ancestor patch from attempt '{}' while restoring parent '{pid}'",
                        ancestor.id
                    )));
                }
            }
        }
        restored_ids.push(ancestor.id);
    }

    Ok(restored_ids)
}

fn is_patch_already_applied(dir: &Path, patch: &str) -> bool {
    // Check JSON search-replace format
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            for edit in &edits {
                let Some(file) = edit["file"].as_str() else {
                    return false;
                };
                let Some(search) = edit["search"].as_str() else {
                    return false;
                };
                let Some(replace) = edit["replace"].as_str() else {
                    return false;
                };
                let target = dir.join(file);
                let Ok(content) = std::fs::read_to_string(&target) else {
                    return false;
                };
                if !content.contains(replace) || content.contains(search) {
                    return false;
                }
            }
            return true;
        }
    }

    // Unified diff format: check with git apply -R --check
    let temp_patch = dir.join(format!(".test-check-{}.patch", uuid_short()));
    if std::fs::write(&temp_patch, patch).is_ok() {
        let status = std::process::Command::new("git")
            .args(["apply", "-R", "--check"])
            .arg(&temp_patch)
            .current_dir(dir)
            .output();
        let _ = std::fs::remove_file(&temp_patch);
        if let Ok(output) = status {
            return output.status.success();
        }
    }

    false
}

/// Create an isolated git worktree for mutation testing, restored to the code state of `parent_id`.
pub fn create_shadow_worktree_for_parent(
    repo_root: &Path,
    attempts_file: &Path,
    parent_id: Option<&str>,
) -> Result<PathBuf, WorktreeError> {
    let worktree_name = format!("evolution-{}", uuid_short());
    let worktree_path = create_shadow_worktree_named(repo_root, &worktree_name)?;

    if let Err(e) = restore_worktree_parent_state(&worktree_path, attempts_file, parent_id) {
        let _ = cleanup_worktree(repo_root, &worktree_path);
        return Err(e);
    }

    Ok(worktree_path)
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/ast_tools/ast_tools_test.rs"]
mod tests;
