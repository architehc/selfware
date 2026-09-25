//! Periodic "still waiting on the model" status for slow LLM calls.
//!
//! A single model call against a loaded endpoint can take minutes (one
//! measured call took ~287 s): the request queues and prefills with no tokens
//! yet, or the model streams a long reasoning block the user may not see. Without
//! a heartbeat the session looks hung. `LlmWaitTicker` fires every
//! `LLM_WAIT_TICK` while a call is in flight and produces a
//! [`ProgressEvent::LlmWaiting`] naming the elapsed time, the phase the call is
//! in, and how many completion tokens have arrived so far.
//!
//! It is deliberately cheap: one timer deadline per call, no background task,
//! no locking. Callers `select!` on `LlmWaitTicker::next_due` alongside the
//! chunk receiver (see `recv_or_tick`) and render the event themselves.

use super::progress::ProgressEvent;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::Instant;

/// Cadence of the waiting heartbeat.
pub(crate) const LLM_WAIT_TICK: Duration = Duration::from_secs(15);

/// Where a model call is when the heartbeat fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmWaitPhase {
    /// Request sent, nothing streamed back yet (server queue + prompt prefill).
    Prefill,
    /// Only reasoning output has arrived so far (or a reasoning block is open).
    Reasoning,
    /// Answer content / tool calls are streaming.
    Streaming,
    /// Non-streaming request: the response arrives all at once, so the phase
    /// cannot be observed — reported honestly instead of guessed.
    AwaitingResponse,
    /// A bounded auxiliary call (`ApiClient::side_chat`) named by its
    /// purpose (`context_summary`, `requirements_audit`, …). Its stream is
    /// collected inside the client, so prefill/reasoning/streaming are not
    /// told apart; the event names WHICH side call is in flight instead.
    SideCall(&'static str),
}

impl LlmWaitPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmWaitPhase::Prefill => "prefill",
            LlmWaitPhase::Reasoning => "reasoning",
            LlmWaitPhase::Streaming => "streaming",
            LlmWaitPhase::AwaitingResponse => "awaiting_response",
            LlmWaitPhase::SideCall(_) => "side_call",
        }
    }

    /// The `phase` string an [`ProgressEvent::LlmWaiting`] carries:
    /// [`Self::as_str`], or `side_call:<purpose>` for a side call.
    pub fn label(self) -> String {
        match self {
            LlmWaitPhase::SideCall(purpose) => format!("side_call:{purpose}"),
            other => other.as_str().to_string(),
        }
    }

    /// Classify a streaming call from what it has produced so far.
    pub fn classify(
        content_so_far: &str,
        reasoning_so_far: &str,
        tool_calls_so_far: usize,
        inside_reasoning_block: bool,
    ) -> Self {
        let has_content = !content_so_far.trim().is_empty() || tool_calls_so_far > 0;
        if inside_reasoning_block {
            LlmWaitPhase::Reasoning
        } else if has_content {
            LlmWaitPhase::Streaming
        } else if !reasoning_so_far.is_empty() {
            LlmWaitPhase::Reasoning
        } else {
            LlmWaitPhase::Prefill
        }
    }
}

/// How `tokens_so_far` was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmWaitTokenSource {
    /// Provider-reported completion usage (mid-stream usage chunk).
    Usage,
    /// Counted from the streamed text via `token_count::estimate_content_tokens`.
    Estimate,
    /// Nothing observable yet (non-streaming call).
    None,
}

impl LlmWaitTokenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            LlmWaitTokenSource::Usage => "usage",
            LlmWaitTokenSource::Estimate => "estimate",
            LlmWaitTokenSource::None => "none",
        }
    }
}

/// Count completion tokens seen so far: provider-reported usage when the
/// stream has sent one, otherwise the shared estimator over streamed text.
pub(crate) fn tokens_so_far(
    reported_completion: Option<u32>,
    content: &str,
    reasoning: &str,
) -> (usize, LlmWaitTokenSource) {
    if let Some(n) = reported_completion {
        return (n as usize, LlmWaitTokenSource::Usage);
    }
    if content.is_empty() && reasoning.is_empty() {
        return (0, LlmWaitTokenSource::Estimate);
    }
    let n = crate::token_count::estimate_content_tokens(content)
        + crate::token_count::estimate_content_tokens(reasoning);
    (n, LlmWaitTokenSource::Estimate)
}

/// Await `fut` (a non-streaming model call), invoking `on_tick` with an
/// `awaiting_response` heartbeat every [`LLM_WAIT_TICK`] until it resolves.
pub(crate) async fn await_with_ticks<F, T>(
    fut: F,
    mut ticker: LlmWaitTicker,
    on_tick: impl FnMut(ProgressEvent),
) -> T
where
    F: std::future::Future<Output = T>,
{
    await_with_phase_ticks(fut, &mut ticker, LlmWaitPhase::AwaitingResponse, on_tick).await
}

/// [`await_with_ticks`] with an explicit `phase` and a borrowed ticker, so
/// one heartbeat cadence (and one elapsed clock) can span several awaited
/// attempts — a side call's retry keeps counting from the first request.
pub(crate) async fn await_with_phase_ticks<F, T>(
    fut: F,
    ticker: &mut LlmWaitTicker,
    phase: LlmWaitPhase,
    mut on_tick: impl FnMut(ProgressEvent),
) -> T
where
    F: std::future::Future<Output = T>,
{
    tokio::pin!(fut);
    loop {
        tokio::select! {
            biased;
            out = &mut fut => return out,
            _ = tokio::time::sleep_until(ticker.next_due()) => {
                on_tick(ticker.fire(phase, 0, LlmWaitTokenSource::None));
            }
        }
    }
}

/// Heartbeat state for one in-flight model call.
#[derive(Debug)]
pub(crate) struct LlmWaitTicker {
    started: Instant,
    interval: Duration,
    next_due: Instant,
}

impl LlmWaitTicker {
    pub(crate) fn start() -> Self {
        Self::with_interval(Instant::now(), LLM_WAIT_TICK)
    }

    pub(crate) fn with_interval(started: Instant, interval: Duration) -> Self {
        let interval = interval.max(Duration::from_millis(1));
        Self {
            started,
            interval,
            next_due: started + interval,
        }
    }

    /// When the next heartbeat is due.
    pub(crate) fn next_due(&self) -> Instant {
        self.next_due
    }

    /// Build the heartbeat event and schedule the next one. Missed ticks
    /// (e.g. a long synchronous stretch) are skipped, not replayed, so the
    /// cadence never bursts.
    pub(crate) fn fire(
        &mut self,
        phase: LlmWaitPhase,
        tokens_so_far: usize,
        source: LlmWaitTokenSource,
    ) -> ProgressEvent {
        let now = Instant::now();
        while self.next_due <= now {
            self.next_due += self.interval;
        }
        ProgressEvent::LlmWaiting {
            elapsed_secs: now.saturating_duration_since(self.started).as_secs(),
            phase: phase.label(),
            tokens_so_far,
            tokens_source: source.as_str().to_string(),
        }
    }
}

/// Human spinner text for a heartbeat: `"<base> — prefill 45s"` (or without
/// the seconds when the spinner renders its own elapsed time).
pub(crate) fn spinner_status(
    base: &str,
    event: &ProgressEvent,
    include_elapsed: bool,
) -> Option<String> {
    match event {
        ProgressEvent::LlmWaiting {
            elapsed_secs,
            phase,
            tokens_so_far,
            ..
        } => {
            let mut text = format!("{} — {}", base, phase);
            if include_elapsed {
                text.push_str(&format!(" {}s", elapsed_secs));
            }
            if *tokens_so_far > 0 {
                text.push_str(&format!(", {} tokens", tokens_so_far));
            }
            Some(text)
        }
        _ => None,
    }
}

/// One step of a heartbeat-aware receive loop.
#[derive(Debug)]
pub(crate) enum RecvOrTick<T> {
    /// A chunk arrived, or `None` when the channel closed.
    Item(Option<T>),
    /// No chunk arrived before the heartbeat deadline; the caller should
    /// [`LlmWaitTicker::fire`] and loop.
    Tick,
}

/// Wait for the next chunk OR the next heartbeat deadline, whichever comes
/// first. An overdue heartbeat is reported BEFORE the next chunk is taken, so
/// a fast continuous stream (e.g. a minutes-long reasoning block) still gets
/// its heartbeat instead of starving the timer branch. Cancel-safe:
/// `mpsc::Receiver::recv` loses nothing when the tick branch wins.
pub(crate) async fn recv_or_tick<T>(
    rx: &mut mpsc::Receiver<T>,
    ticker: &LlmWaitTicker,
) -> RecvOrTick<T> {
    if Instant::now() >= ticker.next_due() {
        return RecvOrTick::Tick;
    }
    tokio::select! {
        biased;
        item = rx.recv() => RecvOrTick::Item(item),
        _ = tokio::time::sleep_until(ticker.next_due()) => RecvOrTick::Tick,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/llm_wait/llm_wait_test.rs"]
mod tests;
