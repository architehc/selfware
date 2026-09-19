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

/// Create an isolated git worktree with an explicit name at a specific commit or ref.
pub fn create_shadow_worktree_named_at(
    repo_root: &Path,
    worktree_name: &str,
    commit_or_ref: Option<&str>,
) -> Result<PathBuf, WorktreeError> {
    let worktree_path = repo_root.join(".worktrees").join(worktree_name);
    let target_ref = commit_or_ref.unwrap_or("HEAD");

    let output = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg(target_ref)
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

/// Create an isolated git worktree with an explicit name. Callers that share
/// the `.worktrees/` namespace with other features (e.g. apply staging vs.
/// mutation testing) MUST use a distinguishing prefix so lifecycle pruning
/// never reaps another feature's live worktree.
pub fn create_shadow_worktree_named(
    repo_root: &Path,
    worktree_name: &str,
) -> Result<PathBuf, WorktreeError> {
    create_shadow_worktree_named_at(repo_root, worktree_name, None)
}

/// Resolves the immutable base git commit associated with a parent attempt or baseline.
/// Returns None if the attempts file is absent or the parent does not define a base commit.
pub fn resolve_parent_base_commit(attempts_file: &Path, parent_id: Option<&str>) -> Option<String> {
    let pid = parent_id?;
    if pid.is_empty() {
        return None;
    }

    let content = std::fs::read_to_string(attempts_file).ok()?;
    let mut nodes_by_id = std::collections::HashMap::new();
    for line in content.lines() {
        if let Ok(node) = serde_json::from_str::<crate::evolution::tree_log::AttemptNode>(line) {
            nodes_by_id.insert(node.id.clone(), node);
        }
    }

    if pid == "att-baseline" {
        return nodes_by_id
            .get("att-baseline")
            .and_then(|n| n.base_commit.clone());
    }

    let mut current = pid.to_string();
    let mut visited = std::collections::HashSet::new();
    while let Some(node) = nodes_by_id.get(&current) {
        if !visited.insert(current.clone()) {
            break; // cycle protection
        }
        if let Some(ref cc) = node.committed_commit {
            return Some(cc.clone());
        }
        if let Some(ref bc) = node.base_commit {
            return Some(bc.clone());
        }
        match node.parent_id.as_deref() {
            None | Some("") => break,
            Some("att-baseline") => {
                return nodes_by_id
                    .get("att-baseline")
                    .and_then(|n| n.base_commit.clone());
            }
            Some(parent) => current = parent.to_string(),
        }
    }

    None
}

/// Returns the current git HEAD commit hash of the repository.
pub fn get_git_head_commit(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if output.status.success() {
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !commit.is_empty() {
            return Some(commit);
        }
    }
    None
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

/// Apply a patch strictly: supports JSON search-replace edits and strict `git apply`.
/// Does not fall back to fuzzy patching with `patch -p1 -F3`.
pub fn apply_strict_patch(dir: &Path, patch: &str) -> bool {
    // Check JSON search-replace format first
    if let Ok(edits) = serde_json::from_str::<Vec<serde_json::Value>>(patch) {
        if !edits.is_empty() && edits[0].get("search").is_some() {
            return crate::evolution::daemon::apply_edits(dir, patch);
        }
    }

    // Unified diff format: strict git apply
    let patch_file = dir.join(format!(".restore-patch-{}", uuid_short()));
    if std::fs::write(&patch_file, patch).is_err() {
        return false;
    }
    let status = Command::new("git")
        .env_remove("GIT_INDEX_FILE")
        .args(["apply", "--whitespace=nowarn"])
        .arg(&patch_file)
        .current_dir(dir)
        .output();
    let _ = std::fs::remove_file(&patch_file);
    status.map(|o| o.status.success()).unwrap_or(false)
}

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

    // If this worktree is a git repository/worktree and an immutable base commit is recorded,
    // ensure the worktree is detached at that base commit so today's HEAD commits do not bleed in.
    if worktree.join(".git").exists() {
        if let Some(ref bc) = resolve_parent_base_commit(attempts_file, parent_id) {
            let out = Command::new("git")
                .env_remove("GIT_INDEX_FILE")
                .args(["checkout", "--detach", bc])
                .current_dir(worktree)
                .output()
                .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;
            if !out.status.success() {
                return Err(WorktreeError::GitFailed(format!(
                    "Failed to checkout base commit {bc}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        }
    }

    let content = std::fs::read_to_string(attempts_file).map_err(WorktreeError::IoError)?;
    let mut nodes_by_id = std::collections::HashMap::new();
    for line in content.lines() {
        if let Ok(node) = serde_json::from_str::<crate::evolution::tree_log::AttemptNode>(line) {
            nodes_by_id.insert(node.id.clone(), node);
        }
    }

    let Some(parent_node) = nodes_by_id.get(pid) else {
        return Err(WorktreeError::GitFailed(format!(
            "Parent attempt '{pid}' not found in attempts ledger"
        )));
    };

    // If parent was already committed to the base repository, the worktree detached at its commit
    // already contains all its code cleanly in the commit history.
    if parent_node.committed_commit.is_some() {
        return Ok(vec![pid.to_string()]);
    }

    let mut ancestor_chain = Vec::new();
    let mut current = pid.to_string();
    let mut visited = std::collections::HashSet::new();

    while let Some(node) = nodes_by_id.get(&current) {
        if !visited.insert(current.clone()) {
            break; // cycle protection
        }
        if node.committed_commit.is_some() {
            // This ancestor was committed to the repository, so everything up to here is in git history
            break;
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
        return Ok(vec![pid.to_string()]);
    }

    // ancestor_chain is [pid, parent, ..., root_ancestor].
    // If any node in the chain has a successful evaluation, its patch is tested_diff
    // relative to its base commit, which already incorporates all its prior ancestors.
    // Prune ancestors older than the latest successfully evaluated attempt.
    let prune_idx = ancestor_chain
        .iter()
        .position(|n| n.is_successful_evaluation());

    if let Some(idx) = prune_idx {
        ancestor_chain.truncate(idx + 1);
    }

    // Apply from oldest ancestor down to the parent
    ancestor_chain.reverse();
    let mut restored_ids = Vec::new();

    for ancestor in &ancestor_chain {
        if let Some(ref patch) = ancestor.patch {
            if !patch.trim().is_empty() {
                let ok = apply_strict_patch(worktree, patch);
                if !ok && !is_patch_already_applied(worktree, patch) {
                    return Err(WorktreeError::PatchApplicationFailed(format!(
                        "Failed to apply ancestor patch from attempt '{}' while restoring parent '{pid}'",
                        ancestor.id
                    )));
                }
            }
        }
        restored_ids.push(ancestor.id.clone());
    }

    // Verify cryptographic diff fidelity against parent's expected diff if in a git repository.
    // Must match daemon::capture_tested_diff canonical representation: `git add -A` followed by
    // `git diff --cached --binary HEAD`, which correctly includes newly added untracked files and binary changes.
    if worktree.join(".git").exists()
        && parent_node.status == crate::evolution::tree_log::AttemptStatus::Evaluated
        && parent_node.diff_sha256.len() == 64
    {
        let add_out = Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["add", "-A"])
            .current_dir(worktree)
            .output()
            .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;
        if !add_out.status.success() {
            return Err(WorktreeError::GitFailed(
                "Failed to stage changes for diff fidelity check".into(),
            ));
        }

        let diff_out = Command::new("git")
            .env_remove("GIT_INDEX_FILE")
            .args(["diff", "--cached", "--binary", "HEAD"])
            .current_dir(worktree)
            .output()
            .map_err(|e| WorktreeError::GitFailed(e.to_string()))?;
        if diff_out.status.success() {
            let actual_diff = String::from_utf8_lossy(&diff_out.stdout);
            let actual_sha = crate::evolution::tree_log::compute_sha256(actual_diff.as_bytes());
            if actual_sha != parent_node.diff_sha256 {
                return Err(WorktreeError::PatchApplicationFailed(format!(
                    "Restored worktree diff sha256 ({actual_sha}) does not match parent expected diff sha256 ({})",
                    parent_node.diff_sha256
                )));
            }
        }
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
    let base_commit = resolve_parent_base_commit(attempts_file, parent_id);
    let worktree_name = format!("evolution-{}", uuid_short());
    let worktree_path =
        create_shadow_worktree_named_at(repo_root, &worktree_name, base_commit.as_deref())?;

    if let Err(e) = restore_worktree_parent_state(&worktree_path, attempts_file, parent_id) {
        let _ = cleanup_worktree(repo_root, &worktree_path);
        return Err(e);
    }

    Ok(worktree_path)
}

#[cfg(test)]
#[path = "../../tests/unit/evolution/ast_tools/ast_tools_test.rs"]
mod tests;
