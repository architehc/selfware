//! Session-scope token and cost totals for the interactive REPL.
//!
//! The per-task accumulators (`cumulative_token_usage`, `cumulative_cost_usd`
//! and the API client's usage ledger) are reset at every task boundary, so
//! in a REPL they describe only the LAST message. `/cost` used to mix that
//! per-task total with the process-global main-loop counter
//! (`output::get_total_tokens`), and `/quit` printed the global counter
//! alone — three different "totals" for one session (0.9.1 field test:
//! 139287 / 156657 / 207115). Every session number now comes from one fold
//! of the same per-task accumulators the run summary reads, plus a
//! main-loop share measured at the same call sites that feed the stream
//! usage display.

/// One task's measured usage, read from the same accumulators as
/// `RunSummary::total_tokens` / `RunSummary::cost_usd`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct TaskUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    /// Provider-reported USD cost; `None` when the endpoint reported none.
    pub cost_usd: Option<f64>,
    /// Every attempt's cost is known.
    pub cost_complete: bool,
    /// Attempts that did not report a provider cost.
    pub unmetered_attempts: usize,
}

/// Session totals: every finished task folded together with the current one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SessionUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    /// All model calls this session (main loop and side calls), measured.
    pub total_tokens: usize,
    /// Provider-reported USD cost, `None` when no call reported one.
    pub cost_usd: Option<f64>,
    /// Every attempt of every task reported its cost.
    pub cost_complete: bool,
    pub unmetered_attempts: usize,
    /// Tasks that consumed tokens.
    pub tasks: usize,
}

impl Default for SessionUsage {
    fn default() -> Self {
        Self {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cost_usd: None,
            // Nothing has been billed yet, so nothing is missing.
            cost_complete: true,
            unmetered_attempts: 0,
            tasks: 0,
        }
    }
}

impl SessionUsage {
    /// Fold one task's usage into the session totals.
    pub(crate) fn with_task(&self, task: &TaskUsage) -> Self {
        let cost_usd = match (self.cost_usd, task.cost_usd) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
        };
        let used = task.total_tokens > 0;
        Self {
            prompt_tokens: self.prompt_tokens.saturating_add(task.prompt_tokens),
            completion_tokens: self
                .completion_tokens
                .saturating_add(task.completion_tokens),
            total_tokens: self.total_tokens.saturating_add(task.total_tokens),
            cost_usd,
            cost_complete: self.cost_complete && (task.cost_complete || !used),
            unmetered_attempts: self
                .unmetered_attempts
                .saturating_add(task.unmetered_attempts),
            tasks: self.tasks + usize::from(used),
        }
    }

    /// Tokens not attributable to main-loop calls: side calls (compaction,
    /// requirements audit, verification, planning, synthesis) plus estimated
    /// usage for calls whose endpoint reported none.
    pub(crate) fn side_tokens(&self, main_loop_tokens: u64) -> usize {
        self.total_tokens
            .saturating_sub(usize::try_from(main_loop_tokens).unwrap_or(usize::MAX))
    }

    /// `cost $X` / `known cost $X (incomplete …)` / `cost not tracked …`.
    pub(crate) fn cost_phrase(&self) -> String {
        cost_phrase(self.cost_usd, self.cost_complete, self.unmetered_attempts)
    }

    /// Compact status-bar cost: only a provider-reported amount, prefixed
    /// with `≥` when billing is incomplete; `None` when nothing was reported
    /// (the status bar then shows no dollar figure at all).
    pub(crate) fn status_bar_cost(&self) -> Option<String> {
        let cost = self.cost_usd?;
        Some(if self.cost_complete {
            format!("${cost:.2}")
        } else {
            format!("≥${cost:.2}")
        })
    }

    /// `/cost` body: one measured total with its breakdowns.
    pub(crate) fn render_cost_lines(&self, main_loop_tokens: u64) -> Vec<String> {
        let tasks = match self.tasks {
            1 => "1 task".to_string(),
            n => format!("{n} tasks"),
        };
        let mut lines = vec![
            format!("Token usage (session, {tasks})"),
            format!("Prompt:     {:>10}", self.prompt_tokens),
            format!("Completion: {:>10}", self.completion_tokens),
        ];
        let split = self.prompt_tokens.saturating_add(self.completion_tokens);
        if self.total_tokens > split {
            lines.push(format!(
                "Unsplit:    {:>10}  (restored or estimated usage without a prompt/completion split)",
                self.total_tokens - split
            ));
        }
        lines.push(format!(
            "Total:      {:>10}  (all model calls, measured)",
            self.total_tokens
        ));
        lines.push(format!(
            "  main loop:  {:>10}",
            main_loop_tokens.min(self.total_tokens as u64)
        ));
        lines.push(format!(
            "  side calls: {:>10}  (compaction, audit, verification, planning; incl. estimates for calls without a usage report)",
            self.side_tokens(main_loop_tokens)
        ));
        lines.push(self.cost_phrase());
        lines
    }

    /// `/quit` line: the same numbers as `/cost`, on one line. `None` when
    /// the session consumed no tokens.
    pub(crate) fn render_quit_line(&self, main_loop_tokens: u64) -> Option<String> {
        if self.total_tokens == 0 {
            return None;
        }
        Some(format!(
            "session tokens: {} prompt + {} completion = {} total (main loop {} · side calls {}) · {}",
            self.prompt_tokens,
            self.completion_tokens,
            self.total_tokens,
            main_loop_tokens.min(self.total_tokens as u64),
            self.side_tokens(main_loop_tokens),
            self.cost_phrase()
        ))
    }
}

/// Cost wording shared by `/cost`, `/quit` and the run summary's cost line:
/// a dollar amount only when the provider reported one (AGENTS.md rule 3).
pub(crate) fn cost_phrase(cost_usd: Option<f64>, complete: bool, unmetered: usize) -> String {
    match cost_usd {
        Some(cost) if complete => format!("cost ${cost:.4}"),
        Some(cost) => format!(
            "known cost ${cost:.4} (incomplete billing; {unmetered} attempts without reported cost)"
        ),
        None => "cost not tracked (provider billing unavailable)".to_string(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/session_usage/session_usage_test.rs"]
mod tests;
