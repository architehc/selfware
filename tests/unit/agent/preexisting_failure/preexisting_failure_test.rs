use super::*;
use crate::agent::verification_scope::{VerificationLedger, VerificationRecord, VerificationScope};
use std::path::Path;

/// The live c24 `cargo_check` result, abridged: six manifest-level errors in
/// `output`, nothing structured, plus a warning that must not count.
fn c24_cargo_check_result(root: &str) -> String {
    let mut output = String::new();
    for (name, rel) in [
        ("evolve", "tests/evolve/mod.rs"),
        ("integration", "tests/integration/mod.rs"),
        ("unit", "tests/unit/mod.rs"),
    ] {
        output.push_str(&format!(
            "error: can't find integration-test `{name}` at path `{root}/{rel}`\n --> {root}/Cargo.toml\n"
        ));
    }
    serde_json::json!({
        "by_file": {},
        "error_count": 0,
        "errors": [],
        "warnings": [{"message": "unused import", "file": "src/lib.rs", "line": 3,
                       "severity": "warning", "code": null, "column": 1, "snippet": "",
                       "suggestion": null}],
        "exit_code": 101,
        "first_error": null,
        "output": output,
        "success": false
    })
    .to_string()
}

fn counts(lines: &[String], root: &Path) -> DiagnosticCounts {
    normalized_counts(lines, &root_spellings(&[root]))
}

#[test]
fn c24_manifest_errors_are_extracted_and_warnings_are_not() {
    let lines = diagnostic_lines(&c24_cargo_check_result("/ws"));
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines
        .iter()
        .all(|l| l.starts_with("error: can't find integration-test")));
    assert!(
        lines[0].ends_with("@ /ws/Cargo.toml"),
        "the --> location is attached: {}",
        lines[0]
    );
    assert!(!lines.iter().any(|l| l.contains("unused import")));
}

#[test]
fn the_same_errors_under_two_different_roots_are_pre_existing() {
    let live = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir().unwrap();
    let now = counts(
        &diagnostic_lines(&c24_cargo_check_result(&live.path().to_string_lossy())),
        live.path(),
    );
    let before = counts(
        &diagnostic_lines(&c24_cargo_check_result(&base.path().to_string_lossy())),
        base.path(),
    );
    assert_eq!(now, before, "roots normalise away");
    let attribution = attribute(
        &now,
        &BaselineRun::Ran {
            passed: false,
            diagnostics: before,
        },
    );
    assert!(attribution.is_preexisting(), "{attribution:?}");
    assert!(!attribution.block_is_capped());
}

#[test]
fn shifted_line_numbers_do_not_make_an_old_error_new() {
    let before = vec!["error[E0308]: mismatched types @ src/lib.rs:3:22".to_string()];
    let now = vec!["error[E0308]: mismatched types @ src/lib.rs:4:22".to_string()];
    let a = attribute(
        &normalized_counts(&now, &[]),
        &BaselineRun::Ran {
            passed: false,
            diagnostics: normalized_counts(&before, &[]),
        },
    );
    assert!(a.is_preexisting(), "{a:?}");
}

#[test]
fn a_second_occurrence_of_the_same_error_is_new() {
    // Same message, same file — only the count tells them apart once line
    // numbers are normalised.
    let before = vec!["error[E0308]: mismatched types @ src/lib.rs:3:22".to_string()];
    let now = vec![
        "error[E0308]: mismatched types @ src/lib.rs:3:22".to_string(),
        "error[E0308]: mismatched types @ src/lib.rs:7:22".to_string(),
    ];
    let a = attribute(
        &normalized_counts(&now, &[]),
        &BaselineRun::Ran {
            passed: false,
            diagnostics: normalized_counts(&before, &[]),
        },
    );
    assert!(matches!(a, Attribution::New { .. }), "{a:?}");
    assert!(!a.block_is_capped(), "a new error is the model's to fix");
    assert!(a.gate_note().contains("caused by the task's changes"));
}

#[test]
fn a_pass_on_the_pre_task_tree_makes_every_error_new() {
    let now = normalized_counts(&["error: boom".to_string()], &[]);
    let a = attribute(
        &now,
        &BaselineRun::Ran {
            passed: true,
            diagnostics: DiagnosticCounts::new(),
        },
    );
    assert!(matches!(a, Attribution::New { .. }), "{a:?}");
}

#[test]
fn disjoint_errors_and_unavailable_baselines_are_capped_not_waved_through() {
    let now = normalized_counts(&["error: now".to_string()], &[]);
    let different = attribute(
        &now,
        &BaselineRun::Ran {
            passed: false,
            diagnostics: normalized_counts(&["error: before".to_string()], &[]),
        },
    );
    assert!(matches!(different, Attribution::Different { .. }));
    assert!(different.block_is_capped());
    assert!(!different.is_preexisting());

    let unknown = attribute(&now, &BaselineRun::Unavailable("no snapshot".to_string()));
    assert!(unknown.block_is_capped());
    assert!(unknown
        .gate_note()
        .contains("could not tell whether this failure already existed"));

    let empty = attribute(
        &DiagnosticCounts::new(),
        &BaselineRun::Ran {
            passed: false,
            diagnostics: now.clone(),
        },
    );
    assert!(matches!(empty, Attribution::Unknown { .. }), "{empty:?}");
}

#[test]
fn diagnostics_cover_python_node_go_and_libtest_shapes() {
    let text = "\
tests/test_a.py::test_x FAILED
FAILED tests/test_a.py::test_x - AssertionError: 1 != 2
ERROR tests/test_b.py - ModuleNotFoundError: No module named 'zz'
src/app.ts(3,5): error TS2322: Type 'string' is not assignable to type 'number'.
pkg/x.py:12: error: Incompatible return value type
--- FAIL: TestAdd (0.00s)
FAIL\texample.com/pkg\t0.012s
not ok 2 - adds numbers
test tests::it_works ... FAILED
SyntaxError: invalid syntax
warning: unused variable `x`
ok 1 - passes";
    let lines = diagnostic_lines(text);
    for expected in [
        "FAILED tests/test_a.py::test_x",
        "ERROR tests/test_b.py",
        "src/app.ts(3,5): error TS2322: Type 'string' is not assignable to type 'number'.",
        "pkg/x.py:12: error: Incompatible return value type",
        "--- FAIL: TestAdd (0.00s)",
        "not ok 2 - adds numbers",
        "test tests::it_works ... FAILED",
        "SyntaxError: invalid syntax",
    ] {
        assert!(
            lines.iter().any(|l| l == expected),
            "missing {expected:?} in {lines:?}"
        );
    }
    assert!(!lines.iter().any(|l| l.contains("unused variable")));
    assert!(!lines.iter().any(|l| l.starts_with("ok ")));
    // Durations and ids are digits: normalised away.
    assert_eq!(
        normalize_diagnostic("--- FAIL: TestAdd (0.00s)", &[]),
        normalize_diagnostic("--- FAIL: TestAdd (1.25s)", &[])
    );
}

#[test]
fn structured_cargo_errors_are_diagnostics_and_warnings_are_not() {
    let result = serde_json::json!({
        "success": false,
        "errors": [{"message": "mismatched types", "code": "E0308", "file": "src/lib.rs",
                    "line": 2, "column": 5, "severity": "error", "snippet": "x",
                    "suggestion": null}],
        "warnings": [{"message": "unused", "code": null, "file": "src/lib.rs",
                      "line": 1, "column": 1, "severity": "warning", "snippet": "",
                      "suggestion": null}],
        "first_error": {"message": "mismatched types", "code": "E0308", "file": "src/lib.rs",
                        "line": 2, "column": 5, "severity": "error", "snippet": "x",
                        "suggestion": null},
        "output": "error: could not compile `p` (lib) due to 1 previous error\n"
    })
    .to_string();
    let lines = diagnostic_lines(&result);
    assert!(lines.contains(&"error[E0308]: mismatched types @ src/lib.rs".to_string()));
    assert_eq!(
        lines
            .iter()
            .filter(|l| l.contains("mismatched types"))
            .count(),
        1,
        "first_error repeats errors[0] and is skipped: {lines:?}"
    );
    assert!(!lines.iter().any(|l| l.contains("unused")));
}

fn failing_record(seq: usize, root: &Path) -> VerificationRecord {
    VerificationRecord {
        check_id: "cargo check".to_string(),
        command: "cargo_check".to_string(),
        scope: VerificationScope {
            working_dir: root.to_path_buf(),
            project_root: Some(root.to_path_buf()),
            runner_exists: Some(true),
        },
        passed: false,
        mutation_sequence: seq,
        summary: "cargo_check failed: can't find integration-test".to_string(),
        diagnostics: vec!["error: can't find integration-test `unit`".to_string()],
        rerun: Some(RerunSpec::Tool {
            name: "cargo_check".to_string(),
            args: "{}".to_string(),
        }),
        attribution: None,
    }
}

#[test]
fn a_pre_existing_failure_does_not_block_but_stays_outstanding() {
    let dir = tempfile::tempdir().unwrap();
    let mut ledger = VerificationLedger::default();
    ledger.record(failing_record(2, dir.path()));
    assert!(ledger.blocking(dir.path(), 2).is_some(), "unjudged: blocks");
    ledger.set_attribution(
        0,
        Attribution::PreExisting {
            sample: vec!["error: can't find integration-test `unit`".to_string()],
        },
    );
    assert!(
        ledger.blocking(dir.path(), 2).is_none(),
        "pre-existing: not blocking"
    );
    assert_eq!(ledger.preexisting(dir.path(), 2).len(), 1);
    assert!(
        !ledger.is_empty(),
        "still an outstanding failure, never a pass"
    );
    // A later edit makes it stale: no longer a current-revision note.
    assert!(ledger.preexisting(dir.path(), 3).is_empty());

    // New / Unknown attributions keep blocking.
    for attribution in [
        Attribution::New {
            new_errors: vec!["error: x".to_string()],
        },
        Attribution::Unknown {
            reason: "r".to_string(),
        },
    ] {
        let mut ledger = VerificationLedger::default();
        ledger.record(failing_record(2, dir.path()));
        ledger.set_attribution(0, attribution);
        assert!(ledger.blocking(dir.path(), 2).is_some());
    }
}

#[test]
fn preexisting_note_names_the_check_and_that_it_is_not_this_change() {
    let note = preexisting_note(
        "gate:type_check",
        &Attribution::PreExisting {
            sample: vec!["error: can't find integration-test `unit`".to_string()],
        },
    );
    assert!(note.starts_with("`type_check`: failing before the task too (pre-existing: "));
    assert!(note.ends_with("— not caused by this change"), "{note}");
}

#[test]
fn the_record_survives_a_checkpoint_round_trip_and_old_records_still_load() {
    let dir = tempfile::tempdir().unwrap();
    let mut record = failing_record(1, dir.path());
    record.attribution = Some(Attribution::Unknown {
        reason: "timeout".to_string(),
    });
    let text = serde_json::to_string(&record).unwrap();
    let back: VerificationRecord = serde_json::from_str(&text).unwrap();
    assert_eq!(back, record);
    let mut legacy = serde_json::to_value(&record).unwrap();
    for key in ["diagnostics", "rerun", "attribution"] {
        legacy.as_object_mut().unwrap().remove(key);
    }
    let legacy: VerificationRecord = serde_json::from_value(legacy).unwrap();
    assert!(legacy.diagnostics.is_empty() && legacy.rerun.is_none());
    assert!(legacy.attribution.is_none());
}

// --- The task-start tree: capture and checkout -----------------------------

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

#[test]
fn the_task_start_tree_captures_dirty_untracked_and_deleted_files_without_touching_the_index() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    std::fs::write(root.join("kept.txt"), "committed\n").unwrap();
    std::fs::write(root.join("gone.txt"), "committed\n").unwrap();
    std::fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);
    // Task-start state: an uncommitted edit, a deletion, an untracked file,
    // an ignored file, and something already staged.
    std::fs::write(root.join("kept.txt"), "dirty at start\n").unwrap();
    std::fs::remove_file(root.join("gone.txt")).unwrap();
    std::fs::write(root.join("new.txt"), "untracked at start\n").unwrap();
    std::fs::write(root.join("ignored.txt"), "ignored\n").unwrap();
    std::fs::write(root.join("staged.txt"), "staged\n").unwrap();
    git(root, &["add", "staged.txt"]);
    let status_before = git(root, &["status", "--porcelain"]);
    let cached_before = git(root, &["diff", "--cached", "--name-status"]);

    let tree = capture_task_start_tree(root).expect("tree captured");

    assert_eq!(git(root, &["status", "--porcelain"]), status_before);
    assert_eq!(
        git(root, &["diff", "--cached", "--name-status"]),
        cached_before,
        "the user's index is untouched"
    );

    // The task then edits things.
    std::fs::write(root.join("kept.txt"), "edited by the task\n").unwrap();
    std::fs::write(root.join("new.txt"), "edited by the task\n").unwrap();

    let top = repo_toplevel(root).unwrap();
    let checkout = BaselineCheckout::materialize(&top, &tree).unwrap();
    let t = checkout.tree();
    assert_eq!(
        std::fs::read_to_string(t.join("kept.txt")).unwrap(),
        "dirty at start\n"
    );
    assert_eq!(
        std::fs::read_to_string(t.join("new.txt")).unwrap(),
        "untracked at start\n"
    );
    assert_eq!(
        std::fs::read_to_string(t.join("staged.txt")).unwrap(),
        "staged\n"
    );
    assert!(
        !t.join("gone.txt").exists(),
        "deleted at start stays deleted"
    );
    assert!(
        !t.join("ignored.txt").exists(),
        "ignored files are not part of it"
    );
    assert_eq!(
        checkout.map(&root.join("src/x.rs")).unwrap(),
        t.join("src/x.rs")
    );
    let base = t.parent().unwrap().to_path_buf();
    drop(checkout);
    assert!(!base.exists(), "the checkout is removed on drop");
    assert_eq!(
        git(root, &["status", "--porcelain"]).lines().count(),
        status_before.lines().count()
    );
}

#[test]
fn outside_a_repository_there_is_no_task_start_tree() {
    let dir = tempfile::tempdir().unwrap();
    // A bare temp dir may still sit inside some repository on a dev machine;
    // only assert when git agrees it is not one.
    if repo_toplevel(dir.path()).is_none() {
        assert_eq!(capture_task_start_tree(dir.path()), None);
    }
}

// --- Agent-level: the gate, the cap and the verdict ------------------------

async fn agent_in(dir: &Path) -> (crate::agent::Agent, crate::testing::mock_api::MockLlmServer) {
    let server = crate::testing::mock_api::MockLlmServer::builder()
        .with_response("done")
        .build()
        .await;
    let config = crate::test_support::mock_agent_config(&format!("{}/v1", server.url()));
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    agent.task_verification_root = Some(dir.to_path_buf());
    (agent, server)
}

#[tokio::test]
async fn the_same_unattributed_failure_ends_the_run_after_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let (mut agent, server) = agent_in(dir.path()).await;
    agent.mutation_sequence = 1;
    let mut record = failing_record(1, dir.path());
    record.attribution = Some(Attribution::Unknown {
        reason: "no snapshot of the pre-task tree was taken".to_string(),
    });
    agent.verification_failures.record(record);

    agent.loop_control.restore_progress(5, 5);
    agent.note_unattributed_failure_block().unwrap();
    // The same iteration again (a second gate call in one turn) is not a
    // second block.
    agent.note_unattributed_failure_block().unwrap();
    agent.loop_control.restore_progress(6, 6);
    agent.note_unattributed_failure_block().unwrap();
    agent.loop_control.restore_progress(7, 7);
    let err = agent
        .note_unattributed_failure_block()
        .expect_err("third distinct block of the same failure ends the run");
    let msg = err.to_string();
    assert!(msg.contains(UNATTRIBUTED_FAILURE_LOOP_MARKER), "{msg}");
    assert!(msg.contains("could not tell"), "{msg}");
    assert!(
        crate::agent::task_runner::is_fatal_loop_error(&err),
        "never auto-recovered"
    );
    server.stop().await;
}

#[tokio::test]
async fn a_changed_failure_or_a_new_error_resets_the_cap() {
    let dir = tempfile::tempdir().unwrap();
    let (mut agent, server) = agent_in(dir.path()).await;
    agent.mutation_sequence = 1;
    let unknown = |line: &str| {
        let mut r = failing_record(1, dir.path());
        r.diagnostics = vec![line.to_string()];
        r.attribution = Some(Attribution::Unknown {
            reason: "timeout".to_string(),
        });
        r
    };
    agent.verification_failures.record(unknown("error: a"));
    for i in 0..2 {
        agent.loop_control.restore_progress(i, i);
        agent.note_unattributed_failure_block().unwrap();
    }
    // The model changed something: a different error now.
    agent.verification_failures.record(unknown("error: b"));
    for i in 2..4 {
        agent.loop_control.restore_progress(i, i);
        agent.note_unattributed_failure_block().unwrap();
    }
    // A failure the change caused is never capped.
    let mut new = failing_record(1, dir.path());
    new.attribution = Some(Attribution::New {
        new_errors: vec!["error: b".to_string()],
    });
    agent.verification_failures.record(new);
    for i in 4..10 {
        agent.loop_control.restore_progress(i, i);
        agent.note_unattributed_failure_block().unwrap();
    }
    server.stop().await;
}

#[tokio::test]
async fn the_unattributed_failure_loop_is_verification_failed_not_max_iterations() {
    use crate::agent::failure_mode::{FailureKind, FailureMode, RunOutcome};
    let dir = tempfile::tempdir().unwrap();
    let (agent, server) = agent_in(dir.path()).await;
    let fm = FailureMode::classify(
        &agent,
        RunOutcome::Failed {
            reason: format!(
                "{UNATTRIBUTED_FAILURE_LOOP_MARKER}: `cargo check` kept failing unchanged and \
                 blocked completion 3 times."
            ),
        },
    );
    assert_eq!(fm.kind, FailureKind::VerificationFailed);
    assert!(fm.advice.contains("do NOT raise max_iterations"));
    server.stop().await;
}

#[test]
fn a_pre_existing_failure_is_never_a_clean_success_banner() {
    use crate::agent::failure_mode::{with_preexisting_failures, FailureKind, FailureMode};
    let base = FailureMode {
        restored_files: Vec::new(),
        kind: FailureKind::Success,
        evidence: "2 mutating tool calls, completed naturally".to_string(),
        advice: "-".to_string(),
    };
    assert!(base.is_clean_success());
    let note = preexisting_note(
        "cargo check",
        &Attribution::PreExisting {
            sample: vec!["error: can't find integration-test `unit`".to_string()],
        },
    );
    let fm = with_preexisting_failures(base, &[note]);
    assert_eq!(
        fm.kind,
        FailureKind::Success,
        "the change caused no failure"
    );
    assert!(!fm.is_clean_success(), "never ✅ over a failing check");
    assert!(fm.cli_banner().contains("failing before the task too"));
    assert!(fm.evidence.contains("`cargo check`"));
}

// --- Mock-LLM runs reproducing the c24 loop --------------------------------

/// A committed cargo workspace whose `cargo check` already fails before the
/// task: `lib_rs` is the library source, `extra_manifest` is appended to the
/// manifest (the c24 shape declares a `[[test]]` target with no file).
fn prebroken_workspace(dir: &Path, lib_rs: &str, extra_manifest: &str) {
    init_repo(dir);
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"prebroken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [workspace]\n{extra_manifest}"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), lib_rs).unwrap();
    std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "pre-broken"]);
}

async fn scripted_run(
    dir: &Path,
    edit: serde_json::Value,
    answer: &str,
    max_iterations: usize,
) -> (crate::agent::Agent, anyhow::Result<()>, Vec<String>) {
    use crate::testing::mock_api::{MockLlmServer, MockResponse, MockToolCall};
    let server = MockLlmServer::builder()
        // The live shape: read, edit, check, answer.
        .with_tool_calls(vec![MockToolCall {
            id: "read_0".to_string(),
            name: "file_read".to_string(),
            arguments: r#"{"path":"src/lib.rs"}"#.to_string(),
        }])
        .with_tool_calls(vec![MockToolCall {
            id: "edit_0".to_string(),
            name: "file_edit".to_string(),
            arguments: edit.to_string(),
        }])
        .with_tool_calls(vec![MockToolCall {
            id: "check_0".to_string(),
            name: "cargo_check".to_string(),
            arguments: "{}".to_string(),
        }])
        .with_default_response(MockResponse::Text(answer.to_string()))
        .build()
        .await;
    let mut config = crate::test_support::mock_agent_config_with_limits(
        &format!("{}/v1", server.url()),
        32_000,
        8_192,
        max_iterations,
        180,
    );
    config.agent.native_function_calling = true;
    let mut agent = crate::agent::Agent::new(config).await.unwrap();
    let _ = dir;
    let outcome = agent
        .run_task("Add a /// doc comment above pub fn a in src/lib.rs.")
        .await;
    let requests = server.captured_request_bodies().await;
    server.stop().await;
    (agent, outcome, requests)
}

fn cargo_available() -> bool {
    std::process::Command::new(crate::tools::cargo::cargo_program())
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// c24, reproduced: `cargo check` fails before the task (a declared
/// `[[test]]` target has no file). A comments-only edit must complete, and
/// the outcome must say the failure pre-exists — not ✅, not MAX_ITERATIONS.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn c24_mock_run_harmless_edit_on_a_prebroken_workspace_completes_with_a_preexisting_note() {
    if !cargo_available() {
        return;
    }
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    prebroken_workspace(
        dir.path(),
        "pub fn a() -> u32 {\n    1\n}\n",
        "\n[[test]]\nname = \"unit\"\npath = \"tests/unit/mod.rs\"\n",
    );
    let (agent, outcome, requests) = scripted_run(
        dir.path(),
        serde_json::json!({
            "path": "src/lib.rs",
            "old_str": "pub fn a() -> u32 {",
            "new_str": "/// Returns one.\npub fn a() -> u32 {",
        }),
        "Added a /// doc comment above pub fn a in src/lib.rs. `cargo check` fails only \
         because the declared test target tests/unit/mod.rs does not exist.",
        10,
    )
    .await;
    assert!(
        std::fs::read_to_string(dir.path().join("src/lib.rs"))
            .unwrap()
            .contains("/// Returns one."),
        "the edit landed"
    );
    let refusal = requests
        .iter()
        .find(|body| body.contains("FailingTestsAccepted"))
        .map(|body| {
            body[body.find("FailingTestsAccepted").unwrap_or(0)..]
                .chars()
                .take(900)
                .collect::<String>()
        });
    assert!(
        refusal.is_none(),
        "a failure that pre-exists the task must not refuse the answer: {refusal:?}"
    );
    assert!(outcome.is_ok(), "run failed: {:?}", outcome.err());
    let summary = agent.run_summary();
    assert!(
        !summary.preexisting_failures.is_empty(),
        "the outcome names the pre-existing failure: {summary:?}"
    );
    assert!(
        summary
            .preexisting_failures
            .iter()
            .all(|n| n.contains("failing before the task too")),
        "{:?}",
        summary.preexisting_failures
    );
    let fm = agent.last_run_failure_mode.clone().expect("verdict");
    assert!(fm.kind.is_nonfailure(), "{fm:?}");
    assert!(
        !fm.is_clean_success(),
        "never ✅ over a failing check: {fm:?}"
    );
}

/// The other half: on the same kind of pre-broken workspace, an edit that
/// ADDS an error is still refused, and the refusal says the error is new.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn c24_mock_run_edit_adding_a_new_error_is_still_blocked_and_named_new() {
    if !cargo_available() {
        return;
    }
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    // Pre-broken: one type error already there.
    prebroken_workspace(dir.path(), "pub fn a() -> u32 {\n    \"x\"\n}\n", "");
    let (agent, outcome, requests) = scripted_run(
        dir.path(),
        serde_json::json!({
            "path": "src/lib.rs",
            "old_str": "pub fn a() -> u32 {",
            "new_str": "/// Returns one.\npub fn b() -> u32 {\n    \"y\"\n}\n\npub fn a() -> u32 {",
        }),
        "Added a /// doc comment above pub fn a in src/lib.rs.",
        6,
    )
    .await;
    let refusal = requests
        .iter()
        .find(|body| body.contains("FailingTestsAccepted"))
        .expect("the answer over a new error is refused");
    assert!(
        refusal.contains("caused by the task's changes"),
        "the refusal names the error as new: {}",
        refusal[refusal.find("FailingTestsAccepted").unwrap_or(0)..]
            .chars()
            .take(600)
            .collect::<String>()
    );
    assert!(outcome.is_err(), "a new error never completes");
    assert!(agent.run_summary().preexisting_failures.is_empty());
}

/// Without a record of the pre-task tree (not a git repository) the gate
/// cannot tell — it says so, and the same unchanged failure refuses at most
/// MAX_UNATTRIBUTED_FAILURE_BLOCKS times before the run ends as
/// VERIFICATION_FAILED instead of MAX_ITERATIONS.
#[tokio::test]
#[cfg_attr(
    target_os = "windows",
    ignore = "mock TCP server unreliable under heavy parallelism on Windows CI"
)]
async fn c24_mock_run_without_a_baseline_says_so_and_ends_before_the_iteration_cap() {
    if !cargo_available() {
        return;
    }
    let _exec = crate::test_support::ExecGuard::hold();
    let dir = tempfile::tempdir().unwrap();
    if repo_toplevel(dir.path()).is_some() {
        return; // the temp dir sits inside a repository on this machine
    }
    let _cwd = crate::test_support::CwdGuard::enter(dir.path());
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"nogit\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n\
         [[test]]\nname = \"unit\"\npath = \"tests/unit/mod.rs\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn a() -> u32 {\n    1\n}\n",
    )
    .unwrap();
    let (agent, outcome, requests) = scripted_run(
        dir.path(),
        serde_json::json!({
            "path": "src/lib.rs",
            "old_str": "pub fn a() -> u32 {",
            "new_str": "/// Returns one.\npub fn a() -> u32 {",
        }),
        "Added a /// doc comment above pub fn a in src/lib.rs.",
        30,
    )
    .await;
    assert!(
        requests
            .iter()
            .any(|body| body.contains("could not tell whether this failure already existed")),
        "the refusal says it could not tell"
    );
    let err = outcome.expect_err("an unattributable failure still does not complete");
    let fm = agent.last_run_failure_mode.clone().expect("verdict");
    assert_eq!(
        fm.kind,
        crate::agent::failure_mode::FailureKind::VerificationFailed,
        "{err} / {fm:?}"
    );
    assert!(
        agent.loop_control.current_iteration() < 30,
        "ended well before the iteration cap"
    );
}
