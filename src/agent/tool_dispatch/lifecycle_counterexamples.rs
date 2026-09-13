//! Classification counterexamples, and the lifecycle tests that check the
//! dispatcher actually acts on them.
//!
//! Four defects reached live container runs after unit assertions passed,
//! because the units checked classification in isolation while the damage was
//! done downstream — a read-only command marked mutating advances
//! `mutation_sequence`, which makes the verification that just passed look
//! stale, which blocks a correct completion.
//!
//! The first half of this file is the classifier half. It is worth having, but
//! an earlier revision of it claimed to be more: a helper named
//! `advances_mutation_sequence` whose body was `tool_call_is_mutating(..)`.
//! That asserts what a function RETURNS, not what the agent DOES with the
//! answer. Every one of those tests would pass against a dispatcher that
//! computed the classification and threw it away, never incremented a counter,
//! and never consulted the gate — which is the defect class they were written
//! to catch.
//!
//! The second half (`agent_lifecycle`) builds a real `Agent`, drives real tool
//! calls through the real dispatch accounting, and asserts on the counters, the
//! completion gate's actual verdict, and what survives a checkpoint round trip.

use super::helpers::{
    has_file_redirect, shell_command_is_observational, shell_command_is_verification,
};
use super::tool_call_is_mutating;
use serde_json::json;

fn shell(command: &str) -> serde_json::Value {
    json!({ "command": command })
}

/// Classification only. Named for what it checks: whether the dispatcher would
/// SEE this command as an edit. Whether it then acts on that is the subject of
/// the `agent_lifecycle` tests below, which are the ones that can fail when the
/// accounting breaks.
fn classified_as_mutating(command: &str) -> bool {
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
            !classified_as_mutating(command),
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
            !classified_as_mutating(command),
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
            classified_as_mutating(command),
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
            classified_as_mutating(command),
            "`{command}` may have written; it must advance the sequence"
        );
    }
}

#[test]
fn a_background_job_is_a_separate_command_and_a_descriptor_is_not() {
    // A bare `&` was never a separator, because splitting on it would have
    // wrecked `2>&1`. The cost was that a read-only prefix vouched for whatever
    // followed the `&`.
    for command in [
        "ls & python3 fix.py",
        "cat notes.txt & sed -i s/a/b/ src/a.rs",
        "pwd & rm -rf build",
    ] {
        assert!(
            classified_as_mutating(command),
            "`{command}` backgrounds one job and runs another that writes; the \
             read-only half must not vouch for the other"
        );
    }

    // The reason the naive fix was wrong. These must stay single commands.
    for command in [
        "cargo test 2>&1",
        "python3 -m unittest 2>&1",
        "grep -r needle src 2>&1",
    ] {
        assert!(
            !classified_as_mutating(command),
            "`2>&1` duplicates a descriptor and writes no file; `{command}` \
             must not read as an edit"
        );
    }
}

#[test]
fn a_read_only_verb_with_a_mutating_option_is_still_an_edit() {
    // `find` is on the read-only prefix list, and these options write anyway.
    // Phi's observer already rejected them; the dispatcher accepted them, so
    // the same command was an edit to one and not to the other.
    for command in [
        "find . -name '*.tmp' -delete",
        "find . -name '*.pyc' -exec rm {} ;",
        "find build -type f -execdir rm {} +",
    ] {
        assert!(
            classified_as_mutating(command),
            "`{command}` deletes files despite starting with a read-only verb"
        );
        assert!(
            crate::phi::observer::command_may_mutate(command),
            "`{command}` must read the same way to Phi as to the dispatcher"
        );
    }

    // A plain search still must not.
    for command in ["find . -name '*.rs'", "find src -type d"] {
        assert!(
            !classified_as_mutating(command),
            "`{command}` only searches"
        );
        assert!(
            !crate::phi::observer::command_may_mutate(command),
            "`{command}` must not inflate Phi's unrecorded-mutation count"
        );
    }
}

#[test]
fn the_dispatcher_and_phi_split_commands_the_same_way() {
    // They kept separate splitters, and the second was the naive one the first
    // had already been fixed for: a pipe inside a quoted argument.
    let quoted_pipe = "jq '.nodes | length' graph.json";
    assert_eq!(
        super::helpers::split_shell_segments(quoted_pipe).len(),
        1,
        "a pipe inside quotes is an argument, not an operator"
    );
    assert!(
        !crate::phi::observer::command_may_mutate(quoted_pipe),
        "`{quoted_pipe}` reads a file and prints a number"
    );

    for command in [
        "cargo build && ./run.sh",
        "make; make install",
        "cat f | tee out.txt",
    ] {
        assert!(
            super::helpers::split_shell_segments(command).len() > 1,
            "`{command}` is a compound command and must split"
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

/// Tests that drive a real `Agent` through the real dispatch accounting.
///
/// Each one fails if the dispatcher stops acting on a classification, if the
/// completion gate stops consulting the ledger, or if a checkpoint drops the
/// evidence — none of which the classifier tests above can detect.
#[cfg(test)]
mod agent_lifecycle {
    use crate::agent::Agent;
    use crate::checkpoint::TaskCheckpoint;
    use crate::config::Config;
    use serde_json::json;

    fn gate_config() -> Config {
        let mut config = Config::default();
        config.agent.min_completion_steps = 0;
        config.agent.require_verification_before_completion = true;
        config
    }

    /// An agent rooted in a scratch directory, so scope resolution does not
    /// depend on the repository the test happens to run in.
    async fn agent() -> (Agent, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut agent = Agent::new(gate_config()).await.expect("agent should build");
        agent.current_checkpoint = Some(TaskCheckpoint::new(
            "lifecycle".to_string(),
            "fix the failing test".to_string(),
        ));
        agent.current_task_context = "fix the failing test in calculator.py".to_string();
        agent.last_assistant_response = "Done.".to_string();
        agent.task_verification_root = Some(dir.path().to_path_buf());
        (agent, dir)
    }

    /// The gate's verdict, narrowed to the two refusals this module is about.
    ///
    /// `check_completion_gate` also enforces gates keyed to the ambient
    /// repository — a tainted verifier, an empty diff — which fire on whatever
    /// the working tree happens to contain while these tests run. Asserting
    /// "no refusal at all" would make the suite pass or fail on unrelated
    /// uncommitted changes, and asserting "some refusal" would let a test pass
    /// for a reason it never meant to check.
    async fn verification_refusal(agent: &mut Agent) -> Option<String> {
        agent
            .check_completion_gate()
            .await
            .filter(|r| r.contains("FailingTestsAccepted") || r.contains("StaleVerification"))
    }

    /// Put a tool call through the same function both dispatch sites call.
    fn dispatch(agent: &mut Agent, tool: &str, args: serde_json::Value, success: bool) {
        let args_str = args.to_string();
        agent.note_tool_call_lifecycle(tool, &args, &args_str, success, "ok");
    }

    fn shell(agent: &mut Agent, command: &str, success: bool) {
        dispatch(agent, "shell_exec", json!({ "command": command }), success);
    }

    #[tokio::test]
    async fn an_edit_advances_the_sequence_and_a_test_run_does_not() {
        let (mut agent, _dir) = agent().await;
        assert_eq!(agent.mutation_sequence, 0);

        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "calculator.py", "content": "def add(a, b): return a + b\n" }),
            true,
        );
        assert_eq!(
            agent.mutation_sequence, 1,
            "an edit must advance the sequence — the classifier saying `mutating` \
             is worth nothing if the dispatcher does not act on it"
        );

        shell(&mut agent, "python3 -m unittest", true);
        assert_eq!(
            agent.mutation_sequence, 1,
            "running the suite is not an edit; if this advances, the verification \
             that just passed is instantly stale and the gate blocks a correct task"
        );
        assert_eq!(
            agent.last_successful_verification_mutation_sequence, 1,
            "the passing run must be credited AT the current revision"
        );
    }

    #[tokio::test]
    async fn a_failing_suite_blocks_completion_and_a_passing_rerun_releases_it() {
        let (mut agent, _dir) = agent().await;
        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "calculator.py", "content": "broken\n" }),
            true,
        );
        shell(&mut agent, "python3 -m unittest", false);

        let refusal = verification_refusal(&mut agent).await;
        assert!(
            refusal
                .as_deref()
                .is_some_and(|r| r.contains("FailingTestsAccepted")),
            "a failing suite at the current revision must refuse completion, got {refusal:?}"
        );

        shell(&mut agent, "python3 -m unittest", true);
        let after = verification_refusal(&mut agent).await;
        assert!(
            after.is_none(),
            "the same check passing at the same revision must release the gate, got {after:?}"
        );
    }

    /// The defect finding #3 named: a green compile erasing a red test suite.
    #[tokio::test]
    async fn a_passing_compile_does_not_clear_a_failing_test_suite() {
        let (mut agent, _dir) = agent().await;
        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "src/lib.rs", "content": "fn main() {}\n" }),
            true,
        );
        shell(&mut agent, "cargo test", false);
        shell(&mut agent, "cargo check", true);

        let refusal = verification_refusal(&mut agent).await;
        assert!(
            refusal
                .as_deref()
                .is_some_and(|r| r.contains("FailingTestsAccepted")),
            "`cargo check` passing answers a different question than `cargo test` \
             failing; it must not clear it. Gate said {refusal:?}"
        );
        assert!(
            agent
                .verification_failures
                .outstanding()
                .iter()
                .any(|f| f.check_id == "cargo test"),
            "the failing suite must still be on the ledger"
        );
    }

    /// A second failing check used to overwrite the first in a single slot.
    #[tokio::test]
    async fn two_different_failing_checks_are_both_retained() {
        let (mut agent, _dir) = agent().await;
        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "src/lib.rs", "content": "x\n" }),
            true,
        );
        shell(&mut agent, "cargo test", false);
        shell(&mut agent, "cargo clippy", false);

        let outstanding = agent.verification_failures.outstanding();
        assert_eq!(
            outstanding.len(),
            2,
            "one slot meant the second failure evicted the first, and whichever \
             was dropped could never be reported or cleared: {outstanding:?}"
        );

        shell(&mut agent, "cargo clippy", true);
        let outstanding = agent.verification_failures.outstanding();
        assert_eq!(
            outstanding.len(),
            1,
            "clippy passing clears only clippy: {outstanding:?}"
        );
        assert_eq!(outstanding[0].check_id, "cargo test");
        let refusal = verification_refusal(&mut agent).await;
        assert!(
            refusal.as_deref().is_some_and(|r| r.contains("cargo test")),
            "the surviving test failure must still block, and must name itself \
             rather than the check that passed: {refusal:?}"
        );
    }

    /// Finding #4: the terminal save wrote whatever the last periodic save had
    /// stamped, so a failure recorded after it vanished from the final record.
    #[tokio::test]
    async fn a_failure_after_the_last_periodic_save_survives_the_terminal_save() {
        let (mut agent, _dir) = agent().await;

        // A periodic save stamps the counters as they stand: nothing yet.
        let stamped = agent.to_checkpoint("lifecycle", "fix the failing test");
        agent.current_checkpoint = Some(stamped);
        assert_eq!(
            agent
                .current_checkpoint
                .as_ref()
                .unwrap()
                .guard_counters
                .mutation_sequence,
            0
        );

        // Everything below happens AFTER that save.
        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "calculator.py", "content": "broken\n" }),
            true,
        );
        shell(&mut agent, "pytest", false);

        // Go through the real terminal path. Calling `refresh_persisted_evidence`
        // directly would still pass if `complete_checkpoint` stopped calling it,
        // which is precisely the defect.
        agent.complete_checkpoint().expect("terminal save");
        let counters = &agent.current_checkpoint.as_ref().unwrap().guard_counters;
        assert_eq!(
            counters.mutation_sequence, 1,
            "the edit made after the last periodic save must reach the terminal record"
        );
        assert!(
            !counters.verification_failures.is_empty(),
            "a run that failed verification must not persist a record claiming otherwise"
        );
        assert_eq!(
            counters.verification_failures.outstanding()[0].check_id,
            "pytest"
        );
    }

    /// Finding #4, second half: the summary string survived resume but the
    /// scope did not, so a restored failure could no longer be told apart from
    /// a foreign one.
    #[tokio::test]
    async fn resume_restores_the_scope_of_a_failure_not_just_its_text() {
        let (mut agent, _dir) = agent().await;
        dispatch(
            &mut agent,
            "file_write",
            json!({ "path": "calculator.py", "content": "broken\n" }),
            true,
        );
        shell(&mut agent, "pytest", false);
        agent.complete_checkpoint().expect("terminal save");

        let checkpoint = agent.current_checkpoint.clone().expect("checkpoint");
        let json = serde_json::to_string(&checkpoint).expect("checkpoint serialises");
        let restored: TaskCheckpoint = serde_json::from_str(&json).expect("and deserialises");

        let failures = &restored.guard_counters.verification_failures;
        assert_eq!(failures.outstanding().len(), 1);
        let failure = &failures.outstanding()[0];
        assert_eq!(failure.check_id, "pytest");
        assert!(
            !failure.passed,
            "a restored failure must still read as a failure"
        );
        assert_eq!(
            failure.mutation_sequence, 1,
            "and must carry the revision it concerned, or staleness cannot be judged"
        );
    }
}
