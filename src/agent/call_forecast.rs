//! Forecast of the next model call and of the final-answer call, from the
//! calls measured in THIS run (AGENTS.md rule 4). The wrap-up (see
//! [`super::deadline`]) fires one turn ahead: when what is left after the
//! next ordinary call would no longer fit the final answer.
//!
//! Why not "2 × the slowest call so far" (0.8.3): exploration calls are
//! short, the final answer is output-heavy. In val083 the wrap-up fired in
//! all four timed-out runs and in three of them no model turn completed
//! after it. b2_65536: slowest call before the wrap-up 189 s → reserve
//! 379 s, wrap-up with 299 s left; the final answer then took 305 s for
//! 5,288 completion tokens (17.3 tok/s at a 36k prompt) and overran.
//!
//! Model: `call_secs(prompt, completion) = prompt × prefill + completion /
//! decode`, both rates taken at a SLOW quantile of this run's calls.

use crate::api::usage::CallShape;

/// Completion size from which a call counts as a decode-rate sample: below
/// it prefill dominates the wall time. 1,500 is the cut the val082/val083
/// measurement used (42 such calls on llm.selfware.design).
pub(crate) const DECODE_SAMPLE_MIN_COMPLETION: u64 = 1_500;

/// Decode rate (completion tokens per second of whole-call time) assumed
/// until this run has measured a long call.
///
/// Why 15: the slowest long-output call across the 42 val082/val083 calls
/// with >= 1,500 completion tokens ran at 15.1 tok/s (b2_350000: 3,495
/// tokens at a 154k prompt; p05 16.5, p10 19.0). Short calls measure much
/// faster (27–30 tok/s on b2_65536) and would predict an answer that does
/// not fit, so they are never decode samples.
pub(crate) const DECODE_TOK_PER_SEC_FALLBACK: f64 = 15.0;

/// Completion size below which a call counts as a prefill sample.
pub(crate) const PREFILL_SAMPLE_MAX_COMPLETION: u64 = 300;

/// Prefill cost (ms of whole-call time per prompt token) assumed until
/// this run has measured a short call.
///
/// Why 0.615: the p90 of elapsed / prompt over the 336 val082/val083 calls
/// with a > 5k prompt and < 300 completion tokens (p50 0.297).
pub(crate) const PREFILL_MS_PER_TOKEN_FALLBACK: f64 = 0.615;

/// Completion tokens assumed for the final answer: the floor under this
/// run's largest measured draft (a tool-less answer of substance) — a larger
/// draft raises it, a smaller one never lowers it.
///
/// Why 6,526: the largest final report measured in the val082/val083
/// b2/b3 review runs (b3_resume; the others: 3,079 – 5,872, b2_65536 5,288).
/// The reserve must fit the answer the run will actually write, and
/// reports of this prompt shape reach that size. (cite2's 12,362 is a
/// different prompt whose answer was mostly hidden reasoning.)
pub(crate) const ANSWER_COMPLETION_FLOOR: u64 = 6_526;

/// Slow quantile used for rates: the SLOWEST measured call (nearest-rank
/// p0 for decode — a run has a handful of long calls, and the final
/// answer is the long call that must fit) and p90 for prefill and for the
/// next ordinary call's completion size.
const SLOW_QUANTILE: f64 = 0.9;

/// Forecast inputs measured from this run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CallForecast {
    /// Prompt tokens of the latest measured call (the next call re-sends
    /// at least that).
    pub prompt_tokens: u64,
    /// Slowest measured decode rate (tok/s), or the fallback.
    pub decode_tok_per_sec: f64,
    /// p90 prefill cost (ms per prompt token), or the fallback.
    pub prefill_ms_per_token: f64,
    /// p90 completion size of this run's calls (the next ordinary call).
    pub next_completion_tokens: u64,
    /// Expected final-answer completion size: this run's largest draft,
    /// never below [`ANSWER_COMPLETION_FLOOR`].
    pub answer_completion_tokens: u64,
}

/// Nearest-rank quantile of an ascending slice.
fn quantile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let idx = ((sorted.len() - 1) as f64 * q).floor() as usize;
    Some(sorted[idx.min(sorted.len() - 1)])
}

impl CallForecast {
    /// Forecast from this run's measured calls and its largest draft's
    /// completion size (`None` when no draft exists yet; the answer size is
    /// never below [`ANSWER_COMPLETION_FLOOR`]).
    pub(crate) fn from_calls(calls: &[CallShape], draft_completion_tokens: Option<u64>) -> Self {
        let mut decode: Vec<f64> = calls
            .iter()
            .filter(|c| c.completion_tokens >= DECODE_SAMPLE_MIN_COMPLETION && c.elapsed_ms > 0)
            .map(|c| c.completion_tokens as f64 / (c.elapsed_ms as f64 / 1000.0))
            .collect();
        decode.sort_by(f64::total_cmp);
        let mut prefill: Vec<f64> = calls
            .iter()
            .filter(|c| c.completion_tokens < PREFILL_SAMPLE_MAX_COMPLETION && c.prompt_tokens > 0)
            .map(|c| c.elapsed_ms as f64 / c.prompt_tokens as f64)
            .collect();
        prefill.sort_by(f64::total_cmp);
        let mut completions: Vec<f64> = calls.iter().map(|c| c.completion_tokens as f64).collect();
        completions.sort_by(f64::total_cmp);
        CallForecast {
            prompt_tokens: calls.last().map(|c| c.prompt_tokens).unwrap_or(0),
            decode_tok_per_sec: decode
                .first()
                .copied()
                .unwrap_or(DECODE_TOK_PER_SEC_FALLBACK),
            prefill_ms_per_token: quantile(&prefill, SLOW_QUANTILE)
                .unwrap_or(PREFILL_MS_PER_TOKEN_FALLBACK),
            next_completion_tokens: quantile(&completions, SLOW_QUANTILE)
                .map(|c| c.ceil() as u64)
                .unwrap_or(0),
            // max(largest draft, floor): an early draft (a 204-char
            // write-up segment) is not the final report's size, and letting
            // it displace the floor collapsed the answer forecast ~8x.
            answer_completion_tokens: draft_completion_tokens
                .unwrap_or(0)
                .max(ANSWER_COMPLETION_FLOOR),
        }
    }

    /// Predicted wall seconds of a call re-sending the current prompt and
    /// producing `completion` tokens.
    fn call_secs(&self, completion: u64) -> f64 {
        self.prompt_tokens as f64 * self.prefill_ms_per_token / 1000.0
            + completion as f64 / self.decode_tok_per_sec
    }

    /// Predicted seconds of the next ordinary (exploration) call.
    pub(crate) fn next_call_secs(&self) -> u64 {
        self.call_secs(self.next_completion_tokens).ceil() as u64
    }

    /// Predicted seconds of the final-answer call.
    pub(crate) fn answer_secs(&self) -> u64 {
        self.call_secs(self.answer_completion_tokens).ceil() as u64
    }

    /// Predicted tokens of the next ordinary call (prompt re-sent +
    /// completion).
    pub(crate) fn next_call_tokens(&self) -> u64 {
        self.prompt_tokens + self.next_completion_tokens
    }

    /// Predicted tokens of the final-answer call.
    pub(crate) fn answer_tokens(&self) -> u64 {
        self.prompt_tokens + self.answer_completion_tokens
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/call_forecast/call_forecast_test.rs"]
mod tests;
