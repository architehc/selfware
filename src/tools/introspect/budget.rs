//! Token Budget Management for Code Introspection
//!
//! Tracks token usage as a hard limit and chooses depth levels from
//! measured renders (see `TokenBudget::suggest_depth`).

use anyhow::Result;

/// Manages token allocation for code introspection operations
#[derive(Debug, Clone)]
pub struct TokenBudget {
    total: usize,
    used: usize,
    reserved: usize,
}

/// Depth levels for code introspection
#[derive(Debug, Clone, PartialEq)]
pub enum Depth {
    /// Imports plus a one-line `Kind: name` entry per declared symbol
    Overview,
    /// One signature line per public / `pub(crate)` symbol
    Signatures,
    /// One signature line per symbol, private ones included (the parser
    /// extracts signatures, not bodies — this is not the file's source)
    Full,
    /// Imports only
    Dependencies,
}

impl Depth {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "overview" => Ok(Self::Overview),
            "signatures" => Ok(Self::Signatures),
            "full" => Ok(Self::Full),
            "dependencies" => Ok(Self::Dependencies),
            _ => anyhow::bail!("Unknown depth level: {}", s),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Signatures => "signatures",
            Self::Full => "full",
            Self::Dependencies => "dependencies",
        }
    }
}

impl std::fmt::Display for Depth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Depth {
    /// Downgrade to next lower detail level
    pub fn downgrade(&self) -> Option<Self> {
        match self {
            Self::Full => Some(Self::Signatures),
            Self::Signatures => Some(Self::Overview),
            Self::Overview => None,
            Self::Dependencies => Some(Self::Signatures),
        }
    }
}

impl TokenBudget {
    /// Create a new budget with specified total tokens
    pub fn new(total: usize) -> Self {
        Self {
            total,
            used: 0,
            reserved: 0,
        }
    }

    /// Reserve a percentage of budget for output formatting
    pub fn reserve(&mut self, percent: usize) {
        self.reserved = (self.total * percent) / 100;
    }

    /// Check if budget is exhausted
    pub fn exhausted(&self) -> bool {
        self.used + self.reserved >= self.total
    }

    /// Get remaining tokens
    pub fn remaining(&self) -> usize {
        self.total.saturating_sub(self.used + self.reserved)
    }

    /// Get used tokens
    pub fn used(&self) -> usize {
        self.used
    }

    /// Allocate `requested` tokens as a HARD limit: the grant is all or
    /// nothing. Returns `true` (and records the tokens as used) only when the
    /// whole request fits in [`remaining`](Self::remaining); otherwise nothing
    /// is recorded and `false` is returned.
    ///
    /// The old `allocate` granted `requested.min(available)` — a partial
    /// grant its caller treated as success, so a file whose rendered symbols
    /// did not fit was still rendered in full while the budget reported
    /// exactly `total` used (2026-09 introspect review, finding 1).
    pub fn try_allocate(&mut self, requested: usize) -> bool {
        if requested > self.remaining() {
            return false;
        }
        self.used += requested;
        true
    }

    /// Choose the depth to use from MEASURED packings of the whole candidate
    /// set, one per depth in preference order (most useful first).
    ///
    /// The first option that covers everything (every readable file, no
    /// symbol dropped for budget) wins. When none does, the option that
    /// covers the most files wins (ties broken by the larger fraction of its
    /// own symbols rendered, then by preference order) — the depth that
    /// reaches furthest is chosen, never one already measured not to fit.
    ///
    /// Replaces a `suggest_depth(&[FileMeta])` that ran before the files
    /// were collected (so it always saw an empty list and returned
    /// `Signatures`) and whose fallback branch recommended `Signatures` when
    /// even `Overview` could not cover the set (finding 2). Returns `None`
    /// only for an empty option list.
    pub fn suggest_depth(options: &[MeasuredDepth]) -> Option<Depth> {
        if let Some(complete) = options.iter().find(|o| o.complete) {
            return Some(complete.depth.clone());
        }
        let mut best: Option<&MeasuredDepth> = None;
        for option in options {
            let better = match best {
                None => true,
                Some(b) => {
                    option.files_included > b.files_included
                        || (option.files_included == b.files_included
                            && option.symbol_fraction() > b.symbol_fraction())
                }
            };
            if better {
                best = Some(option);
            }
        }
        best.map(|o| o.depth.clone())
    }

    /// Get budget utilization percentage
    pub fn utilization_pct(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f64 / self.total as f64) * 100.0
    }
}

/// One measured packing of the candidate set at a given depth, used by
/// [`TokenBudget::suggest_depth`].
#[derive(Debug, Clone)]
pub struct MeasuredDepth {
    pub depth: Depth,
    /// Every readable file rendered with all of its symbols at this depth.
    pub complete: bool,
    /// Files that got an entry in the rendered output.
    pub files_included: usize,
    /// Symbols this depth would render across all candidate files.
    pub symbols_total: usize,
    /// Symbols actually rendered within the budget.
    pub symbols_included: usize,
}

impl MeasuredDepth {
    fn symbol_fraction(&self) -> f64 {
        if self.symbols_total == 0 {
            return 1.0;
        }
        self.symbols_included as f64 / self.symbols_total as f64
    }
}

/// Budget for evolution planning
#[derive(Debug, Clone)]
pub struct PlanBudget {
    pub max_iterations: usize,
    pub max_tokens: usize,
    pub current_iteration: usize,
    pub token_budget: TokenBudget,
}

impl PlanBudget {
    pub fn new(max_iterations: usize, max_tokens: usize) -> Self {
        Self {
            max_iterations,
            max_tokens,
            current_iteration: 0,
            token_budget: TokenBudget::new(max_tokens),
        }
    }

    pub fn iterations_remaining(&self) -> usize {
        self.max_iterations.saturating_sub(self.current_iteration)
    }

    pub fn next_iteration(&mut self) {
        self.current_iteration += 1;
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/tools/introspect/budget/budget_test.rs"]
mod tests;
