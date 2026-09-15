//! Per-request accounting survives retries, cancellation and failed responses.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::types::{CompletionTokensDetails, PromptTokensDetails, Usage};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttemptOutcome {
    InFlight,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct UsageCoverage {
    pub prompt: bool,
    pub completion: bool,
    pub total: bool,
}

impl UsageCoverage {
    pub(crate) fn all() -> Self {
        Self {
            prompt: true,
            completion: true,
            total: true,
        }
    }
    pub(crate) fn intersect(self, other: Self) -> Self {
        Self {
            prompt: self.prompt && other.prompt,
            completion: self.completion && other.completion,
            total: self.total && other.total,
        }
    }
}

/// Missing usage means the provider did not report it, never a zero-cost claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageAttempt {
    pub model: String,
    pub outcome: AttemptOutcome,
    pub http_status: Option<u16>,
    pub usage: Option<Usage>,
    /// Which token fields the provider actually supplied (zero is a valid value).
    pub reported_fields: UsageCoverage,
    /// Measured token-count fallback, kept separate from provider claims.
    pub estimated_usage: Option<Usage>,
    /// Exact raw usage reported by the provider before any budget derivations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_usage: Option<Usage>,
}

#[derive(Debug, Default)]
struct State {
    generation: u64,
    unattributed_usage: bool,
    attempts: Vec<UsageAttempt>,
    total: Usage,
    pending: Usage,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct UsageLedger(Arc<Mutex<State>>);

impl UsageLedger {
    pub fn begin(&self, model: &str) -> AttemptGuard {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let index = state.attempts.len();
        state.attempts.push(UsageAttempt {
            model: model.to_string(),
            outcome: AttemptOutcome::InFlight,
            http_status: None,
            usage: None,
            reported_fields: UsageCoverage::default(),
            estimated_usage: None,
            raw_usage: None,
        });
        AttemptGuard {
            ledger: self.clone(),
            index,
            generation: state.generation,
        }
    }

    pub fn total(&self) -> Usage {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .total
            .clone()
    }

    pub fn pending(&self) -> Usage {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending
            .clone()
    }

    pub fn take_pending(&self) -> Usage {
        std::mem::take(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()).pending)
    }

    pub fn attempts(&self) -> Vec<UsageAttempt> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .attempts
            .clone()
    }

    /// Seed a resumed run, or include an agent's measured fallback estimates.
    /// This does not create provider-reported usage or a second pending delta.
    pub fn ensure_budget_floor(&self, tokens: usize, cost: f64) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if tokens > state.total.total_tokens || cost > state.total.cost.unwrap_or(0.0) + 1e-12 {
            state.unattributed_usage = true;
        }
        state.total.total_tokens = state.total.total_tokens.max(tokens);
        if cost > state.total.cost.unwrap_or(0.0) {
            state.total.cost = Some(cost);
        }
    }

    pub fn mark_restored(&self) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unattributed_usage = true;
    }

    pub fn cost_status(&self) -> (bool, usize) {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let unmetered = state
            .attempts
            .iter()
            .filter(|attempt| {
                attempt
                    .usage
                    .as_ref()
                    .and_then(|usage| usage.cost)
                    .is_none()
            })
            .count();
        (!state.unattributed_usage && unmetered == 0, unmetered)
    }

    pub fn reset(&self) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let generation = state.generation.wrapping_add(1);
        *state = State {
            generation,
            ..State::default()
        };
    }
}

/// Dropping a request future keeps its latest usage and marks it failed.
#[derive(Debug)]
pub(crate) struct AttemptGuard {
    ledger: UsageLedger,
    index: usize,
    generation: u64,
}

/// A stable reference to one request, so concurrent calls cannot be mixed
/// into another operation's response usage.
#[derive(Debug, Clone)]
pub(crate) struct UsageReceipt {
    ledger: UsageLedger,
    index: usize,
    generation: u64,
}

impl UsageReceipt {
    fn attempt(&self) -> Option<UsageAttempt> {
        let state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return None;
        }
        state.attempts.get(self.index).cloned()
    }
    pub fn usage(&self) -> Option<Usage> {
        let state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return None;
        }
        state.attempts.get(self.index)?.usage.clone()
    }
}

pub(crate) fn accounted_receipts(receipts: &[UsageReceipt]) -> Usage {
    let mut total = Usage {
        cost: Some(0.0),
        ..Usage::default()
    };
    for receipt in receipts {
        let attempt = receipt.attempt();
        let mut usage = attempt
            .as_ref()
            .and_then(|attempt| attempt.usage.clone())
            .unwrap_or_default();
        if let Some(estimated) = attempt.and_then(|attempt| attempt.estimated_usage) {
            // Estimates never assert a price; preserve only the reported cost.
            let cost = usage.cost;
            add_usage(&mut usage, &estimated);
            usage.cost = cost;
        }
        add_response_usage(&mut total, &usage);
    }
    total
}

pub(crate) fn receipt_coverage(receipts: &[UsageReceipt]) -> UsageCoverage {
    receipts
        .iter()
        .fold(UsageCoverage::all(), |coverage, receipt| {
            coverage.intersect(
                receipt
                    .attempt()
                    .map(|attempt| attempt.reported_fields)
                    .unwrap_or_default(),
            )
        })
}

/// A response can claim a total cost only if every contributing request
/// reported one. Known partial charges remain in the run ledger for caps.
pub(crate) fn aggregate_receipts(receipts: &[UsageReceipt]) -> Usage {
    let mut total = Usage {
        cost: Some(0.0),
        ..Usage::default()
    };
    for receipt in receipts {
        add_response_usage(&mut total, &receipt.usage().unwrap_or_default());
    }
    total
}

pub(crate) fn add_response_usage(total: &mut Usage, additional: &Usage) {
    let complete_cost = total.cost.zip(additional.cost).map(|(a, b)| a + b);
    add_usage(total, additional);
    total.cost = complete_cost;
}

impl AttemptGuard {
    pub fn receipt(&self) -> UsageReceipt {
        UsageReceipt {
            ledger: self.ledger.clone(),
            index: self.index,
            generation: self.generation,
        }
    }
    pub fn status(&self, status: u16) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        if let Some(attempt) = state.attempts.get_mut(self.index) {
            attempt.http_status = Some(status);
        }
    }

    /// Record raw provider usage for this attempt, preserving the exact raw report
    /// in `attempt.raw_usage` while deriving full budget charges from available
    /// token counts (prompt + completion) in `attempt.usage` and the ledger.
    pub fn record(&self, usage: &Usage) {
        // Validate raw per-attempt provider report before normalization or aggregation (Rule 3)
        if !usage.is_reconciled() {
            tracing::warn!(
                "Provider reported unreconciled raw token usage on attempt: prompt={} + completion={} != total={}",
                usage.prompt_tokens,
                usage.completion_tokens,
                usage.total_tokens
            );
        }

        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        let Some(attempt) = state.attempts.get_mut(self.index) else {
            return;
        };

        // Preserve raw provider claims separately from derived budget charges.
        let prior_raw = attempt.raw_usage.clone().unwrap_or_default();
        let raw = Usage {
            prompt_tokens: prior_raw.prompt_tokens.max(usage.prompt_tokens),
            completion_tokens: prior_raw.completion_tokens.max(usage.completion_tokens),
            total_tokens: prior_raw.total_tokens.max(usage.total_tokens),
            cost: match (
                prior_raw.cost,
                usage.cost.filter(|c| c.is_finite() && *c >= 0.0),
            ) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            reasoning_tokens: match (prior_raw.reasoning_tokens, usage.reasoning_tokens) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            completion_tokens_details: match (
                &prior_raw.completion_tokens_details,
                &usage.completion_tokens_details,
            ) {
                (Some(p), Some(u)) => Some(CompletionTokensDetails {
                    reasoning_tokens: match (p.reasoning_tokens, u.reasoning_tokens) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    },
                    accepted_prediction_tokens: match (
                        p.accepted_prediction_tokens,
                        u.accepted_prediction_tokens,
                    ) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    },
                    rejected_prediction_tokens: match (
                        p.rejected_prediction_tokens,
                        u.rejected_prediction_tokens,
                    ) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    },
                }),
                (Some(p), None) => Some(p.clone()),
                (None, Some(u)) => Some(u.clone()),
                (None, None) => None,
            },
            prompt_tokens_details: match (
                &prior_raw.prompt_tokens_details,
                &usage.prompt_tokens_details,
            ) {
                (Some(p), Some(u)) => Some(PromptTokensDetails {
                    cached_tokens: match (p.cached_tokens, u.cached_tokens) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    },
                }),
                (Some(p), None) => Some(p.clone()),
                (None, Some(u)) => Some(u.clone()),
                (None, None) => None,
            },
        };
        attempt.raw_usage = Some(raw);

        let prior = attempt.usage.clone().unwrap_or_default();
        // SSE usage is cumulative for this request; repeated snapshots must
        // not bill twice, and an incomplete later snapshot must not erase it.
        let current_prompt = prior.prompt_tokens.max(usage.prompt_tokens);
        let current_completion = prior.completion_tokens.max(usage.completion_tokens);
        let available_total = current_prompt.saturating_add(current_completion);
        let current_total = prior
            .total_tokens
            .max(usage.total_tokens)
            .max(available_total);

        let current_reasoning = match (prior.reasoning_tokens, usage.reasoning_tokens) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };

        let current_completion_details = match (
            &prior.completion_tokens_details,
            &usage.completion_tokens_details,
        ) {
            (Some(p), Some(u)) => Some(CompletionTokensDetails {
                reasoning_tokens: match (p.reasoning_tokens, u.reasoning_tokens) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                },
                accepted_prediction_tokens: match (
                    p.accepted_prediction_tokens,
                    u.accepted_prediction_tokens,
                ) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                },
                rejected_prediction_tokens: match (
                    p.rejected_prediction_tokens,
                    u.rejected_prediction_tokens,
                ) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                },
            }),
            (Some(p), None) => Some(p.clone()),
            (None, Some(u)) => Some(u.clone()),
            (None, None) => None,
        };

        let current_prompt_details =
            match (&prior.prompt_tokens_details, &usage.prompt_tokens_details) {
                (Some(p), Some(u)) => Some(PromptTokensDetails {
                    cached_tokens: match (p.cached_tokens, u.cached_tokens) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    },
                }),
                (Some(p), None) => Some(p.clone()),
                (None, Some(u)) => Some(u.clone()),
                (None, None) => None,
            };

        let current = Usage {
            prompt_tokens: current_prompt,
            completion_tokens: current_completion,
            total_tokens: current_total,
            cost: match (
                prior.cost,
                usage.cost.filter(|c| c.is_finite() && *c >= 0.0),
            ) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
            reasoning_tokens: current_reasoning,
            completion_tokens_details: current_completion_details.clone(),
            prompt_tokens_details: current_prompt_details.clone(),
        };

        let delta_reasoning = match (current.reasoning_tokens, prior.reasoning_tokens) {
            (Some(curr), Some(prev)) => {
                let diff = curr.saturating_sub(prev);
                if diff > 0 {
                    Some(diff)
                } else {
                    None
                }
            }
            (Some(curr), None) => Some(curr),
            (None, _) => None,
        };

        let delta_completion_details = match (
            &current.completion_tokens_details,
            &prior.completion_tokens_details,
        ) {
            (Some(curr), Some(prev)) => {
                let r = match (curr.reasoning_tokens, prev.reasoning_tokens) {
                    (Some(c), Some(p)) if c > p => Some(c - p),
                    (Some(c), None) => Some(c),
                    _ => None,
                };
                let a = match (
                    curr.accepted_prediction_tokens,
                    prev.accepted_prediction_tokens,
                ) {
                    (Some(c), Some(p)) if c > p => Some(c - p),
                    (Some(c), None) => Some(c),
                    _ => None,
                };
                let rej = match (
                    curr.rejected_prediction_tokens,
                    prev.rejected_prediction_tokens,
                ) {
                    (Some(c), Some(p)) if c > p => Some(c - p),
                    (Some(c), None) => Some(c),
                    _ => None,
                };
                if r.is_some() || a.is_some() || rej.is_some() {
                    Some(CompletionTokensDetails {
                        reasoning_tokens: r,
                        accepted_prediction_tokens: a,
                        rejected_prediction_tokens: rej,
                    })
                } else {
                    None
                }
            }
            (Some(curr), None) => Some(curr.clone()),
            (None, _) => None,
        };

        let delta_prompt_details =
            match (&current.prompt_tokens_details, &prior.prompt_tokens_details) {
                (Some(curr), Some(prev)) => {
                    let c = match (curr.cached_tokens, prev.cached_tokens) {
                        (Some(curr_c), Some(prev_c)) if curr_c > prev_c => Some(curr_c - prev_c),
                        (Some(curr_c), None) => Some(curr_c),
                        _ => None,
                    };
                    if c.is_some() {
                        Some(PromptTokensDetails { cached_tokens: c })
                    } else {
                        None
                    }
                }
                (Some(curr), None) => Some(curr.clone()),
                (None, _) => None,
            };

        let delta = Usage {
            prompt_tokens: current.prompt_tokens.saturating_sub(prior.prompt_tokens),
            completion_tokens: current
                .completion_tokens
                .saturating_sub(prior.completion_tokens),
            total_tokens: current.total_tokens.saturating_sub(prior.total_tokens),
            cost: current.cost.map(|c| c - prior.cost.unwrap_or(0.0)),
            reasoning_tokens: delta_reasoning,
            completion_tokens_details: delta_completion_details,
            prompt_tokens_details: delta_prompt_details,
        };
        attempt.usage = Some(current);
        attempt.reported_fields = UsageCoverage::all();
        add_usage(&mut state.total, &delta);
        add_usage(&mut state.pending, &delta);
    }

    pub fn record_json(&self, body: &str) -> Option<Usage> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(usage) = value
                .get("usage")
                .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok())
            {
                self.record(&usage);
                let fields = &value["usage"];
                let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
                if state.generation == self.generation {
                    if let Some(attempt) = state.attempts.get_mut(self.index) {
                        attempt.reported_fields = UsageCoverage {
                            prompt: fields
                                .get("prompt_tokens")
                                .is_some_and(serde_json::Value::is_u64),
                            completion: fields
                                .get("completion_tokens")
                                .is_some_and(serde_json::Value::is_u64),
                            total: fields
                                .get("total_tokens")
                                .is_some_and(serde_json::Value::is_u64),
                        };
                    }
                }
                return Some(usage);
            }
        }
        None
    }

    /// Charge measured fallback only after a response was received, and only
    /// for missing provider fields. This does not turn missing usage into zero.
    /// If the provider omitted total_tokens or reported a total smaller than
    /// the component sum, the difference is charged to protect budget enforcement.
    pub fn record_fallback(&self, prompt_tokens: usize, completion_tokens: usize) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        let Some(attempt) = state.attempts.get_mut(self.index) else {
            return;
        };
        if attempt.estimated_usage.is_some() {
            return;
        }
        let fields = attempt.reported_fields;
        let reported = attempt.usage.clone().unwrap_or_default();
        let prompt = if fields.prompt { 0 } else { prompt_tokens };
        let completion = if fields.completion {
            0
        } else {
            completion_tokens
        };
        let effective_prompt = if fields.prompt {
            reported.prompt_tokens
        } else {
            prompt_tokens
        };
        let effective_completion = if fields.completion {
            reported.completion_tokens
        } else {
            completion_tokens
        };
        let available_total = effective_prompt.saturating_add(effective_completion);
        let missing_total = if fields.total && reported.total_tokens >= available_total {
            0
        } else {
            available_total.saturating_sub(reported.total_tokens)
        };
        if prompt == 0 && completion == 0 && missing_total == 0 {
            return;
        }
        let estimated = Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            total_tokens: missing_total,
            cost: None,
            ..Default::default()
        };
        attempt.estimated_usage = Some(estimated.clone());
        add_usage(&mut state.total, &estimated);
        add_usage(&mut state.pending, &estimated);
    }

    pub fn complete(&self) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        if let Some(attempt) = state.attempts.get_mut(self.index) {
            if attempt.outcome == AttemptOutcome::InFlight {
                attempt.outcome = AttemptOutcome::Completed;
            }
        }
    }

    pub fn fail(&self) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        if let Some(attempt) = state.attempts.get_mut(self.index) {
            attempt.outcome = AttemptOutcome::Failed;
        }
    }
}

impl Drop for AttemptGuard {
    fn drop(&mut self) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        if let Some(attempt) = state.attempts.get_mut(self.index) {
            if attempt.outcome == AttemptOutcome::InFlight {
                attempt.outcome = AttemptOutcome::Failed;
            }
        }
    }
}

/// Sum known charges for hard-budget accounting; this is not a complete-cost
/// assertion for a multi-attempt response (use `add_response_usage` there).
pub(crate) fn add_usage(total: &mut Usage, additional: &Usage) {
    total.prompt_tokens = total.prompt_tokens.saturating_add(additional.prompt_tokens);
    total.completion_tokens = total
        .completion_tokens
        .saturating_add(additional.completion_tokens);
    total.total_tokens = total.total_tokens.saturating_add(additional.total_tokens);
    total.cost = match (total.cost, additional.cost) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
    total.reasoning_tokens = match (total.reasoning_tokens, additional.reasoning_tokens) {
        (Some(a), Some(b)) => Some(a.saturating_add(b)),
        (a, b) => a.or(b),
    };
    if let Some(add_details) = &additional.completion_tokens_details {
        let details = total
            .completion_tokens_details
            .get_or_insert_with(Default::default);
        if let Some(r) = add_details.reasoning_tokens {
            details.reasoning_tokens =
                Some(details.reasoning_tokens.unwrap_or(0).saturating_add(r));
        }
        if let Some(a) = add_details.accepted_prediction_tokens {
            details.accepted_prediction_tokens = Some(
                details
                    .accepted_prediction_tokens
                    .unwrap_or(0)
                    .saturating_add(a),
            );
        }
        if let Some(rej) = add_details.rejected_prediction_tokens {
            details.rejected_prediction_tokens = Some(
                details
                    .rejected_prediction_tokens
                    .unwrap_or(0)
                    .saturating_add(rej),
            );
        }
    }
    if let Some(add_prompt) = &additional.prompt_tokens_details {
        let details = total
            .prompt_tokens_details
            .get_or_insert_with(Default::default);
        if let Some(c) = add_prompt.cached_tokens {
            details.cached_tokens = Some(details.cached_tokens.unwrap_or(0).saturating_add(c));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_stream_snapshots_charge_only_deltas_and_survive_abort() {
        let ledger = UsageLedger::default();
        {
            let attempt = ledger.begin("m");
            let usage = Usage {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
                cost: Some(0.1),
                ..Default::default()
            };
            attempt.record(&usage);
            assert_eq!(ledger.take_pending().total_tokens, 15);
            attempt.record(&usage);
            assert_eq!(ledger.take_pending().total_tokens, 0);
            attempt.record(&Usage {
                completion_tokens: 20,
                total_tokens: 25,
                ..usage
            });
        }
        assert_eq!(ledger.total().total_tokens, 25);
        assert_eq!(ledger.take_pending().total_tokens, 10);
        assert_eq!(ledger.attempts()[0].outcome, AttemptOutcome::Failed);
        assert_eq!(ledger.attempts()[0].usage.as_ref().unwrap().cost, Some(0.1));
    }

    #[test]
    fn task_reset_does_not_let_an_old_stream_charge_the_next_task() {
        let ledger = UsageLedger::default();
        let old = ledger.begin("old");
        ledger.reset();
        let new = ledger.begin("new");
        old.record(&Usage {
            total_tokens: 99,
            ..Usage::default()
        });
        drop(old);
        assert_eq!(ledger.total().total_tokens, 0);
        assert_eq!(ledger.attempts()[0].outcome, AttemptOutcome::InFlight);
        assert!(ledger.attempts()[0].usage.is_none());
        new.complete();
    }

    #[test]
    fn unknown_failed_usage_stays_unknown() {
        let ledger = UsageLedger::default();
        drop(ledger.begin("m"));
        let attempts = ledger.attempts();
        assert_eq!(attempts[0].outcome, AttemptOutcome::Failed);
        assert!(attempts[0].usage.is_none());
    }

    #[test]
    fn missing_and_zero_and_inconsistent_totals_derive_budget_charges() {
        let ledger = UsageLedger::default();

        // 1. Zero/missing total in ordinary response: prompt 100, completion 20, total 0.
        {
            let attempt = ledger.begin("m");
            let raw = Usage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 0,
                ..Default::default()
            };
            attempt.record(&raw);
            let attempts = ledger.attempts();
            assert_eq!(attempts[0].raw_usage.as_ref().unwrap().total_tokens, 0);
            assert_eq!(attempts[0].usage.as_ref().unwrap().total_tokens, 120);
            assert_eq!(ledger.total().total_tokens, 120);
            assert_eq!(ledger.take_pending().total_tokens, 120);

            // Fallback does not double-count
            attempt.record_fallback(100, 20);
            assert_eq!(ledger.total().total_tokens, 120);
            assert!(attempts[0].estimated_usage.is_none());
            attempt.complete();
        }

        // 2. Inconsistent total (underreported total 50 < 100 + 20):
        {
            let attempt = ledger.begin("m");
            let raw = Usage {
                prompt_tokens: 100,
                completion_tokens: 20,
                total_tokens: 50,
                ..Default::default()
            };
            attempt.record(&raw);
            let attempts = ledger.attempts();
            assert_eq!(attempts[1].raw_usage.as_ref().unwrap().total_tokens, 50);
            assert_eq!(attempts[1].usage.as_ref().unwrap().total_tokens, 120);
            assert_eq!(ledger.total().total_tokens, 240); // 120 + 120
            assert_eq!(ledger.take_pending().total_tokens, 120);
            attempt.complete();
        }

        // 3. Streaming multi-chunk with zero total progressing to final total:
        {
            let attempt = ledger.begin("m");
            // Chunk 1: prompt 100, completion 10, total 0
            attempt.record(&Usage {
                prompt_tokens: 100,
                completion_tokens: 10,
                total_tokens: 0,
                ..Default::default()
            });
            assert_eq!(ledger.take_pending().total_tokens, 110);
            assert_eq!(ledger.total().total_tokens, 350); // 240 + 110

            // Chunk 2: prompt 100, completion 25, total 0
            attempt.record(&Usage {
                prompt_tokens: 100,
                completion_tokens: 25,
                total_tokens: 0,
                ..Default::default()
            });
            assert_eq!(ledger.take_pending().total_tokens, 15);
            assert_eq!(ledger.total().total_tokens, 365); // 240 + 125

            // Chunk 3: prompt 100, completion 30, total 135 (extra tokens reported)
            attempt.record(&Usage {
                prompt_tokens: 100,
                completion_tokens: 30,
                total_tokens: 135,
                ..Default::default()
            });
            assert_eq!(ledger.take_pending().total_tokens, 10);
            assert_eq!(ledger.total().total_tokens, 375); // 240 + 135

            let attempts = ledger.attempts();
            assert_eq!(attempts[2].raw_usage.as_ref().unwrap().total_tokens, 135);
            assert_eq!(attempts[2].usage.as_ref().unwrap().total_tokens, 135);
            attempt.complete();
        }
    }

    #[test]
    fn record_json_omitted_total_retains_raw_and_charges_derived() {
        let ledger = UsageLedger::default();
        let attempt = ledger.begin("m");
        let body = r#"{"usage": {"prompt_tokens": 80, "completion_tokens": 15}}"#;
        let recorded = attempt.record_json(body);
        assert!(recorded.is_some());

        let attempts = ledger.attempts();
        assert!(!attempts[0].reported_fields.total);
        assert!(attempts[0].reported_fields.prompt);
        assert!(attempts[0].reported_fields.completion);

        assert_eq!(attempts[0].raw_usage.as_ref().unwrap().total_tokens, 0);
        assert_eq!(attempts[0].usage.as_ref().unwrap().total_tokens, 95);
        assert_eq!(ledger.total().total_tokens, 95);

        // record_fallback should recognize the missing total is already charged and not double bill
        attempt.record_fallback(80, 15);
        assert!(attempts[0].estimated_usage.is_none());
        assert_eq!(ledger.total().total_tokens, 95);
    }

    #[test]
    fn reasoning_and_cache_details_telemetry_propagation_retries_and_aborts() {
        let ledger = UsageLedger::default();

        // Attempt 1: streaming snapshots then aborted
        {
            let attempt = ledger.begin("m");
            let chunk1 = Usage {
                prompt_tokens: 50,
                completion_tokens: 10,
                total_tokens: 60,
                reasoning_tokens: Some(10),
                completion_tokens_details: Some(CompletionTokensDetails {
                    reasoning_tokens: Some(10),
                    accepted_prediction_tokens: Some(5),
                    rejected_prediction_tokens: Some(1),
                }),
                prompt_tokens_details: Some(PromptTokensDetails {
                    cached_tokens: Some(30),
                }),
                ..Default::default()
            };
            attempt.record(&chunk1);

            let pending = ledger.take_pending();
            assert_eq!(pending.reasoning_tokens, Some(10));
            assert_eq!(
                pending
                    .completion_tokens_details
                    .as_ref()
                    .unwrap()
                    .accepted_prediction_tokens,
                Some(5)
            );
            assert_eq!(
                pending
                    .prompt_tokens_details
                    .as_ref()
                    .unwrap()
                    .cached_tokens,
                Some(30)
            );

            // Chunk 2: reasoning and accepted increase
            let chunk2 = Usage {
                prompt_tokens: 50,
                completion_tokens: 20,
                total_tokens: 70,
                reasoning_tokens: Some(18),
                completion_tokens_details: Some(CompletionTokensDetails {
                    reasoning_tokens: Some(18),
                    accepted_prediction_tokens: Some(8),
                    rejected_prediction_tokens: Some(1),
                }),
                prompt_tokens_details: Some(PromptTokensDetails {
                    cached_tokens: Some(30),
                }),
                ..Default::default()
            };
            attempt.record(&chunk2);

            let pending2 = ledger.take_pending();
            assert_eq!(pending2.reasoning_tokens, Some(8)); // 18 - 10
            assert_eq!(
                pending2
                    .completion_tokens_details
                    .as_ref()
                    .unwrap()
                    .accepted_prediction_tokens,
                Some(3) // 8 - 5
            );
            // prompt cached did not increase, so not in delta
            assert!(pending2.prompt_tokens_details.is_none());

            // Attempt dropped without complete() -> aborts
            drop(attempt);
        }

        let total_after_abort = ledger.total();
        assert_eq!(total_after_abort.total_tokens, 70);
        assert_eq!(total_after_abort.reasoning_tokens, Some(18));
        assert_eq!(
            total_after_abort
                .completion_tokens_details
                .as_ref()
                .unwrap()
                .reasoning_tokens,
            Some(18)
        );
        assert_eq!(
            total_after_abort
                .completion_tokens_details
                .as_ref()
                .unwrap()
                .accepted_prediction_tokens,
            Some(8)
        );
        assert_eq!(
            total_after_abort
                .prompt_tokens_details
                .as_ref()
                .unwrap()
                .cached_tokens,
            Some(30)
        );
        assert_eq!(ledger.attempts()[0].outcome, AttemptOutcome::Failed);

        // Attempt 2 (retry)
        {
            let attempt2 = ledger.begin("m");
            let retry_usage = Usage {
                prompt_tokens: 40,
                completion_tokens: 15,
                total_tokens: 55,
                reasoning_tokens: Some(5),
                completion_tokens_details: Some(CompletionTokensDetails {
                    reasoning_tokens: Some(5),
                    accepted_prediction_tokens: Some(2),
                    rejected_prediction_tokens: None,
                }),
                prompt_tokens_details: Some(PromptTokensDetails {
                    cached_tokens: Some(20),
                }),
                ..Default::default()
            };
            attempt2.record(&retry_usage);
            attempt2.complete();
        }

        let final_total = ledger.total();
        assert_eq!(final_total.total_tokens, 125); // 70 + 55
        assert_eq!(final_total.reasoning_tokens, Some(23)); // 18 + 5
        assert_eq!(
            final_total
                .completion_tokens_details
                .as_ref()
                .unwrap()
                .accepted_prediction_tokens,
            Some(10) // 8 + 2
        );
        assert_eq!(
            final_total
                .prompt_tokens_details
                .as_ref()
                .unwrap()
                .cached_tokens,
            Some(50) // 30 + 20
        );

        // Verify individual attempts
        let attempts = ledger.attempts();
        assert_eq!(attempts.len(), 2);
        assert_eq!(
            attempts[0].usage.as_ref().unwrap().reasoning_tokens,
            Some(18)
        );
        assert_eq!(
            attempts[1].usage.as_ref().unwrap().reasoning_tokens,
            Some(5)
        );
    }
}
