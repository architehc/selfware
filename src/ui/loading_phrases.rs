//! Spinner label shown while waiting for the model.
//!
//! This used to rotate ~100 "witty" phrases, many of which claimed concrete
//! work that was not happening ("Formatting with rustfmt...", "Benchmarking
//! solutions...", "Reviewing the codebase...") — shown even during read-only
//! tasks. A waiting spinner may only say what is actually known: the agent is
//! waiting on the model. Anything more specific (phase, tokens so far) comes
//! from the measured heartbeat in `agent::llm_wait`, never from invented copy.

/// The honest spinner label while a model call is in flight and nothing more
/// specific has been observed yet.
pub const WAITING_LABEL: &str = "Waiting for the model";

/// Every static label the waiting spinner may show. Kept as a list so the
/// honesty test can check all of them; dynamic text is built only from
/// measured phase/token data in `agent::llm_wait::live_spinner_status`.
pub const WAITING_LABELS: &[&str] = &[WAITING_LABEL];

/// The label to show while waiting for the model. Kept as a function so
/// callers do not depend on the constant's name.
pub fn waiting_label() -> &'static str {
    WAITING_LABEL
}

#[cfg(test)]
#[path = "../../tests/unit/ui/loading_phrases/loading_phrases_test.rs"]
mod tests;
