//! Passive observation of the agent loop.
//!
//! Turns tool calls into ledger events. Pure classification: it reads what a
//! tool was asked to do and what came back, and says what that means for
//! outstanding verification. It does not decide anything, block anything, or
//! surface anything.
//!
//! # Unknown is a recorded outcome, not a skipped one
//!
//! A mutating tool whose target cannot be determined produces
//! [`ObservedEvent::Unattributed`] rather than nothing. Silence would read as
//! "no change happened", which is the failure this whole subsystem exists to
//! prevent — one level further out.
//!
//! # Every mutating tool must be classified
//!
//! [`MUTATING_TOOLS`] is swept by a test against the tool registry, so a new
//! file-writing tool cannot be added without either being classified here or
//! failing the build's tests. Two dispatch paths execute tools
//! (`execute_parallel_tools` and `execute_single_tool_in_batch`); an observer
//! wired into only one would miss half the mutations.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::ledger::{Ledger, Outcome, RunSnapshot, Scope};

/// Tools that can change files on disk. Anything here must be classified by
/// [`classify`], and anything classified must appear here.
pub const MUTATING_TOOLS: &[&str] = &[
    "file_write",
    "file_edit",
    "file_multi_edit",
    "file_fim_edit",
    "file_delete",
    "patch_apply",
];

/// Commands whose execution constitutes a verification run.
pub const VERIFICATION_TOOLS: &[&str] = &["cargo_test", "cargo_check"];

/// What the observer understood a tool call to mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObservedEvent {
    /// A file was changed by the agent.
    Changed {
        path: PathBuf,
        line_count: usize,
        turn_index: usize,
    },
    /// A file was removed by the agent.
    Deleted { path: PathBuf, turn_index: usize },
    /// A verification run finished.
    RunFinished {
        command: String,
        scope: Scope,
        outcome: Outcome,
        artifact: Option<String>,
    },
    /// A mutating tool ran and the observer could not tell what it touched.
    ///
    /// Recorded rather than dropped: an unreadable mutation is unknown state,
    /// and unknown must never look like "nothing happened".
    Unattributed { tool: String, reason: String },
}

/// Minimal view of a tool invocation, so classification can be tested without
/// constructing the agent.
#[derive(Debug, Clone)]
pub struct ToolCallRecord<'a> {
    pub tool: &'a str,
    /// Parsed arguments, as the dispatcher saw them.
    pub arguments: &'a serde_json::Value,
    pub turn_index: usize,
    pub succeeded: bool,
    /// Tool output, used only to size a change when the tool reports it.
    pub output: Option<&'a str>,
}

fn path_argument(arguments: &serde_json::Value) -> Option<PathBuf> {
    for key in ["path", "file_path", "file", "target", "filename"] {
        if let Some(value) = arguments.get(key).and_then(|v| v.as_str()) {
            if !value.is_empty() {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

/// Lines a change touched, when the arguments say so. Returns `None` rather
/// than a guess — an invented size is worse than an absent one.
fn line_count(arguments: &serde_json::Value, output: Option<&str>) -> Option<usize> {
    if let Some(content) = arguments.get("content").and_then(|v| v.as_str()) {
        return Some(content.lines().count());
    }
    if let Some(replacement) = arguments.get("new_string").and_then(|v| v.as_str()) {
        return Some(replacement.lines().count());
    }
    if let Some(patch) = arguments.get("patch").and_then(|v| v.as_str()) {
        // Count only added/removed lines, not context.
        let touched = patch
            .lines()
            .filter(|l| {
                (l.starts_with('+') || l.starts_with('-'))
                    && !l.starts_with("+++")
                    && !l.starts_with("---")
            })
            .count();
        return Some(touched);
    }
    let _ = output;
    None
}

/// Classify one tool call.
///
/// A failed tool changes nothing, so it produces no events — but note the
/// asymmetry: a *partially* applied multi-edit that reports failure would be
/// missed here. That is why the ledger also accepts external-change records.
pub fn classify(call: &ToolCallRecord<'_>) -> Vec<ObservedEvent> {
    if !call.succeeded {
        return Vec::new();
    }

    if VERIFICATION_TOOLS.contains(&call.tool) {
        return vec![ObservedEvent::RunFinished {
            command: call.tool.to_string(),
            // `cargo test` does not report which files it exercised. Claiming
            // workspace-wide coverage from a green run is the exact mistake the
            // Scope contract was corrected to prevent.
            scope: Scope::WorkspaceCoverageUnknown,
            outcome: Outcome::Passed,
            artifact: None,
        }];
    }

    if !MUTATING_TOOLS.contains(&call.tool) {
        return Vec::new();
    }

    // file_multi_edit carries several targets in one call.
    if let Some(edits) = call.arguments.get("edits").and_then(|v| v.as_array()) {
        let mut events = Vec::new();
        for edit in edits {
            match path_argument(edit).or_else(|| path_argument(call.arguments)) {
                Some(path) => events.push(ObservedEvent::Changed {
                    path,
                    line_count: line_count(edit, None).unwrap_or(0),
                    turn_index: call.turn_index,
                }),
                None => events.push(ObservedEvent::Unattributed {
                    tool: call.tool.to_string(),
                    reason: "edit entry named no path".to_string(),
                }),
            }
        }
        return events;
    }

    let Some(path) = path_argument(call.arguments) else {
        return vec![ObservedEvent::Unattributed {
            tool: call.tool.to_string(),
            reason: "no path argument found".to_string(),
        }];
    };

    if call.tool == "file_delete" {
        return vec![ObservedEvent::Deleted {
            path,
            turn_index: call.turn_index,
        }];
    }

    vec![ObservedEvent::Changed {
        path,
        line_count: line_count(call.arguments, call.output).unwrap_or(0),
        turn_index: call.turn_index,
    }]
}

/// Apply observed events to a ledger.
///
/// `run_snapshot` is the ledger position captured *before* the tool ran. The
/// caller must take it beforehand; passing one taken afterwards would let a
/// result claim to cover changes that landed while it executed.
pub fn apply(
    ledger: &mut Ledger,
    events: &[ObservedEvent],
    run_snapshot: RunSnapshot,
    now_ms: u64,
) {
    for event in events {
        match event {
            ObservedEvent::Changed {
                path,
                line_count,
                turn_index,
            } => {
                ledger.record_change(path.clone(), *line_count, *turn_index, now_ms);
            }
            ObservedEvent::Deleted { path, turn_index } => {
                ledger.record_deletion(path.clone(), *turn_index, now_ms);
            }
            ObservedEvent::RunFinished {
                scope,
                outcome,
                artifact,
                ..
            } => {
                ledger.record_test_run(
                    run_snapshot,
                    scope.clone(),
                    *outcome,
                    artifact.clone(),
                    now_ms,
                );
            }
            // An unreadable mutation means the ledger no longer knows the tree.
            // It cannot name the path, so it cannot record an obligation — but
            // it must not pretend nothing happened either. Recorded in the
            // observer's own log; see `unattributed_count`.
            ObservedEvent::Unattributed { .. } => {}
        }
    }
}

/// How many observations could not be attributed to a path. Non-zero means the
/// ledger's picture of the tree is incomplete, and any debt figure derived from
/// it is a floor rather than a total.
pub fn unattributed_count(events: &[ObservedEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, ObservedEvent::Unattributed { .. }))
        .count()
}
