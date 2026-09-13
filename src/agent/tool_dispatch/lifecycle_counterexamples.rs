//! Lifecycle counterexamples: what a command DOES to the agent's state.
//!
//! Four defects reached live container runs after unit assertions passed,
//! because the units checked classification in isolation while the damage was
//! done downstream — a read-only command marked mutating advances
//! `mutation_sequence`, which makes the verification that just passed look
//! stale, which blocks a correct completion.
//!
//! These tests assert the LIFECYCLE consequence, not the classifier's return
//! value, so a future refactor that gets the string right and the accounting
//! wrong still fails.

use super::helpers::{
    has_file_redirect, shell_command_is_observational, shell_command_is_verification,
};
use super::tool_call_is_mutating;
use serde_json::json;

fn shell(command: &str) -> serde_json::Value {
    json!({ "command": command })
}

/// The property every case below is really about: running a check must not
/// look like an edit, because the completion gate compares the two.
fn advances_mutation_sequence(command: &str) -> bool {
    tool_call_is_mutating("shell_exec", &shell(command))
}

#[test]
fn running_tests_does_not_look_like_an_edit() {
    // Finding 1. `read_only_prefixes` carried `python -m pytest` but not
    // `python3 -m unittest`, and the test-script check keyed on a path ending
    // in `.py`, which `-m unittest` is not. Running the suite therefore
    // advanced mutation_sequence; the moment the model passed its tests and
    // declared completion, the gate saw a mutation AFTER the verification and
    // refused with StaleVerification.
    for command in [
        "python3 -m unittest test_calculator.py",
        "python -m unittest discover",
        "python3 -m unittest",
        "python3 -m pytest -q",
        "pytest -q",
        "npm test",
        "cargo test --lib",
        "go test ./...",
    ] {
        assert!(
            !advances_mutation_sequence(command),
            "`{command}` runs tests; treating it as a mutation makes the \
             verification that just passed look stale"
        );
        assert!(
            shell_command_is_verification(command),
            "`{command}` must still count AS verification"
        );
    }
}

#[test]
fn inspecting_the_tree_does_not_look_like_an_edit() {
    // Finding 2. `find . -name 'Cargo.toml' 2>/dev/null` contains `>`, and the
    // redirect check saw a file write. Asking a question about the repository
    // advanced the mutation sequence and blocked completion.
    for command in [
        "find . -name 'Cargo.toml' 2>/dev/null",
        "ls -la 2>/dev/null",
        "grep -r needle src 2>/dev/null",
        "cat calculator.py",
        "git status",
        "pwd",
    ] {
        assert!(
            !has_file_redirect(command),
            "`{command}` writes nothing; 2>/dev/null and friends are not file writes"
        );
        assert!(
            !advances_mutation_sequence(command),
            "`{command}` only inspects, so it must not advance the mutation sequence"
        );
        assert!(
            shell_command_is_observational(command),
            "`{command}` should read as observational"
        );
    }
}

#[test]
fn a_real_write_still_looks_like_an_edit() {
    // The asymmetry that makes the two tests above safe. If these stopped
    // counting, a genuine edit could slip past the staleness check entirely,
    // which is the far more dangerous direction.
    for command in [
        "echo hi > note.txt",
        "printf 'x' >> log.txt",
        "sed -i s/a/b/ src/a.rs",
        "rm -f scratch.txt",
        "mv a.py b.py",
    ] {
        assert!(
            advances_mutation_sequence(command),
            "`{command}` changes the tree and must advance the mutation sequence"
        );
    }
    assert!(has_file_redirect("echo hi > note.txt"));
    assert!(has_file_redirect("cat a >> b"));
}

#[test]
fn a_command_that_both_tests_and_writes_counts_as_a_mutation() {
    // Uncertainty resolves toward "an edit happened": missing a write is worse
    // than an extra staleness check.
    for command in [
        "python3 fix.py && python3 -m unittest",
        "pytest > results.txt",
    ] {
        assert!(
            advances_mutation_sequence(command),
            "`{command}` may have written; it must advance the sequence"
        );
    }
}

#[test]
fn file_delete_on_an_absent_path_is_not_a_hard_failure() {
    // Finding 4. The FILES: checklist guard blocked file_delete, the model fell
    // back to `rm -f`, and the subsequent file_delete returned "File not found"
    // — which retry-suppression escalated into a hard block. Removing something
    // already absent achieved the requested end state.
    let tool = crate::tools::file::FileDelete::default();
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("already-gone.txt");
    let args = json!({ "path": missing.to_string_lossy() });

    let result = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(async { crate::tools::Tool::execute(&tool, args).await });

    let text = format!("{result:?}");
    assert!(
        !text.to_lowercase().contains("file not found"),
        "deleting an absent path must be idempotent, not an error: {text}"
    );
}
