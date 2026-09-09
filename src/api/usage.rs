//! Per-request accounting survives retries, cancellation and failed responses.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::types::Usage;

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

    pub fn record(&self, usage: &Usage) {
        let mut state = self.ledger.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.generation != self.generation {
            return;
        }
        let Some(attempt) = state.attempts.get_mut(self.index) else {
            return;
        };
        let prior = attempt.usage.clone().unwrap_or_default();
        // SSE usage is cumulative for this request; repeated snapshots must
        // not bill twice, and an incomplete later snapshot must not erase it.
        let current = Usage {
            prompt_tokens: prior.prompt_tokens.max(usage.prompt_tokens),
            completion_tokens: prior.completion_tokens.max(usage.completion_tokens),
            total_tokens: prior.total_tokens.max(usage.total_tokens).max(
                prior
                    .prompt_tokens
                    .max(usage.prompt_tokens)
                    .saturating_add(prior.completion_tokens.max(usage.completion_tokens)),
            ),
            cost: match (
                prior.cost,
                usage.cost.filter(|c| c.is_finite() && *c >= 0.0),
            ) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        };
        let delta = Usage {
            prompt_tokens: current.prompt_tokens.saturating_sub(prior.prompt_tokens),
            completion_tokens: current
                .completion_tokens
                .saturating_sub(prior.completion_tokens),
            total_tokens: current.total_tokens.saturating_sub(prior.total_tokens),
            cost: current.cost.map(|c| c - prior.cost.unwrap_or(0.0)),
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
        if fields.prompt && fields.completion {
            return;
        }
        let reported = attempt.usage.clone().unwrap_or_default();
        let prompt = if fields.prompt { 0 } else { prompt_tokens };
        let completion = if fields.completion {
            0
        } else {
            completion_tokens
        };
        let estimated = Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
            // A reported total already includes any missing components.
            total_tokens: if fields.total {
                0
            } else {
                prompt.saturating_add(completion)
            },
            cost: None,
        };
        if reported.total_tokens == 0
            && estimated.total_tokens == 0
            && prompt == 0
            && completion == 0
        {
            return;
        }
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
fn add_usage(total: &mut Usage, additional: &Usage) {
    total.prompt_tokens = total.prompt_tokens.saturating_add(additional.prompt_tokens);
    total.completion_tokens = total
        .completion_tokens
        .saturating_add(additional.completion_tokens);
    total.total_tokens = total.total_tokens.saturating_add(additional.total_tokens);
    total.cost = match (total.cost, additional.cost) {
        (Some(a), Some(b)) => Some(a + b),
        (a, b) => a.or(b),
    };
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
}
