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
    /// The command could not run at all here: the project has no such runner
    /// (a cargo command in a directory whose ancestry contains no
    /// `Cargo.toml`). NOT a failure — the suite never executed, so the record
    /// asserts nothing about the tree's correctness. Never blocks, and
    /// [`VerificationLedger::record`] drops it outright (the missing-manifest
    /// case reproduced on Python tasks: `cargo_test` in a directory with no
    /// Cargo.toml failed, was held as an unknown-scope failure, and a later
    /// passing unittest could not discharge it under its own check identity).
    NoRunner,
    /// The scope could not be established. Treated as in-scope: an unknown
    /// failure must not be waved through.
    Unknown,
}

/// Where a verification command was actually pointed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerificationScope {
    /// Directory the command ran in.
    pub working_dir: PathBuf,
    /// The project the runner resolved to — for cargo, the directory holding
    /// the `Cargo.toml` it walked up to. `None` when it cannot be determined.
    pub project_root: Option<PathBuf>,
    /// Whether a runner for this command exists in (or above) the working
    /// directory. `Some(false)` means the command could not have executed at
    /// all (cargo with no manifest anywhere in the ancestry) — the "no test
    /// runner / no project exists" case, distinct from "the suite ran and
    /// failed". `None` (old checkpoints, non-cargo commands) means unknown and
    /// is treated exactly as before.
    #[serde(default)]
    pub runner_exists: Option<bool>,
}

impl VerificationScope {
    /// Relevance to a task rooted at `task_root`.
    ///
    /// In scope when the project root is the task root or lies beneath it.
    /// Out of scope when it strictly encloses the task root — that is the
    /// nested case, and it is the only one that can be established as foreign
    /// with confidence. A command whose runner does not exist anywhere is
    /// `NoRunner` before any task comparison. Everything else stays Unknown,
    /// which blocks.
    pub fn relevance_to(&self, task_root: &Path) -> Relevance {
        if self.runner_exists == Some(false) {
            return Relevance::NoRunner;
        }
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerificationRecord {
    /// Which CHECK this was, normalised: `cargo test`, `cargo check`, `pytest`.
    ///
    /// A single failure slot meant any success cleared any failure, so a green
    /// `cargo check` erased a red `cargo test` — same project, different
    /// question. A check answers only for itself.
    pub check_id: String,
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
    /// reported and does not block; a `NoRunner` result is not a failure at
    /// all; an unknown one does, because it cannot be established as harmless.
    pub fn blocks_completion(&self, task_root: &Path, current_mutation_sequence: usize) -> bool {
        if self.passed || self.is_stale(current_mutation_sequence) {
            return false;
        }
        !matches!(
            self.relevance_to(task_root),
            Relevance::OutOfScope | Relevance::NoRunner
        )
    }

    /// Whether this record describes a runner that does not exist in (or
    /// above) the working directory — e.g. a cargo command with no manifest
    /// anywhere. Such a command never executed, so its failure asserts
    /// nothing about the tree.
    pub fn runner_is_missing(&self) -> bool {
        self.scope.runner_exists == Some(false)
    }

    /// Whether a passing result may clear `other`.
    ///
    /// A green check clears only the SAME check in the same scope.
    ///
    /// Two earlier versions were both wrong in the same direction. First, any
    /// success wiped the single stored failure, so an unrelated passing command
    /// erased a relevant red one. Then scope was compared but the check was
    /// not — and `cargo check` and `cargo test` resolve to the same project, so
    /// a passing compile still cleared a failing test suite.
    ///
    /// Scope matching also no longer treats two unresolvable roots as equal.
    /// It falls back to the working directory, which is always known, so a pass
    /// can still clear its own failure where no project resolves (the agent is
    /// never permanently blocked) without two unrelated directories clearing
    /// each other.
    pub fn clears(&self, other: &VerificationRecord) -> bool {
        if !self.passed || other.passed {
            return false;
        }
        if !self.same_scope_as(other) {
            return false;
        }
        if self.check_id == other.check_id {
            return true;
        }
        // Cargo identities carry their subset selection (`--lib`, `--doc`,
        // `-p x`, filters); coverage is decided structurally so a subset pass
        // never clears a broader failure.
        match (cargo_shape(&self.check_id), cargo_shape(&other.check_id)) {
            (Some(pass), Some(failed)) => return pass.covers(&failed),
            (Some(_), None) | (None, Some(_)) => return false,
            (None, None) => {}
        }
        if other.check_id.starts_with(&format!("{} ", self.check_id)) {
            return true;
        }
        false
    }

    /// Whether two records operate in the same scope.
    pub fn same_scope_as(&self, other: &VerificationRecord) -> bool {
        match (&self.scope.project_root, &other.scope.project_root) {
            (Some(a), Some(b)) => normalise(a) == normalise(b),
            // Neither root resolved. Unknown is not a match: fall back to the
            // directory the command actually ran in.
            (None, None) => {
                normalise(&self.scope.working_dir) == normalise(&other.scope.working_dir)
            }
            _ => false,
        }
    }

    /// Whether two records answer the same question about the same tree.
    pub fn same_check_as(&self, other: &VerificationRecord) -> bool {
        if self.check_id != other.check_id {
            return false;
        }
        self.same_scope_as(other)
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
        // `runner_exists`: a cargo command in a directory whose ancestry
        // contains no Cargo.toml could never have executed — the missing
        // runner case, which is not a failed test. Distinguishing the two is
        // what lets a Python-only workspace discharge a meaningless cargo
        // failure. Non-cargo commands leave it `None` (unknown, as before).
        runner_exists: if is_cargo {
            Some(project_root.is_some())
        } else {
            None
        },
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
            check_id: check_id_for(command, command),
            command: command.to_string(),
            scope: VerificationScope {
                working_dir: cwd.to_path_buf(),
                project_root: root.map(Path::to_path_buf),
                runner_exists: None,
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

    /// A Python-only workspace: NO Cargo.toml anywhere in the ancestry.
    fn python_only() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let py = tmp.path().join("pyproj");
        fs::create_dir_all(&py).unwrap();
        fs::write(py.join("solution.py"), "def f():\n    return 1\n").unwrap();
        (tmp, py)
    }

    #[test]
    fn missing_cargo_manifest_is_no_runner_not_a_failure() {
        // Finding 1(b): a `cargo_test` call in a directory with no Cargo.toml
        // is "no test runner / no project exists", NOT "the suite ran and
        // failed". Two live Python-task runs exited 1 after a correct, tested
        // fix because this failure was held as Unknown-scope and blocked.
        let (_tmp, py) = python_only();
        let scope = scope_for_command("cargo_test", "", &py);
        assert_eq!(scope.runner_exists, Some(false), "cargo cannot run here");
        let record = VerificationRecord {
            check_id: "cargo test".to_string(),
            command: "cargo_test".to_string(),
            scope,
            passed: false,
            mutation_sequence: 2,
            summary: "cargo_test failed: could not find Cargo.toml".to_string(),
        };
        assert_eq!(record.relevance_to(&py), Relevance::NoRunner);
        assert!(
            !record.blocks_completion(&py, 2),
            "a missing manifest must not block a correct completion"
        );
        // Classified as no-runner, NOT as a failure: the ledger must not
        // retain it -- a later passing unittest (a different check identity,
        // per the reproduction) cannot discharge it.
        let mut ledger = VerificationLedger::default();
        ledger.record(record);
        assert!(ledger.is_empty(), "a no-runner record asserts nothing");
        assert!(ledger.blocking(&py, 2).is_none());
    }

    #[test]
    fn python_unittest_failure_still_blocks_and_a_pass_discharges_it() {
        // Finding 1(c) preserved: a GENUINE in-scope failure blocks and a
        // genuine pass discharges it, in a Python-only workspace through the
        // real `scope_for_command` path.
        let (_tmp, py) = python_only();
        let mut ledger = VerificationLedger::default();
        ledger.record(VerificationRecord {
            check_id: "python3 unittest".to_string(),
            command: "python3 -m unittest".to_string(),
            scope: scope_for_command("shell_exec", "python3 -m unittest", &py),
            passed: false,
            mutation_sequence: 2,
            summary: "1 test failed".to_string(),
        });
        assert!(
            ledger.blocking(&py, 2).is_some(),
            "a real unittest failure in the task's own project still blocks"
        );
        ledger.record(VerificationRecord {
            check_id: "python3 unittest".to_string(),
            command: "python3 -m unittest".to_string(),
            scope: scope_for_command("shell_exec", "python3 -m unittest", &py),
            passed: true,
            mutation_sequence: 2,
            summary: "OK".to_string(),
        });
        assert!(
            ledger.blocking(&py, 2).is_none(),
            "the passing unittest discharges the failure it owns"
        );
    }

    #[test]
    fn check_id_preserves_test_selectors_and_drops_flags() {
        assert_eq!(check_id_for("shell_exec", "cargo test"), "cargo test");
        assert_eq!(
            check_id_for("shell_exec", "cargo test passing_test"),
            "cargo test passing_test"
        );
        // Subset selection is part of the identity (review finding: `--lib`
        // was dropped and a lib-only pass cleared a full-suite failure).
        assert_eq!(
            check_id_for("shell_exec", "cargo test --lib passing_test"),
            "cargo test --lib passing_test"
        );
        assert_eq!(
            check_id_for("shell_exec", "cargo test --lib -- --nocapture"),
            "cargo test --lib"
        );
        // Output-only libtest flags are still dropped.
        assert_eq!(
            check_id_for("shell_exec", "cargo test -- --nocapture --test-threads 1"),
            "cargo test"
        );
        assert_eq!(
            check_id_for("shell_exec", "python3 -m unittest test_user"),
            "python3 unittest test_user"
        );
        assert_eq!(
            check_id_for("shell_exec", "python3 -m unittest"),
            "python3 unittest"
        );
        assert_eq!(check_id_for("cargo_test", ""), "cargo test");
        assert_eq!(check_id_for("cargo_check", ""), "cargo check");
        assert_eq!(check_id_for("shell_exec", "cargo test 2>&1"), "cargo test");
        assert_eq!(
            check_id_for("shell_exec", "cargo test >/dev/null 2>&1"),
            "cargo test"
        );
    }

    #[test]
    fn passing_test_subset_cannot_clear_failing_full_suite() {
        let (_tmp, parent, _py) = nested();
        let full_suite_failure = record("cargo test", Some(&parent), &parent, false, 1);
        let subset_pass = record("cargo test passing_test", Some(&parent), &parent, true, 1);

        assert!(
            !subset_pass.clears(&full_suite_failure),
            "a passing test subset must NOT clear a failing full-suite result"
        );

        let full_suite_pass = record("cargo test", Some(&parent), &parent, true, 1);
        assert!(
            full_suite_pass.clears(&full_suite_failure),
            "a passing full-suite run clears the full-suite failure"
        );

        let subset_failure = record("cargo test failing_test", Some(&parent), &parent, false, 1);
        assert!(
            full_suite_pass.clears(&subset_failure),
            "a passing full-suite run clears narrower subset failures"
        );
    }

    #[test]
    fn check_id_keeps_every_subset_selector_and_strips_redirections() {
        for (command, id) in [
            ("cargo test --lib", "cargo test --lib"),
            ("cargo test --lib 2>&1", "cargo test --lib"),
            ("cargo test 2>&1 > out.log", "cargo test"),
            ("cargo test --bins", "cargo test --bins"),
            ("cargo test --doc", "cargo test --doc"),
            (
                "cargo test --test integration",
                "cargo test --test=integration",
            ),
            (
                "cargo test --test=integration",
                "cargo test --test=integration",
            ),
            ("cargo test -p core", "cargo test --package=core"),
            ("cargo test --package core", "cargo test --package=core"),
            ("cargo test -pcore", "cargo test --package=core"),
            ("cargo test -- parser", "cargo test parser"),
            ("cargo test parser", "cargo test parser"),
            (
                "cargo test -- --exact parser",
                "cargo test -- --exact parser",
            ),
            ("cargo test -- --ignored", "cargo test -- --ignored"),
            ("cargo test -j 4 --no-fail-fast", "cargo test"),
            ("RUST_BACKTRACE=1 cargo test", "cargo test"),
            ("cd sub && cargo test --lib", "cargo test --lib"),
            ("/usr/bin/cargo test --doc", "cargo test --doc"),
            ("cargo clippy -- -D warnings", "cargo clippy -- -D warnings"),
            ("pytest --lf", "pytest --lf"),
            ("pytest -q", "pytest"),
        ] {
            assert_eq!(check_id_for("shell_exec", command), id, "for `{command}`");
        }
        // The cargo_test tool's rendered command keeps package scope apart
        // from a test-name filter.
        assert_eq!(
            check_id_for("cargo_test", "cargo test -p core parser"),
            "cargo test --package=core parser"
        );
    }

    #[test]
    fn subset_selecting_passes_cannot_clear_broader_failures() {
        let (_tmp, parent, _py) = nested();
        let fail = |cmd: &str| record(cmd, Some(&parent), &parent, false, 1);
        let pass = |cmd: &str| record(cmd, Some(&parent), &parent, true, 1);

        // A narrower pass never clears the broader failure.
        for (narrow_pass, broad_failure) in [
            ("cargo test --lib", "cargo test"),
            ("cargo test --bins", "cargo test"),
            ("cargo test --doc", "cargo test"),
            ("cargo test --test integration", "cargo test"),
            ("cargo test -p core", "cargo test"),
            ("cargo test -- parser", "cargo test"),
            ("cargo test parser", "cargo test"),
            ("cargo test --lib", "cargo test --lib --bins"),
            ("cargo test --lib", "cargo test --doc"),
            ("cargo test -- --exact parser", "cargo test parser"),
            ("cargo test", "cargo test -p other"),
            ("cargo test", "cargo test --release"),
            ("cargo test", "cargo test -- --ignored"),
            ("cargo test", "cargo test --features extra"),
            ("cargo check --lib", "cargo check"),
            ("cargo check", "cargo check --tests"),
            ("cargo clippy", "cargo clippy -- -D warnings"),
            ("pytest --lf", "pytest"),
        ] {
            assert!(
                !pass(narrow_pass).clears(&fail(broad_failure)),
                "passing `{narrow_pass}` must NOT clear failing `{broad_failure}`"
            );
        }

        // The unrestricted run (or a workspace-wide one) still clears the
        // narrower failures it covers, and an identical run clears its own.
        for (broad_pass, narrow_failure) in [
            ("cargo test", "cargo test --lib"),
            ("cargo test", "cargo test --doc"),
            ("cargo test", "cargo test --test integration"),
            ("cargo test", "cargo test -- parser"),
            ("cargo test 2>&1", "cargo test --lib -- --exact parser"),
            ("cargo test --workspace", "cargo test --lib"),
            ("cargo test -p core", "cargo test -p core --lib"),
            ("cargo test --lib", "cargo test --lib 2>&1"),
            ("cargo check --all-targets", "cargo check"),
            ("pytest", "pytest --lf"),
        ] {
            assert!(
                pass(broad_pass).clears(&fail(narrow_failure)),
                "passing `{broad_pass}` must clear failing `{narrow_failure}`"
            );
        }
    }

    #[test]
    fn ledger_keeps_full_suite_failure_after_lib_only_pass() {
        let (_tmp, parent, _py) = nested();
        let mut ledger = VerificationLedger::default();
        ledger.record(record("cargo test", Some(&parent), &parent, false, 1));
        ledger.record(record("cargo test --lib", Some(&parent), &parent, true, 1));
        assert!(
            ledger.blocking(&parent, 1).is_some(),
            "a lib-only pass must leave the full-suite failure outstanding"
        );
        ledger.record(record("cargo test", Some(&parent), &parent, true, 1));
        assert!(ledger.is_empty(), "the full-suite pass clears it");
    }
}

/// Boolean flags of non-cargo runners that select a SUBSET of the suite
/// (`pytest --lf` reruns only the last failures, `go test -short` skips long
/// tests). Kept in the check identity so a subset pass cannot clear a
/// full-suite failure; every other dash flag is dropped as before.
const SUBSET_BOOL_FLAGS: &[&str] = &[
    "--lf",
    "--last-failed",
    "--sw",
    "--stepwise",
    "--stepwise-skip",
    "-short",
    "--onlychanged",
    "--only-changed",
    "--changed",
];

/// Is `word` a bare redirection operator whose TARGET is the next word
/// (`>`, `>>`, `2>`, `&>`, `<`)? Its target is not a test selector.
fn is_bare_redirect_operator(word: &str) -> bool {
    (word.ends_with('>') || word.ends_with('<'))
        && word
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '&' | '>' | '<'))
}

/// The words of the segment that actually runs the check: leading setup
/// segments (`cd sub && …`, `export X=1; …`) are skipped, as are leading
/// `sudo`/`env` wrappers, and a path prefix on the program is removed.
/// Leading `NAME=value` assignments are returned separately. Redirections and
/// their targets are removed (`2>&1`, `> out.log`).
fn check_segment_words(text: &str) -> (Vec<&str>, Vec<&str>) {
    let all: Vec<&str> = text.split_whitespace().collect();
    let segments = all.split(|w| matches!(*w, "&&" | "||" | ";" | "|"));
    let mut chosen: &[&str] = &[];
    for segment in segments {
        let first = segment.first().copied().unwrap_or("");
        if segment.is_empty()
            || matches!(
                first,
                "cd" | "pushd"
                    | "popd"
                    | "export"
                    | "source"
                    | "."
                    | "set"
                    | "unset"
                    | "true"
                    | ":"
            )
        {
            continue;
        }
        chosen = segment;
        break;
    }
    let mut assignments = Vec::new();
    let mut words = Vec::new();
    let mut skip_next = false;
    for (i, word) in chosen.iter().copied().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if words.is_empty() {
            if matches!(word, "sudo" | "env") {
                continue;
            }
            if word.split_once('=').is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            }) {
                assignments.push(word);
                continue;
            }
            let program = word
                .rsplit('/')
                .next()
                .filter(|p| !p.is_empty())
                .unwrap_or(word);
            words.push(program);
            continue;
        }
        if is_bare_redirect_operator(word) {
            skip_next = i + 1 < chosen.len();
            continue;
        }
        if word.contains('>') || word.contains('<') {
            continue;
        }
        words.push(word);
    }
    (words, assignments)
}

/// The value of a flag: attached (`--flag=v`, `-pv`) or the next word.
fn flag_value<'a>(inline: Option<&'a str>, rest: &mut impl Iterator<Item = &'a str>) -> &'a str {
    inline.or_else(|| rest.next()).unwrap_or("")
}

/// The selection a cargo invocation makes, parsed into the parts that decide
/// whether one run covers another.
#[derive(Debug, Default, PartialEq, Eq)]
struct CargoCheckShape {
    /// `test`, `check`, `clippy`, `build`, …
    sub: String,
    /// Flags that change WHAT is built or run in a way that is not a plain
    /// narrowing of the default run: `--package=x`, `--features=f`,
    /// `--release`, `--workspace`, `--ignored`, toolchain `+nightly`, build
    /// env such as `RUSTFLAGS=…`, and any unrecognized flag (fail-closed).
    scope: std::collections::BTreeSet<String>,
    /// Target selections that narrow the default run: `--lib`, `--bins`,
    /// `--bin=x`, `--doc`, `--tests`, `--test=x` (test only), plus libtest's
    /// `--exact` / `--skip=x`.
    narrowing: std::collections::BTreeSet<String>,
    /// Test-name filters (`cargo test foo`, `cargo test -- foo bar`).
    filters: std::collections::BTreeSet<String>,
}

impl CargoCheckShape {
    /// Parse the words after `cargo` (wrappers/assignments already split off).
    fn parse(args: &[&str], assignments: &[&str]) -> Option<Self> {
        const DROP_ENV: &[&str] = &[
            "RUST_BACKTRACE",
            "RUST_LOG",
            "CARGO_TERM_COLOR",
            "NO_COLOR",
            "TERM",
            "CARGO_INCREMENTAL",
        ];
        let mut shape = CargoCheckShape::default();
        for a in assignments {
            let name = a.split('=').next().unwrap_or("");
            if !DROP_ENV.contains(&name) {
                shape.scope.insert((*a).to_string());
            }
        }
        let mut iter = args.iter().copied().peekable();
        // Toolchain selector and global flags before the subcommand.
        while let Some(word) = iter.peek().copied() {
            if word.starts_with('+') {
                shape.scope.insert(word.to_string());
                iter.next();
            } else if matches!(word, "-q" | "--quiet" | "-v" | "-vv" | "--verbose") {
                iter.next();
            } else {
                break;
            }
        }
        shape.sub = iter.next()?.to_string();
        if shape.sub.starts_with('-') {
            return None;
        }
        let is_test = shape.sub == "test";
        let mut after_dashdash = false;
        while let Some(word) = iter.next() {
            if word == "--" && !after_dashdash {
                after_dashdash = true;
                continue;
            }
            if after_dashdash && !is_test {
                // `cargo clippy -- -D warnings`: lint configuration is part of
                // what the check asserts, kept verbatim.
                shape.scope.insert(format!("-- {word}"));
                continue;
            }
            if !word.starts_with('-') || word == "-" {
                if is_test {
                    shape.filters.insert(word.to_string());
                } else {
                    shape.scope.insert(word.to_string());
                }
                continue;
            }
            let (flag, inline_value) = match word.split_once('=') {
                Some((f, v)) => (f, Some(v)),
                None => (word, None),
            };
            if after_dashdash {
                // libtest arguments.
                match flag {
                    "--exact" => {
                        shape.narrowing.insert("--exact".into());
                    }
                    "--skip" => {
                        let v = flag_value(inline_value, &mut iter);
                        shape.narrowing.insert(format!("--skip={v}"));
                    }
                    "--nocapture" | "--show-output" | "-q" | "--quiet" | "--report-time" => {}
                    "--test-threads" | "--color" | "--format" | "-Z" => {
                        let _ = flag_value(inline_value, &mut iter);
                    }
                    other => {
                        // `--ignored` / `--include-ignored` / unknown: a
                        // different selection, never a narrowing.
                        shape.scope.insert(format!("-- {other}"));
                    }
                }
                continue;
            }
            // Short-flag spellings with the value attached: `-pfoo`, `-j4`.
            let (flag, inline_value) = if flag.len() > 2
                && !flag.starts_with("--")
                && inline_value.is_none()
                && flag.is_char_boundary(2)
                && matches!(&flag[..2], "-p" | "-j" | "-F" | "-Z")
            {
                (&flag[..2], Some(&flag[2..]))
            } else {
                (flag, inline_value)
            };
            let take_value = |iter: &mut _| flag_value(inline_value, iter);
            match flag {
                // Output / parallelism only.
                "-q" | "--quiet" | "-v" | "-vv" | "--verbose" | "--no-fail-fast" | "--locked"
                | "--offline" | "--frozen" | "--timings" | "--keep-going" => {}
                "-j" | "--jobs" | "--color" | "--message-format" | "--target-dir" => {
                    let _ = take_value(&mut iter);
                }
                "--lib" | "--bins" => {
                    shape.narrowing.insert(flag.to_string());
                }
                "--doc" | "--tests" if is_test => {
                    shape.narrowing.insert(flag.to_string());
                }
                "--bin" => {
                    let v = take_value(&mut iter);
                    shape.narrowing.insert(format!("--bin={v}"));
                }
                "--test" if is_test => {
                    let v = take_value(&mut iter);
                    shape.narrowing.insert(format!("--test={v}"));
                }
                "-p" | "--package" | "-F" | "--features" | "--test" | "--example" | "--bench"
                | "--exclude" | "--manifest-path" | "--target" | "--profile" | "--config" => {
                    let canonical = match flag {
                        "-p" => "--package",
                        "-F" => "--features",
                        other => other,
                    };
                    let v = take_value(&mut iter);
                    shape.scope.insert(format!("{canonical}={v}"));
                }
                "--all" => {
                    shape.scope.insert("--workspace".into());
                }
                other => {
                    // --release, --all-features, --no-default-features,
                    // --workspace, --all-targets, --examples, --benches, and
                    // anything unrecognized: a different selection.
                    shape.scope.insert(other.to_string());
                }
            }
        }
        Some(shape)
    }

    /// Canonical, runnable rendering — the check identity.
    fn render(&self) -> String {
        let mut out = format!("cargo {}", self.sub);
        let (cargo_scope, libtest_scope): (Vec<&String>, Vec<&String>) =
            self.scope.iter().partition(|s| !s.starts_with("-- "));
        let (cargo_narrow, libtest_narrow): (Vec<&String>, Vec<&String>) = self
            .narrowing
            .iter()
            .partition(|s| !(s.as_str() == "--exact" || s.starts_with("--skip=")));
        for token in cargo_scope.iter().chain(cargo_narrow.iter()) {
            out.push(' ');
            out.push_str(token);
        }
        let mut filters = self.filters.iter();
        let tail_needed =
            !libtest_scope.is_empty() || !libtest_narrow.is_empty() || self.filters.len() > 1;
        if !tail_needed {
            if let Some(f) = filters.next() {
                out.push(' ');
                out.push_str(f);
            }
            return out;
        }
        out.push_str(" --");
        for token in &libtest_scope {
            out.push(' ');
            out.push_str(token.trim_start_matches("-- "));
        }
        for token in libtest_narrow
            .iter()
            .map(|s| s.as_str())
            .chain(filters.map(String::as_str))
        {
            out.push(' ');
            out.push_str(token);
        }
        out
    }

    /// Flags a pass may carry beyond the failure's and still cover it: the
    /// whole workspace covers the default members, and for non-test
    /// subcommands `--all-targets` covers the default lib+bins selection
    /// (NOT for `cargo test`, where `--all-targets` skips doc tests).
    fn is_broadening(&self, token: &str) -> bool {
        match token {
            "--workspace" => true,
            "--all-targets" => self.sub != "test",
            "-- --include-ignored" => self.sub == "test",
            _ => false,
        }
    }

    /// Does a PASSING run of `self` cover everything `failed` ran?
    ///
    /// Only when both run the same subcommand, the pass's selection scope
    /// equals the failure's (or broadens it by workspace/all-targets), and
    /// either the selections are identical or the pass is UNRESTRICTED (no
    /// target narrowing, no filters). A narrowed pass (`--lib`, `--doc`,
    /// `-- foo`) covers only its identical failure: target flags and filters
    /// combine as unions, so no narrowed run provably covers another.
    fn covers(&self, failed: &CargoCheckShape) -> bool {
        if self.sub != failed.sub {
            return false;
        }
        if !failed.scope.is_subset(&self.scope) {
            return false;
        }
        if !self
            .scope
            .difference(&failed.scope)
            .all(|token| self.is_broadening(token))
        {
            return false;
        }
        let unrestricted = self.narrowing.is_empty() && self.filters.is_empty();
        unrestricted || (self.narrowing == failed.narrowing && self.filters == failed.filters)
    }
}

/// Parse a check identity (or command) as a cargo invocation.
fn cargo_shape(check: &str) -> Option<CargoCheckShape> {
    let (words, assignments) = check_segment_words(check.trim());
    match words.split_first() {
        Some((&"cargo", rest)) => CargoCheckShape::parse(rest, &assignments),
        _ => None,
    }
}

/// Normalise a command to the CHECK it performs.
///
/// `cargo test -- --nocapture` and `cargo test 2>&1` are the same check as
/// `cargo test`; `cargo check` is a different one. Output-only flags and
/// redirections are dropped, the subcommand is kept — that is the
/// distinction a single failure slot lost.
///
/// Subset selection IS the check: `cargo test --lib`, `--doc`, `--test x`,
/// `-p x`, `cargo test foo` and `cargo test -- foo` each keep their
/// selectors (review finding: `cargo test --lib` normalised to `cargo test`
/// and a passing lib-only run cleared a failing full-suite result). Whether
/// one identity's pass covers another's failure is decided by
/// [`VerificationRecord::clears`].
pub fn check_id_for(tool: &str, command: &str) -> String {
    let text = command.trim();
    if text.is_empty() {
        return match tool {
            "cargo_test" => "cargo test".to_string(),
            "cargo_check" => "cargo check".to_string(),
            "cargo_clippy" => "cargo clippy".to_string(),
            "cargo_fmt" => "cargo fmt".to_string(),
            other => other.to_string(),
        };
    }
    if let Some(shape) = cargo_shape(text) {
        return shape.render();
    }
    let (segment_words, _assignments) = check_segment_words(text);
    let words: Vec<&str> = segment_words
        .into_iter()
        .filter(|w| !w.starts_with('-') || SUBSET_BOOL_FLAGS.contains(w))
        .collect();
    match words.as_slice() {
        [] => match tool {
            "cargo_test" => "cargo test".to_string(),
            "cargo_check" => "cargo check".to_string(),
            "cargo_clippy" => "cargo clippy".to_string(),
            "cargo_fmt" => "cargo fmt".to_string(),
            other => other.to_string(),
        },
        // `python3 -m pytest` / `python3 -m unittest`: the module is the check,
        // and it is behind a flag the filter above dropped.
        [interp, rest @ ..] if interp.starts_with("python") || *interp == "node" => {
            let has_m = text.split_whitespace().any(|w| w == "-m");
            let (module, selectors) = if has_m {
                let m = text
                    .split_whitespace()
                    .skip_while(|w| *w != "-m")
                    .nth(1)
                    .unwrap_or("script");
                let sel = rest.iter().copied().filter(|w| *w != m).collect::<Vec<_>>();
                (m, sel)
            } else {
                let m = rest.first().copied().unwrap_or("script");
                let sel = if rest.len() > 1 {
                    rest[1..].to_vec()
                } else {
                    Vec::new()
                };
                (m, sel)
            };
            if selectors.is_empty() {
                format!("{interp} {module}")
            } else {
                format!("{interp} {module} {}", selectors.join(" "))
            }
        }
        [single] => (*single).to_string(),
        // `cargo test`, `npm run`, `go build`.
        [first, second, rest @ ..] => {
            if rest.is_empty() {
                format!("{first} {second}")
            } else {
                format!("{first} {second} {}", rest.join(" "))
            }
        }
    }
}

/// Outstanding verification failures, one per check identity.
///
/// Replaces a single `Option<VerificationRecord>` slot. With one slot a second
/// failure overwrote the first, so a red `cargo test` followed by a red
/// `pytest` left only one of them, and whichever was dropped could never be
/// cleared or reported. Both the automatic post-edit path and the explicit
/// tool-dispatch path write here, so the two cannot drift apart.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VerificationLedger {
    outstanding: Vec<VerificationRecord>,
}

impl VerificationLedger {
    /// Record an outcome. A failure replaces the prior result for that same
    /// check; a pass clears only what it actually covers. A failure whose
    /// runner does not exist (a cargo command with no manifest anywhere) is
    /// classified as no-runner, NOT as a failure: the suite never executed,
    /// so nothing is recorded. Retaining it reproduced the Python-task wall —
    /// the meaningless cargo failure blocked completion and a later passing
    /// unittest could not discharge it under its different check identity.
    pub fn record(&mut self, record: VerificationRecord) {
        if !record.passed && record.runner_is_missing() {
            tracing::debug!(
                "verification ledger: {:?} could not run (no runner/project exists) — not recorded as a failure",
                record.check_id
            );
            return;
        }
        if record.passed {
            self.outstanding.retain(|failed| !record.clears(failed));
        } else {
            self.outstanding
                .retain(|prior| !prior.same_check_as(&record));
            self.outstanding.push(record);
        }
    }

    /// The first outstanding failure that should block a task rooted here.
    pub fn blocking(
        &self,
        task_root: &Path,
        current_mutation_sequence: usize,
    ) -> Option<&VerificationRecord> {
        self.outstanding
            .iter()
            .find(|record| record.blocks_completion(task_root, current_mutation_sequence))
    }

    /// Failures that are real but concern a project outside this task — worth
    /// reporting, never worth blocking on.
    pub fn out_of_scope(&self, task_root: &Path) -> Vec<&VerificationRecord> {
        self.outstanding
            .iter()
            .filter(|record| record.relevance_to(task_root) == Relevance::OutOfScope)
            .collect()
    }

    pub fn outstanding(&self) -> &[VerificationRecord] {
        &self.outstanding
    }

    pub fn is_empty(&self) -> bool {
        self.outstanding.is_empty()
    }

    pub fn clear(&mut self) {
        self.outstanding.clear();
    }
}
