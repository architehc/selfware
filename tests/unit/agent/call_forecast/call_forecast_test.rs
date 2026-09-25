use super::*;
use crate::agent::deadline::{time_window_reached, time_window_secs};

fn shape(prompt_tokens: u64, completion_tokens: u64, elapsed_ms: u64) -> CallShape {
    CallShape {
        prompt_tokens,
        completion_tokens,
        elapsed_ms,
    }
}

/// val083 b2_65536, calls 1–26 (prompt, completion, elapsed ms) from
/// out.jsonl — every call before the 189 s write-up call (27) after which
/// 0.8.3's wrap-up fired with 299 s left.
const B2_65536_CALLS_1_TO_26: &[(u64, u64, u64)] = &[
    (25045, 136, 11_200),
    (27948, 76, 4_400),
    (29061, 72, 7_200),
    (35263, 209, 11_400),
    (22388, 157, 11_900),
    (23763, 251, 13_800),
    (26152, 110, 6_000),
    (30272, 109, 8_400),
    (34563, 86, 10_100),
    (36029, 90, 6_400),
    (31733, 480, 28_200),
    (32091, 305, 14_800),
    (35079, 459, 18_500),
    (36129, 125, 11_000),
    (36129, 179, 12_100),
    (35753, 232, 14_800),
    (36513, 282, 10_400),
    (36552, 223, 10_700),
    (37203, 448, 18_000),
    (36662, 315, 15_600),
    (33892, 401, 12_900),
    (36240, 1194, 40_000),
    (36906, 411, 20_300),
    (36591, 278, 10_900),
    (36297, 1012, 37_300),
    (36544, 416, 20_000),
];

fn b2_65536_calls() -> Vec<CallShape> {
    B2_65536_CALLS_1_TO_26
        .iter()
        .map(|&(p, c, ms)| shape(p, c, ms))
        .collect()
}

/// The lead's regression: in b2_65536 the next call (27) took 188.8 s and
/// the final answer (28) 304.9 s for 5,288 tokens; 0.8.3 wrapped up only
/// after call 27, with 299 s left. Before call 27 at least 299 + 188.8 =
/// 487.8 s remained. The forecast window must exceed that, so the wrap-up
/// fires BEFORE call 27 and the 305 s answer fits.
#[test]
fn b2_65536_wrap_up_fires_before_the_last_exploration_call() {
    let forecast = CallForecast::from_calls(&b2_65536_calls(), None);
    // No long call measured yet: the fallback decode rate, the report floor.
    assert_eq!(forecast.decode_tok_per_sec, DECODE_TOK_PER_SEC_FALLBACK);
    assert_eq!(forecast.answer_completion_tokens, ANSWER_COMPLETION_FLOOR);
    assert_eq!(forecast.prompt_tokens, 36_544);
    // The measured answer (304.9 s) fits the forecast answer time.
    assert!(forecast.answer_secs() >= 305, "{forecast:?}");
    let window = time_window_secs(&forecast, 900);
    assert!(
        window > 488,
        "window {window}s must cover 487.8 s: {forecast:?}"
    );
    assert!(time_window_reached(488, &forecast, 900).is_some());
    // …and still leaves a third of the budget for exploration.
    assert!(window <= 600, "{window}");
}

/// Short calls are never decode samples (they measure 27–30 tok/s here and
/// would forecast an answer that does not fit); a measured long call is.
#[test]
fn decode_rate_comes_from_long_calls_only_at_the_slowest_measured() {
    let mut calls = b2_65536_calls();
    calls.push(shape(37_332, 3_591, 188_800)); // call 27: 19.0 tok/s
    let f = CallForecast::from_calls(&calls, None);
    assert!(
        (f.decode_tok_per_sec - 3_591.0 / 188.8).abs() < 1e-6,
        "{f:?}"
    );
    calls.push(shape(40_000, 6_000, 120_000)); // a faster long call: 50 tok/s
    let f = CallForecast::from_calls(&calls, None);
    assert!(
        (f.decode_tok_per_sec - 3_591.0 / 188.8).abs() < 1e-6,
        "slowest measured, not the mean: {f:?}"
    );
}

/// A draft this run produced sets the answer size instead of the floor.
#[test]
fn a_measured_draft_replaces_the_answer_floor() {
    let f = CallForecast::from_calls(&b2_65536_calls(), Some(3_079));
    assert_eq!(f.answer_completion_tokens, 3_079);
    assert_eq!(f.answer_tokens(), 36_544 + 3_079);
}

/// No measurement at all: the fallbacks, and a zero prompt.
#[test]
fn empty_run_uses_the_measured_fallbacks() {
    let f = CallForecast::from_calls(&[], None);
    assert_eq!(f.prompt_tokens, 0);
    assert_eq!(f.next_completion_tokens, 0);
    assert_eq!(f.prefill_ms_per_token, PREFILL_MS_PER_TOKEN_FALLBACK);
    assert_eq!(f.answer_secs(), (6_526.0f64 / 15.0).ceil() as u64);
}
