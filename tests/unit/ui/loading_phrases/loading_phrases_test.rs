use super::*;
use crate::agent::llm_wait::{live_spinner_status, LlmWaitPhase, LlmWaitTokenSource};

/// Verbs/nouns that would claim the agent is doing concrete work while it is
/// only waiting on the model (the 0.9.1 phrases "Formatting with rustfmt...",
/// "Benchmarking solutions...", "Consulting the documentation...", ...).
const ACTION_CLAIMS: &[&str] = &[
    "format",
    "benchmark",
    "running",
    "run ",
    "review",
    "consult",
    "analyz",
    "analys",
    "search",
    "reading",
    "compil",
    "test",
    "lint",
    "rustfmt",
    "cargo",
    "clippy",
    "refactor",
    "debug",
    "writing",
    "editing",
    "applying",
    "building",
    "scanning",
    "indexing",
    "documentation",
    "codebase",
    "hammer",
    "sanding",
    "crafting",
    "tending",
];

fn assert_no_action_claim(label: &str) {
    let lower = label.to_lowercase();
    for claim in ACTION_CLAIMS {
        assert!(
            !lower.contains(claim),
            "waiting label {label:?} claims an action ({claim:?}) that is not happening"
        );
    }
}

#[test]
fn waiting_label_is_honest() {
    assert_eq!(waiting_label(), "Waiting for the model");
    assert!(WAITING_LABELS.contains(&waiting_label()));
}

#[test]
fn no_static_label_names_an_action() {
    assert!(!WAITING_LABELS.is_empty());
    for label in WAITING_LABELS {
        assert!(!label.is_empty());
        assert_no_action_claim(label);
    }
}

#[test]
fn no_live_status_names_an_action() {
    let phases = [
        LlmWaitPhase::Prefill,
        LlmWaitPhase::Reasoning,
        LlmWaitPhase::Streaming,
        LlmWaitPhase::AwaitingResponse,
        LlmWaitPhase::SideCall("context_summary"),
    ];
    for phase in phases {
        for (tokens, source) in [
            (0, LlmWaitTokenSource::Estimate),
            (1234, LlmWaitTokenSource::Estimate),
            (1234, LlmWaitTokenSource::Usage),
            (5, LlmWaitTokenSource::None),
        ] {
            assert_no_action_claim(&live_spinner_status(phase, tokens, source));
        }
    }
}

#[test]
fn live_status_reports_measured_phase_and_tokens() {
    assert_eq!(
        live_spinner_status(LlmWaitPhase::Prefill, 0, LlmWaitTokenSource::Estimate),
        "Waiting for the model"
    );
    assert_eq!(
        live_spinner_status(LlmWaitPhase::Reasoning, 1234, LlmWaitTokenSource::Estimate),
        "Model reasoning · ~1.2K tokens"
    );
    assert_eq!(
        live_spinner_status(LlmWaitPhase::Streaming, 340, LlmWaitTokenSource::Usage),
        "Model responding · 340 tokens"
    );
    // Nothing observable: no count is invented.
    assert_eq!(
        live_spinner_status(LlmWaitPhase::AwaitingResponse, 9, LlmWaitTokenSource::None),
        "Waiting for the model"
    );
    assert_eq!(
        live_spinner_status(
            LlmWaitPhase::SideCall("context_summary"),
            0,
            LlmWaitTokenSource::None
        ),
        "Waiting for the model (context_summary)"
    );
}
