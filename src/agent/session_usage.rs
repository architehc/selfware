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

/// OpenRouter reference pricing per 1M tokens (USD) for input, prefix/cached
/// input, and output tokens. Used to surface equivalent OpenRouter dollar
/// value saved when running against a self-hosted or unmetered endpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenRouterReferencePricing {
    pub input_per_million: f64,
    pub cached_input_per_million: f64,
    pub output_per_million: f64,
}

impl OpenRouterReferencePricing {
    pub fn for_model(model: &str) -> Self {
        let lower = model.trim().to_ascii_lowercase();
        if lower.contains("deepseek-r1") || lower.ends_with("/r1") {
            Self {
                input_per_million: 0.55,
                cached_input_per_million: 0.14,
                output_per_million: 2.19,
            }
        } else if lower.contains("qwen") {
            Self {
                input_per_million: 0.30,
                cached_input_per_million: 0.075,
                output_per_million: 1.20,
            }
        } else if lower.contains("glm") {
            Self {
                input_per_million: 0.50,
                cached_input_per_million: 0.125,
                output_per_million: 2.00,
            }
        } else if lower.contains("opus") {
            Self {
                input_per_million: 15.00,
                cached_input_per_million: 1.50,
                output_per_million: 75.00,
            }
        } else if lower.contains("sonnet") {
            Self {
                input_per_million: 3.00,
                cached_input_per_million: 0.30,
                output_per_million: 15.00,
            }
        } else {
            // Default: DeepSeek V3/V4 (`deepseek-v4.1-flash`, `deepseek-chat`, etc.)
            Self {
                input_per_million: 0.27,
                cached_input_per_million: 0.07,
                output_per_million: 1.10,
            }
        }
    }
}

/// Breakdown of OpenRouter-equivalent dollar value saved and token speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenRouterSavings {
    pub uncached_input_tokens: usize,
    pub cached_input_tokens: usize,
    pub output_tokens: usize,
    pub uncached_input_usd: f64,
    pub cached_input_usd: f64,
    pub prefix_cache_saved_usd: f64,
    pub output_usd: f64,
    pub total_saved_usd: f64,
    pub tok_per_sec: Option<f64>,
    pub pricing: OpenRouterReferencePricing,
}

impl OpenRouterSavings {
    pub fn compute(
        model: &str,
        prompt_tokens: usize,
        cached_prompt_tokens: usize,
        completion_tokens: usize,
        actual_cost_usd: Option<f64>,
        tok_per_sec: Option<f64>,
    ) -> Self {
        let pricing = OpenRouterReferencePricing::for_model(model);
        let cached = cached_prompt_tokens.min(prompt_tokens);
        let uncached = prompt_tokens.saturating_sub(cached);
        let uncached_input_usd = (uncached as f64 / 1_000_000.0) * pricing.input_per_million;
        let cached_input_usd = (cached as f64 / 1_000_000.0) * pricing.cached_input_per_million;
        let full_uncached_input_usd =
            (prompt_tokens as f64 / 1_000_000.0) * pricing.input_per_million;
        let prefix_cache_saved_usd =
            (full_uncached_input_usd - (uncached_input_usd + cached_input_usd)).max(0.0);
        let output_usd = (completion_tokens as f64 / 1_000_000.0) * pricing.output_per_million;
        let openrouter_equiv_usd = uncached_input_usd + cached_input_usd + output_usd;
        let total_saved_usd = (openrouter_equiv_usd - actual_cost_usd.unwrap_or(0.0)).max(0.0);
        Self {
            uncached_input_tokens: uncached,
            cached_input_tokens: cached,
            output_tokens: completion_tokens,
            uncached_input_usd,
            cached_input_usd,
            prefix_cache_saved_usd,
            output_usd,
            total_saved_usd,
            tok_per_sec: tok_per_sec.filter(|v| v.is_finite() && *v >= 0.1),
            pricing,
        }
    }

    /// Detailed line for `RunSummary`, `/cost`, and `/quit`.
    pub fn summary_line(&self) -> String {
        let speed_clause = match self.tok_per_sec {
            Some(tps) => format!(" · speed: {tps:.1} tok/s"),
            None => String::new(),
        };
        format!(
            "openrouter value saved: ~${:.4} (${:.4} input + ${:.4} prefix-cache [saved ${:.4}] + ${:.4} output @ ${:.2}/${:.2}/${:.2}/1M){}",
            self.total_saved_usd,
            self.uncached_input_usd,
            self.cached_input_usd,
            self.prefix_cache_saved_usd,
            self.output_usd,
            self.pricing.input_per_million,
            self.pricing.cached_input_per_million,
            self.pricing.output_per_million,
            speed_clause,
        )
    }

    /// Compact badge for the interactive status bar.
    pub fn status_bar_badge(&self) -> String {
        let saved = if self.total_saved_usd >= 0.01 {
            format!("saved ~${:.2} OR", self.total_saved_usd)
        } else {
            format!("saved ~${:.4} OR", self.total_saved_usd)
        };
        match self.tok_per_sec {
            Some(tps) => format!("{saved} · {tps:.1} tok/s"),
            None => saved,
        }
    }
}

/// Measure prefix-cached prompt tokens and average decode speed (`tok/s`) from
/// the API client's recorded call shapes and attempts.
pub(crate) fn measure_cache_and_speed(
    shapes: &[crate::api::usage::CallShape],
    attempts: &[crate::api::usage::UsageAttempt],
) -> (usize, Option<f64>) {
    let reported_cached: usize = attempts
        .iter()
        .filter_map(|a| a.usage.as_ref())
        .filter_map(|u| u.prompt_tokens_details.as_ref())
        .filter_map(|d| d.cached_tokens)
        .sum();
    let cached_prompt_tokens = if reported_cached > 0 {
        reported_cached
    } else {
        shapes
            .windows(2)
            .map(|pair| {
                let (prev, curr) = (pair[0].prompt_tokens, pair[1].prompt_tokens);
                if prev > 0 && curr >= prev * 3 / 4 {
                    prev.min(curr) as usize
                } else {
                    0
                }
            })
            .sum()
    };
    let (total_comp, total_ms) = shapes
        .iter()
        .filter(|s| s.completion_tokens > 0 && s.elapsed_ms > 0)
        .fold((0u64, 0u64), |(c, ms), s| {
            (
                c.saturating_add(s.completion_tokens),
                ms.saturating_add(s.elapsed_ms),
            )
        });
    let tok_per_sec = if total_comp > 0 && total_ms > 0 {
        Some(total_comp as f64 / (total_ms as f64 / 1000.0))
    } else {
        None
    };
    (cached_prompt_tokens, tok_per_sec)
}

#[cfg(test)]
#[path = "../../tests/unit/agent/session_usage/session_usage_test.rs"]
mod tests;
