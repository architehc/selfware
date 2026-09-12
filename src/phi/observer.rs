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
/// Tools that EXECUTE tests. `cargo_check` is deliberately absent: it compiles
/// and runs nothing, so treating it as a test run would discharge obligations
/// on the strength of the code merely building.
pub const TEST_EXECUTION_TOOLS: &[&str] = &["cargo_test"];

/// Tools that compile without executing. Recorded, but they discharge nothing.
pub const COMPILE_ONLY_TOOLS: &[&str] = &["cargo_check"];

/// Tools that run arbitrary commands, which may or may not be verification and
/// may or may not mutate. Their command text decides, via the authoritative
/// pipeline-aware classifier in agent::tool_dispatch::helpers.
pub const SHELL_TOOLS: &[&str] = &["shell_exec", "pty_shell"];

/// What the observer understood a tool call to mean.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ObservedEvent {
    /// A file was changed by the agent.
    Changed {
        path: PathBuf,
        /// `None` when the mutation did not report a size. Never silently zero.
        line_count: Option<usize>,
        turn_index: usize,
    },
    /// A file was removed by the agent.
    Deleted { path: PathBuf, turn_index: usize },
    /// A test-executing run finished, whatever its outcome.
    RunFinished {
        command: String,
        scope: Scope,
        outcome: Outcome,
        artifact: Option<String>,
    },
    /// A command ran that compiles but executes no tests, or whose effects are
    /// opaque. Recorded so the session log is complete; discharges nothing.
    ///
    /// `may_have_mutated` marks a shell command that could have changed files
    /// the observer cannot name — the ledger's picture is then incomplete, and
    /// saying so is the point.
    OpaqueRun {
        command: String,
        outcome: Outcome,
        may_have_mutated: bool,
    },
    /// A mutating tool ran and the observer could not tell what it touched.
    ///
    /// Recorded rather than dropped: an unreadable mutation is unknown state,
    /// and unknown must never look like "nothing happened".
    Unattributed { tool: String, reason: String },
}

/// A mutation the observer could not attribute, kept in durable telemetry.
///
/// Debug-logging the count was not enough to answer the question it exists for:
/// whether the classifier's schema assumptions match real traffic. That needs
/// the tool, the reason, and the call id, in the saved artifact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnattributedRecord {
    pub turn: usize,
    pub tool: String,
    pub reason: String,
    /// Tool-call id where the dispatcher provided one.
    pub call_id: Option<String>,
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
    // The real file_edit / file_multi_edit schema is old_str / new_str. An
    // earlier version read "new_string", which matched nothing, so every edit
    // produced a zero-line obligation and never raised unattributed_count.
    let old_str = arguments.get("old_str").and_then(|v| v.as_str());
    let new_str = arguments.get("new_str").and_then(|v| v.as_str());
    if old_str.is_some() || new_str.is_some() {
        // Both sides count: a deletion-only edit removes lines and adds none,
        // and sizing it by the replacement alone would call it zero.
        let removed = old_str.map(|s| s.lines().count()).unwrap_or(0);
        let added = new_str.map(|s| s.lines().count()).unwrap_or(0);
        return Some(removed.max(added));
    }
    if let Some(patch) = arguments
        .get("diff")
        .or_else(|| arguments.get("patch"))
        .and_then(|v| v.as_str())
    {
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

/// Paths a unified diff touches, from its `+++ b/<path>` headers. A patch may
/// cover several files; attributing it to one would leave the rest unrecorded.
fn diff_paths(diff: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let cleaned = rest
                .split('\t')
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_start_matches("b/");
            if !cleaned.is_empty()
                && cleaned != "/dev/null"
                && !paths.iter().any(|p| p == &PathBuf::from(cleaned))
            {
                paths.push(PathBuf::from(cleaned));
            }
        }
    }
    paths
}

/// Lines each path gains or loses in a multi-file diff.
fn diff_line_counts(diff: &str) -> Vec<(PathBuf, usize)> {
    let mut out: Vec<(PathBuf, usize)> = Vec::new();
    let mut current: Option<PathBuf> = None;
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let cleaned = rest
                .split('\t')
                .next()
                .unwrap_or(rest)
                .trim()
                .trim_start_matches("b/");
            current =
                (!cleaned.is_empty() && cleaned != "/dev/null").then(|| PathBuf::from(cleaned));
            if let Some(path) = &current {
                if !out.iter().any(|(p, _)| p == path) {
                    out.push((path.clone(), 0));
                }
            }
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if (line.starts_with('+') || line.starts_with('-')) && current.is_some() {
            let path = current.clone().unwrap();
            if let Some(entry) = out.iter_mut().find(|(p, _)| *p == path) {
                entry.1 += 1;
            }
        }
    }
    out
}

/// Classify one tool call.
///
/// A failed tool changes nothing, so it produces no events — but note the
/// asymmetry: a *partially* applied multi-edit that reports failure would be
/// missed here. That is why the ledger also accepts external-change records.
pub fn classify(call: &ToolCallRecord<'_>) -> Vec<ObservedEvent> {
    let outcome = if call.succeeded {
        Outcome::Passed
    } else {
        Outcome::Failed
    };

    // Shell tools run anything. The command text decides, using the
    // pipeline-aware classifier the codebase already has rather than a second
    // heuristic that would drift from it.
    if SHELL_TOOLS.contains(&call.tool) {
        let command = call
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if crate::agent::tool_dispatch::helpers::shell_command_is_verification(command) {
            return vec![ObservedEvent::RunFinished {
                command: command.to_string(),
                scope: Scope::WorkspaceCoverageUnknown,
                outcome,
                artifact: None,
            }];
        }
        // Not verification. It may still have written files the observer cannot
        // name, so the session log records that the tree may have moved.
        return vec![ObservedEvent::OpaqueRun {
            command: command.to_string(),
            outcome,
            may_have_mutated: true,
        }];
    }

    if TEST_EXECUTION_TOOLS.contains(&call.tool) {
        // Recorded whatever the outcome: a failed run is evidence the work is
        // not done, and losing it makes a red session look merely quiet.
        return vec![ObservedEvent::RunFinished {
            command: call.tool.to_string(),
            scope: Scope::WorkspaceCoverageUnknown,
            outcome,
            artifact: None,
        }];
    }

    if COMPILE_ONLY_TOOLS.contains(&call.tool) {
        return vec![ObservedEvent::OpaqueRun {
            command: call.tool.to_string(),
            outcome,
            may_have_mutated: false,
        }];
    }

    if !MUTATING_TOOLS.contains(&call.tool) {
        return Vec::new();
    }

    // A failed mutation may still have applied partially. Saying "nothing
    // changed" is a claim the observer cannot support.
    if !call.succeeded {
        return vec![ObservedEvent::Unattributed {
            tool: call.tool.to_string(),
            reason: "mutation reported failure; partial application unknown".to_string(),
        }];
    }

    // A unified diff may span several files; attributing it to one leaves the
    // rest unrecorded.
    if let Some(diff) = call
        .arguments
        .get("diff")
        .or_else(|| call.arguments.get("patch"))
        .and_then(|v| v.as_str())
    {
        let counts = diff_line_counts(diff);
        if !counts.is_empty() {
            return counts
                .into_iter()
                .map(|(path, lines)| ObservedEvent::Changed {
                    path,
                    line_count: Some(lines),
                    turn_index: call.turn_index,
                })
                .collect();
        }
        if diff_paths(diff).is_empty() {
            return vec![ObservedEvent::Unattributed {
                tool: call.tool.to_string(),
                reason: "diff named no target files".to_string(),
            }];
        }
    }

    if let Some(edits) = call.arguments.get("edits").and_then(|v| v.as_array()) {
        let mut events = Vec::new();
        for edit in edits {
            match path_argument(edit).or_else(|| path_argument(call.arguments)) {
                Some(path) => events.push(ObservedEvent::Changed {
                    path,
                    line_count: line_count(edit, None),
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
        line_count: line_count(call.arguments, call.output),
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
            // Compiles or opaque commands discharge nothing; they are recorded
            // in the observation log, not the ledger.
            ObservedEvent::OpaqueRun { .. } => {}
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
