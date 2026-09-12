//! Which project a verification result actually concerns.
//!
//! A verification failure used to be a bare string, so the completion gate
//! could not tell a failing test in the task's own project from a compile error
//! in an unrelated crate that happened to enclose it. Both blocked completion,
//! and — the mirror of the same blindness — any passing check cleared any
//! failure, so an unrelated green run erased a relevant red one.
//!
//! Reproduced deterministically: a Python project under a broken Rust parent.
//! `cargo_check` run from the Python directory walks UP to the parent
//! `Cargo.toml`, fails there, and the gate refuses to let a correct, tested
//! Python repair finish.
//!
//! The fix is not to ignore foreign failures. It is to record what each result
//! concerned, and to let the gate ask.

use std::path::{Path, PathBuf};

/// How a verification result relates to the task's own work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relevance {
    /// The result concerns the task's working root, or something inside it.
    InScope,
    /// The result concerns a project that encloses or sits beside the task.
    /// Reported, never silently blocking.
    OutOfScope,
    /// The scope could not be established. Treated as in-scope: an unknown
    /// failure must not be waved through.
    Unknown,
}

/// Where a verification command was actually pointed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationScope {
    /// Directory the command ran in.
    pub working_dir: PathBuf,
    /// The project the runner resolved to — for cargo, the directory holding
    /// the `Cargo.toml` it walked up to. `None` when it cannot be determined.
    pub project_root: Option<PathBuf>,
}

impl VerificationScope {
    /// Relevance to a task rooted at `task_root`.
    ///
    /// In scope when the project root is the task root or lies beneath it.
    /// Out of scope when it strictly encloses the task root — that is the
    /// nested case, and it is the only one that can be established as foreign
    /// with confidence. Everything else stays Unknown, which blocks.
    pub fn relevance_to(&self, task_root: &Path) -> Relevance {
        let Some(project_root) = &self.project_root else {
            return Relevance::Unknown;
        };
        let project_root = normalise(project_root);
        let task_root = normalise(task_root);
        if project_root == task_root || project_root.starts_with(&task_root) {
            return Relevance::InScope;
        }
        if task_root.starts_with(&project_root) {
            // The project encloses the task: a parent workspace the runner
            // discovered by walking up, not something this task is working on.
            return Relevance::OutOfScope;
        }
        Relevance::Unknown
    }
}

fn normalise(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Resolve the project a cargo invocation will actually operate on, by the same
/// upward walk cargo performs.
///
/// Returning the discovered root — rather than assuming the working directory —
/// is what makes the nested case visible instead of silent.
pub fn cargo_project_root(working_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(normalise(working_dir));
    while let Some(dir) = current {
        if dir.join("Cargo.toml").is_file() {
            return Some(dir);
        }
        current = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// Whether a cargo-based check can meaningfully run for a task rooted here.
///
/// False when the nearest manifest lies outside the task root: offering
/// `cargo_check` to a Python task nested in a Rust repository invites exactly
/// the failure this module exists for.
pub fn cargo_applies_to_task(task_root: &Path) -> bool {
    match cargo_project_root(task_root) {
        Some(root) => {
            let root = normalise(&root);
            let task_root = normalise(task_root);
            root == task_root || root.starts_with(&task_root)
        }
        None => false,
    }
}

/// A recorded verification outcome with the scope it concerned.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationRecord {
    pub command: String,
    pub scope: VerificationScope,
    pub passed: bool,
    /// Mutation sequence at the time the result was recorded, so a later edit
    /// makes it stale.
    pub mutation_sequence: usize,
    pub summary: String,
}

impl VerificationRecord {
    pub fn relevance_to(&self, task_root: &Path) -> Relevance {
        self.scope.relevance_to(task_root)
    }

    /// Whether this result still describes the current tree.
    pub fn is_stale(&self, current_mutation_sequence: usize) -> bool {
        current_mutation_sequence > self.mutation_sequence
    }

    /// Whether this result should block completion for a task rooted here.
    ///
    /// Only a relevant, current failure blocks. An out-of-scope failure is
    /// reported and does not block; an unknown one does, because it cannot be
    /// established as harmless.
    pub fn blocks_completion(&self, task_root: &Path, current_mutation_sequence: usize) -> bool {
        if self.passed || self.is_stale(current_mutation_sequence) {
            return false;
        }
        !matches!(self.relevance_to(task_root), Relevance::OutOfScope)
    }

    /// Whether a passing result may clear `other`.
    ///
    /// A green check clears only failures in the same scope. Previously any
    /// success wiped the single stored failure, so an unrelated passing command
    /// erased a relevant red one — the mirror image of the blocking bug.
    /// Two unresolvable scopes count as the same scope. Requiring a known root
    /// meant a pass could never clear its own failure where no project could be
    /// resolved, leaving the agent permanently blocked — a worse failure than
    /// the one being prevented. Only a KNOWN, different scope refuses.
    pub fn clears(&self, other: &VerificationRecord) -> bool {
        self.passed && !other.passed && self.scope.project_root == other.scope.project_root
    }
}

/// Resolve the scope a verification command will operate on.
///
/// Cargo tools resolve to the manifest cargo would discover by walking up —
/// which is the whole point, since that walk is what reaches outside the task.
/// Everything else is attributed to the working directory.
pub fn scope_for_command(tool: &str, command: &str, working_dir: &Path) -> VerificationScope {
    let is_cargo = tool.starts_with("cargo_") || command.trim_start().starts_with("cargo ");
    let project_root = if is_cargo {
        cargo_project_root(working_dir)
    } else {
        Some(working_dir.to_path_buf())
    };
    VerificationScope {
        working_dir: working_dir.to_path_buf(),
        project_root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A Python project nested inside a Rust project, as the reproduction has.
    fn nested() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let parent = tmp.path().to_path_buf();
        fs::write(parent.join("Cargo.toml"), "[package]\nname=\"p\"\n").unwrap();
        let py = parent.join("pyproj");
        fs::create_dir_all(&py).unwrap();
        (tmp, parent, py)
    }

    fn record(
        command: &str,
        root: Option<&Path>,
        cwd: &Path,
        passed: bool,
        seq: usize,
    ) -> VerificationRecord {
        VerificationRecord {
            command: command.to_string(),
            scope: VerificationScope {
                working_dir: cwd.to_path_buf(),
                project_root: root.map(Path::to_path_buf),
            },
            passed,
            mutation_sequence: seq,
            summary: format!("{command} result"),
        }
    }

    #[test]
    fn cargo_resolves_the_enclosing_manifest_not_the_working_directory() {
        // This upward walk is the whole mechanism: run from pyproj, cargo finds
        // the PARENT manifest and reports failures from a project the task is
        // not working on.
        let (_tmp, parent, py) = nested();
        assert_eq!(
            cargo_project_root(&py).map(|p| p.canonicalize().unwrap()),
            Some(parent.canonicalize().unwrap())
        );
        assert!(
            !cargo_applies_to_task(&py),
            "a Python task must not be offered cargo checks belonging to its parent"
        );
        assert!(
            cargo_applies_to_task(&parent),
            "the Rust project itself still qualifies"
        );
    }

    #[test]
    fn python_passes_and_an_unrelated_parent_rust_build_fails() {
        // Acceptance row 1: the task can complete.
        let (_tmp, parent, py) = nested();
        let foreign = record("cargo_check", Some(&parent), &py, false, 1);
        assert_eq!(foreign.relevance_to(&py), Relevance::OutOfScope);
        assert!(
            !foreign.blocks_completion(&py, 1),
            "a failure in the enclosing workspace must not block the nested task"
        );
    }

    #[test]
    fn a_relevant_failing_test_blocks_completion() {
        // Acceptance row 2.
        let (_tmp, _parent, py) = nested();
        let own = record("python3 -m unittest", Some(&py), &py, false, 1);
        assert_eq!(own.relevance_to(&py), Relevance::InScope);
        assert!(own.blocks_completion(&py, 1));
    }

    #[test]
    fn an_unrelated_green_check_does_not_erase_a_relevant_failure() {
        // Acceptance row 3, and the mirror of the blocking bug: the previous
        // code cleared the single stored failure on ANY success.
        let (_tmp, parent, py) = nested();
        let relevant_failure = record("python3 -m unittest", Some(&py), &py, false, 1);
        let foreign_green = record("cargo_check", Some(&parent), &py, true, 1);
        assert!(
            !foreign_green.clears(&relevant_failure),
            "a green check in another project must not clear this task's failure"
        );
        assert!(relevant_failure.blocks_completion(&py, 1), "still blocked");

        let own_green = record("python3 -m unittest", Some(&py), &py, true, 1);
        assert!(
            own_green.clears(&relevant_failure),
            "a green run in the same scope does clear it"
        );
    }

    #[test]
    fn a_result_taken_before_the_latest_edit_is_stale() {
        // Acceptance row 4.
        let (_tmp, _parent, py) = nested();
        let earlier = record("python3 -m unittest", Some(&py), &py, false, 3);
        assert!(!earlier.is_stale(3));
        assert!(earlier.is_stale(4), "an edit after the run makes it stale");
        assert!(
            !earlier.blocks_completion(&py, 4),
            "a stale failure describes a tree that no longer exists"
        );
    }

    #[test]
    fn a_task_that_includes_both_projects_keeps_both_checks() {
        // Acceptance row 5: rooted at the parent, the Rust failure is in scope.
        let (_tmp, parent, py) = nested();
        let rust = record("cargo_check", Some(&parent), &parent, false, 1);
        let python = record("python3 -m unittest", Some(&py), &py, false, 1);
        assert_eq!(rust.relevance_to(&parent), Relevance::InScope);
        assert_eq!(python.relevance_to(&parent), Relevance::InScope);
        assert!(rust.blocks_completion(&parent, 1));
        assert!(python.blocks_completion(&parent, 1));
    }

    #[test]
    fn an_unestablished_scope_still_blocks() {
        // Unknown is not permission. A failure whose project cannot be resolved
        // must not be waved through just because it cannot be attributed.
        let (_tmp, _parent, py) = nested();
        let opaque = record("make check", None, &py, false, 1);
        assert_eq!(opaque.relevance_to(&py), Relevance::Unknown);
        assert!(opaque.blocks_completion(&py, 1));
    }

    #[test]
    fn a_pass_clears_its_own_failure_even_when_no_project_can_be_resolved() {
        // Requiring a resolvable root meant an unresolvable scope could never
        // be cleared, so the agent could never finish. Unknown == unknown.
        let (_tmp, _parent, py) = nested();
        let failed = record("make check", None, &py, false, 1);
        let passed = record("make check", None, &py, true, 1);
        assert!(passed.clears(&failed));
    }

    #[test]
    fn a_sibling_project_is_not_silently_dismissed() {
        // Out-of-scope is reserved for projects that ENCLOSE the task, which is
        // the case that can be established. A sibling is Unknown, and blocks.
        let (_tmp, parent, py) = nested();
        let sibling = parent.join("other");
        fs::create_dir_all(&sibling).unwrap();
        let record = record("cargo_check", Some(&sibling), &py, false, 1);
        assert_eq!(record.relevance_to(&py), Relevance::Unknown);
        assert!(record.blocks_completion(&py, 1));
    }
}
