use super::*;
use crate::agent::turn_artifacts::{ExecutedTool, RejectedTool};

// Per-turn outcomes of real validation runs, one char per turn artifact:
// `O` a dispatched turn that ran a call (or was refused for a non-protocol
// reason), `P` a dispatched turn that ran nothing because every call failed at
// the tool protocol, `-` a turn without a tool call (final answer, prose).

/// val084 runs/review at f11e6f68 (53 turns, SIGTERM at 2,642 s).
const REVIEW_084: &str = "OOOOPOOPPOOOPPOPOPOPPOPPOPPPPPPOPPPPPPPPPPPPPPPPPPPPP";

/// Healthy runs with rejected turns (val082/val083/val084); none may stop.
const HEALTHY: &[(&str, &str)] = &[
    (
        "val082 long_review",
        "OPPOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOP-OOOP-OOOOOOOOOOOOO-O--",
    ),
    ("val083 b3_resume", "OOOOPOOOOOOOOOOOOOPO-"),
    ("val083 b3_review", "OOOOOPPOOOOOOOOOOO"),
    ("val083 c24", "OOOOOOOOOOOOOOOOOOOOOPOOOOPOOO"),
    (
        "val083 long_review",
        "OOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOPOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOPOOOOOOOOOOOOOOOOOOOOOOOO-O-",
    ),
    ("val083 review", "OOOOOOOOOOOOOPOOOOOO-O--"),
    (
        "val084 long_review",
        "OOOOOOOOOOOOOOOOOOPOOOOPOOOOOOOOOOOOOOOOOOOOOPOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOOO-",
    ),
];

/// Replay a run through the window; the 1-based turn at which it stops.
fn stop_turn(sequence: &str) -> Option<(usize, String)> {
    let mut window = ProtocolStallWindow::default();
    for (i, turn) in sequence.chars().enumerate() {
        let failure = match turn {
            'P' => Some(vec![format!("turn {} rejected", i + 1)]),
            'O' => None,
            _ => continue,
        };
        if let Some(message) = window.record(failure) {
            return Some((i + 1, message));
        }
    }
    None
}

#[test]
fn review_084_stops_at_turn_27_not_at_sigterm() {
    let (turn, message) = stop_turn(REVIEW_084).expect("the review run must stop");
    assert_eq!(turn, 27);
    assert!(message.starts_with(PROTOCOL_STALL_MARKER), "{message}");
    assert!(message.contains("6 of the last 8"), "{message}");
    // The last rejection reasons are the evidence, newest first.
    assert!(
        message.contains("[turn 27 rejected] [turn 26 rejected]"),
        "{message}"
    );
}

#[test]
fn healthy_runs_never_stop() {
    for (run, sequence) in HEALTHY {
        assert_eq!(stop_turn(sequence), None, "{run}");
    }
}

#[test]
fn only_a_failed_turn_can_stop_and_executed_turns_age_failures_out() {
    let mut window = ProtocolStallWindow::default();
    for _ in 0..PROTOCOL_STALL_THRESHOLD - 1 {
        assert!(window.record(Some(vec!["bad".into()])).is_none());
    }
    // A turn that ran something never stops the run.
    assert!(window.record(None).is_none());
    assert_eq!(window.failed_in_window(), PROTOCOL_STALL_THRESHOLD - 1);
    // The next failure reaches the threshold within the window.
    assert!(window.record(Some(vec!["bad".into()])).is_some());
    // Healthy turns push old failures out of the window.
    let mut window = ProtocolStallWindow::default();
    for _ in 0..PROTOCOL_STALL_THRESHOLD - 1 {
        window.record(Some(vec!["bad".into()]));
    }
    for _ in 0..PROTOCOL_STALL_WINDOW {
        window.record(None);
    }
    assert_eq!(window.failed_in_window(), 0);
    assert!(window.record(Some(vec!["bad".into()])).is_none());
    window.clear();
    assert_eq!(window.failed_in_window(), 0);
}

fn rejection(reason: &str) -> ParseRejection {
    ParseRejection {
        tool_name: Some("tool".into()),
        reason: reason.into(),
        raw_text: String::new(),
    }
}

#[test]
fn protocol_failure_is_a_turn_that_ran_nothing_for_a_protocol_reason() {
    let parse = [rejection("Tool call NOT executed: malformed")];
    // All-unparseable turn (review turn_0030 shape).
    let decision = AgentDecision::RejectedTools {
        rejected_tools: vec![RejectedTool {
            name: "tool".into(),
            reason: "Tool call NOT executed: malformed".into(),
        }],
    };
    assert_eq!(
        protocol_failure_reasons(&decision, &parse),
        Some(vec!["Tool call NOT executed: malformed".to_string()])
    );
    // A call to a tool that does not exist (review turn_0049 shape).
    let unknown = "Safety check failed: tool 'tool' does not exist. Available tools: file_read";
    let decision = AgentDecision::RejectedTools {
        rejected_tools: vec![RejectedTool {
            name: "tool".into(),
            reason: unknown.into(),
        }],
    };
    assert_eq!(
        protocol_failure_reasons(&decision, &[]),
        Some(vec![unknown.to_string()])
    );
    // A turn that ran a call is healthy, even with a rejected sibling.
    let decision = AgentDecision::Dispatched {
        tools: vec![ExecutedTool {
            name: "file_read".into(),
            ok: true,
        }],
        rejected_tools: Vec::new(),
    };
    assert_eq!(protocol_failure_reasons(&decision, &parse), None);
    // Refusals for non-protocol reasons (safety, duplicates) are not stalls.
    let decision = AgentDecision::RejectedTools {
        rejected_tools: vec![RejectedTool {
            name: "file_read".into(),
            reason: "Safety check failed: Safety error: Path not in allowed list".into(),
        }],
    };
    assert_eq!(protocol_failure_reasons(&decision, &[]), None);
    // Turns without a dispatch never count.
    assert_eq!(
        protocol_failure_reasons(&AgentDecision::NoToolCall, &parse),
        None
    );
}
