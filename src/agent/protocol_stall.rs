//! Tool-protocol stall stop.
//!
//! A turn whose tool calls ALL fail at the protocol level (no parser accepts
//! the markup, the native call is malformed, or the call names a tool that
//! does not exist) runs nothing. Each such turn is answered with a refusal,
//! but before this module nothing counted them on a task that does not
//! require edits: the only bound was the mutation no-tool stall, which is a
//! no-op on read-only work. The 0.8.4 validation `review` run (a read-only
//! review) had 38 protocol-failed turns out of 53, interleaved with 15 turns
//! that did run a call, and spun for 2,642 s until SIGTERM.
//!
//! The stop is a sliding window over DISPATCHED turns (turns with at least
//! one tool call, parsed or rejected): when `PROTOCOL_STALL_THRESHOLD` of
//! the last `PROTOCOL_STALL_WINDOW` dispatched turns failed at the
//! protocol level, the run ends with a typed `TOOL_PROTOCOL_STALL` failure
//! that names the last rejection reasons. Turns that end without a tool call
//! (final answers, prose) are not dispatches and do not move the window.
//!
//! Bound (measured on every turn artifact of the val082/val083/val084
//! validation runs, 1,139 turns in 46 runs): healthy runs never had more than
//! 2 protocol-failed turns in any 8-dispatch window (val083 `b3_review`,
//! `c24`; val082/val084 `long_review`). The val084 `review` run reached 6 of
//! 8 at turn 27 (about 11.5 minutes in, 20:18 of a run that started 20:06),
//! instead of running on until the SIGTERM at 2,642 s. A consecutive-only
//! counter would not have fired early there: its rejected turns were
//! interleaved with executed ones (`EEEEREERREEERRERERERRERR…`).

use std::collections::VecDeque;

use super::turn_artifacts::AgentDecision;
use crate::tool_parser::ParseRejection;

/// Dispatched turns the stall window looks back over.
pub(crate) const PROTOCOL_STALL_WINDOW: usize = 8;

/// Protocol-failed turns within the window that stop the run.
pub(crate) const PROTOCOL_STALL_THRESHOLD: usize = 6;

/// Marker the typed failure carries; `FailureMode::classify` and the fatal
/// loop-error check key on it.
pub(crate) const PROTOCOL_STALL_MARKER: &str = "TOOL_PROTOCOL_STALL";

/// Rejection reasons kept in the failure evidence.
const EVIDENCE_REASONS: usize = 2;

/// Characters kept per rejection reason in the evidence.
const EVIDENCE_REASON_CHARS: usize = 240;

/// Refusal text of a call to a tool that is not registered (tool_dispatch
/// safety check). A generic wrapper leaking through (`tool`, `tool_call`)
/// lands here, so it is a protocol failure, not a tool failure.
const UNKNOWN_TOOL_REASON: &str = "does not exist. Available tools";

/// Why a dispatched turn ran nothing because of the tool protocol, or `None`
/// when the turn executed at least one call or was refused for a
/// non-protocol reason (safety, policy, duplicate suppression, ...).
///
/// `parse_rejections` are the turn's unparseable calls (text markup no parser
/// accepted, malformed native calls); `decision` is the classified dispatch.
pub(crate) fn protocol_failure_reasons(
    decision: &AgentDecision,
    parse_rejections: &[ParseRejection],
) -> Option<Vec<String>> {
    let rejected = match decision {
        AgentDecision::Dispatched { tools, .. } if !tools.is_empty() => return None,
        AgentDecision::Dispatched { rejected_tools, .. } => rejected_tools,
        AgentDecision::RejectedTools { rejected_tools } => rejected_tools,
        _ => return None,
    };
    let mut reasons: Vec<String> = parse_rejections.iter().map(|r| r.reason.clone()).collect();
    reasons.extend(
        rejected
            .iter()
            .filter(|r| r.reason.contains(UNKNOWN_TOOL_REASON))
            .map(|r| r.reason.clone()),
    );
    (!reasons.is_empty()).then_some(reasons)
}

/// Sliding window over the outcomes of the last dispatched turns.
#[derive(Debug, Default, Clone)]
pub(crate) struct ProtocolStallWindow {
    /// One entry per dispatched turn, oldest first: the protocol-failure
    /// reasons of a failed turn, `None` for a turn that ran something.
    recent: VecDeque<Option<Vec<String>>>,
}

impl ProtocolStallWindow {
    /// Record one dispatched turn. Returns the typed stall message when this
    /// turn failed at the protocol level and brings the window to the
    /// threshold.
    pub(crate) fn record(&mut self, failure: Option<Vec<String>>) -> Option<String> {
        let failed_now = failure.is_some();
        if self.recent.len() == PROTOCOL_STALL_WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(failure);
        let failed = self.failed_in_window();
        if !failed_now || failed < PROTOCOL_STALL_THRESHOLD {
            return None;
        }
        let mut last: Vec<String> = Vec::new();
        for reason in self.recent.iter().rev().flatten().flatten() {
            let short: String = reason.chars().take(EVIDENCE_REASON_CHARS).collect();
            if !last.contains(&short) {
                last.push(short);
            }
            if last.len() == EVIDENCE_REASONS {
                break;
            }
        }
        Some(format!(
            "{PROTOCOL_STALL_MARKER}: {failed} of the last {} tool-call turns ran nothing because \
             every call was malformed or named no real tool; last rejection(s): {}",
            self.recent.len(),
            last.iter()
                .map(|r| format!("[{r}]"))
                .collect::<Vec<_>>()
                .join(" ")
        ))
    }

    /// Protocol-failed turns currently in the window.
    pub(crate) fn failed_in_window(&self) -> usize {
        self.recent.iter().filter(|t| t.is_some()).count()
    }

    /// Forget every recorded turn (new task).
    pub(crate) fn clear(&mut self) {
        self.recent.clear();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/protocol_stall/protocol_stall_test.rs"]
mod tests;
