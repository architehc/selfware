//! Observer tests, including the sweep that stops a new mutating tool from
//! bypassing obligation creation.

use super::ledger::*;
use super::observer::*;
use serde_json::json;

const T: u64 = 1_700_000_000_000;

fn call<'a>(
    tool: &'a str,
    arguments: &'a serde_json::Value,
    turn_index: usize,
) -> ToolCallRecord<'a> {
    ToolCallRecord {
        tool,
        arguments,
        turn_index,
        succeeded: true,
        output: None,
    }
}

/// A tool record carrying a shell-style result payload.
fn shell_call<'a>(
    tool: &'a str,
    arguments: &'a serde_json::Value,
    output: &'a str,
) -> ToolCallRecord<'a> {
    ToolCallRecord {
        tool,
        arguments,
        turn_index: 1,
        succeeded: true,
        output: Some(output),
    }
}

#[test]
fn every_registry_tool_is_explicitly_classified_or_explicitly_inert() {
    // The previous version read CRITICAL_TOOLS and then filtered it through a
    // handwritten list of six names -- so a new mutation tool with any other
    // name vanished before the assertion. Every registry tool must now be
    // accounted for by name, with no filter in between.
    //
    // `cargo_fmt` is the live example: it rewrites files and is not in
    // MUTATING_TOOLS. It is listed below as a KNOWN uncovered mutation so the
    // gap is recorded rather than implied by omission.
    const KNOWN_INERT: &[&str] = &[
        "file_read",
        "directory_tree",
        "grep_search",
        "glob_find",
        "symbol_search",
        "git_status",
        "git_diff",
        "tool_search",
    ];
    // Mutating, and knowingly not yet observed. Emptying this list is the goal;
    // it exists so the gap cannot be forgotten.
    const KNOWN_UNCOVERED_MUTATIONS: &[&str] = &["cargo_fmt"];

    let mut unaccounted = Vec::new();
    for tool in crate::tools::CRITICAL_TOOLS {
        let known = MUTATING_TOOLS.contains(tool)
            || TEST_EXECUTION_TOOLS.contains(tool)
            || COMPILE_ONLY_TOOLS.contains(tool)
            || SHELL_TOOLS.contains(tool)
            || KNOWN_INERT.contains(tool)
            || KNOWN_UNCOVERED_MUTATIONS.contains(tool);
        if !known {
            unaccounted.push(*tool);
        }
    }
    assert!(
        unaccounted.is_empty(),
        "registry tools the observer neither classifies nor declares inert: {unaccounted:?}. \
         Add each to a list above -- silence means mutations through it create no obligation."
    );
}

#[test]
fn shell_verification_is_recognised_across_languages() {
    // shell_exec was invisible to the ledger: not a mutating tool, not in the
    // verification list, so every pytest / npm test / go test run was
    // unrecorded. Uses the codebase's own pipeline-aware classifier rather than
    // a second heuristic that would drift from it.
    for command in [
        "pytest -q",
        "python3 -m unittest discover",
        "npm test",
        "cargo test --lib",
        "go test ./...",
    ] {
        let args = json!({"command": command});
        let events = classify(&call("shell_exec", &args, 1));
        assert!(
            matches!(events.as_slice(), [ObservedEvent::RunFinished { .. }]),
            "{command} should record a test run, got {events:?}"
        );
    }
}

#[test]
fn an_opaque_shell_command_is_recorded_as_possibly_mutating() {
    // "Not verification" does not mean "changed nothing". A shell command can
    // write files the observer cannot name, and silence would read as no
    // mutation at all.
    let args = json!({"command": "sed -i s/a/b/ src/a.rs"});
    let events = classify(&call("shell_exec", &args, 1));
    match events.as_slice() {
        [ObservedEvent::OpaqueRun {
            may_have_mutated, ..
        }] => assert!(*may_have_mutated),
        other => panic!("expected OpaqueRun, got {other:?}"),
    }
}

#[test]
fn cargo_check_is_not_a_test_run() {
    // It compiles and executes nothing, so it cannot discharge UntestedLogic.
    let check_args = json!({});
    let events = classify(&call("cargo_check", &check_args, 1));
    assert!(
        matches!(events.as_slice(), [ObservedEvent::OpaqueRun { .. }]),
        "cargo_check must not be recorded as test execution, got {events:?}"
    );

    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call(
            "file_write",
            &json!({"path": "src/a.rs", "content": "a"}),
            1,
        )),
        snap,
        T,
    );
    let run_snap = ledger.snapshot();
    apply(&mut ledger, &events, run_snap, T);
    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UntestedLogic),
        1,
        "a successful compile discharges nothing"
    );
}

#[test]
fn a_failed_test_run_is_recorded_not_dropped() {
    // Losing failed runs makes a red session look merely quiet.
    let args = json!({});
    let mut record = call("cargo_test", &args, 1);
    record.succeeded = false;
    match classify(&record).as_slice() {
        [ObservedEvent::RunFinished { outcome, .. }] => assert_eq!(*outcome, Outcome::Failed),
        other => panic!("expected a recorded failed run, got {other:?}"),
    }
}

#[test]
fn a_failed_mutation_is_unknown_not_harmless() {
    // A failed edit may have applied partially. "Nothing changed" is a claim
    // the observer cannot support.
    let args = json!({"path": "src/a.rs", "new_str": "x"});
    let mut record = call("file_edit", &args, 1);
    record.succeeded = false;
    let events = classify(&record);
    assert_eq!(unattributed_count(&events), 1, "got {events:?}");
}

#[test]
fn a_write_creates_a_change_sized_by_its_content() {
    let args = json!({"path": "src/a.rs", "content": "one\ntwo\nthree"});
    let events = classify(&call("file_write", &args, 3));
    assert_eq!(
        events,
        vec![ObservedEvent::Changed {
            path: "src/a.rs".into(),
            line_count: Some(3),
            turn_index: 3
        }]
    );
}

#[test]
fn a_failed_mutation_is_not_treated_as_no_mutation() {
    // This test previously asserted a failed tool produced NO events, which
    // encoded the assumption that failure means the tree is untouched. A failed
    // edit can apply partially; the honest record is "unknown".
    let args = json!({"path": "src/a.rs", "content": "x"});
    let mut record = call("file_write", &args, 1);
    record.succeeded = false;
    let events = classify(&record);
    assert_eq!(unattributed_count(&events), 1, "got {events:?}");
}

#[test]
fn a_failed_read_still_changes_nothing() {
    // The asymmetry that makes the above safe: non-mutating tools that fail
    // genuinely cannot have altered anything.
    let args = json!({"path": "src/a.rs"});
    let mut record = call("file_read", &args, 1);
    record.succeeded = false;
    assert!(classify(&record).is_empty());
}

#[test]
fn a_mutation_whose_target_is_unreadable_is_recorded_as_unknown() {
    // The critical case: silence would read as "nothing happened".
    let args = json!({"content": "x"});
    let events = classify(&call("file_write", &args, 1));
    assert_eq!(unattributed_count(&events), 1);
    match &events[0] {
        ObservedEvent::Unattributed { tool, reason } => {
            assert_eq!(tool, "file_write");
            assert!(reason.contains("no path"), "the reason must be legible");
        }
        other => panic!("expected Unattributed, got {other:?}"),
    }
}

#[test]
fn a_multi_edit_owes_for_every_file_it_touched() {
    let args = json!({"edits": [
        {"path": "src/a.rs", "new_str": "a\nb"},
        {"path": "src/b.rs", "new_str": "c"}
    ]});
    let events = classify(&call("file_multi_edit", &args, 4));
    assert_eq!(events.len(), 2);

    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(&mut ledger, &events, snap, T);
    assert_eq!(
        ledger.outstanding().len(),
        4,
        "two files, two obligations each"
    );
    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UnreviewedChange),
        3
    );
}

#[test]
fn a_multi_edit_entry_with_no_path_is_unattributed_without_losing_the_others() {
    let args = json!({"edits": [
        {"path": "src/a.rs", "new_str": "a"},
        {"new_str": "orphan"}
    ]});
    let events = classify(&call("file_multi_edit", &args, 1));
    assert_eq!(events.len(), 2);
    assert_eq!(unattributed_count(&events), 1, "one entry is unknown");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ObservedEvent::Changed { .. })),
        "the readable entry is still recorded"
    );
}

#[test]
fn a_patch_is_sized_by_touched_lines_not_context() {
    let args = json!({
        "path": "src/a.rs",
        "patch": "--- a/src/a.rs\n+++ b/src/a.rs\n context\n+added\n-removed\n context"
    });
    let events = classify(&call("patch_apply", &args, 2));
    match &events[0] {
        ObservedEvent::Changed { line_count, .. } => assert_eq!(*line_count, Some(2)),
        other => panic!("expected Changed, got {other:?}"),
    }
}

#[test]
fn a_delete_retires_rather_than_accrues() {
    let write = json!({"path": "src/gone.rs", "content": "a\nb\nc"});
    let remove = json!({"path": "src/gone.rs"});
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call("file_write", &write, 1)),
        snap,
        T,
    );
    apply(
        &mut ledger,
        &classify(&call("file_delete", &remove, 2)),
        snap,
        T,
    );
    assert_eq!(ledger.outstanding_lines(ObligationKind::UntestedLogic), 0);
    assert_eq!(
        ledger.outstanding().len(),
        1,
        "the removal still wants reading"
    );
}

#[test]
fn a_green_cargo_test_does_not_discharge_untested_logic() {
    // The payoff of the Scope correction, end to end through the observer:
    // cargo test reports no per-file coverage, so a green run leaves the
    // obligation standing and says why.
    let write = json!({"path": "src/a.rs", "content": "a\nb"});
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call("file_write", &write, 1)),
        snap,
        T,
    );

    let run_snap = ledger.snapshot();
    let events = classify(&call("cargo_test", &json!({}), 2));
    apply(&mut ledger, &events, run_snap, T);

    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UntestedLogic),
        2,
        "a suite that did not report coverage discharges nothing"
    );
    let evidence = ledger.evidence()[0].clone();
    let obligation = ledger
        .obligations()
        .iter()
        .find(|o| o.kind == ObligationKind::UntestedLogic)
        .unwrap();
    assert_eq!(
        ledger.assess(&evidence, obligation),
        Err(Unsatisfied::CoverageUnknown)
    );
}

#[test]
fn duplicate_delivery_of_one_tool_call_is_visible_as_two_changes() {
    // The observer cannot tell a replayed hook from a genuine second edit, and
    // must not guess. Both are recorded; verification then has to cover the
    // later sequence.
    let args = json!({"path": "src/a.rs", "content": "a"});
    let events = classify(&call("file_write", &args, 1));
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(&mut ledger, &events, snap, T);
    apply(&mut ledger, &events, snap, T);
    assert_eq!(ledger.outstanding().len(), 4);
}

#[test]
fn a_non_mutating_tool_produces_nothing() {
    for tool in ["file_read", "grep_search", "glob_find", "directory_tree"] {
        let args = json!({"path": "src/a.rs"});
        assert!(
            classify(&call(tool, &args, 1)).is_empty(),
            "{tool} must not create debt"
        );
    }
}

#[test]
fn observing_a_session_survives_a_restart() {
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    for (path, content) in [("src/a.rs", "a\nb"), ("src/b.rs", "c")] {
        let args = json!({"path": path, "content": content});
        apply(
            &mut ledger,
            &classify(&call("file_write", &args, 1)),
            snap,
            T,
        );
    }
    let persisted = serde_json::to_string(&ledger).unwrap();
    let restored: Ledger = serde_json::from_str(&persisted).unwrap();
    assert_eq!(ledger, restored);
    assert_eq!(
        restored.outstanding().len(),
        4,
        "a restart forgives nothing"
    );
}

#[test]
fn a_multi_file_patch_owes_for_every_file_it_touches() {
    // patch_apply takes a `diff` that may span files. Attributing it to one
    // path left the rest unrecorded entirely.
    let diff = "--- a/src/a.rs\n+++ b/src/a.rs\n@@\n+one\n+two\n--- a/src/b.rs\n+++ b/src/b.rs\n@@\n-gone\n";
    let args = json!({"diff": diff});
    let events = classify(&call("patch_apply", &args, 5));
    assert_eq!(events.len(), 2, "got {events:?}");

    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(&mut ledger, &events, snap, T);
    let paths: Vec<_> = ledger
        .outstanding()
        .iter()
        .map(|o| o.path.display().to_string())
        .collect();
    assert!(paths.iter().any(|p| p.ends_with("a.rs")));
    assert!(paths.iter().any(|p| p.ends_with("b.rs")));
    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UnreviewedChange),
        3,
        "two added plus one removed"
    );
}

#[test]
fn a_deletion_only_edit_is_not_sized_at_zero() {
    // Sizing an edit by its replacement alone calls a pure deletion weightless.
    let args = json!({"path": "src/a.rs", "old_str": "a\nb\nc", "new_str": ""});
    match classify(&call("file_edit", &args, 1)).as_slice() {
        [ObservedEvent::Changed { line_count, .. }] => {
            assert_eq!(*line_count, Some(3), "removed lines must count")
        }
        other => panic!("expected Changed, got {other:?}"),
    }
}

#[test]
fn an_edit_with_no_recognisable_size_reports_unknown_not_zero() {
    // The failure this whole change is about: `new_string` matched nothing in
    // the real schema, so every edit silently became a zero-line obligation
    // without raising unattributed_count.
    let args = json!({"path": "src/a.rs", "mystery_field": "?"});
    match classify(&call("file_edit", &args, 1)).as_slice() {
        [ObservedEvent::Changed { line_count, .. }] => {
            assert_eq!(*line_count, None, "unknown size must not be reported as 0")
        }
        other => panic!("expected Changed, got {other:?}"),
    }

    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call("file_edit", &args, 1)),
        snap,
        T,
    );
    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UnreviewedChange),
        0
    );
    assert_eq!(
        ledger.outstanding_unknown_size(ObligationKind::UnreviewedChange),
        1,
        "the line total is a floor, and the ledger must say so"
    );
}

#[test]
fn shell_compilation_is_not_recorded_as_test_execution() {
    // shell_command_is_verification returns true for cargo check, npx tsc,
    // go build and sqlfluff lint. Reusing that boolean recreated, through shell
    // dispatch, the exact bug that removing cargo_check from
    // TEST_EXECUTION_TOOLS had just fixed.
    for command in [
        "cargo check",
        "npx tsc --noEmit",
        "go build ./...",
        "cargo clippy",
    ] {
        let args = json!({"command": command});
        let events = classify(&call("shell_exec", &args, 1));
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, ObservedEvent::RunFinished { .. })),
            "{command} executes no tests but was recorded as a test run: {events:?}"
        );
    }

    // And it must not discharge anything.
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call(
            "file_write",
            &json!({"path": "src/a.rs", "content": "a"}),
            1,
        )),
        snap,
        T,
    );
    let run = ledger.snapshot();
    let args = json!({"command": "cargo check"});
    apply(
        &mut ledger,
        &classify(&call("shell_exec", &args, 2)),
        run,
        T,
    );
    assert_eq!(ledger.outstanding_lines(ObligationKind::UntestedLogic), 1);
}

#[test]
fn a_compound_command_that_ends_in_tests_counts_and_flags_the_mutation() {
    // `python fix.py && pytest` both changed files this observer cannot name
    // AND ran tests. Recording only one of those loses information either way.
    let args = json!({"command": "python fix.py && pytest -q"});
    let events = classify(&call("shell_exec", &args, 1));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ObservedEvent::RunFinished { .. })),
        "the tests did run: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            ObservedEvent::OpaqueRun {
                may_have_mutated: true,
                ..
            }
        )),
        "and the tree may have moved: {events:?}"
    );
}

#[test]
fn a_mixed_patch_records_deleted_files_too() {
    // A deletion is `+++ /dev/null`. Keying only on the destination header
    // dropped removed files entirely, with no unattributed event to show it.
    let diff = "--- a/src/keep.rs\n+++ b/src/keep.rs\n@@\n+added\n--- a/src/gone.rs\n+++ /dev/null\n@@\n-one\n-two\n";
    let args = json!({"diff": diff});
    let events = classify(&call("patch_apply", &args, 1));
    let paths: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            ObservedEvent::Changed { path, .. } => Some(path.display().to_string()),
            _ => None,
        })
        .collect();
    assert!(paths.iter().any(|p| p.ends_with("keep.rs")), "{paths:?}");
    assert!(
        paths.iter().any(|p| p.ends_with("gone.rs")),
        "the deleted file must still be recorded: {paths:?}"
    );
}

#[test]
fn an_opaque_run_is_recorded_not_discarded() {
    // Classifying OpaqueRun and then dropping it in apply() left a shell
    // mutation with no trace at all.
    let args = json!({"command": "sed -i s/a/b/ src/a.rs"});
    let events = classify(&call("shell_exec", &args, 1));
    assert!(matches!(
        events.as_slice(),
        [ObservedEvent::OpaqueRun {
            may_have_mutated: true,
            ..
        }]
    ));
}

#[test]
fn a_failing_command_is_not_recorded_as_a_passing_test_run() {
    // Found by a deterministic scenario, not by a unit test: shell_exec reports
    // ITS OWN success, so a red suite arrived as succeeded=true and was
    // recorded Passed. The exit code is the only honest signal.
    let args = json!({"command": "python3 -m unittest discover"});
    let red = r#"{"exit_code":1,"stdout":"","stderr":"FAILED (errors=1)"}"#;
    match classify(&shell_call("shell_exec", &args, red)).as_slice() {
        [ObservedEvent::RunFinished { outcome, .. }] => assert_eq!(*outcome, Outcome::Failed),
        other => panic!("expected a failed run, got {other:?}"),
    }

    let green = r#"{"exit_code":0,"stdout":"OK","stderr":""}"#;
    match classify(&shell_call("shell_exec", &args, green)).as_slice() {
        [ObservedEvent::RunFinished { outcome, .. }] => assert_eq!(*outcome, Outcome::Passed),
        other => panic!("expected a passing run, got {other:?}"),
    }
}

#[test]
fn an_unreadable_shell_result_is_not_treated_as_success() {
    // Absence of a parsable exit code is not evidence the command passed.
    let args = json!({"command": "pytest"});
    for output in ["not json at all", r#"{"stdout":"ok"}"#] {
        match classify(&shell_call("shell_exec", &args, output)).as_slice() {
            [ObservedEvent::RunFinished { outcome, .. }] => {
                assert_eq!(*outcome, Outcome::Failed, "output was: {output}")
            }
            other => panic!("expected RunFinished, got {other:?}"),
        }
    }
}

#[test]
fn a_failing_test_run_discharges_nothing() {
    let mut ledger = Ledger::new();
    let snap = ledger.snapshot();
    apply(
        &mut ledger,
        &classify(&call(
            "file_write",
            &json!({"path": "src/a.rs", "content": "a"}),
            1,
        )),
        snap,
        T,
    );
    let run = ledger.snapshot();
    let args = json!({"command": "pytest"});
    let red = r#"{"exit_code":1}"#;
    apply(
        &mut ledger,
        &classify(&shell_call("shell_exec", &args, red)),
        run,
        T,
    );
    assert_eq!(
        ledger.outstanding_lines(ObligationKind::UntestedLogic),
        1,
        "a red suite establishes work remains"
    );
}
