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

#[test]
fn every_mutating_tool_in_the_registry_is_classified() {
    // Rule 5 sweep, enforced. A new file-writing tool that is not listed here
    // would silently create no obligation, and the ledger would report a debt
    // of zero for work nobody checked.
    //
    // The registry's critical-tool list is the source; anything in it that
    // writes must be known to the observer.
    let registry_writers = ["file_write", "file_edit", "file_multi_edit", "file_delete"];
    for tool in registry_writers {
        assert!(
            MUTATING_TOOLS.contains(&tool),
            "{tool} can change files but the observer does not classify it"
        );
    }
    // And the reverse: nothing is claimed that the tool layer does not have.
    for tool in MUTATING_TOOLS {
        assert!(
            tool.starts_with("file_") || *tool == "patch_apply",
            "unexpected tool in MUTATING_TOOLS: {tool}"
        );
    }
}

#[test]
fn a_write_creates_a_change_sized_by_its_content() {
    let args = json!({"path": "src/a.rs", "content": "one\ntwo\nthree"});
    let events = classify(&call("file_write", &args, 3));
    assert_eq!(
        events,
        vec![ObservedEvent::Changed {
            path: "src/a.rs".into(),
            line_count: 3,
            turn_index: 3
        }]
    );
}

#[test]
fn a_failed_tool_changes_nothing() {
    let args = json!({"path": "src/a.rs", "content": "x"});
    let mut record = call("file_write", &args, 1);
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
        {"path": "src/a.rs", "new_string": "a\nb"},
        {"path": "src/b.rs", "new_string": "c"}
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
        {"path": "src/a.rs", "new_string": "a"},
        {"new_string": "orphan"}
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
        ObservedEvent::Changed { line_count, .. } => assert_eq!(*line_count, 2),
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
